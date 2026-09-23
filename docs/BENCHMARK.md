# epubveri vs epubcheck — performance

epubveri and epubcheck answer the same question about an EPUB. This document
measures what each one costs to get there: time, CPU, memory and disk.

**Measured 2026-09-23.** Every number here is an observation made on one machine
with one library on one day, not a property of either tool. Re-measure before
quoting any of it — the last section tells you how.

This is a **performance** document only. For what the two tools *find*, see
[`COVERAGE.md`](COVERAGE.md).

## Setup

| | |
|---|---|
| epubveri | 0.17.4, release build |
| epubcheck | 5.4.0, official distribution |
| JVM | OpenJDK 26.0.2, default heap settings |
| Machine | Apple M2 Pro, 10 cores, 32 GiB RAM, macOS 26.7 |
| Library | 474 real EPUBs, 685 MiB total — 390 EPUB 2, 74 EPUB 3, 5 declaring a 1.x version, 5 with a broken or absent version |
| Method | `/usr/bin/time -l` per book, inside one whole-library run per tool with the other tool not running |

Both tools were given `-u` so that usage-severity findings are reported by both,
and both wrote their output to `/dev/null`. What is being timed is validation,
not printing.

**epubveri's per-book times come from a high-resolution timer**, the median of
three runs per book, because `/usr/bin/time` *truncates* to 10 ms: it prints a
49 ms run as `0.04`, which would understate epubveri's typical book by a
fifth. The same truncation costs epubcheck's two-second runs under 0.5%, so its figures are
`/usr/bin/time`'s. Whole-library totals are one long interval each and are
unaffected.

### Both tools did the same work

A tool that validates less is not faster. On the same 474 books the two agree on
the verdict for **425**. All 49 books where they differ are Project Gutenberg
EPUB 3s that epubcheck 5.4.0 rejects over `aria-label` on their navigation
`nav`, a confirmed regression in that release
([w3c/epubcheck#1726](https://github.com/w3c/epubcheck/issues/1726)); **there is
no book epubveri rejects and epubcheck accepts.** A separate finding-by-finding comparison on
the same day found **identical message-ID sets on 423 of 474, with no ID
reported by epubveri alone**.

### What changed since the last measurement

The previous edition of this page (2026-08-25: epubveri 0.12.0, epubcheck
5.3.0, 385 books) found epubveri about **11x** faster over the whole library.
It is **18x** here. The library grew and epubcheck moved a version, so the two
figures are not a controlled experiment. One comparison was controlled: on this
same library, run back to back, epubveri 0.17.2 took 99 s and 0.17.3 took 58 s.
That release stopped its RELAX NG engine rebuilding each element's attribute
model for every attribute it read.

## Summary

Whole library, 474 books, each tool running alone:

| | epubveri | epubcheck | ratio |
|---|---:|---:|---|
| Wall-clock time | **55 s** | 967 s | **18x** |
| CPU time | **55 s** | 3 624 s | **66x** |
| Memory, typical book | **7.9 MiB** | 421 MiB | **53x** |
| Install footprint | **3.1 MB** | 418 MB | **135x** |

## Time

The same three situations, measured for both tools:

| | epubveri | epubcheck |
|---|---:|---:|
| One small book (74 KB) | **11 ms** | 1.83 s |
| A typical book (median of 474) | **0.05 s** | 1.96 s |
| The whole 474-book library | **55 s** | 967 s |

epubcheck's time barely depends on the book. Over a hundredfold range of
content — 100 KB to 10 MB, which is 98% of this library — its median moves by
6%:

| book size | books | epubcheck, median | epubveri, median |
|---|---:|---:|---:|
| under 100 KB | 3 | 1.82 s | 0.011 s |
| 100–500 KB | 198 | 1.90 s | 0.029 s |
| 0.5–2 MB | 201 | 2.01 s | 0.064 s |
| 2–10 MB | 66 | 2.02 s | 0.068 s |
| over 10 MB | 6 | 2.63 s | 0.344 s |

## CPU

| | epubveri | epubcheck |
|---|---:|---:|
| CPU-seconds, whole library | **55 s** | 3 624 s |
| Cores busy while running | 1.00 | 3.75 |

The CPU gap (66x) is almost four times the wall-clock gap (18x), and the reason
is measurable: epubcheck keeps about **3.75 cores** busy — JIT compiler and
garbage collector threads alongside the work — while epubveri is
single-threaded. So epubcheck recovers part of the wall-clock difference through
parallelism, but it takes that back from the rest of the machine.

Two places where the CPU number is the one that matters rather than the clock:
a CI or ingestion pipeline, where you want those cores for your own concurrency,
and battery life, which tracks CPU-seconds.

## Memory

| | epubveri | epubcheck |
|---|---:|---:|
| Typical book (median) | **7.9 MiB** | 421 MiB |
| Worst book | **92 MiB** | 1 684 MiB |
| Books needing over 512 MiB | **0** | 124 |
| Books needing over 1 GiB | **0** | 4 |

The tail matters more than the average here. In a 512 MB container or inside a
mobile application, epubcheck's typical book is already near the limit and a
quarter of this library is past it.

## Disk and deployment

| | size |
|---|---:|
| epubveri CLI — one file, no runtime needed | **3.1 MB** |
| epubveri release archive, per platform | 1.1–1.4 MB |
| epubveri WASM, for the browser | **534 KB** brotli (2.0 MB raw) |
| epubcheck distribution | 34.7 MB (`epubcheck.jar` plus 39 dependency jars) |
| — plus the required JVM | 383 MB |
| **epubcheck total** | **418 MB** |

In a browser there is nothing to compare: 534 KB over the wire against a JVM
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
| Validating the smallest book (74 KB) | 1 830 ms |

So it is not JVM startup, which is about 1% of a per-book run, and it is not
work proportional to the book either — the table above shows the time is nearly
flat with size. About **1.75 seconds is fixed setup performed inside the
validation path**, most likely compiling the RELAX NG and Schematron schemas and
the JIT warm-up over that work.

epubveri has no equivalent because its schemas are **compiled into the binary**.
Its process starts in 2 ms, its floor is 11 ms, and its time then grows with the
content.

This also states epubcheck's best case fairly. If that 1.75 seconds were shared
across many books, the two tools would be far closer — subtracting each tool's
floor, the remaining per-book work is roughly 0.13 s against 0.04 s. But the
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

For per-book figures, wrap each book individually and read `real`/`user`/`sys`
and `maximum resident set size`.

**Run the two tools in separate passes, not alternating.** Interleaving them
made the faster tool look about 40% slower here, because each epubcheck run left
behind winding-down JIT and GC threads and a page cache its ~460 MB working set
had just evicted.
