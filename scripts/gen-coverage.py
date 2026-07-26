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
    "PKG-015": ("na", "Dead ID - \"unable to read EPUB contents\" exists only "
        "in DefaultSeverities and the translation bundles; no Java source "
        "line emits it and no scenario expects it (verified 2026-07-26, same "
        "shape as OPF-036). The conditions it reads as covering are reported "
        "as PKG-004/PKG-008."),
    "PKG-001": (None, "Not emitted, and the condition cannot arise here: it "
        "fires only when the caller demands a specific EPUB version that "
        "disagrees with the book's own (epubcheck's `--version`), and "
        "epubveri has no version-override flag. A missing *feature* rather "
        "than a missing check - implementing the message means first "
        "deciding whether to validate a 3.0 book as 2.0 on request, which is "
        "a behaviour change, not a rule (#61)."),
    # --- OPF (reviewed) ---
    "OPF-052": (None, "Membership in the 273-code MARC relator list "
        "epubcheck itself carries, plus its `oth.` escape hatch; checked on "
        "`creator` only, as epubcheck does (#54)."),
    "OPF-044": (None, "A spine item whose fallback chain exists but never "
        "reaches a content document (distinct from OPF-043, no fallback at "
        "all). Split from OPF-043 in #41."),
    "OPF-010": (None, "Not emitted; reference resolution is covered under "
        "RSC-007/RSC-012."),
    "OPF-016": (None, "Not emitted; a rootfile missing `full-path` is caught "
        "via the container.xml RNG grammar (RSC-005)."),
    "OPF-017": (None, "Not emitted; a rootfile with an empty `full-path` is "
        "caught via the container.xml RNG grammar (RSC-005)."),
    "OPF-047": (None, "Legacy OEBPS 1.2 backwards-compat syntax - deliberately "
        "out of scope (pre-EPUB format)."),
    "OPF-064": (None, "Informational profile-selection message - not emitted."),
    "OPF-036": ("na", "Dead ID - the video-codec-support note exists only in "
        "MessageId/DefaultSeverities and the translation bundles; nothing in "
        "epubcheck's source emits it and no test expects it (verified #53)."),
    "OPF-005": (None, "A prefix declaration ending in a name with no URI. "
        "Reported instead of the OPF-004 syntax error, as epubcheck does - "
        "its parser ends in a non-final URI state there (#50)."),
    "OPF-006": (None, "A prefix declaration whose URI half doesn't parse. "
        "Conservative, matching Java's `new URI(...)`: illegal characters and "
        "malformed percent-escapes only (#50)."),
    "OPF-011": ("na", "Dead ID - commented out in epubcheck's OPFHandler30 "
        "(\"Checked with Schematron\"), which reports the page-spread-left/"
        "-right conflict as RSC-005. We emit that same RSC-005, and "
        "epubcheck's own test expects it (verified #51)."),
    "OPF-021": (None, "Unregistered URI scheme in a *DTBook* content document's "
        "`<a href external=\"true\">` - its only call site is DTBookHandler, not "
        "the OPF. Gated behind DTBook validation, which is deliberately out of "
        "scope (owner decision, #52): a legacy DAISY format EPUB 2 permits but "
        "the ecosystem has moved off, same call as OPF-047. We still accept "
        "`application/x-dtbook+xml` as a content type; we just don't validate "
        "those documents. Counted as a gap rather than N/A on purpose - it is a "
        "real check we don't do, and calling it N/A would inflate coverage with "
        "a scope decision."),
    "OPF-067": (None, "A metadata `<link>` target that is also a manifest "
        "item - and, as epubcheck has it, only when that item is not in the "
        "spine (#55)."),
    # --- RSC (partials confirmed earlier) ---
    "RSC-005": ("partial",
        "XHTML content model is real (EPUB 2 XHTML 1.1 grammar + EPUB 3 HTML5 "
        "grammar + Schematron nesting/IDREF rules + closed per-element "
        "attribute allowlists). **SVG is not an opaque subtree** - epubcheck "
        "validates SVG twice, and only the small `forgiving` grammar is "
        "normative (id datatype, title/foreignObject content models, "
        "`epub:type` placement); the full SVG 1.1 grammar runs with "
        "`isNormative=false`, so its findings are RSC-025 usage - which is "
        "the shape `src/svg.rs` already has. Of the three normative rules we "
        "do all three (`epub:type` placement matched to epubcheck's own "
        "allowlist), except the SVG `id` datatype. MathML: both the EPUB "
        "restriction (Presentation-only + `semantics`) and the MathML3 "
        "presentation **content models** are implemented - the arity of "
        "`mfrac`/`msubsup`/`munderover` and the like, and table containment. "
        "Attribute *values* stay permissive throughout (MathML attributes are "
        "unconstrained, `role` accepts any token, `aria-*` unranged) - a "
        "separate surface with its own false-positive risk and much less to "
        "catch."),
    "RSC-020": ("partial",
        "Checked: host syntax and scheme (space/comma in host, missing `//`) "
        "on any reference, plus an unencoded space in a manifest href. Not "
        "checked: an unencoded space in a *content document* reference, "
        "backslashes, malformed percent-escapes, and the illegal-character "
        "set. **Correcting an earlier note here**: it claimed a path space is "
        "valid and that this matched epubcheck. It does not - epubcheck "
        "parses every URL twice through galimatias, once with a strict error "
        "handler that turns WHATWG's recoverable warnings into errors, and "
        "its own two sibling fixtures settle it (a `%20` href is PKG-010 "
        "warning, an unencoded one is RSC-020 error). Deliberately left "
        "partial (2026-07-26): galimatias is a Maven dependency rather than "
        "vendored, so the exact rule cannot be read here; the corpus carries "
        "one RSC-020 scenario, which we already pass; and a scan of 61 real "
        "books found exactly one malformed relative URL - the manifest-href "
        "space we already catch. Building strictness against an unreadable "
        "reference, on one test case, for something 60 of 61 books never do "
        "is more likely to invent false positives than to catch defects."),
    "RSC-006": (None, "Remote stylesheet references (also SVG stylesheet forms)."),
    "RSC-030": (None, "Any reference starting with `file:` (CSS, XHTML, SVG forms)."),
    "RSC-022": ("na", "Not a validation check - epubcheck reporting its own "
        "Java-runtime limitation. N/A for epubveri (we check image details "
        "via PKG-021/022)."),
    "RSC-024": ("na", "Not a distinct check - it is the non-normative half of "
        "a pair (`normative ? RSC_017 : RSC_024`, alongside RSC-005/RSC-025). "
        "epubcheck downgrades to it for validations it runs advisorily; we "
        "have no non-normative mode, so it mirrors internal plumbing rather "
        "than catching anything in a book (verified #57)."),
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
    "HTM-045": (None, "An empty `href=\"\"` resolves to the document itself - "
        "legal, so a usage hint rather than an error (#56)."),
    # --- CSS (reviewed) ---
    "CSS-001": (None, "epubcheck flags exactly `direction`/`unicode-bidi` "
        "(EPUB 3 only) - we match it."),
    "CSS-006": (None, "`position: fixed` (USAGE) - matches epubcheck's "
        "first-value-component == \"fixed\" test."),
    "CSS-008": (None, "Covers bad-string/bad-url tokens, unterminated "
        "rules/blocks, malformed declaration shapes, malformed selector lists "
        "(styloria 0.5) and over-long `U+` unicode-ranges (styloria 0.6) - "
        "epubcheck's whole live CSS error surface. Two of its error codes are "
        "dead and never raised (`GRAMMAR_INVALID_SELECTOR`, "
        "`SCANNER_MALFORMED_ESCAPE`); selector errors reach CSS-008 via "
        "`GRAMMAR_EXPECTING_TOKEN`, and epubcheck does *not* validate at-rule "
        "preludes at all (its ATRULE_PARAM restriction is `return true`). One "
        "deliberate deviation: selector validation flags only what is "
        "malformed under Selectors Level 4 **and** epubcheck flags, so "
        "modern-but-valid selectors its old parser rejects stay silent."),    # --- MED (reviewed) ---
    "MED-004": (None, "Reserved for a file too short to contain a 4-byte image "
        "header, matching epubcheck; a >=4-byte header that matches nothing is "
        "a declared/actual mismatch (OPF-029). Aligned in #45."),
}

