# epubveri — Project Handoff / Bootstrap

> **For a fresh Claude Code session:** Read this file first. It carries the full
> context of the `epubveri` project, which was scoped during sessions on the
> `epublift` project and is now being spun out into its own folder/repo. Treat
> the decisions below as **already made** (don't relitigate unless asked).
> After reading, the natural first task is the "coverage spike" (see bottom).

---

## What epubveri is

A **standalone, pure-Rust EPUB validator** — a small, fast, **JVM-free,
embeddable** alternative to **epubcheck** (the official W3C EPUB validator,
written in Java → clunky, needs a JVM).

- **Consumers:** EPUB producers (tools / retailers / sites) embed it in their
  web/apps; `epublift` (a separate sibling project) also consumes it to validate
  its own output via something like `epublift check`.
- **Positioning:** a **foundational product**, NOT a sub-module of epublift.
  epubcheck is industry infrastructure (every publisher/retailer ingestion
  pipeline uses it) — epubveri may grow bigger in reach than epublift itself.
- **Distribution (all named `epubveri`, availability verified free 2026-06-27):**
  - crates.io library (`epubveri`)
  - thin CLI binary (`epubveri`)
  - **WASM** package for browser/app embedding, no JVM (likely `epubveri-wasm`) —
    a real differentiator vs Java epubcheck.

## Why it's a separate repo & org (not inside epublift, not under the ePubLift org)

Decided 2026-06-27. The earlier idea was to start as a crate inside epublift's
workspace and split later — that predated the license decision. Separating from
day one is now the better call because:

1. **License cleanliness (decisive).** epublift is pure **AGPL-3.0**; epubveri is
   **dual AGPL-OR-commercial with a CLA**. Keeping a commercially-sold, copyright-
   sensitive product out of the AGPL monorepo means the entire epubveri history is
   provably under the owner's copyright / CLA — no contamination. (The owner cares
   deeply about copyright provenance; see "Owner context" below.)
2. **Clean dependency direction:** epublift → depends on published `epubveri`
   crate. One direction, no cycle; enforces good architecture.
3. **A validator should parse independently and strictly** — it must NOT inherit a
   *producer's* lenient parser. So not sharing epublift's parsing code is a feature,
   not a loss.
4. Independent identity, versioning, CI, issue tracker — fits the "may outgrow
   epublift" positioning.
5. **Separate GitHub org — `veripublica`, NOT the ePubLift org.** epubveri is the
   first product of a distinct **house brand**, `veripublica`, that holds the
   owner's *dual-licensed / commercial / CLA-gated* products — as opposed to
   ePubLift (pure AGPL). The owner explicitly wants this independent from epublift,
   which is *why* its licensing differs. See the house-brand decision below.

During dev, if epublift needs epubveri before it's on crates.io, use a temporary
`path`/`git` Cargo dependency, then switch to the crates.io version once published.

---

## Locked decisions

### Name — product `epubveri`, house brand / org `veripublica`  (LOCKED 2026-06-27)
- **Repo:** `github.com/veripublica/epubveri` — under a **new, independent
  `veripublica` GitHub org**, NOT the ePubLift org.
- Same single word `epubveri` for crate + CLI binary + npm package. WASM pkg likely `epubveri-wasm`.
- **Why `epubveri`:** trademark strength — distinctive/coined (defensible mark) over
  generic descriptive names like `epubvalid` / `epublint` / `epubverify` / `epub-validator`.
  Still reads as "EPUB". The "veri" stem carries a **triple resonance**, all from Latin
  *verus* ("true"): **veri(fy/fier)** = exactly what a validator does · **veritas / verity**
  = truth · plus Turkish **veri** = "data". So it passes the epubcheck-style readability
  test ("epub-veri" reads as *epub verify*) **while staying coined / ownable**.
- **Why not a descriptive name like `epubverify`?** W3C can ship a descriptive `epubcheck`
  because it's *free industry infrastructure* with no need for a defensible mark; this
  product is *commercially licensed*, so it needs a distinctive/ownable mark. Descriptive
  = weakest trademark class. (Availability was a tie — `epubverify` is also free — so the
  decision rests purely on mark strength, where `epubveri` wins.)
