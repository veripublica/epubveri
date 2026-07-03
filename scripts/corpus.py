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

# A trailing lowercase letter (e.g. "HTM-060a"/"HTM-060b") is a
# Gherkin-authoring convention to label sub-cases of the same real
# epubcheck code, not part of the reported message id - matched but not
# captured, so "HTM-060a" scores as "HTM-060".
ID_RE = re.compile(r"\b([A-Z]{2,4}-\d{2,4})[a-z]?\b")
CHECK_RE = re.compile(r"checking (?:EPUB|document|file|the EPUB)\s+'([^']+)'")
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
                # A Gherkin comment line (e.g. a disabled assertion like
                # "#Then error RSC-005 is reported", left in on purpose by
                # epubcheck itself with a FIXME above it) must not be read
                # as a real assertion - without this, a commented-out
                # expectation silently corrupts scoring (a fixture we're
                # already correctly silent on looks like a miss).
                if line.startswith("#"):
                    continue
                m = LOCATED_RE.search(line)
                if m:
                    base = m.group(1)
                if line.startswith("Scenario Outline"):
                    cur = None  # skip parameterized outlines
                    table_mode = None
                    continue
                # "Example:" is a real Gherkin synonym for "Scenario:" (used
                # e.g. in vocabularies.feature) - matching only the exact
                # singular keyword, since "Examples:" (plural) is the
                # unrelated Scenario Outline parameter-table keyword. Two
                # feature files use it; missing it let assertions bleed
                # across scenario boundaries (cur never reset), silently
                # corrupting scoring for every scenario after the first.
                if line.startswith("Scenario") or line.startswith("Example:"):
                    cur = {"file": path, "base": base, "name": None,
                           "errs": set(), "warns": set(), "clean": False,
                           "as_nav": False}
                    scenarios.append(cur)
                    table_mode = None
                    continue
                if cur is None:
                    continue
                if "EPUBCheck configured to check a navigation document" in line:
                    cur["as_nav"] = True
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
                # "X is reported 0 times" is a negative assertion (the ID
                # must NOT appear) - the opposite of every other "is
                # reported" phrasing here. Only 2 scenarios in the whole
                # corpus use it; without this check, both were misread as
                # *expecting* the named ID, backwards from their real
                # (and, since they're paired with "no other errors/
                # warnings", fully clean) intent.
                if "is reported" in line and "reported 0 times" not in line:
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


def wrap_nav_doc(target_full, target_name, version="3.0"):
    """epubcheck has a dedicated "check this as a navigation document"
    single-file mode ("Given EPUBCheck configured to check a navigation
    document"), used throughout navigation-document.feature. Unlike
    `wrap_single_doc` (which wraps the target as an ordinary content
    document behind a *separate* synthetic nav), this wraps the target
    itself as the book's real nav doc (`properties="nav"`), alongside one
    dummy ordinary content document so the spine isn't empty. Directory
    siblings are still included (demoted to inert media types, mirroring
    `wrap_single_doc`) so the target's own relative references resolve."""
    with open(target_full, "rb") as f:
        nav_content = f.read()
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
    content_xhtml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml">\n'
        '<head><title>t</title></head><body><p>t</p></body>\n'
        '</html>\n'
    )
    manifest_items = [
        f'<item id="_navtarget" href="{target_name}" media-type="application/xhtml+xml" properties="nav"/>',
        '<item id="_content" href="_content.xhtml" media-type="application/xhtml+xml"/>',
    ]
    for i, fn in enumerate(siblings):
        if fn == target_name:
            continue
        mt = guess_media_type(fn)
        if mt in ("application/xhtml+xml", "image/svg+xml"):
            mt = "application/octet-stream"
        manifest_items.append(f'<item id="f{i}" href="{fn}" media-type="{mt}"/>')
    opf = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f'<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">\n'
        '  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">\n'
        '    <dc:identifier id="id">corpus-wrap</dc:identifier>\n'
        '    <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n'
        '    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>\n'
        '  </metadata>\n'
        '  <manifest>\n    ' + '\n    '.join(manifest_items) + '\n  </manifest>\n'
        '  <spine><itemref idref="_content"/></spine>\n'
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
        z.writestr("OEBPS/_content.xhtml", content_xhtml)
        z.writestr(f"OEBPS/{target_name}", nav_content)
        for fn in siblings:
            if fn == target_name:
                continue
            with open(os.path.join(src_dir, fn), "rb") as fh:
                z.writestr(f"OEBPS/{fn}", fh.read())
    return tmp


