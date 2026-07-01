#!/usr/bin/env python3
"""Measure epubveri against epubcheck's own test corpus (Cucumber features).

For each scenario we extract: the test publication name + the expected outcome
(`error X` / `warning X` / "no errors or warnings"). We resolve the publication
(zip an expanded directory on the fly), run epubveri, and score:

  * detection recall  — on should-error cases, did we flag *any* error?
  * exact-ID recall   — did we report the *same* message ID epubcheck expects?
  * false-positive %  — on should-be-clean cases, did we wrongly flag an error?
  * family breakdown  — expected error IDs by prefix, + our exact hits per family.

Honest by design: we hand-code ~20 structural checks, so overall recall across
the FULL corpus (HTM/CSS/MED/a11y/…) is expected to be low. The informative
numbers are recall *within the families we target* and the false-positive rate.

Usage: python3 scripts/corpus.py [path-to-epubveri-binary]
"""
import os
import re
import subprocess
import sys
import tempfile
import zipfile
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RES = os.path.join(ROOT, "corpus", "epubcheck", "src", "test", "resources")
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "epubveri")

# Dedicated epubcheck IDs our spike emits (reconciled 2026-06-27). RSC-005 is
# deliberately EXCLUDED here: it is epubcheck's RelaxNG/Schematron catch-all
# (~116 corpus cases), so counting it would swamp this precision metric. We DO
# emit RSC-005 for our structural conditions, so those wins still show up in the
# overall exact-ID recall — just not in this "within target" number.
TARGET_IDS = {
    "PKG-004", "PKG-006", "PKG-007",
    "RSC-001", "RSC-002", "RSC-003",
    "OPF-001", "OPF-002", "OPF-030", "OPF-033", "OPF-034", "OPF-043",
    "OPF-049", "OPF-050",
}

ID_RE = re.compile(r"\b([A-Z]{2,4}-\d{2,4})\b")
CHECK_RE = re.compile(r"checking (?:EPUB|document|the EPUB)\s+'([^']+)'")
LOCATED_RE = re.compile(r"located at\s+'([^']+)'")


def parse_features():
    scenarios = []
    for dirpath, _, files in os.walk(RES):
        for fn in files:
            if not fn.endswith(".feature"):
                continue
            path = os.path.join(dirpath, fn)
            base = None
            cur = None  # current scenario dict
            table_mode = None  # "err" / "warn" while inside a Cucumber table
            with open(path, encoding="utf-8") as f:
                lines = f.readlines()
            for raw in lines:
                line = raw.strip()
                m = LOCATED_RE.search(line)
                if m:
                    base = m.group(1)
                if line.startswith("Scenario Outline"):
                    cur = None  # skip parameterized outlines
                    table_mode = None
                    continue
                if line.startswith("Scenario"):
                    cur = {"file": path, "base": base, "name": None,
                           "errs": set(), "warns": set(), "clean": False}
                    scenarios.append(cur)
                    table_mode = None
                    continue
                if cur is None:
                    continue
                cm = CHECK_RE.search(line)
                if cm:
                    cur["name"] = cm.group(1)
                # Cucumber table form: "And the following errors/warnings are
                # reported" followed by "| ID | message |" rows — these rows
                # don't repeat the phrase "is reported", so they need separate
                # handling, or scenarios using this form get misparsed as
                # having no expected errors (and can look like false clean-
                # scenario positives once we start reporting real ones).
                tm = re.search(r"the following (errors?|warnings?) are reported", line)
                if tm:
                    table_mode = "warn" if tm.group(1).startswith("warning") else "err"
                    continue
                if line.startswith("|"):
                    ids = ID_RE.findall(line)
                    if ids:
                        cur[("warns" if table_mode == "warn" else "errs")].update(ids)
                    continue
                table_mode = None
                if "is reported" in line:
                    ids = ID_RE.findall(line)
                    if "warning" in line:
                        cur["warns"].update(ids)
                    else:  # 'error' or 'fatal error'
                        cur["errs"].update(ids)
                if re.search(r"no (other )?errors? (or|and) warnings? (are|is) reported", line):
                    cur["clean"] = True
    return [s for s in scenarios if s["name"]]


