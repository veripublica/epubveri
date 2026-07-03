# epubveri — architecture and internals

This document is for people who want to understand (or work on) the
codebase itself: how validation actually flows through the code, what
each module is responsible for, how the custom schema engines work, and
how to test and measure the project. If you just want to *use* epubveri,
see the main [`README.md`](../README.md) instead — this document assumes
you've read that first.

If you're looking for the *reasoning* behind a specific decision (why a
check is scoped the way it is, what real test fixture proved a rule,
what was tried and abandoned) rather than the current shape of the code,
[`CLAUDE.md`](../CLAUDE.md) is the place — it's a dated, increment-by-
increment engineering log of the entire project. This document is a
map of *what exists*; `CLAUDE.md` is the story of *why*.

## The validation pipeline, in order

Feeding a `.epub` file into epubveri walks through the following layers,
each one only reached if the layer before it didn't already fail
outright:

1. **Is it a real ZIP file at all?** (`src/ocf.rs`) — corrupted archives,
   truncated files, or files that are secretly some other format
   entirely (e.g. a JPEG someone renamed to `.epub`) are caught here,
   with a real byte-signature sniff (`src/image.rs`) used to give a more
   specific "this is corrupted" vs. "this is actually a JPEG" message.
2. **OCF container rules** (`src/ocf.rs`) — the `mimetype` entry's exact
   required shape (present, first, uncompressed, no extra ZIP metadata),
   `META-INF/container.xml` pointing at the real package document,
   `META-INF/encryption.xml` and `signatures.xml` if present, file-name
   conformance for every entry in the archive (`src/filename.rs`).
3. **The package document (OPF)** (`src/opf.rs`, by far the largest
   module) — this is where most cross-referencing happens, since it's
   the one place with a view of the *whole* book: metadata completeness,
   manifest/spine integrity, declared media-types vs. actual file
   content, the EPUB 3 navigation document requirement, and dispatching
   into every other check that needs whole-book context (which content
   documents reference which resources, which extension specs are in
   play, and so on).