def wrap_single_doc(target_full, target_name, version="3.0"):
    """epubcheck can check a single content document in isolation; epubveri
    only validates full books. So for a bare content-document fixture, build a
    minimal synthetic EPUB that includes it (plus all of its directory
    siblings, so any relative reference it makes still resolves — avoiding
    spurious missing-resource errors that would be an artifact of this
    harness, not of epubveri) via a synthetic nav doc satisfying the EPUB 3
    nav requirement, and the fixture itself as an ordinary (non-nav, non-
    spine) manifest item, so only the content-model checks are exercised.
    `version` defaults to 3.0 but is set to 2.0 for scenarios that
    originate from an `epub2/` feature file — real corpus fixtures found
    this matters: several checks (e.g. the XHTML content-model's obsolete-
    DOCTYPE rule, HTM-004) are EPUB3-only, and an EPUB2-context fixture
    legitimately uses constructs (like the XHTML 1.1 DTD doctype) that
    would otherwise wrongly get EPUB3 rules applied via a version="3.0"
    wrap."""
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
    # EPUB 2 has no `<nav>`/nav-document concept at all - `<nav>` is
    # itself an HTML5-only element, so using it here would make an EPUB
    # 2-context wrap fail EPUB 2's own new "no HTML5-only elements" DOM
    # check (confirmed the hard way: it broke `minimal.xhtml` and every
    # other epub2 content-model fixture once that check existed). The
    # EPUB 2 wrap uses a plain hyperlink to the target instead, which
    # still gives the harness the same "a reason to include this
    # resource, but keep it out of the spine to isolate the content-
    # model check" property the EPUB 3 nav-based version has.
    if version.startswith("2"):
        nav_xhtml = (
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<html xmlns="http://www.w3.org/1999/xhtml">\n'
            '<head><title>t</title></head>\n'
            f'<body><p><a href="{target_name}">t</a></p></body>\n'
            '</html>\n'
        )
        manifest_items = [
            '<item id="_nav" href="_nav.xhtml" media-type="application/xhtml+xml"/>'
        ]
    else:
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
    # images, fonts, ...) resolve. Other bare xhtml/html/svg siblings are
    # separate, independent test fixtures in their own right — including
    # them as real content documents here would make every single-doc wrap
    # exercise the content-model check against ALL of them at once, not
    # just the one under test, so they're demoted to an inert media type
    # (the target itself keeps its real one). svg was added to this
    # demotion alongside xhtml/html once SVG got its own content-model
    # checks (foreignObject/title/vocabulary) - the shared `files/`
    # directories hold dozens of independent `.svg` fixtures.
    for i, fn in enumerate(siblings):
        mt = guess_media_type(fn)
        if fn != target_name and mt in ("application/xhtml+xml", "image/svg+xml"):
            mt = "application/octet-stream"
        manifest_items.append(f'<item id="f{i}" href="{fn}" media-type="{mt}"/>')
    # EPUB 2 requires a spine 'toc' (NCX) attribute - without one, an
    # otherwise-clean epub2-context wrap would spuriously fail with "EPUB 2
    # spine is missing the required toc attribute", a harness artifact
    # from the wrap being minimal, not a real defect in the fixture.
    toc_attr = ""
    if version.startswith("2"):
        manifest_items.append('<item id="ncx" href="_toc.ncx" media-type="application/x-dtbncx+xml"/>')
        toc_attr = ' toc="ncx"'
    opf = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f'<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">\n'
        '  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">\n'
        '    <dc:identifier id="id">corpus-wrap</dc:identifier>\n'
        '    <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n'
        '    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>\n'
        '  </metadata>\n'
        '  <manifest>\n    ' + '\n    '.join(manifest_items) + '\n  </manifest>\n'
        f'  <spine{toc_attr}><itemref idref="_nav"/></spine>\n'
        '</package>\n'
    )
    toc_ncx = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">\n'
        '  <head><meta name="dtb:uid" content="corpus-wrap"/></head>\n'
        '  <docTitle><text>Corpus wrap</text></docTitle>\n'
        '  <navMap><navPoint id="np1" playOrder="1">'
        f'<navLabel><text>t</text></navLabel><content src="{target_name}"/>'
        '</navPoint></navMap>\n'
        '</ncx>\n'
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
        if version.startswith("2"):
            z.writestr("OEBPS/_toc.ncx", toc_ncx)
        for fn in siblings:
            with open(os.path.join(src_dir, fn), "rb") as fh:
                z.writestr(f"OEBPS/{fn}", fh.read())
    return tmp