# Families whose per-ID full/partial/notes have been reviewed by hand. The
# rest are first-pass: "full" there means "epubveri has the ID", not yet
# checked for partialness.
REVIEWED = {"PKG", "OPF", "RSC", "HTM", "CSS", "MED", "NAV", "NCX",
            "ACC", "SCP", "CHK", "INF"}
# whole families / notable gaps described once (applied to every id in the
# family that epubveri lacks, as a shared note)
FAMILY_GAP = {
    "SCP": "Scripting checks - not implemented (no SCP family).",
    "ACC": "Accessibility checks - mostly not implemented (only ACC IDs epubveri "
           "has a constant for are covered).",
}

# Whole families that are N/A for epubveri - not content validation, so their
# IDs are excluded from the live denominator (⊘) rather than counted as gaps.
# Each such ID gets FAMILY_NA_NOTE[fam] unless it has its own ANN note.
NA_FAMILIES = {"CHK", "INF"}
FAMILY_NA_NOTE = {
    "CHK": "epubcheck CLI/tooling message about its *custom message-overrides "
           "file* (a file that renames/re-severities messages) - not EPUB "
           "content validation. epubveri is an embeddable library with no such "
           "config file, so this can never apply.",
    "INF": "epubcheck meta-message flagging that one of *epubcheck's own* rules "
           "is under review and its severity may change - a note about the "
           "tool, not a finding about the EPUB. Nothing for epubveri to report.",
}

