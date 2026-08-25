# epubveri vs epubcheck — performance

epubveri and epubcheck answer the same question about an EPUB. This document
measures what each one costs to get there: time, CPU, memory and disk.

**Measured 2026-08-25.** Every number here is an observation made on one machine
with one library on one day, not a property of either tool. Re-measure before
quoting any of it — the last section tells you how.

This is a **performance** document only. For what the two tools *find*, see
[`COVERAGE.md`](COVERAGE.md).

## Setup

| | |
|---|---|
| epubveri | 0.12.0, release build |
| epubcheck | 5.3.0, official distribution |
| JVM | OpenJDK 26.0.2, default heap settings |
| Machine | Apple M2 Pro, 10 cores, 32 GiB RAM, macOS 26.6.2 |
| Library | 385 real EPUBs, 611 MiB total — 307 EPUB 2, 69 EPUB 3, 9 with a broken or absent version |
| Method | `/usr/bin/time -l` per book, plus one whole-library run per tool with the other tool not running |

Both tools were given `-u` so that usage-severity findings are reported by both,
and both wrote their output to `/dev/null`. What is being timed is validation,
not printing.

### Both tools did the same work

A tool that validates less is not faster. On the same 385 books the two agree on
the verdict for **384**, and a separate finding-by-finding comparison run on
2026-08-21 found **identical message-ID sets on 383 of 385, with no ID reported
by epubveri alone**.

## Summary

Whole library, 385 books, each tool running alone:

| | epubveri | epubcheck | ratio |
|---|---:|---:|---|
| Wall-clock time | **69 s** | 758 s | **11x** |
| CPU time | **68 s** | 2 875 s | **42x** |
| Memory, typical book | **7.4 MiB** | 415 MiB | **56x** |
| Install footprint | **2.8 MB** | 434 MB | **156x** |

## Time

The same three situations, measured for both tools:

| | epubveri | epubcheck |
|---|---:|---:|
| One small book (78 KB) | **9 ms** | 1.76 s |
| A typical book (median of 385) | **0.08 s** | 1.89 s |
| The whole 385-book library | **69 s** | 758 s |

epubcheck's time barely depends on the book. Over a hundredfold range of
content — 100 KB to 10 MB, which is 97% of this library — it moves by 13%:

| book size | epubcheck, median |
|---|---:|
| under 100 KB | 1.77 s |
| 100–500 KB | 1.84 s |
| 0.5–2 MB | 1.93 s |
| 2–10 MB | 2.00 s |
| over 10 MB | 2.54 s |

## CPU

| | epubveri | epubcheck |
|---|---:|---:|
| CPU-seconds, whole library | **68 s** | 2 875 s |
| Cores busy while running | 0.99 | 3.79 |

The CPU gap (42x) is four times the wall-clock gap (11x), and the reason is
measurable: epubcheck keeps about **3.79 cores** busy — JIT compiler and garbage
collector threads alongside the work — while epubveri is single-threaded. So
epubcheck recovers part of the wall-clock difference through parallelism, but it
takes that back from the rest of the machine.

Two places where the CPU number is the one that matters rather than the clock:
a CI or ingestion pipeline, where you want those cores for your own concurrency,
and battery life, which tracks CPU-seconds.

## Memory

| | epubveri | epubcheck |
|---|---:|---:|
| Typical book (median) | **7.4 MiB** | 415 MiB |
| Worst book | **90 MiB** | 1 687 MiB |
| Books needing over 512 MiB | **0** | 94 |
| Books needing over 1 GiB | **0** | 6 |

The tail matters more than the average here. In a 512 MB container or inside a
mobile application, epubcheck's typical book is already at the limit and a
quarter of this library is past it.

## Disk and deployment

| | size |
|---|---:|
| epubveri CLI — one file, no runtime needed | **2.8 MB** |
| epubveri release archive, per platform | 1.0–1.3 MB |
| epubveri WASM, for the browser | **477 KB** brotli (1.8 MB raw) |
| epubcheck distribution | 36.4 MB (`epubcheck.jar` plus 39 dependency jars) |
| — plus the required JVM | 398 MB |
| **epubcheck total** | **434 MB** |

In a browser there is nothing to compare: 477 KB over the wire against a JVM
that is not an option but a prerequisite.

Disk traffic during validation was not a differentiator — with a warm page cache
neither tool reached the disk, and on a cold cache both must read the same book.

## Where the difference comes from

Two common explanations for epubcheck's per-book cost are both wrong, and each
part was measured separately:

| | |
|---|---:|
| Starting a bare JVM (`java --version`) | 22 ms |
| Loading epubcheck's classes (`epubcheck --version`) | 65 ms |
| Validating the smallest book (78 KB) | 1 758 ms |

So it is not JVM startup, which is about 1% of a per-book run, and it is not
work proportional to the book either — the table above shows the time is nearly
flat with size. About **1.7 seconds is fixed setup performed inside the
validation path**, most likely compiling the RELAX NG and Schematron schemas and
the JIT warm-up over that work.

epubveri has no equivalent because its schemas are **compiled into the binary**.
Its floor is 9 ms, and its time then grows with the content.

This also states epubcheck's best case fairly. If that 1.7 seconds were shared
across many books, the two tools would be far closer — subtracting each tool's
floor, the remaining per-book work is roughly 0.13 s against 0.07 s. But the
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
export JAR=/path/to/epubcheck-5.3.0/epubcheck.jar
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