def wrap_opf_file(full, name):
    """Wrap a bare .opf fixture (epubcheck's single-file package-document
    check mode) in a minimal synthetic book: mimetype + container.xml
    pointing straight at it. Package/Schematron-level checks (id
    uniqueness, unique-identifier resolution, dcterms:modified, ...) only
    need the OPF itself; manifest items it references won't exist in this
    minimal wrap (same harness limitation as wrap_single_doc), so RSC-001
    from these is excluded from scoring the same way. Read as raw bytes,
    not decoded text - some fixtures are deliberately non-UTF-8 (encoding
    tests), and the bytes get written straight into the zip either way."""
    with open(full, "rb") as f:
        opf_content = f.read()
    container_xml = (
        '<?xml version="1.0"?>\n'
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n'
        f'  <rootfiles><rootfile full-path="{name}" '
        'media-type="application/oebps-package+xml"/></rootfiles>\n'
        '</container>\n'
    )
    fd, tmp = tempfile.mkstemp(suffix=".epub")
    os.close(fd)
    with zipfile.ZipFile(tmp, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, "application/epub+zip")
        z.writestr("META-INF/container.xml", container_xml)
        z.writestr(name, opf_content)
    return tmp


def wrap_smil_file(full, name):
    """Wrap a bare .smil fixture (epubcheck's single-document media-overlay
    check mode) in a minimal synthetic book. Scans the SMIL's own <text src>/
    <audio src> attributes to generate matching stub resources (a content
    document with an anchor for every referenced fragment id, and an empty
    audio file) so those references resolve — avoiding a harness-artifact
    RSC-001 the same way wrap_single_doc/wrap_opf_file do (also excluded
    from scoring below via single_doc_wrap, belt-and-suspenders)."""
    with open(full, encoding="utf-8") as f:
        smil_content = f.read()
    srcs = re.findall(r'src="([^"]*)"', smil_content)
    xhtml_names, audio_names, anchors = set(), set(), set()
    for src in srcs:
        path, _, frag = src.partition("#")
        if path.endswith((".xhtml", ".html", ".htm")):
            xhtml_names.add(path)
            if frag:
                anchors.add(frag)
        else:
            audio_names.add(path)
    if not xhtml_names:
        xhtml_names.add("chapter1.xhtml")
    body = "".join(f'<p id="{a}">t</p>' for a in sorted(anchors)) or "<p>t</p>"
    content_xhtml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml">\n'
        f'<head><title>t</title></head>\n<body>{body}</body>\n</html>\n'
    )
    container_xml = (
        '<?xml version="1.0"?>\n'
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n'
        '  <rootfiles><rootfile full-path="OEBPS/content.opf" '
        'media-type="application/oebps-package+xml"/></rootfiles>\n'
        '</container>\n'
    )
    first_xhtml = sorted(xhtml_names)[0]
    nav_xhtml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">\n'
        '<head><title>Nav</title></head>\n'
        f'<body><nav epub:type="toc"><ol><li><a href="{first_xhtml}">t</a></li></ol></nav></body>\n'
        '</html>\n'
    )
    manifest_items = [
        '<item id="_nav" href="_nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
        f'<item id="mo" href="{name}" media-type="application/smil+xml"/>',
    ]
    spine_items = ['<itemref idref="_nav"/>']
    for i, fn in enumerate(sorted(xhtml_names)):
        manifest_items.append(f'<item id="c{i}" href="{fn}" media-type="application/xhtml+xml" media-overlay="mo"/>')
        spine_items.append(f'<itemref idref="c{i}"/>')
    for i, fn in enumerate(sorted(audio_names)):
        manifest_items.append(f'<item id="a{i}" href="{fn}" media-type="audio/mpeg"/>')
    opf = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">\n'
        '  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">\n'
        '    <dc:identifier id="id">corpus-wrap</dc:identifier>\n'
        '    <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n'
        '    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>\n'
        # epubveri checks that a publication using media-overlay attributes
        # declares both a global and a per-overlay media:duration meta
        # (RSC-005 if either is missing) - without these the wrap itself
        # would spuriously fail that check, a harness artifact rather than
        # a defect in the fixture under test (same reasoning as the
        # synthetic NCX wrap_single_doc adds for EPUB 2 scenarios).
        '    <meta property="media:duration">1s</meta>\n'
        '    <meta property="media:duration" refines="#mo">1s</meta>\n'
        '  </metadata>\n'
        '  <manifest>\n    ' + '\n    '.join(manifest_items) + '\n  </manifest>\n'
        '  <spine>' + "".join(spine_items) + '</spine>\n'
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
        for fn in xhtml_names:
            z.writestr(f"OEBPS/{fn}", content_xhtml)
        for fn in audio_names:
            z.writestr(f"OEBPS/{fn}", b"\x00" * 16)
        z.writestr(f"OEBPS/{name}", smil_content)
    return tmp


