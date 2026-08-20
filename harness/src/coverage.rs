//! Generate `docs/COVERAGE.md`: a per-message-ID matrix of epubveri against
//! epubcheck.
//!
//! Three of the four inputs are derived, so the document cannot drift from
//! the code: the ID universe, message text and default severities come from
//! epubcheck's own sources under `corpus/epubcheck/`, and "which IDs does
//! epubveri have" from `src/ids.rs`. The fourth is the human-judgment layer —
//! the `ANN` table below, which is where a full/partial/N-A call and its
//! reasoning live. That table is the only part worth editing by hand.
//!
//! The rule: an epubcheck ID with no constant in `src/ids.rs` is a gap (`x`).
//! An ID we have is covered, full (`Y`) unless `ANN` marks it partial (`~`)
//! or not-a-live-check (`⊘`). epubveri's own IDs get their own table.
//!
//! **Coverage is over the _live_ denominator**, and that is the one number a
//! reader takes away, so what leaves the denominator matters: an
//! epubcheck-suppressed ID, a dead ID, and an epubcheck tooling/meta message
//! are all `⊘`, and every one of them carries a note saying which. Never move
//! a real check there — the matrix is a trust signal, and reclassifying a gap
//! to buy a percentage point trades the asset for the appearance of one.
//!
//! Usage:
//!     cargo run --release -p epubveri-harness --bin coverage
//!     cargo run --release -p epubveri-harness --bin coverage -- --stdout
//!
//! Both paths are resolved from `CARGO_MANIFEST_DIR`, not the working
//! directory, so this writes the same file from anywhere in the tree.
//! Ported from `scripts/gen-coverage.py` (2026-08-03); the port writes the
//! file itself rather than relying on a `>` redirection, which truncates
//! `COVERAGE.md` to nothing when the generator fails.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use regex::Regex;

