#!/usr/bin/env python3
"""Generate docs/COVERAGE.md: a per-ID coverage matrix of epubveri vs
epubcheck. Derives the ID universe + text + severity from epubcheck's own
sources, the "which IDs epubveri has" from src/ids.rs, and the
full/partial/none status + notes from a version-controlled annotations
table (docs/coverage-notes.toml, inlined here for the first draft).

Rule: an epubcheck ID epubveri has no constant for -> not implemented (x).
An ID epubveri has -> implemented, defaulting to full (Y) unless the
annotations mark it partial (~) or the annotations override it. epubveri's
own IDs (ADV-*) are marked epubcheck: -."""

import re, sys, pathlib

REPO = pathlib.Path(__file__).resolve().parent.parent
EC = REPO / "corpus/epubcheck/src/main/resources/com/adobe/epubcheck/messages"
ECJ = REPO / "corpus/epubcheck/src/main/java/com/adobe/epubcheck/messages"

# --- 1. epubcheck ID universe (MessageId.java) ---
# Match the enum NAME (always XXX_NNN), not the string literal - epubcheck's
# literals are inconsistent (some use "HTM_054" with an underscore instead
# of "HTM-054"). Normalize to hyphens.
mid = (ECJ / "MessageId.java").read_text()
ec_ids = [f"{m.group(1)}-{m.group(2)}"
          for m in re.finditer(r'^\s*([A-Z]+)_([0-9]+)\(', mid, re.M)]
ec_ids = sorted(set(ec_ids), key=lambda x: (x.split('-')[0], int(x.split('-')[1])))

# --- 2. epubcheck message text (MessageBundle.properties) ---
text = {}
for line in (EC / "MessageBundle.properties").read_text(errors="replace").splitlines():
    m = re.match(r'^([A-Z]+)_([0-9]+)=(.*)$', line)
    if m:
        text[f"{m.group(1)}-{m.group(2)}"] = m.group(3).strip().strip('"')

# --- 3. epubcheck severity (DefaultSeverities.java) ---
sev = {}
for m in re.finditer(r'MessageId\.([A-Z]+)_([0-9]+),\s*Severity\.([A-Z]+)',
                     (ECJ.parent / "messages/DefaultSeverities.java").read_text()):
    sev[f"{m.group(1)}-{m.group(2)}"] = m.group(3).capitalize()

# --- 4. epubveri IDs + inline comments (ids.rs) ---
# The trailing `// comment` is OPTIONAL - some IDs (e.g. RSC-005) have none,
# and requiring it would wrongly drop them from epubveri's coverage.
ev = {}
for m in re.finditer(r'pub const [A-Z0-9_]+: &str = "([A-Z]+-[0-9]+)";(?:\s*//\s*(.*))?',
                     (REPO / "src/ids.rs").read_text()):
    ev[m.group(1)] = (m.group(2) or "").strip()

