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

**Remaining increments:** ~~(d) Schematron~~ and ~~(e) the XHTML content-model~~ — **both
done, see below.**

## Increment (e1) — XHTML content-model schema + engine hash-consing fix (2026-07-01)

**Owner decision:** author our own XHTML content-document RNG from scratch (not an upstream
W3C/IDPF schema), continuing the "author our own schemas" provenance principle. Built
`schemas/xhtml.rng`: strict on element vocabulary (only defined elements are legal, so
obsolete/removed HTML elements — `keygen`, `applet`, `marquee`, `frame(set)`, etc. — are
rejected for free by omission), permissive on nesting order (shared `flowContent`/
`phrasingContent` pools via `mixed`/`choice`, not HTML5's exact per-element nesting rules —
trades some precision for near-zero false-positive risk). Covers document skeleton,
sectioning/heading, grouping/flow, phrasing/inline, tables, embedded content (img/audio/
video/picture/iframe/embed/object/canvas), a basic forms vocabulary (form/input/button/
select/textarea/label/fieldset/output/datalist — epubcheck accepts these in content
documents, contrary to an initial assumption that forms were out of scope), `<script>`/
`<noscript>`/the deprecated `epub:trigger` element, image maps (`<map>`/`<area>`), and
`epub:switch`/`epub:case`/`epub:default` (real sequencing: `case*` then optional `default`,
enforced structurally; branch content allows both ordinary flow content and opaque foreign
markup, since case/default commonly hold non-XHTML fallback rendering like MathML/CML).
SVG/MathML embeds stay opaque (any attributes/children, recursive) — not modeling their own
content models. RELAX NG name classes can't express string-prefix wildcards (no "any
`data-*` attribute", no "any hyphenated custom-element name"), so global attributes use a
curated allow-list plus a permissive catch-all that excludes a small obsolete/removed-name
blocklist (this is how obsolete-attribute errors are still caught without an exhaustive
allow-list). Wired into `opf::check`'s existing content-document loop (mirrors the OPF/
`package.rng` wiring): a non-conformant content document reports **RSC-005**.

**A genuine, serious bug surfaced and fixed en route: exponential blowup in the derivative
engine.** Ordinary pretty-printed prose (15-20 sibling elements under a `mixed`/`<interleave>`
pattern — exactly what `flowContent`/`phrasingContent` are) made `src/rng/derive.rs` hang
(a 20-paragraph synthetic chapter timed out at >8s). Root cause (traced by hand, confirmed
via isolated microbenchmarks with *no* schema recursion involved): every insignificant
whitespace text node is handled via `choice(cur, text_deriv(cur, s))`, and without pattern
canonicalization, two independently-built-but-structurally-identical `Pattern` trees never
compare equal, so the tree doubles at every whitespace node. **Fixed via hash-consing**
(`src/rng/pattern.rs`): `Pattern`/`NameClass`/`Datatype` gained manual/derived `Hash`+`Eq`
(Rc children compared by *pointer*, not recursively — valid since children are always
interned before their parent is built), a `thread_local` intern table canonicalizes every
constructed pattern, and `choice()` now short-circuits `choice(a, a) -> a` via `Rc::ptr_eq`.
Result: the same 20-paragraph case that timed out now runs in ~4ms; 2000 paragraphs run in
~50ms (roughly linear, not exponential). `clear_intern_cache()` is called at the end of
`validate_bytes` to bound memory in long-lived embedded use. This was flagged as a future
risk in the original phase-1 notes above ("may need hash-consing... at scale") — it turned
out to bite at ~15-20 events, not just "at scale," making the fix a hard prerequisite for
shipping *any* `<mixed>`-based schema, not an optimization.

