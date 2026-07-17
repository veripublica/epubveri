# epubveri-wasm

WebAssembly bindings for [**epubveri**](https://github.com/veripublica/epubveri) — a
pure-Rust EPUB validator. Validate an `.epub` **entirely in the browser** (or any JS
runtime): no JVM, no server round-trip, no native dependencies. The bytes never leave
the page.

This is the WASM delivery of epubveri, a small/fast/embeddable alternative to the
official Java **epubcheck**. It reuses epubcheck-compatible message IDs (`RSC-…`,
`OPF-…`, `HTM-…`, …) so existing toolchains recognize the output.

## Install

```
npm install @veripublica/epubveri-wasm
```

## Usage (bundlers — webpack / Vite / Rollup)

The published package is built for **bundlers**, so there's no manual init step —
your bundler loads the `.wasm` for you:

```js
import { validate, version } from "@veripublica/epubveri-wasm";

const bytes = new Uint8Array(await file.arrayBuffer()); // a File / fetched .epub
const report = validate(bytes, undefined); // second arg: profile or undefined

console.log(report.status, report.summary); // "ok" | "problems", { errors, warnings }
for (const it of report.items) {
  console.log(`${it.severity} ${it.code}: ${it.message}`, it.location ?? "");
}
```

`report` is the veripublica machine envelope's `inputs[i]` shape — the *same*
object the CLI's `--format json` emits per input (minus `path`), so one parser
reads CLI, CI and browser output alike.

Using it **directly in a browser without a bundler**? Build the `web` target
instead (`wasm-pack build . --target web`), which exposes an async `init()` you
`await` once before calling `validate()` — that's what the demo below uses.

### Return shape (fully typed — real `.d.ts` ships in the package)

```ts
interface Report {
  status: string;        // "ok" (valid) | "problems" (error/fatal findings remain)
  summary: { fatals?: number; errors: number; warnings: number };
  items: Item[];
}
interface Item {
  type: string;          // "finding"
  code: string;          // epubcheck-compatible, e.g. "RSC-005"
  rule?: string;         // epubveri's finer sub-code, when present
  severity: string;      // "fatal" | "error" | "warning" | "info" | "usage"
  location?: string;     // container-relative path, when available
  position?: { line: number; column: number };
  message: string;       // epubveri's own message wording
  data?: { params: string[] };
}

function validate(bytes: Uint8Array, profile?: string | null): Report;
function version(): string;
```

### Profiles

`profile` mirrors epubcheck's `--profile` flag: pass `"dict"`, `"edupub"`, `"idx"`,
`"preview"`, or `undefined`/`null` for default behavior. Unknown names are treated as
`undefined` (permissive).

### One CLI-only difference

The `PKG-016` check (the `.epub` **file extension** should be lowercase) is filename-
based and is **not** reported here — this entry point only ever sees bytes, never a
filename. Everything else matches the native CLI/library exactly.

## Try the demo

The `demo/` folder has a zero-dependency drag-and-drop page. From the crate root:

```
wasm-pack build . --target web --out-name epubveri   # produces pkg/
# then serve this folder over HTTP (wasm needs http://, not file://):
#   any static server works, e.g. `miniserve .` or `python3 -m http.server`
# open http://localhost:8000/demo/
```

## Building from source

```
cargo install wasm-pack

# the published npm package (bundler target, @veripublica scope):
wasm-pack build . --target bundler --scope veripublica --out-name epubveri

# or the web target used by the demo above:
wasm-pack build . --target web --out-name epubveri
```

The generated package lands in `pkg/` (git-ignored). Each `--target`
(`bundler` / `web` / `nodejs`) emits different JS glue, so pick the one that
matches how you'll load it; use `--target nodejs` for Node.

## License

Dual-licensed: **AGPL-3.0-only OR a commercial license** (`LicenseRef-veripublica-Commercial`).
Open-source use is free under the AGPL; closed/commercial embedders should contact the
author for a commercial license. Both texts ship inside the npm package —
`LICENSE` (the AGPL) and `LICENSE.COMMERCIAL.md` (what the `LicenseRef-` above
means, and who to ask).

> **The `.` in `LICENSE.COMMERCIAL.md` is load-bearing — do not rename it to a
> hyphen.** These two files are copies of the repository root's `LICENSE` and
> `LICENSE-COMMERCIAL.md`; `wasm-pack` only picks up licenses from the crate
> directory, so the copies have to live here. npm then always packs a file
> matching `license` optionally followed by a *dotted* extension, whatever
> `files` says — a hyphen does not match, and the file is silently dropped from
> the tarball. That is not hypothetical: it is what happened to
> `LICENSE-COMMERCIAL.md` in 0.5.10, which shipped the AGPL text with no word of
> the commercial option beside it. Relying on `wasm-pack`'s own `files` list
> instead is not an option — it writes `package.json` *before* it copies the
> licenses, so a clean build never lists them.