def resolve(s):
    """Return (epub_path, is_temp, skip_reason, single_doc_wrap)."""
    name = s["name"]
    if "<" in name:
        return None, False, "outline-param", False
    base = (s["base"] or "").lstrip("/")
    full = os.path.join(RES, base, name)
    if name.endswith(".opf"):
        if os.path.isfile(full):
            return wrap_opf_file(full, name), True, None, True
        return None, False, "opf-only (missing file)", False
    if name.endswith(".epub"):
        if os.path.isfile(full):
            return full, False, None, False
        return None, False, "missing-file", False
    if os.path.isdir(full):
        return zip_dir(full), True, None, False
    if os.path.isfile(full + ".epub"):
        return full + ".epub", False, None, False
    if os.path.isfile(full) and name.endswith((".xhtml", ".html", ".htm")):
        version = "2.0" if "/epub2/" in (s["file"] or "") else "3.0"
        if s.get("as_nav"):
            return wrap_nav_doc(full, name, version), True, None, True
        return wrap_single_doc(full, name, version), True, None, True
    if os.path.isfile(full) and name.endswith(".smil"):
        return wrap_smil_file(full, name), True, None, True
    return None, False, "missing-file", False


SEVERITY_LINE_RE = re.compile(r"^(ERROR|WARNING|INFO)\s+([A-Z]{2,4}-\d{2,4})\b")


def run(path):
    # `--format human` (not `ids`) so severity is available: the
    # single_doc_wrap rc-recompute below needs to ignore Info-severity
    # findings (e.g. OPF-090), which real epubcheck's default reporting
    # level wouldn't show either - without this, a wrapped "should stay
    # clean" fixture that happens to trigger an Info-level usage message
    # looks like a false positive purely because the wrap's own rc
    # recomputation (unlike the CLI's real exit code) doesn't check
    # severity at all.
    p = subprocess.run([BIN, "--format", "human", path],
                       capture_output=True, text=True)
    ids, error_ids = [], []
    for ln in p.stdout.splitlines():
        m = SEVERITY_LINE_RE.match(ln.strip())
        if not m:
            continue
        sev, id_ = m.groups()
        ids.append(id_)
        if sev == "ERROR":
            error_ids.append(id_)
    return ids, error_ids, p.returncode


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
            ids, error_ids, rc = run(path)
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
            reported.discard("RSC-007")
            # wrap_single_doc's synthetic nav hyperlinks to the wrapped
            # target (so the harness has a reason to include it at all),
            # but deliberately keeps it out of the synthetic spine to
            # isolate the content-model check - RSC-011 ("hyperlinked but
            # not in spine") is a real, correct finding on that synthetic
            # wrapping, not a defect in the fixture under test.
            reported.discard("RSC-011")
            # Likewise, RSC-008 ("remote resource not declared in the
            # manifest") is a real check, but wrap_single_doc's synthetic
            # manifest only accounts for local directory siblings, never
            # the target's own remote URL references - so it can't
            # reasonably declare every remote resource the fixture
            # happens to reference either.
            reported.discard("RSC-008")
            # OPF-014 (remote-resources/scripted/svg used but undeclared)
            # is also unreachable here: wrap_single_doc's own manifest
            # item for the target never sets any `properties` at all, so
            # a target that happens to use SVG/script/remote resources
            # would always look "undeclared" - a wrapping-harness gap, not
            # a defect the fixture is meant to test (none of epubveri's
            # real OPF-014 scenarios are single-doc-wrapped fixtures).
            reported.discard("OPF-014")
            # RSC-012 (a hyperlink's fragment doesn't resolve to a real id)
            # is a real check, but wrap_single_doc demotes every *other*
            # directory-sibling xhtml/html file to an inert media type
            # (so a single-doc wrap doesn't also exercise their own,
            # independent content-model tests) - meaning a cross-doc
            # fragment the fixture legitimately references may live in a
            # sibling that this harness never actually parses as a real
            # content document, a wrapping-harness gap rather than a
            # defect in the fixture under test.
            reported.discard("RSC-012")
            rc = 1 if (reported & set(error_ids)) else 0

        # A scenario can expect only a *warning* (no "errs"), e.g. MED-016
        # or CSS-003/019 — these were previously falling through to the
        # "should stay clean" bucket below (since that branch only checked
        # s["errs"]), which silently mis-scored them as false positives the
        # moment the corresponding check started actually firing. Score
        # errs+warns together here; only genuinely expectation-free
        # scenarios fall to the clean bucket. `rc` (the CLI's exit code) is
        # error-only by design (`Report::is_valid`), so the "detection
        # recall (flagged any error)" sub-metric still only means something
        # for scenarios that expect an actual error - it's simply not
        # incremented for warning-only ones, while exact-ID recall (the
        # more important number) still counts them correctly either way.
        expected = s["errs"] | s["warns"]
        if expected:
            n_err += 1
            for e in expected:
                exp_family[family(e)] += 1
            if s["errs"] and rc == 1:
                n_detect += 1
            hit = expected & reported
            if hit:
                n_exact += 1
                for e in hit:
                    hit_family[family(e)] += 1
            if expected & TARGET_IDS:
                n_inscope += 1
                if hit:
                    n_inscope_exact += 1
                elif len(miss_examples) < 12:
                    miss_examples.append((s["name"], sorted(expected), ids or ["(none)"]))
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