**Two more pre-existing (not introduced this session) bugs surfaced by finally exercising
content documents at scale, both fixed:** (1) `roxmltree::Document::parse` rejects *any*
`<!DOCTYPE>` by default (an extra security precaution on top of its own built-in
billion-laughs protection) — and **131 of 136** real-world-style content-document fixtures
in the corpus have one, so content documents were being silently skipped entirely (no
broken-reference check, and now no schema check either) until switched to
`parse_with_options` with `allow_dtd: true` (new shared `ocf::parse_xml` helper, used by all
4 call sites). (2) The broken-reference check treated `<base href>` as a checkable resource
reference (it isn't — it sets a base URI) and didn't trim whitespace-only `href`/`src`
values before the emptiness check; both fixed in `opf.rs`.

**Measurement-harness work (`scripts/corpus.py`, not shipped code):** epubcheck's corpus
includes ~160 bare single-content-document fixtures (epubcheck's single-file check mode) that
the harness previously skipped entirely (no book to wrap them in). Added `wrap_single_doc`:
synthesizes a minimal book (synthetic nav + the target doc + its directory siblings, so
relative refs resolve) so these are measurable too. Also fixed a real parser gap: Cucumber's
table form (`the following errors are reported` / `| ID | message |` rows) wasn't captured,
silently misclassifying some error-expecting scenarios as "should stay clean." Documented,
accepted limitation: single-doc wraps can't see resources outside the fixture's own
directory (epubcheck's single-file mode never needed them either), so **RSC-001 is excluded
from scoring specifically for single-doc-wrapped scenarios** — a harness-scoping choice, not
a silenced product defect.

**Final honest numbers** (708 corpus scenarios; 83 skipped as out-of-scope/missing):

| metric | before (e1) | after (e1) |
|---|---|---|
| should-error cases scored | 217 | 325 |
| detection recall | 13.8% | 18.8% |
| exact-ID recall | 13.4% (29 hits) | 16.3% (**53 hits**) |
| should-be-clean cases scored | 181 | 282 |
| false positives | 0 | **1** (0.4%) |
| RSC family exact hits | 13/116 | 37/203 |

The 1 remaining false positive (`custom-elements-valid.xhtml`) is a known, accepted RELAX NG
limitation: name classes can't express "any element name containing a hyphen" (HTML5 custom
elements), the same class of limitation as "any `data-*` attribute" — not fixable without
either a broad permissive fallback (defeats obsolete-element detection) or per-name
enumeration (unbounded). `HTM`/`CSS`/`MED` families stayed at 0 — confirmed **not**
grammar-shaped (encoding/doctype/entity checks, CSS parsing, fixed-layout viewport
meta-tag parsing) — this increment's honest target was always the content-model-shaped
`RSC-005` subset, and that's where the movement is (13 → 37 exact hits).

## Sibling project: `styloria` (2026-07-01)

To move the `CSS` family off 0%, the owner decided (same session as increment e1) to build
a **real, pure-Rust CSS3 parser** — but as its **own standalone repo**,
`github.com/veripublica/styloria`, not a module inside epubveri. Reasoning: some users may
want to download and use *only* the CSS parser, independent of EPUB validation. Same dual
`AGPL-3.0-only OR LicenseRef-veripublica-Commercial` license + CLA model as epubveri; scoped
as a **general-purpose** CSS3 parser/serializer (CSS Syntax Level 3 tokenizer + core
grammar), not EPUB-specific. epubveri will eventually take a normal crate dependency on it
(path/git dependency until it's published). See `styloria`'s own `CLAUDE.md` for the full
naming/decision writeup.

**Phase 1 done (2026-07-01):** the CSS Syntax Level 3 tokenizer, core-grammar parser
(`Stylesheet`/`QualifiedRule`/`AtRule`/`Declaration`, "parse a stylesheet" + "parse a list
of declarations" entry points), and a spec-faithful serializer are built and pushed —
33 unit tests, `cargo fmt` clean. No selector or property-value semantics yet (by design —
see styloria's own CLAUDE.md). **Wired into epubveri 2026-07-02** — see the dated
"wire in styloria, real CSS checks" entry further down for that integration and the
concrete checks built on top of it.

## Increment (d) — mini-XPath engine + Schematron (2026-07-01)

**Research first, same session:** before designing this, read the *real* epubcheck source
(the full repo is cloned into gitignored `corpus/epubcheck/` for test fixtures, so its actual
`.sch` schema files are there too — `src/main/resources/com/adobe/epubcheck/schema/`. Read
only for understanding, never copied — same clean-room stance as `schemas/*.rng`). Finding:
individual rules (id uniqueness, unique-identifier resolution, dcterms:modified cardinality,
`@refines` target existence, ...) are each simple — but real epubcheck writes them in
**XPath 2.0** (`matches()`/`tokenize()`/`resolve-uri()`/`lower-case()`, plus Schematron's
`is-a`/`<param>` templating), which is a much bigger engine than "a small XPath subset"
implied. **Owner's call: build the real XPath *1.0* core anyway** (not a shortcut of
hand-coding the rules directly in Rust) — path expressions/axes/predicates, node-set↔string↔
number↔boolean coercion (including the existential node-set comparison semantics real rules
lean on, e.g. `$id-set[@id = current()/@id]`), `count`/`normalize-space`/`string`/`contains`/
`starts-with`/`substring`/`concat`/`lower-case`/`upper-case`, variables via `<let>`,
`current()` — explicitly **without** `matches()`/`tokenize()`/`resolve-uri()` (deferred).

**Built:** `src/xpath/` (`lexer.rs`/`ast.rs`/`parser.rs`/`eval.rs`) and `src/schematron/`
(loader + executor for `<schema>`/`<pattern>`/`<rule>`/`<let>`/`<assert>`/`<report>`), wired
into `opf::check` alongside `package_grammar()`, reporting **RSC-005** (same catch-all
epubcheck itself uses for nearly all Schematron findings). Own-authored `schemas/package.sch`
covers: id-uniqueness, unique-identifier→dc:identifier resolution, dcterms:modified occurring
exactly once (EPUB 3 only — scoped via `starts-with(@version,'3')`, since EPUB 2 packages
don't have it), and `@refines` fragment-target existence.

**Two real, non-obvious bugs found and fixed via TDD against the actual rule shapes:**
(1) **XPath's document-node vs. root-element distinction.** `/opf:package` must match a root
`<opf:package>` element directly — but naively seeding path evaluation with "root element,
then search its children" (which is correct for every *other* step) never finds a root
element matching its own name, since `/foo` really means "the document-node's child named
foo," and the document-node is a level *above* the actual root element in XPath's data model.
Fixed by special-casing only the first step's `Child` axis of an absolute path (every other
axis, crucially the `DescendantOrSelf` that `//` desugars to, behaves normally). (2) Real
Schematron `context` is a *match pattern* (XSLT-style), not an ordinary XPath expression —
`context="opf:package[@unique-identifier]"` legitimately matches the document's own root
element, which a literal `//opf:package` search never would. Implemented via walking every
node and checking the step sequence *backwards* through its ancestor chain
(`matches_context_pattern`), not via forward path-navigation.

**A measurement-harness gap, not a product bug, cost real accuracy until fixed:** epubcheck's
corpus has real per-rule test scenarios for id-uniqueness/unique-identifier/etc., but they
check a *bare `.opf` file* (epubcheck's single-file package-check mode) — `scripts/corpus.py`
was unconditionally skipping every `.opf`-suffixed scenario as "opf-only, out of scope."
Added `wrap_opf_file` (mirrors the earlier `wrap_single_doc` for bare `.xhtml`) so these are
measurable too — without it, the numbers below would have looked exactly like a no-op,
masking real, verified-working coverage.

**One more, subtler false-positive class, found and fixed the same way as the DOCTYPE gap in
increment e1:** the new dcterms:modified-count rule is a genuine EPUB 3 requirement, but our
own `scripts/spike.py` and `scripts/corpus.py` synthetic fixture generators (predating this
rule) didn't declare one — so it flagged our *own* "valid" test fixtures. Fixed by adding a
`dcterms:modified` meta element to both generators (the fixtures were incomplete, not the
rule wrong) — a reminder that this project's synthetic fixtures need to keep pace with new
checks, not just real books.

**Honest numbers** (708 corpus scenarios; wrapping now covers OPF-only scenarios too, so
denominators grew *again*, honestly, same as every prior increment):

| metric | before (d) | after (d) |
|---|---|---|
| should-error cases scored | 325 | 349 |
| exact-ID recall | 16.3% (53 hits) | 16.6% (**58 hits**) |
| should-be-clean cases scored | 282 | 296 |
| false positives | 1 | **1** (same known gap — see below) |
| RSC family exact hits | 37/203 | 41/216 |

The 1 remaining false positive (`custom-elements-valid.xhtml`) is the same RELAX NG
limitation noted in increment e1 (can't express "any hyphenated custom-element name") —
unrelated to this increment. `matches()`/`tokenize()`/`resolve-uri()`-dependent real rules
(dcterms:modified's exact date-format regex, `@refines`-as-relative-URL) and any
content-document-level Schematron (title-non-empty, `epub:switch`-deprecated,
`http-equiv`+`charset` both declared, aria cross-reference rules, ...) are explicitly
deferred — this increment's honest target was the four package-level, XPath-1.0-reachable
rules, and that's where the movement is.

## Second sibling project: `schemora` (2026-07-02)

Same reasoning as `styloria`'s split: the `src/xpath/` + `src/schematron/` engine built for
increment (d) is general-purpose (not EPUB-specific), so the owner had it extracted into its
own repo, `github.com/veripublica/schemora`, same dual-license/CLA model. **epubveri keeps
its own copy of `src/xpath`/`src/schematron` and `schemas/package.sch` for now** — the owner
explicitly chose NOT to switch epubveri to a path/git dependency on `schemora` immediately;
that de-duplication is a deliberately deferred, separate decision. See `schemora`'s own
`CLAUDE.md` for the extraction details and the two non-obvious XPath/Schematron correctness
fixes (document-node vs. root-element; Schematron `context` as a match-pattern) it inherited.

## Increment: wire in `styloria`, real CSS checks (2026-07-02)

The `CSS` message-ID family had been stuck at 0% since the project's start — the reason
`styloria` got built in the first place. This increment actually wires it in
(`styloria = { path = "../styloria" }`, unlike `schemora`, which stayed un-integrated by
explicit choice — here integrating *is* the point) and adds real checks on top of its
phase-1 output (tokenizer + generic `Stylesheet`/`ComponentValue` tree — no selector or
property-value grammar needed).

**Scope, grounded in the real corpus** (same clean-room read-only research as every prior
increment): `CSS-002` (`@font-face` `src` has an empty `url()`), `CSS-019` (`@font-face`
with an empty block), `CSS-008` (a generic "CSS syntax error" catch-all), plus a generic
`url()` resource-resolution pass (covers `@import`/`@font-face src`/`background`/etc.
uniformly, reported as **RSC-001** — a missing resource is a missing resource regardless of
which document type found it). New `src/css.rs`; wired into `opf::check` two ways: manifest
items declared `text/css`, and `<style>` elements inside content documents (reusing the
existing content-document loop). Deferred: `CSS-003`/`004` (byte-level encoding-mismatch
warnings — a different concern from parsing structure), `CSS-001` and other property-*value*
semantic checks (need real property grammar, deliberately not in styloria yet), `CSS-029`/
`030` (package-document cross-referencing).

**`CSS-008`'s real shape, found by testing against actual failing scenarios, not by
guessing:** the initial "any `BadString`/`BadUrl` token" heuristic is correct but too
narrow — none of the corpus's five `CSS-008` scenarios are tokenizer-level bad tokens; they're
either an **unclosed rule** (`{` with no matching `}`, which silently swallows everything up
to the next real `}` — including an entire unrelated sibling rule — as if it were the first
rule's own content) or a **malformed declaration shape** (`span.bold: bold;` — the stray `.`
splits the name into two tokens with no colon directly following the first). Both surface as
"this semicolon-delimited chunk inside a rule's block doesn't parse as `ident: ...`," so
`check_declaration_shapes` walks every rule's block (recursing into nested blocks, so this
also reaches rules nested inside `@media` etc.) checking exactly that shape. Known,
accepted trade-off: this can't yet distinguish "genuinely malformed" from modern CSS
nesting syntax (`.parent { &:hover { ... } }`) — flagged as a limitation, not silently
special-cased, since no real corpus fixture forced a decision on it either way.

**A real false-positive found and fixed the same way as every prior increment's — via the
corpus, not by inspection:** a UTF-16-encoded (real, `@charset`-declarable) stylesheet, read
naively as UTF-8 (`String::from_utf8_lossy`), turns into byte-level garbage (stray NULs and
`U+FFFD`s between every character) that trips the new declaration-shape check — a false
positive caused by the wrong encoding, not by the CSS. Fixed with a small BOM-aware decoder
(`css::decode_bytes`) used for standalone `.css` manifest items (inline `<style>` text
doesn't need this — it's already correctly decoded by the time roxmltree hands us the
XHTML's parsed text).

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 16.6% (58 hits) | 18.1% (**63 hits**) |
| CSS family exact hits | 0/17 | **5/17** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: Media Overlays (SMIL) validation (2026-07-02)

Next family: `MED`. Owner's scope call, after seeing the size comparison (same
kind of decision as XPath 1.0/2.0 and EPUB-XHTML-profile-vs-full-XHTML5
earlier): implement the **EPUB Media Overlays profile of SMIL** specifically,
not general SMIL 3.0 (~40-50 elements across a dozen modules — animation,
visual layout/regions, non-audio media objects, content-control switching,
linking, transitions — almost none of which EPUB Media Overlays touches; the
actual profile is `smil`/`body`/`seq`/`par`/`text`/`audio` plus
`epub:textref`/`epub:type` and the `clipBegin`/`clipEnd` clock-value grammar).
Because the scope is narrow and EPUB-specific (unlike `styloria`/`schemora`,
which are genuinely general-purpose), this stayed **inside epubveri**
(`src/smil.rs`) rather than becoming a new `veripublica` repo — the owner's
explicit call this round.

**Grounded in the real corpus** (read-only, for understanding — same
clean-room stance as every prior increment). One provenance note worth
recording: a grep incidentally pulled up epubcheck's own Java implementation
file; I stopped short of reading it, since that crosses from "reading a
declarative schema/test fixture for understanding" (already-established
practice) into "reading their algorithm" — a real line this project draws.
The audio Core Media Types list (`audio/mpeg`, `audio/mp4`) is public EPUB3
spec knowledge, not derived from their code.

**Implemented:** new `src/smil.rs` — a clock-value parser (the three SMIL
grammar forms: full-clock `HH:MM:SS(.mmm)?`, partial-clock `MM:SS(.mmm)?`,
timecount `N(.mmm)?(h|min|s|ms)?`, with `Minutes`/`Seconds` constrained to
exactly 2 digits, 00-59, per the actual W3C grammar — this resolved an
initial puzzle: several corpus fixtures reject 3-digit "minutes" values that
looked plausible at first glance, but the grammar requires *exactly* 2
digits, not 2-or-more); structural nesting checks (`<seq>` may only contain
`<seq>`/`<par>`, `<par>` may only contain `<text>`/`<audio>` — both RSC-005,
confirmed against the corpus rather than assumed, including the non-obvious
detail that a `<seq>` with *both* a stray `<text>` and a stray `<audio>`
reports RSC-005 *twice*, once per offending child, not deduplicated per
container); per-`<audio>` checks (URL fragments forbidden — MED-014; Core
Media Type — MED-005; `clipBegin`/`clipEnd` comparison, only once both sides
parse — MED-008/009); per-`<text>` checks (missing resource — RSC-001,
reusing the existing convention). `opf::check` gained a `media-overlay`
cross-referencing pass (needs the whole-package view, so it can't live in
`smil.rs` itself): grouping every SMIL's `<text src>` targets by which
content document they reference, then comparing against each document's
declared `media-overlay` manifest attribute to produce MED-010 (referenced
but no attribute declared), MED-011 (referenced by more than one overlay),
MED-012 (declares the wrong overlay), MED-013 (declares an overlay that
doesn't actually reference it back) — reverse-engineered precisely from
reading the real corpus's four dedicated fixture directories side by side
(manifest + all `.smil` files) since the plan's initial short description of
these four codes had MED-010 partly backwards from what the actual scenario
tests.

**Deferred, by design:** MED-015 (SMIL `<text>` order must match the
referenced content document's DOM order), MED-016 (package `media:duration`
meta values must sum correctly across overlays), MED-017/018 (fragment-scheme
edge cases for XHTML/SVG targets) — all real, but a distinctly separate and
smaller-value slice; also MED-003/004/007, which turned out (once measured)
to be `<picture>`/image-corruption checks sharing the `MED` prefix but
unrelated to media overlays at all — out of this increment's scope by
definition, not something to chase here.

**A real, useful gap found via measurement, not guessing:** 3 of the 8
targeted scenarios (`MED-008`/`009`/`014`) live as *bare* `.smil` fixtures
(epubcheck's single-document check mode), which `scripts/corpus.py` had no
wrapping support for yet (same class of gap `wrap_single_doc`/`wrap_opf_file`
closed earlier for bare `.xhtml`/`.opf` fixtures) — they were silently
falling into "missing-file" and not being measured at all. Added
`wrap_smil_file`, which scans the SMIL's own `<text src>`/`<audio src>`
attributes to generate matching stub resources (a content doc with an anchor
for every referenced fragment id, an empty audio file) so references
resolve without a harness-artifact RSC-001 — after which all three scenarios
measured correctly on the first run (the underlying check logic already
worked; only the harness was blind to them).

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 18.1% (63 hits) | 20.3% (**73 hits**) |
| MED family exact hits | 0/10 | **8/13** (all 8 targeted codes hit; the 5 misses are explicitly out of scope — 3 deferred, 2 unrelated `<picture>`/image checks sharing the `MED` prefix) |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: deferred sub-parts (2026-07-02)

Worked through the "deferred sub-parts" basket named in the CSS and Media Overlays
increments, rather than starting a new family (`HTM`) or a new area (fonts) —
owner's explicit choice. Same clean-room, corpus-grounded approach as every prior
increment.

**Implemented, all confirmed against real corpus fixtures:**
- **CSS-001** (use of the `direction`/`unicode-bidi` property): turned out to be a
  plain property-*name* match, not the property-value semantics originally assumed
  when this was deferred — reused the existing `check_declaration_shapes` walk in
  `src/css.rs`, one extra branch.
- **CSS-003** (stylesheet is UTF-16 encoded) / **CSS-004** (`@charset` value isn't
  utf-8/utf-16): `css::check` gained an `Option<&[u8]>` parameter, `Some` only from
  the standalone-`.css`-manifest-item call site (encoding is a file concept — inline
  `<style>` text is already decoded as part of its XHTML document by the time we see
  it, so that call site passes `None`).
- **MED-016** (package `media:duration` total must equal the sum of `refines`-scoped
  overlay durations, 1s tolerance): pure package-metadata arithmetic, no SMIL parsing
  needed (confirmed via the real fixture being a **bare `.opf`**) — reuses
  `smil::parse_clock_value`. Skipped silently if the total is absent or any part
  fails to parse, to avoid false positives on partial data.
- **MED-017/018** (scheme-based fragment on an XHTML `<text>` target / invalid SVG
  fragment identifier): `smil::check_text` classifies the fragment by the target's
  extension — `(` anywhere in an XHTML target's fragment (e.g. `#xpointer(id('c01'))`)
  or, for SVG, anything that isn't a plain id or `svgView(...)`.
- **MED-015** (SMIL `<text>` order must match the referenced content document's DOM
  order): grouped an overlay's targets by content doc (order preserved), built an
  `id -> DOM-index` map for that doc, and checked the referenced-id-subsequence is
  non-decreasing. Mapped to `Severity::Info` since epubcheck's "usage" severity has
  no equivalent in our `Severity` enum yet (only Error/Warning/Info).
- **Three content-document checks**, added to the existing `content_docs` loop in
  `opf.rs` (same `d: roxmltree::Document` already parsed there, additive):
  empty `<title>` (RSC-005), both an `http-equiv="Content-Type"` meta and a `charset`
  meta present (RSC-005), and any `<epub:switch>` element present (**RSC-017**, a
  new message ID — separate from and additional to the structural case/default
  sequencing `schemas/xhtml.rng` already enforces on it).
- **Confirmed already working, no action needed:** `aria-describedat` (an
  obsolete/removed ARIA attribute) was already caught by `schemas/xhtml.rng`'s
  existing obsolete-attribute blocklist.
- **Explicitly out of scope, named rather than silently dropped:** a real DPUB-ARIA
  role taxonomy (which roles are valid on which elements, which are deprecated) is a
  genuinely separate, larger undertaking — not a "deferred sub-part" of anything
  already built.

**Three real bugs found via testing against the actual fixtures, not by
inspection:**
1. **`Node::text()` returns content for comment nodes too, not just text nodes** — a
   roxmltree API surprise. The new title-empty check's first draft used
   `.descendants().filter_map(|n| n.text())` (no `is_text()` filter), so
   `<title><!--empty--></title>` was read as having real text ("empty", the
   comment's own content) and the check silently never fired. Fixed by filtering to
   `is_text()` first, matching how the existing `<style>` text-extraction code
   already did it correctly.
2. **SVG has its own, unrelated native `<switch>` element** (conditional rendering),
   which the new epub:switch-deprecated check's local-name-only match
   misidentified as `epub:switch` — a real false positive on two real SVG fixtures.
   Fixed by also checking the element's namespace equals the EPUB ops namespace
   (`http://www.idpf.org/2007/ops`).
3. **`scripts/corpus.py`'s `CHECK_RE` only matched "checking EPUB/document/the EPUB"
   — never "checking file"**, the step verb real epubcheck uses for the large
   majority of bare `.opf` fixtures (248 of 272 total occurrences) plus a handful of
   bare `.xhtml`/`.svg`/`.smil` ones. This silently hid **252 scenarios** from every
   prior increment's measurement — a pure undercount (conservative, not inflated),
   but a real gap since it's exactly why MED-016 (whose only real fixture is a bare
   `.opf`) showed zero signal at first. Fixing the regex also surfaced a
   `wrap_opf_file` bug (assumed UTF-8, crashed on a real UTF-16-encoded `.opf`
   encoding-test fixture — fixed to read raw bytes) and a genuine, separate
   scoring bug: scenarios expecting only a **warning** (`errs` empty, `warns`
   non-empty) were falling through to the "should stay clean" bucket, since the
   scoring loop only checked `s["errs"]` — meaning MED-016/CSS-003 looked like false
   positives the moment they started actually firing. Fixed by scoring
   `s["errs"] | s["warns"]` together (detection-recall's `rc`-based sub-metric stays
   error-only by definition, since `Report::is_valid()` is errors-only).

**A real, pre-existing gap found the same way, unplanned but fixed since it was
directly surfaced:** `OPF-043`'s spine fallback check flagged a spine item with a
non-core media type even when it had a **valid** `fallback` attribute pointing to a
core-type item — the code's own comment already admitted "we do not yet trace
fallback chains." Fixed by walking the `fallback` chain (bounded to 10 hops against
cycles) looking for a core type.

**Honest numbers** (scenario count jumped 708 → 960 because of the `CHECK_RE` fix
above — a real, deliberate widening of what's measured, not a change in the
product):

| metric | before | after |
|---|---|---|
| scenarios measured | 708 | 960 |
| exact-ID recall | 20.3% (73 hits) | 18.2% (**105 hits**, on the larger, more honest denominator) |
| CSS family exact hits | 5/17 | **11/20** |
| MED family exact hits | 8/13 | **12/16** |
| RSC family exact hits | 41/216 | **65/374** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: NAV and NCX validation (2026-07-02)

Owner's choice this round: the `NAV` (0/5) and `NCX` (0/2) message families —
both small (7 corpus scenarios total, a smaller upside than prior increments)
but clean, well-understood slices. No NCX content validation existed at all
before this (the NCX file was only checked for existence + media-type via the
spine `toc` attribute, `OPF-050`) — genuinely new territory, same shape as
building `src/smil.rs` for SMIL.

**Implemented, all confirmed against real corpus fixtures, including exact
occurrence counts:**
- **New `src/ncx.rs`**: `<meta name="dtb:uid" content="X">` vs the package's
  actual `dc:identifier` text (not just its `id` attribute — needed capturing
  the identifier's real text content in `opf.rs`, which wasn't tracked
  before) → **NCX-001**. Empty `<docTitle><text>` or `<navLabel><text>` →
  **NCX-006** (usage-level, mapped to `Severity::Info`, same convention as
  MED-015) — applied the `is_text()`-filtered extraction from the start
  (the `Node::text()`-on-comments gap fixed last increment).
- **NAV-010** (external link inside a `toc`/`page-list`/`landmarks` nav —
  custom nav types are exempt, confirmed via the fixture's own comment):
  added to the existing `content_docs` loop in `opf.rs`, gated on the doc
  being the actual nav doc (`nav_path`, newly captured alongside the
  existing `nav_present` flag). Namespace-checks the `epub:type` attribute
  via `node.attribute((EPUB_NS, "type"))` (roxmltree's namespaced-attribute
  lookup) rather than a bare local-name match.
- **NAV-011** (`toc` nav links, in nav order, not matching reading order):
  two variants sharing one code — wrong **spine** order, and wrong **DOM**
  order for fragments into the same document (a fragment-less link must sort
  before any fragment into that document). Built a comparison key per link,
  `(spine_index, dom_index)` — `dom_index` is `0` for a fragment-less link
  and `real_index + 1` otherwise, so plain tuple ordering handles the
  "fragment-less sorts first" rule for free, no separate flag needed.
  Counted **adjacent-pair inversions**, not "any disorder = one finding" —
  confirmed against the corpus this is the right granularity (a single
  spine-order mistake reports once; two fragment-order mistakes report
  twice, exactly matching real epubcheck's own counts). `spine_order`
  (content-doc path -> reading-order position) captured during the existing
  spine `<itemref>` loop.
- Extracted `dom_id_order` (id -> DOM-order-index map) as a shared helper in
  `opf.rs`, since NAV-011 needed the exact same computation MED-015 already
  built inline last increment — genuine reuse, not premature abstraction.

**Explicitly out of scope, named rather than silently dropped:** NAV-003
("edupub publication missing a page list") and NAV-009 ("region-based nav
not pointing to fixed-layout documents") both belong to optional EPUB
*extension profiles* (EDUPUB, region-based navigation) layered on top of
core EPUB 3 — not worth one-off profile-detection machinery for 1 scenario
each.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 18.2% (105 hits) | 19.0% (**110 hits**) |
| NAV family exact hits | 0/5 | **3/5** (both targeted codes hit; the 2 misses are the out-of-scope EDUPUB/region-nav items) |
| NCX family exact hits | 0/2 | **2/2** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: HTM family, fixed-layout viewport/viewBox cluster (2026-07-02)

Owner's choice: start the biggest remaining untapped family, `HTM` (0/29).
Real corpus research showed `HTM` splits into three fairly separate
clusters: (1) fixed-layout viewport/viewBox (12 scenarios, self-contained),
(2) XML/encoding/doctype (8 scenarios), (3) misc attribute checks (6
scenarios, excluding HTM-051/052 which — like NAV-003/009 last increment —
are optional EDUPUB/region-nav extension-profile checks, not core EPUB 3).
This increment scoped to **cluster 1 only**, the single biggest,
self-contained slice — clusters 2 and 3 are a natural follow-up, same
"split into digestible increments" pattern as CSS and Media Overlays.

**Implemented, all confirmed against real corpus fixtures on the first
attempt** (`src/layout.rs`, wired into a new pass over spine itemrefs in
`opf.rs`, since fixed-layout is fundamentally a per-spine-item concept):
fixed-layout detection (package-level `<meta property="rendition:layout">
pre-paginated</meta>` default, overridable per itemref via
`properties="rendition:layout-pre-paginated"`/`-reflowable`); a hand-written
`content="key=value,..."` mini-grammar for `<meta name="viewport">`
distinguishing several real, distinct malformation shapes the corpus
actually exercises: **HTM-046** (no viewport meta at all), **HTM-056**
(width/height key entirely absent), **HTM-057** (a recognized key with no
`=` at all, or a value that fails the numeric/`device-width`/`device-height`
grammar — units like `600px` count as this, not a syntax error — reported
once per bad key), **HTM-047** (the value slot exists but is
blank/whitespace-only — a whole-viewport syntax failure, reported once
regardless of which key), **HTM-059** (a key duplicated within the same
content string, once per duplicated key), and **HTM-060** (usage-level,
`Severity::Info`: viewport metas beyond the first aren't checked at all,
and neither is viewport metadata on a reflowable document — both just
usage-flagged, not errors). **HTM-048**: a fixed-layout SVG's root `<svg>`
missing a `viewBox` attribute, regardless of whether/how `width`/`height`
are set.

**A real, small measurement-harness gap found while researching, not
guessing:** the two usage-level scenarios are written as `HTM-060a`/
`HTM-060b` in the feature file — a Gherkin-authoring convention to label
two related sub-cases, not a real epubcheck message-ID suffix — which
`scripts/corpus.py`'s `ID_RE` never matched at all (no regex word boundary
between a digit and a following lowercase letter), silently hiding both
scenarios from every measurement. Fixed by widening `ID_RE` to match (but
not capture) an optional trailing lowercase letter, so `HTM-060a` scores as
`HTM-060` — same class of fix as the `CHECK_RE`/`wrap_opf_file` gaps found
two increments ago.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 19.0% (110 hits) | 21.0% (**123 hits**) |
| HTM family exact hits | 0/29 | **13/31** (denominator grew from the `ID_RE` fix surfacing the 2 usage scenarios) |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

**Deliberately deferred, named rather than silently dropped**: cluster 2
(XML/encoding/doctype: HTM-001/003/004/009/058) and cluster 3 (misc
attribute checks: HTM-007/054/055/061) — the next natural slice of `HTM`.

## Increment: HTM family, remaining two clusters (2026-07-02)

Finished the `HTM` family's two deferred clusters from the previous increment.
`roxmltree` exposes no structured API for the XML declaration's version,
DOCTYPE entities, or a document's original encoding (confirmed by reading its
public API) — those needed small hand-written raw byte/text scanners
(`src/htm.rs`'s `check_raw`), same "no new dependency" style as `smil.rs`'s
clock-value parser and `layout.rs`'s viewport grammar. The rest
(`check_dom`) works off the already-parsed document.

**Implemented, all 16 targeted codes confirmed hit at least once against real
corpus fixtures:** HTM-001 (XML declaration `version="1.1"`), HTM-003 (an
entity declared `SYSTEM`/`PUBLIC` in the DOCTYPE internal subset — a purely
literal internal entity is valid), HTM-004 (a DOCTYPE with a `PUBLIC`
identifier at all), HTM-009 (the OPF's own DOCTYPE, see below), HTM-058
(non-UTF-8 content document, via a BOM check), HTM-054 (a custom attribute's
namespace host is/ends with `w3.org`/`idpf.org`), HTM-055 (a `<base>`/
`<embed>`/`<rp>` element — usage-level), HTM-007 (an `ssml:ph` attribute with
a blank value), HTM-061 (an invalid `data-*` attribute name — empty, starts
with `-`, or contains an uppercase letter after the prefix).

**Two real, corpus-driven precision fixes made *during* verification, not
guessed up front:**
1. **HTM-004's real scope is EPUB3-only.** The initial implementation
   flagged the XHTML 1.1 DTD doctype (`<!DOCTYPE html PUBLIC "-//W3C//DTD
   XHTML 1.1//EN" ...>`) unconditionally — but dozens of real EPUB2 corpus
   fixtures use exactly this doctype as their *standard, expected* XHTML 1.1
   content-document template (EPUB2's OPS/XHTML content model is XHTML
   1.1-DTD-based; only EPUB3 moved away from it). Fixed by threading
   `is_epub3` into `check_raw`/`check_dom` and early-returning when false —
   applied to all of `htm.rs`'s content-document checks, since they're all
   confirmed from the same `epub3/06-content-document/content-document-
   xhtml.feature` corpus section.
2. **HTM-009's real rule is a DOCTYPE root-name mismatch, not "any DOCTYPE at
   all."** Confirmed via two real, deliberately-paired corpus fixtures: the
   legacy OEB 1.2 **Package** doctype (`<!DOCTYPE package PUBLIC "...OEB 1.2
   Package//EN" ...>`) is explicitly valid — its root name "package" matches
   the OPF's own root element — while an `<!DOCTYPE html PUBLIC "...OEB 1.2
   **Document**//EN" ...>` is invalid (root name "html" doesn't match
   "package"). Rewrote `check_opf_doctype` to extract and compare the
   DOCTYPE's declared root name instead of a blanket presence check.

**A real measurement-harness gap found the same way, not guessed:**
`scripts/corpus.py`'s `wrap_single_doc` always wrapped bare content-document
fixtures as `version="3.0"`, even for scenarios that originate from an
`epub2/` feature-file directory — meaning a genuinely EPUB2-context fixture
(like the XHTML 1.1 DTD doctype ones above) got EPUB3 rules wrongly applied
via the wrap itself, independent of fix (1). Fixed by having `resolve()`
pass `version="2.0"` when the scenario's originating feature file path
contains `/epub2/`. This surfaced a second-order gap: an EPUB2 wrap without
an NCX spuriously failed "EPUB 2 spine is missing the required toc
attribute" (a harness artifact, not a real defect) — fixed at the root by
having `wrap_single_doc` synthesize a minimal valid NCX (with a matching
`dtb:uid` and non-empty labels, avoiding *new* NCX-001/006 false positives
too) whenever it wraps as EPUB2, rather than suppressing the resulting
finding in the scoring logic.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 21.0% (123 hits) | 22.9% (**134 hits**) |
| HTM family exact hits | 13/31 | **24/31** (all 16 targeted codes hit at least once; the 5 remaining misses are HTM-051/052, the deliberately out-of-scope EDUPUB/region-nav items) |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

With this, the `HTM` family is effectively **done** for this project's
current scope (core EPUB 3 content-document conformance) — the only gap
left is the EDUPUB/region-nav extension-profile pair, consistent with the
same scope line already drawn for NAV-003/009.

## Fix: OPF-033 for an all-non-linear spine (2026-07-02)

A small, previously-diagnosed bug: the spine "no linear resources" check
(`OPF-033`) only ever checked whether the `<spine>` had zero `<itemref>`
elements — but a spine where *every* itemref is explicitly marked
`linear="no"` also has no linear resources, and real epubcheck reports the
same code for that case (confirmed via two real corpus fixtures,
`spine-no-linear-itemref-error.opf` and `spine-linear-all-no-error.opf` —
the latter using `linear=" no "` with surrounding whitespace, which the
fix trims before comparing). Fixed by checking `refs.iter().all(|ir| ...
== "no")` instead of `refs.is_empty()` — the `all()` form still correctly
covers the empty-spine case too (vacuously true), so no separate branch is
needed.

**Honest numbers:** the two previously-listed "in-scope misses" are gone
entirely — target-id exact recall is now **32/32 = 100%** (up from 30/32).
Overall exact-ID recall 22.9% → 23.2% (134 → 136 hits), OPF family 11/139 →
13/139, false positives held at 1 (same known RELAX NG gap).

## Increment: font obfuscation validation, PKG-026 (2026-07-02)

Owner's choice: fonts. Real corpus research showed this was narrower than
originally scoped when "fonts" was first floated as an option (byte-level
font-file-signature checking) — the actual, corpus-backed check is
**PKG-026**: an obfuscated resource declared in `META-INF/encryption.xml`
under the IDPF font-obfuscation algorithm (`http://www.idpf.org/2008/
embedding`) must have a manifest-declared media-type that's a real font
Core Media Type — purely a *declared-type* check, not a byte-signature one
(no corpus scenario tests file-signature validation). `OPF-090`
(non-preferred-but-valid Core Media Type usage, which also covers non-font
types like JS in its own fixture) is a separate, general resource-hygiene
concern, not font/obfuscation-specific — named as out of scope rather than
folded in.

**Implemented in `src/opf.rs`** (needs the manifest's id→(path,
media-type) map, `items`, which only exists inside `opf::check` —
`ocf::check_encryption` runs *before* the OPF is parsed, so this couldn't
live there): `check_font_obfuscation`, called at the end of `opf::check`.
Walks every `EncryptedData` entry (matches both the corpus's unprefixed
and `enc:`-prefixed XML forms for free, since `roxmltree`'s
`tag_name().name()` returns the local name regardless of namespace
prefix) whose `EncryptionMethod/@Algorithm` is the IDPF embedding
algorithm, resolves its `CipherReference/@URI`, and checks the resolved
resource's manifest media-type against a `FONT_CORE_MEDIA_TYPES` set
assembled from every real media-type string the corpus's font fixtures
actually use (not guessed): the modern preferred types (`font/otf`,
`font/ttf`, `font/woff`, `font/woff2`) plus non-preferred-but-valid legacy
aliases (`application/font-sfnt`, `application/font-woff`,
`application/x-font-ttf`, `application/x-font-woff`,
`application/vnd.ms-opentype`) and `image/svg+xml` for SVG fonts.
Deliberately excluded `application/vnd.dafont`, which the corpus uses only
to demonstrate that *remote* resources are exempt from core-type checks —
an unrelated rule, not a real accepted font type.

**A real, non-obvious detail confirmed via the fixtures, not assumed:**
`CipherReference/@URI` is relative to the **OCF container root**, not the
OPF's own directory — the fixtures' `package.opf` lives at
`EPUB/package.opf`, but the cipher reference reads
`URI="EPUB/obfuscated-font.otf"`, the full container-relative path.
Resolved with an empty base directory rather than `opf_path`'s parent, the
one resource-resolution call site in this file that doesn't use the OPF's
own directory as its base.

Added one new `scripts/spike.py` regression fixture (an obfuscated font
declared as `application/xml`) as a permanent local check, since this
increment — unlike smaller pure-function modules — has no natural home for
inline `#[cfg(test)]` unit tests (it needs a full OCF + manifest context,
same convention already used for MED-016 and similar cross-referencing
checks).

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 23.2% (136 hits) | 23.6% (**138 hits**) |
| PKG family exact hits | 10/37 | **12/37** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: media-overlay active-class CSS cross-referencing, CSS-029/030 (2026-07-02)

Owner's choice: CSS-029/030, the last real piece of the `CSS` family. These
cross-reference the package's `media:active-class`/`media:playback-active-class`
metadata properties (the CSS class a reading system applies to the
currently active/playing media-overlay element) against the actual CSS
class selectors defined in each content document's own stylesheets.

**Implemented:** `css::selector_class_names` (new, in `src/css.rs`) walks a
stylesheet's top-level qualified-rule preludes for `Token::Delim('.')`
immediately followed by `Token::Ident(name)` — a class selector, at the
token level, since styloria's phase-1 output has no selector grammar (same
"scan the raw prelude tokens" approach `check_font_face`'s `src` lookup
already used). Wired into `opf.rs`'s existing `content_docs` loop, which
now also collects each doc's CSS class names from both inline `<style>`
(reusing the parse already happening there for `css::check`) and any
linked `<link rel="stylesheet">` (new — resolved and read the same way the
existing manifest `text/css` loop does). A cross-referencing pass after
that loop reports **CSS-029** (`Severity::Info`, usage-level, same
convention as MED-015/NCX-006/HTM-055/060: a well-known class name
`-epub-media-overlay-active`/`-playing` is used as a selector somewhere but
its property isn't declared at all) and **CSS-030** (`Severity::Error`: a
declared property has no matching selector in the specific content
document its media overlay applies to).

Also added two small, closely-related bonus rules to `schemas/package.sch`
(own-authored, confirmed via 4 real corpus fixtures, no new XPath engine
work needed): `media:active-class`/`media:playback-active-class` must not
have a `refines` attribute, and their text must be a single class name (no
whitespace) — both reported as `RSC-005`, epubcheck's own catch-all for
this class of package-metadata constraint.

**A real false positive found via the corpus, not by inspection:** the
CSS-030 cross-referencing pass initially treated *any* content doc absent
from the newly-built `doc_class_names` map as "no CSS found" — but that
map is only ever populated for **XHTML** content docs (SVG's own
`<style>`/`xml-stylesheet` forms are a separate, deferred extension, named
below), so an SVG content doc with a legitimate, valid media-overlay
association was wrongly flagged as missing CSS it was never checked for at
all. Fixed by capturing `xhtml_doc_paths` before the content-doc loop
consumes the XHTML-only list, and gating the CSS-030 pass on it — an SVG
doc is now correctly skipped rather than treated as "checked and failed."

**Deliberately out of scope, named rather than silently dropped:** the SVG
content-document variant of this whole check (`mediaoverlays-active-class-
svg-*`) — SVG top-level content documents aren't looped over for
CSS-checking at all yet.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 23.6% (138 hits) | 24.8% (**145 hits**) |
| CSS family exact hits | 11/20 | **14/20** |
| RSC family exact hits | 65/377 | **69/377** (the 4 bonus refines/multiple-class-name scenarios) |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: EDUPUB + Region-Based Navigation extension profiles (2026-07-02)

Owner's choice: close out the two extension-profile families every prior
increment had deliberately deferred (NAV-003/009, HTM-051/052). Research
surfaced a real, material scope discovery worth recording: EDUPUB's
"multiple renditions" scenarios need genuine **multi-rootfile support** —
`ocf::find_rootfile` returned a single OPF path and `lib.rs::validate_bytes`
called `opf::check` exactly once. The owner explicitly chose to build this
architecture change too rather than defer it.

**Architecture change:** `ocf::find_rootfile` → `ocf::find_rootfiles`,
collecting every `<rootfile>` with the OPF media-type (RSC-003 only if the
result is empty, unchanged behavior otherwise). `lib.rs::validate_bytes`
loops `opf::check` once per rootfile into the same `Report`. A normal
single-rootfile book is just the `len() == 1` case — no regression (25/25
spike fixtures + all prior corpus hits held steady).

**Region-Based Navigation** (`epub-region-nav/region-nav-publication.feature`,
7/7 scenarios hit exactly): new `src/regionnav.rs`. Triggered by a manifest
item with `properties="data-nav"` (the "Data Navigation Document"):
**OPF-012** (not `application/xhtml+xml`), **OPF-077** (referenced from the
spine), **RSC-005** (more than one data-nav item; a `<nav>` inside it with
no `epub:type`). For the one `<nav epub:type="region-based">`: **HTM-052**
if region-based navigation is found *outside* the data-nav document
(confirmed via the real fixture: an ordinary content doc, not an
element/media-type mismatch as the scenario title alone suggested);
**NAV-009** if an `<a href>` target isn't fixed-layout (reuses a new
`fixed_layout_docs: HashMap<String, bool>` map, captured during the
existing spine loop alongside the `is_fixed_layout` computation the HTM
viewport checks already do per itemref); and the region-based nav's own
content model (**RSC-005** ×N + **RSC-017** warning), fully reverse-
engineered from one richly-annotated fixture and cross-checked line-by-line
against its inline comments: the `<nav>` must contain exactly one `<ol>`;
each `<li>`'s first element child must be `<a>` or `<span>`; a `<span>`
must contain exactly two `<a>` elements; an `<a>` may be followed by at
most one more child, which must be an `<ol>` (nested sub-regions); an `<a>`
containing actual text (not just e.g. a `<meta>` annotation) is RSC-017,
not an error.

**A real bug found via the corpus, not by inspection:** the content-model
check's first version, on finding the container-level violation ("must
contain exactly one child ol"), returned immediately without walking the
`<ol>` at all — but the real fixture (`<h1>` stray sibling *next to* a
single, otherwise-valid `<ol>`) expects *both* the container-level RSC-005
*and* every violation found inside that `<ol>`, not one or the other.
Fixed by still locating and walking whichever `<ol>` is present regardless
of the container check's outcome — after the fix, the fixture produces
exactly 7× RSC-005 + 1× RSC-017, matching the corpus precisely.

**EDUPUB** (`epub-edupub/edupub-publication.feature`, all targeted
scenarios hit exactly): new `src/edupub.rs`. Triggered by
`<dc:type>edupub</dc:type>`, either in a single-rendition book's own OPF or
in `META-INF/metadata.xml` (confirmed as a real, separate publication-level
metadata file used only for multi-rendition packages, via
`edupub-multiple-renditions-valid`). **HTM-051** (warning): an
`itemscope`-rooted HTML5 microdata item in an edupub content document —
confirmed via the corpus to key off `itemscope` alone, not `itemtype`/
`itemprop` independently (the real fixture has one `itemscope` element and
a separate `itemprop`-only element, a property *of* that same item rather
than a second item, and expects exactly one finding, not two). **NAV-003**
/ **OPF-066**: a print-source for pagination (`dc:source` + a
`meta[property=source-of]` refining it to `pagination`) and a
`epub:type="page-list"` nav must both be present or both be absent — one
without the other fires whichever code names the missing half.

**Multi-rendition `dc:type` cardinality (2× RSC-005), a second real bug
found via the corpus:** the first version required `metadata.xml` to
*always* have its own `dc:type` whenever present, which false-positived on
every ordinary (non-edupub) multi-rendition package — confirmed via
`renditions-basic-valid`/`renditions-mapping-multiple-nav-valid`, two
formerly-passing scenarios that broke the moment multi-rootfile support
started actually checking their second OPF. Re-examining the real
"publication-level dc:type missing" fixture showed *why*: its
`metadata.xml` has `dc:type` commented out, but **both** renditions still
declare `edupub` — proving the trigger isn't "metadata.xml always needs a
dc:type" but "the publication is edupub if *either* metadata.xml *or* any
rendition says so; once it is, every level must declare it, and whichever
doesn't gets its own RSC-005." Rewritten `edupub::check_multi_rendition_dc_type`
around that rule fixes both new false positives while still hitting both
real corpus error scenarios (publication-level and rendition-level) exactly.

**Deliberately out of scope:** the full EDUPUB conformance suite beyond
the four checks above (sectioning rules, accessibility metadata, etc.) —
the corpus only exercises those indirectly via `-valid` fixtures with no
dedicated error codes to target.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 24.8% (145 hits) | 26.8% (**157 hits**) |
| NAV family exact hits | 3/5 | **5/5** (100%) |
| HTM family exact hits | 24/31 | **26/31** |
| RSC family exact hits | 69/377 | **75/377** |
| OPF family exact hits | 13/139 | **16/139** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: D-vocabularies family (2026-07-02)

Research turn: the `RSC-005` catch-all's 251 miss scenarios broken down by
originating feature file. The three largest are `05-package-document` (34),
`08-layout` (20), and `D-vocabularies` (28) — all Schematron-shaped (no new
parser/engine needed, just more rules in `schemas/package.sch`), unlike the
much larger `content-document-xhtml`/`svg` pool (54), which needs a genuinely
deeper, stricter per-element HTML5/SVG content model — a bigger, lower-ROI
undertaking, left alone for now. Of the three, `D-vocabularies` is the most
self-contained single topic and was scoped as this increment; `08-layout`
(rendition:* properties) and the remaining `05-package-document` structural
rules are named as the next two natural increments.

**meta-properties-vocab.feature (22 scenarios):** a closed set of
`meta[@property]` values, each with its own refines-target-type rule and a
"cannot be declared more than once per refined expression" cardinality rule
— `authority`/`term` (must refine `dc:subject`, need a companion pair),
`belongs-to-collection` (must refine another `belongs-to-collection` meta or
be primary), `collection-type` (must refine `belongs-to-collection`),
`display-seq`/`file-as`/`group-position` (cardinality only, any target),
`identifier-type` (must refine `dc:identifier` or `dc:source`), `role` (must
refine `dc:creator`/`contributor`/`publisher`, no cardinality limit),
`source-of` (must refine `dc:source`, value must be exactly `"pagination"`),
`title-type` (must refine `dc:title`). Plus `media-overlays-vocab.feature`'s
`media:active-class`/`media:playback-active-class` cardinality (2 scenarios,
same shape, extending the existing refines/single-name rules from the
CSS-029/030 increment) and `metadata-link-vocab.feature`'s `link[rel=record]`
(must not have `refines`) / `link[rel=voicing]` (must have `refines`) (2
scenarios) — 26 new `<pattern>` blocks in `schemas/package.sch` total.

**A real, load-bearing engine gap found via the corpus, not by
inspection:** every "strip an optional leading `#` from `@refines`" `<let>`
used `substring(str, 1 + number(starts-with(str, '#')))` — but
epubveri's XPath 1.0 *core* engine has no `number()` function (confirmed
by reading `src/xpath/eval.rs`'s function dispatch, which explicitly falls
back unknown functions to an empty node-set, "non-fatal" by design). An
empty node-set coerced to a number is `NaN`, so `1 + NaN` stayed `NaN`,
`substring` degraded to "return the whole string," and `$target-id` kept
its leading `#` — silently failing every "must refine X" assertion on
otherwise-valid fixtures (`metadata-meta-authority-valid.opf` and five
siblings), a real false-positive class only the corpus caught. Fixed by
dropping the `number()` wrapper entirely: `BinOp::Add`'s own evaluator
already calls `.to_number()` on both operands (confirmed in the same
file), so `1 + starts-with(...)` coerces the boolean correctly without
needing the missing function at all.

**`media:duration`** (part of `media-overlays-vocab.feature`, 1 scenario):
not Schematron — hand-coded in `opf.rs` alongside the existing
metadata-scanning block, reusing `smil::parse_clock_value` unchanged
(already correctly rejects the fixture's three invalid values: a
comma-decimal separator, a 4-digit minutes field, and a bogus `mon` unit).

**Reserved vocabulary prefixes, new `OPF-007`** (`vocabularies.feature`, 1
scenario covering both a bare `.opf` and a bare `.xhtml` fixture): a
`prefix`/`epub:prefix` declaration must not redeclare one of EPUB's default
vocabulary prefixes (`a11y`/`dcterms`/`marc`/`media`/`onix`/`rendition`/
`schema`/`xsd` at the package level, `msv`/`prism` confirmed via the
content-document fixture) to a *different* URI — hand-coded in `opf.rs`
(`check_reserved_prefixes`, a small `name: URI` mini-grammar parser, called
on both the package's own `prefix` attribute and each content document's
root-element `epub:prefix`). **A second real false positive found via the
corpus:** the first version warned on any reserved-name redeclaration at
all, but a real, valid fixture (`prefix-mapping-reserved-valid.{opf,xhtml}`)
explicitly redeclares all 10 prefixes to their own correct default URIs —
"which is allowed," per the fixture's own comment. Fixed by recording each
prefix's real default URI (extracted straight from that valid fixture) and
only warning when the declared URI *differs* from it.

**A real, corpus-wide measurement-harness gap found via the same false
positive, not guessed:** `scripts/corpus.py`'s scenario parser only
recognized the Gherkin keyword `Scenario:`, never its real, standard
synonym `Example:` — used by exactly two feature files (`cli.feature`,
out of this project's scope, and `D-vocabularies/vocabularies.feature`,
directly in scope). Missing it meant `cur` (the in-progress scenario dict)
was never reset at those boundaries, so every scenario's `errs`/`warns`
silently accumulated into whatever scenario followed, corrupting scoring
past that point in the file (this is why the OPF-007 scenario's `expected`
set showed unrelated `OPF-028`/`RSC-005` entries bled in from earlier
scenarios). Fixed by treating `line.startswith("Example:")` the same as
`Scenario:` (careful to match only the singular form — `Examples:`, plural,
is the unrelated Scenario Outline parameter-table keyword, confirmed via a
`grep` for both spellings before writing the fix) — this widened the
measured corpus from 960 to 981 scenarios, a real, honest increase in what's
being scored, not a change to the product.

**Deliberately out of scope, named rather than silently dropped:** the SVG
content-document variant of the `epub:prefix` check (SVG root elements
aren't looped over for this yet, same gap as CSS-029/030's SVG deferral);
`role`'s exact message wording for its 3-way refines list wasn't
independently re-verified against a dedicated fixture beyond the one tested
(`metadata-meta-role-refines-disallowed-error.opf`, confirmed working).

**Honest numbers** (scenario count grew 960 → 981 from the `Example:` fix,
same honest-denominator-growth pattern as every prior harness-gap fix):

| metric | before | after |
|---|---|---|
| scenarios measured | 960 | 981 |
| exact-ID recall | 26.8% (157 hits) | 31.2% (**186 hits**) |
| RSC family exact hits | 75/377 | **102/379** |
| OPF family exact hits | 16/139 | **18/148** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: 08-layout (rendition:* properties) (2026-07-02)

Next natural increment named at the end of D-vocabularies:
`08-layout.feature`'s 20 miss scenarios, covering `rendition:layout`,
`rendition:orientation`, `rendition:spread`, `rendition:flow`, and
`rendition:viewport` (deprecated). Same shape as D-vocabularies — all
Schematron-reachable via the existing XPath 1.0 engine — and highly
templated: 4 of the 5 properties share an identical 4-rule pattern
(unknown-value, global-duplicate, refines-error, itemref-conflict),
confirmed via real fixture pairs. The content-document-level viewport/
viewBox checks this feature file also exercises (HTM-046/047/048/056/
057/059/060) were already implemented in an earlier increment and stayed
untouched here.

**Implemented:** 13 new `<pattern>` blocks in `schemas/package.sch` for
the 4 properties' unknown-value/duplicate/refines-error rules (enum
values confirmed empirically from the real fixtures: layout =
reflowable/pre-paginated; orientation = auto/landscape/portrait; spread =
auto/none/landscape/portrait/both; flow = auto/paginated/
scrolled-continuous/scrolled-doc), plus `rendition:viewport`'s own
duplicate-cardinality pattern. `check_itemref_rendition_conflicts` (new,
`src/opf.rs`) handles the itemref-override side: for each of
layout/orientation/spread/flow, more than one `properties` token sharing
the same `rendition:X-` prefix is a conflict (a *count*, not specific
named pairs — confirmed the real fixtures each use a different value
pair but the same general shape), plus `page-spread-*`'s own conflict
check (accepting both the `rendition:`-prefixed and unprefixed forms,
confirmed via `rendition-page-spread-itemref-unprefixed-valid`).
`rendition:viewport` itself (deprecated) is hand-coded in `opf.rs`: new
`OPF-086` fires on every occurrence (not deduplicated), and its value is
syntax-checked by reusing `is_valid_viewport_value` from `src/layout.rs`
(newly made `pub(crate)`) rather than duplicating the grammar.

**A real, non-obvious limitation of the Schematron engine found via the
corpus, not by inspection:** `rendition:spread`'s `"portrait"` value is
separately deprecated (its own `OPF-086` warning, on top of being a
valid enum value) — the natural way to express this is a Schematron
`<report>` pattern (fires when true, the inverse of `<assert>`). But
`opf::check`'s call site for `crate::schematron::run(...)` maps *every*
returned finding to `RSC-005`/`Severity::Error` uniformly (`for message
in ... { report.push_at(RSC_005, Severity::Error, message, opf_path); }`)
— fine for every pattern so far, since they were all genuinely RSC-005,
but wrong for a warning with its own dedicated code. Rather than
extending the schematron engine to carry per-pattern severity/ID (a
larger, separate architecture change), this one check was written as a
small hand-coded `opf.rs` check instead (same file, right next to the
`media:active-class`/`rendition:viewport` scans) — the pragmatic fix for
a single, narrow case, not a reason to touch the engine's return type.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 31.2% (186 hits) | 35.0% (**209 hits**) |
| RSC family exact hits | 102/379 | **122/379** |
| OPF family exact hits | 18/148 | **23/148** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: finish 05-package-document, all 76 remaining scenarios (2026-07-02)

Owner's explicit choice: finish all of `05-package-document.feature`'s
remaining misses in one increment rather than splitting further. Research
revealed this was much larger than the earlier "34 RSC-005-only" estimate
— 76 scenarios once dedicated codes (OPF-092, OPF-014/015/018, OPF-096,
RSC-006/007/008/011/012, etc.) were counted too, spanning 7 genuinely
distinct sub-areas (labeled clusters A-G during planning), the largest
being a new "does this content document use remote resources / scripting
/ embedded SVG" cross-reference — comparable in scope to the original CSS
or Media Overlays increments.

**A: language tags (OPF-092, new).** `xml:lang`/`link[@hreflang]`/
`dc:language`'s own text must have no leading/trailing whitespace and,
once trimmed, be empty or a plausible BCP-47 tag — no regex needed since
the corpus's only real failure mode is a single-letter primary subtag
("a-value"), which real BCP-47 never allows.

**B: small package-metadata rules** (~14 Schematron patterns + hand-coded
`OPF-065`/`OPF-085`): empty dc:identifier/language/title/meta values,
multiple dc:date, meta property/scheme NMTOKEN shape, metadata-before-
manifest element order, `package/@unique-identifier` missing (new
`OPF-048`), a general **`@refines` cycle detector** (new `OPF-065`, real
graph-cycle DFS over every `id`→refines-target edge in the whole
document, not scoped to any one property), and a `urn:uuid:` UUID-syntax
check (new `OPF-085`). Deferred, named not dropped: `OPF-053`/the exact
ISO-8601 `dcterms:modified` format (needs real date-regex, which the
XPath engine still doesn't have).

**C: link element rules** (new `OPF-098`/`OPF-093`, reused `RSC-007`):
a `link/@href` must not be a fragment-only reference to a manifest item's
own id (`OPF-098`); a link to a **local** resource needs a `media-type`
(`OPF-093`, remote links may omit it); a link to a missing local resource
is `RSC-007` (warning). **Scoped to metadata-level `<link>` only** after
a real false positive: a `<link>` inside a `<collection>` (e.g. a
`role="preview"` collection indexing existing manifest resources) follows
different rules and legitimately omits `media-type`/reuses already-
declared resources - confirmed via `preview-embedded-valid`.

**D: manifest item / href / fallback rules** (new `OPF-099/074/040/045`,
`RSC-020`/`PKG-010`/`OPF-091`): self-referencing manifest items, two
items resolving to the same resource, unencoded spaces, fragment
identifiers on manifest hrefs, broken/self-referencing `fallback` chains,
the obsolete EPUB 2 `fallback-style` attribute (scoped to EPUB 3 only -
see below), and unknown `item/@properties` values.

**E: remote-resources/scripted/svg cross-reference (`OPF-014/015/018`,
new) — the largest cluster.** For every content document, three booleans
are computed by scanning its own DOM (plus its associated CSS, reusing
the CSS-029/030 increment's stylesheet-collection code and a new
`css::stylesheet_urls`): `has_remote` (any `src`/`href`/`data`/`poster`
or CSS `url()` pointing at a genuine `http(s)://` URL), `has_script` (a
`<script>` with no/JS-family `type`, or an interactive form control), and
`has_svg` (an embedded `<svg>`). Cross-referenced against the item's own
declared `properties`: used-but-undeclared is uniformly `OPF-014`;
declared-but-unused is `OPF-018`/Warning for remote-resources but
`OPF-015`/Error for scripted/svg (confirmed per-property, not assumed
uniform). Also covers a hyperlink to a remote **image** (`RSC-006`, wrong
construct - should be embedded) and a remote resource used anywhere
(content doc, standalone CSS, or a media-overlay SMIL file) but not
declared as its own manifest item at all (`RSC-008`).

**A real, load-bearing predicate bug found via the corpus, not by
inspection:** the existing `is_external()` helper means "don't try to
resolve this as a local container path" - correctly including
fragment-only refs, `data:`, `mailto:`, `tel:`. But "is this genuinely a
remote resource in use" is a *narrower* question, and reusing
`is_external()` for it produced real false positives: a CSS `filter:
url(#filter)` (a same-document SVG filter reference) and an `@namespace
xlink url('http://www.w3.org/1999/xlink')` (a namespace URI, not a
fetchable resource) both got misread as "uses a remote resource". Fixed
by adding a new, narrower `is_remote_url()` (only `http://`/`https://`)
for every remote-resource-*detection* site, while leaving every
resolution-*skipping* site on the original `is_external()` unchanged; and
by excluding CSS `@namespace` from `stylesheet_urls()`'s collection
entirely. A related bug, found the same way: a remote URL can carry a
fragment (`https://x/y#glyph`, a real SVG-font-glyph reference) while its
manifest item declares the bare URL - fixed with a `strip_url_fragment`
helper applied before every remote-URL comparison.

**A second real, load-bearing bug: EPUB2 has no `properties` concept at
all.** The whole cluster-E cross-reference and the `fallback-style`
obsolescence check both need to be scoped to `is_epub3` - an EPUB 2
content document legitimately uses `<script>` with no properties concept
in play at all, and `fallback-style` is a valid *EPUB 2* OPF attribute,
only obsolete in EPUB 3. Both confirmed as real false positives via real
epub2-context corpus fixtures (`ops-xhtml-script-valid`,
`fallback-valid.opf`) before being fixed.

**F: spine reachability (new `RSC-011`/`OPF-096`).** A book-wide pass
collects every content document's outbound local `<a href>` targets
(including the nav). `RSC-011`: any target not listed in the spine at
all. `OPF-096`: any `linear="no"` spine item not reachable via any
hyperlink or the nav. A real false positive found via the corpus: a
hyperlink to the **package document itself** (a CFI-style
self-reference, confirmed via `nav-cfi-valid`) isn't a content document
that could ever be "in the spine" - excluded explicitly.

**G: collection/legacy/NCX rules** (new `OPF-070`, extended `src/ncx.rs`
for `RSC-012`): a `collection/@role` used as a URL must have valid
percent-encoding; a `role="manifest"` collection must not be top-level
(Schematron); duplicate `guide/reference` entries (`RSC-017`, once per
offending entry, not once per pair); the legacy "NCX present requires
spine `toc`" rule (Schematron); and NCX `<content src="target#frag">`
fragments must resolve to a real id in the target document (`RSC-012`,
new, caches each distinct target doc's id set since a real book can have
many navPoints into the same document).

**A genuine engine-architecture limitation found via the corpus, not
guessed:** the XPath 1.0 core has no `preceding-sibling::` axis at all -
attempting to load a Schematron pattern using it doesn't silently no-op,
it **panics at startup** (the `built-in package.sch must parse` assertion
fires), crashing every single scenario until found. The
"metadata-before-manifest" element-order rule was rewritten as a plain
hand-coded child-index compare in `opf.rs` instead.

**Three more small vocabulary gaps found the same way:** manifest
`item/@properties` values from EPUB Dictionaries & Glossaries
(`dictionary`, `search-key-map`) and EPUB Indexes (`glossary`, `index`) -
extension specs not implemented, but their property names are real and
were false-positiving `OPF-027` on otherwise-valid fixtures; and a
custom, package-declared-prefix property (e.g. `ex2:itemprop`) was also
wrongly flagged - any token containing `:` is now exempted from the
known-property check (custom vocabulary prefixes are always allowed).

**A measurement-harness bug found the same way, not guessed:**
`scripts/corpus.py`'s `wrap_single_doc`/`wrap_opf_file`/`wrap_smil_file`
synthetic identifier was the literal string `"urn:uuid:corpus-wrap"` -
not a real UUID, so it started false-positiving the brand-new `OPF-085`
check on every single-document-wrapped scenario. Fixed by dropping the
`urn:uuid:` prefix entirely (`"corpus-wrap"`, matching the NCX `dtb:uid`
side too). Also widened the existing `single_doc_wrap` RSC-001/RSC-007
scoring exclusion to cover `RSC-011`, `RSC-008`, and `OPF-014` - the
synthetic wrap's nav hyperlinks to the target (so the harness has a
reason to include it) but deliberately keeps it out of the synthetic
spine, and never declares any `properties` on the target's own synthetic
manifest item - both are real, correct findings on the synthetic
wrapping itself, not defects in the fixture under test. Same fix applied
to `scripts/spike.py`'s own synthetic identifier (`urn:uuid:12345` →
a real-shaped UUID).

**Deliberately deferred sub-scenarios, named not silently dropped:** a
content document referencing a remote resource only *transitively*
(embedding a local SVG file that itself contains a remote font
reference) - no SVG-content parser exists to trace into it
(`package-remote-font-in-svg-missing-property-error`); the exact
`link/@properties` known-value vocabulary (`OPF-027` for
`link-rel-record-properties-undefined-error`) - no confirmed valid
example exists in the corpus to derive the real vocabulary from safely.

**Honest numbers** (all 76 targeted scenarios hit exactly except the two
named deferrals above; false positives held at 1, the same pre-existing
RELAX NG gap):

| metric | before | after |
|---|---|---|
| exact-ID recall | 35.0% (209 hits) | **48.4% (289 hits)** |
| RSC family exact hits | 122/379 | **154/379** |
| OPF family exact hits | 23/148 | **69/148** |
| PKG family exact hits | 12/37 | **19/37** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

With this, `05-package-document.feature` is effectively **done** for
this project's current scope, aside from the two named deferrals above.

## Increment: EPUB Multiple-Rendition Publications 1.0 (2026-07-02)

Next family named at the end of "Sırada ne kaldı?":
`epub-multiple-renditions/multiple-rendition-publication.feature`, 13
scenarios (not the 8 originally estimated). The general (non-EDUPUB-
specific) Multiple-Rendition spec, reachable thanks to the multi-rootfile
architecture (`ocf::find_rootfiles`) built for the EDUPUB increment.
Genuinely new surface area: `META-INF/container.xml`'s `<links>` element
(a Rendition Mapping Document reference, entirely separate from
`<rootfiles>`) had never been parsed before, and `rendition:*` selection
attributes on `<rootfile>` elements are a new namespaced-attribute
concept at the container level.

**Implemented in new `src/renditions.rs`**, called once from
`lib.rs::validate_bytes` when `opf_paths.len() > 1` (the same gating
convention as `edupub::check_multi_rendition_dc_type`, since everything
here is about the publication as a whole, not any one rendition's OPF):
- **`META-INF/metadata.xml`**: new **`RSC-019`** (warning) if the file is
  missing entirely from a multi-rendition publication; `RSC-005` if
  present but its `dcterms:modified` doesn't occur exactly once.
  Deliberately not routed through `schemas/package.sch` - that schema is
  scoped to `opf:package` documents, and metadata.xml's root is a
  different element in the `http://www.idpf.org/2013/metadata` namespace
  entirely.
- **Rendition selection attributes**: every `rendition:*`-namespaced
  attribute on a `<rootfile>` is a selection attribute - only `media`
  and `layout` are real (confirmed via the spec), anything else is
  `RSC-005`; `rendition:media`'s value is validated with a lightweight
  "contains both `(` and `)`" check (not a full CSS media-query parser -
  the corpus's one invalid case, `"syntaxerror"`, has neither). A
  non-first `<rootfile>` with no selection attribute at all is new
  **`RSC-017`** usage (the first rootfile is the default rendition and
  needs none).
- **Rendition Mapping Document** (optional - only checked when
  `container.xml` actually declares one via `<links><link
  rel="mapping">`, confirmed the `href` is container-root-relative, like
  `full-path`, not `META-INF`-relative): `RSC-005` for more than one
  mapping link, a non-`application/xhtml+xml` media-type, a missing
  `<meta name="epub.multiple.renditions.version" content="1.0">`, not
  exactly one `<nav epub:type="resource-map">`, or any other `<nav>`
  lacking an `epub:type` at all (custom-prefixed types like `foo:bar`
  are fine, confirmed via the real valid fixture).

All 13 targeted scenarios matched exactly on the first implementation
attempt - no false positives or bugs found this round, a rare clean
pass for an increment this size (the design was fully nailed down
during research, since every rule here is a plain attribute/cardinality
check with no ambiguous edge cases like prior increments' Schematron-
engine or is_external/is_remote_url surprises).

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 48.4% (289 hits) | **50.3% (300 hits)** |
| RSC family exact hits | 154/379 | **166/379** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

## Increment: finish 09-media-overlays, all 10 remaining scenarios (2026-07-02)

Owner's choice: close out the 10 remaining misses in
`epub3/09-media-overlays/media-overlays.feature` rather than start a new
family. All grounded in real corpus fixtures (clean-room, read-only, same
stance as every prior increment).

**`src/smil.rs`**: a bare `<meta>` directly inside `<head>` (must be
wrapped in a `<metadata>` element) → RSC-005; a `<par>` may contain at
most one `<text>` child, confirmed the *first* is processed normally and
every one after it is RSC-005 "not allowed here" (not deduplicated per
`<par>`); `clipBegin`/`clipEnd` values that fail to *parse* now report
RSC-005 (previously silently skipped — the existing code comment and unit
test `clock_value_syntax_errors_from_the_real_corpus` already anticipated
this gap, it just wasn't wired to a `report.push_at` yet); `epub:textref`
on `<seq>`/`<par>` is now collected (`check()`'s return type grew to
`(text_targets, textref_targets)`) and cross-referenced by the caller in
`opf.rs` against the target document's real ids, the same RSC-012 shape
already used for NCX `<content src>` fragments; new **`OPF-088`**
(usage/Info) for an `epub:type` token that's neither in a generously-
inclusive EPUB Structural Semantics vocabulary list nor custom-prefixed
(any token containing `:` is exempt, same convention as the OPF-027
item-property prefix exemption) — biased toward inclusion in the
vocabulary list since this is Info-level, so a false negative is far
safer than a false positive.

**`src/opf.rs`**: a `media-overlay` attribute's target manifest item must
itself be `application/smil+xml` (RSC-005); a `media-overlay` attribute
is only allowed on an item whose *own* type is a content-document type —
confirmed both `application/xhtml+xml` *and* `image/svg+xml` qualify (the
existing `mediaoverlays-svg-valid` fixture uses exactly this pattern and
must stay clean), only a genuinely non-content type is the violation
(RSC-005); once any item declares a `media-overlay`, a package-level
global (non-`refines`) `media:duration` must exist (RSC-005 "global...
not set"), and each distinct referenced overlay id needs its own
`refines`-scoped one (RSC-005 "item... not set", confirmed fired once per
missing overlay id, not once per referencing content doc) — both
distinct from the pre-existing MED-016 total-vs-sum check, which only
compares values once *both* sides are already known to exist and so
never covered either "not defined at all" case.

**CSS-030's SVG variant** (`mediaoverlays-active-class-svg-style-not-
found-error`), explicitly deferred in the earlier CSS-029/030 increment:
new `collect_svg_class_names` in `opf.rs`, reached only for SVG top-level
content documents that declare a `media-overlay`. To include SVG without
regressing the 4 already-passing SVG "valid" fixtures (which must keep
finding their real CSS class), all 4 real linking mechanisms those
fixtures use had to work: inline `<style>` and linked `<link
rel="stylesheet">` (both reuse existing per-doc CSS handling), `@import
url(...)` inside a `<style>` block (new `css::import_targets` - narrower
than the existing `stylesheet_urls`, which mixes *every* `url()` in a
sheet together and so can't safely be treated as "also parse this as a
nested stylesheet" without conflating it with e.g. `background:
url(x.png)`), and a top-level `<?xml-stylesheet type="text/css"
href="..."?>` processing instruction (confirmed `roxmltree::Node::pi()`
exposes `target`/`value`; PIs are siblings of the root *element* at the
document level, found via `doc.root().children()`, not
`root_element().children()` - a small hand-rolled scan pulls `href="..."`
out of the PI's pseudo-attribute value string).

**A real harness-only regression found and fixed during verification, not
a product bug:** `scripts/corpus.py`'s `wrap_smil_file` (used for the 3
bare-`.smil` scenarios in this batch) synthesizes a minimal package that
declares `media-overlay` on every wrapped content item but had no
`media:duration` meta at all — meaning the brand-new global/per-item
duration-not-defined checks above immediately flagged the wrap itself on
every bare-SMIL scenario, including the three that must stay clean
(`minimal.smil`, `epubtype-valid.smil`, `epubtype-prefix-declared-
valid.smil`). Fixed at the harness root, same precedent as the earlier
synthetic-NCX fix for EPUB 2 wraps: `wrap_smil_file`'s synthesized OPF now
also declares a matching global + refines-scoped `media:duration`.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 50.3% (300 hits) | **51.9% (310 hits)** |
| RSC family exact hits | 166/379 | **174/379** |
| OPF family exact hits | 69/148 | **70/148** |
| CSS family exact hits | 14/20 | **15/20** |
| false positives | 1 | 1 (same known RELAX NG gap, unrelated) |

With this, `09-media-overlays.feature` is effectively **done** for this
project's current scope - all 10 targeted scenarios hit exactly, plus 12
adjacent already-passing scenarios spot-checked with no regressions.

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