4. **Content documents** — the actual XHTML chapter files, plus embedded
   SVG and MathML, each validated against their own content model
   (`src/rng` for XHTML's grammar, `src/svg.rs`, `src/mathml.rs`), plus a
   long tail of narrower checks (`src/htm.rs`, `src/navdoc.rs`,
   `src/ncx.rs`, `src/layout.rs`, `src/url.rs`, `src/css.rs` via the
   sibling `styloria` crate).
5. **Optional extension specifications**, each triggered only when a
   book actually declares it's using that feature: Media Overlays
   (`src/smil.rs`), EDUPUB (`src/edupub.rs`), Dictionaries & Glossaries
   (`src/dict.rs`), Indexes (`src/indexes.rs`), Previews
   (`src/previews.rs`), Region-Based Navigation (`src/regionnav.rs`),
   Multiple Renditions (`src/renditions.rs`).

Every finding, from every layer, accumulates into one flat
[`Report`](../src/report.rs) — a list of `Message`s, each with an
epubcheck-compatible ID (e.g. `RSC-005`), a severity (`Error` / `Warning`
/ `Info`), a human-readable message, and an optional location. There's
no early-exit-on-first-error; a single validation run collects
everything it can find.

## Workspace layout

This repository is a **Cargo workspace** with two members:

- **`epubveri`** (root `Cargo.toml`, `src/`) — the actual product: the
  library (`lib.rs`, used as a dependency by other Rust projects) and
  the thin CLI binary (`main.rs`). This is the crate that will
  eventually be published to crates.io, so its own dependency list is
  kept deliberately minimal (only what the validator itself needs at
  runtime: `zip`, `roxmltree`, `unicode-normalization`, and the sibling
  `styloria` CSS-parser crate).
- **`harness/`** (`epubveri-harness` crate) — a development-only tool
  that measures epubveri's real-world accuracy against epubcheck's own
  test suite (see "Testing and measurement" below). It depends on
  `epubveri` via a plain path dependency (`epubveri = { path = ".." }`),
  plus `regex` and `zip` for its own purposes. It lives in a separate
  workspace member specifically so a dev-only dependency like `regex`
  never has to appear in the *published* crate's own `Cargo.toml` — a
  library consumer pulling in `epubveri` from crates.io should never see
  `regex` in their dependency tree at all.

There's also a small standalone binary, **`src/bin/spike.rs`**, which
*is* part of the main `epubveri` crate (Cargo auto-discovers any
`src/bin/*.rs` file as an extra binary target) — it needs no dependency
the main crate doesn't already have, so it didn't need its own workspace
member. See "Testing and measurement" for what it does.

## Module reference

### Container & package layer

- **`ocf.rs`** — the OCF (Open Container Format) layer: opening the ZIP,
  the `mimetype` rules, locating the package document via
  `META-INF/container.xml`, `encryption.xml`/`signatures.xml`. Also
  home to `parse_xml`, the shared `roxmltree`-based XML parsing helper
  every other module uses (handles `DOCTYPE`-bearing documents, which
  `roxmltree` rejects by default as an extra security precaution).
- **`opf.rs`** — the package document: metadata, manifest, spine,
  declared media-types, the navigation-document requirement, broken
  internal references. This is the largest file in the project by a
  wide margin, because it's the only module with a full view of the
  book (every other content-document-level check gets called *from*
  here, once per content document, with the manifest/spine context
  those checks need to resolve cross-references).
- **`filename.rs`** — OCF file-name conformance (forbidden characters,
  trailing full stops, non-ASCII usage) and case-fold/Unicode-
  normalization duplicate-name detection.
- **`image.rs`** — byte-level magic-number sniffing for the raster Core
  Media Types (JPEG/PNG/GIF/WebP), used to cross-check a declared
  media-type against what a file's bytes actually are.
- **`cmt.rs`** — the EPUB 3 Core Media Types list (§3.2), shared by
  several checks that need to know "is this a type EPUB natively
  supports, or a foreign one that needs a fallback."
- **`foreign.rs`** — the foreign-resource/fallback rules (§3.3/§3.5): a
  non-Core-Media-Type resource needs a working fallback chain, with
  several real, narrow exemptions (`video/*`, `<link>`/`<track>`
  targets, `<picture>`'s own stricter rule).

### Schema engines

Two general-purpose validation engines were built from scratch for this
project, because the checks epubcheck itself expresses via RelaxNG and
Schematron needed *some* real engine behind them, and a from-scratch,
narrowly-scoped implementation was more maintainable than a full spec
implementation would have been.

- **`rng/`** — a pure-Rust, derivative-based RELAX NG validation engine
  (implementing the algorithm from James Clark's "An algorithm for
  RELAX NG validation"). See `rng/pattern.rs` (the pattern data model,
  with hash-consing — every structurally-identical `Pattern` is
  interned so `Rc::ptr_eq` becomes a reliable identity check; this
  turned out to be load-bearing, not just an optimization — without it,
  ordinary flowing prose triggers exponential blowup after ~15-20
  sibling elements), `rng/derive.rs` (the derivative algorithm itself),
  `rng/load.rs` (parses the RELAX NG *XML* syntax into the in-memory
  grammar), `rng/datatype.rs` (XSD datatype lexical validation). Used
  to validate the package document against `schemas/package.rng` and
  content documents against `schemas/xhtml.rng`.
- **`xpath/`** + **`schematron/`** — a real (if deliberately scoped-down)
  XPath 1.0 engine and a Schematron rule executor built on top of it.
  `xpath/lexer.rs` → `xpath/parser.rs` (recursive-descent, standard
  XPath 1.0 precedence) → `xpath/eval.rs` (the evaluator — see its own
  doc comment for a genuine XPath gotcha it gets right: an unprefixed
  name in a node-test means the *null* namespace, never "whatever the
  document's default namespace is"). Deliberately excludes
  `matches()`/`tokenize()` (would need a regex engine) and
  `resolve-uri()`. `schematron/mod.rs` loads and runs
  `<pattern>`/`<rule>`/`<assert>`/`<report>` documents against it — see
  its own doc comment for why Schematron's `context` attribute needs a
  genuinely different resolution strategy (a backwards-walking "match
  pattern" check) than an ordinary forward XPath query.

Both engines were split into their own standalone repositories at one
point (`styloria` for a related CSS parser, `schemora` for the
XPath/Schematron engine) under the theory that they're general-purpose
and might be useful to other projects. `styloria` earned that split — it
has a real, active dependency relationship with epubveri. `schemora`
didn't: epubveri never actually switched to depending on it, so it sat
as an orphaned fork with no real user, and was archived; the XPath/
Schematron code you see in `xpath/`/`schematron/` here is the one true
canonical copy going forward. The lesson (recorded in `CLAUDE.md`): only
split code into its own repo for a *real, concrete* second consumer, not
a theoretical one.

### Content-document checks

- **`htm.rs`** — XML declaration / DOCTYPE / entity / encoding checks,
  plus a handful of attribute- and element-level checks that don't fit
  neatly elsewhere. Includes small hand-written raw-text scanners for
  things `roxmltree` has no structured API for at all (the XML
  declaration's version, DOCTYPE-declared entities, a document's
  original byte-level encoding).
- **`svg.rs`** — SVG content-model checks: `foreignObject` (reuses the
  XHTML RELAX NG grammar via a wrap-and-reparse trick — no separate SVG
  content model needed for that one case), `title` (a much more
  permissive content model, confirmed via real fixtures), a generic
  element-vocabulary check, `epub:type` placement rules, id validity/
  uniqueness, and SVG link accessibility.
- **`mathml.rs`** — MathML content-model checks (Presentation vs.
  Content MathML, the `annotation-xml` encoding-equivalence rules).
- **`navdoc.rs`** — the EPUB 3 navigation document's own content model
  (the `toc`/`page-list`/`landmarks` nav types and their differing
  content rules).
- **`ncx.rs`** — the EPUB 2 NCX (legacy table-of-contents) format.
- **`layout.rs`** — fixed-layout-specific checks: the `<meta
  name="viewport">` mini-grammar and the SVG `viewBox` requirement.
- **`url.rs`** — a small hand-written absolute-URL syntax validator
  (scheme/host/character-set rules), scoped to exactly what the real
  test corpus exercises rather than a full URL-spec implementation.
- **`css.rs`** — CSS checks, built on the sibling `styloria` crate's
  tokenizer/parser (which has no selector or property-value grammar
  yet — everything here works at the token/component-value level:
  malformed declarations, empty `@font-face` rules, `url()` resource
  resolution, encoding checks).

### Optional extension specifications

Each of these is triggered by a book actually declaring it uses the
corresponding feature (typically via `dc:type`, a manifest `properties`
value, or a `<collection>` element) — a book that doesn't use a feature
never touches the corresponding module at all.

- **`smil.rs`** — Media Overlays (synchronized text/audio playback).
  Deliberately scoped to the actual EPUB Media Overlays profile of
  SMIL, not general SMIL 3.0 (which has ~40-50 elements across a dozen
  modules epubveri has no reason to support).
- **`edupub.rs`** — the EDUPUB profile (educational publications):
  teacher's editions, accessibility metadata requirements, HTML5
  microdata usage.
- **`dict.rs`** — EPUB Dictionaries & Glossaries: dictionary entry
  content models, Search Key Map documents.
- **`indexes.rs`** — EPUB Indexes: index content models and collection
  structure.
- **`previews.rs`** — EPUB Previews: preview-publication identification
  and preview collections.
- **`regionnav.rs`** — Region-Based Navigation (an accessibility-
  oriented navigation extension).
- **`renditions.rs`** — Multiple-Rendition Publications (one book
  packaged as several renditions — e.g. reflowable + fixed-layout —
  selected via `container.xml`).

### Support

- **`report.rs`** — the `Report`/`Message`/`Severity` types every check
  writes into. Deliberately simple: a flat `Vec<Message>`, no early
  termination.
- **`ids.rs`** — every message ID this project emits, as named
  constants, each with a one-line comment describing the condition.
  Reconciled against epubcheck's own real message bundle so that IDs
  match exactly where checks overlap — but message *wording* is always
  our own, never copied.

## The `schemas/` directory

`schemas/package.rng` and `schemas/xhtml.rng` (RELAX NG grammars) and
`schemas/package.sch` (Schematron rules) are all **authored from
scratch for this project** — not derived from, or copied out of,
epubcheck's or W3C's own schema files. This is a deliberate provenance
decision: it keeps the entire codebase's copyright clean for the dual
AGPL/commercial licensing model (see the main README's license section),
and it also means these schemas are *not* a mechanical translation of
epubcheck's — they were built and tuned directly against real corpus
test fixtures (see "Testing and measurement" below), which is also why
they lean permissive in places a stricter schema wouldn't (favoring
"don't false-positive on a valid real-world book" over "reject anything
even slightly unusual").

## Testing and measurement

There are three distinct layers, answering three different questions:

1. **`cargo test`** (170 tests as of this writing) — ordinary Rust unit
   tests scattered across the modules above, each testing one specific
   function or rule in isolation with a small hand-built input. Answers
   "does this one piece of logic do what I think it does."
2. **`cargo run --release --bin spike`** — builds ~25 small, synthetic,
   deliberately-broken EPUB files (one per high-value structural check,
   e.g. "manifest references a file that doesn't exist"), validates
   each one in-process, and reports whether epubveri caught the
   specific problem each fixture was built to expose. Answers "do the
   core mechanisms work at all," as a fast sanity check — it doesn't
   need epubcheck's real corpus (see below) to run, so it's the
   quickest thing to run after a change.
3. **`cargo run --release --bin corpus`** (from the `harness/`
   directory, or `cd harness && cargo run --release --bin corpus`) —
   the real accuracy measurement. epubcheck ships its own test suite as
   Cucumber `.feature` files, each describing one specific test book
   and the exact error/warning it's supposed to produce (or that it
   should be completely clean). This tool parses those `.feature`
   files, builds (or wraps) the corresponding EPUB fixture, runs it
   through epubveri, and reports: what fraction of "this should error"
   cases got the *exact same message ID* epubcheck itself would report,
   and what fraction of "this should be clean" cases stayed clean (no
   false alarms) — broken down by message-ID family (`RSC-`, `OPF-`,
   `HTM-`, ...). This is the headline number quoted in the main README.

   epubcheck's own test corpus isn't redistributed in this
   repository — it's a separate project under a separate license. It's
   expected to be cloned locally into a gitignored `corpus/` directory
   (`corpus/epubcheck/`) before running this tool; the harness will
   tell you if it can't find it.

   Since epubveri only ever validates a *complete* EPUB (a real ZIP
   container), but many of epubcheck's own test fixtures are bare,
   single-file test cases (a lone `.opf`, or a lone `.xhtml` content
   document, exercising epubcheck's own single-file check modes), the
   harness's `wrap.rs` module synthesizes a minimal, otherwise-valid
   book around each bare fixture so it can still be validated as a real
   book. This wrapping is itself a documented source of a few narrow,
   understood scoring exclusions (see `harness/src/corpus.rs`'s own doc
   comments) — a handful of message IDs that are real, correct findings
   on the *synthetic wrapper* rather than the fixture under test, and
   are excluded from scoring for that reason alone.

### A worked example: adding a new check

If you want to see how a check actually gets built in this project, the
pattern (repeated dozens of times across `CLAUDE.md`'s dated history) is
always the same:

1. **Find the real rule.** Read the relevant section of the EPUB
   specification, and — critically — find the corresponding real test
   fixtures in epubcheck's own corpus (both the fixture that should
   trigger the error, *and* at least one fixture that's valid despite
   looking superficially similar). Never guess a rule from the spec
   text alone; the corpus's actual fixtures repeatedly turn out to
   encode narrower or subtly different rules than a first reading of
   the spec would suggest.
2. **Implement it** in the most specific module for the concern (a new
   function in an existing file, or a new file if it's a genuinely new
   area — see the module reference above for precedent).
3. **Verify against the corpus.** Run the `harness`'s corpus tool,
   scoped to the relevant feature file if useful, and confirm the
   target scenario is now hit *and* that no previously-passing "should
   stay clean" fixture regressed (a new check can absolutely introduce
   a false positive on unrelated, previously-fine content — this has
   happened repeatedly and is exactly what this step catches).
4. **Add a unit test** for the specific function, independent of the
   corpus.
5. Only once all of that is green: consider it done, and move on.