- `epubcheck` is W3C's trademark — **must not be reused** or closely imitated.
- **Availability verified 2026-06-27** (both fully free): for `epubveri` *and* `veripublica`,
  `.com` + `.io` domains, GitHub org/user, npm, and crates.io were all open.
- **Reserved name — `veredictum`** (Latin *vere dictum* → "verdict"; a validator renders a
  verdict): parked for a possible **future, EPUB-external validation product** under the
  veripublica house. `epubdictum` was also considered for *this* product, but `epubveri`
  won on brevity (8 vs 10 chars) + the shared **`veri`** family bond with `veripublica`.

### House brand / org — `veripublica`  (LOCKED 2026-06-27)
- **Why a separate org (not under ePubLift):** the owner wants this to be an
  **independent organization** from epublift, with its **own dual-commercial
  licensing**. `veripublica` is the house / umbrella brand that holds the owner's
  dual-licensed, copyright-consolidated, CLA-gated products; `epubveri` is its
  **first product**. Same pattern as **imazen** → `imageflow` / `imageresizer`:
  the house brand is NOT the product name.
- **Why this name:** coined from Latin **veri** (*verus*, "true") + **publica**
  ("public / publication") → "**verified / trustworthy publishing**". It encodes the
  whole ecosystem — epublift *produces* → veripublica is the *house* → epubveri
  *validates* — **without** locking the brand to "EPUB" (headroom for future,
  non-EPUB products). Coined = defensible.
- **Naming family:** `veripublica` (house) + `epubveri` (product) share the **`veri`**
  morpheme → tight, recognizable brand identity. (This `veri`-bond is *why* `epubveri`
  beat `epubdictum` for the product.)
- **Availability verified 2026-06-27:** `veripublica.com`, `.io`, GitHub org, npm,
  crates.io — all free.