# A one-line scope note printed under a family's detail header, so a reader
# sees *why* a whole family is absent before scanning the rows - these are
# deliberate exclusions, not oversights.
FAMILY_SCOPE = {
    "SCP": "All scripting-check IDs are SUPPRESSED by default in epubcheck "
           "(no live check), so there is nothing here to implement.",
    "CHK": "Every CHK ID is an epubcheck CLI/tooling message about its custom "
           "message-overrides file - not EPUB content validation. N/A for an "
           "embeddable library, not a gap.",
    "INF": "epubcheck meta-messages about the review status of epubcheck's own "
           "rules - not findings about the EPUB. N/A, not a gap.",
    "ACC": "epubcheck defines many ACC IDs but SUPPRESSES all but two by "
           "default; epubveri implements both live ones.",
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
    if fam in NA_FAMILIES and not (ann and ann[0] == "partial"):
        # A whole-family N/A: an epubcheck tooling / meta message, not content
        # validation. ⊘ on our side, excluded from the live denominator, with
        # a note saying why (so it reads as a deliberate exclusion).
        note = (ann[1] if ann else None) or FAMILY_NA_NOTE.get(fam, "N/A.")
        rows.setdefault(fam, []).append((iid, desc, "Y", "⊘", note))
        c = counts.setdefault(fam, [0, 0, 0, 0, 0])
        c[3] += 1
        c[4] += 1
        continue
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
         "minus every ID that isn't a live *content-validation* check: the "
         "ones epubcheck **suppresses** by default, plus its runtime/tooling/"
         "meta messages (⊘ below). Not implementing those is not a gap - each "
         "such row carries a note saying why it's out of scope, so the "
         "exclusions read as deliberate, not as oversights. A raw \"X of 298\" "
         "would badly understate real coverage.\n")
o.append("- Status: **Y** full · **~** partial (epubcheck flags cases we "
         "don't — see the note) · **x** not implemented (a real gap) · **⊘** "
         "not a live content check — epubcheck-suppressed, a dead ID, or an "
         "epubcheck runtime/tooling/meta message; **the row's note says which**."
         "\n")
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
    if fam in FAMILY_SCOPE:
        o.append(f"> **Scope:** {FAMILY_SCOPE[fam]}\n")
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