# --- 5. annotations: status override + note (the human-judgment layer) ---
# status: "partial" (~), or a note-only string. Absent -> full if epubveri
# has it, none if not. Keyed by ID.
# ID -> (status, note). status "partial" flips a Y to ~; None keeps the
# default (full if epubveri has it, none if not) but supplies a custom note
# (used to say "detected under another ID" for gaps we actually cover).
ANN = {
    # --- PKG (reviewed) ---
    "PKG-003": ("partial",
        "Emitted only for a literally empty (0-byte) file; a corrupted-but-"
        "nonempty header goes to PKG-004/PKG-008 instead."),
    "PKG-020": (None, "Not emitted, but the condition IS detected: a missing "
        "declared OPF is reported as OPF-002 (Fatal)."),
    # --- OPF (reviewed) ---
    "OPF-052": ("partial",
        "Approximated as \"3 lowercase ASCII letters\" (shape), not the real "
        "MARC relator list - a fake code like `xyz` passes us but epubcheck "
        "flags it."),
    "OPF-044": (None, "Not emitted, but the condition IS detected: a spine "
        "item whose fallback chain never reaches a content document is "
        "reported as OPF-043. Splitting the two IDs is tracked in #41."),
    "OPF-010": (None, "Not emitted; reference resolution is covered under "
        "RSC-007/RSC-012."),
    "OPF-016": (None, "Not emitted; a rootfile missing `full-path` is caught "
        "via the container.xml RNG grammar (RSC-005)."),
    "OPF-017": (None, "Not emitted; a rootfile with an empty `full-path` is "
        "caught via the container.xml RNG grammar (RSC-005)."),
    "OPF-047": (None, "Legacy OEBPS 1.2 backwards-compat syntax - deliberately "
        "out of scope (pre-EPUB format)."),
    "OPF-064": (None, "Informational profile-selection message - not emitted."),
    "OPF-036": (None, "Video-codec-support usage note - not implemented."),
    "OPF-005": (None, "Prefix-URI-doesn't-exist - not done (we do prefix "
        "syntax OPF-004 + undeclared prefix OPF-028)."),
    "OPF-006": (None, "Prefix-URI-not-a-valid-URI - not done (same family)."),
    "OPF-011": (None, "itemref can't be both page-spread-left & -right - small "
        "discrete gap."),
    "OPF-021": (None, "Non-registered URI scheme in an OPF href - we have "
        "HTM-025 for content docs, but not OPF-021 for OPF hrefs."),
    "OPF-067": (None, "Resource listed as both a `<link>` and a manifest item "
        "- small discrete gap."),
    # --- RSC (partials confirmed earlier) ---
    "RSC-005": ("partial",
        "XHTML content model is real (EPUB 2 XHTML 1.1 grammar + EPUB 3 HTML5 "
        "grammar + Schematron nesting/IDREF rules + closed per-element "
        "attribute allowlists). SVG/MathML are accepted as opaque foreign "
        "subtrees, not schema-validated. Attribute *values* are permissive "
        "(e.g. `role` accepts any token, `aria-*` values aren't range-checked)."),
    "RSC-020": ("partial",
        "Host syntax + scheme are checked (space/comma in host, missing `//`). "
        "A path/query space is treated as valid (matches epubcheck; WHATWG "
        "normalizes it). Not a full WHATWG URL parse."),
    "RSC-006": (None, "Remote stylesheet references (also SVG stylesheet forms)."),
    "RSC-030": (None, "Any reference starting with `file:` (CSS, XHTML, SVG forms)."),
    "RSC-022": ("na", "Not a validation check - epubcheck reporting its own "
        "Java-runtime limitation. N/A for epubveri (we check image details "
        "via PKG-021/022)."),
    "RSC-024": (None, "Generic passthrough of raw XML-parser warnings (usage); "
        "we surface real parse errors under RSC-016. Minor gap."),
    # --- HTM (reviewed) ---
    "HTM-002": ("na", "Dead ID - epubcheck defines a severity for it but never "
        "emits it anywhere in its source. Not a live check."),
    "HTM-005": ("na", "Dead ID - epubcheck never emits it anywhere in its "
        "source. Not a live check."),
    "HTM-011": ("na", "Undeclared entity. epubcheck's own code comment says this "
        "\"may never be reported\" - an undeclared entity is a SAX parse error "
        "reported as RSC-005. epubveri catches the same defect as RSC-016 "
        "(fatal). The defect is covered; the ID itself is effectively dead."),
    "HTM-044": ("na", "Dead ID - epubcheck never emits it anywhere in its "
        "source. Not a live check."),
    "HTM-045": (None, "Empty `href=\"\"` self-reference hint (USAGE). Small "
        "discrete gap - epubveri doesn't emit it."),
    # --- CSS (reviewed) ---
    "CSS-001": (None, "epubcheck flags exactly `direction`/`unicode-bidi` "
        "(EPUB 3 only) - we match it."),
    "CSS-006": (None, "`position: fixed` (USAGE) - matches epubcheck's "
        "first-value-component == \"fixed\" test."),
    "CSS-008": ("partial",
        "Covers bad-string/bad-url tokens and unterminated rules/blocks (via "
        "styloria 0.4's `syntax_errors`), plus in-block malformed declaration "
        "shapes. Still a subset of epubcheck's full CSS-parser error surface - "
        "styloria's parser is error-recovering, so it accepts some constructs "
        "epubcheck rejects (a bad selector, an invalid at-rule prelude)."),
}

