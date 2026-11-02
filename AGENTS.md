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
- **It matches epubcheck's verdict.** A book that passes epubcheck passes
  epubveri. Where the two disagree, the specification decides: if epubveri is
  wrong it gets fixed, and if epubcheck is wrong an issue goes upstream.
- **A check epubcheck does not make goes behind `--advisory`.** An extra
  finding on a book epubcheck passes looks like a false positive to anyone
  comparing the tools. Accepting what used to be rejected needs no flag.
- **An advisory must be true every time it fires,** and is worded as an
  observation, not a verdict.
- **The library never filters.** `validate_bytes` returns every finding at
  every severity, and rules that consumers dispatch on can sit below error
  severity. Filtering belongs at display.
- **It parses independently.** It shares no parser with the tools it judges.
- **It has no C dependencies and no JVM.** The code must also build for
  `wasm32-unknown-unknown`.

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
| `--advisory` | Also emit `NEXT-*` and `ADV-*`. They print with or without `-u`. |
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
| `advisory_basis` | On advisories, `spec-ahead` or `spec-silent`. |

Severities are `fatal`, `error`, `warning`, `info` and `usage`. Only `fatal` and
`error` make a book invalid. A fatal stops reading the unit it occurs in, so
later findings in that unit are missing.

Codes use `-` or `_`: epubcheck spells some of them `HTM_061` or `MED_007`, and
epubveri mirrors that. A pattern must accept both, and a trailing letter
(`[A-Z]+[-_][0-9]+[a-z]?`, for `HTM_060a` or `RSC-007w`).

`rule` keys come from two places: the code, and the pattern ids of the
Schematron files in `schemas/`. Searching `src/` alone misses the second.

### Advisory families

Both are `usage` severity and never affect the verdict or the exit code.

- **`NEXT-*` (`spec-ahead`):** a published specification requires it and
  epubcheck has not implemented it yet. The ID moves to epubcheck's own when
  epubcheck ships the rule.
  - NEXT-016, NEXT-018.
- **`ADV-*` (`spec-silent`):** no specification requires it, but the book is
  probably wrong. These never graduate.
  - ADV-001 to ADV-004, ADV-009 to ADV-012, ADV-014, ADV-015, ADV-017,
    ADV-019 to ADV-024.

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

A consumer running a debug build, `cargo test` included, needs this in its root
manifest. Without it a 256-level document overflows the 2 MiB test thread:

```toml
[profile.dev.package.roxmltree]
opt-level = 1
```

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

**epubcheck's own test suite,** epubveri 0.36.0 against epubcheck 5.4.0, 1086
scenarios (each run prints what it skips, and why):

| | |
|---|---|
| Should-error cases flagged | 548 of 548 |
| Exact message ID | 687 of 687 |
| False positives on should-be-clean cases | 0 of 368 |
| Wrong severity | 0 of 727 |

These fixtures are synthetic and say nothing about real books. One scenario is a
partial hit: epubcheck adds CHK-008 there, announcing a failure of its own checker
([w3c/epubcheck#1735](https://github.com/w3c/epubcheck/issues/1735)).

**Real books,** the same versions, 562 books on a private shelf, compared by
message ID with `-u`:

- 512 books report exactly the same set of IDs.
- No ID is reported by epubveri alone.
- 49 of the other 50 are EPUB 3 books whose `nav` carries `aria-label` or
  `aria-labelledby`, which epubcheck 5.4.0 rejects
  ([w3c/epubcheck#1726](https://github.com/w3c/epubcheck/issues/1726), closed
  upstream). epubveri accepts it.
- The last one is a deliberate difference about entity handling, in a document
  epubcheck stops reading.
- Where both report an ID a different number of times (225 cases), epubveri's
  count is always the lower.

**Coverage,** per message ID against epubcheck 5.4.0: 217 of 218 live checks, 214
in full and 3 in part. `docs/COVERAGE.md` is generated, so regenerate it rather
than editing it.

**Speed,** epubveri 0.36.0 against epubcheck 5.4.0 on one machine
(`docs/BENCHMARK.md`, with the method): a median book took 0.023 s against 2.01 s,
and the shelf 19 s against about 19 minutes. Each release is also checked by
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
