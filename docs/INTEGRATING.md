# Integrating epubveri into another program

For plugin and tool authors — a Sigil or calibre plugin, a CI step, a build
pipeline, an editor integration.

If you are a person validating a book by hand, you want
[USAGE.md](USAGE.md) instead.

---

## The one rule: parse `--format json`, never the human output

```sh
epubveri --format json -i book.epub
```

The human report is written for a person reading a terminal. Its wording, its
line order, its spacing and which severities it shows are **allowed to change
between releases**, and they have: recent versions changed a message's wording,
moved a column to point at an attribute instead of its element, and stopped
printing `USAGE` lines by default.

Its order is not even fixed *within* a release — `--sort severity` (the default)
groups most-severe-first and `--sort document` walks the book front to back, so
the arrangement of the human report is the reader's choice. The JSON is always
in document order, whatever the user typed, so a tool never receives an order
its user picked.

The JSON envelope is the opposite. It is a documented contract, it is versioned,
and it carries things the human report has no room for — a stable sub-code per
finding, a resolvable path to the offending node, the kind of a schema
violation. If your plugin parses the text, none of that exists for you.

**`-u`/`--usage` decides what every format contains**, not only the human
report: without it, usage-severity findings are absent from `--format json` and
`--format ids` as well, and the `summary` counts describe what the output holds.
That matches epubcheck, whose `-u` gates its JSON the same way. **If your tool
wants everything — and most should, so it can filter in its own UI without
re-running the validator — pass `-u`.** Findings from `--advisory` are the
exception: they print whenever that flag is on, since it is their switch.

**The library is the one place that never filters.** `epubveri::validate_bytes`
returns every finding at every severity regardless of any flag, because a
consumer dispatching on findings below error severity would otherwise go
silently dark.

## What the envelope looks like

One JSON object per run. Trimmed from a real run:

```json
{
  "tool": "epubveri",
  "tool_version": "0.10.0",
  "convention": "0.4",
  "status": "problems",
  "inputs": [
    {
      "path": "book.epub",
      "status": "problems",
      "summary": { "error": 1, "warning": 0, "usage": 1 },
      "items": [
        {
          "type": "finding",
          "code": "OPF-003",
          "rule": "opf.container.resource_not_in_manifest",
          "severity": "usage",
          "location": "OEBPS/content.opf",
          "message": "container resource 'OEBPS/stray.txt' is not listed in the manifest",
          "data": { "params": ["OEBPS/stray.txt"] }
        },
        {
          "type": "finding",
          "code": "RSC-005",
          "rule": "opf.spine.missing_toc_epub2",
          "severity": "error",
          "location": "OEBPS/content.opf",
          "position": { "line": 8, "column": 3 },
          "message": "EPUB 2 <spine> is missing the required 'toc' (NCX) attribute",
          "data": {
            "element_path": "/opf:package[1]/opf:spine[1]",
            "namespaces": { "opf": "http://www.idpf.org/2007/opf" }
          }
        }
      ]
    }
  ]
}
```

