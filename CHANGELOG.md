# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

## [0.5.14] - 2026-07-18

Two additions, both opt-in or usage-level, so no book's verdict changes: an
EPUB 2 empty-metadata usage note (OPF-072), and an opt-in CSS advisory layer.

### Added

- **OPF-072 (usage): an empty `dc:*` metadata element in an EPUB 2 package.**
  A `dc:` element (other than `dc:title`/`dc:date`, which have their own rules)
  whose content is empty or whitespace-only now draws OPF-072 at usage severity,
  matching epubcheck. EPUB 2 only. (#95, reported by Doitsu on the MobileRead
  forum.)
- **Opt-in CSS advisory checks (`--advisory`), via `styloria` 0.3.** Enables
  tool-owned advisory findings epubcheck has no verdict on — currently unknown
  CSS **property** names (`ADV-001`) and unknown at-rule **descriptor** names
  (`ADV-002`, e.g. a bogus `@font-face` descriptor), in stylesheets, inline
  `<style>` blocks, and `style="…"` attributes. Always `Usage` severity, so
  they never affect the exit code, and **off by default** — with the flag off
  the output is byte-identical (the corpus is unchanged: 599/607, 0 over-
  reported). `ADV-*` is a deliberately distinct, epubveri-owned message family:
  matching epubcheck on verdicts means not inventing a `CSS-0xx` it does not
  define. Library API: `Options { profile, advisory }` with
  `validate_bytes_with_options` / `validate_path_with_options`.
- Dependency: `styloria` 0.2 → 0.3 (adds its `validate` layer).

## [0.5.13] - 2026-07-17

An EPUB 2 content model - EPUB 2 books are no longer validated against HTML5 -
plus the rest of Tier-C's message detail. **This changes verdicts for EPUB 2
books**: see the note under the first entry.

### Fixed / Changed

- **EPUB 2 content documents now use an EPUB 2 (XHTML 1.1 + OPS 2.0.1) content
  model.** They were validated against the EPUB 3 (HTML5) grammar, which is wrong
  in both directions: `<big>`/`<tt>`/`<acronym>` are valid XHTML 1.1 but removed
  in HTML5 (we flagged them), and `<s>`/`<u>` and the HTML5 additions are the
  reverse (we missed them). Now they match epubcheck. (#24, reported by Doitsu on
  the MobileRead forum.)

  **Heads-up — this newly flags common shapes.** XHTML 1.1 is block-level under
  `<body>` (and `<p>` takes inline content only), so a `<br>` or `<span>` directly
  under `<body>`, or a block element inside a `<p>`, is now an `RSC-005` error.
  This is very common - Calibre and similar tools produce it - and on two real
  EPUB 2 books it went from 0 findings to 401 and 25. This is **not a regression**:
  those documents are XHTML 1.1 invalid and epubcheck reports them identically. The
  point is parity; repair is a separate tool's job. (#13, reported by Doitsu.)

- **`RSC-005` messages now name what was expected, and tell a bad attribute name
  from a bad value.** A content-model rejection reads `element "span" is not allowed
  here; expected one of "address", "blockquote", … "ul"` where the model is a
  genuine constraint (epubcheck lists these too). And an attribute failure now
  distinguishes `attribute "x" is not allowed here` (unknown name) from `value of
  attribute "dir" is invalid: "sideways"` (known name, bad value) - the two were
  one message before. (Tier-C.)

