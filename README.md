# epubveri

A standalone, pure-Rust EPUB validator — a small, fast, JVM-free, embeddable
alternative to [epubcheck](https://github.com/w3c/epubcheck), the official
Java-based EPUB validator.

**Status: early / pre-alpha.** This is not a drop-in epubcheck replacement
yet. See "Current coverage" below for an honest snapshot of what's actually
implemented and measured.

## Why

epubcheck is industry-standard infrastructure, but it requires a JVM, which
makes it awkward to embed in web services, native apps, or the browser.
`epubveri` aims to be:

- **Pure Rust** — no C dependencies, no JVM.
- **Embeddable** — usable as a library (`epubveri` crate), a CLI, and
  eventually as a WASM package for browser/app embedding.
- **Familiar** — reuses epubcheck's message ID scheme (`PKG-`, `RSC-`,
  `OPF-`, etc.) where our checks overlap, so existing tooling and users
  recognize the output.

## Current coverage

Measured against epubcheck's own Cucumber test corpus (not redistributed
here — see "Test corpus" below):

- ~24 hand-coded structural checks covering OCF/mimetype, OPF well-formedness,
  manifest/spine integrity, and broken internal references.
- 0 false positives on the corpus's clean fixtures.
- ~13% exact-message-ID recall against the corpus's error scenarios.
- A from-scratch RELAX NG validation engine (`src/rng/`) is wired in for the
  package document (OPF), authored against our own schema
  (`schemas/package.rng`). It is FP-safe but does not yet move the coverage
  number — the package layer is already saturated by the hand-coded checks.
  The bigger unlock (XHTML content-model validation) is still ahead.

This project does not aim to overclaim parity with epubcheck. Full parity is
a long road; this repo tracks that honestly.

## Usage

```
cargo run --bin epubveri -- --format human path/to/book.epub
cargo run --bin epubveri -- --format ids path/to/book.epub
```

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

## Test corpus

We validate against epubcheck's own test corpus and W3C's `epub-tests` to
measure coverage, but we do not redistribute their test fixtures in this
repo (separate license, and we don't want to ship copies of their test
EPUBs). Corpus tooling under `scripts/` fetches/builds fixtures locally into
a gitignored `corpus/` directory.