# Families whose per-ID full/partial/notes have been reviewed by hand. The
# rest are first-pass: "full" there means "epubveri has the ID", not yet
# checked for partialness.
REVIEWED = {"PKG", "OPF", "RSC", "HTM", "CSS"}
# whole families / notable gaps described once (applied to every id in the
# family that epubveri lacks, as a shared note)
FAMILY_GAP = {
    "SCP": "Scripting checks - not implemented (no SCP family).",
    "ACC": "Accessibility checks - mostly not implemented (only ACC IDs epubveri "
           "has a constant for are covered).",
    "CHK": "Internal checker/tooling messages - not applicable/implemented.",
}

# --- build rows ---
FAM_ORDER = ["PKG", "OCF", "OPF", "RSC", "HTM", "CSS", "MED", "NAV", "NCX",
             "ACC", "SCP", "CHK", "INF"]
def fam_key(f): return (FAM_ORDER.index(f) if f in FAM_ORDER else 99, f)

rows = {}  # family -> list of (id, desc, ec_mark, ev_mark, note)
counts = {}  # family -> [full, partial, none, total]
for iid in ec_ids:
    fam = iid.split('-')[0]
    desc = text.get(iid, "")
    # shorten desc
    desc = re.sub(r'\s+', ' ', desc)
    if len(desc) > 90:
        desc = desc[:87] + "..."
    if not desc:
        desc = "_(no message text in epubcheck's bundle)_"
    have = iid in ev
    ann = ANN.get(iid)
    suppressed = sev.get(iid) == "Suppressed"
    if ann and ann[0] == "na":
        # Not a real validation check for epubveri (e.g. an epubcheck
        # runtime-limitation message) - excluded from the live denominator.
        ev_mark = "⊘"
        note = ann[1]
        st = "supp"
        ec_mark = "Y"
        rows.setdefault(fam, []).append((iid, desc, ec_mark, ev_mark, note))
        c = counts.setdefault(fam, [0, 0, 0, 0, 0])
        c[3] += 1
        c[4] += 1
        continue
    if suppressed and not have:
        # epubcheck disabled this ID by default -> not a real check, N/A.
        ev_mark = "⊘"
        note = "epubcheck-suppressed (disabled by default) — not a gap"
        st = "supp"
    elif suppressed and have:
        ev_mark = "Y+"
        note = "epubveri reports this; epubcheck suppresses it (we are stricter). " + (ann[1] if ann else ev.get(iid, ""))
        st = "full"  # counts as covered (a live check we do)
    elif not have:
        ev_mark = "x"
        note = (ann[1] if ann else None) or FAMILY_GAP.get(fam, "Not implemented.")
        st = "none"
    elif ann and ann[0] == "partial":
        ev_mark = "~"
        note = ann[1]
        st = "partial"
    else:
        ev_mark = "Y"
        note = (ann[1] if ann else "") or ev.get(iid, "")
        st = "full"
    ec_mark = "⊘" if suppressed else "Y"
    rows.setdefault(fam, []).append((iid, desc, ec_mark, ev_mark, note))
    c = counts.setdefault(fam, [0, 0, 0, 0, 0])
    c[{"full": 0, "partial": 1, "none": 2, "supp": 3}[st]] += 1
    c[4] += 1

# epubveri-owned IDs (in ids.rs but not epubcheck)
own = sorted(set(ev) - set(ec_ids), key=lambda x: (x.split('-')[0], int(x.split('-')[1])))

# --- emit ---
o = []
o.append("# epubveri coverage vs epubcheck\n")
o.append("A per-message-ID transparency matrix: for every epubcheck message "
         "ID, does epubveri implement the same check? This is honest-not-hype "
         "— the gaps are as visible as the coverage.\n")
