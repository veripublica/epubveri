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

**The trap that creates, and it is not hypothetical.** If you gate `-u` on a
user preference and then show `summary` in your own results panel, a user with
usage display turned off reads *0 usages* — a true statement about the output
and a false one about their book. Fetch with `-u` always, keep the whole
report, and let the preference decide what your interface *draws*. Toggling it
then costs a repaint instead of a second validation run.

**The library is the one place that never filters.** `epubveri::validate_bytes`
returns every finding at every severity regardless of any flag, because a
consumer dispatching on findings below error severity would otherwise go
silently dark.

## What the envelope looks like

One JSON object per run. Trimmed from a real run:

```json
{
  "tool": "epubveri",
  "tool_version": "0.13.2+254ab72",
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

**`tool_version` is the version that produced this report — do not run
`epubveri -V` to find it.** Every envelope carries it, so a tool that already
parses the JSON has the answer without a second process launch and without
depending on the shape of a human-readable line we have never promised. It may
carry build metadata after a `+` (`0.13.2+254ab72`); split on `+` if you want
the bare version.

**Three fields you will touch first are specified in
[FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)
rather than here**, because they are the same for every tool in the family:
`inputs[].status` (`ok` | `problems` | `error`), `inputs[].error` (present only
with `status: "error"`), and `item.type` (`finding`, for a validator).

**Handle `status: "error"` separately from findings**, and note where the line
actually falls: it is not "could the file be read" but **"can we name the fault
with a code"**. A fault we can name is a *verdict* — the input is `"problems"`
and the finding sits in `items`, however severe, which is why a file that does
not exist arrives as a `PKG-018` fatal you can display like any other. `error`
is the residue: no code, no verdict, `items` empty, and the reason in
`inputs[].error` as plain prose for a person.

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
| `location` | **yes** | Path *inside* the EPUB container, directories included (`OEBPS/text/ch1.xhtml`). Use the whole path: two files in one book can share a basename, and reducing it to one sends the reader to the wrong file. |
| `rule` | **yes** | epubveri's own semantic sub-code. Prefer this when `code` is too coarse — `RSC-005` alone means dozens of unrelated things. |
| `position` | line yes, column may improve | 1-based line and column. Use it to place a cursor. |
| `message` | **no** | Display it; never match on it. |
| `data.params` | **yes** | The values interpolated into `message`, so you can re-render it. |
| `data.element_path` | **yes** | XPath-style path to the offending node, with `data.namespaces` giving the prefix bindings needed to resolve it. This is the robust way to find the thing a finding is about. |
| `data.violation_kind` | **yes** | On schema violations: which of six kinds of fault this is, without parsing the message. |
| `data.advisory_basis` | **yes** | On `ADV-*`/`NEXT-*` findings only: `spec-ahead` or `spec-silent`. See the note on `--advisory` below. |

**Why `rule` matters.** `RSC-005` is epubcheck's catch-all for "the document
does not match its schema", so keying behaviour on `code` alone puts a hundred
unrelated conditions in one bucket. `rule` splits them —
`opf.spine.missing_toc_epub2`, `opf.content_document.schema_violation` — and is
stable. It exists because a repair tool asked for it.

**Codes are not all punctuated the same way, and that is deliberate.** Most
are `RSC-005`, but seventeen use an underscore: `HTM_054`, `HTM_055`,
`HTM_056`, `HTM_057`, `HTM_058`, `HTM_059`, `HTM_061`, `MED_007`, and
`MED_010` through `MED_018`. That is a typo on nobody's side — epubcheck's own
`MessageId` declares them that way and prints `ERROR(HTM_061)`, and we mirror
it exactly so that a toolchain grepping either tool's output finds the same
string. **A consumer routing on a pattern therefore needs `[A-Z]+[-_][0-9]+`**;
matching only on `-` drops those seventeen silently. Comparing the whole string
literally is always safe.

**Why `element_path` matters more than `position`.** A position is a place in a
file and can be improved; a path identifies a *node* and does not move. If you
need to act on the thing a finding describes rather than just show it, resolve
the path.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Valid — no error- or fatal-severity findings. |
| `1` | At least one error or fatal. |
| `2` | An input could not be read, or the command line was wrong. |

`2` is about the *tool or the invocation*, not the book: a file that is
readable but broken — even one that is not a valid ZIP — still gets a verdict
and exit `1`. With several `-i` inputs the exit code is the worst across them.

**`2` does not mean "there is nothing to read", and assuming it does costs your
user the explanation.** There are three shapes, and only the first of them has
its answer on stderr:

| What happened | stdout | stderr | where the reason is |
|---|---|---|---|
| The command line was wrong | *empty* | `error: … (see --help)` | stderr |
| The file does not exist | full envelope | *empty* | a `PKG-018` fatal finding, with `status: "problems"` |
| The input could not be processed and the fault has no code — a directory, an I/O failure | full envelope | *empty* | `inputs[].error`, with `status: "error"` |

**This is the one place our exit codes differ from epubcheck's, deliberately.**
epubcheck uses `0` and `1` only, so a missing file, a directory and a mistyped
flag all come back as `1` — indistinguishable from an invalid book. We keep a
third code so that a CI job can tell *"this book is invalid"* from *"your
invocation was wrong"*, which are different failures with different fixes. If
you are porting an invocation, treat any non-zero as failure and read the
envelope for the reason; do not assume `1` means the book was judged.

So the robust order is **parse stdout first, and fall back to stderr only when
there is no envelope to parse.** A tool that branches on the exit code and
prints stderr shows its user a blank message in the two commonest cases — a
missing file and a directory — both of which epubveri explains in full and in
words meant for a person.

**A note on `--advisory`.** It never changes the verdict or the exit code, by
permanent design, so you can offer it as a display option without worrying that
it will start failing your users' books. See [USAGE.md](USAGE.md#the-two-switches-and-what-is-on-by-default).

**Two families arrive under that flag and they make different claims**, so
separate them if you surface them at all. `NEXT-*` is *spec-ahead*: a published
specification requires it and epubcheck has not implemented it yet, so it
becomes an ordinary error the day epubcheck catches up — worth showing to
someone preparing a book to last. `ADV-*` is *spec-silent*: no specification
says anything, but the book is still likely wrong, and these never become
errors. Key on the prefix, or read `data.advisory_basis`, which carries
`spec-ahead` or `spec-silent` so you need not know our prefix convention at
all.

## Showing findings to a person

Everything above is about what to parse. This is the other half, and it is
where most of the confused reports we receive actually come from: the same
finding reads as a defect or as noise depending only on how it is drawn.

**Severity is five values and most hosts have three.** `fatal`, `error`,
`warning`, `info`, `usage`. Feeding an editor's existing panel usually means
collapsing them — `fatal` onto the host's error level, `usage` onto its lowest.
**Keep the original word visible in the text when you do.** The collapse throws
away something the reader needs, and putting `FATAL` or `USAGE` at the front of
the line puts it back for free.

**`usage` is not a defect in the book.** It is an observation: a construct that
is valid but unusual, or a place where a tool could have done better. A reader
who meets one styled like an error concludes their book is broken, and that has
happened on the forum more than once. Give it its own colour and its own word.
Whether to show them at all by default is your call, and both answers are in
the wild: the two independently written plugins we have read default them
**off**, and the two we ship default them **on** with every line labelled and
each advisory carrying the sentence that keeps it from reading as the two
tools disagreeing. Hiding them is defensible; hiding them *silently* is not —
if a switch shortens the report, say so and say where the switch is, or a
quiet report is indistinguishable from a clean book.

**A `fatal` means findings are missing.** It stops whatever was being read —
one document, or the container itself — so nothing after it in that unit was
reported. A list of five findings after a fatal is not a list of five problems;
say so, or your user will fix five things and be puzzled that the next run is
no better.

**"Valid" means one specific thing:** no `error` and no `fatal`, which is
exactly what exit code `0` says. It is not a quality score, and a valid book can
still carry warnings and usages. Say "no errors", not "no problems".

**Show which version produced the report.** `tool_version`, one line, costs
nothing. When someone asks why a finding appeared this week and not last, that
string is the entire answer — and it is the first thing we will ask you for.

**Place the reader precisely, or say you cannot.** `location` is the full path
inside the container and `position` is a 1-based line and column. If a finding
has no `position`, tell the reader the position is unknown rather than sending
them to line 1 of the file.

## A worked example

Everything above in one piece: run the validator, tell a tool failure apart
from a broken book, and present the result. It is deliberately small enough to
read in one sitting — the shape is the point, not the features.

```python
#!/usr/bin/env python3
"""Minimal epubveri integration: run it, parse it, present it."""