def zip_dir(src_dir):
    """Zip an expanded EPUB directory to a temp .epub (mimetype first, stored)."""
    fd, tmp = tempfile.mkstemp(suffix=".epub")
    os.close(fd)
    all_files = []
    for dp, _, fns in os.walk(src_dir):
        for fn in fns:
            full = os.path.join(dp, fn)
            rel = os.path.relpath(full, src_dir).replace(os.sep, "/")
            all_files.append((rel, full))
    all_files.sort(key=lambda x: (x[0] != "mimetype", x[0]))  # mimetype first
    with zipfile.ZipFile(tmp, "w") as z:
        for rel, full in all_files:
            comp = zipfile.ZIP_STORED if rel == "mimetype" else zipfile.ZIP_DEFLATED
            zi = zipfile.ZipInfo(rel)
            zi.compress_type = comp
            with open(full, "rb") as fh:
                z.writestr(zi, fh.read())
    return tmp


EXT_MEDIA_TYPE = {
    ".xhtml": "application/xhtml+xml", ".html": "application/xhtml+xml",
    ".htm": "application/xhtml+xml", ".css": "text/css",
    ".svg": "image/svg+xml", ".png": "image/png", ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp",
    ".otf": "application/font-sfnt", ".ttf": "application/font-sfnt",
    ".woff": "application/font-woff", ".woff2": "font/woff2",
    ".js": "text/javascript", ".ncx": "application/x-dtbncx+xml",
    ".mp3": "audio/mpeg", ".mp4": "video/mp4", ".m4a": "audio/mp4",
    ".pdf": "application/pdf", ".xml": "application/xml",
    ".opf": "application/oebps-package+xml",
}


def guess_media_type(name):
    _, ext = os.path.splitext(name)
    return EXT_MEDIA_TYPE.get(ext.lower(), "application/octet-stream")


def wrap_single_doc(target_full, target_name):
    """epubcheck can check a single content document in isolation; epubveri
    only validates full books. So for a bare content-document fixture, build a
    minimal synthetic EPUB that includes it (plus all of its directory
    siblings, so any relative reference it makes still resolves — avoiding
    spurious missing-resource errors that would be an artifact of this
    harness, not of epubveri) via a synthetic nav doc satisfying the EPUB 3
    nav requirement, and the fixture itself as an ordinary (non-nav, non-
    spine) manifest item, so only the content-model checks are exercised."""
    src_dir = os.path.dirname(target_full)
    siblings = sorted(
        fn for fn in os.listdir(src_dir) if os.path.isfile(os.path.join(src_dir, fn))
    )
    container_xml = (
        '<?xml version="1.0"?>\n'
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n'
        '  <rootfiles><rootfile full-path="OEBPS/content.opf" '
        'media-type="application/oebps-package+xml"/></rootfiles>\n'
        '</container>\n'
    )
    nav_xhtml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">\n'
        '<head><title>Nav</title></head>\n'
        f'<body><nav epub:type="toc"><ol><li><a href="{target_name}">t</a></li></ol></nav></body>\n'
        '</html>\n'
    )
    manifest_items = [
        '<item id="_nav" href="_nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
    ]
    # Siblings are included so the *target*'s relative references (css,
    # images, fonts, ...) resolve. Other bare xhtml/html siblings are separate,
    # independent test fixtures in their own right — including them as real
    # content documents here would make every single-doc wrap exercise the
    # content-model check against ALL of them at once, not just the one under
    # test, so they're demoted to an inert media type (the target itself keeps
    # its real one).
    for i, fn in enumerate(siblings):
        mt = guess_media_type(fn)
        if fn != target_name and mt == "application/xhtml+xml":
            mt = "application/octet-stream"
        manifest_items.append(f'<item id="f{i}" href="{fn}" media-type="{mt}"/>')
    opf = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">\n'
        '  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">\n'
        '    <dc:identifier id="id">urn:uuid:corpus-wrap</dc:identifier>\n'
        '    <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n'
        '  </metadata>\n'
        '  <manifest>\n    ' + '\n    '.join(manifest_items) + '\n  </manifest>\n'
        '  <spine><itemref idref="_nav"/></spine>\n'
        '</package>\n'
    )
    fd, tmp = tempfile.mkstemp(suffix=".epub")
    os.close(fd)
    with zipfile.ZipFile(tmp, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, "application/epub+zip")
        z.writestr("META-INF/container.xml", container_xml)
        z.writestr("OEBPS/content.opf", opf)
        z.writestr("OEBPS/_nav.xhtml", nav_xhtml)
        for fn in siblings:
            with open(os.path.join(src_dir, fn), "rb") as fh:
                z.writestr(f"OEBPS/{fn}", fh.read())
    return tmp


