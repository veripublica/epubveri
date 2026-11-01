# epubveri

epubveri checks an EPUB file against the EPUB specification and tells you what is wrong with it and where. It does the same job as W3C's [epubcheck](https://github.com/w3c/epubcheck) and reports the same message IDs, so its output will look familiar if you know that tool. epubcheck needs Java. epubveri doesn't: it's a small program written in Rust, and it also runs in a web browser, where your book never leaves the page.

I built it for people who make ebooks, and for software that has to check them, such as editors and conversion pipelines, where shipping a Java runtime is a problem.

To check a book right now, [try it in your browser](https://veripublica.github.io/epubveri/). If you edit in Sigil or calibre, [my plugins](https://github.com/veripublica/epubveri-plugins) run it for you. Otherwise download the program from the [releases page](https://github.com/veripublica/epubveri/releases) ([step by step](docs/USAGE.md)) or install it with Rust:

```sh
cargo install epubveri
```

Then point it at a book:

```
$ epubveri -i book.epub
ERROR RSC-007: reference to a resource missing from the publication: 'cover.jpg' [OEBPS/ch1.xhtml:6:4]
— 1 error(s), 0 warning(s): INVALID
```

It's also a Rust crate, and an npm package called `@veripublica/epubveri-wasm`.

epubveri never changes your book. Its sibling [epubsana](https://github.com/veripublica/epubsana) repairs the problems that have one right answer. If a retailer asks for epubcheck by name, run epubcheck as well: I work to match its verdicts, but passing epubveri isn't the same as passing epubcheck.

If epubveri reports an error on a book you believe is fine, please [open an issue](https://github.com/veripublica/epubveri/issues). That's the report I most want.

The licence is AGPL-3.0. If that doesn't work for your product, a commercial licence is available: see [LICENSING.md](LICENSING.md) or write to baris@kayadelen.com. I can't accept pull requests yet, and [CONTRIBUTING.md](CONTRIBUTING.md) explains why.

Options, exit codes, the API, limits, measurements and how to build it are all in [AGENTS.md](AGENTS.md).