import json
import subprocess
import sys

# `-u` so the report contains everything. Filter in the interface, not here.
BASE_CMD = ["--format", "json", "-u"]

# Worst first. A host with fewer levels collapses these, but keeps the
# original word in the text it shows.
ORDER = {"fatal": 0, "error": 1, "warning": 2, "info": 3, "usage": 4}

ADVISORY_TAG = {
    "spec-ahead": " [will become an error]",
    "spec-silent": " [advisory]",
}


def validate(binary, epub_path, advisory=False):
    """Return (tool_version, status, error, findings) for one book."""
    cmd = [binary, *BASE_CMD, "-i", epub_path]
    if advisory:
        cmd.append("--advisory")

    proc = subprocess.run(cmd, capture_output=True, timeout=300)

    # Parse stdout first, whatever the exit code. Exit 2 still carries a full
    # envelope for a missing or unreadable file, and it holds the explanation.
    try:
        envelope = json.loads(proc.stdout.decode("utf-8", "replace"))
    except json.JSONDecodeError:
        # No envelope at all: epubveri did not start, or the command line was
        # wrong. This is the only case where stderr is the answer.
        raise RuntimeError(
            proc.stderr.decode("utf-8", "replace").strip()
            or f"epubveri exited with {proc.returncode} and said nothing"
        )

    book = envelope["inputs"][0]

    # status "error" is the residue: a fault we could not name with a code,
    # so there is no verdict and `items` is empty. The reason is in `error`.
    return (
        envelope["tool_version"],
        book["status"],
        book.get("error"),
        book.get("items", []),
    )


