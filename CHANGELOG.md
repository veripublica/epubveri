# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

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