- **Rejected house-brand candidates** (for the record, so we don't relitigate): real
  Latin words `veridia` / `libria` / `rubrica` were all dropped — their GitHub orgs &
  `.com` were taken and each carried a same-or-near-field brand collision (`Veridia
  Brands`; `Libria` library software; `rubrica` = common Italian word). Lesson: short
  real-word `.com`s are long gone, so a **coined/compound** name wins on both
  availability and trademark strength.

### License — dual `AGPL-3.0-only OR LicenseRef-veripublica-Commercial`
- Same model as the imazen image codecs the owner already uses.
- **Commercial-license ref** changed from `LicenseRef-ePubLift-Commercial` to
  `LicenseRef-veripublica-Commercial` (tracks the new house) — it's just a label.
- **Copyright holder / seller (decided for now):** the owner **personally** (sole
  author) holds copyright and grants the commercial license under his **own name** —
  fine for solo authorship. If a company is formed later, the assignment/transfer is
  handled **with lawyers, on their guidance**.
- **Open-source users get it free under AGPL** (e.g. Calibre-web, itself GPL-3.0,
  can use it — GPLv3 §13 permits combining with AGPL).
- **Closed/commercial embedders** (Kobo, Kindle, publisher pipelines) negotiate a
  **paid commercial license**.
- Conscious tradeoff: **protection + monetization over maximum adoption.** An AGPL
  validator will NOT displace BSD-3 epubcheck as the universal embedded standard —
  that adoption cost was accepted deliberately.
- **HARD PREREQUISITE — CLA.** Selling commercial licenses requires the owner to
  hold ALL copyright. Solo authorship = fine. **ANY external contribution requires a
  signed CLA/assignment before merge**, otherwise that code can't be sold under the
  commercial license. (This is why imazen requires a CLA.) The CLA also completes the
  owner's anti-rip-off armor (see "Owner context").

### Repo setup checklist (when the repo is created)
- [x] `LICENSE` → full AGPL-3.0 text (done 2026-07-01, official gnu.org text)
- [x] `LICENSE-COMMERCIAL.md` → short "contact for commercial use" note (done 2026-07-01)
- [x] `Cargo.toml` → `license = "AGPL-3.0-only OR LicenseRef-veripublica-Commercial"`
- [x] `CONTRIBUTING.md` → in place, states no external contributions until CLA
      mechanism exists (done 2026-07-01) — the CLA mechanism itself is still open
- [x] README stating the dual-license model plainly (done 2026-07-01)
- [x] Pure-Rust only, **no C dependencies** (consistent with epublift's philosophy)

**Repo live:** `github.com/veripublica/epubveri`, public, pushed 2026-07-01.

---

## Technical approach (data-first — the owner's style)

1. **Hand-code high-value checks first** (we know these matter and can do them
   without a schema engine): OCF/mimetype, OPF well-formedness + required metadata +
   manifest/spine integrity + declared media-types, broken internal references,
   nav-doc structure.
2. **Measure coverage against epubcheck's own test corpus** (+ W3C `epub-tests`) as
   the headline progress metric — coverage %.
3. **Adopt epubcheck's message IDs** (`RSC-…`, `OPF-…`, `HTM-…`, etc.) for drop-in
   familiarity, so existing toolchains/users recognize the output.
4. **RelaxNG / Schematron schema engine only later** — the XHTML content-model is the
   deepest, hardest part; don't start there.
5. Be honest: full parity with epubcheck is a long credibility climb. **Embeddable +
   fast + WASM delivers value before parity.**

## The killer strategic edge
W3C lags in updating epubcheck after a spec release. epublift already tracks the
**EPUB 3.4** draft. So: build **EPUB 3.4 validation rules as the 3.4 spec firms up
and ship 3.4 support BEFORE epubcheck does** → "first validator to support EPUB 3.4."
(EPUB 3.4 adds AVIF/JXL images + Opus/AAC audio, is XHTML-only / HTML-syntax removed,
TTF media type, SHA-1 phase-out — all new rules to validate.)

---

## Owner context (carry this into how you work)

- **Name:** Baris (baris@kayadelen.com). Based in Turkey. Speaks Turkish — he often
  writes in Turkish; mirror his language.
- **Working style:** data-first, calibrated experiments; **honest-not-hype** (don't
  oversell; surface failures plainly); accumulate work locally and **push/release
  only when explicitly asked**; verifies on real books; values long strategic
  discussion before coding.
- **NEVER credit Claude/Anthropic** as a contributor — not in commits, PR bodies,
  authors, or docs. (Standing rule across all his projects.)
- **License = protection, not a technical detail.** Background: in a past project a
  partner closed/commercialized their shared work and gave Baris and his friend
  nothing — no written agreement, they were left helpless. This is why he chose AGPL
  + dual-commercial + CLA. When discussing licensing, **always keep the protection
  dimension on the table**, never reduce it to "X is more popular/adoptable." For
  anything involving money or partners, recommend a written agreement (he's under
  Turkish FSEK law; suggest a qualified review). You are not a lawyer — say so.
- **CI note (epublift convention, likely reuse here):** `cargo fmt --check` is
  enforced — run `cargo fmt` before pushing.

---

## First concrete step  (DONE 2026-06-27 — see "Spike results" below)

A **measurement spike**: a minimal Rust validator (~15–20 high-value checks) run
against epubcheck's test corpus, reporting coverage %. This is the data point that
tells us how far hand-coded checks get us before we need a schema engine. Set up the
`epubveri` crate (lib + thin CLI), implement the OCF/OPF/manifest/spine/refs checks,
point it at the corpus, and report the % caught + the categories of misses.

## Spike results (2026-06-27)

**Built:** `epubveri` crate at repo root — `src/` lib + thin CLI bin (`--format
human|ids`), ~24 hand-coded structural checks with reconciled epubcheck IDs (PKG/RSC/
OPF). **Pure-Rust, no C deps** (verified: binary links only libSystem). Deps: `zip`
(flate2→miniz_oxide), `roxmltree`, `unicode-normalization`. Harnesses:
`scripts/spike.py` (synthetic fixtures) and `scripts/corpus.py` (parses epubcheck's
Cucumber `.feature` files, zips expanded test dirs on the fly, scores recall + FPs).
The epubcheck corpus is cloned to a **gitignored `corpus/`** (never redistributed).

**Synthetic fixtures:** 24/24 checks fire, **0 false positives** on a valid book.

**Against epubcheck's own corpus** (708 scenarios with a publication; 285 skipped =
247 single-file/auto-wrapped content tests + 38 opf-only, both outside structural scope):

| metric | result |
|---|---|
| detection recall (flag *any* error on should-error) | 30/217 = **13.8%** |
| exact-ID recall (same message ID) | 29/217 = **13.4%** |
| within the 14 dedicated structural codes we target | 26/26 = **100%** exact |
| should-be-clean files: stayed silent | 181/181 = **100%** (0 false positives) |

Family breakdown (exact hits / total): **PKG 10/26** (our strongest — packaging),
OPF 7/48, **RSC 13/116**, HTM 0/12, CSS 0/12, MED 0/10, NAV 0/3, NCX 0/2.

**Structural checks added 2026-06-27** (after ID reconciliation): duplicate spine
reference (**OPF-034**); spine `toc` → non-NCX resource (**OPF-050**); EPUB 2 spine
missing the NCX `toc` attribute (**RSC-005**); encrypted resources declared in
`META-INF/encryption.xml` (**RSC-004**, INFO). These lifted exact recall 11.5% → 13.4%
while holding within-target precision at 100% and false positives at 0.

**Message IDs reconciled against epubcheck (2026-06-27).** Verified every emitted ID
against epubcheck's `MessageBundle.properties` + `.feature` assertions. Found and fixed
several **collisions** where our provisional IDs meant something else in epubcheck
(real `OPF-011` = page-spread, `OPF-031` = guide ref, `OPF-013` = MIME mismatch,
`NAV-001` = "nav not allowed in EPUB 2"). Key finding: epubcheck enforces most package
constraints via RelaxNG+Schematron and reports them all under the **`RSC-005`**
catch-all — so missing dc:title/language/identifier, a missing nav doc, a malformed or
duplicate manifest item all map to `RSC-005` (we now emit that). Dedicated codes we use
verbatim: `PKG-004/006/007`, `RSC-001/002/003`, `OPF-001/002/030/033/043/049`. Net
effect: within-target exact precision jumped 35.9% → **100% (24/24)**; overall recall
unchanged (~12%) — reconciliation fixed *correctness*, it did not inflate the metric.

**The data point (decisive):** hand-coded structural checks plateau around **~12%** of
the corpus's error scenarios. The bulk is unreachable by hand: **RSC-005 alone is ~116
cases** — epubcheck's schema/Schematron catch-all (content-model, renditions, previews,
…) — plus HTM/CSS/MED content checks. So the next big lever to move coverage is a
**schema engine** (RelaxNG content-model + Schematron package rules), exactly the
"schema engine only later" call — now **quantified**. Hand-coding nailed the
packaging/structural layer (PKG/OCF strong, OPF-structural partial) with zero false
positives; that's the embeddable+fast value to ship before parity.