def render(finding):
    where = finding.get("location") or "(no file)"
    position = finding.get("position")
    where += (f":{position['line']}:{position['column']}" if position
              else " (position unknown)")

    basis = finding.get("data", {}).get("advisory_basis")
    return "{:<7} {:<9} {}{}\n{:>8}{}".format(
        finding["severity"].upper(),      # keep the word after any collapse
        finding["code"],                  # may use '-' or '_'; never rewrite it
        where,
        ADVISORY_TAG.get(basis, ""),
        "",
        finding["message"],
    )


def main(binary, epub_path):
    version, status, error, findings = validate(binary, epub_path)
    print(f"epubveri {version.split('+')[0]}\n")

    if status == "error":
        print(f"could not read this file: {error}")
        return 2

    for finding in sorted(findings, key=lambda f: ORDER.get(f["severity"], 9)):
        print(render(finding))

    # "valid" is exactly: no error and no fatal. Not a quality score.
    verdict = "valid" if status == "ok" else "not valid"
    print(f"\n{verdict} — {len(findings)} finding(s)")

    if any(f["severity"] == "fatal" for f in findings):
        print("a fatal stopped something being read, so this list is "
              "incomplete — fix those first and run again")
    return 0 if status == "ok" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
```

## If you download the binary

Each `v*` tag publishes eight archives, named after the Rust target triple:

```
epubveri-x86_64-unknown-linux-gnu.tar.gz    epubveri-x86_64-apple-darwin.tar.gz
epubveri-x86_64-unknown-linux-musl.tar.gz   epubveri-aarch64-apple-darwin.tar.gz
epubveri-aarch64-unknown-linux-gnu.tar.gz   epubveri-x86_64-pc-windows-msvc.zip
epubveri-aarch64-unknown-linux-musl.tar.gz  epubveri-aarch64-pc-windows-msvc.zip
```

**These names are stable and will not be renamed.** If a target is ever added
it will be added alongside these, not substituted for one of them. Each archive
contains a single directory of the same name, holding the executable
(`epubveri`, or `epubveri.exe` on Windows) plus `LICENSE`,
`LICENSE-COMMERCIAL.md` and `README.md`.

**If you are choosing one archive per platform automatically, choose `musl`
on Linux.** The `musl` builds are static: they run on any distribution, with no
glibc version to match. A `gnu` build downloaded onto an older or non-glibc
system fails to start, and the message the user sees says nothing about why.
`gnu` is the better choice only when you know the machine you are installing
onto.

The whole mapping, from what a program can read about the machine it is
running on:

| OS | 64-bit Intel/AMD | 64-bit ARM |
|---|---|---|
| Linux | `epubveri-x86_64-unknown-linux-musl.tar.gz` | `epubveri-aarch64-unknown-linux-musl.tar.gz` |
| macOS | `epubveri-x86_64-apple-darwin.tar.gz` | `epubveri-aarch64-apple-darwin.tar.gz` |
| Windows | `epubveri-x86_64-pc-windows-msvc.zip` | `epubveri-aarch64-pc-windows-msvc.zip` |

There is no 32-bit build. If you cannot map the machine to a row, say so
plainly rather than guessing one — a wrong-architecture binary fails with an
error the user cannot act on.

### Checking that you got what we built

Every release since 0.12.4 carries a ninth asset, `SHA256SUMS.txt`, listing all
eight archives in `sha256sum` format:

```
sha256sum -c SHA256SUMS.txt --ignore-missing     # Linux
shasum -a 256 -c SHA256SUMS.txt --ignore-missing # macOS
```

`--ignore-missing` is what lets you check the one archive you downloaded
instead of all eight.

**Be clear about what that does and does not prove.** It proves the download
completed and that you fetched the file you meant to. It does not, on its own,
prove much about provenance: the checksum file sits next to the archives, so
whoever could replace one could replace the other. It is the offline,
no-tooling half.

The other half is a **signed build attestation**, recorded for every archive
from the same release onward:

```
gh attestation verify epubveri-x86_64-apple-darwin.tar.gz --repo veripublica/epubveri
```

That checks a signature made by GitHub's own OIDC identity for this repository
and workflow, so it says *which commit and which workflow produced this file* —
which a checksum published beside the file cannot. It needs the `gh` CLI and a
network connection.

**Why this exists, in one sentence, because it is worth knowing if you are
writing a plugin:** the usual reassurance for a small tool is "unzip it and
read the source", and that is exactly what does not work when the thing being
downloaded is a compiled binary. If your plugin fetches our binary on a user's
behalf, they never see it — so verify it for them.

### Verifying it from your own code

The commands above are for a person at a terminal. If your plugin downloads the
archive on a user's behalf, do the same check in code — `SHA256SUMS.txt` is a
release asset like any other, so it comes out of the same API response you
already have:

```python
import hashlib
import urllib.request