/// Status override for an ID, plus the note explaining it. `None` keeps the
/// derived status (full if `src/ids.rs` has the ID, gap if not) and supplies
/// only the note — used for gaps we do catch under a different ID, where
/// "not implemented" would be true of the ID and false of the check.
type Ann = (&'static str, Option<&'static str>, &'static str);

#[rustfmt::skip]
const ANN: &[Ann] = &[
    // --- PKG (reviewed) ---
    ("PKG-003", None,
        "epubcheck's `OCFZipChecker` reads a **58-byte** header and reports \
         this when the file cannot fill it, so it covers any container \
         shorter than that - not only an empty one, which is all this had \
         until 2026-08-18 (#80). 58 rather than 30 because the check reaches \
         past the local file header to the `mimetype` name at offset 30; a \
         comment here said 30, and that misreading is what made the two tools \
         look inconsistent when they agreed. Boundary measured from both \
         sides: 57 bytes is PKG-003, 58 is PKG-004."),
    ("PKG-004", None,
        "The other half of the same header check: long enough to fill 58 \
         bytes but not starting with `PK`. epubcheck's test is \
         `header[0] != 'P' && header[1] != 'K'` - an **and**, so `PX…` or \
         `xK…` falls through to PKG-006 instead, measured. This was guarded \
         on an image sniff until 2026-08-18, so 200 random bytes drew the \
         generic PKG-008 alone; the sniff is kept as a second route for a \
         file that is a recognisable other format yet happens to start `PK`."),
    ("PKG-006", Some("partial"),
        "Reported from the parsed zip, so it never runs on a container that \
         fails to open - where epubcheck still reports it, reading the raw \
         58-byte header (`OCFZipChecker`: filename-size != 8). Measured with \
         `PX…`/`xK…` headers, which epubcheck calls PKG-006 and we call \
         PKG-008 alone. Same family as #80 and left as its own change; \
         PKG-005 reads the same raw header there and is likely the same \
         shape, unmeasured."),
    ("PKG-020", Some("na"),
        "Unreachable for the input this tool accepts (verified 2026-08-04). \
         `OPFChecker.checkPackage` asks whether the container holds the \
         package document - but `OCFChecker` asks the same question of the \
         same container first, for every rootfile, and returns on failure \
         with OPF-002 (Fatal), which we emit identically. The only path that \
         reaches PKG-020 is `-mode opf` on an `http(s)` URL, a standalone \
         mode we do not have. No scenario expects it."),
    ("PKG-015", Some("na"),
        "Dead ID - \"unable to read EPUB contents\" exists only in \
         DefaultSeverities and the translation bundles; no Java source line \
         emits it and no scenario expects it (verified 2026-07-26, same shape \
         as OPF-036). The conditions it reads as covering are reported as \
         PKG-004/PKG-008."),
    // --- OPF (reviewed) ---
    ("OPF-052", None,
        "Membership in the 273-code MARC relator list epubcheck itself \
         carries, plus its `oth.` escape hatch; checked on `creator` only, as \
         epubcheck does (#54)."),
    ("OPF-044", None,
        "A spine item whose fallback chain exists but never reaches a content \
         document (distinct from OPF-043, no fallback at all). Split from \
         OPF-043 in #41."),
    ("OPF-010", Some("na"),
        "Dead ID - \"error resolving reference\" appears only in MessageId \
         and DefaultSeverities; no Java line emits it and no scenario expects \
         it (verified 2026-08-04, sixth of its kind after OPF-036, OPF-011, \
         PKG-015, NAV-001). Reference resolution is reported under \
         RSC-007/RSC-012."),
    ("OPF-016", None,
        "Reported for every `<rootfile>`, whatever its media type - \
         epubcheck's handler asks for the path before it looks at the type."),
    ("OPF-017", None,
        "Whitespace-only counts as empty, matching epubcheck's `trim()`."),
    ("OPF-047", None,
        "Emitted for a package document in the OEBPS 1.2 namespace (or in none \
         at all), matching epubcheck's own guard - which admits absent, empty \
         and the OEBPS 1.2 URI, and nothing else, so a package in some other \
         wrong namespace stays a schema error. Reporting it is also what \
         switches off the EPUB 2 rules the pre-EPUB format does not have: \
         required Dublin Core in the OPF namespace, `spine/@toc`, OPF-043 on \
         `text/x-oeb1-document`, OPF-035 on `text/html`, and the \
         OPF-namespace-bound package grammar. Measured on epubcheck's own \
         `opf-legacy-oebps12-*` fixtures: we used to report 7 errors against \
         its 4 findings, six of ours invented; now our set is a strict subset \
         of its and the verdict agrees."),
    ("OPF-038", None,
        "A modern content media-type inside an OEBPS 1.2 package, which wants \
         `text/x-oeb1-document`. Ported from `OPFChecker.checkItem`, which \
         asks it in two places that are not interchangeable: `text/html` is \
         OPF-038 unconditionally there (and OPF-035 in a normal package), \
         while a *blessed* type - EPUB 2's XHTML or DTBook - is OPF-038 only \
         when the item declares no `fallback`. **Counted as implemented long \
         before it was**: until 2026-08-10 `ids.rs` declared the constant and \
         no line emitted it, which the matrix reads as coverage. The dead-ID \
         rule runs in this direction too - grep a constant's uses, not its \
         declaration."),
    ("OPF-039", None,
        "The stylesheet half of OPF-038: a `text/css` item with no fallback \
         inside an OEBPS 1.2 package, which wants `text/x-oeb1-css`."),
    ("OPF-064", None,
        "Informational profile-selection message - not emitted."),
    ("NAV-001", Some("na"),
        "Dead ID - its only call site is `NavChecker`'s constructor under \
         `version == VERSION_2`, but a NavChecker is only built for an item \
         where `isNav()` holds, and `isNav()` reads the manifest `properties` \
         attribute, which only the EPUB 3 handler parses; the CLI's \
         single-file path guards the same construction with `version == \
         VERSION_3`. Unreachable from either direction. epubveri used to emit \
         it, which was a false positive - confirmed against epubcheck's own \
         output on a real mislabelled book (MobileRead #134, DNSB). An EPUB 2 \
         book carrying a nav document is still reported through the XHTML 1.1 \
         content model, exactly as epubcheck reports it."),
    ("OPF-036", Some("na"),
        "Dead ID - the video-codec-support note exists only in \
         MessageId/DefaultSeverities and the translation bundles; nothing in \
         epubcheck's source emits it and no test expects it (verified #53)."),
    ("OPF-005", None,
        "A prefix declaration ending in a name with no URI. Reported instead \
         of the OPF-004 syntax error, as epubcheck does - its parser ends in a \
         non-final URI state there (#50)."),
    ("OPF-006", None,
        "A prefix declaration whose URI half doesn't parse. Conservative, \
         matching Java's `new URI(...)`: illegal characters and malformed \
         percent-escapes only (#50)."),
    ("OPF-011", Some("na"),
        "Dead ID - commented out in epubcheck's OPFHandler30 (\"Checked with \
         Schematron\"), which reports the page-spread-left/-right conflict as \
         RSC-005. We emit that same RSC-005, and epubcheck's own test expects \
         it (verified #51)."),
    ("OPF-021", None,
        "Unregistered URI scheme in a *DTBook* content document's `<a href \
         external=\"true\">` - its only call site is DTBookHandler, not the \
         OPF. Gated behind DTBook validation, which is deliberately out of \
         scope (owner decision, #52): a legacy DAISY format EPUB 2 permits but \
         the ecosystem has moved off, same call as OPF-047. We still accept \
         `application/x-dtbook+xml` as a content type; we just don't validate \
         those documents. Counted as a gap rather than N/A on purpose - it is \
         a real check we don't do, and calling it N/A would inflate coverage \
         with a scope decision."),
    ("OPF-067", None,
        "A metadata `<link>` target that is also a manifest item - and, as \
         epubcheck has it, only when that item is not in the spine (#55)."),
    // --- RSC (partials confirmed earlier) ---
    ("RSC-005", Some("partial"),
        "XHTML content model is real (EPUB 2 XHTML 1.1 grammar + EPUB 3 HTML5 \
         grammar + Schematron nesting/IDREF rules + closed per-element \
         attribute allowlists). **SVG is not an opaque subtree** - epubcheck \
         validates SVG twice, and only the small `forgiving` grammar is \
         normative (id datatype, title/foreignObject content models, \
         `epub:type` placement); the full SVG 1.1 grammar runs with \
         `isNormative=false`, so its findings are RSC-025 usage - which is the \
         shape `src/svg.rs` already has. Of the three normative rules we do \
         all three (`epub:type` placement matched to epubcheck's own \
         allowlist), except the SVG `id` datatype. MathML: both the EPUB \
         restriction (Presentation-only + `semantics`) and the MathML3 \
         presentation **content models** are implemented - the arity of \
         `mfrac`/`msubsup`/`munderover` and the like, and table containment. \
         Attribute *values* stay permissive throughout (MathML attributes are \
         unconstrained, `role` accepts any token, `aria-*` unranged) - a \
         separate surface with its own false-positive risk and much less to \
         catch. **The NCX is grammar-validated too** (#83, `schemas/ncx.rng`): \
         its structure was checked in exactly one place - the `navPoint` \
         content model - and the format's other ~26 constraints not at all, \
         so sixteen shapes epubcheck errors on were silent here. Three \
         constraints of epubcheck's own NCX grammar are deliberately **not** \
         reproduced, each measured one book against 5.3.0 and each in the \
         looser direction: it demands `id` and `class` *together* on `text`, \
         `img` and `pageList` (so `<pageList id='x'>` alone draws `missing \
         required attribute \"class\"` there, a message no NCX author could \
         act on), and it admits at most one `navLabel` in a `pageList` while \
         fixing the order against the one the format itself defines - which \
         its own `ncx_multiNavLabel` Schematron rule contradicts by \
         presupposing several. Reproducing them would mean inventing errors \
         on valid books."),
    ("RSC-020", Some("partial"),
        "Checked: host syntax and scheme (space/comma in host, missing `//`) \
         on any reference, plus an unencoded space in a manifest href, in a \
         content-document reference (including SVG `href`/`xlink:href`) and in \
         an NCX `<content src>`. Not checked: backslashes, malformed \
         percent-escapes, and the illegal-character set. **Correcting an \
         earlier note here**: it claimed a path space is valid and that this \
         matched epubcheck. It does not - epubcheck parses every URL twice \
         through galimatias, once with a strict error handler that turns \
         WHATWG's recoverable warnings into errors, and its own two sibling \
         fixtures settle it (a `%20` href is PKG-010 warning, an unencoded one \
         is RSC-020 error). What remains unchecked is still deliberate \
         (2026-07-26): galimatias is a Maven dependency rather than vendored, \
         so the exact rule cannot be read here, and the corpus carries one \
         RSC-020 scenario, which we already pass. **The deferral's own \
         evidence has since expired, twice, which is why the space cases are \
         now done.** It rested on a scan of 61 real books finding exactly one \
         malformed relative URL; on a 375-book shelf, 17 books carry unencoded \
         spaces and one carried 28 in its NCX alone. Each space case was \
         closed only after measuring our count against epubcheck's per book \
         and finding it lower, never higher - strictly gap-closing, so none of \
         them could invent a false positive. **The remaining sites were then \
         enumerated and measured, and the answer is do nothing** (2026-08-20). \
         This check is organised per *source* here and per *reference* in \
         epubcheck, so the honest question is which of our sites it has \
         joined: RSC-012 runs at all four reference sites (guide, NCX, content \
         document, media overlay) while RSC-020 runs at three (manifest, \
         content document, NCX) - leaving the guide, the EPUB 3 navigation \
         document, media overlays, CSS `url()` and the dictionary's \
         search-key-group href. Every one of those was scanned across 375 real \
         books for an interior space and **the population is zero in all of \
         them**; the NCX was the only site with real books behind it. So these \
         cells stay empty on evidence rather than on estimate, and the \
         argument for the remaining *shapes* (backslashes, percent-escapes, \
         the illegal-character set) is likewise population, not principle. \
         Re-measure rather than re-deriving this matrix."),
    ("RSC-010", Some("partial"),
        "Three of epubcheck's four cells. It runs RSC-010 from two places in \
         `ResourceReferencesChecker`: `case HYPERLINK` (:231), where the \
         target is neither a blessed nor a deprecated-blessed item type and \
         no fallback reaches one, and `case OVERLAY_TEXT_LINK` (:257), where \
         a media overlay's `<text src>` target is not a blessed type - that \
         one with no deprecated-type exemption and no fallback test. \
         **Implemented**: the NCX `<content src>`, the nav toc link, and \
         (2026-08-18, #78) an ordinary hyperlink, XHTML and SVG alike. \
         **One deliberate divergence, measured and settled 2026-08-18**: \
         for an overlay text link to a non-blessed target we report \
         **MED-013**, epubcheck reports **RSC-010**, and each reports exactly \
         one message. The probe that settled it used a *valid* content \
         document as the overlay's target: there both tools report MED-013 \
         plus MED-010 and agree exactly, so epubcheck's MED-013 works \
         normally and its silence in the non-blessed case comes from the \
         `CheckAbortException` its RSC-010 throws. That abort drops a \
         second, unrelated package-level defect - the content document \
         declares `media-overlay` and the overlay never references it - \
         which is a real finding a user needs. **We keep MED-013 on purpose**: \
         the verdict is INVALID from both tools either way, so nothing about \
         the decision differs, and matching would mean reproducing a \
         suppression rather than a check. Same reasoning as the `&nbsp;` \
         divergence on RSC-016. Reported upstream. \
         Reported *instead of* RSC-011, never alongside it - epubcheck aborts \
         the reference's remaining checks right after, and our spine-\
         reachability loop skips the same targets for the same reason. \
         Before #78 this row read `Y | Y` while the note described a toc-only \
         implementation, which is the same overstatement RSC-014 carried \
         before 0.9.22: one cell of a check counted as the whole. \
         Measured against 5.3.0 one book per shape; the 356-book shelf is \
         byte-identical across the change, so no real book on it hyperlinks a \
         non-Content-Document resource."),
    ("RSC-014", None,
        "A *type-matching* check, and worth reading as one: epubcheck types \
         every `id` from the element carrying it - an SVG `symbol` is \
         SVG_SYMBOL, `linearGradient`/`radialGradient`/`pattern` are \
         SVG_PAINT, `clipPath` is SVG_CLIP_PATH, everything else GENERIC - \
         then requires each reference's type to match. All five live \
         reference kinds are implemented (2026-08-18): XHTML hyperlink, SVG \
         `<a xlink:href>`, `<use xlink:href>`, `fill`/`stroke=\"url(#...)\"`, \
         `cite` on blockquote/q/ins/del (EPUB 3 only), and a media overlay's \
         `<text src>`. Same-document and cross-document alike; a fragment \
         resolving to nothing is RSC-012, matching epubcheck's own split. \
         Twenty shapes measured one book per run against 5.3.0. \
         **Two of epubcheck's cells are dead and are deliberately not \
         implemented here**, because reporting where it is silent reads as a \
         false positive: nothing ever registers an SVG_CLIP_PATH reference, \
         so `clip-path=\"url(#...)\"` is unchecked and that `case` is \
         unreachable; and its SVG handlers read `xlink:href` only, so SVG \
         2's plain `<use href>` and plain `<a href>` register no reference. \
         Both confirmed by probe. \
         Also worth knowing: the single RSC-014 scenario in epubcheck's whole \
         test suite carries their own comment `# FIXME not sure this error is \
         legit`, and it covers the hyperlink-to-symbol cell. The paint cell \
         has firmer ground (SVG 1.1 §13.2 puts a document in error when a \
         paint reference is not a paint server). \
         **0 of 356 shelf books define an SVG symbol, gradient, pattern or \
         clipPath**, so no instrument here exercises this family - the \
         enumeration against epubcheck and the unit tests are the whole \
         evidence, including the overlay cell, which now has a \
         media-overlay test builder of its own."),
    ("RSC-016", Some("partial"),
        "Implemented, with one measured and deliberate divergence: a named \
         HTML entity (`&nbsp;` and friends) under an **XHTML 1.0** doctype. \
         epubcheck bundles `xhtml1-strict.dtd` but does not resolve it, so it \
         emits HTM-004 for the doctype *and* a FATAL RSC-016 per entity; we \
         bundle the entity list, resolve them, and emit HTM-004 only. \
         Verified against epubcheck 5.3.0 with a minimal book (2026-08-04) - \
         an earlier note here asserted the opposite about epubcheck and was \
         wrong. **The verdict never differs**: the irregular doctype is \
         already an error on our side, so such a book is INVALID either way. \
         Not matched on purpose, because a FATAL makes the document \
         unparseable and drops every other finding in it - one real book had \
         15 fatals and 1 other finding on a file whose siblings each produced \
         ~300 (the silent-skip class the 0.7.12-0.7.14 audits were about). \
         What matters is finding and reporting the defects; escalating one of \
         them to fatal costs the rest. **The fatal also manufactures findings \
         that are not defects**, measured on the same book (2026-08-09): \
         because the document is dropped before its ids are indexed, all 18 \
         fragments pointing into it are reported RSC-012 \"fragment identifier \
         is not defined\" - and every one of those ids is present in the file \
         (`id=\"x14-42800012.5\"` and friends, checked individually). So the \
         divergence costs epubcheck 18 false positives on this book and costs \
         us one usage-level label."),
    ("RSC-006", None,
        "Remote stylesheet references (also SVG stylesheet forms)."),
    ("RSC-030", None,
        "Any reference starting with `file:` (CSS, XHTML, SVG forms)."),
    ("RSC-022", Some("na"),
        "Not a validation check - epubcheck reporting its own Java-runtime \
         limitation. N/A for epubveri (we check image details via \
         PKG-021/022)."),
    ("RSC-024", Some("na"),
        "Not a distinct check - it is the non-normative half of a pair \
         (`normative ? RSC_017 : RSC_024`, alongside RSC-005/RSC-025). \
         epubcheck downgrades to it for validations it runs advisorily; we \
         have no non-normative mode, so it mirrors internal plumbing rather \
         than catching anything in a book (verified #57)."),
    // --- HTM (reviewed) ---
    ("HTM-002", Some("na"),
        "Dead ID - epubcheck defines a severity for it but never emits it \
         anywhere in its source. Not a live check."),
    ("HTM-005", Some("na"),
        "Dead ID - epubcheck never emits it anywhere in its source. Not a live \
         check."),
    ("HTM-011", Some("na"),
        "Undeclared entity. epubcheck's own code comment says this \"may never \
         be reported\" - an undeclared entity is a SAX parse error reported as \
         RSC-005. epubveri catches the same defect as RSC-016 (fatal). The \
         defect is covered; the ID itself is effectively dead."),
    ("HTM-044", Some("na"),
        "Dead ID - epubcheck never emits it anywhere in its source. Not a live \
         check."),
    ("HTM-045", None,
        "An empty `href=\"\"` resolves to the document itself - legal, so a \
         usage hint rather than an error (#56)."),
    // --- CSS (reviewed) ---
    ("CSS-001", None,
        "epubcheck flags exactly `direction`/`unicode-bidi` (EPUB 3 only) - we \
         match it."),
    ("CSS-006", None,
        "`position: fixed` (USAGE) - matches epubcheck's first-value-component \
         == \"fixed\" test."),
    ("CSS-008", None,
        "Covers bad-string/bad-url tokens, unterminated rules/blocks, \
         malformed declaration shapes, malformed selector lists (styloria 0.5) \
         and over-long `U+` unicode-ranges (styloria 0.6) - epubcheck's whole \
         live CSS error surface. Two of its error codes are dead and never \
         raised (`GRAMMAR_INVALID_SELECTOR`, `SCANNER_MALFORMED_ESCAPE`); \
         selector errors reach CSS-008 via `GRAMMAR_EXPECTING_TOKEN`, and \
         epubcheck does *not* validate at-rule preludes at all (its \
         ATRULE_PARAM restriction is `return true`). One deliberate deviation: \
         selector validation flags only what is malformed under Selectors \
         Level 4 **and** epubcheck flags, so modern-but-valid selectors its \
         old parser rejects stay silent. Known granularity difference, left as \
         is (re-measured 2026-08-17): in a malformed **selector list** we \
         report every bad selector, epubcheck the first and then abandons the \
         rule - `. h-100, . y-100 { }` is 2 findings here, 1 there. Both \
         selectors are genuinely malformed, so neither tool is wrong; ours \
         names each thing to fix. Same class as the `colgroup`/`col` \
         difference. Blast radius: 1 shelf book of 336, 22 findings against \
         epubcheck's 12. **Second deliberate deviation, decided 2026-08-17: \
         an empty declaration stays silent.** CSS Syntax 5.4.4 discards a \
         stray semicolon in a declaration list, so `a:link {;color:#000}` is \
         valid CSS; epubcheck's older parser reports it and we do not. This \
         is the selector policy above applied to declarations - flag only \
         what is malformed under the current spec *and* epubcheck flags - \
         and extending it from one to the other was the decision, since the \
         policy had only ever been written for selectors. Measured: 1 shelf \
         book of 375 contains an empty declaration at all (re-measured \
         2026-08-20; it was 1 of 346). **Unlike the RSC-016 divergence, this \
         one changes the verdict, and that is the part to know before diffing \
         the two tools**: CSS-008 is an ERROR, so on that book epubcheck \
         reports INVALID and we report VALID, with no other difference \
         between the two reports. It is the only verdict disagreement across \
         all 375 books. A minimal reproduction and an upstream issue draft are \
         held in `docs/UPSTREAM_TRIAGE.md` (draft G): both placements are \
         affected (`{;color:#000}` and `{color:#000;;}`), epubcheck's \
         `CSSHandler.error()` turns any parser exception into CSS-008 \
         unconditionally, and none of its own CSS-008 fixtures covers this \
         shape - they are unterminated blocks, which really are malformed. \
         A *second* book \
         used to sit in this row for a \
         different reason and no longer does - a stray declaration outside \
         any rule was reported once per bad token (4 against epubcheck's 1), \
         which was a real defect rather than a granularity choice and is \
         fixed in styloria 0.9.1. When this row shows a new count gap, ask \
         which of the two it is: several *selectors* is deliberate, several \
         findings inside *one* selector is not."),
    ("CSS-028", None,
        "A `@font-face` declaration is present (usage). Known granularity \
         difference, left as is (measured 2026-08-09): we report once per \
         `@font-face` **rule**, epubcheck once per **declaration inside it**, \
         so a block of four descriptors is 1 finding here and 4 there. It is \
         informational either way and never touches the verdict. Visible on \
         **118 of 375 shelf books** (re-measured 2026-08-20; it was 32 of \
         136). The ratio is exactly 4x in 104 of those and 2x or 3x in the \
         other 14, which is the mechanism showing through rather than an \
         exception - a block with three descriptors gives 3x. Mostly from one \
         producer whose blocks each carry \
         four descriptors - which is why the ratio looks like a suspiciously \
         exact 4x."),
    // --- MED (reviewed) ---
    ("MED-004", None,
        "Reserved for a file too short to contain a 4-byte image header, \
         matching epubcheck; a >=4-byte header that matches nothing is a \
         declared/actual mismatch (OPF-029). Aligned in #45."),
];

/// Families whose per-ID full/partial/notes have been reviewed by hand. The
/// rest are first-pass: "full" there means "epubveri has the ID", not yet
/// checked for partialness.
const REVIEWED: &[&str] = &[
    "PKG", "OPF", "RSC", "HTM", "CSS", "MED", "NAV", "NCX", "ACC", "SCP", "CHK", "INF",
];

/// Whole families / notable gaps described once, applied to every ID in the
/// family that epubveri lacks.
const FAMILY_GAP: &[(&str, &str)] = &[
    ("SCP", "Scripting checks - not implemented (no SCP family)."),
    (
        "ACC",
        "Accessibility checks - mostly not implemented (only ACC IDs epubveri \
         has a constant for are covered).",
    ),
];

/// Whole families that are N/A for epubveri - not content validation, so their
/// IDs are excluded from the live denominator (⊘) rather than counted as gaps.
/// Each such ID gets `FAMILY_NA_NOTE` unless it has its own `ANN` note.
const NA_FAMILIES: &[&str] = &["CHK", "INF"];

const FAMILY_NA_NOTE: &[(&str, &str)] = &[
    (
        "CHK",
        "epubcheck CLI/tooling message about its *custom message-overrides \
         file* (a file that renames/re-severities messages) - not EPUB content \
         validation. epubveri is an embeddable library with no such config \
         file, so this can never apply.",
    ),
    (
        "INF",
        "epubcheck meta-message flagging that one of *epubcheck's own* rules is \
         under review and its severity may change - a note about the tool, not \
         a finding about the EPUB. Nothing for epubveri to report.",
    ),
];

/// A one-line scope note printed under a family's detail header, so a reader
/// sees *why* a whole family is absent before scanning the rows - these are
/// deliberate exclusions, not oversights.
const FAMILY_SCOPE: &[(&str, &str)] = &[
    (
        "SCP",
        "All scripting-check IDs are SUPPRESSED by default in epubcheck (no \
         live check), so there is nothing here to implement.",
    ),
    (
        "CHK",
        "CHK-001..007 are epubcheck CLI/tooling messages about its custom \
         message-overrides file - not EPUB content validation. N/A for an \
         embeddable library, not a gap. **CHK-008 is not one of them**, and \
         this note used to say it was (corrected 2026-08-10): its call site is \
         the `catch (IllegalStateException)` around `checker.check()` in \
         `OPFChecker`, i.e. epubcheck reporting that one of its own checkers \
         threw and the item was skipped. Still N/A here, but for RSC-022's \
         reason rather than this family's - it describes the validator's \
         internal failure, not the book. We have no equivalent: the \
         0.7.12-0.7.14 audits removed the silent per-item bail it announces.",
    ),
    (
        "INF",
        "epubcheck meta-messages about the review status of epubcheck's own \
         rules - not findings about the EPUB. N/A, not a gap.",
    ),
    (
        "ACC",
        "epubcheck defines many ACC IDs but SUPPRESSES all but two by default; \
         epubveri implements both live ones.",
    ),
];

const FAM_ORDER: &[&str] = &[
    "PKG", "OCF", "OPF", "RSC", "HTM", "CSS", "MED", "NAV", "NCX", "ACC", "SCP", "CHK", "INF",
];

fn lookup<'a>(table: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn ann_for(id: &str) -> Option<&'static Ann> {
    ANN.iter().find(|(k, _, _)| *k == id)
}

/// Sort key shared by every ID list: family name, then the number as a
/// number - so OPF-9 precedes OPF-10, which a plain string sort would not.
fn id_key(id: &str) -> (String, u32, String) {
    let (fam, rest) = id.split_once('-').unwrap_or((id, "0"));
    // A lettered id sorts immediately after its own number, not as 0:
    // OPF-004, OPF-004a, ... OPF-004f, OPF-005.
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let letter: String = rest[digits.len()..].to_string();
    (fam.to_string(), digits.parse().unwrap_or(0), letter)
}

fn fam_key(fam: &str) -> (usize, String) {
    (
        FAM_ORDER.iter().position(|f| *f == fam).unwrap_or(99),
        fam.to_string(),
    )
}

/// One row of the per-ID table.
struct Row {
    id: String,
    desc: String,
    ec_mark: &'static str,
    ev_mark: &'static str,
    note: String,
}

/// A family's rows plus its `[full, partial, none, na, total]` tally, kept
/// together because every ID contributes to both.
struct Family {
    name: String,
    rows: Vec<Row>,
    counts: [usize; 5],
}

/// Index of `fam`'s slot, appending it if this is its first ID. Families stay
/// in first-seen order until the final sort, exactly as the dict they replace.
fn fam_slot(fams: &mut Vec<Family>, fam: &str) -> usize {
    match fams.iter().position(|f| f.name == fam) {
        Some(i) => i,
        None => {
            fams.push(Family {
                name: fam.to_string(),
                rows: Vec::new(),
                counts: [0; 5],
            });
            fams.len() - 1
        }
    }
}

/// Python's `round()` is half-to-even; Rust's `f64::round` is half-away-from-
/// zero. They differ only on an exact `.5`, which the coverage percentage can
/// land on - matching the original keeps the published number stable across
/// the port.
fn round_half_even(x: f64) -> i64 {
    let f = x.floor();
    let diff = x - f;
    let f = f as i64;
    match diff.partial_cmp(&0.5) {
        Some(std::cmp::Ordering::Less) => f,
        Some(std::cmp::Ordering::Greater) => f + 1,
        _ if f % 2 == 0 => f,
        _ => f + 1,
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ec = root.join("corpus/epubcheck/src/main/resources/com/adobe/epubcheck/messages");
    let ecj = root.join("corpus/epubcheck/src/main/java/com/adobe/epubcheck/messages");
    if !ecj.is_dir() {
        eprintln!("epubcheck sources not found at {}", ecj.display());
        eprintln!("this needs the corpus submodule checked out");
        std::process::exit(1);
    }

    let read = |p: PathBuf| -> String {
        // Lossy on purpose: MessageBundle.properties is not valid UTF-8, and
        // the replacement characters land in text we truncate anyway.
        match std::fs::read(&p) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                eprintln!("cannot read {}: {e}", p.display());
                std::process::exit(1);
            }
        }
    };

    // --- 1. epubcheck ID universe (MessageId.java) ---
    // Match the enum NAME, not the string literal - epubcheck's literals are
    // inconsistent (some use "HTM_054" with an underscore instead of
    // "HTM-054"). Normalize to hyphens.
    //
    // The name may end in a lowercase letter, and 17 of them do
    // (`HTM_060a`, `OPF_004a`..`f`, `OPF_007a`..`c`, `RSC_007w`, ...).
    // Requiring digits at the end dropped every one of them from the ID
    // universe, so the matrix was computed against a denominator that was
    // missing 16 live checks and the published percentage was overstated
    // (#70).
    let mid = read(ecj.join("MessageId.java"));
    let re_id = Regex::new(r"(?m)^\s*([A-Z]+)_([0-9]+[a-z]?)\(").unwrap();
    let mut ec_ids: Vec<String> = re_id
        .captures_iter(&mid)
        .map(|c| format!("{}-{}", &c[1], &c[2]))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    ec_ids.sort_by_key(|i| id_key(i));

    // --- 2. epubcheck message text (MessageBundle.properties) ---
    let bundle = read(ec.join("MessageBundle.properties"));
    let re_text = Regex::new(r"^([A-Z]+)_([0-9]+[a-z]?)=(.*)$").unwrap();
    let mut text: HashMap<String, String> = HashMap::new();
    for line in bundle.lines() {
        if let Some(c) = re_text.captures(line) {
            text.insert(
                format!("{}-{}", &c[1], &c[2]),
                c[3].trim().trim_matches('"').to_string(),
            );
        }
    }

    // --- 3. epubcheck severity (DefaultSeverities.java) ---
    let sevsrc = read(ecj.join("DefaultSeverities.java"));
    let re_sev = Regex::new(r"MessageId\.([A-Z]+)_([0-9]+[a-z]?),\s*Severity\.([A-Z]+)").unwrap();
    let mut sev: HashMap<String, String> = HashMap::new();
    for c in re_sev.captures_iter(&sevsrc) {
        let s = &c[3];
        let capitalized = format!("{}{}", &s[..1], s[1..].to_lowercase());
        sev.insert(format!("{}-{}", &c[1], &c[2]), capitalized);
    }

    // --- 4. epubveri IDs + inline comments (ids.rs) ---
    // The trailing `// comment` is OPTIONAL - some IDs (e.g. RSC-005) have
    // none, and requiring it would wrongly drop them from epubveri's coverage.
    let ids_rs = read(root.join("src/ids.rs"));
    // Our own literals are inconsistent for the same reason epubcheck's are
    // - `HTM_060a` mirrors the separator epubcheck prints, `OPF-086b` the one
    // it prints there - so accept either and fold to `-` for the join.
    let re_ev =
        Regex::new(r#"pub const [A-Z0-9_]+: &str = "([A-Z]+[-_][0-9]+[a-z]?)";(?:\s*//\s*(.*))?"#)
            .unwrap();
    let mut ev: HashMap<String, String> = HashMap::new();
    for c in re_ev.captures_iter(&ids_rs) {
        let comment = c.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        ev.insert(c[1].replacen('_', "-", 1), comment.to_string());
    }

    // --- build rows ---
    let re_ws = Regex::new(r"\s+").unwrap();
    let mut fams: Vec<Family> = Vec::new();

    for iid in &ec_ids {
        let fam = iid.split('-').next().unwrap().to_string();

        let raw = text.get(iid).cloned().unwrap_or_default();
        let mut desc = re_ws.replace_all(&raw, " ").into_owned();
        if desc.chars().count() > 90 {
            desc = desc.chars().take(87).collect::<String>() + "...";
        }
        if desc.is_empty() {
            desc = "_(no message text in epubcheck's bundle)_".to_string();
        }

        let have = ev.contains_key(iid);
        let ann = ann_for(iid);
        let suppressed = sev.get(iid).map(|s| s == "Suppressed").unwrap_or(false);

        // A whole-family N/A: an epubcheck tooling / meta message, not content
        // validation. ⊘ on our side, excluded from the live denominator, with
        // a note saying why (so it reads as a deliberate exclusion).
        if NA_FAMILIES.contains(&fam.as_str()) && ann.map(|a| a.1) != Some(Some("partial")) {
            let note = ann
                .map(|a| a.2)
                .or_else(|| lookup(FAMILY_NA_NOTE, &fam))
                .unwrap_or("N/A.");
            let i = fam_slot(&mut fams, &fam);
            fams[i].rows.push(Row {
                id: iid.clone(),
                desc,
                ec_mark: "Y",
                ev_mark: "⊘",
                note: note.to_string(),
            });
            fams[i].counts[3] += 1;
            fams[i].counts[4] += 1;
            continue;
        }

        // Not a real validation check for epubveri (e.g. an epubcheck
        // runtime-limitation message) - excluded from the live denominator.
        if let Some(a) = ann
            && a.1 == Some("na")
        {
            let i = fam_slot(&mut fams, &fam);
            fams[i].rows.push(Row {
                id: iid.clone(),
                desc,
                ec_mark: "Y",
                ev_mark: "⊘",
                note: a.2.to_string(),
            });
            fams[i].counts[3] += 1;
            fams[i].counts[4] += 1;
            continue;
        }

        let (ev_mark, note, slot): (&str, String, usize) = if suppressed && !have {
            // epubcheck disabled this ID by default -> not a real check, N/A.
            (
                "⊘",
                "epubcheck-suppressed (disabled by default) — not a gap".to_string(),
                3,
            )
        } else if suppressed && have {
            let tail = ann
                .map(|a| a.2.to_string())
                .unwrap_or_else(|| ev.get(iid).cloned().unwrap_or_default());
            (
                "Y+",
                format!("epubveri reports this; epubcheck suppresses it (we are stricter). {tail}"),
                0, // counts as covered (a live check we do)
            )
        } else if !have {
            let note = ann
                .map(|a| a.2)
                .or_else(|| lookup(FAMILY_GAP, &fam))
                .unwrap_or("Not implemented.");
            ("x", note.to_string(), 2)
        } else if ann.map(|a| a.1) == Some(Some("partial")) {
            ("~", ann.unwrap().2.to_string(), 1)
        } else {
            let note = match ann.map(|a| a.2).unwrap_or("") {
                "" => ev.get(iid).cloned().unwrap_or_default(),
                n => n.to_string(),
            };
            ("Y", note, 0)
        };

        let i = fam_slot(&mut fams, &fam);
        fams[i].rows.push(Row {
            id: iid.clone(),
            desc,
            ec_mark: if suppressed { "⊘" } else { "Y" },
            ev_mark,
            note,
        });
        fams[i].counts[slot] += 1;
        fams[i].counts[4] += 1;
    }

    fams.sort_by_key(|f| fam_key(&f.name));

    // epubveri-owned IDs (in ids.rs but not epubcheck)
    let ec_set: HashSet<&String> = ec_ids.iter().collect();
    let mut own: Vec<String> = ev.keys().filter(|k| !ec_set.contains(k)).cloned().collect();
    own.sort_by_key(|i| id_key(i));

    // --- emit ---
    let mut o: Vec<String> = vec!["# epubveri coverage vs epubcheck\n".into()];
    o.push(
        "A per-message-ID transparency matrix: for every epubcheck message ID, \
         does epubveri implement the same check? This is honest-not-hype — the \
         gaps are as visible as the coverage.\n"
            .into(),
    );
    o.push("**Methodology.**\n".into());
    o.push(
        "- The ID universe is epubcheck's own `MessageId.java` (epubveri \
         adopted epubcheck's ID scheme, so almost every ID here is \
         epubcheck's — the signal is the *epubveri* column).\n"
            .into(),
    );
    o.push(
        "- **Coverage is over the _live_ denominator** = epubcheck's total \
         minus every ID that isn't a live *content-validation* check: the ones \
         epubcheck **suppresses** by default, plus its runtime/tooling/meta \
         messages (⊘ below). Not implementing those is not a gap - each such \
         row carries a note saying why it's out of scope, so the exclusions \
         read as deliberate, not as oversights. A raw \"X of 298\" would badly \
         understate real coverage.\n"
            .into(),
    );
    o.push(
        "- Status: **Y** full · **~** partial (epubcheck flags cases we \
         don't — see the note) · **x** not implemented (a real gap) · **⊘** \
         not a live content check — epubcheck-suppressed, a dead ID, or an \
         epubcheck runtime/tooling/meta message; **the row's note says \
         which**.\n"
            .into(),
    );
    o.push(
        "- **Review status.** Families marked *reviewed* below have had each \
         ID's full/partial status checked against the source by hand. The rest \
         are *first-pass*: **x**/**⊘** are reliable (derived from the code + \
         epubcheck's severities), but a **Y** there means only \"epubveri has \
         this ID\" and hasn't yet been checked for partialness — treat those \
         as provisional.\n"
            .into(),
    );
    o.push(
        "- _Generated by `cargo run -p epubveri-harness --bin coverage` — \
         regenerate rather than hand-editing; the status/notes annotations \
         live in `harness/src/coverage.rs`._\n"
            .into(),
    );

    // family summary
    o.push("## Summary by family\n".into());
    o.push("| Family | full | partial | gap | ⊘ N/A | live | coverage | review |".into());
    o.push("|---|---:|---:|---:|---:|---:|---:|:---:|".into());
    let mut tot = [0usize; 5];
    for f in &fams {
        let c = f.counts;
        for i in 0..5 {
            tot[i] += c[i];
        }
        let live = c[4] - c[3];
        let cov = if live > 0 {
            format!("{}/{}", c[0] + c[1], live)
        } else {
            "—".to_string()
        };
        let rv = if REVIEWED.contains(&f.name.as_str()) {
            "reviewed"
        } else {
            "first-pass"
        };
        o.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            f.name, c[0], c[1], c[2], c[3], live, cov, rv
        ));
    }
    let live = tot[4] - tot[3];
    o.push(format!(
        "| **All** | **{}** | **{}** | **{}** | **{}** | **{}** | **{}/{}** | |",
        tot[0],
        tot[1],
        tot[2],
        tot[3],
        live,
        tot[0] + tot[1],
        live
    ));
    o.push(String::new());
    let covered = tot[0] + tot[1];
    o.push(format!(
        "**epubveri implements {covered} of {live} live epubcheck checks \
         (~{}%)** — {} fully, {} partially — plus {} checks of its own \
         (`ADV-*` and viewport/data-* extras). {} epubcheck IDs are suppressed \
         or non-checks and don't count.\n",
        round_half_even(100.0 * covered as f64 / live as f64),
        tot[0],
        tot[1],
        own.len(),
        tot[3]
    ));

    // per-ID detail
    o.push("## Per-ID detail\n".into());
    for f in &fams {
        let rv = if REVIEWED.contains(&f.name.as_str()) {
            "reviewed"
        } else {
            "first-pass — `Y` = has-the-ID, not yet checked for partialness"
        };
        o.push(format!("### {}  _({rv})_\n", f.name));
        if let Some(scope) = lookup(FAMILY_SCOPE, &f.name) {
            o.push(format!("> **Scope:** {scope}\n"));
        }
        o.push("| ID | Checks | epubcheck | epubveri | Notes |".into());
        o.push("|---|---|:---:|:---:|---|".into());
        for r in &f.rows {
            o.push(format!(
                "| {} | {} | {} | {} | {} |",
                r.id,
                r.desc.replace('|', "\\|"),
                r.ec_mark,
                r.ev_mark,
                r.note.replace('|', "\\|")
            ));
        }
        o.push(String::new());
    }

    // epubveri-owned
    if !own.is_empty() {
        o.push("## epubveri-owned IDs (not in epubcheck)\n".into());
        o.push("| ID | Checks | epubcheck | epubveri |".into());
        o.push("|---|---|:---:|:---:|".into());
        for iid in &own {
            o.push(format!(
                "| {iid} | {} | — | Y |",
                ev.get(iid).cloned().unwrap_or_default()
            ));
        }
        o.push(String::new());
    }

    let out = o.join("\n");
    let have_ec = ev.keys().filter(|k| ec_set.contains(k)).count();

    if std::env::args().any(|a| a == "--stdout") {
        print!("{out}");
    } else {
        let dest = root.join("docs/COVERAGE.md");
        if let Err(e) = std::fs::write(&dest, &out) {
            eprintln!("cannot write {}: {e}", dest.display());
            std::process::exit(1);
        }
        eprintln!("wrote {}", dest.display());
    }
    eprintln!(
        "\n[epubcheck IDs: {} | epubveri has: {have_ec} | epubveri-owned: {}]",
        ec_ids.len(),
        own.len()
    );
}
