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

## Increment: content-document-xhtml/svg, first slice (2026-07-02)

Next family: `epub3/06-content-document/content-document-xhtml.feature` +
`content-document-svg.feature`, repeatedly deferred in earlier "what's
left" menus as "a bigger, lower-ROI undertaking." Measured directly: 196
scenarios, 87 expecting an error/warning, only 35 hit exactly at the
start - 52 real misses, spanning far more distinct rules than any prior
increment (most prior big increments were Schematron-pattern variations
on one engine; this family needed many genuinely different hand-coded
checks). Scoped to a **first slice, Clusters A-F (~41 scenarios)**,
explicitly deferring **Cluster G (11 scenarios)** - the SVG
`foreignObject`/`title` nested-flow-content model and the MathML
`annotation-xml` content model - to a follow-up increment (real
schema-engineering, qualitatively bigger than everything else here).
`aria-roles-li-deprecated-warning` (a full DPUB-ARIA role taxonomy)
stays out of scope, as named in an earlier increment.

**Cluster A - reference/fragment reclassification.** Real corpus finding,
grep-verified across the *entire* corpus: `RSC-001` is used only for a
manifest `item/@href` missing from the container and a CSS `@import`
target (handled in `css.rs`) - every other "this content-doc reference
doesn't resolve" case is `RSC-007`, and a fragment that doesn't resolve
is `RSC-012`. Reclassified the generic per-content-doc resource-scan loop
from `RSC-001` to `RSC-007` (and updated `scripts/spike.py`'s
`broken_ref.epub` fixture to match - the fixture encoded the old,
now-corrected assumption); added `altimg` to that same loop (MathML's
alt-image is a resource reference too); added same-doc/cross-doc/nav-doc
`<a href>` fragment resolution (`RSC-012`).

