# epubveri: detail for code agents and maintainers

`README.md` is the short introduction. This file holds the detail and is kept
true at every release. API docs: <https://docs.rs/epubveri>. Past changes:
`CHANGELOG.md`.

Figures below name the versions they were measured on. If you need one for a
claim, run the tool that produces it (see [Measuring](#measuring)).

## Scope

epubveri validates EPUB files: a pure-Rust, JVM-free alternative to
[epubcheck](https://github.com/w3c/epubcheck), W3C's validator, written in Java.
It ships as a library and a CLI (crate `epubveri`) and a WebAssembly package
(`@veripublica/epubveri-wasm`), released together under one version.

It covers the container (OCF), the package document (OPF), content documents
(XHTML, SVG, MathML, CSS, navigation, NCX), and these extensions when a book
declares them: Media Overlays, EDUPUB, Dictionaries and Glossaries, Indexes,
Previews, Fixed Layout, Region-Based Navigation and Multiple Renditions.

Rules the code follows:

- **It reports the same message IDs as epubcheck, in its own wording.**
  Wording, line order and columns can change between releases. IDs and `rule`
  keys are stable.
- **The specification decides the verdict, not epubcheck.** A broken MUST or
  MUST NOT is an error and a broken SHOULD a warning, including where
  epubcheck says nothing; what the specification allows passes, including
  where epubcheck rejects it. Each such finding cites its clause and must be
  true every time it fires.
- **Every departure from epubcheck is listed** under
  [Known differences from epubcheck](#known-differences-from-epubcheck), with
  the clause and the upstream issue. Where the specification is silent,
  epubveri follows epubcheck.
- **`--advisory` holds only what no specification requires.** Those findings
  never change the verdict or the exit code.
- **An advisory must be true every time it fires,** and is worded as an
  observation, not a verdict.
- **The library never filters.** `validate_bytes` returns every finding at
  every severity, and rules that consumers dispatch on can sit below error
  severity. Filtering belongs at display.
- **It parses independently.** It shares no parser with the tools it judges.
- **It has no C dependencies and no JVM.** The code must also build for
  `wasm32-unknown-unknown`.

## Known differences from epubcheck

Each line is a place where epubveri follows the specification and epubcheck
5.4.0 does not, with the clause and the upstream issue.

- **A `nav` element with `aria-label` or `aria-labelledby` passes.** EPUB 3.3
  allows ARIA naming on `nav`; epubcheck 5.4.0 rejects it
  ([w3c/epubcheck#1726](https://github.com/w3c/epubcheck/issues/1726)).
- **Elements and attributes from other namespaces in `container.xml` are
  ignored.** EPUB 3.3 § 4.2.6.3.1 and OCF 2.0.1 § 3.5.1 validate the file
  after removing them; epubcheck 5.4.0 reports them as RSC-005
  ([w3c/epubcheck#1754](https://github.com/w3c/epubcheck/issues/1754)).
- **An `svg` element directly inside an SVG `title` is RSC-005.** EPUB 3.3
  § 6.2.3: "the markup MUST contain only elements declared in the HTML
  namespace". epubcheck 5.4.0 accepts it, as flow content
  ([w3c/epubcheck#1759](https://github.com/w3c/epubcheck/issues/1759)).
- **An `hgroup` in a restricted `nav` is RSC-005.** EPUB 3.3 § 7.3 gives the
  heading as "h1-h6"; epubcheck 5.4.0 also accepts `hgroup`
  ([w3c/epubcheck#1758](https://github.com/w3c/epubcheck/issues/1758)).
- **A `nav` of another type needs no heading.** EPUB 3.3 § 7.3 has "h1-h6
  [0 or 1]" and § 7.4.5 adds nothing; epubcheck 5.4.0 reports RSC-005
  ([w3c/epubcheck#1758](https://github.com/w3c/epubcheck/issues/1758)).
- **`dc-metadata` and `x-metadata` pass in EPUB 2.** OPF 2.0.1 § 2.2 deprecates
  them and keeps them; epubcheck 5.4.0 reports OPF-049
  ([w3c/epubcheck#1761](https://github.com/w3c/epubcheck/issues/1761)).
- **An `img` with no `alt` in EPUB 3 is RSC-005,** with HTML's own exceptions.
  HTML 4.8.4.4.15 and EPUB 3.3 § 6.1.2; epubcheck 5.4.0 is silent
  ([w3c/epubcheck#1760](https://github.com/w3c/epubcheck/issues/1760)).
- **EPUB 2: OPF-092 on a malformed `dc:language`, RSC-005 on an `ncx` with no
  `version`, SPEC-002 and SPEC-003.** OPF 2.0.1 §§ 2.2.12, 2.4.1.2, 2.3.1.1 and
  2.4.1.2; epubcheck 5.4.0 reports none of them
  ([w3c/epubcheck#1761](https://github.com/w3c/epubcheck/issues/1761)).
- **A compressed `mimetype` is PKG-007, an error.** EPUB 3.3 § 4.3.3: "EPUB
  creators MUST NOT compress or encrypt the `mimetype` file". epubcheck 5.4.0
  compares only the inflated contents.
- **A bad `url()` inside another CSS function is reported** (RSC-007, RSC-008,
  RSC-026 or RSC-030, as at the top level), such as `image-set(url(a.png)
  1x)` naming a missing file. CSS reads the url, and EPUB 3.3 asks the same of
  every resource a publication uses. epubcheck 5.4.0 reads only a
  declaration's top-level components.
- **EPUB 2 content documents are held to OPS 2.0.1's restrictions beyond
  XHTML 1.1:** a `noscript` before every `script` (§ 2.3.7), a `style` type
  other than `text/css` or `text/x-oeb1-css` (§ 2.3.8), `align="char"`
  (§ 2.3.9), and SVG animation, `script` and event attributes (§ 2.5.1) are
  RSC-005, and an `object` type that names another media type than the
  manifest is OPF-013 as an error (§ 2.3.6). epubcheck 5.4.0 accepts the
  first four and gives OPF-013 as a warning.
- **UTF-16 draws no finding in an EPUB 2 book.** OCF 2.0.1 § 1.3: "All XML
  files MUST be encoded in UTF-8 or UTF-16", and OPF 2.0.1 § 1.4.1.1 and
  OPS 2.0.1 § 1.4.1.2 say it again. epubcheck 5.4.0 grades encoding without
  the version, so a UTF-16 XHTML document is HTM-058 there and any other
  UTF-16 XML file RSC-027. The same holds for a stylesheet with a byte order
  mark (OPS 2.0.1 § 1.4.1.5), which epubcheck 5.4.0 gives CSS-003
  ([w3c/epubcheck#1772](https://github.com/w3c/epubcheck/issues/1772)).
- **A fixed-layout viewport `width` or `height` of `0` is HTM-057.** EPUB
  3.3 § 8.2.2.6: each "MUST have as its value a positive number [css2]".
  epubcheck 5.4.0's pattern accepts zero
  ([w3c/epubcheck#1762](https://github.com/w3c/epubcheck/issues/1762)).
- **Viewport values `+1200` and `.5` pass.** § 8.2.2.6 cites CSS 2's
  `<number>`, which allows a sign and a leading dot (CSS 2 § 4.3.1);
  epubcheck 5.4.0 reports HTM-057 for both
  ([w3c/epubcheck#1762](https://github.com/w3c/epubcheck/issues/1762)).
- **A viewport property with no value, followed by a space, passes**
  (`initial-scale 1, width=1200, height=600`). EPUB 3.3 appendix F's
  grammar reads it as two properties without values, which the appendix
  allows; epubcheck 5.4.0 reports HTM-047
  ([w3c/epubcheck#1763](https://github.com/w3c/epubcheck/issues/1763)).
- **An `@charset` label is read as CSS Syntax 3 § 3.2 reads it.** A label
  the Encoding Standard does not know (`x-unknown`) leaves the sheet in the
  referring document's encoding or UTF-8, an alias of UTF-8 (`utf8`,
  `unicode-1-1-utf-8`) names UTF-8, and the UTF-16 family (`utf-16`,
  `unicode`, `ucs-2`) becomes UTF-8. A declaration in single quotes or
  with two spaces is no declaration. All of these pass EPUB 3.3 § 6.3.1.2.
  epubcheck 5.4.0 reports CSS-004 for each, and CSS-003 with CSS-008 for
  `"utf-16"` ([w3c/epubcheck#1770](https://github.com/w3c/epubcheck/issues/1770)).
- **SPEC-001 (error): `encryption.xml` declares encrypted a file that must
  never be.** That is `mimetype`, the six `META-INF` files, or a package
  document: EPUB 3.3 § 4.2.6.3.2.1, OCF 2.0.1 § 3.5.2. epubcheck 5.4.0 does
  not check the list. `SPEC-*` is epubveri's family for a specification rule
  epubcheck has no id for; it is an ordinary finding, not an advisory.
- **EPUB 3: a CSS parse error is CSS-008 at usage.** EPUB 3.3 `sec-css-req`
  asks for no `direction`, no `unicode-bidi` and UTF-8 or UTF-16, and nothing
  about parsing, so a sheet CSS Syntax recovers from passes. A nested style
  rule is CSS Nesting and draws nothing. A `<bad-url-token>` refers to nothing
  (CSS Syntax § 4.3.6), so it draws no RSC-007 or RSC-020. epubcheck 5.4.0
  reports CSS-008, RSC-007 and RSC-020 as errors
  ([w3c/epubcheck#1768](https://github.com/w3c/epubcheck/issues/1768)).
- **EPUB 2 follows OCF 2.0.1 on file names and rootfiles.** § 3.3 does not
  forbid `|`, controls or private use, and lets two names differ only by
  normalization; § 3.5.1 says only one package rootfile SHOULD be included,
  so a second is a PKG-013 warning. epubcheck 5.4.0 applies EPUB 3.3's rules
  ([w3c/epubcheck#1771](https://github.com/w3c/epubcheck/issues/1771)).
- **EPUB 3: `dir` is matched as HTML matches an enumerated attribute,** ASCII
  case-insensitive and unpadded (HTML § 2.3.3), and `<bdo dir="auto">` is
  RSC-005 (§ 4.5.25). So `dir=" rtl"` is RSC-005 too. epubcheck 5.4.0
  accepts both ([w3c/epubcheck#1769](https://github.com/w3c/epubcheck/issues/1769)).
- **EPUB 3: a `<script type="module">` is scripted content.** EPUB 3.3
  § 6.3.2.1 excludes only data blocks, and HTML 4.12.1 does not make a module
  script one, so it needs the `scripted` property (OPF-014 without it, no
  OPF-015 with it). epubcheck 5.4.0 asks only for a JavaScript media type, so
  it reports OPF-015 on the declared case and nothing on the other
  ([w3c/epubcheck#1773](https://github.com/w3c/epubcheck/issues/1773)).
- **An RDFa `prefix` with more than one space after the colon passes**
  (`dc:  http://…`). RDFa Core 1.1 § 5 gives `NCName ':' ' '+ xsd:anyURI`;
  epubcheck 5.4.0's pattern asks for exactly one space and reports RSC-005
  ([w3c/epubcheck#1776](https://github.com/w3c/epubcheck/issues/1776)).
- **EPUB 3: `itemtype` or `itemref` without `itemscope`, and `itemid` without
  both, are RSC-005.** HTML § 5.2.2 has a "must not" for each, and EPUB 3.3
  § 6.1.2 holds XHTML content documents to HTML's conformance criteria.
  epubcheck 5.4.0 is silent ([w3c/epubcheck#1775](https://github.com/w3c/epubcheck/issues/1775)).
- **SPEC-004 and SPEC-005: "version needed to extract" other than 10, 20 or
  45 in a local file header, and a file name over 255 bytes.** EPUB 3.3
  §§ 4.3.2 and 4.2.3, OCF 2.0.1 §§ 4 and 3.3. epubcheck 5.4.0 checks neither
  ([w3c/epubcheck#1777](https://github.com/w3c/epubcheck/issues/1777)).
- **EPUB 2: `container version` must be `1.0`,** and `META-INF/manifest.xml`,
  `metadata.xml` and `rights.xml` follow OCF 2.0.1 §§ 3.5.1–3.5.6 (ODF manifest
  root, namespace-qualified elements, well-formed). epubcheck 5.4.0 takes any
  `version` at EPUB 2 and does not check these rules for the other three
  ([w3c/epubcheck#1777](https://github.com/w3c/epubcheck/issues/1777)).
- **EPUB 3: a prefix mapped to `http://idpf.org/epub/vocab/structure/#` is
  OPF-007b,** as one mapped to the `www` spelling already was. EPUB 3.4
  § 7.4.3.2 gives this URL for the Structural Semantics Vocabulary, and
  § 5.3.4 forbids mapping a prefix to a default vocabulary. epubcheck 5.4.0
  checks only the `www` spelling.
- **SPEC-006: the manifest lists the `mimetype` file** (EPUB 3.3 § 4.2.2,
  "MUST NOT be listed in the manifest"). epubcheck 5.4.0 gives only USAGE
  OPF-097.
- **SPEC-007 and SPEC-008: bit 3 set with no data descriptor, and a central
  directory header that points at no local file header** (ZIP application
  note §§ 4.3.9.1 and 4.3.2, through EPUB 3.3 § 4.3.2 and OCF 2.0.1 § 4).
  epubcheck 5.4.0 reads both books as valid.
- **EPUB 3: one `media:duration` per publication and per overlay, and
  `media:active-class` is a class name, not a selector** (D.8.2 with D.2;
  § 9.3.4). epubcheck 5.4.0 checks neither ([w3c/epubcheck#1778](https://github.com/w3c/epubcheck/issues/1778)).

## What it declines to do

- **It never writes to a book.**
  [epubsana](https://github.com/veripublica/epubsana) does repairs.
- **It takes no unpacked directories.** Input is a packaged file, so
  epubcheck's `-mode exp` has no equivalent. A directory gets exit code 2.
- **It does not validate DTBook,** so OPF-021, which fires only inside DTBook
  content, will never be implemented. That is the one gap in
  `docs/COVERAGE.md`.
- **It adds no OEBPS 1.2 support** beyond what exists.
- **It does not reproduce host parsing in RSC-020,** which depends on epubcheck's
  URL library, galimatias.
- **It reads no network and no other file.** Any such access is a security
  defect (`SECURITY.md`).
- **It has no EPUB 3.x version gate.** Every EPUB 3 book declares
  `version="3.0"`, so a book cannot say which 3.x it targets. Rules follow the
  latest stable EPUB 3, as epubcheck does.

"Out of scope" never justifies inventing an error for a book we don't cover.

## CLI

```
epubveri -i <PATH> [-i <PATH> ...] [OPTIONS]
```

Positional paths are not accepted.

| Option | Meaning |
|---|---|
| `-i`, `--input <PATH>` | The input. Repeat for several; the exit code aggregates. |
| `--format human\|json\|ids` | Report format; `human` is the default. |
| `--profile dict\|edupub\|idx\|preview` | Also check against an extension profile. |
| `-v`, `--epub-version 2\|2.0\|3\|3.0` | Validate as this version. On a disagreement PKG-001 reports it and the requested version wins, as in epubcheck. |
| `--sort severity\|document` | Order of the human report only; `severity` is the default. Machine formats are always in document order. |
| `-u`, `--usage` | Include usage-severity findings. Decides what every format contains, and the JSON `summary` describes the output. |
| `--advisory` | Also emit `ADV-*`. They print with or without `-u`. |
| `-V`, `--version` | Print the version and exit 0. |
| `-h`, `--help` | Print help and exit 0. |

| Exit code | Meaning |
|---|---|
| 0 | Every input is valid: no error or fatal findings. |
| 1 | Every input was processed; at least one has errors. |
| 2 | A usage error, or an input that could not be processed. |

A readable but broken file gets a verdict and exit 1, even if it is not a ZIP.
A missing file exits 2 with a full envelope holding a PKG-018 fatal. A directory
exits 2 with `status: "error"` and the reason in `inputs[].error`. Only a wrong
command line leaves stdout empty, so parse stdout first and fall back to stderr.
epubcheck has no exit code 2.

## Output

**Parse `--format json`, never the human report.** The envelope is specified in
[FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md),
and `convention` names the version of that spec. `docs/INTEGRATING.md` has the
full contract, how to present findings, a worked example, and how to choose and
verify a release archive.

Each item has `type`, `code`, `severity`, `message`, and when known `rule`,
`location`, `position` (1-based line and column) and `data`. `data` can hold:

| Field | Meaning |
|---|---|
| `params` | The values interpolated into `message`, unabbreviated. |
| `element_path`, `namespaces` | An XPath-style path to the node, and its prefix bindings. |
| `violation_kind` | On schema violations, one of six kinds (`report::ViolationKind`). |
| `advisory_basis` | On advisories, `spec-silent`. (`spec-ahead` is no longer emitted.) |

Severities are `fatal`, `error`, `warning`, `info` and `usage`. Only `fatal` and
`error` make a book invalid. A fatal stops reading the unit it occurs in, so
later findings in that unit are missing.

Codes use `-` or `_`: epubcheck spells some of them `HTM_061` or `MED_007`, and
epubveri mirrors that. A pattern must accept both, and a trailing letter
(`[A-Z]+[-_][0-9]+[a-z]?`, for `HTM_060a` or `RSC-007w`).

`rule` keys come from two places: the code, and the pattern ids of the
Schematron files in `schemas/`. Searching `src/` alone misses the second.

### Advisory family

`usage` severity, and it never affects the verdict or the exit code.

- **`ADV-*` (`spec-silent`):** no specification requires it, but the book is
  probably wrong. These never graduate.
  - ADV-001 to ADV-004, ADV-009 to ADV-012, ADV-014, ADV-015, ADV-017,
    ADV-019 to ADV-024.

`NEXT-*` (`spec-ahead`) is closed. Its rules are required by a specification,
so since the verdict follows the specification they report without the flag:
NEXT-016 as PKG-007 and NEXT-018 as RSC-007, RSC-008, RSC-026 or RSC-030.

`src/ids.rs` has what each one means.

## Library

Pin the minor version: before 1.0, breaking changes land as `0.x.0`, marked
"Breaking" in `CHANGELOG.md`. MSRV is in `Cargo.toml` (`rust-version`).

- `validate_bytes`, `validate_bytes_with_profile`, `validate_bytes_with_options`
  return a `report::Report`.
- `validate_path`, `validate_path_with_profile`, `validate_path_with_options` do
  the same from a path and return `io::Result<Report>`.
- `Options` has `profile`, `epub_version` and `advisory`. The default matches
  `validate_bytes`.
- `PROFILES` lists the profile names.
- `Report` has `messages`, `is_valid()`, `errors()`, `fatals()`, `warnings()`,
  `infos()`, `usages()` and `render_human()`.
- `Message` has `id`, `severity`, `text`, `location`, `position`, `rule`,
  `params`, `element_path`, `violation_kind` and `render_human()`.
- `check_style_text(css, label, &options)` reports what validation would say
  about that CSS in an inline `<style>`, except where its urls lead.
  `style_text_urls(css)` lists the urls. A test keeps these equal to the
  validator's answers.

Other tools in the family call these directly. Do not rename them or change
their meaning without telling the consumers:

- `xmlguard::check`, the scan run before each XML parse, and its `MAX_*`
  constants;
- `xmlext::is_xml_blank`;
- `htm::declare_dtd_entities`, `htm::DtdShift` and `htm::xhtml_entity`;
- `encoding::sniff`, `encoding::Sniffed` and `encoding::parser_accepts`.

The `rule`, `location`, `params` and `violation_kind` fields are part of the
same contract. Positions and wording are not.

## WebAssembly

`epubveri-wasm/` holds the bindings and publishes `@veripublica/epubveri-wasm`.
It adds no validation logic. `validate(bytes, profile?, advisory?, epubVersion?)`
returns one envelope `inputs[i]` object, and `version()` returns the version.
Its `README.md` has the API.

It differs from the CLI in two ways:

- PKG-016 is not reported, because the check is about the file name and the
  function receives only bytes.
- `data.namespaces` is a `Map`, not a plain object.

`epubveri-wasm/src/lib.rs` has a test that counts the advisories listed in that
README. A new advisory has to be listed there too.

## Limits

| Limit | Value | Where |
|---|---|---|
| One inflated container entry | 64 MiB; past it, LIM-001 | `ocf::MAX_ENTRY_BYTES` |
| Bytes inflated for one book | 256 MiB; past it, LIM-002 | `ocf::MAX_BOOK_BYTES` |
| XML nesting depth | 256 | `xmlguard::MAX_XML_DEPTH` |
| Bytes added by entity expansion | 64 MiB | `xmlguard::MAX_EXPANSION_BYTES` |
| Attributes on one element | 256 | `xmlguard::MAX_ATTRIBUTES` |
| Elements in one document | 1,000,000 | `xmlguard::MAX_ELEMENTS` |

Each constant's doc comment gives the measurement behind it. The largest values
seen on real books are well below these limits. A panic, an abort or resource
use out of proportion to the input counts as a security problem (`SECURITY.md`).

## Measurements

**epubcheck's own test suite,** epubveri 0.43.0 against epubcheck 5.4.0, 1086
scenarios (each run prints what it skips, and why):

| | |
|---|---|
| Should-error cases flagged | 542 of 547 |
| Exact message ID | 686 of 686 |
| False positives on should-be-clean cases | 1 of 369 |
| Wrong severity | 6 of 726 |

Every miss and every gap above is a deliberate difference listed under "Known
differences from epubcheck": a CSS parse error is CSS-008 at usage in EPUB 3
(four misses, five severities), a second package document in EPUB 2 is a PKG-013
warning (one miss, one severity), and the clean case is a document with
`<bdo dir="auto">`, which HTML forbids. These fixtures are synthetic and say
nothing about real books. One scenario is a partial hit: epubcheck adds CHK-008
there, announcing a failure of its own checker
([w3c/epubcheck#1735](https://github.com/w3c/epubcheck/issues/1735)).

**Real books,** the same versions, 562 books on a private shelf, compared by
message ID with `-u`:

- 503 books report exactly the same set of IDs.
- 49 of the other 59 are EPUB 3 books whose `nav` carries `aria-label` or
  `aria-labelledby`, which epubcheck 5.4.0 rejects
  ([w3c/epubcheck#1726](https://github.com/w3c/epubcheck/issues/1726), closed
  upstream). epubveri accepts it.
- 9 report an ID epubveri alone has, each a rule epubcheck lacks: SPEC-002 in 7
  EPUB 2 books (OPF 2.0.1 § 2.3.1.1), which epubcheck rejects on other grounds,
  and RSC-005 for an EPUB 3 `img` without `alt` in 2 books, which epubcheck
  accepts.
- The last one is a deliberate difference about entity handling, in a document
  epubcheck stops reading.
- Where both report an ID a different number of times (221 cases), epubveri's
  count is the lower, except in two books where the surplus is exactly those
  `img` errors.

**Coverage,** per message ID against epubcheck 5.4.0: 217 of 218 live checks, 214
in full and 3 in part. `docs/COVERAGE.md` is generated, so regenerate it rather
than editing it.

**Speed,** epubveri 0.43.0 against epubcheck 5.4.0 on one machine
(`docs/BENCHMARK.md`, with the method): a median book took 0.023 s against 1.96 s,
and the shelf 18.5 s against about 19 minutes. Each release is also checked by
`--bin speed` against the previous one.

## Repository layout

| Path | What it holds |
|---|---|
| `src/lib.rs`, `src/main.rs` | The public entry points; the thin CLI. |
| `src/ids.rs` | Every message ID. **The authority on what the build emits.** |
| `src/report.rs`, `src/envelope.rs` | Findings; the JSON envelope. |
| `src/ocf.rs`, `src/filename.rs`, `src/encoding.rs` | The container, file names, encoding detection. |
| `src/opf.rs` | The package document and whole-book cross-references. |
| `src/htm.rs`, `src/svg.rs`, `src/mathml.rs`, `src/css.rs`, `src/navdoc.rs`, `src/ncx.rs`, `src/url.rs`, `src/image.rs`, `src/cmt.rs`, `src/foreign.rs`, `src/ssv.rs`, `src/aria_roles.rs` | Content and resources. CSS goes through the sibling crate [styloria](https://github.com/veripublica/styloria). |
| `src/smil.rs`, `src/edupub.rs`, `src/dict.rs`, `src/indexes.rs`, `src/previews.rs`, `src/layout.rs`, `src/regionnav.rs`, `src/renditions.rs` | Extensions. |
| `src/rng/`, `src/schematron/`, `src/xpath/` | Our RELAX NG and Schematron engines, and the XPath they use. |
| `src/xmlguard.rs`, `src/xmlext.rs`, `src/xmlname.rs` | The pre-parse guard; XML helpers. |
| `schemas/` | The grammars, embedded at compile time by `build.rs`. |
| `epubveri-wasm/` | The WebAssembly bindings and the demo page. |
| `harness/` | Measurement tools (`epubveri-harness`, not published). |
| `scripts/preflight.sh` | The release pre-flight. |
| `LICENSE-THIRD-PARTY.txt` | Generated by `--bin notices`; the copy in `epubveri-wasm/` goes into the npm package. |
| `docs/` | `USAGE.md` (for people), `INTEGRATING.md` (for tool authors), `ARCHITECTURE.md` (the pipeline), `BENCHMARK.md`, `COVERAGE.md`. |

`corpus/` is gitignored and holds third-party material. Test fixtures are never
committed here.

## Build and test

```sh
cargo build --release
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build -p epubveri-wasm --target wasm32-unknown-unknown --locked
scripts/preflight.sh            # everything above and the instruments; SKIP_SLOW=1 skips the slow ones
```

On Windows, Rust needs Microsoft's linker, from Build Tools for Visual Studio
(*Desktop development with C++*). Without it every build, `cargo install epubveri`
included, stops with ``linker `link.exe` not found``. VS Code does not provide it.

`cargo fmt --check` is enforced in CI. After changing a schema, rebuild with
`cargo build --release --workspace`. The grammars are embedded at compile time,
so a stale harness binary reports the old schema's findings.

To use a local styloria, add an uncommitted override and check that `Cargo.lock`
no longer names the registry for it, because a patch can be silently ignored:

```toml
[patch.crates-io]
styloria = { path = "../styloria" }
```

## Measuring

Each tool runs as `cargo run --release -p epubveri-harness --bin <name>`. Each
answers a different question, so do not read one as a general result.

| `--bin` | Question | Needs |
|---|---|---|
| `corpus` | Do we report what epubcheck's own fixtures expect? | `corpus/epubcheck` |
| `compare` | Where do we and epubcheck disagree on these books? | a JVM, the jar; `-- <paths>` |
| `coverage` | Regenerates `docs/COVERAGE.md`. | `corpus/epubcheck` |
| `severities` | Does each ID carry epubcheck's severity? | `corpus/epubcheck` |
| `versions` | Which IDs are EPUB 2 or EPUB 3 only, by their call sites? | `corpus/epubcheck` |
| `downgrade` | Prepares EPUB 3 books re-declared as EPUB 2, for `compare`. | input books |
| `epubtests` | What do we say about W3C's conformance publications? | `corpus/epub-tests` |
| `hostile` | Does adversarial input abort, panic or time out? | nothing |
| `fuzz` | Does a mutated valid book panic? | nothing |
| `speed` | Per book, is this build slower than the previous release? | the release binary |
| `spike` | A quick self-contained sanity check. | nothing |
| `notices` | Regenerates `LICENSE-THIRD-PARTY.txt` from what the packages link. `--check` compares. | nothing |

```sh
git clone --branch v5.4.0 --depth 1 https://github.com/w3c/epubcheck corpus/epubcheck
git clone --depth 1 https://github.com/w3c/epub-tests corpus/epub-tests
```

`compare` looks for the jar at `corpus/tools/epubcheck-5.4.0/epubcheck.jar` and
for Java under Homebrew's OpenJDK path. Override them with `EPUBCHECK_JAR` and
`EPUBCHECK_JAVA`. Run epubcheck with `-u`, or the two tools are not comparing the
same output.

A "no change" result needs evidence that both sides ran. Compare per book, not
by total: one bad book can hide a regression of many findings.

Before implementing a row from `docs/COVERAGE.md`, read epubcheck's call site in
its source, and ask what runs before it.

## Contributing and licence

External pull requests are not accepted yet (`CONTRIBUTING.md`): selling the
commercial licence requires holding copyright over the whole codebase, and the
contributor agreement that would allow it does not exist yet. Issues are welcome,
and a false positive is the report the project most wants.

`AGPL-3.0-only OR LicenseRef-veripublica-Commercial` (`LICENSE`,
`LICENSE-COMMERCIAL.md`, `LICENSING.md`). `LICENSE-THIRD-PARTY.txt` holds the
licences of the crates compiled into the binaries and the npm package. After any
dependency change, regenerate it with `--bin notices`; the pre-flight fails if it
is stale.

"epubcheck" is W3C's name. Do not reuse or imitate it in a product name, label or
caption, and write our own message wording.
