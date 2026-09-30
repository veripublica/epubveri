# epubveri vs epubcheck — performance

epubveri and epubcheck answer the same question about an EPUB. This document
measures what each one costs to get there: time, CPU, memory and disk.

**Measured 2026-09-30, both tools in one sitting, same machine, same books.**
Every number here is an observation made on one machine with one library on one
day, not a property of either tool. Re-measure before quoting any of it — the
last section tells you how.

This is a **performance** document only. For what the two tools *find*, see
[`COVERAGE.md`](COVERAGE.md).

## Setup

| | |
|---|---|
| epubveri | 0.19.0, release build of the tagged commit |
| epubcheck | 5.4.0, official distribution |
| JVM | OpenJDK 26.0.2, default heap settings |
| Machine | Apple M2 Pro, 10 cores, 32 GiB RAM, macOS 27.0.1 |
| Library | 544 real EPUBs, 763 MiB total — 463 EPUB 2, 75 EPUB 3, 6 declaring a 1.x version |
| Method | one process per book, timed from outside, inside one whole-library pass per tool with the other tool not running |

Both tools were given `-u` so that usage-severity findings are reported by both,
and both wrote their output to `/dev/null`. What is being timed is validation,
not printing.

**Every per-book figure, for both tools, comes from the same harness**: a
high-resolution clock around each process for wall time, and the kernel's own
accounting for that one process (`wait4`) for CPU time and peak memory. Not
`/usr/bin/time`, which *truncates* to 10 ms: it prints a 25 ms run as `0.02`,
which would understate epubveri's typical book by a fifth. Whole-library totals
are the sum of the per-book times. epubveri's pass was run twice and the second
is reported (the first came to 19.4 s). Both tools ran on a warm page cache.

### Both tools did the same work

