# epubveri

[![CI](https://github.com/veripublica/epubveri/actions/workflows/ci.yml/badge.svg)](https://github.com/veripublica/epubveri/actions/workflows/ci.yml)

**A pure-Rust EPUB validator — a small, fast, JVM-free alternative to
[epubcheck](https://github.com/w3c/epubcheck).**

If you've never heard of epubcheck either, don't worry — this README
explains everything from the ground up. No prior EPUB or Rust knowledge
assumed.

---

## What is epubveri?

An EPUB file is a `.epub` file — the format almost every ebook uses. Under
the hood, it's actually a ZIP archive containing HTML-like content files,
some metadata files that describe the book (title, author, table of
contents, reading order), and a few required structural pieces that make
it a valid EPUB rather than just "a ZIP file full of HTML."

It's surprisingly easy to produce a `.epub` file that *looks* fine —
opens in your reading app, shows the right pages — but is actually
**broken** according to the EPUB specification: a missing piece of
metadata, a reference to a file that doesn't exist, a table of contents
that points at the wrong place, an image declared with the wrong file
type. Some reading apps are lenient and paper over these problems
silently. Others aren't, and the book fails to open, or a retailer's
ingestion pipeline rejects it outright.

**A validator's job is to catch these problems before a reader ever
sees them**, by checking the file against the official EPUB
specification and reporting exactly what's wrong and why.

**epubveri does that job.** Feed it a `.epub` file, and it tells you
whether the file conforms to the EPUB specification, and if not, exactly
what's wrong (with a short code and a plain-language message, e.g.
`OPF-002: OPF package document not found`).

## Why does this exist? Isn't epubcheck already the standard?

Yes — [epubcheck](https://github.com/w3c/epubcheck) is the official,
W3C-maintained EPUB validator, and it's the industry standard. Every
major publishing pipeline uses it somewhere. epubveri is not trying to
discredit it or replace its authority — it's trying to solve one
specific, practical problem with it:

**epubcheck is written in Java, and needs a JVM (Java Virtual Machine)
to run.** That's fine if you're running it as a standalone command-line
tool on a server. It's a real obstacle if you want to:

- run validation **inside a web browser** (there's no JVM in a browser —
  but there is WebAssembly, which Rust compiles to natively),
- **embed** a validator inside a native desktop/mobile app without
  bundling an entire Java runtime,
- run validation as part of a **fast, lightweight command-line tool or
  CI pipeline** without the JVM startup cost, or
- just avoid the operational overhead of "make sure a compatible JVM is
  installed" as a deployment requirement.

epubveri is written in **pure Rust**, with **zero C dependencies** and
**zero JVM dependency**. Rust code compiles down to a small, fast native
binary — and, crucially, can also compile to **WebAssembly (WASM)**,
meaning the exact same validation logic can eventually run directly in a
web browser tab, with no server round-trip and no Java anywhere.

It reuses epubcheck's own short error-code scheme (`OPF-002`, `RSC-005`,
`PKG-007`, and so on) wherever the checks overlap, specifically so that
anyone already familiar with epubcheck's output — or any tooling built
to parse it — recognizes epubveri's output immediately. This is a
deliberate compatibility choice, not a coincidence.

## Who is this for?

- **EPUB producers** (publishing tools, conversion pipelines, self-
  publishing platforms) who want to validate the files they generate
  before shipping them, ideally without adding a JVM to their toolchain.
- **Retailers and distributors** who ingest EPUB files from many sources
  and need to reject malformed ones automatically.
- **Reading-app / e-reader developers** who want a lightweight,
  embeddable check — including, eventually, one that runs client-side
  in a browser via WebAssembly.
- **Developers building any tool that touches EPUB files** and wants a
  validator as a library dependency (a Rust crate) rather than shelling
  out to a separate Java process.
- **Anyone curious how EPUB validation actually works.** This project is
  also meant to be *readable* — see "Going deeper" below.

If none of that describes you, but you just have one `.epub` file you're
curious about, the command-line tool works fine for that too (see
"Trying it out" below).

## What does epubveri actually check?

An EPUB file has a few layers, and epubveri validates each one:

1. **The container itself (OCF).** Is it really a ZIP file? Is the
   required `mimetype` entry present, first, and uncompressed (a real
   requirement of the format, there for historical file-type-detection
   reasons)? Is there a `META-INF/container.xml` pointing at the actual
   package document?
2. **The package document (OPF).** This is the book's own manifest: its
   metadata (title, language, a unique identifier), the full list of
   every file that belongs to the book (the "manifest"), and the reading
   order (the "spine"). epubveri checks that all of this is well-formed,
   internally consistent, and complete — e.g. that every file the
   manifest lists actually exists in the ZIP, that the spine doesn't
   reference something outside the manifest, that required metadata
   isn't missing.
3. **The content documents themselves** — the actual HTML-like chapter
   files, plus embedded SVG, MathML, and CSS. Do they use only real,
   non-obsolete markup? Do internal links and image references actually
   resolve? Is the navigation document (the EPUB 3 table of contents)
   structured correctly?
4. **A number of optional EPUB extension specifications** — Media
   Overlays (synchronized text/audio playback), EDUPUB (educational
   publications), Dictionaries & Glossaries, Indexes, Previews, Fixed
   Layout, Multiple Renditions, and more — each only checked when a book
   actually declares it's using that feature.

Every one of these is a real, separately-specified part of the EPUB
standard — epubveri doesn't invent rules, it implements the ones the
specification (and epubcheck's own real-world test suite) already
define.

## How good is it right now? (the honest part)

epubveri is **pre-1.0 and under active development.** This is not a
drop-in replacement for epubcheck yet, and this project is deliberately
upfront about that rather than overclaiming. (The WebAssembly build is
already published to npm as `@veripublica/epubveri-wasm`; the Rust crate
isn't on crates.io yet — the `epubveri` name there is currently just a
reserved placeholder. See "Using it as a library" below.)

To measure real progress (not just "does it seem to work"), epubveri is
tested against **epubcheck's own test suite** — hundreds of real,
official test cases, each one a small EPUB file specifically constructed
to be valid or to trip exactly one specific rule. As of this writing:

- **98.8%** of the test suite's "this should be flagged" cases are
  correctly caught, with the *exact same error code* epubcheck itself
  would report.
- **98.9%** of the test suite's "this is perfectly valid" cases are
  correctly left alone (no false alarms) — the remaining handful are
  understood and narrow.

## Trying it out

You'll need [Rust installed](https://www.rust-lang.org/tools/install)
(the `cargo` command). Then, from a clone of this repo:

```sh
# Build it once:
cargo build --release

# Validate a book, human-readable output:
./target/release/epubveri path/to/book.epub

# Just the list of error/warning codes (useful for scripting):
./target/release/epubveri --format ids path/to/book.epub
```

Example output:

```
$ ./target/release/epubveri my-broken-book.epub
ERROR OPF-002: OPF package document not found: OEBPS/content.opf
— 1 error(s), 0 warning(s): INVALID
```

The exit code follows Unix convention: `0` if the book is valid, `1` if
it found at least one error, `2` if something went wrong just trying to
read the file (e.g. it isn't a ZIP at all).

**Using it as a library** (inside your own Rust project): the functional
crate isn't on crates.io yet, so add it as a **git (or path) dependency** —
for example, in your `Cargo.toml`:

```toml
epubveri = { git = "https://github.com/veripublica/epubveri" }
```

⚠️ Don't `cargo add epubveri`: the `epubveri` name on crates.io is currently
only a **`0.0.0` placeholder** reserving the name — it has no functionality
yet. Then:

```rust
let report = epubveri::validate_path(std::path::Path::new("book.epub"))?;
for message in &report.messages {
    println!("{} {}: {}", message.severity, message.id, message.text);
}
println!("valid: {}", report.is_valid());
```

**The `--profile` flag**, if you've used epubcheck before: epubcheck
supports checking a book against an *additional* profile — e.g.
`--profile dict` to also enforce EPUB Dictionaries & Glossaries-specific
rules. epubveri supports the same four profiles (`dict`, `edupub`,
`idx`, `preview`) the same way:

```sh
./target/release/epubveri --profile dict my-dictionary.epub
```

## Use it in the browser (WASM)

Because epubveri is pure Rust with no JVM and no native dependencies, it
compiles to **WebAssembly** and runs entirely in the browser (or any
JavaScript runtime) — the `.epub` never leaves the page, and there's no
server to run. This is the thing real epubcheck can't do: it's Java, so
browser/app embedding means shipping or hosting a JVM.

The bindings live in the [`epubveri-wasm/`](epubveri-wasm/) workspace
crate and publish to npm as **`@veripublica/epubveri-wasm`**. Once installed
(via a bundler like webpack/Vite), no init step is needed:

```js
import { validate } from "@veripublica/epubveri-wasm";
const report = validate(new Uint8Array(await file.arrayBuffer()), undefined);
console.log(report.valid, report.messages); // fully typed (.d.ts ships in the package)
```

Build it yourself with [`wasm-pack`](https://rustwasm.github.io/wasm-pack/):

```sh
cargo install wasm-pack
wasm-pack build epubveri-wasm --target bundler --scope veripublica --out-name epubveri
```

`epubveri-wasm/demo/` has a zero-dependency drag-and-drop demo page (built
with `--target web`). See [`epubveri-wasm/README.md`](epubveri-wasm/README.md)
for the full API and the one CLI-only difference (the filename-based
`PKG-016` check).

## Frequently asked questions

**Is this a drop-in replacement for epubcheck?** Not yet, and maybe
never entirely — see "How good is it right now?" above. It's much
further along on structural/packaging correctness than on some of the
deeper content-model checks. If you need epubcheck's full authority
today (e.g. for a retailer's official ingestion gate), keep using real
epubcheck; epubveri is a complementary, lighter-weight option that's
improving quickly.

**Can I use it in production today?** You can — several
publisher/retailer-style checks are already at 100% recall against the
real test suite (packaging, manifest/spine integrity, OCF, navigation
documents, and more). Just go in with eyes open about what's measured
and what isn't, and validate against your own real EPUB files before
depending on it for anything load-bearing.

**What license is this under? Can I use it commercially?** epubveri is
dual-licensed: free under **AGPL-3.0** for any use (including
commercial), as long as your own product also complies with the AGPL's
terms (notably: if you offer it as a network service, you must make your
complete corresponding source available to your users). If that doesn't
work for you — e.g. you want to embed epubveri in a closed-source
product without those obligations — a separate **commercial license** is
available. See [`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md) and
contact baris@kayadelen.com.

**Why isn't it just MIT/Apache licensed like most Rust crates?** A
deliberate choice, not an oversight. The dual AGPL/commercial model
exists specifically so that open-source users always get it for free,
while companies that want to embed it in a closed product contribute
back financially — funding continued development, rather than the
common open-source outcome where a company profits from unpaid
volunteer work with no path back to the people building it.

**Can I contribute?** Not via pull request yet — see
[`CONTRIBUTING.md`](./CONTRIBUTING.md) for exactly why (short version:
selling a commercial license requires the project to hold full copyright
over the whole codebase, which needs a Contributor License Agreement
process that isn't built yet). Opening an issue to discuss an idea is
always welcome.

**Does it support WebAssembly (WASM) yet?** Not yet — it's on the
roadmap, and it's one of the more exciting reasons this project exists:
a pure-Rust validator can compile to WASM and run **directly in a web
browser**, something a JVM-based tool fundamentally cannot do.

**Why is it called "epubveri"?** "Veri" carries a deliberate triple
meaning: it's the start of "veri(fy)" (English), it echoes "veritas /
verity" (Latin, "truth"), and it's also the actual Turkish word for
"data" (the author is Turkish). So "epub-veri" reads naturally as "epub
verify" while still being a distinctive, ownable name rather than a
generic one.

**Where do I report a bug, or a book epubveri gets wrong?** Open an
issue on this repository with the `.epub` file (or a minimal excerpt)
that reproduces the problem, and what you expected vs. what epubveri
reported.

**What's `styloria`? What's `schemora`?** `styloria`
(`github.com/veripublica/styloria`) is a sibling pure-Rust CSS parser
project epubveri depends on for its CSS checks — split out because it's
genuinely useful on its own, independent of EPUB. `schemora` was a
similar attempt at splitting out epubveri's XPath/Schematron engine, but
it was archived after turning out to have no real independent use — that
code now lives only inside epubveri itself. Both are examples of a
broader principle this project follows: split code into its own repo
only when there's a real second user for it, not just because it's
theoretically reusable.

## License

`epubveri` is dual-licensed:

- **AGPL-3.0-only** ([`LICENSE`](./LICENSE)) — free for any use, including
  commercial products, as long as your product also complies with the AGPL
  (including the network-use / source-disclosure clause).
- **Commercial license** (`LicenseRef-veripublica-Commercial`,
  see [`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md)) — for embedding
  `epubveri` in closed-source or proprietary products without AGPL's
  copyleft obligations. Contact baris@kayadelen.com.

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md). In short: not accepting external
contributions yet — a CLA is required first (see that file for why).

## Going deeper

This README is deliberately kept beginner-friendly. If you want to
understand how the validator actually works internally — the module
layout, the custom RELAX NG and XPath/Schematron engines built for this
project, how the test/measurement setup works, and how to add a new
check — see [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md).