The envelope itself is a shared, cross-tool format, specified in
[**FORMATS.md**](https://github.com/veripublica/conventions/blob/main/FORMATS.md).
The `convention` field names the version of that spec. Read it for the parts
this page does not repeat.

**`summary`'s keys are the severity words themselves**, so
`summary[item.severity]` is a direct lookup rather than something needing a
mapping table. `error` and `warning` are always present; `fatal`, `info` and
`usage` are omitted when zero. The counts describe what the output contains, so
without `-u` the usage count is `0`.

**Absent means absent.** Every optional field is omitted rather than sent as
`null`, so test for presence, not for a null value.

**Ignore what you do not recognise.** New fields are added over time; a
consumer that rejects unknown keys will break on a release that took nothing
away from it.

## The fields, and what you can rely on

| Field | Stable? | Use it for |
|---|---|---|
| `code` | **yes** | epubcheck's message ID (`RSC-005`, `OPF-003`). The interop key. |
| `severity` | **yes** | `fatal` \| `error` \| `warning` \| `info` \| `usage`, lowercase. |
| `location` | **yes** | Path *inside* the EPUB container. |
| `rule` | **yes** | epubveri's own semantic sub-code. Prefer this when `code` is too coarse — `RSC-005` alone means dozens of unrelated things. |
| `position` | line yes, column may improve | 1-based line and column. Use it to place a cursor. |
| `message` | **no** | Display it; never match on it. |
| `data.params` | **yes** | The values interpolated into `message`, so you can re-render it. |
| `data.element_path` | **yes** | XPath-style path to the offending node, with `data.namespaces` giving the prefix bindings needed to resolve it. This is the robust way to find the thing a finding is about. |
| `data.violation_kind` | **yes** | On schema violations: which of six kinds of fault this is, without parsing the message. |

**Why `rule` matters.** `RSC-005` is epubcheck's catch-all for "the document
does not match its schema", so keying behaviour on `code` alone puts a hundred
unrelated conditions in one bucket. `rule` splits them —
`opf.spine.missing_toc_epub2`, `opf.content_document.schema_violation` — and is
stable. It exists because a repair tool asked for it.

**Why `element_path` matters more than `position`.** A position is a place in a
file and can be improved; a path identifies a *node* and does not move. If you
need to act on the thing a finding describes rather than just show it, resolve
the path.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Valid — no error- or fatal-severity findings. |
| `1` | At least one error or fatal. |
| `2` | epubveri could not run, or could not read an input at all. |

`2` is about the *tool*, not the book: a file that is readable but broken —
even one that is not a valid ZIP — still gets a verdict and exit `1`. With
several `-i` inputs the exit code is the worst across them.

**A note on `--advisory`.** It never changes the verdict or the exit code, by
permanent design, so you can offer it as a display option without worrying that
it will start failing your users' books. See [USAGE.md](USAGE.md#the-two-switches-and-what-is-on-by-default).

## Or link the library directly

If your tool is Rust, skip the process boundary:

```toml
[dependencies]
epubveri = "0.10"
```

```rust
let report = epubveri::validate_bytes(std::fs::read("book.epub")?);
for m in &report.messages {
    println!("{}", m.render_human());
}
```

`validate_bytes` returns **every** finding at every severity — hiding `usage`
is the CLI's job, never the library's. `Message::render_human()` gives you the
exact line the CLI prints, so you can group and order findings however you like
without your output drifting from ours.

epubveri is pre-1.0, so breaking API changes land as minor bumps (`0.x.0`) and
are listed in [CHANGELOG.md](../CHANGELOG.md). **0.10.0 is one of those**: it
adds a field to `report::Message` and turns `rng::Blame::Text` into a struct
variant, so any struct literal or `match` over those needs a line. Nothing
changes for a consumer that only reads a `Report` — which is most of them, and
is why the CLI, the JSON envelope and the WASM bindings are untouched.

## In the browser

There is a WASM package — `@veripublica/epubveri-wasm` — so a web tool needs no
server and no JVM. It returns one envelope `inputs[i]` object, fully typed
(a real `.d.ts` ships in the package).

**As of 0.10.0 it carries the whole `data` slot** — `element_path`, `namespaces`,
`advisory_basis` and `violation_kind` included. Through 0.9.x it carried `params`
alone, so everything under "what only machines can see" above reached a CLI
consumer and never a browser one.

One shape difference remains, and it is the kind that fails quietly:
**`data.namespaces` arrives as a JavaScript `Map`, not a plain object**, because
that is how a Rust map crosses the boundary. Use `data.namespaces.get("opf")` —
`data.namespaces["opf"]` is silently `undefined`. See
[`epubveri-wasm/README.md`](../epubveri-wasm/README.md).

## If something here is wrong or missing

Open an issue at
[github.com/veripublica/epubveri/issues](https://github.com/veripublica/epubveri/issues).
An integration that breaks because this page was unclear is a bug in this page.