**Two real bugs found & fixed via the corpus** (FP 7→0): (1) XML `id`/`idref`
whitespace normalization in manifest/spine matching; (2) resource resolution now
percent-decodes hrefs and compares ZIP entry names under Unicode NFC.

## Schema engine — phase 1 (2026-06-27)

Started the big phase. **Phase 1 = a pure-Rust, derivative-based RELAX NG validation
engine** (James Clark, "An algorithm for RELAX NG validation") in `src/rng/`:
- `pattern.rs` — pattern model (empty/notAllowed/text/choice/interleave/group/oneOrMore/
  after/element/attribute/data/value/list) + name classes + **smart constructors**
  (normal form) + `nullable`.
- `derive.rs` — the derivatives (text / startTagOpen / att / startTagClose / children /
  endTag) driven over a `roxmltree` document; `validate_node` / `validate_xml`.
- `mod.rs` — builder re-exports + a real (simplified) `container.xml` grammar via the API.
- `load.rs` — **RNG-XML loader (increment a, DONE):** parses the RELAX NG XML syntax
  (`grammar`/`start`/`define`/`ref`, `element`/`attribute`/`group`/`choice`/`interleave`/
  `optional`/`*More`/`list`/`mixed`/`value`/`data`, `ns` + `datatypeLibrary` inheritance,
  prefix resolution) into a [`Grammar`].
- `datatype.rs` — **XSD datatypes (increment b, DONE):** real lexical validation for
  `language`, `NCName`/`NMTOKEN`/`Name`, `ID`/`IDREF(S)`, the integer family, `decimal`,
  `boolean`, `dateTime`/`date`/`time`, and `anyURI` (lax), with XSD whiteSpace handling
  and value-space equality for `<value>`; unrecognized types stay lenient. (Facets /
  `<param>` enumerations come with the schemas that need them.)