def asset_url(release, name):
    for a in release["assets"]:
        if a["name"] == name:
            return a["browser_download_url"]
    raise LookupError(name)


def fetch(url, timeout=180):
    with urllib.request.urlopen(url, timeout=timeout) as r:
        return r.read()


def download_verified(release, archive_name):
    """Return the archive's bytes, or raise if they are not what we published."""
    blob = fetch(asset_url(release, archive_name))
    sums = fetch(asset_url(release, "SHA256SUMS.txt")).decode("utf-8")

    want = None
    for line in sums.splitlines():          # "<sha256>  <filename>"
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == archive_name:
            want = parts[0]
    if want is None:
        raise LookupError(f"{archive_name} is not listed in SHA256SUMS.txt")

    got = hashlib.sha256(blob).hexdigest()
    if got != want:
        raise ValueError(f"checksum mismatch for {archive_name}")
    return blob
```

**Be honest with yourself about what those twenty lines buy**, for the reason
given above: both files come from the same host over the same connection, so
this is not protection against someone who controls that host. What it does
catch is a **truncated or corrupted download** — which otherwise installs as a
binary that fails in a way neither you nor your user can diagnose — and a
release in which one archive was replaced and the checksum file was not. That
is worth twenty lines. Releases before 0.12.4 carry no `SHA256SUMS.txt`; treat
its absence as *cannot verify*, not as failure.

### Three things that turn a bad network into a bad experience

Each costs about one line, and each is the difference between a plugin that
degrades and one that hangs.

- **Put a timeout on every call.** Python's `urlopen` and `urlretrieve` have
  none by default, so a stalled connection hangs your plugin — and with it the
  editor window it is running in — with no way out but killing the process.
- **Fall back to the binary you already have.** If an update check cannot reach
  GitHub, validating with the installed version is almost always better than
  refusing to validate. Mention the failed check quietly; do not block the work
  over it.
- **Expect to be rate-limited.** The GitHub API allows 60 unauthenticated
  requests an hour per IP address. One person never reaches that; an office
  behind one address can. A `403` is not a reason to re-download, and not a
  reason to fail loudly.

**Resolve the version through `releases/latest`**, not the first element of
`releases`. The list endpoint includes prereleases and drafts in publication
order; `releases/latest` excludes them, which is almost certainly what you
want. Do not assume a tag parses as three integers either — that is true today
and is not a promise.

## How often to update, and whether to update at all

**A validator is not an ordinary dependency, and following every release is
usually the wrong default.** New checks mean a book that was clean yesterday
can be flagged today without the author changing anything. That is the tool
working correctly, and it is still a bad surprise if it arrives unannounced in
the middle of someone's work.

So, in rough order of preference:

- **Pin a version** and move deliberately when you have a reason to.
- If you do update automatically, **check every few days at most** — a week is
  a perfectly reasonable interval. Releases here are frequent and most of them
  will not matter to your users.
- **Show which epubveri version produced a report.** When someone asks why a
  finding appeared, that single string answers it, and it costs you one line.
- Treat a *new* finding on an unchanged book as expected behaviour rather than
  as a bug — but if it looks wrong, please report it. A false positive is the
  report this project wants most.

## Or link the library directly

If your tool is Rust, skip the process boundary:

```toml
[dependencies]
epubveri = "0.13"
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

epubveri is pre-1.0, so breaking API changes land as minor bumps (`0.x.0`).
**Pin the minor version, and read the `Breaking` heading in
[CHANGELOG.md](../CHANGELOG.md) before moving it** — that file is the authority
on what changed, and this page will not list them.

They have all been of one kind so far: a new field on `report::Message`, or a
public enum gaining a variant or becoming a struct variant, so a struct literal
or an exhaustive `match` needs a line. A consumer that only reads a `Report` —
which is most of them — has generally not had to touch anything.

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