A tool that validates less is not faster. On the same 544 books the two agree on
the verdict for **495**. All 49 books where they differ are Project Gutenberg
EPUB 3s that epubcheck 5.4.0 rejects over `aria-label` on their navigation
`nav`, a confirmed regression in that release
([w3c/epubcheck#1726](https://github.com/w3c/epubcheck/issues/1726)); **there is
no book epubveri rejects and epubcheck accepts.** A separate finding-by-finding
comparison finds **identical message-ID sets on 494 of 544, with no ID reported
by epubveri alone**.

## Summary

Whole library, 544 books, each tool running alone:

| | epubveri | epubcheck | ratio |
|---|---:|---:|---|
| Wall-clock time | **19.1 s** | 1 100 s | **58x** |
| CPU time | **18.4 s** | 4 133 s | **225x** |
| Memory, typical book | **9.2 MiB** | 423 MiB | **46x** |
| Install footprint | **3.3 MB** | 434 MB | **131x** |

**epubveri was the faster tool and the smaller one on every one of the 544
books.** The narrowest per-book margins were 4x on time and 7x on memory.

## Time

The same three situations, measured for both tools:

| | epubveri | epubcheck |
|---|---:|---:|
| The smallest book (66 KB) | **7 ms** | 1.87 s |
| A typical book (median of 544) | **0.023 s** | 1.97 s |
| The whole 544-book library | **19.1 s** | 1 100 s |

epubcheck's time barely depends on the book. Over a hundredfold range of
content — 100 KB to 10 MB, which is 98% of this library — its median moves by
6%:

| book size | books | epubcheck, median | epubveri, median |
|---|---:|---:|---:|
| under 100 KB | 4 | 1.81 s | 0.008 s |
| 100–500 KB | 231 | 1.92 s | 0.016 s |
| 0.5–2 MB | 223 | 2.01 s | 0.029 s |
| 2–10 MB | 80 | 2.04 s | 0.029 s |
| over 10 MB | 6 | 2.66 s | 0.103 s |

## CPU

| | epubveri | epubcheck |
|---|---:|---:|
| CPU-seconds, whole library | **18.4 s** | 4 133 s |
| Cores busy while running | 0.96 | 3.76 |

The CPU gap (225x) is almost four times the wall-clock gap (58x), and the reason
is measurable: epubcheck keeps about **3.8 cores** busy — JIT compiler and
garbage collector threads alongside the work — while epubveri is
single-threaded. So epubcheck recovers part of the wall-clock difference through
parallelism, but it takes that back from the rest of the machine.

Two places where the CPU number is the one that matters rather than the clock:
a CI or ingestion pipeline, where you want those cores for your own concurrency,
and battery life, which tracks CPU-seconds.

## Memory

| | epubveri | epubcheck |
|---|---:|---:|
| Typical book (median) | **9.2 MiB** | 423 MiB |
| Worst book | **96 MiB** | 1 684 MiB |
| Books needing over 512 MiB | **0** | 138 |
| Books needing over 1 GiB | **0** | 4 |

The tail matters more than the average here. In a 512 MB container or inside a
mobile application, epubcheck's typical book is already near the limit and a
quarter of this library is past it.

Part of epubveri's figure is deliberate: it keeps each decompressed file, up to
a fixed 64 MiB, instead of decompressing it again, which costs a few MiB and
saves time.

## Disk and deployment

| | size |
|---|---:|
| epubveri CLI — one file, no runtime needed | **3.3 MB** |
| epubveri release archive, per platform | 1.2–1.5 MB |
| epubveri WASM, for the browser | **561 KB** brotli (2.1 MB raw) |
| epubcheck distribution | 36.2 MB (`epubcheck.jar` plus 39 dependency jars) |
| — plus the required JVM | 398 MB |
| **epubcheck total** | **434 MB** |

In a browser there is nothing to compare: 561 KB over the wire against a JVM
that is not an option but a prerequisite.

Disk traffic during validation was not a differentiator — with a warm page cache
neither tool reached the disk, and on a cold cache both must read the same book.

## Where the difference comes from

Two common explanations for epubcheck's per-book cost are both wrong, and each
part was measured separately:

| | |
|---|---:|
| Starting a bare JVM (`java --version`) | 22 ms |
| Loading epubcheck's classes (`epubcheck --version`) | 66 ms |
| Validating the smallest book (66 KB) | 1 870 ms |

So it is not JVM startup, which is about 1% of a per-book run, and it is not
work proportional to the book either — the table above shows the time is nearly
flat with size. About **1.8 seconds is fixed setup performed inside the
validation path**, most likely compiling the RELAX NG and Schematron schemas and
the JIT warm-up over that work.

epubveri has no equivalent because its schemas are **compiled into the binary**.
Its process starts in 3 ms, its floor is 7 ms, and its time then grows with the
content.

This also states epubcheck's best case fairly. If that 1.8 seconds were shared
across many books, the two tools would be far closer — subtracting each tool's
floor, the remaining per-book work is roughly 0.10 s against 0.02 s. But the
epubcheck command line takes **one file per invocation**, so the fixed cost is
paid again for every book. Its Java API could amortise it; its CLI cannot.

## Limits

- **This is not a correctness comparison.** See [`COVERAGE.md`](COVERAGE.md).
- **The two cost profiles differ in shape, not just in size.** epubcheck's
  per-book cost is high but very predictable. epubveri's is near zero for most
  books and grows with the content, so an unusual book can cost it much more
  than an average one.
- **One machine, one library, one day.** The library is mostly Turkish trade
  titles, Calibre output and Project Gutenberg. A number measured here is a fact
  about this library, not about every EPUB.
- **epubcheck ran with default JVM settings.** Constraining `-Xmx` would lower
  its peak memory; the effect on time was not measured.
- **Nothing here was measured under parallelism.** Both tools were run one book
  at a time.

## Reproducing this

```bash
# Release build. The schemas are embedded at compile time, so --workspace matters.
cargo build --release --workspace

export EV="$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/release/epubveri"
export JAR=/path/to/epubcheck-5.4.0/epubcheck.jar
export SHELF=/path/to/your/epub/library

# Whole library, epubveri
/usr/bin/time -l bash -c 'find "$SHELF" -name "*.epub" \
  | while IFS= read -r b; do "$EV" -u -i "$b"; done' >/dev/null

# Whole library, epubcheck. Without -u the comparison is unfair: epubcheck
# hides usage findings by default and much of epubveri's output is usage.
/usr/bin/time -l bash -c 'find "$SHELF" -name "*.epub" \
  | while IFS= read -r b; do java -jar "$JAR" -u "$b"; done' >/dev/null
```

These two give whole-library wall and CPU time. For per-book figures, run each
book as its own process and read that process's resource usage when it exits
(`os.wait4` in Python returns the CPU time and peak RSS for exactly
that child); `/usr/bin/time -l` per book works too, but its 10 ms truncation
understates epubveri's typical book by about a fifth.

**Run the two tools in separate passes, not alternating.** Interleaving them
made the faster tool look about 40% slower here, because each epubcheck run left
behind winding-down JIT and GC threads and a page cache its ~460 MB working set
had just evicted.