- **`Ref` node + memoization (increment e-prep, DONE):** a `Grammar { start, defs }`
  holds named definitions; `Pattern::Ref(idx)` points into them **without inlining**, so
  **recursive content models are supported** (recursion is guarded by `element`, so it
  terminates per start-tag event). `nullable` and `startTagOpenDeriv` are **memoized at
  `Ref` boundaries**, which bounds the work and guards pathological unguarded cycles.

**19 unit tests green** — toy + container grammars (via the API *and* loaded from `.rng`
text), a **recursive** grammar, datatype lexical/equality checks, and our own
`schemas/package.rng` (accept minimal OPF / reject malformed). The engine **is now wired
into the validator** for the package document (increment c, below).

**Provenance decision (owner, 2026-06-27): author our own schemas** — cleanest provenance
(all copyright ours, committable, sellable). So schemas live in `schemas/` (committed).

**Increment (c) — package RNG wired in (DONE, with an honest null result).** Authored our
own permissive `schemas/package.rng` (committed; not derived from epubcheck/W3C), embedded
via `include_str!`, and wired into `opf::check`: a non-conformant package document is
reported as **RSC-005**. Result on the corpus: **0 false positives (181/181 clean)** — the
permissive schema is FP-safe — but **coverage did not move** (detection 30/217, exact
29/217, unchanged). Why: our hand-coded package checks already saturate the package layer,
so an FP-safe (permissive) schema only overlaps them; the package-level RSC-005 cases we
still miss are **Schematron** rules (prefix/refines/vocab), not grammar. The value landed
is **infrastructure**: the "author-own-schema → load → validate → RSC-005" pipeline is
proven end-to-end and FP-safe, and is the template for (e). The package RNG also stands as
a generic structural backstop (catches gross malformations the targeted checks don't aim
at).

**The strategic point this surfaced:** the real schema-engine payoff is the **XHTML
content-model** (HTM 0/12, and most of the ~116 RSC-005 are content/Schematron, not
package). Under "author our own schema," a full XHTML content-model RNG is a *very large*
effort (it is why epubcheck uses W3C's). So increment (e) is a genuine magnitude/approach
decision — author our own XHTML RNG, revisit provenance for that one schema specifically,
or accept hand-coded+package-schema as the structural tier for now.

**Remaining increments:** (d) **Schematron** (small XPath subset) for package rules — this
is what would actually lift package RSC-005 coverage; (e) the **XHTML content-model**
(magnitude decision above; engine groundwork — `Ref`+memoization — is in place; may need
**hash-consing** for interleave at scale + XSD facets).

## Open / not-yet-decided
- **Trademark clearance SKIPPED (owner decision, 2026-07-01).** Preliminary
  clearance for `veripublica` + `epubveri` (US/USPTO + EU/EUIPO) was on the
  books as a pre-public-launch gate, but the owner decided to skip it — a real
  search takes too long and costs money he doesn't want to spend right now.
  The repo went public on GitHub (`github.com/veripublica/epubveri`) without
  it. Residual risk (not a lawyer): if a prior conflicting mark exists, a
  future objection/rebrand is possible. Don't re-raise this unless the owner
  brings it up again (e.g. before a bigger launch, funding, or trademark
  registration push) — it's a decided tradeoff, not an oversight.
- **DONE (2026-07-01):** `veripublica` GitHub org + `epubveri` repo created
  and pushed — public, AGPL-3.0 detected by GitHub. Still open: reserve
  crates.io/npm placeholders, grab `epubveri-wasm`, consider `.com`/`.io`.
- CLA mechanism (DCO sign-off vs a full CLA doc/service).
- Exact WASM packaging + npm publish flow.
- **Test-corpus handling — NOT a license question about our code.** We write a
  100% clean-room Rust implementation (no derivation from epubcheck's Java), so
  **epubcheck's license is irrelevant to our source.** It matters in *one* narrow case:
  **redistributing their test fixtures.** So don't commit copies of epubcheck's test
  EPUBs into our repo — pull them as a git submodule / fetch in CI, and/or build our own
  corpus + use W3C `epub-tests` (separate license). Adopt their message **ID scheme**
  (short codes aren't copyrightable) but write our **own** message wording, never copy
  theirs verbatim.
