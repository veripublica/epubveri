# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

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
