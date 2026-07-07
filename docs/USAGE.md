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
| **Windows** | `epubveri-x86_64-pc-windows-msvc.zip` |
| **Linux** | `epubveri-x86_64-unknown-linux-gnu.tar.gz` |

> Not sure which Mac you have? Click the Apple menu → **About This Mac**.
> If the chip says "Apple", pick Apple Silicon; if it says "Intel", pick
> Intel.

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

Then type the program's location, a space, and the book's location. The
easiest way to avoid typing long paths is to **drag the file into the
terminal window** — it fills in the full path for you:

```sh
# Type the program name (or drag the epubveri file in), a space,
# then drag your .epub file in, and press Enter:
~/Downloads/epubveri  ~/Desktop/my-book.epub
```

On Windows it looks like this (from PowerShell):

```powershell
C:\Users\you\Downloads\epubveri.exe  C:\Users\you\Desktop\my-book.epub
```

That's it. To see all options at any time, run `epubveri --help`.

---

## Understanding the results

A typical run looks like this:

```
ERROR OPF-002: OPF package document not found: OEBPS/content.opf
ERROR RSC-005: spine references manifest item id 'ch1' more than once [OEBPS/content.opf:15:5]
— 2 error(s), 0 warning(s): INVALID
```

Reading a line from left to right:

- **`ERROR` / `WARNING` / `INFO`** — how serious it is. Only **errors**
  make a book invalid; warnings and info are advisories.
- **`RSC-005`** — a short code identifying the kind of problem. These are
  the **same codes epubcheck uses**, so you can look any of them up in
  [epubcheck's message documentation](https://www.w3.org/publishing/epubcheck/docs/messages/)
  and existing tutorials still apply.
- **the message** — a plain-English description.
- **`[OEBPS/content.opf:15:5]`** — *where* it is: the file inside the EPUB,
  then the line and column. (A few kinds of check can't point at an exact
  line and show just the file name — that's normal.)

The last line is the summary and verdict: **VALID** or **INVALID**.

### The exit code (for scripting)

If you're calling epubveri from a script, it also returns a standard exit
code: **`0`** = valid, **`1`** = at least one error was found, **`2`** =
something went wrong reading the file (e.g. it isn't really a ZIP/EPUB).

---

## Handy options

**Just the codes** — for feeding into another tool or a script, print only
the list of message IDs:

```sh
epubveri --format ids my-book.epub
```

**Extension profiles** — if your book targets a specific EPUB extension,
you can additionally enforce its rules (same idea as epubcheck's
`--profile`). The available profiles are `dict` (Dictionaries &
Glossaries), `edupub` (EDUPUB), `idx` (Indexes), and `preview` (Previews):

```sh
epubveri --profile dict my-dictionary.epub
```

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
