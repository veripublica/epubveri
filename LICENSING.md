# Licensing, in plain terms

`epubveri` is dual-licensed: **AGPL-3.0-only** ([`LICENSE`](./LICENSE)) **OR** a
commercial license ([`LICENSE-COMMERCIAL.md`](./LICENSE-COMMERCIAL.md)).

This page answers the questions people actually ask before adopting it. It is
short on purpose. **The full answers — including the GPL-compatibility
reasoning, clause by clause — live in one shared FAQ covering both `epubveri`
and its sibling `epubsana`:**

**→ [Licensing FAQ](https://github.com/veripublica/epubsana/blob/main/LICENSING-FAQ.md)**

There is deliberately only one canonical text, so the two projects cannot drift
into saying different things. This page does not repeat its legal reasoning; it
states the few things that are specific to `epubveri`, plus the commitment
behind all of it.

---

## Are the books I check covered by your license?

**No. Unconditionally.**

For `epubveri` the question does not even arise, and this is the one answer
that is simpler here than for any repair tool: **`epubveri` never writes to
your book.** It opens an EPUB, reads it, and reports what it found. There is no
code path by which any part of `epubveri` can end up inside an EPUB.

Your books are yours. Sell them, ship them to a retailer, give them away — the
license governs this software, not the data it reads.

## I check books commercially. Do I need the commercial license?

**No.** Running `epubveri` — on your own books, on customers' books, inside
your company, as often as you like, for money — needs no commercial license and
no permission. There is no hobbyist/commercial distinction in how you may *use*
it.

## What is the commercial license actually for?

Two narrow cases, both about **distributing or serving this code** rather than
using it:

1. **Embedding `epubveri` in a closed-source product you distribute** — an
   e-reader, an editor, a retailer's ingestion pipeline — without meeting the
   AGPL's source-disclosure obligations.
2. **Running a modified version as a network service** without publishing your
   modifications.

Nothing else. If you are not doing one of those two things, the AGPL is free
and sufficient.

## Can a Sigil or calibre plugin use `epubveri`? Can GPL-3.0 software?

Yes to both — but the reasoning is a licence-compatibility argument, and it
belongs in exactly one place rather than two.
**See the [Licensing FAQ](https://github.com/veripublica/epubsana/blob/main/LICENSING-FAQ.md)**,
which walks through GPLv3 §13 and AGPLv3 §13 and quotes both.

## Contributing

External contributions are not being accepted yet — see
[`CONTRIBUTING.md`](./CONTRIBUTING.md). Selling commercial licenses requires the
copyright holder to hold full copyright, so a CLA has to exist before any
external code can be merged. That mechanism is not built yet.

---

## The commitment, and the disclaimer

I am not a lawyer, and nothing here is legal advice. Where this page and
[`LICENSE`](./LICENSE) could be read differently, **`LICENSE` governs.**

What is *not* hedged is the commitment itself, which I can make plainly as the
copyright holder: **I will not come after users, plugin authors, or editor
projects.** The AGPL is here because work of mine was once closed and
commercialised by someone else and I got nothing back. It is aimed at that, and
at nothing that a person checking their own books is doing.

Questions: baris@kayadelen.com
