# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

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