o.append("**Methodology.**\n")
o.append("- The ID universe is epubcheck's own `MessageId.java` "
         "(epubveri adopted epubcheck's ID scheme, so almost every ID here is "
         "epubcheck's — the signal is the *epubveri* column).\n")
o.append("- **Coverage is over the _live_ denominator** = epubcheck's total "
         "minus the IDs epubcheck **suppresses** by default (84 of 298 are "
         "disabled in epubcheck itself; not implementing those is not a gap). "
         "A raw \"X of 298\" would badly understate real coverage.\n")
o.append("- Status: **Y** full · **~** partial (epubcheck flags cases we "
         "don't — see the note) · **x** not implemented · **⊘** not a live "
         "check (epubcheck-suppressed, or an epubcheck runtime message).\n")
o.append("- **Review status.** Families marked *reviewed* below have had "
         "each ID's full/partial status checked against the source by hand. "
         "The rest are *first-pass*: **x**/**⊘** are reliable (derived from "
         "the code + epubcheck's severities), but a **Y** there means only "
         "\"epubveri has this ID\" and hasn't yet been checked for "
         "partialness — treat those as provisional.\n")
o.append("- _Generated by `scripts/gen-coverage.py` — regenerate rather than "
         "hand-editing; the status/notes annotations live in that script._\n")

# family summary
o.append("## Summary by family\n")
o.append("| Family | full | partial | gap | ⊘ N/A | live | coverage | review |")
o.append("|---|---:|---:|---:|---:|---:|---:|:---:|")
tot = [0, 0, 0, 0, 0]
for fam in sorted(counts, key=fam_key):
    c = counts[fam]
    for i in range(5): tot[i] += c[i]
    live = c[4] - c[3]
    cov = f"{(c[0]+c[1])}/{live}" if live else "—"
    rv = "reviewed" if fam in REVIEWED else "first-pass"
    o.append(f"| {fam} | {c[0]} | {c[1]} | {c[2]} | {c[3]} | {live} | {cov} | {rv} |")
live = tot[4] - tot[3]
o.append(f"| **All** | **{tot[0]}** | **{tot[1]}** | **{tot[2]}** | **{tot[3]}** | **{live}** | **{tot[0]+tot[1]}/{live}** | |")
o.append("")
o.append(f"**epubveri implements {tot[0]+tot[1]} of {live} live epubcheck "
         f"checks (~{round(100*(tot[0]+tot[1])/live)}%)** — {tot[0]} fully, "
         f"{tot[1]} partially — plus {len(own)} checks of its own "
         f"(`ADV-*` and viewport/data-* extras). {tot[3]} epubcheck IDs are "
         f"suppressed or non-checks and don't count.\n")

# per-ID detail
o.append("## Per-ID detail\n")
for fam in sorted(rows, key=fam_key):
    rv = "reviewed" if fam in REVIEWED else "first-pass — `Y` = has-the-ID, not yet checked for partialness"
    o.append(f"### {fam}  _({rv})_\n")
    o.append("| ID | Checks | epubcheck | epubveri | Notes |")
    o.append("|---|---|:---:|:---:|---|")
    for iid, desc, ec, ev_m, note in rows[fam]:
        note = note.replace("|", "\\|")
        desc = desc.replace("|", "\\|")
        o.append(f"| {iid} | {desc} | {ec} | {ev_m} | {note} |")
    o.append("")

# epubveri-owned
if own:
    o.append("## epubveri-owned IDs (not in epubcheck)\n")
    o.append("| ID | Checks | epubcheck | epubveri |")
    o.append("|---|---|:---:|:---:|")
    for iid in own:
        o.append(f"| {iid} | {ev.get(iid,'')} | — | Y |")
    o.append("")

out = "\n".join(o)
sys.stdout.write(out)
sys.stderr.write(f"\n[epubcheck IDs: {len(ec_ids)} | epubveri has: {len(set(ev)&set(ec_ids))} "
                 f"| epubveri-owned: {len(own)}]\n")