**Cluster B - URL/base/fragment validity.** New `src/url.rs` (no new
dependency, same style as `smil.rs`'s clock-value parser): `RSC-020` for
URL syntax errors (spaces, missing `//` after `http(s):`, invalid host
characters) and `HTM-025` for an unregistered scheme - both scoped to
absolute `http`/`https` URLs only after real false positives surfaced
(see below). Base-URI-aware remote reclassification: an absolute-remote
`<base href>`/`xml:base` makes every relative-or-fragment-only `<a href>`
elsewhere in that document resolve to a remote URL (`RSC-006`, additive
to - not replacing - the existing image-hyperlink `RSC-006` check).
Fragment/resource-type classification, one dedicated real fixture each:
`RSC-013` (stylesheet link with a fragment), `RSC-014` (hyperlink to an
SVG `<symbol>` - "incompatible resource type"), `RSC-015` (SVG `<use>`
with no fragment), `RSC-009` (non-SVG image referenced via a fragment),
`RSC-008` (an `<img srcset>` candidate not declared in the manifest).

**Cluster C - structural RSC-005/RSC-017 checks**, all in the existing
per-content-doc loop: duplicate `id`; ID-referencing attributes
(`aria-*`, `for`, `list`) resolving to a real same-document id; empty
`<img src>`; `lang`/`xml:lang` mismatch; `usemap` needing a leading `#`
(EPUB3-only, see below); `http-equiv="Content-Type"` value; HTML5
microdata `itemprop` placement (a curated element -> required-attribute
table); nested `<dfn>`; missing `<title>` entirely; `epub:trigger`
(deprecated + `ref`/`ev:observer` id resolution).

**Cluster D - entities and datetime grammar.** New raw-text entity
scanner in `htm.rs` (`RSC-016`, mapped to `Severity::Error` - no `Fatal`
variant exists) for a missing `;` or an undeclared/unknown entity name -
runs regardless of whether the document parses at all, since `roxmltree`
simply fails to parse either malformed case. A real, exhaustively
reverse-engineered HTML5 `<time datetime>` microsyntax
(`is_valid_html5_datetime`, `htm.rs`) - built and unit-tested directly
against all 25 of the real corpus's invalid values *and* all 32 of its
valid values, since several rules are non-obvious (separator may be `T`
*or* a literal space; a fractional-seconds part is capped at 1-3 digits
everywhere it appears; a "duration" has two entirely different valid
shapes - an ISO-8601-like `P...T...` form, or a bare whitespace-separated
`<number><unit>` sequence with no `P`/`T` at all).

**Cluster E - epub:type taxonomy extension to content docs.** Reused
`smil::is_default_vocab_type`/`OPF-088` (built for SMIL) for XHTML
content docs. Two new taxonomies confirmed via real fixtures: deprecated
SSV terms (`OPF-086`, reusing the existing rendition-property constant -
epubcheck itself reuses IDs across conditions routinely) and
misuse/redundant-with-host-element terms (new `OPF-087`, e.g.
`<table epub:type="table">` restates its own element's native semantic).

**Cluster F - small CSS/MathML/namespace usage checks:** `CSS-005`
(stylesheet link's `class` names more than one alt-style-tag), `CSS-015`
(alternate-stylesheet link missing/empty title), `CSS-008` extended to
`style="..."` attributes (wrapped in a throwaway rule so styloria's
existing tokenizer produces the same `&[ComponentValue]` shape
`check_declaration_shapes` already expects, rather than adding a
styloria entry point for one caller), `HTM-010` (unrecognized `epub:`
namespace URI), `ACC-009` (new `ACC` family: MathML with no alt text).

**Real false positives found via the corpus and fixed, not guessed
up front - a longer list than usual, since URL/host/scheme grammars are
exactly the kind of rule real-world documents stress in ways a single
"invalid" fixture doesn't anticipate:**
1. Internationalized domain names (`https://ü.example.org`) and
   percent-encoded hosts (`%C3%BC`) were misread as invalid host
   characters; fixed by only rejecting stray *ASCII* punctuation
   (non-ASCII and `%` are always allowed).
2. The "`//` required after scheme" and host-character rules were
   applied to *every* absolute scheme uniformly, false-positiving on a
   legitimate `mailto:` link (mailto never has `//`) - scoped to
   `http`/`https` only, matching what the real fixtures actually test.
3. An underscore in a host (`w_w.example.com`, non-standard but accepted
   by most browsers per a real fixture's own comment) was rejected;
   added to the allowed host-character set.
4. `usemap` without a leading `#` is EPUB3-only invalid - EPUB2's XHTML
   1.1 DTD retyped `usemap` as URIREF, explicitly permitting the bare
   form too (confirmed via a real, deliberately-commented EPUB2
   fixture); the check is now gated on `is_epub3`.
5. `http-equiv="Content-Type"` content-value comparison was
   case-sensitive; a real fixture uses `Text/HTML; Charset=UTF-8` and is
   valid - fixed with `eq_ignore_ascii_case`.
6. `<output for="o2 o3">` is a space-separated *list* of control ids
   (like the ARIA attributes), unlike `<label for>`/`<input list>`,
   which each name a single id - confirmed via a real fixture combining
   both forms in one document; `<output>` now gets its own multi-token
   handling.
7. ACC-009 (MathML no-alt-text) fired on several "valid" fixtures that
   have no `alttext` attribute but *do* have a `<semantics><annotation-xml>`
   child providing an alternative representation - fixed by exempting any
   `math` element with an `annotation`/`annotation-xml` descendant.
8. `&foo;` inside an HTML comment or `<![CDATA[...]]>` section is literal
   text, not a real entity reference (confirmed via a real fixture titled
   exactly this) - the raw entity scanner now masks any `&` found inside
   those spans before scanning.
9. `<img srcset="...">` candidates were checked against `name_index`
   (container file existence) instead of `items` (manifest declaration) -
   the real fixture's undeclared candidate genuinely exists as a file, so
   the original check never fired at all; fixed to check manifest
   declaration.
10. `epub:type="endnote"` is deprecated only when used *without* being
    nested inside its proper `endnotes` container - a real "valid" fixture
    nests `<div epub:type="endnote">` inside `<section
    epub:type="endnotes">`, while the deprecated-usage fixture uses it
    standalone; fixed with an ancestor-based exemption specific to this
    one term (the only one of the 13 deprecated terms with a real
    fixture contradicting a blanket rule).
11. A same-document `<a href="#frag">` fragment reference was never
    reaching the new RSC-012/RSC-006/RSC-014 checks at all, because the
    pre-existing `is_external(href)` guard (correctly built for the old
    "does this file exist" check) treats *any* fragment-only href as
    "skip resolution" - reordered so a fragment-only href only bails out
    when it's genuinely remote/`data:`/`mailto:`/`tel:`, not merely
    `#`-prefixed.
12. Two real, corpus-confirmed EPUB Structural Semantics terms were
    missing from the default-vocabulary allowlist entirely (`title`,
    `region-based`), causing spurious `OPF-088` usage on legitimate
    region-based-nav and edupub title/subtitle fixtures - added both.
13. Same class of bug as `nav-cfi-valid`'s CFI fragment: a hyperlink's
    fragment may be a CFI (`epubcfi(...)`) or a Media Fragments URI
    (`xywh=percent:5,5,15,15`), neither a plain id - both real, valid
    constructs (confirmed via `nav-cfi-valid`/`region-based-nav-valid`)
    that RSC-012 must not try to resolve as an id; and a hyperlink whose
    target resolves to the OPF document itself (a CFI self-reference)
    has no ids to resolve against at all - both now skipped.

**Two harness-only fixes, not product bugs:** `scripts/corpus.py`'s
`parse_features` didn't skip Gherkin comment lines (`#Then ...`), so a
disabled assertion (`img-alt-missing-error.xhtml`, which even epubcheck's
own suite doesn't enforce, per its own `FIXME`) was misread as a real
expectation; and `"X is reported 0 times"` (a negative assertion, used by
exactly 2 scenarios in the whole corpus) was misread as *expecting* that
ID, backwards from its actual meaning - both fixed. Also added `RSC-012`
to the existing `single_doc_wrap` scoring-exclusion set (alongside
`RSC-001`/`RSC-007`/`RSC-011`/`RSC-008`/`OPF-014`): a bare-fixture wrap
demotes every *other* directory-sibling xhtml/html file to an inert media
type, so a cross-doc fragment a fixture legitimately references may live
in a sibling this harness never actually parses as a real content
document - a wrapping-harness gap, not a defect in the fixture under
test.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| scenarios measured | 981 | 981 (same denominator; the two harness fixes shifted 1 scenario each between buckets, net neutral) |
| exact-ID recall | 51.9% (310 hits) | **59.4% (352 hits)** |
| within target-family spot-check (content-document-xhtml/svg) | 35/87 | **72/85** (all Cluster A-F scenarios hit exactly; the 12 remaining misses are the 11 Cluster-G deferrals + the 1 out-of-scope DPUB-ARIA scenario) |
| RSC family exact hits | 174/379 | **209/377** |
| HTM family exact hits | 26/31 | **28/31** |
| CSS family exact hits | 15/20 | **18/19** |
| ACC family exact hits | 0/1 | **1/1** |
| false positives | 1 | **2** (the same known RELAX NG gap, plus one newly-exposed, epubcheck-acknowledged single-document-mode limitation - `unique-identifier-not-found-error.opf` carries a `# FIXME this error should be detected... in single-document mode` comment in epubcheck's own suite; our validator has no separate single-document mode and correctly detects it always, which epubcheck's own bare-`.opf` check mode doesn't) |

**Next follow-up, explicitly named:** Cluster G (11 scenarios - SVG
`foreignObject`/`title` nested flow-content model, MathML `annotation-xml`
content model with its encoding-equivalence table) and the still-larger
`content-document-xhtml`/`svg` content-model tail beyond that (~54 more
scenarios not yet scoped in detail) remain the natural next slice of this
family.

## Increment: Cluster G - SVG foreignObject/title + MathML annotation content models (2026-07-02)

Closed out Cluster G, the 11 scenarios explicitly deferred from the
previous increment as needing real schema-engineering. All grounded in the
real corpus's error *and* valid fixtures (clean-room, read-only) - several
rules here only resolve by diffing matched valid/invalid pairs, not from a
single fixture.

**SVG `foreignObject` (3 scenarios) - reused the existing schema, no engine
changes.** Real finding: the errors are ordinary XHTML content-model/
attribute violations surfacing inside `foreignObject` (`element "body" not
allowed here`, `element "title" not allowed here`, `attribute "href" not
allowed here`). Mechanism: `roxmltree::Node::range()` (the `positions`
feature is already a default feature of our `roxmltree` dependency) gives
the exact original-text byte span of any node, so `foreignObject`'s inner
XML can be reconstructed verbatim, wrapped in a synthetic
`<html>...<body>{inner}</body></html>` that carries forward every
namespace binding from the real document's root (so prefixed content still
resolves), re-parsed, and validated via the **already-built**
`crate::rng::xhtml_grammar()` - zero RNG engine changes. New `src/svg.rs`.
**EPUB3-only**: a real EPUB2 fixture (`svg-foreignObject-switch-valid.xhtml`,
titled "body allowed inside foreignObject") explicitly permits a bare
`<body>` there, unlike EPUB3's own error fixture for the identical shape -
same EPUB2/EPUB3 content-model split precedent used elsewhere in this
project.

**A real gap found via this reuse, not guessed:** `schemas/xhtml.rng`'s
attribute handling is a deliberately permissive global catch-all (not a
per-element attribute allowlist - see `anyOtherAttr`'s own design comment),
so `href` on a `<p>` isn't actually caught by the flow-content grammar at
all. Added a small, narrow, real HTML5 rule alongside the reuse: `href` is
only valid on `a`/`area`/`link`/`base`; any other XHTML-namespaced element
carrying it is `RSC-005`.

**SVG `title` (2 scenarios) - NOT flow-content, a real fixture proves it.**
`svg-title-content-valid.xhtml` shows a bare `<body>` and even a **whole
embedded `<html>` document** are valid title content - far more permissive
than `foreignObject`. So `title`'s rule is a plain recursive namespace
check (any descendant not in the XHTML namespace -> `RSC-005`, "elements
from namespace X are not allowed") plus the same narrow `href` rule above -
confirmed via a fixture where only a *nested* `<svg>` inside an otherwise-fine
XHTML `<body>` is flagged, never the `<body>` itself.

**SVG generic vocabulary (1 scenario, usage-level).** `svg-invalid-usage.xhtml`
wants unrecognized SVG elements reported as `Severity::Info` (new `RSC-025`) -
SVG conformance is deliberately non-blocking, unlike XHTML's. A curated,
generously-inclusive real SVG 1.1 element allowlist; walks every top-level
`<svg>` subtree, stopping at `foreignObject`/`title` boundaries (their own
rules apply) and only ever looking at SVG-namespaced children (so foreign
content like embedded RDF in `<metadata>` is never touched). Confirmed
against `svg-regression-valid.xhtml` (SVG's own `<a xlink:href>` with
`rel`/`target` - a false-positive precedent the fixture's own comment
already calls out) and `svg-rdf-valid.xhtml`.

**MathML (5 scenarios) - reverse-engineered from 5 error + 8 valid
fixtures.** New `src/mathml.rs`. A `<math>` element's own content (outside
`annotation`/`annotation-xml`) must be Presentation MathML - a curated
allowlist walk, stopping recursion at the first violation (confirmed: the
real fixture reports exactly one finding per invalid subtree, not one per
nested element inside it too). `annotation-xml`'s `encoding` attribute must
be one of a closed, real enumeration (Content/Presentation/XHTML/SVG
variants); an unrecognized value is `RSC-005` and **short-circuits every
other check** for that element (the fixture expects exactly one finding,
not a cascade). `name` is required, and constrained to exactly
`"contentequiv"`, **only** when `encoding` is a Content-MathML value -
confirmed via a real fixture where an XHTML-encoded annotation omits `name`
entirely and is valid. When `encoding` is Presentation-MathML, the
annotation's own content must also be Presentation MathML (reuses the same
allowlist walk). Deliberately left unenforced, confirmed safe by a real
fixture: the inverse (Content-MathML encoding with presentation content)
and xhtml/svg-encoding content-type mismatches - one valid fixture nests a
`<math>` *inside* an xhtml-encoded annotation despite its own comment
suggesting real epubcheck's Schematron might otherwise object, and no
scenario here enforces that.

**A real, corpus-wide harness regression found and fixed, not a product
bug:** the new SVG checks, once wired in, needed a second pass over
standalone top-level SVG content documents (`image/svg+xml`) - the existing
per-content-doc loop is scoped to `application/xhtml+xml` only, so a bare
`.svg` file (like `content-svg-use-href-no-fragment-error`'s `cover.svg`)
never went through it at all. Added a dedicated loop over `svg_doc_paths`
(already computed for the CSS-029/030 cross-reference) running vocabulary/
foreignObject/title/`<use>`-fragment checks. This immediately surfaced a
real `scripts/corpus.py` gap: `wrap_single_doc`'s "include all directory
siblings so relative refs resolve" convention already demoted other bare
`.xhtml`/`.html` siblings to an inert media type (since the shared `files/`
directories hold dozens of independent, unrelated fixtures) - but never
extended that demotion to `.svg` siblings, since SVG had no content-model
checks before this increment. Once it did, every single-doc-wrapped
scenario in a `06-content-document/files/` directory started sweeping in
every *other* unrelated `.svg` fixture in that same folder as if it were
part of the same book, producing a flood of unrelated findings. Fixed by
extending the existing xhtml/html demotion to also cover `image/svg+xml`.

**Two real, narrow bugs found via testing, not by inspection:** (1)
`roxmltree::Node::ancestors()` includes the node **itself** as its first
item (confirmed by reading its source) - the top-level-`<svg>`-root filter
(`!ancestors().any(is an svg)`) matched every `<svg>` against itself and
silently excluded all of them; fixed with `.skip(1)`. (2) the presentation-
MathML walk's first version still recursed into an already-rejected
element's children, producing 3 findings (`apply`, then its own nested
`csymbol`/`ci`) where the real fixture expects exactly 1 - fixed by not
recursing past the first violation in a subtree.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 59.4% (352 hits) | **61.4% (364 hits)** |
| within target-family spot-check (11 Cluster G scenarios + adjacent valid) | 72/85 | **all 11 hit exactly, 18 adjacent valid fixtures confirmed clean** |
| RSC family exact hits | 209/377 | **222/377** |
| false positives | 2 | 2 (same two known, unrelated gaps - the RELAX NG hyphenated-custom-element limitation and the epubcheck-acknowledged single-document-mode limitation) |

With this, `content-document-xhtml`/`svg`'s Cluster G is **done**. The
remaining, not-yet-scoped tail of this family (~54 more scenarios, per the
prior increment's estimate) is the next natural slice.

## Increment: content-document-xhtml/svg family finished (CSS split + ARIA) (2026-07-02)

Re-measured before starting the next slice, since the prior increment's
"~54 more scenarios" estimate turned out stale. Real state: `content-
document-svg.feature` was already 100%, `content-document-xhtml.feature`
was 83/84 (only the deliberately out-of-scope `aria-roles-li-deprecated-
warning` DPUB-ARIA scenario missing), and `content-document-css.feature`
had 2 real misses left. Closed both remaining gaps rather than moving to
an unrelated family.

**CSS RSC-001/007/008 split (2 scenarios).** The exact same split already
established for XHTML content-doc references (RSC-001 = manifest-declared
resource whose file is missing; RSC-007 = undeclared and absent; RSC-008 =
undeclared but the file genuinely exists) had never been applied to CSS's
own `url()`/`@import` resolution, which still reported a blanket RSC-001
for everything missing. Confirmed via three distinctly-named real
fixtures (`content-css-import-not-present-error` - declared, missing,
already correctly RSC-001; `content-css-import-not-declared-error` -
undeclared, present, needs RSC-008; `content-css-url-not-present-error` -
undeclared, absent, needs RSC-007) that the same three-way split applies
uniformly to every CSS `url()` construct, not just `@import`. `css::check`
gained a new `manifest_paths: &HashSet<String>` parameter (computed once
in `opf.rs::check` alongside the existing `name_index`) to distinguish
manifest declaration from container file existence - both existing call
sites updated. Two of `css.rs`'s own unit tests (synthetic, no manifest
context) had their expected ID corrected from `RSC-001` to `RSC-007` -
same "fixture must keep pace with the now-correct rule" precedent as
several earlier increments.

**Deprecated DPUB-ARIA roles (1 scenario, the last remaining item in
`content-document-xhtml.feature`).** Real corpus finding: every other
ARIA/DPUB-ARIA scenario in the whole feature file is a "-valid" fixture
that already stays clean with no role-validity check at all (role
attribute values aren't restricted by the schema today) - `aria-roles-
li-deprecated-warning` is the *only* negative assertion in the entire
cluster. Rather than building the fuller "which roles are valid on which
host elements" taxonomy the scenario's title suggested (`aria-roles-li`),
no fixture anywhere actually tests per-element role validity - only that
`doc-endnote`/`doc-biblioentry` are deprecated, confirmed regardless of
host element (the real fixture fires the warning on both a `<li>` and a
`<div>` carrying the same role). Implementing the wider taxonomy would
have been guessing rather than following evidence, so this stayed
narrowly scoped to exactly what's tested - a two-entry deprecated-role
list, checked on any element's `role` attribute, `RSC-017`/Warning per
occurrence.

**Honest numbers:**

| metric | before | after |
|---|---|---|
| exact-ID recall | 61.4% (364 hits) | **61.9% (367 hits)** |
| `content-document-xhtml.feature` | 83/84 | **84/84 (100%)** |
| `content-document-svg.feature` | 100% | 100% (unchanged) |
| `content-document-css.feature` | 10/12 | **12/12 (100%)** |
| CSS family exact hits | 18/19 | **18/19** (unchanged - both fixes were RSC-001/007/008/RSC-017, not new CSS-prefixed codes) |
| RSC family exact hits | 222/377 | **226/377** |
| false positives | 2 | 2 (same two known, unrelated gaps) |

With this, the `content-document-xhtml`/`svg`/`css` family is **fully
done** - the only miss anywhere in these three feature files is the one
false positive shared with every other increment (the pre-existing RELAX
NG hyphenated-custom-element-name limitation). The next natural slice is
a genuinely different family: `epub3/03-resources` (56 misses),
`epub3/04-ocf` (26), `navigation-document.feature` (19), or
`epub-dictionaries` (17+9) - all unscoped so far.

## Increment: epub3/03-resources, finished in full (2026-07-03)

Closed out `epub3/03-resources.feature` entirely - all 59 should-error
scenarios hit exactly, 0 misses. Scoped as 10 clusters (research-first,
same clean-room corpus-reading stance as every prior increment), the
biggest single-family push since `05-package-document`.

**Cluster 1 - foreign-resource fallback model (RSC-032/MED-003/MED-007),
new `src/cmt.rs` + `src/foreign.rs`.** EPUB 3's actual rule, reverse-
engineered from real fixture pairs rather than assumed: a resource is
"foreign" if its declared media-type isn't a Core Media Type (§3.2); a
foreign resource used anywhere needs a fallback (a manifest `fallback`
chain reaching a Core Media Type), **except** `video/*` (no CMT exists for
video at all, so it's exempt everywhere - confirmed via a foreign
`video/avi` resource used directly as an `<img src>` with no fallback,
still valid) and `<link>`/`<track>` targets (§3.4, always exempt). `<audio>`/
`<video>` additionally support an *intrinsic* fallback: a group of
candidate resources (the element's own `@src`, or its child `<source>`
list) is fine as long as *any one* candidate is usable. A `<picture>`'s own
`<img>` fallback is held to a *stricter* rule (must itself be a Core Media
Type, no manifest-fallback rescue at all - it's the picture's "always
works" raster fallback, MED-003); its `<source>` elements are exempt only
when they declare a `type` attribute (MED-007 otherwise, regardless of any
manifest fallback). `src/cmt.rs` centralizes the Core Media Type list
itself (assembled from the corpus's own `resources-core-media-types-*.opf`
fixtures) since three separate clusters this increment (1, 6, 9) all need
the same classification.

**Cluster 2 - remote-resource-misuse expansion (RSC-006/RSC-031).**
EPUB 3 restricts *where* a remote resource may be used, independent of the
foreign/fallback question above: `<img>`/`<iframe>`/`<script src>`/a
stylesheet reference (`<link rel=stylesheet>`, an SVG `<?xml-stylesheet?>`
PI, or a CSS `@import`) can never be remote; `<object>` follows its
resource's own category (exempt only if audio/video/font - confirmed via
`resources-remote-audio-object-valid` vs `-object-undeclared-error`); a
manifest item typed `application/xhtml+xml` can never be remote *at all*,
even if never referenced anywhere (a real fixture declares one unused,
still an error) - deliberately **not** extended to `image/svg+xml`, since
SVG is dual-purpose (a real fixture uses a remote `image/svg+xml` item
exclusively as an `@font-face` font, which must stay valid). RSC-031 (any
plain `http://` instead of `https://`) is a flat, context-independent scan
over every genuine embedded-resource reference.

Two real false positives found via the corpus, not by inspection: (1) a
`<link rel="foaf:topic" href="http://...">` (an RDFa vocabulary term used
as `rel`, from `rdfa-valid.xhtml`) was being treated as "using a remote
resource" by the pre-existing generic attribute scan - fixed by only
tracking a `<link>`'s `href` as a resource reference at all when its `rel`
contains "stylesheet" (metadata/navigation `rel` values aren't resource
dependencies). (2) that same pre-existing scan was *also* inserting plain
`<a href="http://...">` hyperlink URLs into the general remote-resource
set (contradicting its own code comment, which already said hyperlinks
shouldn't count) - once RSC-031 started scanning that set unconditionally,
ordinary external hyperlinks in `rdfa-valid.xhtml` started false-positiving
on both RSC-008 (undeclared) and RSC-031. Fixed by only tracking hyperlink
URLs in the separate `remote_link_refs` set (already used for the image-
hyperlink RSC-006 check), never the general one.

**Cluster 3 - data URLs (RSC-029), extending `src/foreign.rs`.** `data:`
URLs are flatly disallowed in structural contexts (manifest item href,
package `<link>` href, `<a>`/`<area>` href in HTML *or* SVG - the latter
via `xlink:href`) but allowed as an ordinary resource reference in `<img>`/
`<picture>` (classified by the media-type declared *inside* the URL
itself, e.g. `data:image/jpeg;base64,...` - a `data:` URL can never carry a
manifest `fallback`, so a foreign one can only be rescued by an intrinsic
mechanism like a `<picture><source type=...>`).

**Cluster 4 - file: URLs (RSC-030).** Simpler than data: URLs - disallowed
everywhere, no exceptions, across manifest item href, package `<link>`
href, any content-doc attribute, and CSS `url()`/`@import`. `is_external`
gained `file:` to its exclusion list (alongside `data:`/`mailto:`/`tel:`)
so a bare `file:example` (no `//`) doesn't get misresolved as a relative
container path before the file: check even runs.

**Cluster 5 - XML encoding conformance (RSC-027/028/016).** The OPF's own
bytes were being decoded with a blind `String::from_utf8_lossy`, so a
genuinely UTF-16/Latin-1/UTF-32-encoded package document just produced
garbage that either failed to parse (falling through to a generic "not
well-formed" message) or silently misbehaved. New `decode_opf_bytes` in
`opf.rs`: sniffs a real BOM (UTF-8/UTF-16 either endian) or, for BOM-less
UTF-32, the XML spec's own Appendix F autodetection pattern (`00 00 00
'<'` / `'<' 00 00 00`); reports **RSC-027** (warning) for genuine UTF-16 -
still decodable, so checking continues - and **RSC-028** (error) for any
other non-UTF-8 encoding (UTF-32, Latin-1, or any other *recognized*
declared name); reports **RSC-016** (fatal, aborting all further checks)
*additionally* when the declared encoding doesn't match the actual bytes
(a UTF-16-BOM'd file declaring `UTF-8`) or names an encoding nobody
recognizes at all - confirmed via real fixture pairs that isolate each
combination. Also reclassified the OPF's own "not well-formed XML" /
"undeclared namespace prefix" parse-failure fallback from RSC-005 to
**RSC-016** (real epubcheck's actual ID for a genuine fatal parser
failure, confirmed via two dedicated fixtures) - `scripts/spike.py`'s own
`opf_malformed.epub` regression fixture had encoded the old, now-corrected
ID and needed updating to match, same "fixture must keep pace" precedent
as every prior increment that changed a message ID.

**Cluster 6 - OPF-090 (non-preferred Core Media Type usage), new.** A
closed set of legacy-but-valid MIME aliases (`application/font-sfnt`,
`application/x-font-ttf`, `application/vnd.ms-opentype`,
`application/font-woff`, `application/ecmascript`, `text/javascript`) is
flagged Info/usage when used instead of the modern preferred form.
**A real classification bug found via the corpus, not guessed:**
`application/x-font-woff` was initially folded into this "valid, non-
preferred" set (it appears in the pre-existing, differently-scoped
`FONT_CORE_MEDIA_TYPES` used only for font-*obfuscation* eligibility) -
but no real "valid Core Media Type" fixture anywhere actually uses it;
`foreign-exempt-font-valid` uses exactly that media-type and expects it
treated as a genuinely *foreign* font (CSS-007, cluster 9). Removed from
`cmt.rs`'s CMT list, with the distinction documented so the two separate
"is this a plausible font" vocabularies (font-obfuscation eligibility vs.
the real Core Media Type list) don't get conflated again.

**Cluster 7 - OPF-013 (MIME-type mismatch), new.** A `type` attribute on
`<object>`/`<embed>`, or a `<picture><source>`'s `srcset` target, must
match the resource's own manifest-declared media-type - warning if not.
An ordinary resource-hygiene check, not EPUB-specific (same convention
epubcheck itself follows, reusing an "OPF-" prefixed code for it).

**Cluster 8 - OPF-045 circular fallback chains, new.** A `fallback`
chain that eventually loops back on itself (not just a direct self-
reference, already caught) is an error - same DFS-cycle-detector shape
already used for OPF-065's `@refines`-cycle check, just walking
`fallback_map` (already built) instead of the DOM again.

**Cluster 9 - CSS-007 (exempt font usage), new.** A `@font-face src`
resolving to a genuinely foreign (non-CMT, non-exempt-video) font is
allowed (fonts are always exempt from needing a fallback, §3.4) but
flagged as Info-level usage. New `css::font_face_src_urls` (parallel to
the existing `check_font_face`, but *collecting* `src` targets instead of
just checking for emptiness - the pre-existing `collect_urls` pass
deliberately skips `@font-face` blocks entirely, so this needed its own
extraction), cross-referenced against the manifest's declared media-type
in `opf.rs` (the same cross-referencing shape as CSS-029/030), wired into
both the standalone-`.css`-manifest-item loop and inline `<style>` blocks.

**Cluster 10 - image signature sniffing, new `src/image.rs`.** Originally
scoped as "likely deferred" (a bigger, separate undertaking), but turned
out bounded enough to finish in the same increment: magic-byte detection
for the four raster Core Media Types (JPEG/PNG/GIF/WebP - SVG is XML,
already validated as such elsewhere) against every manifest item declaring
one of them. **MED-004 + PKG-021** (both fire together) when the bytes
don't match any known signature at all (confirmed via a real 0-byte file
declared `image/jpeg` - corrupt/truncated, not just "wrong format").
**OPF-029** when the sniffed actual format doesn't match the declared
media-type (a real JPEG declared as `image/gif`). **PKG-022** (warning)
when the sniffed format *matches* the declared type but the file's own
extension doesn't (a real JPEG named `image.gif`, correctly declared as
`image/jpeg` in the manifest) - these three are mutually exclusive checks
on the same sniff result, confirmed via three distinctly-shaped fixtures
that never combine more than one condition at once.

**A real measurement-harness bug found via this increment's own Info-
severity additions (OPF-090/CSS-007), not guessed:** `scripts/corpus.py`'s
single-doc-wrap scenarios recomputed their pass/fail `rc` as "any message
ID remained after discarding harness artifacts" - unlike the *real* CLI
exit code (`Report::is_valid()`, errors-only), this recomputation didn't
check severity at all, so a wrapped "should stay clean" fixture that
happened to trigger a brand-new Info-level usage message (like OPF-090 on
`resources-core-media-types-valid.opf`, which legitimately mixes preferred
and non-preferred font aliases) looked like a false positive purely from
the harness's own inconsistency, not a real defect. Fixed by switching
`run()` to parse `--format human` (which carries severity) instead of
`--format ids`, and recomputing wrapped-scenario `rc` from only the
Error-severity subset - consistent with the real CLI's own semantics. This
also corrected the "detection recall" metric (58.7%→~59%-ish range across
increments) to no longer overcount wrapped Warning/Info-only scenarios as
"detected" the way it silently had been - a measurement correction, not a
product regression; **exact-ID recall**, the metric this project has
always treated as the headline number, was never affected by this bug
either way.

**Two more instances of the already-accepted "no separate single-document
mode" limitation** (first documented in the content-document-xhtml/svg
increment): `resources-remote-xhtml-error.opf` and `resources-remote-
audio-valid.opf` both declare a manifest item that's only invalid when
checked as part of a full, cross-referenced publication (a remote XHTML
content-document item, or an unrelated one alongside valid remote audio) -
epubcheck's own bare-`.opf` single-file check mode doesn't run that
whole-publication check, but this project always validates fully, so it
correctly (if more strictly than epubcheck's own single-file mode) flags
them. Same accepted tradeoff as before, not a defect to chase.

**Honest numbers** (`epub3/03-resources.feature`: all 59 should-error
scenarios hit exactly, 0 misses; overall corpus numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 61.9% (367 hits) | **71.5% (424 hits)** |
| RSC family exact hits | 226/377 | **271/377** |
| OPF family exact hits | 72/146 | **81/146** |
| PKG family exact hits | 19/36 | **21/36** |
| CSS family exact hits | 18/19 | **19/19 (100%)** |
| MED family exact hits | 12/16 | **16/16 (100%)** |
| false positives | 2 | 4 (2 pre-existing + 2 new instances of the same accepted single-document-mode limitation) |

With this, `epub3/03-resources.feature` is **fully done**. Remaining
unscoped families: `epub3/04-ocf` (26 misses), `navigation-document.feature`
(19), `epub-dictionaries` (17+9).

## Increment: epub3/04-ocf, finished in full (2026-07-03)

Closed out both `epub3/04-ocf/ocf.feature` and `.../filename-checker.feature`
entirely - all 43 should-error scenarios in `ocf.feature` hit exactly, 0
misses, 13/13 clean fixtures stay clean. Scoped as 5 clusters.

**Cluster E - ZIP-level error classification (PKG-003/004/005/008/027),
first since it's foundational.** The pre-existing `ocf::open` treated
every `ZipArchive::new` failure identically (blanket PKG-004) - real
epubcheck distinguishes: an **empty file** (PKG-003 + PKG-008, "the zip
file is empty"); bytes that are actually a recognizable *other* format
(reused `image::sniff_image_type` to detect a JPEG mis-named `.epub` -
PKG-004 *and* PKG-008 together, a more specific "corrupted header"
defect); a generic truncated/malformed archive with no recognizable
format (PKG-008 alone). Also new: **PKG-027** (a ZIP entry's file name
isn't valid UTF-8, checked via `name_raw()` bytes directly rather than
the crate's own lossy-decoded `name()`, which never fails) - fatal,
aborts all further checking, confirmed via a real fixture using raw
CP437-encoded bytes for an 'é'. **PKG-005** (the `mimetype` entry's ZIP
header must carry no extra field, needed so tools can sniff the media
type at a fixed byte offset without a full ZIP parse) via `extra_data()`.

**Cluster A - file-name conformance, new `src/filename.rs` - the
biggest cluster.** Three checks assembled from the corpus's own
`filename-checker.feature` table (forbidden characters, non-ASCII usage,
trailing full stop) plus **OPF-060** (duplicate names after case-folding/
normalization). Real, non-obvious findings: (1) OPF-060's real rule,
reverse-engineered from 4 fixture pairs, is "NFC-normalize, then full-
case-fold" - canonically-equivalent names (precomposed vs. decomposed
Á) and case-fold-equivalent names (`CONTENT_001` vs `content_001`; German
ß vs "ss", needing a hardcoded special case since Rust's `to_lowercase`
performs simple lowercasing, not full Unicode case *folding*) collide,
but merely *compatibility*-equivalent names (math double-struck ℍ vs
plain H) must **not** - confirmed via a dedicated "-valid" fixture. (2) A
genuine `zip` crate limitation: `ZipArchive` de-duplicates same-named
entries into one `IndexMap` slot internally (a later central-directory
record silently overwrites an earlier one), so its own entry list can
*never* expose a true duplicate-name defect - confirmed via a real
fixture with 6 central-directory records but `ZipArchive::len()`
reporting only 5. Worked around with a small hand-rolled raw-bytes
central-directory scanner (`find_exact_duplicate_entry`, same "narrow
binary-format parser" style as `image.rs`'s signature sniffing), run on
the bytes *before* they're moved into the crate's own reader. (3) real
epubcheck's single-package-document check mode has no actual container
to inspect, so PKG-009/012 also validate a manifest item's declared
`href` string directly when the resource doesn't actually exist (gated on
that, to avoid double-reporting the same defect for a normal, fully-
resolvable publication where the real file name is already checked).

**Two real false positives found via the corpus, not guessed:** a
Unicode emoji tag sequence (a real flag emoji, using the same "Tags"
codepoint block E0000-E007F that also contains the deprecated LANGUAGE
TAG character) was being rejected entirely - the real rule (confirmed by
re-reading the filename-checker table closely) forbids only the single
literal `U+E0001` LANGUAGE TAG codepoint, not the whole block; the tag
*letters* and the reinstated CANCEL TAG are fine. And the new manifest-
href PKG-009 check initially flagged a URL's own `?query` delimiter as a
"forbidden character" (`?` is in the forbidden set) - fixed by stripping
query/fragment before extracting path segments, same convention already
used elsewhere for href handling.

**Cluster B - PKG-025 (publication resource in META-INF).** Only a closed
set of reserved file names may live directly in META-INF. **A real,
version-scoped false positive found via the corpus, not guessed:** a
dedicated EPUB **2** fixture, "Ignore unknown files in the META-INF
directory," explicitly expects *no* error for the exact same shape EPUB 3
forbids - so this check had to move out of `ocf::open` (which runs before
the OPF, and thus the package version, is even known) into `opf.rs`,
gated on `is_epub3`.

**Cluster C - URL checks (RSC-026, RSC-033) + `cite` attribute
(RSC-007).** **RSC-026**: a manifest item href that's path-absolute
(`/...`) or whose `..` segments, honestly walked from `base_dir`, escape
above the container root entirely - `resolve()`'s own path-joining is
deliberately lenient here (a `pop()` past empty is a harmless no-op, so a
leaking href still resolves to the "intended" real path), so a separate,
stricter `href_leaks_container_root` does the actual flagging. **RSC-033**:
a query string (`?...`) on any *local* reference (manifest item href,
package `<link>` href, or a content-doc hyperlink) - remote references are
exempt (query strings are meaningful there). **`cite`** (on `<blockquote>`/
`<q>`/`<ins>`/`<del>`) just needed adding to the existing generic attr-scan
list for the RSC-007 missing-resource check - but needed the same
"navigation, not an embedded dependency" carve-out `<a href>` already has
(a real false positive surfaced immediately: a remote `cite` was
wrongly treated as an undeclared embedded resource, OPF-014/RSC-008).

**Cluster D - META-INF XML content-model checks.** `<container>`'s only
real children are `<rootfiles>` and `<links>` (the Rendition Mapping
Document reference) - anything else is RSC-005. `encryption.xml`'s root
element must be named "encryption" (else RSC-005); every `Id` attribute
value must be unique document-wide (reported once *per element* sharing a
duplicate, confirmed via a real 2-element fixture expecting exactly 2
findings, not 1); the IDPF compression extension's `<Compression
Method="0|8" OriginalLength="<non-negative integer>">` attributes are
validated. `signatures.xml` (never read at all before this) gained the
same "root element must be named 'signatures'" check, wired into
`lib.rs::validate_bytes` alongside `check_encryption`.

**Honest numbers** (`epub3/04-ocf`: all 43 should-error scenarios across
both feature files hit exactly, 0 misses; overall corpus numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 71.5% (424 hits) | **75.9% (450 hits)** |
| RSC family exact hits | 271/377 | **282/377** |
| PKG family exact hits | 21/36 | **34/36** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, `epub3/04-ocf` is **fully done**. Remaining unscoped families:
`navigation-document.feature` (19 misses), `epub-dictionaries` (17+9),
`epub2` package-document rules (19), `D-vocabularies` siblings (17),
`epub-edupub` gaps (13).

## Increment: epub3/07-navigation-document, finished in full (2026-07-03)

Closed out `navigation-document.feature` entirely - all 23 should-error
scenarios hit exactly, 0 misses. New `src/navdoc.rs`: a real content model
for the EPUB 3 `<nav>` element, the first genuinely new per-element
content-model engine since the XHTML/SVG/MathML work.

**A real harness gap found first, before any product code:** the large
majority of this feature file's scenarios use a step this project had
never seen before, `Given EPUBCheck configured to check a navigation
document` - epubcheck's own single-file "check this as a nav doc"
mode. `scripts/corpus.py`'s existing `wrap_single_doc` wraps a bare
fixture as an *ordinary* content document behind a **separate** synthetic
nav, which is exactly backwards for these scenarios (the fixture itself
*is* meant to be the nav). Added `wrap_nav_doc` (declares the target with
`properties="nav"`, plus one dummy ordinary spine document so the book
isn't empty) and a new `as_nav` per-scenario flag parsed from that `Given`
line, routed in `resolve()`. Without this fix essentially the whole
feature file would have stayed unmeasurable, the same class of gap as
`wrap_opf_file`/`wrap_smil_file` before it.

**The real content model, reverse-engineered from ~20 fixture pairs, not
guessed:** a `<nav>` with **no** `epub:type` at all is completely
unrestricted (a real fixture uses arbitrary markup in one). Any `<nav>`
that **does** declare a type restricts its own children to `[heading]?
<ol>` - `toc`/`page-list`/`landmarks` don't require the heading; any
*other* named type does (a same-shaped "lot" nav fixture pair, valid with
a heading and invalid without one, pinned this down - it would have been
easy to assume all named types behave alike). An `<ol>` needs >=1 `<li>`;
each `<li>`'s first element child must be `<a>` or `<span>` (the "label" -
anything else, e.g. a bare nested `<ol>`, is "not allowed yet"); a `<span>`
label has no link of its own, so a nested `<ol>` sub-navigation is
*required* right after it, while an `<a>` label may optionally have one.
Both `<a>` and `<span>` labels need real content - non-whitespace text
*or* an `<img>` descendant (confirmed via a fixture using two images with
no text at all, one even with an empty `alt`, still valid). `page-list`/
`landmarks` specifically forbid nested sublists (RSC-017 warning, not a
content-model error - a fixture with otherwise-correct label-then-ol
ordering still gets flagged, meaning this is a policy restriction on top
of the base model, not part of it). Document-level: at least one `toc`
nav is required; more than one `page-list` or `landmarks` nav is an error
(reported once, not once per extra). `hidden` (checked on any element,
not just `nav`) is an HTML5 boolean attribute - only `""` or the literal
`"hidden"` are valid.

**New RSC-010**: a `toc` nav link must target a real Content Document
(xhtml/svg) - a real fixture links to a plain image instead. **New
landmarks-specific rules**: every entry needs its own `epub:type`
(reported once per missing occurrence); no two entries may share both an
`epub:type` token *and* their target resource, including the fragment
(reported once per offending entry - confirmed via a real 2-entry-
collision fixture expecting exactly 2 findings) - same type with a
*different* target, or different type with the same target, are each
independently valid.

**Two real, general (not nav-specific) bugs found via the corpus, not
guessed - both were latent in code from earlier increments, just never
exercised by a fixture that hit them before:** (1) the existing NAV-010
"external link" check used `is_external()` (which also treats fragment-
only/`data:`/`mailto:`/`tel:` hrefs as "external", correct for container-
path resolution but wrong here) instead of the narrower `is_remote_url()`
- a same-document `<a href="#toc">` inside a real, valid `landmarks` nav
was being wrongly flagged as an "external link". Fixed by switching that
one check to `is_remote_url`. (2) the RSC-011 "hyperlinked but not in
spine" check only verified the target *existed* in the container, not
that it was actually a Content Document - a hyperlink to a
plain image (the same `nav-links-to-non-content-document-type-error`
fixture, which expects *only* RSC-010) was also wrongly getting RSC-011,
since "should be in the spine" was never a meaningful expectation for a
non-content resource in the first place. Fixed by gating that check on
the target's manifest media-type being xhtml/svg.

**Honest numbers** (`navigation-document.feature`: all 23 should-error
scenarios hit exactly, 0 misses; overall corpus numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 75.9% (450 hits) | **79.1% (469 hits)** |
| RSC family exact hits | 282/377 | **301/377** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, `epub3/07-navigation-document.feature` is **fully done**.
Remaining unscoped families: `epub-dictionaries` (17+9 misses), `epub2`
package-document rules (19), `D-vocabularies` siblings (17), `epub-edupub`
gaps (13), `epub3/05-package-document` (11, regrew since "all 76 done").

## Increment: the `epub2` side, finished in full (2026-07-03)

Closed out all five `epub2` feature files (`opf-package-document`,
`opf-publication`, `ops-content-document-xhtml`, `ncx-publication`,
`ocf-publication`) - 67/69 should-error scenarios hit exactly, the 2
remaining named and deliberately deferred. The recurring theme this
increment, unlike prior ones: most misses weren't "check doesn't exist
yet" but **EPUB 3 checks over-applied to EPUB 2**, where the real rule
either doesn't apply at all or has the opposite polarity.

**Package-document metadata (opf-package-document.feature, 10 misses).**
New: OPF-052 (a `dc:creator`/`contributor`'s `opf:role`/`epub:role` -
both prefixes bind to the same namespace - must be a real MARC relator
code, approximated as "exactly 3 lowercase ASCII letters" since the
corpus's own fixtures - "edc"/"clr" valid, 9-letter "companion" invalid -
don't need the full ~500-entry vocabulary to distinguish); OPF-054
(`dc:date` must be empty-or-ISO-8601 `YYYY[-MM[-DD]]`, the W3C-DTF
profile); OPF-055 (`dc:title` empty is only a *warning* in EPUB 2, vs.
RSC-005 in EPUB 3 - required moving `schemas/package.sch`'s existing
`opf-title-not-empty` pattern to be EPUB-3-scoped, then hand-coding the
EPUB 2 warning separately, same "engine can't carry per-pattern severity"
precedent as the `rendition:spread` deprecation warning); OPF-037 (the
deprecated OEB 1.x `text/x-oeb1-css` media-type); OPF-041 (an obsolete
`fallback-style` attribute's target must resolve, mirroring OPF-040's
existing `fallback`-target check); OPF-042 (a spine item that's an image
specifically, vs. the generic OPF-043 warning for other non-content
types - a real fixture confirms images get their own, error-level code).
Also tightened `schemas/package.rng`'s previously-fully-permissive
"any other package child" pattern to a real allowlist (`guide`/
`bindings`/`collection`, assembled by scanning the *entire* corpus's own
`<package>` children so nothing legitimate was missed) plus any foreign-
namespaced element, catching a stray unrecognized element (RSC-005) that
the schema used to wave through by design; `<guide>` similarly tightened
to require at least one child (RSC-005 "incomplete" on an empty one).

**Package/manifest cross-references (opf-publication.feature, 8 misses,
6 fixed + 2 deferred).** New: OPF-003 (usage: a real container file not
declared as any manifest item - `mimetype`/`META-INF/*`/OS junk files
like `.DS_Store` excluded); OPF-035 (a manifest item declared `text/html`
whose real content, sniffed by trying to parse it as XML, actually *is*
XHTML); a `<spine page-map="...">` attribute (an invalid, never-
standardized Adobe extension) is always RSC-005, plus OPF-063 if it also
doesn't resolve; new `check_guide_references` for `<guide><reference>`
targets (OPF-031 if undeclared in the manifest, additionally RSC-007 if
the file doesn't exist either; OPF-032 if declared but not a real Content
Document, e.g. a plain image). **Deliberately deferred, named not
guessed:** the two `opf-legacy-oebps12-mediatype-*-warning` scenarios use
an entirely different, older package vocabulary (`xmlns="http://
openebook.org/namespaces/oeb-package/1.0/"`, capitalized `dc:Title`-style
metadata via a `<dc-metadata>` wrapper) - epubcheck's own test suite
carries a `FIXME there's no real point in reporting these since OEBPS 1.2
is not fully supported` comment on both, so building out a whole separate
legacy-namespace metadata parser for 2 scenarios epubcheck's own
maintainers consider low-value wasn't worth it.

**Content-document DOM rules (ops-content-document-xhtml.feature, 7
misses).** EPUB 2's content model is XHTML-1.1-DTD-based, not HTML5-
based - the *opposite* shape from most EPUB-3-only checks this project
has built. New `htm::check_doctype_epub2` (opposite polarity from EPUB
3's `check_doctype`: the DOCTYPE *must* carry one of a small set of real,
recognized XHTML/OEB PUBLIC identifiers - a missing one, i.e. a bare
HTML5-style `<!DOCTYPE html>`, or a malformed one, is HTM-004). New
`htm::check_dom_epub2`: a curated (not exhaustive) HTML5-only-element
blocklist (confirmed via a real fixture using `<aside>` - the only such
element used anywhere in the whole EPUB 2 corpus); any custom-namespaced
attribute at all (XHTML 1.1 is closed, unlike EPUB 3's more extensible
profile); `<a>` may never nest another `<a>`. Also ungated RSC-016
(entity well-formedness) from the EPUB-3-only gate it was previously
bundled under - a basic XML concern, not EPUB-3-specific, confirmed via a
real EPUB 2 fixture using an unknown named entity.

**A real, substantial measurement-harness regression found and fixed
mid-increment, not guessed:** the new EPUB-2 "no HTML5-only elements"
check immediately broke *nine* previously-passing `-valid` fixtures,
including the trivial `minimal.xhtml`. Root cause: `scripts/corpus.py`'s
`wrap_single_doc` unconditionally synthesizes an EPUB-3-style `_nav.xhtml`
containing a real `<nav epub:type="toc">` element, *even when wrapping as
EPUB 2* - and `<nav>` is itself one of the newly-forbidden HTML5-only
elements. EPUB 2 has no nav-document concept at all (it's satisfied by
the NCX, which the harness already handles separately), so the harness's
own synthetic wrapper was accidentally exercising a rule against itself.
Fixed by giving the EPUB 2 wrap path its own synthetic `_nav.xhtml`
variant - a plain hyperlink to the target instead of a `<nav>` element -
preserving the same "gives the harness a reason to include the resource,
but keeps it out of the spine to isolate the content-model check"
property the EPUB 3 version has, without using a forbidden element.

**NCX content checks (ncx-publication.feature, all 5 misses fixed).** New
`ncx::check_id_attributes`: every `id` in the NCX must be a valid XML
NCName (RSC-005, confirmed via a real fixture using `np:1`, invalid
*only* because of the colon) and unique document-wide (RSC-005, reported
once *per* colliding element - a real fixture sharing one id between
`navMap` and `navPoint` expects exactly 2 findings, not 1, same
"per-element not per-pair" convention already used for encryption.xml's
duplicate `Id` and the `landmarks` nav's duplicate-type-and-reference
rule). New `ncx::check_page_target_types`: `pageTarget/@type` must be one
of the three DAISY-defined values. Extended the existing (fragment-only)
`check_ncx_content_fragments` into a full `<content src>` resolution
pass: RSC-007 if the target doesn't exist in the container at all (a real
fixture references a bogus local path); RSC-010 if it exists but isn't a
real OPS/Content-Document resource (a real fixture points at a plain
image) - reusing the exact code introduced for the navigation-document
increment's `toc`-nav-link check just one increment earlier.

**A real, narrow false positive found via the corpus, not guessed:** the
new RSC-010/OPF-032 "must be a real Content Document" checks initially
only recognized `application/xhtml+xml`/`image/svg+xml`, breaking a
real, valid EPUB 2 fixture (`ops-dtbook-valid`) that legitimately uses
`application/x-dtbook+xml` - a genuine, DAISY DTBook-based EPUB 2 OPS
content type this project had never encountered before. Fixed with a new
shared `opf::is_content_document_type` helper (xhtml/svg/dtbook), used by
both the new guide-reference and NCX-content checks.

**Container-level (ocf-publication.feature, both misses fixed).** New
PKG-014 (an empty directory entry in the ZIP - one with no other entry
nested inside it). New PKG-013 (`container.xml` declaring more than one
`<rootfile>` outside of EPUB 3's Multiple Renditions feature) - required
a new `peek_opf_version` helper in `lib.rs` to check every declared
rootfile's own `version` attribute *before* deciding whether to run the
multi-rendition machinery at all, since a real EPUB 2 fixture declares
two rootfiles (both `version="2.0"`) and expects only PKG-013, none of
the EPUB-3-only Multiple-Renditions checks. Also moved the newly-added
PKG-025 ("publication resource in META-INF") from `ocf::open` (which
runs before the OPF, and thus the package version, is even known) into
`opf.rs`, gated on `is_epub3` - a real EPUB 2 fixture, "Ignore unknown
files in the META-INF directory," explicitly stays clean with the exact
shape EPUB 3 forbids.

**Honest numbers** (all five `epub2` feature files: 67/69 should-error
scenarios hit exactly, the 2 remaining explicitly deferred; overall
corpus numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 79.1% (469 hits) | **84.3% (500 hits)** |
| RSC family exact hits | 301/377 | **315/377** |
| OPF family exact hits | 85/146 | **99/146** |
| PKG family exact hits | 34/36 | **36/36 (100%)** |
| HTM family exact hits | 28/31 | **31/31 (100%)** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, the entire `epub2` corpus is **effectively done** for this
project's current scope (aside from the 2 named OEBPS-1.2-legacy
deferrals). Remaining unscoped families: `epub-dictionaries` (17+9
misses), `D-vocabularies` siblings (17), `epub-edupub` gaps (13),
`epub3/05-package-document` (11, regrew since "all 76 done"). Also
still open, named but not chased: `scripts/corpus.py` has no
`wrap_svg_file` (bare `.svg` single-document-mode scenarios in
`content-document-svg.feature` are silently skipped rather than
measured - not a product defect, since what *is* measured is 100%, but a
quick harness addition, same pattern as `wrap_opf_file`/`wrap_smil_file`/
`wrap_nav_doc`, that would likely surface a few more real misses).

## Increment: EPUB Dictionaries & Glossaries 1.0 (2026-07-03)

Implemented the whole extension spec (http://idpf.org/epub/dict/) from
scratch across both its feature files - 25/26 should-error scenarios hit
exactly, the 1 remaining named and deliberately deferred. New `src/dict.rs`
(content-document dictionary content model + Search Key Map document
parsing) plus a large new `check_dictionaries` in `src/opf.rs` (package-
level cross-referencing: dc:type detection, single- vs. collection-based
Search Key Map requirements, source/target-language declarations).

**No CLI profile concept, by deliberate choice.** epubcheck's real
behavior here is gated by a `--profile dict|default` flag its test suite
exercises directly - a `dict`-profile scenario expects a hard RSC-005 for
a missing `dc:type` even with *zero* content-based signal that the book
is a dictionary at all (a bare `.opf` single-file check, so there's no
content to detect from regardless). Building a real profile system for
one scenario wasn't worth it: instead, "is this a dictionary" is detected
either from an explicit `dc:type>dictionary` (full strict checks, matching
every other scenario in the whole spec, since all of them declare it
explicitly) or, absent that, from real content carrying an
`epub:type="dictionary"` marker (demoted to a warning, OPF-079 - matching
epubcheck's own *default*-profile behavior, which is the only mode this
project's CLI has). The one scenario this can't reach
(`dictionary-metadata-type-missing-error.opf`) is the sole miss.

**Content model** (`dict::check_content_doc`, reused unconditionally on
every content document regardless of dc:type - confirmed safe against a
"default profile, no dc:type" fixture that still has a fully correct
article/dfn structure): an `epub:type="dictionary"` element needs >=1
`<article>` child (RSC-005 otherwise); each such article ("dictionary
entry") needs >=1 `<dfn>` descendant (a second, independent RSC-005, both
confirmed firing together from one real fixture's two content documents).
`epub:type="glossary"` is a separate vocabulary term with no such model
(confirmed via a real fixture using a `<dl>`, not `<article>`/`<dfn>`, and
expecting zero findings).

**Search Key Map documents** (`dict::check_skm` + cross-referencing in
`check_dictionaries`): `<search-key-map>` needs >=1 `<search-key-group>`
child (RSC-005 otherwise); each group's `href` must resolve to a real
resource (RSC-007) that's a genuine Content Document, not e.g. a CSS file
(new **RSC-021**); the manifest item itself should have an `.xml`
extension (new **OPF-080**, warning); and the `search-key-map` manifest
*property* requires the real SKM media type specifically - extended the
existing `cover-image`-only OPF-012 check with a second property/media-
type pair rather than inventing a new code.

**Single-dictionary vs. collection-based structure**, the largest single
piece: a confirmed dictionary publication with no `<collection
role="dictionary">` at all must have exactly one manifest item whose
`properties` includes `search-key-map` (RSC-005 "must contain exactly one
..." if none), and that item's `properties` must *also* include
`dictionary` (a separate RSC-005 mentioning `"dictionary"` if a lone SKM
item lacks it - confirmed these are two independently-triggerable
conditions, not one). A multi-dictionary publication instead uses one
`<collection role="dictionary">` per dictionary: must not nest another
`<collection>` (RSC-005); every `<link href>` must resolve to a declared
manifest item (new **OPF-081** otherwise) that's either the collection's
own Search Key Map or an XHTML/SVG Content Document (new **OPF-084**
otherwise); exactly one linked resource may carry the `search-key-map`
property (new **OPF-083**/**OPF-082** for zero/more-than-one); and the
same Search Key Map must not be linked from more than one collection
(RSC-005, tracked via a `resolved-path -> owning-collection-index` map).
New **OPF-078**: at least one of a collection's own linked resources must
itself carry the `epub:type="dictionary"` content-model marker - a real,
non-obvious per-collection subtlety (see the bug below), not a single
whole-publication check.

**dictionary-type/source-language/target-language metadata**: an optional
`dictionary-type` meta must be one of `monolingual`/`bilingual`/
`multilingual` (RSC-005 otherwise); exactly one `source-language` meta is
required (RSC-005 for zero *or* more than one, distinct messages); a
declared `target-language` must also appear as a `dc:language` value
(RSC-005). These are checked either at the package level (no collections)
or, for a multi-dictionary publication, per-collection - **a collection's
own nested `<metadata>` is authoritative when present, but falls back
entirely to the package-level metadata when the collection has none at
all** (a real fixture, "a dictionary collection can be used to define
multiple dictionaries," declares source/target-language only once at the
package level despite having two dictionary collections with no nested
`<metadata>` of their own - the first version of this check treated
"collections exist" as "always use per-collection scope," which
false-positived "missing source language" on both). A genuinely odd,
corpus-confirmed quirk kept rather than smoothed over: a *missing*
target-language reports the same message text as a missing *source*
language ("must declare its source language") - apparently a real
epubcheck message-reuse bug, but that's the literal substring the
scenario checks for.

**The real per-collection OPF-078 bug, found via the corpus, not
guessed:** the first version computed OPF-078 as one whole-publication
scan ("does *any* content document anywhere have the marker"), which
happened to coincidentally score correctly on both the single-dictionary
fixture and a naive read of the multi-dictionary ones - until a real
fixture (`dictionary-multiple-no-content-error`) showed collection 1's own
content document *does* have a fully valid `epub:type="dictionary"`
structure, while only collection 2's is missing it, and still expects
exactly one OPF-078. That's only explicable as a *per-collection* check
("does this specific collection's own linked content have the marker"),
not a publication-wide one - fixed by tracking marked-doc paths in a
`HashSet` (not a bool) built during the existing content-doc loop, and
checking each collection's own linked targets against it individually.

**A harness-only false positive, not a product bug, found the same way
every prior increment's have been:** `dictionaries-package-document.
feature`'s scenarios all use epubcheck's single-Package-Document check
mode (`wrap_opf_file`), which has no real content documents at all - so
the brand-new OPF-078 unconditionally fired on every single one of them,
including the "-valid" fixtures. Added `OPF-078` to `scripts/corpus.py`'s
existing single-doc-wrap scoring-exclusion set (alongside RSC-001/007/
011/008/OPF-014/RSC-012) - the same "no real book to check against"
reasoning as every entry already there.

**Honest numbers** (25/26 should-error scenarios across both feature
files hit exactly, the 1 remaining a named, accepted gap requiring real
CLI-profile support; 8/8 should-clean fixtures stay clean; overall corpus
numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 84.3% (500 hits) | **88.5% (525 hits)** |
| RSC family exact hits | 315/377 | **330/377** |
| OPF family exact hits | 99/146 | **109/146** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, `epub-dictionaries` is **effectively done**. Remaining
unscoped families: `D-vocabularies` siblings (17 misses),
`epub-edupub` gaps (13), `epub3/05-package-document` (11, regrew).

## Increment: finish `epub3/D-vocabularies`, all three remaining feature files (2026-07-03)

Closed out the rest of the `D-vocabularies` family - `vocabularies.feature`
(12 misses), `metadata-link-vocab.feature` (4), and
`package-rendering-vocab.feature` (1) - all 49 should-error scenarios
across all three files hit exactly, 27/27 should-clean fixtures stay
clean, 0 new false positives. `vocabularies.feature` is essentially a
full "vocabulary association" mini-engine (the `prefix`/`epub:prefix`
attribute grammar, reserved-prefix rules, and undeclared-prefix usage
checking) that had never existed before beyond the single, narrow
"reserved prefix redeclared to a different URI" check from an earlier
increment.

**A load-bearing scoring discovery, made before writing any product
code:** real epubcheck's feature file labels several distinct
sub-conditions `OPF-004c`/`OPF-007a`/`OPF-007b`/`OPF-007c` - but tracing
`scripts/corpus.py`'s own `ID_RE` regex (`\b([A-Z]{2,4}-\d{2,4})[a-z]?\b`)
shows the trailing lowercase letter is matched but **not captured** by
the group, so a scenario expecting "OPF-007b" is actually scored against
plain **OPF-007** - the exact same Gherkin-sub-case-labeling convention
already established for `HTM-060a` two increments ago, not a set of real
distinct message IDs. Confirmed empirically (`ID_RE.findall(...)` on the
literal feature-file line) before writing any Rust, avoiding an entire
wasted set of `OPF_007A`/`OPF_007B`/`OPF_007C`/`OPF_004C` constants that
would never have matched.

**The `prefix` attribute grammar** (`opf::parse_prefix_value`): a
whitespace-separated list of `name:` `URI` pairs, parsed leniently to
tolerate two real syntax-error shapes a corpus fixture exercises in one
value (`"foaf http://... dbp : http://..."` - `foaf` has no colon at
all, `dbp` has its colon separated by whitespace) - each increments an
**OPF-004** count (2, matching the fixture) while still best-effort
recording the pair, so a downstream "is this prefix declared" check
doesn't cascade into an unrelated second finding for a name that's
present, just malformed.

**`check_prefix_declaration`** (replaces the old, narrow
`check_reserved_prefixes`, returns the declared name→URI map for the
caller's own usage-checking): four conditions, all sharing the single
**OPF-007** message ID per the scoring discovery above - the reserved
prefix `_` must never be declared; a prefix must not be mapped to one of
the **4 default-vocabulary URIs** (`.../package/{meta,link,item,itemref}/#`
- the rule is about the *URI* side, confirmed since the one real fixture
happens to also reuse those same 4 words as prefix *names*, but the spec
text is explicit it's the URL being reused that's disallowed); a prefix
must not be mapped to the Dublin Core elements namespace; and the
pre-existing "reserved prefix redeclared to a different URI" warning.

**`check_prefix_usage`** (OPF-028): for every `prefix:term` token found
in a `property`/`properties`/`epub:type` attribute value, the prefix must
be either one of the 10 fixed reserved prefixes (usable without
declaring) or present in the document's own declared map - checked
uniformly across package metadata/manifest/spine (`property`/
`properties`), XHTML and standalone-SVG content documents (`epub:type`),
and - genuinely new territory - **Media Overlays (SMIL) documents**,
which had no prefix-declaration handling of any kind before this
increment (`epub:type` custom-prefix tokens were previously waved through
by an unconditional "contains ':' is exempt" leniency, never actually
checking whether the prefix was declared).

**`check_prefix_placement`** (RSC-005): a `prefix`/`epub:prefix`
attribute is only valid on the document's own root element - confirmed
via two real fixtures (an XHTML `<head>`, and an embedded `<svg>` inside
an XHTML body) that both expect the exact same message,
`attribute "epub:prefix" not allowed here`. A **SMIL-specific
companion finding**, also new: a *bare* (non-namespaced) `prefix`
attribute on the `<smil>` root is a structural violation there
(`RSC-005`, "attribute \"prefix\" not allowed here" - note the different
message text, since it's the un-namespaced attribute name, not
`epub:prefix`) - SMIL has no permissive RelaxNG-style attribute
catch-all the way XHTML does, so this needed a small hand-coded check
rather than a schema change.

**metadata-link-vocab.feature (4 misses), extending the existing
metadata-`<link>` cross-referencing loop:** a `rel="alternate"` link must
not be combined with any other `rel` keyword (new **OPF-089**); `rel`
containing `record` or `voicing` must declare a `media-type` **even when
the link is remote** (new **OPF-094**) - a stricter carve-out from the
existing OPF-093 check, which deliberately exempts remote links in
general; and a `voicing` link's declared media-type must actually be an
audio type (new **OPF-095**).

**package-rendering-vocab.feature + meta-properties-vocab.feature (2
misses), both simple `meta[@property]` name checks:** a `rendition:`-
prefixed property outside the 5 known ones (`layout`/`orientation`/
`spread`/`flow`/`viewport`) is an unknown property (reused the existing
**OPF-027**, "unknown property," rather than inventing a new code for
what's the same underlying concept as the manifest-item-properties
version); the deprecated `meta-auth` property is a plain presence check
(**RSC-017** warning).

**Honest numbers** (all 49 should-error scenarios across `vocabularies`/
`metadata-link-vocab`/`package-rendering-vocab` hit exactly; 27/27
should-clean fixtures stay clean; the 1 remaining "skip" -
`prefix-attribute-valid.svg` - is the already-known, harness-only
`wrap_svg_file` gap noted in an earlier increment, not a real miss):

| metric | before | after |
|---|---|---|
| exact-ID recall | 88.5% (525 hits) | **91.7% (544 hits)** |
| RSC family exact hits | 330/377 | **334/377** |
| OPF family exact hits | 109/146 | **125/146** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, `epub3/D-vocabularies` is **fully done** (aside from the named
`wrap_svg_file` harness gap, orthogonal to this family). Remaining
unscoped families: `epub-edupub` gaps (13), `epub3/05-package-document`
(10, regrew), `epub-indexes` (12), `epub-previews` (7), a handful of
small niche extension profiles (~6 combined).

## Increment: finish `epub3/05-package-document.feature`'s regrowth (2026-07-03)

Closed out all 10 scenarios that had regrown since the earlier "all 76
done" increment - all 94 should-error scenarios in the file hit exactly,
27/27 should-clean fixtures stay clean, 0 new false positives. The
recurring theme, once again: **the exact same underlying condition often
has a different message ID/severity in EPUB2 vs. EPUB3** - three of the
ten misses were this shape, not "check doesn't exist."

**dc:date and dcterms:modified (3 misses).** A malformed/empty `dc:date`
is **OPF-053/Warning** in EPUB3 ("does not follow recommended syntax")
but **OPF-054/Error** in EPUB2 (confirmed via the EPUB2 corpus's own
dedicated fixtures) - the existing `is_valid_dc_date` check (built during
the epub2 increment) was firing OPF-054 unconditionally; gated it on
`is_epub3`. New: `dcterms:modified` must be exactly `CCYY-MM-DDThh:mm:ssZ`
(a plain fixed-width byte-shape check - `is_valid_dcterms_modified` -
not the XPath-engine date regex this was originally deferred as needing;
the exact format was simple enough to hand-code once actually attempted).

**Duplicate spine itemref (1 miss), the same version-split shape found
again:** a spine referencing the same manifest item id twice is
**RSC-005** in EPUB3 but **OPF-034** in EPUB2 - confirmed via two
corpus fixtures using the *exact same* XML shape (content_001 twice,
content_002 in between) that differ only in `version="2.0"` vs `"3.0"`.
The pre-existing OPF-034 check (one of the very first checks this project
ever wrote) had never been version-gated; fixed, and `scripts/spike.py`'s
own `duplicate_spine.epub` regression fixture (a default-EPUB3 synthetic
book) updated to expect RSC-005 instead - a real, if minor, "our own
synthetic fixture predates a correctness fix" gap, same precedent as
several earlier increments.

**Manifest item vocabulary tightening (2 misses).** `link[@properties]`'s
only real vocabulary term is `"onix"` (confirmed via a real fixture
pairing it with an exempt custom-prefixed token) - anything else
unprefixed is `OPF-027`. More subtly: a manifest `<item>`'s `properties`
token with a *reserved* prefix (e.g. `rendition:layout-pre-paginated`,
which is only ever a valid `<itemref>` override, never an `<item>`
property) has no valid item-level meaning at all - the existing "unknown
property" check only ever rejected *unprefixed* unknown tokens, exempting
*any* colon-containing one as if it were always a legitimate custom
vocabulary term. Fixed by splitting the check: an unprefixed token still
needs the `KNOWN_ITEM_PROPERTIES` allowlist; a prefixed token is only
exempt when its prefix is genuinely custom (not one of the 10 reserved
prefixes) - a real, if narrow, false-negative this project had been
carrying since the original `KNOWN_ITEM_PROPERTIES` design.

**The single nav item + nav-must-be-XHTML rules (2 misses), both new,
straightforward manifest-scan additions:** more than one manifest item
declaring `properties="nav"` is `RSC-005` (added a `nav_count` alongside
the pre-existing `nav_present` bool); a `nav`-declared item whose
media-type isn't `application/xhtml+xml` fires *both* `OPF-012`
("property not defined for media type," extending the existing
cover-image/search-key-map pattern) *and* a separate `RSC-005` - both,
not either/or, confirmed via the one real fixture testing this shape.

**`<bindings>` deprecation (1 miss):** a plain presence check - any
`<bindings>` element anywhere in the package document is `RSC-017`
(warning), regardless of its own content (already schema-permissive via
`package.rng`'s `looseContent`).

**Remote font embedded in a standalone SVG content document (1 miss) -
narrower than the increment that originally deferred it.** The original
"deferred" note assumed this needed tracing *into* an SVG **embedded
inline inside XHTML markup** (no parser for that). The real fixture is
simpler: the SVG is its own **standalone manifest item** (`image/
svg+xml`), referenced from XHTML only via an ordinary `<img src=
"cover.svg">` - meaning it's already walked by the existing standalone-
SVG loop (`svg_doc_paths`, built for the CSS-029/030 cross-reference).
Added a check there: an `<font-face-uri xlink:href="...">` pointing to a
remote URL means the SVG document "uses a remote resource" just like an
XHTML doc would, and needs its own manifest item to declare
`remote-resources` (OPF-014, reusing the exact message shape the XHTML
check already uses). The genuinely-inline-embedded-SVG case (no separate
manifest item at all) remains out of scope, unchanged.

**Honest numbers** (all 94 should-error scenarios in
`epub3/05-package-document.feature` hit exactly; 27/27 should-clean
fixtures stay clean; overall corpus numbers below):

| metric | before | after |
|---|---|---|
| exact-ID recall | 91.7% (544 hits) | **93.4% (554 hits)** |
| RSC family exact hits | 334/377 | **339/377** |
| OPF family exact hits | 125/146 | **131/146** |
| false positives | 4 | 4 (same accepted set, unrelated) |

With this, `epub3/05-package-document.feature` is **fully done** again.
Remaining unscoped families: `epub-edupub` gaps (13), `epub-indexes`
(12), `epub-previews` (7), `content-document-svg.feature`'s ~12 misses
(likely mostly the `wrap_svg_file` harness gap, unconfirmed), a handful
of small niche extension profiles (~6 combined).

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
