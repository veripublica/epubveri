# Getting started with epubveri

A beginner-friendly guide to checking an EPUB file with **epubveri** — no
prior command-line experience assumed, and **no need to install Rust or
anything else** if you use one of the ready-made options below.

epubveri looks at an `.epub` file and tells you whether it's valid, and if
not, exactly what's wrong and where. It's a faster, install-free
alternative to the official `epubcheck`.

---

## Pick the easiest option for you

| You want to… | Use this | Install needed |
|---|---|---|
| Just check one book, right now | [In your browser](#option-1-in-your-browser-nothing-to-install) | None |
| Check books regularly on your computer | [A downloaded program](#option-2-download-the-ready-to-run-program) | None (just download) |
| Use it inside your own code | [The library / build from source](#option-3-for-developers) | Rust |

---

## Option 1: In your browser (nothing to install)

Open **<https://veripublica.github.io/epubveri/>** and drag your `.epub`
onto the page. It runs entirely on your own machine (the file is never
uploaded anywhere) and shows the same results as the program. This is the
quickest way to try it.

---

## Option 2: Download the ready-to-run program

### Step 1 — Download the right file

Go to the **[latest release](https://github.com/veripublica/epubveri/releases/latest)**
and download the one archive that matches your computer:

| Your computer | File to download |
|---|---|
| **Mac** with Apple Silicon (M1/M2/M3/M4) | `epubveri-aarch64-apple-darwin.tar.gz` |
| **Mac** with an Intel chip (older Macs) | `epubveri-x86_64-apple-darwin.tar.gz` |
| **Windows** (almost all PCs) | `epubveri-x86_64-pc-windows-msvc.zip` |
| **Windows on ARM** (e.g. Surface Pro X, Snapdragon laptops) | `epubveri-aarch64-pc-windows-msvc.zip` |
| **Linux** (Intel/AMD — most PCs and servers) | `epubveri-x86_64-unknown-linux-musl.tar.gz` |
| **Linux on ARM** (Raspberry Pi, ARM servers) | `epubveri-aarch64-unknown-linux-musl.tar.gz` |

> **Ignore the two "Source code" entries at the bottom of that page.** GitHub
> adds them to every release automatically; they contain the program's source
> text, not the program, and nothing in them will run. The file you want is
> one of the eight named above, and its name starts with `epubveri-`.
>
> There is also a small `SHA256SUMS.txt`. You do not need it to run epubveri —
> it is there so anyone who wants to can check their download is intact and
> unaltered. `docs/INTEGRATING.md` explains how, and what it does and does not
> prove.

> **Not sure which Mac you have?** Click the Apple menu → **About This Mac**.
> If the chip says "Apple", pick Apple Silicon; if it says "Intel", pick
> Intel.
>
> **Not sure about Windows?** Pick the regular **Windows** file — nearly all
> PCs are that kind. Only choose *Windows on ARM* if you know your laptop
> uses an ARM/Snapdragon chip.
>
> **Which Linux file?** The ones listed above are **`musl` builds** — a
> single self-contained program with no dependencies that runs on any Linux
> distribution (including Alpine and older systems). If you specifically
> prefer a dynamically-linked build, a `…-linux-gnu.tar.gz` is also published
> for each architecture on the same release page; the `musl` one is the safe
> default.

### Step 2 — Unpack it

Double-click the downloaded archive to unpack it. Inside you'll find a
single program file named `epubveri` (or `epubveri.exe` on Windows). Put
it somewhere easy to find, such as your **Downloads** or **Desktop**
folder.

### Step 3 — Let it run the first time

Because this is a small independent project (not signed with a paid
Apple/Microsoft developer certificate), your system may warn you the first
time. This is expected — here's how to get past it:

- **macOS** — Opening it may say *"epubveri cannot be opened because Apple
  cannot check it for malicious software."* Either:
  - Open the **Terminal** app and run this once (adjust the path to where
    you put it), which clears the quarantine flag:
    ```sh
    xattr -d com.apple.quarantine ~/Downloads/epubveri
    ```
  - …or go to **System Settings → Privacy & Security**, scroll down, and
    click **Allow Anyway** next to the epubveri message, then try again.

- **Windows** — If you see *"Windows protected your PC"*, click **More
  info → Run anyway**. (You can also right-click the file → **Properties**
  → check **Unblock** → **OK**.)

- **Linux** — Mark it as executable once, in a terminal:
  ```sh
  chmod +x ~/Downloads/epubveri
  ```

### Step 4 — Run it

epubveri is a command-line tool, so you run it from a terminal:

- **macOS**: open the **Terminal** app (Applications → Utilities, or search
  "Terminal" in Spotlight).
- **Windows**: open **PowerShell** or **Command Prompt** (search for it in
  the Start menu).
- **Linux**: open your terminal.

Then type the program's location, ` -i ` (the book is always passed with
`-i`), and the book's location. The easiest way to avoid typing long paths is
to **drag the file into the terminal window** — it fills in the full path for
you:

```sh
# Type the program name (or drag the epubveri file in), then type " -i ",
# then drag your .epub file in, and press Enter:
~/Downloads/epubveri -i ~/Desktop/my-book.epub
```

On Windows it looks like this (from PowerShell):

```powershell
C:\Users\you\Downloads\epubveri.exe -i C:\Users\you\Desktop\my-book.epub
```

That's it. To see all options at any time, run `epubveri --help`.

---

## Understanding the results

A typical run looks like this:

```
ERROR RSC-005: EPUB 2 <spine> is missing the required 'toc' (NCX) attribute [OEBPS/content.opf:8:3]
— 1 error(s), 0 warning(s): INVALID
```

Reading a line from left to right:

- **`ERROR`** — how serious it is. See the table below.
- **`RSC-005`** — a short code identifying the kind of problem. These are
  the **same codes epubcheck uses**, so you can look any of them up in
  [epubcheck's message documentation](https://www.w3.org/publishing/epubcheck/docs/messages/)
  and existing tutorials still apply.
- **the message** — a plain-English description.
- **`[OEBPS/content.opf:8:3]`** — *where* it is: the file inside the EPUB,
  then the line and column. (A few kinds of check can't point at an exact
  line and show just the file name — that's normal.)

The last line is the summary and verdict: **VALID** or **INVALID**.

### How serious is it? The five levels

epubveri uses the same five levels epubcheck does. Two of them decide the
verdict; the rest are things worth knowing that do **not** fail your book.

| Level | Makes the book invalid? | Shown by default? | What it means |
|---|---|---|---|
| `FATAL` | **yes** | yes | Something stopped the check partway — a file that isn't valid XML, say. |
| `ERROR` | **yes** | yes | A real rule is broken. |
| `WARNING` | no | yes | Allowed, but very likely not what you meant. |
| `INFO` | no | yes | A neutral fact about the book. |
| `USAGE` | no | **no — use `-u`** | Names a feature the book *uses*. Nothing is wrong. |

**`USAGE` is hidden unless you ask for it, exactly as in epubcheck.** These
lines describe correct content — an `@font-face` declaration, a file in the
container that the manifest doesn't list — and reading them as problems is a
mistake the tool shouldn't invite. Ask for them with `-u` (or `--usage`) when
you want the full picture:

```sh
epubveri -u -i my-book.epub
```

### The two switches, and what is on by default

This is the part that trips people up, so here it is in one place. epubveri
has exactly two switches that change *which findings you see* — and **neither
changes the verdict**:

| | Default | What it adds |
|---|---|---|
| `-u`, `--usage` | **off** | The `USAGE` lines described above. |
| `--advisory` | **off** | Extra opinions epubcheck does not hold, in two families: `NEXT-*` (a specification requires it and epubcheck hasn't implemented it yet) and `ADV-*` (no specification says anything, but the book is still probably wrong). |

Everything else you see is a finding epubcheck would report too. **A book that
passes epubcheck passes epubveri**, with or without either switch — `--advisory`
findings never affect `VALID`/`INVALID` or the exit code, by permanent design.

### The order the findings come in

By default the report is **grouped by severity, most serious first** — fatals,
then errors, then warnings, then info — because that is the order most people
work in: fix what makes the book invalid, run it again, then look at the rest.
Inside each group the findings stay in file order, so each group still reads
top-to-bottom.

If you would rather walk the book once, front to back, whatever the severities:

```sh
epubveri --sort document -i my-book.epub
```

Both orders contain exactly the same findings; only the arrangement differs, and
neither changes the verdict.

### The exit code (for scripting)

If you're calling epubveri from a script, it also returns a standard exit
code: **`0`** = valid, **`1`** = at least one error or fatal was found, **`2`**
= the tool couldn't run or couldn't read an input at all (a missing or
unreadable file). A file that *is* readable but broken — even one that isn't a
valid ZIP — still gets a verdict (a `FATAL` finding, exit `1`), not exit `2`.

With more than one `-i`, epubveri validates every book and reports on each; the
exit code is the worst across them.

---

## Handy options

**Just the codes** — for feeding into another tool or a script, print only
the list of message IDs:

```sh
epubveri --format ids -i my-book.epub
```

**Machine-readable output** — for a tool, a CI job, or another program, print
the shared JSON envelope (one object; the browser demo can save the same file):

```sh
epubveri --format json -i my-book.epub
```

> **Writing a plugin or a tool around epubveri?** Read
> **[INTEGRATING.md](INTEGRATING.md)** first. Short version: parse
> `--format json`, never the human output. The JSON is a documented, stable
> contract, while the human text is free to change wording, order and spacing
> between releases. Note that `-u` decides what the JSON contains too, so pass
> it if your tool wants everything — and filter in your own interface, or the
> summary you show will describe your flags rather than the book.

**A different order** — group by severity (the default) or walk the book in
file order:

```sh
epubveri --sort document -i my-book.epub
```

**Extension profiles** — if your book targets a specific EPUB extension,
you can additionally enforce its rules (same idea as epubcheck's
`--profile`). The available profiles are `dict` (Dictionaries &
Glossaries), `edupub` (EDUPUB), `idx` (Indexes), and `preview` (Previews):

```sh
epubveri --profile dict -i my-dictionary.epub
```

**Checking against a specific EPUB version** — normally epubveri judges a
book as whatever version its package document declares, which is almost
always what you want. If you need to ask "would this pass as EPUB 2?", say so
with `-v` (the same flag epubcheck uses):

```sh
epubveri -v 2.0 -i my-book.epub
```

On a disagreement you get a `PKG-001` warning and the version you asked for
wins — so checking an EPUB 3 book as 2.0 produces a long list of complaints
that are really all one complaint ("this isn't an EPUB 2 book"). Note that
`-v` takes a value and `-V` prints epubveri's own version; they are one
letter apart because epubcheck's flag is `-v`.

---

## Getting help or reporting a problem

- Run `epubveri --help` for the full list of options.
- If epubveri reports an error on a book you believe is valid (or misses
  one it should catch), please open an issue at
  <https://github.com/veripublica/epubveri/issues> — ideally with the
  message it printed and, if you can share it, the file. Reports like that
  are how the tool improves.

For developers who want to embed epubveri or build it from source, see the
[README](../README.md) and [ARCHITECTURE.md](./ARCHITECTURE.md).