def resolve(s):
    """Return (epub_path, is_temp, skip_reason, single_doc_wrap)."""
    name = s["name"]
    if "<" in name:
        return None, False, "outline-param", False
    if name.endswith(".opf"):
        return None, False, "opf-only (no container; out of scope)", False
    base = (s["base"] or "").lstrip("/")
    full = os.path.join(RES, base, name)
    if name.endswith(".epub"):
        if os.path.isfile(full):
            return full, False, None, False
        return None, False, "missing-file", False
    if os.path.isdir(full):
        return zip_dir(full), True, None, False
    if os.path.isfile(full + ".epub"):
        return full + ".epub", False, None, False
    if os.path.isfile(full) and name.endswith((".xhtml", ".html", ".htm")):
        return wrap_single_doc(full, name), True, None, True
    return None, False, "missing-file", False


def run(path):
    p = subprocess.run([BIN, "--format", "ids", path],
                       capture_output=True, text=True)
    ids = [ln.strip() for ln in p.stdout.splitlines() if ln.strip()]
    return ids, p.returncode


def family(idstr):
    return idstr.split("-")[0]


def main():
    if not os.path.isdir(RES):
        print(f"corpus not found at {RES}", file=sys.stderr)
        return 1
    scenarios = parse_features()

    skipped = Counter()
    n_clean = n_clean_pass = n_clean_fp = 0
    n_err = n_detect = n_exact = 0
    n_inscope = n_inscope_exact = 0
    exp_family = Counter()           # expected error IDs by family (error cases)
    hit_family = Counter()           # exact hits by family
    fp_examples, miss_examples = [], []

    for s in scenarios:
        path, is_temp, reason, single_doc_wrap = resolve(s)
        if path is None:
            skipped[reason] += 1
            continue
        try:
            ids, rc = run(path)
        finally:
            if is_temp:
                os.unlink(path)
        reported = set(ids)
        if single_doc_wrap:
            # epubcheck's single-document check mode never resolves
            # cross-file references (there's no "book" to check them
            # against); our synthetic wrap only has the target's own
            # directory siblings, so an RSC-001 here is a wrapping-harness
            # artifact (a dangling reference the original fixture was never
            # meant to have resolved), not a real epubveri defect. Drop it
            # from scoring for these scenarios specifically.
            reported.discard("RSC-001")
            rc = 1 if reported else 0

        if s["errs"]:
            n_err += 1
            for e in s["errs"]:
                exp_family[family(e)] += 1
            if rc == 1:
                n_detect += 1
            hit = s["errs"] & reported
            if hit:
                n_exact += 1
                for e in hit:
                    hit_family[family(e)] += 1
            if s["errs"] & TARGET_IDS:
                n_inscope += 1
                if hit:
                    n_inscope_exact += 1
                elif len(miss_examples) < 12:
                    miss_examples.append((s["name"], sorted(s["errs"]), ids or ["(none)"]))
        elif s["clean"]:
            n_clean += 1
            if rc == 0:
                n_clean_pass += 1
            else:
                n_clean_fp += 1
                if len(fp_examples) < 12:
                    fp_examples.append((s["name"], ids))

    def pct(a, b):
        return f"{(100.0*a/b):.1f}%" if b else "n/a"

    print(f"\n=== epubveri vs epubcheck corpus ===")
    print(f"scenarios parsed (with a publication): {len(scenarios)}")
    print(f"skipped: {sum(skipped.values())}  " +
          "  ".join(f"{k}={v}" for k, v in skipped.most_common()))

    print(f"\n-- should-ERROR cases: {n_err} --")
    print(f"  detection recall (flagged any error): {n_detect}/{n_err} = {pct(n_detect, n_err)}")
    print(f"  exact-ID recall  (same message ID)  : {n_exact}/{n_err} = {pct(n_exact, n_err)}")
    print(f"  within our TARGET ids ({len(TARGET_IDS)} ids): "
          f"{n_inscope_exact}/{n_inscope} = {pct(n_inscope_exact, n_inscope)} exact")

    print(f"\n-- should-be-CLEAN cases: {n_clean} --")
    print(f"  passed (we stayed silent): {n_clean_pass}/{n_clean} = {pct(n_clean_pass, n_clean)}")
    print(f"  FALSE POSITIVES (we errored): {n_clean_fp}/{n_clean} = {pct(n_clean_fp, n_clean)}")

    print(f"\n-- expected-error families (top) : exact hits / total --")
    for fam, tot in exp_family.most_common(14):
        print(f"  {fam:<5} {hit_family[fam]:>4} / {tot}")

    if miss_examples:
        print(f"\n-- in-scope MISSES (target id expected, we missed exact) --")
        for name, exp, got in miss_examples:
            print(f"  {name}\n      expected {exp}  got {got}")
    if fp_examples:
        print(f"\n-- FALSE-POSITIVE examples (clean file, we errored) --")
        for name, got in fp_examples:
            print(f"  {name}  ->  {got}")
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