- **`OPF-018` is downgraded to the usage-level `OPF-018b` for scripted content** -
  a declared-but-unused `remote-resources` property can't be disproven when a
  script might fetch remotely, so epubcheck reports it as usage, and now so do we.
  (#27.)

## [0.5.12] - 2026-07-17

Fixes a false-fatal regression epubsana reported (#25), plus everything found by
auditing for its kind and by two new corpus-harness scoring checks. Corpus recall
599/607.

### Fixed

- **EPUB 2 documents with a `[` in the body no longer draw a false fatal.** 0.5.11's
  DTD-entity injection searched the whole document for the DOCTYPE's internal subset
  rather than the DOCTYPE itself, so a `[1]` footnote marker was mistaken for it and
  the entity declarations were injected into the body, breaking the parse. 78 false
  fatals across 11 valid books on a real shelf; the DOCTYPE is now scanned, not
  searched, and the same bug in two other DOCTYPE readers is fixed with it. (#25,
  reported by epubsana.)

- **`OPF-043` is now an error** (was a warning) - a spine item the reading system
  can't render, with no fallback, is a hole in the reading order; epubcheck's
  severity table and fixture both say error.

- **A valid EPUB 2 DTBook book is no longer flagged `OPF-043`.** The content types
  allowed directly in the spine are version-specific - XHTML or SVG in EPUB 3, but
  XHTML or DTBook (`application/x-dtbook+xml`) in EPUB 2 - and we applied the EPUB 3
  set to all. (Surfaced by the `OPF-043` severity fix.)

### Changed

- **`element_path` (JSON) now pins the attribute or text run a finding is about** -
  eight content-document findings about a specific attribute end their path in
  `/@name` instead of stopping at the element, and loose-text findings end in
  `/text()[n]`. (#20.)

- **The corpus harness now scores two more things** (internal, but it's how the
  above were found): over-reporting (findings on a book that expects one specific
  thing) and severity agreement (an id reported at the wrong severity). Both drove
  real fixes; both are now clean. Plus invariant tests over the rule tables - the
  class of bug no fixture can reach. (#26.)

## [0.5.11] - 2026-07-17

Doitsu's EPUB 2 test case and JSWolf's unused-resource request, both from the MobileRead
thread, plus a position fix and a licensing gap found along the way. Corpus recall is
unchanged (600/607) — and could not move for any of it, which is becoming the pattern
worth naming: epubcheck's corpus scores none of these, either because the finding is
usage-level (invisible to a metric that checks the expected ID was reported, not that
nothing extra was) or because it has no scenario for the rule at all.

### Added

- **`OPF-097`: a manifest resource that no content document uses** (usage). An unused font
  or image is almost always dead weight left by an earlier revision; the book stays valid
  and the note says which. (Requested by JSWolf on the MobileRead forum, for unused fonts
  and images specifically. epubcheck has the rule; epubveri simply hadn't implemented it —
  no new message ID was invented.)

  "Referenced" is narrower than it sounds, and the narrowness is the rule: a **hyperlink
  does not count**. Only references that embed or load a resource do — an image drawn, a
  stylesheet applied, a font loaded, a media overlay attached. What is exempt is what the
  *container* consumes rather than a document: the spine, the nav document, the NCX.

  Note the message says "no **content document** references it", and the precision is
  deliberate: a `properties="cover-image"` cover with no cover page is reported, because it
  is referenced by the package document and used by the reading system, but drawn by
  nothing. epubcheck reports it too. The note is factually true; what to do about it is the
  author's call, which is why this is usage and not advice.

### Fixed

- **`OPF-096` no longer fires on EPUB 2.** "Non-linear content is not reachable from the
  reading order" is an EPUB 3 requirement; EPUB 2.0.1 has none, so we were inventing an
  error on books epubcheck passes. Three independent signals agreed: every `OPF-096`
  fixture in epubcheck's corpus lives under `epub3/`, epubcheck stays silent on a real
  EPUB 2 book that we flagged, and the note already in our own code cited epubcheck's
  *EPUB 3* checker as where the rule came from. Same class as #9, #21 and #24 — an EPUB 3
  rule leaking into EPUB 2. (Reported by Doitsu on the MobileRead forum.)

- **Duplicate NCX `playOrder` values are now reported** (`RSC-005`). `playOrder` is the
  reading position, so two elements claiming the same one while pointing elsewhere is a
  contradiction; epubcheck flagged four on a real book where epubveri flagged none. The
  exception is what stops this being a plain duplicate scan: elements naming the *same*
  target may share a value, since that is one position reached by two routes. Every
  colliding element is reported, not one arbitrary member. (Reported by Doitsu on the
  MobileRead forum.)

- **`OPF-062` (usage) is now reported for Adobe's `page-map` spine extension.** The
  attribute already drew an `RSC-005`; the two say different things — one that the document
  is invalid, the other *which* non-standard feature is in use, which is the part that tells
  an author whether they meant it. (Reported by Doitsu on the MobileRead forum.)

- **Positions reported for EPUB 2 documents with DTD-declared entities are now exact.**
  0.5.10 made those documents parse by injecting `<!ENTITY>` declarations before the
  DOCTYPE's closing `>`. That adds no newline, so line numbers were always right — but
  inserting text on a line pushes whatever follows it on that line to the right, and the
  claim that "nothing is ever anchored there" was an assumption, not a fact. Measured: for
  a document whose DOCTYPE and `<html>` share a line, the root element's column was
  reported 25 too far. The shift cannot be avoided (fitting the declarations inside the
  DOCTYPE's own footprint leaves room for about three entities; skipping the injection
  sends the document back to not parsing at all, silently skipping every check on it), so
  it is corrected instead: the injection reports which line it moved, past which column,
  by how much, and the content-document walk subtracts it from every finding before the
  report is handed out. Matters most to tools that edit by position — a column that is
  right for our parser and wrong for the file on disk is worse than no column.

- **The npm package now ships the commercial-license text.** `@veripublica/epubveri-wasm`
  carried `LICENSE` (the AGPL) but not `LICENSE-COMMERCIAL.md`, so an npm consumer saw the
  AGPL text and no word of the `LicenseRef-veripublica-Commercial` half its own
  `package.json` declares. Two causes, both now handled: `wasm-pack` only collects licenses
  from the crate directory (the files are copied into `epubveri-wasm/`), and npm only
  always-packs a license file whose name is `license` plus a *dotted* extension — the
  hyphen in `LICENSE-COMMERCIAL.md` did not match, so it was dropped. The copy is named
  `LICENSE.COMMERCIAL.md`; the dot is functional, not style (see
  `epubveri-wasm/README.md`). `wasm-pack`'s own `files` list can't be relied on here: it
  writes `package.json` before it copies the licenses, so a clean build never lists them.

## [0.5.10] - 2026-07-17

Doitsu's MobileRead report and epubsana's #23, both of which found rules that
were wrong in ways the corpus scores green. Recall is unchanged (600/607) — no
verdict moves except #23's, which stops inventing 1079 errors and starts
reporting 163 real ones.

### Fixed

- **A deprecated `epub:type` value is no longer also reported as unknown.** `sidebar`
  and `note` drew both `OPF-088` ("is not in the default vocabulary") and `OPF-086b`
  ("is deprecated") — claims that cannot both hold, since knowing a term is deprecated
  means knowing the term. The vocabulary allowlist and the deprecated list lived in
  different modules and had drifted apart; 7 of the 13 deprecated terms were missing
  from the allowlist. Both now live in one module and the "is this a known term?"
  answer derives from both, so the contradiction cannot be stated. An invariant test
  over the whole table then found an eighth case nobody had reported: `figure` was in
  neither list, so `<figure epub:type="figure">` drew a false `OPF-088` too.
  (Reported by Doitsu on the MobileRead forum.)

- **`OPF-087` now states the actual rule, and catches the cases it was missing.** The
  Structural Semantics Vocabulary gives `table`, `table-row`, `table-cell`, `list`,
  `list-item`, `figure` and `aside` an HTML usage context of *"Not Allowed"* — they
  identify escapable/skippable structure on a media overlay's `seq`/`par` and mean
  nothing on an HTML element. epubveri instead read this as *"the value restates the
  semantic of its host element"* (`ol` + `list`, `table` + `table`, …), which agreed
  with epubcheck on every count of its own test fixture — that fixture only ever pairs
  each term with its matching element — but is not the rule: `<div epub:type="list">`
  went unreported entirely. (Reported by Doitsu on the MobileRead forum.)

  Corpus recall is unchanged (600/607) for both, and cannot move: it checks that the
  expected ID was reported, not that nothing extra was, so a spurious usage-level
  message is invisible to it — and its one `OPF-087` fixture is exactly the case where
  the wrong rule and the right one agree.

- **`CSS-007` now says what is actually wrong, and where.** It read *"font 'X' is a
  foreign resource, exempt from requiring a fallback"* — which describes the rule that
  does *not* fire (fonts never need a fallback), buries the one that does, and reads as
  a report of a non-problem. It now names the offending media type (e.g. the
  widespread-but-never-registered `application/x-font-opentype`) and points at the
  `@font-face` `src` that names the font, rather than at the stylesheet as a whole.
  (Reported by Doitsu on the MobileRead forum.)

- **`CSS-029` now points at the stylesheet the class name is written in**, and fires
  once per place it is written. It pointed at the content document that merely *links*
  that stylesheet — a file the class name does not appear in — and repeated itself once
  per linking document. (Reported by Doitsu on the MobileRead forum.)

- **CSS findings inside an inline `<style>` now report the line in the document.** They
  reported the line within the *extracted style text* against the document's path — a
  `direction` property on line 7 of a content document came out as line 3, where the
  reader finds `<head>`. One root cause behind every CSS rule (`CSS-001`, `-008`,
  `-019`, `-007`, `-028`); a linked stylesheet was never affected, since its offsets
  are file offsets. Where the style text isn't a verbatim slice of the document (a
  CDATA section, several text nodes, expanded entities) no offset can be mapped, so the
  finding falls back to the `<style>` element's own position rather than a confidently
  wrong line. (Found while fixing the above.)

### Added

- **`OPF-086b` now names what to use instead of a deprecated `epub:type`** — e.g.
  `sidebar` → a bare HTML `aside` element, `note` → the `footnote` semantic, `warning` →
  the `notice` semantic. The EPUB SSV names a replacement for 5 of its 13 deprecated
  terms; the other 8 say only that they are deprecated, rather than inventing advice.
  (Prompted by Doitsu on the MobileRead forum.)

- **`CSS-028`** (usage): notes each `@font-face` declaration, as real epubcheck does, so
  a reader comparing the two outputs isn't left wondering which tool missed an embedded
  font.

- **EPUB 2 content documents whose DOCTYPE declares the XHTML entities now parse
  (`&nbsp;` and friends).** An EPUB 2 content document references an external DTD
  (XHTML 1.1 / OEB 1.2) that declares the standard HTML named entities, but the
  parser never fetches an external DTD — so `&nbsp;`, the single most ordinary
  thing in a real EPUB 2, failed the parse as an unknown entity. Nothing is
  fetched now either: the entity set is fixed and known, so the referenced ones
  are declared inline before parsing (positions are unaffected — no line shifts).
  Measured on a real 171-book shelf, this affected **690 of 7207 content documents
  (10%), across 48 of 171 books — every one of them valid**. Two things followed
  from it, and both are fixed:
  - **1079 invented `RSC-012` errors** (86% of all `RSC-012` on that shelf, across
    31 books): an unparseable document's id map was built with
    `unwrap_or_default()`, turning *"I could not read this"* into *"this has no
    ids"*, so every fragment pointing into it was reported undefined — against ids
    that were plainly there. "I could not check" and "I checked, and it's absent"
    are now distinct, and only the latter reports.
  - **163 real findings that were never reported** (157 of them `RSC-005`
    `empty_title`): a document that fails to parse has *every* check on it
    silently skipped, so the book validates clean.

  This was the seam between two changes that were each right on their own: #12 made
  a parse failure report `RSC-016` but deliberately let the entity scan own
  entity-reference failures, and 0.5.8 (correctly) stopped that scan reporting
  DTD-declared entities in EPUB 2. Each deferred to the other, so nothing reported
  it — reopening the exact class #12 set out to close, this time silently.
  Reporting these documents as malformed would have been the wrong fix: they are
  valid, and it would have resurrected the false positive 0.5.8 removed.
  (Reported by epubsana, with measurements, in #23.)

  Corpus recall is unchanged (600/607): epubcheck's own corpus is mostly EPUB 3 and
  contains no document of this shape — which is why this survived to 0.5.9.

## [0.5.9] - 2026-07-16

Two more MobileRead forum fixes: an EPUB 2 false positive on the content-type
`<meta>`, and a better source location for `RSC-011`. Corpus recall is unchanged
(600/607) — neither changes a valid/invalid verdict.

### Fixed

- **EPUB 2 content documents are no longer flagged for a valid `<meta http-equiv="Content-Type">`.**
  The rule requiring the `content` attribute to be exactly `text/html; charset=utf-8`
  is an HTML5 (encoding-declaration-state) rule, so it applies to EPUB 3 only. EPUB 2
  content is XHTML 1.1, served as `application/xhtml+xml`, where
  `content="application/xhtml+xml; charset=utf-8"` is the correct form; epubcheck never
  flags it there. It was firing for EPUB 2 too (`RSC-005`) — a false positive. Both
  encoding-declaration checks are now gated to EPUB 3, and a duplicate copy of one of
  them (which also double-reported on EPUB 3) was removed. (Reported by Doitsu on the
  MobileRead forum. Same class as the EPUB-3-rule-leaking-into-EPUB-2 defect in #9.)

### Changed

- **`RSC-011` ("hyperlinked but not listed in the spine") now points at the source link.**
  It used to anchor at the OPF package root (`content.opf:2:1`) because it only knew the
  resolved target; it now anchors at the `<a>` element that creates the hyperlink — the
  right file, its `line:column`, and (in JSON) a `data.element_path` — matching where
  epubcheck locates it. Verdict is unchanged. (Reported by Doitsu on the MobileRead forum.)

## [0.5.8] - 2026-07-15

Two fixes from MobileRead forum reports: a fatal false positive on EPUB 2 named
character entities, and clearer `RSC-005` content-model messages. Corpus recall
is unchanged (600/607) — neither changes a valid/invalid verdict.

### Changed

- **`RSC-005` content-model messages now name the offending element or attribute.**
  A schema violation used to read as a blanket "content document does not conform
  to the EPUB XHTML content-model schema"; it now says *what* is wrong — e.g.
  `element "p" is not allowed here`, `character data is not allowed in element
  "ol"`, `element "x" is missing a required attribute`, `element "x" has
  incomplete content`, or `attribute "y" is not allowed here` — in the style of
  epubcheck's own RSC-005 wording. The offending name is also surfaced as a
  structured `data.params` entry alongside the existing `data.element_path`, so
  the detail is visible in the plain CLI output, not only in the JSON envelope.
  (Reported by Doitsu on the MobileRead forum. Naming the *expected* element as
  well — epubcheck's "…; expected element "li"" — remains future work.)

### Fixed

- **EPUB 2 named character entities (`&nbsp;`, `&eacute;`, `&copy;`, …) no longer
  raise a spurious `FATAL RSC-016`.** An EPUB 2 XHTML content document pulls the
  full set of standard HTML named entities in through its external DTD (XHTML 1.1
  or OEB 1.2), referenced by the DOCTYPE; because the underlying XML parser does
  not resolve external DTDs, every such reference was being reported as an
  undeclared entity — a fatal false positive epubcheck never emits, and a painful
  one since `&nbsp;` is ubiquitous (especially in French ebooks and `<p>&nbsp;</p>`
  spacing). These references are now accepted when the document carries a
  recognized EPUB 2 XHTML/OEB DOCTYPE. Genuinely undeclared entities still fail,
  and EPUB 3 is unchanged (it requires numeric references). (Reported by Doitsu,
  confirmed by KevinH, on the MobileRead forum.)

## [0.5.7] - 2026-07-15

Two content-model reporting improvements, both grounded in forum feedback.
Corpus recall is unchanged (600/607, 1.1% false positives) — these change
*what* and *how much* is reported, not the valid/invalid verdict.

### Added

- **Bare text directly in `<body>` is now flagged in EPUB 2.** XHTML 1.1's `body`
  content model is block-level only, so loose text there is a content-model error
  (`RSC-005`) — one per text run, with a real `line:column`. epubcheck reports
  this; epubveri was silently missing it. (EPUB 3, whose HTML5 body allows flow
  content including text, is unaffected.) The unambiguous EPUB 2 half of the
  bare-text discussion on the MobileRead forum.

### Changed

- **Content-model (`RSC-005`) failures now report every offending node, not just
  the first.** A list like `<ol><p>…</p><p>…</p></ol>` used to draw one finding;
  it now reports each misplaced child, each with its own `line:column` and element
  path — matching epubcheck's per-node output. (Reported by Doitsu on the
  MobileRead forum. A valid-but-empty list is still fine, so only the misplaced
  children are flagged, not the list element itself.)

## [0.5.6] - 2026-07-15

Sharpens the machine-readable locations added in 0.5.5: schema (`RSC-005`)
content-model findings now point at the exact offending node, and the
`element_path` form is corrected so it actually resolves in the XPath engine most
consumers use. Both build on 0.5.5's `data.element_path`; corpus recall is
unchanged (600/607, 1.1% false positives).

### Added

- **`RSC-005` content-model findings now carry a real `line:column` and
  `element_path`.** The RELAX NG engine reports *which* node collapsed the
  content model, so an OPF or XHTML schema violation points at the offending
  element — or the offending **attribute** (`…/@name`) when the violation is
  attribute-level — instead of anchoring the whole document at its root.
  ([issue #17](https://github.com/veripublica/epubveri/issues/17), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **`data.element_path` now binds every namespaced name to a non-empty prefix.**
  The 0.5.5 form left default-namespaced names bare and recorded the URI under an
  empty-string key, which is not resolvable in libxml2 / `lxml` (XPath 1.0 has no
  default namespace). Each namespace URI now gets a bound prefix — a readable
  well-known one for the common EPUB namespaces (`opf`, `dc`, `h` for XHTML,
  `svg`, …) or a generated `ns…` — so a path resolves directly with
  `root.xpath(path, namespaces=data["namespaces"])`.
  ([issue #18](https://github.com/veripublica/epubveri/issues/18), reported by
  Jens Tröger.)

## [0.5.5] - 2026-07-15

Adds a **machine-resolvable node path** to JSON findings, so an automated
consumer (an editor plugin, or a pipeline like Bookalope's) can jump straight to
the offending node instead of re-deriving it from a line/column — plus a
real-world false-positive fix. Both are additive: exact-ID recall against the
epubcheck corpus is unchanged (600/607, 1.1% false positives).

### Added

- **`data.element_path` (with `data.namespaces`) on node-anchored findings.** A
  rooted, XPath-style path with 1-based sibling indices — e.g.
  `/package[1]/spine[1]/itemref[2]`, or, when the finding is about a specific
  attribute, `…/dc:contributor[1]/@opf:role`. Names carry the source prefix as
  authored (a default-namespaced element stays bare); because EPUB documents are
  always namespaced and XPath 1.0 has no default-namespace concept, a
  `namespaces` prefix→URI map (the default namespace under the `""` key) travels
  alongside so a strict engine can resolve the path. Emitted across the
  node-anchored OPF and content-document checks, and — where a finding is about
  an attribute — pinning it directly (`@href`, `@prefix`, `@epub:prefix`, …).
  This lives in the tool-owned `data` slot, so it is purely additive: a consumer
  that ignores the field sees unchanged output.
  ([issue #18](https://github.com/veripublica/epubveri/issues/18), requested by
  Jens Tröger, mirroring the upstream ask on epubcheck.)

### Fixed

- **No more false `RSC-005` on a navigation-document index landmark.** A nav link
  like `<a epub:type="index" href="index.xhtml">Index</a>` was wrongly treated as
  an index *structure* and required to contain an `index-entry-list`. Matching
  epubcheck, the index content-model check now runs only on documents *declared*
  as an index (a manifest `properties="index"` item, a document linked from an
  index `<collection>`, or `dc:type="index"`), never on a document that merely
  *contains* an `epub:type="index"` element. A document actually declared an
  index is still validated.
  ([issue #19](https://github.com/veripublica/epubveri/issues/19), reported by
  Doitsu on the MobileRead forum.)

## [0.5.4] - 2026-07-15

A **foundation refresh**: the toolchain baseline and both behaviour-bearing
dependencies (the ZIP reader and the XML parser) move to current versions, all
verified behaviour-neutral against the full epubcheck corpus (600/607 exact-ID
recall and the 1.1% false-positive rate are unchanged, byte for byte) — plus one
real-world false-positive fix. Shipping these alone, before the next feature
work, so any field report can be attributed cleanly.

### Fixed

- **`PKG-025` no longer flags ordinary metadata files in `META-INF/`.** Files
  like Apple's `com.apple.ibooks.display-options.xml` or calibre's bookmark
  files drew an Error ("publication resource stored inside META-INF") and
  wrongly invalidated common real-world books. Per the OCF spec — and confirmed
  against epubcheck's own test fixture — the error is only for a
  **manifest-declared** resource stored in META-INF (e.g.
  `<item href="../META-INF/image.jpeg">`); undeclared container-level metadata
  is permitted and now stays silent. The declared case still errors.
  ([issue #16](https://github.com/veripublica/epubveri/issues/16), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **Minimum supported Rust version is now 1.88** (declared via `rust-version`,
  so older toolchains get a clear error — and a modern Cargo resolver simply
  keeps them on 0.5.3 rather than breaking). Raised by the `zip` upgrade below;
  1.88 is also the stabilization floor of let-chains, which the codebase now
  uses throughout.
- **`zip` 2.4.2 → 8.6.0** — the ZIP reader an EPUB validator feeds on. This
  buys six majors of reader robustness accumulated upstream (malformed
  EOCD/central-directory detection, panic-safety on malformed input) and aligns
  the whole veripublica family on one `zip` major, so tools embedding the
  `epubveri` crate don't compile two copies. Verified behaviour-neutral:
  malformed-archive verdicts (`PKG-003`/`PKG-004`/`PKG-008`), exit codes, and
  the corpus are unchanged. Two notes: `PKG-008`'s free-text message now embeds
  the new zip version's error wording (the `id`/`rule`/`params` machine contract
  is untouched), and the crate remains pure Rust (the deflate backend is now
  `zlib-rs`; no C dependencies — verified).
- **`roxmltree` 0.20 → 0.21** — the XML parser under every document epubveri
  reads. 0.21 changed bare-string attribute lookup to match by local name
  (ignoring the namespace), which would silently confuse e.g. `lang` with
  `xml:lang`; epubveri instead pins the intended semantics explicitly — every
  namespace-less attribute access goes through a new internal accessor that is
  version-independent **by construction**, verified neutral on 0.20 first and
  then on 0.21 (the full corpus, including the scenarios that caught the
  difference, is identical).
- **The codebase moved to Rust edition 2024**, and ~90 nested `if let` sites
  were collapsed into let-chains (net −95 lines) — internal only; no
  user-facing behaviour, CLI, or JSON change.

## [0.5.3] - 2026-07-13

### Added

- **Deprecated metadata `<link>` relationship keywords are now flagged.** The
  legacy per-format record keywords (`marc21xml-record`, `mods-record`,
  `onix-record`, `xmp-record`) — superseded by the generic `record` keyword with
  a `properties` attribute — and `xml-signature` now draw a warning-level
  `OPF-086`, matching epubcheck (EPUB 3 §D.4.1).

### Changed

- **Library: `epubveri::envelope` (the `--format json` types) is now generic
  over its two tool-owned slots** — the per-input `summary` and per-item `data`
  — so the whole veripublica tool family can build one envelope shape from these
  reference types. epubveri's own types stay the defaults and `Envelope::new`
  keeps its signature, so existing callers are unaffected and the JSON epubveri
  emits is byte-for-byte unchanged. A library-only addition: the CLI, its output,
  and the WASM binding are untouched.

## [0.5.2] - 2026-07-12

### Fixed

- **Malformed content documents are no longer silently accepted.** A content
  document that was not well-formed XML — for example an unclosed `<p>` — was
  skipped without a word, so the book validated clean (a false negative). It is
  now reported as a fatal `RSC-016` at the exact line and column, the same way a
  malformed package document already was. Undeclared/malformed named-entity
  references (e.g. `&nbsp;` with no declaration) keep their existing single
  `RSC-016` and are not double-reported.
  ([issue #12](https://github.com/veripublica/epubveri/issues/12), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **A deprecated `epub:type` value is now reported as usage-level `OPF-086b`**
  (previously info-level `OPF-086`), matching epubcheck — which distinguishes
  the usage-level `OPF-086b` for a deprecated semantic from the warning-level
  `OPF-086` it uses for deprecated rendition/viewport properties. The set of
  deprecated values and the `endnote`-inside-`endnotes` exemption are unchanged.

## [0.5.1] - 2026-07-12

### Fixed

- **Two EPUB 3-only metadata rules no longer fire on EPUB 2 books.** An EPUB 2
  package with more than one `dc:date` — the common creation/modification pair
  that tools like Sigil and Calibre write — was wrongly reported `RSC-005`
  *"element 'dc:date' not allowed here (only one dc:date element is allowed)"*
  and shown INVALID; and a legacy OpenType font drew a spurious `OPF-090`
  *"non-preferred Core Media Type"*. Both are EPUB 3 concepts (EPUB 2
  legitimately carries several `dc:date` elements distinguished by `opf:event`,
  and Core Media Types are an EPUB 3 notion), so they are now scoped to EPUB 3
  — an EPUB 2 book validates exactly as epubcheck does.
  ([issue #9](https://github.com/veripublica/epubveri/issues/9), reported on the
  Sigil PageEdit User Guide.)

## [0.5.0] - 2026-07-11

This release adopts the **[veripublica CLI convention
v0.4](https://github.com/veripublica/conventions)** — the shared command-line
and machine-output contract the tool family follows. The invocation changes, so
this is a breaking release (a minor bump, per pre-1.0 SemVer). epubveri now
states *"Conforms to veripublica conventions v0.4"* in its `--help`.
([tracking issue #8](https://github.com/veripublica/epubveri/issues/8).)

### Changed

- **The input is now passed with `-i`/`--input`, never as a positional path.**
  `epubveri book.epub` becomes `epubveri -i book.epub`. A bare path is now a
  usage error that shows the corrected form. **Repeat `-i` to validate several
  books in one run** — each is reported, and the exit code is the worst across
  them.
- **Unrecognized input fails loudly (exit `2`) instead of being silently
  misread.** An unknown flag, an out-of-set `--format`/`--profile` value, or the
  same single-valued option given twice now stops with a short message pointing
  at `--help`, rather than being swallowed as a file name or falling back to a
  default.
- **Findings now carry epubcheck's five severity levels** —
  `fatal | error | warning | info | usage` — instead of folding fatals into
  errors and usage-level notes into info. Only `error` and `fatal` make a book
  invalid; `warning`/`info`/`usage` are reported but do not. Fifteen conditions
  (e.g. a missing or unreadable OPF, a corrupt container, malformed XML) are now
  `fatal`, and twenty advisory notes (e.g. `OPF-090`, `OPF-003`, `RSC-025`) are
  now `usage`, matching epubcheck's own classification.
- **Exit codes are clarified.** A broken-but-readable file — even one that isn't
  a valid ZIP — now gets a *verdict* (a `fatal` finding, exit `1`); exit `2` is
  reserved for the tool being unable to run or read an input at all.

### Added

- **`--format json`** — the shared veripublica machine envelope
  ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)):
  one JSON object with the tool, version, convention key, aggregate status, and
  one self-contained object per input carrying its findings. The
  `epubveri-wasm` binding returns the same per-input shape, and the browser demo
  can **download it as `<book-name>.epubveri.json`** — byte-for-byte what the CLI
  emits. ([issue #11](https://github.com/veripublica/epubveri/issues/11).)
- **`-V`/`--version` carries git build metadata** — `0.5.0+<short-hash>`, with
  `.dirty` when built from a modified tree, falling back silently to the plain
  version when there is no checkout (a crates.io build). The CLI's `-V`, the wasm
  `version()`, and the demo footer all print this one string, so a bug report
  from any surface pins the exact source. ([conventions issue #20].)
- **`--help` gained an EXAMPLES section, an EXIT CODES summary, and the
  conformance line**; usage errors now point the reader at `--help`.

### Fixed

- **`epubveri -v` (and any unknown flag) now reports a real usage error** —
  `error: unexpected option '-v' (see --help)` — instead of the misleading
  `cannot read -v`. ([issue #7](https://github.com/veripublica/epubveri/issues/7).)

### Demo

- The in-browser WASM demo adopted the shared **family-web template v2**,
  which fixes two live accessibility defects (a keyboard-unreachable drop zone
  and a verdict chip failing WCAG AA contrast) and colours all five severity
  levels. ([issue #10](https://github.com/veripublica/epubveri/issues/10).)

## [0.4.4] - 2026-07-08

### Added

- **CSS findings now report an exact line/column**, closing epubveri's last
  position gap — CSS was the only finding family that could carry just a
  file name. Every CSS finding (`CSS-001`, `CSS-002`, `CSS-008`, `CSS-019`,
  and the `RSC-001`/`RSC-007`/`RSC-008`/`RSC-030` resource references found
  inside stylesheets) now points at the offending token, e.g. `CSS-001: use
  of the 'direction' property is not recommended [OEBPS/style.css:3:3]`.
  Built on [styloria](https://github.com/veripublica/styloria) 0.2's new
  source-span parse tree. ([issue
  #1](https://github.com/veripublica/epubveri/issues/1), requested by Kevin
  Hendricks for Sigil integration.)

### Fixed

- **Non-linear content reachability (`OPF-096`) now matches epubcheck's
  self-link rule.** A `linear="no"` spine item is reachable if any hyperlink
  points at it — *including a link the document makes to itself* (a nav's
  landmarks self-link such as `href="nav.xhtml"`, or a fragment-only
  `href="#…"`), which is how epubcheck has always treated it. The 0.4.2
  release instead exempted the toc nav categorically; that was
  over-correction — it wrongly silenced a non-linear nav that nothing links
  to. The nav is no longer special-cased: a self-linking nav still passes,
  and a genuinely unreachable one is flagged, exactly as epubcheck does.
  ([issue #1](https://github.com/veripublica/epubveri/issues/1), thanks to
  Kevin Hendricks for the pinpointed behavior.)

## [0.4.3] - 2026-07-08

### Fixed

- **Media-query (and other conditional-group) stylesheets were wrongly
  flooded with `CSS-008` "CSS syntax error"** ([issue
  #5](https://github.com/veripublica/epubveri/issues/5), reported by DNSB
  against a Vellum-generated book). The block of a conditional-group
  at-rule (`@media`, `@supports`, `@container`, …) holds nested *rules*,
  not declarations; each nested rule's selector was being mis-read as a
  malformed declaration, so a stylesheet fired one false `CSS-008` per
  `@media` block. Such blocks are now walked as rule lists — the
  declarations inside the nested rules are still checked, and a genuinely
  malformed declaration (or an unclosed qualified rule) is still reported.

### Changed

- **Malformed-XML findings now report the exact line/column where parsing
  failed.** A not-well-formed OPF package document (`RSC-016`) or
  `META-INF` container/encryption/signatures file (`RSC-005`) previously
  reported only the file name; each now points at the precise spot the XML
  parser gave up, which makes these findings directly actionable for a
  producer fixing them programmatically. (Position coverage across all
  finding call sites is now ~82%; the remainder — CSS checks and
  whole-container/ZIP-structure checks — have no single line to point at.)

## [0.4.2] - 2026-07-08

### Fixed

- **`dc:date` full timestamps were wrongly rejected** ([issue
  #4](https://github.com/veripublica/epubveri/issues/4), reported by
  JSWolf). A value like `2025-04-24T17:00:00Z` — a valid W3C-DTF (ISO 8601)
  timestamp — was flagged `OPF-054` ("doesn't conform to ISO 8601"). The
  date validator only accepted the date-only forms (`YYYY`, `YYYY-MM`,
  `YYYY-MM-DD`); it now also accepts a full timestamp (`T`, a time, and a
  `Z` or `±hh:mm` timezone designator).
- **A non-linear navigation document was wrongly flagged as unreachable**
  ([issue #5](https://github.com/veripublica/epubveri/issues/5), reported
  by DNSB). A nav (toc) document placed in the spine as `linear="no"` with
  no hyperlink pointing at it triggered `OPF-096` ("non-linear content is
  not reachable from the reading order"). The navigation document is always
  reachable through the reading system's own navigation controls, so it is
  now exempt from this check (matching epubcheck 5.3). Genuinely-unreachable
  non-linear *content* documents are still reported.

### Changed

- **Schematron-derived findings now carry line/column positions.** These
  were the one documented family that reported only a file path after
  0.2.0's position work; each now points at the exact element the rule
  matched (e.g. an empty `<meta property="">`). Completes the line/column
  coverage requested in [issue
  #1](https://github.com/veripublica/epubveri/issues/1).

## [0.4.1] - 2026-07-07

### Fixed

- The `opf-meta-property-not-empty` Schematron rule (behind `RSC-005`,
  "value of attribute 'property' is invalid (must not be empty)") was
  scoped to `opf:meta` — *every* `<meta>` element — instead of
  `opf:meta[@property]`. This meant any legacy, `property`-less `<meta>`
  (e.g. the extremely common OPF2-style `<meta name="cover"
  content="..."/>`) was wrongly flagged, since an absent `@property`
  normalizes to an empty string too. The corpus's own fixture for this
  rule only ever exercised `property=""` / `property="   "`, so the gap
  wasn't caught by the recall metric. Rescoped to match the rule's actual
  intent: only meta elements that already carry a `property` attribute are
  checked for emptiness.

Thanks to forum user **DNSB** ([MobileRead
thread](https://www.mobileread.com/forums/showthread.php?t=374286)) for
the report, via [issue #1](https://github.com/veripublica/epubveri/issues/1).

## [0.4.0] - 2026-07-06

### Added

- The `rule`/`params` sub-code introduced in 0.3.0 (for `RSC-005` only) is
  now populated at **every message ID with 2 or more call sites** across
  the crate — 36 additional IDs (`RSC-006` through `RSC-033`, `OPF-001`
  through `OPF-092`, `CSS-008`/`CSS-015`, `HTM-004`/`HTM-057`/`HTM-060`,
  `PKG-007`/`008`/`009`/`012`), on top of the `RSC-005` sites already
  done. IDs used at exactly one call site are left as-is — `id` alone is
  already unambiguous there.
- New `Report::push_rule` method (alongside the existing `push`/`push_at`/
  `push_at_pos`/`push_at_rule`/`push_full`) for the handful of sites with
  a `rule`/`params` pair but no `location` at all — a whole-container
  failure (corrupt/empty ZIP) detected before any file is identified.

## [0.3.0] - 2026-07-06

### Added

- Every diagnostic can now carry a stable, semantic **sub-code** (`rule`)
  and the **values interpolated into its message** (`params`), alongside
  the existing epubcheck-compatible `id`. This exists because a single
  `id` — especially `RSC-005`, epubcheck's generic RelaxNG/Schematron
  catch-all — covers many structurally unrelated conditions with only the
  rendered English sentence to tell them apart. `Message` gained
  `rule: Option<&'static str>` (e.g. `"opf.spine.duplicate_itemref"`) and
  `params: Vec<String>`. `rule` is populated at every `RSC-005` call site
  in the crate except a handful where no stable sub-code is derivable yet
  (schematron-derived output, and a few "input didn't parse as XML at
  all" cases) — other message IDs don't have `rule` populated yet and are
  a candidate for the same treatment later.
- Additive: `Report::push_full` (with position) and `Report::push_at_rule`
  (without) sit alongside the existing `push`/`push_at`/`push_at_pos`,
  which are unchanged. The WASM bindings expose the same fields.

## [0.2.1] - 2026-07-06

### Fixed

- `OPF-096` ("non-linear spine content isn't reachable from the reading
  order") is now downgraded to a usage-level `OPF-096b` when the book uses
  scripting anywhere — matching real epubcheck, which allows for script
  adding navigation/hyperlinks dynamically that static analysis can't see.
  Previously always reported as a hard error, which could misfire on a
  legitimate pattern such as a `nav.xhtml` placed in the spine as
  `linear="no"` in a scripted book.

Thanks to forum user **DNSB** ([MobileRead
thread](https://www.mobileread.com/forums/showthread.php?t=374286)) for
finding this. See [issue #3](https://github.com/veripublica/epubveri/issues/3).

## [0.2.0] - 2026-07-06

### Added

- Every diagnostic can now carry an exact source **position** (line and
  column), not just a bare file path. `Message` gained a new
  `position: Option<Position>` field; the CLI's human-readable output now
  shows `path:line:col` when a position is available (`--format ids` is
  unaffected). The WASM bindings expose the same `Position` type.
- This is additive: `Report::push_at_pos` sits alongside the existing
  `push`/`push_at` methods, which are unchanged. Position is populated at
  the large majority of check sites; a documented minority (schematron-
  generated findings, CSS-based checks, ZIP-archive-entry-level checks,
  and a few "input didn't parse at all" cases) have no coherent position
  to report and correctly stay `None`.

### Fixed

- `frontmatter` is a valid EPUB 3 Structural Semantics vocabulary term
  (sibling to `bodymatter`/`backmatter`) but was incorrectly flagged as
  unknown vocabulary.

Thanks to [Kevin Hendricks](https://github.com/kevinhendricks) (author of
the Sigil EPUB editor) for the detailed bug report that prompted both of
these fixes.

## [0.1.0] - 2026-07-04

Initial real release to [crates.io](https://crates.io/crates/epubveri) and
npm (`@veripublica/epubveri-wasm`) — a pure-Rust EPUB validator covering
OCF/OPF/manifest/spine integrity, content-document checks (XHTML, SVG,
MathML, CSS), navigation documents, and the Media Overlays, EDUPUB,
Dictionaries & Glossaries, Indexes, Previews, and Multiple-Renditions
extension specifications. At the time of this release: 98.8% exact
message-ID recall and 98.9% clean-file recall against epubcheck's own test
corpus.
