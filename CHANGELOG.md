# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

## [0.13.1] - 2026-08-28

**Six false positives, and none of them was reaching anyone.** Every one is a
parity fix against epubcheck's own fixtures: measured across the 415-book local
shelf, not one of the six changes a real book. What the day actually adds to a
reader's output is three *true* findings on two books, each confirmed against
epubcheck at the same file, line and wording.

**The content-document set is the version's set, and ours was version-blind**
(issue #129). OPS 2.0.1's content documents are XHTML and DTBook; EPUB 3's are
XHTML and SVG — so the two differ **in both directions**, and a predicate that
ignores the version is wrong twice. Four sites consulted it:

- a **hyperlink** to a manifest-declared SVG that is not in the spine drew
  RSC-011 from us and RSC-010 from epubcheck at 2.0 — a wrong id on entirely
  ordinary markup;
- a **`<guide>` reference** to an SVG is OPF-032 at 2.0 and we were silent,
  the EPUB 3 mirror (a guide reference to DTBook) likewise;
- the **NCX `<content src>`** needed *both* of epubcheck's questions rather
  than one. It now shares `hyperlink_abort` instead of restating its
  conditions, which is how it had come to hold the first arm and not the
  second: an NCX pointing at a declared XHTML document that is simply **not in
  the spine** drew nothing at all from us;
- the **fallback chain**, the hyperlink's escape hatch from RSC-010, was the
  last and the sharpest. `FallbackChainResolver` calls
  `isBlessedItemType(type, version)`, so a PDF that falls back to an SVG is
  rescued at 3.0 and not at 2.0, DTBook being the mirror. Half of those four
  cells were a wrong id, and the comment at that site had claimed the resolver
  applies "no version condition" — it never did.

The other three sites are **not** version-dependent, read at the source rather
than probed: `ResourceReferencesChecker`:179 gates fragment resolution on
`MIMEType.SVG.is(..) || MIMEType.XHTML.is(..)` with no version in the call. One
dead line went with them — the `<guide>` site carried the same content-document
guard twice in a row, the second unreachable.

**A `data:` URL in a hyperlink is not finished at RSC-029** (issue #128).
epubcheck does not stop there: the reference reaches the ordinary hyperlink
checks and is asked the same two questions of the media type the data URL
declares for itself. Fourteen books later — one per type per version — the
either/or is gone: at 3.0 we reported RSC-029 alone where epubcheck reports it
*and* RSC-010 or RSC-011; at 2.0 we reported RSC-010 for every type, right for
an image or a PDF and wrong for `text/html`, XHTML and DTBook. The comparison
is case-sensitive, so `data:TEXT/HTML,x` and `data:text/html,x` differ.

**OPF-030 was asked only inside the `<metadata>` block, and OPF-048 could not
be reached at all.** Resolving `unique-identifier` lived under
`if let Some(md) = metadata`, so a package with no `<metadata>` element drew no
OPF-030 — and one with neither the element nor the attribute drew **nothing**:
not the RSC-005, not OPF-048, not OPF-030. epubcheck reports all three. No
fixture anywhere builds that book, which is why the regression test does.

**A `<package>` in a foreign namespace now stops after the schema error.** A
namespace that is neither the OPF one nor a legacy one means the document is
not a package document, and epubcheck's grammar says so by rejecting the root —
after which it never builds the package model, asks nothing about the manifest,
and opens no content document. We kept going: two RSC-001 on its own fixture,
and on a book carrying a genuinely broken content document, two more RSC-005
from validating a document epubcheck never opens. What is left behind is a
strict subset of epubcheck's findings, and the stop is guarded on the schema
violation having actually been reported rather than on the namespace test
alone.

**OPF-097's three exemptions were all wider or narrower than epubcheck's.**
`isNcx()` is set in exactly one place — on the item the spine's `toc` attribute
resolves to — so an NCX-typed item that no `toc` names is not exempt, and our
media-type test excused it. `isInSpine()` is a property of the *item*, not of
the resource: two manifest items may declare the same href, and then only the
one the spine names is exempt. And a `data:` href was not asked the question at
all. A data URL is named by its first 30 characters plus an ellipsis, as
epubcheck names it; the first version put three kilobytes of base64 into a
usage message.

**A remote `<base>` restricts the stylesheet and not the hyperlink.** RSC-006
says a remote reference is not allowed *in this context*, and a hyperlink is
not one of those contexts — you may link to a website. We had it inverted:
RSC-006 on every relative `<a href>` in such a document, and silence on the
`<link rel="stylesheet">` beside it. The corpus could not see this, because
both of its base fixtures expect one RSC-006 and got one, from the wrong
element. The restricted-remote classification is now one predicate shared by
the two sites that ask it, which is how they had drifted: only a reference
written remote was asked, never one that resolves remote through a base.

### SVG in EPUB 2 is validated normatively (issue #93)

`schema/20/rng/content.rng` includes the SVG 1.1 modules directly, so inline
and standalone SVG in an EPUB 2 publication is validated **normatively** and
reported as ERROR RSC-005, where EPUB 3 runs the same grammar informatively as
RSC-025 usage. Three checks were gated to EPUB 3 on a comment that is correct
about RSC-025 and wrong about the validation underneath it — the gate was not
avoiding an opinion epubcheck lacks, it was suppressing a finding epubcheck
makes. Its own example proves it: a lowercase `viewbox` in a real book had been
written off there as our false positive, and epubcheck reports it.

Both lists were diffed against the authority before either arm was switched on,
and nothing was missing: **0 of the 81 element names** and **0 of the 256
unprefixed attribute names** `schema/20/rng/svg/*.rng` declares are absent from
ours. What ours carry beyond SVG 1.1 is one element (`feDropShadow`) and four
attributes (`focusable`, `href`, `rel`, `tabindex`), each probed in both
versions: all are RSC-005 at 2.0 and clean at 3.0.

The **content model** now covers its closed half, in four shapes measured cell
by cell — 33 books, one per cell:

- the graphics elements (`rect`, `circle`, `image`, `use`, `path`, `tref`, …)
  hold descriptive and animation elements and nothing else, not even text;
- the text elements (`text`, `tspan`, `textPath`) are a mixed pool that also
  admits `a`, `altGlyph`, `tref` and `tspan` — and `textPath` only directly
  inside `<text>`;
- the gradients add `<stop>`;
- `<stop>` itself takes **animation elements only**, not even a `<desc>`.

Indentation whitespace is not loose text, which is the one cell of the 33 that
could have cost a real book. The container elements — `g`, `defs`, `svg`, `a`,
`switch`, `marker` and the rest — are deliberately outside this slice: their
models are open-ended pools, and that is where a from-scratch grammar starts
reporting what epubcheck does not.

Measured cost before and after: of the shelf's 261 EPUB 2 books carrying inline
SVG, the whole population uses **two** element names and **nine** unprefixed
attribute names. Three of the nine are outside SVG 1.1 — one occurrence each,
two books — and all three are errors epubcheck reports at the same line.

### Positions are unchanged, and that is a decision

An empty `<guide>` draws the same finding from both tools at different
coordinates: epubcheck at the character after `</guide>`, epubveri at the
`<guide>` itself. Both are internally consistent. epubcheck reports where its
reader had reached when the fault surfaced, which gives it three different
anchors for three kinds of fault — just past the start tag, just past the end
tag, and just past the start tag's `>` for an attribute, where it points at
neither the element nor the attribute. We report where the fault begins,
because that is where an author has to go and where a repair tool has to start.
`range().end` reproduces epubcheck's number exactly if this is ever revisited;
the reasoning now lives on `Position::of`.

## [0.13.0] - 2026-08-28

**Breaking, library only:** `rng::ElementFault::MissingAttribute` is now
`MissingAttribute(Vec<String>)` - it carries the names of the attributes the
element is missing. A consumer matching on that variant needs a binding; the
CLI, the JSON output and the WASM bindings are unaffected. `params[0]` is
unchanged for every kind, and the names arrive after it.

**Twenty-six false positives.** Twenty-three are one shape — a defect reported twice,
where each message was true but the count did not match epubcheck's, which to
anyone diffing the two tools is indistinguishable from an invented error. Three
are invented errors outright: `<iframe srcdoc>` is valid and we rejected it, so
is a comma in a hostname, and so is a media-overlay document whose stylesheet
does not happen to define the active-class.

All twenty-six were found the same way, by running epubcheck over the corpus's
own dumped books and diffing the **counts** rather than the id sets — the
pairing that has now produced thirty-five findings in two days - two of them false negatives rather than false positives, both surfaced by the same line-by-line diff. The id-set diff
is blind to every one of them, and so is the corpus: the extra finding carries
the same id at the same severity, so its "no other errors" comparison passes.

**The meta-properties vocabulary reported a refinement duplicate twice**, and
in two different ways, because epubcheck checks the family with two different
idioms and our Schematron used neither. Its per-property rules
(`title-type`, `display-seq`, `file-as`, `group-position`, `identifier-type`,
`source-of`, `collection-type`) report `preceding-sibling::` — so N duplicates
draw N-1 findings and the first occurrence is not blamed; ours asserted
`count(...) = 1` from each `meta` and drew N. The `authority`/`term` pair is
not in that family at all: epubcheck checks it once per `dc:subject`, so any
number of duplicates is one finding. Both counts were measured against
epubcheck on a three-duplicate book rather than read off its schema.

The same block also carried a **second copy of all seven "must refine X"
rules**, with a looser target lookup, so every one of those violations was
reported twice. The duplicates were removed and the originals kept — and the
looser lookup turned out to be the reason the pairing rule over-reported too:
matching a `@refines` with no leading `#` grouped two properties epubcheck
counts as unrelated, inventing a cardinality finding on top.

**A duplicate manifest item `id` drew three findings where epubcheck draws
two.** The manifest loop had its own duplicate-`id` check on top of the package
document's id-uniqueness rule. Removing it was verified in both directions
first: the general rule normalises whitespace, and it is not EPUB 3 only.

**An item that falls back to itself drew two OPF-045.** The cycle detector's
own comment said the self-fallback case was handled elsewhere and it fell
through anyway. One-item cycles are now left to the check that names the item
and carries its position.

**A non-ASCII directory name drew two PKG-012** — one for the file inside it,
one for the directory entry. epubcheck runs its filename checks only on
non-directory entries; a directory *name* is still checked, as a segment of the
paths of the files inside it.

**MED_013 was asked of an item that is not a content document.** A
`media-overlay` attribute on an image already draws RSC-005; epubcheck reaches
MED_013 only through its content-document checkers and never asks the second
question.

**Three more one-defect-two-findings pairs, all of them a hand-written check
saying what a schema had already said.** A missing `<spine>` drew the content
model's `element "package" has incomplete content; missing required element
"spine"` and a hand-coded `OPF is missing the <spine> element`; an EPUB 2
`<spine>` with no `toc` drew both a Schematron rule and a hand check; the Adobe
`page-map` attribute drew the EPUB 2 grammar's rejection and a hand check.

**None of the three could be closed by simply deleting the extra check, and
that is the part worth recording.** Each hand check was load-bearing somewhere
the schema is silent, and only measuring both versions showed where:

- the `toc` Schematron rule needs an NCX item to be present, while epubcheck's
  EPUB 2 requirement (`opf20.rng`) does not — so a 2.0 book with no NCX at all
  is reported by epubcheck and by the hand check, and by nothing else. The rule
  is now scoped to 3.x, matching epubcheck's, which lives in `package-30.sch`;
- the EPUB 3 grammar does not reject `page-map`, so there the hand check is the
  only reporter. It now asks the report whether the content model already named
  the attribute instead of guessing from the version — a structural query on
  `violation_kind` and `params[0]`, not a text match, so it cannot drift if
  either grammar changes;
- only the missing `<spine>` was a true duplicate, and both grammars were
  checked before the check came out.

The `page-map` pair is the first of this whole run to move a real book: one
title on the local shelf uses the extension and loses one of its 803 findings.
Everything else here is visible only against epubcheck's own fixtures.

**`<iframe srcdoc>` was rejected on valid markup, and five more attributes
with it.** This one is not a count difference: epubcheck rejects `seamless` on
`obsolete-seamless-error.xhtml` and lists `srcdoc` among the attributes it
*would* have accepted, so we were inventing an error. HTML5's whole
`iframe.attrs` set is now taken from epubcheck's `mod/html5/embed.rnc` —
`srcdoc`, `loading`, `sandbox`, `allowfullscreen`, `allow`, `referrerpolicy` —
because a fixture only ever names the one attribute it happens to carry.
EPUB 2 is untouched: two of the six are `v5only` in epubcheck's own schema.
Attribute *values* are left unconstrained, the call already made for MathML
and for `role`/`aria-*` — taking the names closes the gap in the safe
direction, taking the value grammars would open a restrictive one.

**Three more hand-written checks that duplicated a schema**: a missing
`<metadata>`, a manifest `<item>` with a missing `id`/`href`/`media-type`, and
a foreign-namespaced attribute in an EPUB 2 content document. Each was probed
in both versions before its check came out — and the manifest-item one is a
small illustration of why naming the missing attribute was worth doing, since
the content model's message (`element "item" is missing the required attribute
"media-type"`) now says strictly more than the hand-written one it replaces.
That one also used to print a Rust `Option` through `{:?}` into user-facing
text and into `params[0]`: `id=Some("img002")`.

**The host character set was an allowlist and is now epubcheck's denylist —
seventeen printable ASCII characters stopped being errors.** `,`, `!`, `$`,
`&`, `'`, `(`, `)`, `*`, `+`, `;`, `=`, `~`, `^`, `|`, `{`, `}` and a backtick
in a hostname were all RSC-020 here and are all clean in epubcheck, measured
one URL per character against 5.3.0. The comma is the one its own corpus
names: `url-host-unparseable-warning.xhtml` carries `https://w,w.example.com`
with the comment "Host contains an invalid character (see issue #1034)" —
epubcheck records that it *should* flag this, does not, and #1034 is still
open. Reporting it anyway is a restrictive divergence, indistinguishable to
anyone diffing the two tools from an error we invented.

What it does reject is now the whole rule: a backslash, and — after the
percent-decode — `@` and a space. Two gaps stay gaps on purpose, both false
negatives: a non-numeric port and an unmatched bracket, which would mean
modelling galimatias's port and IPv6 parsing, the inference this module
already refuses to make.

**A missing file named by both the manifest and a CSS `@import` was two
findings.** epubcheck's RSC-001 is per publication *resource*, not per
reference, so it reports once; the `@import` walk now leaves a declared target
to the manifest and keeps its other two arms, which are about the reference.

**The nav label rules were not scoped to the list.** epubcheck's contexts are
`html:nav[@epub:type]//html:ol//html:a` and `…//html:ol//html:span`; ours
walked every descendant of the nav, so an empty `<span>` in the nav's heading
drew the span rule on top of the heading rule that already covers it.

**Two of the failing tests were the point, not an obstacle.** The comma
assertion and the CSS `@import` assertion both encoded our own stance rather
than epubcheck's, and both had been passing for as long as the divergence
existed. Checking an assertion against the oracle before treating it as a
constraint is the rule that applies; both now assert the measured behaviour,
with the characters epubcheck *does* reject asserted beside them so the new
rule cannot drift into accepting everything.

**CSS-030 was asking the wrong question, once per property.** epubcheck's
condition is `!hasCSS` — the overlaid content document has no CSS at all — and
it says so once. Ours asked whether each declared class had a *matching
selector*, once per declared property, which was wrong in two ways measured
one book per arrangement: a document whose stylesheet simply does not define
the class is valid and we reported it (a false positive), and a document with
no CSS at all drew two findings when both `media:active-class` and
`media:playback-active-class` were declared.

Fixing it needed a signal `doc_class_names` could not give: an empty entry
means "found nothing" for an SVG and "no stylesheet" for an XHTML document.
The three SVG style sources — an `xml-stylesheet` PI, a `<style>` element and
an XHTML-namespaced `<link rel="stylesheet">` — are now enumerated in one
place and report whether any was found. **The first attempt duplicated that
detection at the call site, missed the `<link>` form, and turned a valid
fixture red**; the family run is what caught it, one build later.

**Nested `<dfn>` had a hand-coded check beside its Schematron rule**, blaming
the outer element for containing one while `xhtml.sch`'s `no-dfn-in-dfn`
blamed the inner one — as epubcheck's single `descendant-dfn-dfn` pattern
does. EPUB 2 stays silent either way, probed both ways.

**A hyperlink to a remote image is no longer reported.** `<a href>` pointing
at a manifest item declared `image/*` drew RSC-006 from a content-document
walk, on the reasoning that an image should be embedded rather than linked.
epubcheck reports nothing of the kind: its fixture expects one RSC-006 and
gets it from the manifest side, and in the arrangement where the manifest walk
goes quiet — the image both hyperlinked and embedded — epubcheck still reports
one, for the `<img>`. Both arrangements were probed before the walk came out.

**An unclosed CSS block was reported twice, and the framing of it was the
real defect.** This row had been written off as a granularity difference in
styloria's error recovery. It was not: on `content-css-syntax-error` styloria
reports exactly two errors, one per unclosed `{`, which is exactly what
epubcheck reports. The third finding was ours — a
`css.declaration.malformed_shape` raised on the rule that the first unclosed
block had swallowed, since an unclosed `{` absorbs the next selector and
braces and what is left does not parse as a declaration list. Declaration
shapes are no longer second-guessed inside a block the parser has already
called unterminated.

**Calling it styloria's had sent the fix to the wrong repository.** Parity
with epubcheck is a consumer's question — a CSS crate's job is to parse CSS
well, not to match another tool's finding count — and this one belonged here
the whole time. Worth remembering the next time a row looks like it belongs to
a dependency: check what the dependency actually reported before deciding.

Positions still differ and are left alone: epubcheck points at the token that
got confused, or at end of file; we point at the `{` that was never closed,
which is the one an author has to fix.

**`schematron-error.xhtml` now agrees exactly, 43 findings against 43** - the
largest remaining count gap, and it turned out to be three separate things
whose totals had been cancelling each other out. Diffing the two reports
*line by line* rather than by count is what separated them; by count it looked
like a single +2 discrepancy.

- **An IDREFS attribute is one finding, however many of its tokens fail.**
  epubcheck's `idrefs-any` pattern asserts `every $idref in tokenize(...)
  satisfies ...` from a rule contexted on the element, so `headers="th1 th2"`
  naming two absent cells is one finding there and was two here. Seven
  attributes go through that shape - `aria-describedby`, `aria-labelledby`,
  `aria-controls`, `aria-flowto`, `aria-owns`, `output/@for` and `@headers` -
  and the fixture exercises only the last, so all seven were changed. Every
  failing token is still named, and they all reach `params`: matching
  epubcheck's count is parity, matching its vagueness would not be.
- **A stray `<area>` drew two errors.** HTML5 puts `area` in phrasing content
  and leaves "must be inside a `map`" to prose, which epubcheck enforces with
  a Schematron rule alone; our grammar restricted it to `map` as well, so the
  content model and the Schematron rule both spoke. `area` now sits in
  phrasing content in EPUB 3, matching `common.elem.phrasing |= area.elem`.
  EPUB 2 is unchanged - XHTML 1.1 declares `area` only inside `map` and has no
  Schematron rule to fall back on.
- **A duplicate `id` in a content document was reported once, not once per
  occurrence** - a false *negative*, and the only change in this whole run
  that moved real books: seven shelf titles gained findings, and on the two
  checked against epubcheck the duplicate counts then matched exactly. It also
  disagreed with our own package-document rule, which had always reported
  every occurrence. **Consumers keying on
  `opf.content_document.duplicate_id` will see more findings of it than
  before**, all at positions that were previously silent.
- **`<bdo>` without `dir` is now reported**, epubcheck's `bdo-dir`. A real gap,
  and it is here rather than in some later release only because the
  line-by-line diff surfaced it: two of its findings were missing and one of
  ours was extra, which a count comparison reads as "we are one over".

**The RSC-001 trio stays open and is documented rather than chased.** When two
manifest items share an `id`, epubcheck's manifest is keyed by that id, so the
second entry hides the first and only the second item's missing file is
reported; we check both and report both. Matching would mean reproducing an
implementation artefact and dropping a finding that is true. Both books are
invalid in both tools either way, and no book on the 415-title shelf has a
duplicate manifest `id` at all.

**Three rows are deliberately left open.** The SVG `<title>` pair is issue #94 —
epubcheck's behaviour there is Jing's error-recovery internals, which are not
readable from this checkout, and the real-book population is zero; the corpus
fixtures are a better witness than the hand-written probes the issue records,
and are noted there. The `epub:switch` pair is epubcheck reporting one message
where we report two true ones (`element "epub:default" not allowed yet; missing
required element "epub:case"`); merging them is engine surgery on a blame path
that produces 8,992 findings on the local shelf, against a population of zero
for this shape — the same trade #94 declines. The Content MathML row is
mostly #94 as well: our own MathML walk already reports one finding per
invalid subtree, exactly as epubcheck does, and the extra nine come from the
grammar descending into a subtree it has just rejected. The shelf carries
**zero** MathML findings, so this is fixture-only in both directions.

**A missing required attribute is now named** (JSWolf, MobileRead #256).
`element "meta" is missing a required attribute` left the author to work out
which one, on an element whose entire attribute set is written on the line in
front of them; it now reads `element "meta" is missing required attributes
"content" and "name"`, as epubcheck's does. The commonest shape by far is an
`<img>` with no `alt` — **3,434 of the 3,434 occurrences on the local
405-book shelf** — which used to say nothing at all about `alt`.

The names also reach `params`, after the element name, so a consumer can act on
the finding without parsing the message text. `params[0]` is unchanged and
still the element's local name; this is an additive tail, the same shape the
`element_not_allowed` and `incomplete_content` kinds already used. The
`missing_attribute` kind was the odd one out only because the engine never
computed the names.

Neither the corpus nor the shelf can see this class: the extra finding carries
the same id at the same severity, so the corpus's "no other errors" comparison
passes, and no book on the shelf refines metadata, declares a media overlay, or
carries a non-ASCII directory. Both are unchanged by all five fixes; the new
unit tests are the protection.

## [0.12.5] - 2026-08-27

**The published corpus recall figure goes 595 → 603, and this time the
denominator grew** (#111). Eight EDUPUB scenarios that 0.12.4 had to skip are
asked and answered again: the test harness was wrapping a loose fixture into a
book while leaving the document *outside* the reading order, and some of
epubcheck's rules do not apply there — so those fixtures could not pose their
question, and 0.12.4's notes said so. That was the harness's doing rather than
the suite's, and the wrap now puts the target in the spine. Verified from
epubcheck's side too: handed the rebuilt books it now reports the finding the
feature file expects, where before it reported nothing.

Four scenarios remain unaskable, all one cause — they are written for
epubcheck's loose-file mode and are about a badly-named file, which a wrap's
container does not contain. The count and the reason print on every run, and
the README states the suite's 607 beside the scored 603.

The same work removed a large source of noise in the comparison harness: a
wrap declared every sibling fixture in the directory as a manifest item,
including other package documents, which epubcheck then validated in full. On
the 981 built scenarios, agreement with epubcheck went from 831 books to 872.

**`dc:type=teacher-edition` alone no longer turns on the EDUPUB rules**
(#119). epubcheck's publication types are `epub`, `dictionary`, `edupub`,
`index` and `preview` — there is no `teacher-edition` — so a book declaring
only that gets no EDUPUB checks unless the profile is asked for. We reported
`RSC-005` on one; with `--profile edupub` both tools report it and always did.

The comment justifying the wider detection had expired twice: it named
CLI-profile support as a gap this project does not build, and `--profile
edupub` has existed for some time — and the premise was wrong anyway.

**Five places where one defect drew two findings** (#117). Each is a second
check still asking a question epubcheck had already stopped asking, so the
extra message named a repair that would not have helped:

- a package `<link>` to a **declared** manifest item no longer adds `RSC-007w`
  beside the `RSC-001` for the missing file;
- a malformed `property` value (`"foo:"`) no longer adds `OPF-028` beside
  `OPF-026` — epubcheck reports the malformed value and never looks the prefix
  up;
- an empty or whitespace-only `property` no longer draws `OPF-027`, since
  there is no token to look up and the grammar already reports the value;
- `OPF-033` needs a spine that *has* itemrefs — it means "none of them is
  linear", not "the spine is empty", which the grammar reports;
- a `data:` URL's wrapped base64 no longer draws `RSC-020` for the newlines in
  its payload.

Two of these had their reasoning written down and wrong. The `OPF-033` comment
argued that an empty spine has no linear resources — true, and not what the
message means.

**The OCF filename rules are asked of the container, not of a manifest href**
(#116). epubcheck says so in its own source — *"only check the filename in
single-file mode (it is checked by the container checker in full-publication
mode)"* — and epubveri takes a packaged `.epub` only, so the href form could
never be right here. It fired exactly where epubcheck is silent: on a file the
manifest declares and the container does not hold, and on a `%20` href whose
entry is literally named that, where the space exists only after decoding.

`PKG-010` had to be **added** to the container loop first: it carried the
other three filename rules and not that one, so removing the href form alone
would have lost the rule on real books. 22 of the 405 shelf books report it,
241 findings, and the per-book counts are unchanged.

Four more corpus scenarios turn out to be written for single-file mode and
unaskable once wrapped — handed the harness's book epubcheck reports
`RSC-001` and nothing else on each. The scored denominator goes 599 → 595,
the count and reason print on every run, and the README now states the whole
suite's size beside it rather than only the shrinking number.

**`data-*` is allowed on SVG, and a malformed one is now named properly**
(#115). `RSC-025` fired on `data-epub` in a standalone SVG, including on
epubcheck's own fixture whose title says the family is allowed. The attribute
vocabulary could never have carried the rule — `data-*` is an open-ended
family, not a vocabulary entry.

Accepting the shape alone would have traded a wrong finding for silence, so
the suffix is judged here too: `data-` and `data-FOO` draw `HTM_061`, exactly
as they do on the XHTML side. That check runs over documents declared
`application/xhtml+xml`, so inline SVG was always covered and a bare `.svg`
file never was.

**A hyperlink epubcheck aborts is asked nothing about its fragment** (#114).
`RSC-012` was reported alongside `RSC-011`, where epubcheck reports the second
alone — `case HYPERLINK` throws after either of its two findings, so the
fragment is never looked at. Two errors for one defect, and the second names a
repair that would not help: adding the missing id does not put the document in
the spine. Same family as #106.

Probing that turned up a second defect in the opposite direction. Our
`RSC-011` was gated on the target being XHTML or SVG; epubcheck has no such
gate, and the gate only looked right because a hyperlink to an image is
aborted by `RSC-010` first. For `text/html` — deprecated-blessed, so `RSC-010`
does not fire — epubcheck reports `RSC-011` and we reported `RSC-014` about
the fragment and no `RSC-011` at all. One wrong gate, a false positive and a
false negative together.

Both conditions now live in one predicate that the reporting loop and the
fragment check share, since writing them twice is how they would drift — and
this pair had drifted once already.

**Only a stylesheet `<link>` is a reference** (#113). `rel="prev"`,
`rel="next"` and an RDFa or microdata `<link property href>` with no `rel`
drew `RSC-007` when their target was missing; epubcheck registers a
content-document `<link>` as a reference only when `rel` names a stylesheet,
in both versions, and reports nothing on its own `rdfa-valid` fixture, which
carries all three shapes.

The exemption sits after the URL handling rather than before it, because
epubcheck parses the URL before it reads `rel`: a malformed URL is still
reported on a link of any kind, and only the existence question goes unasked.

**`<input type="image">` needs a non-empty `alt`** (#109), which neither half
of was checked. Filed yesterday and left open because a
conditional-on-attribute-value requirement is the kind that invents false
positives when guessed; the boundary was probed instead, one book per shape,
and it turns out to be small — the requirement turns on `type="image"` alone
and `src` is not required.

**A required attribute with a bad value is blamed once, not twice** (#123).
The RELAX NG engine reverts a rejected attribute so the rest of the element
still gets checked, and the close of the start tag then found the required
slot empty and reported it missing as well. The element has the attribute;
only its value is wrong. An `<epub:trigger action="zzz">` drew two findings
against epubcheck's one.

A `file:` URL in a stylesheet now counts as a remote resource for both
`remote-resources` questions, as it does in epubcheck — measured in six
combinations, since `data:` and an empty `url('')` look like the same case and
are not.

**`<epub:trigger>` keeps its own attributes** (#124). Our grammar gave it the
global attribute set, which holds neither `action` nor `ref`, so epubcheck's
own `trigger-deprecated-warning` fixture — where it reports the deprecation
and nothing else — drew two `attribute … is not allowed here` errors from us.
Modelled on epubcheck's `epub-trigger.rnc` and probed in four shapes.

**A standalone stylesheet is asked both property questions** (#124).
`CSS-014`'s mirror, `OPF-018` — declared `remote-resources` that the
stylesheet never uses — was missing, so a stylesheet declaring the property
over a local font drew nothing. 0 of the 405 shelf books declare that property
on a CSS item, so the probes are the whole evidence here rather than the
unchanged shelf.

**A remote font still has a media type** (#124). The `@font-face` walk skipped
every external URL outright, so `CSS-007` never examined one; epubcheck asks
the question of the manifest entry whether or not the URL is remote.

**A media overlay's remote audio counts as a reference** (#125). Surfaced by
#121: once `OPF-097` reached remote manifest items, the remote audio of every
media-overlay book was reported unreferenced, because the only thing
referencing it is the SMIL and that extractor dropped external hrefs. Fixed as
one walk with two outputs rather than a second walk — a mirrored function is
how the local half of this came to be missing a source in the first place.

**`OPF-097` now reaches a remote manifest item** (#121). In epubcheck it is
one check over every manifest item, and the `RSC-006` branch beside it is
additional rather than an alternative; ours ran the remote branch and stopped,
so a remote resource nothing references drew the error and not the usage note.
Two of epubcheck's own fixtures show both on one item.

Only 3 of the 405 shelf books declare a remote manifest item at all, so the
unchanged shelf diff says little here — the fixtures are the evidence.

**The nav-document errors now point at `<manifest>`** (#122), which is where
epubcheck points and where the missing property belongs. Reported by JSWolf on
MobileRead in the same post as #120: we named line 2 — the package root, on
every book — where epubcheck named line 15. Nothing about the finding was
wrong, only where it said to look, which matters to anyone using an editor
that jumps to the position.

The column still differs by our own convention: we point at the start of a tag
and epubcheck at the end of it, consistently, everywhere.

**A misspelt `<guide>` child is an error again** (#120). Reported by JSWolf on
MobileRead: `<guide><referen/>…</guide>` validated here and draws `element
"referen" not allowed anywhere; expected element "reference"` from epubcheck.
Our grammar had the child as *any* element, so any name passed. Both of
epubcheck's grammars name it, so the fix holds at either version.

The comment that justified the anonymous child said naming it "would duplicate
a violation the surrounding grammar already reports". Nothing reported it —
the silent-skip shape, where a case falls between two checks and produces no
output at all. Measured after the fix: one finding, not two.

Fixing it exposed two more beside it, since `<tours>` carried the same
comment: its child is named `tour`, and a `<tour>` holds `<site>` elements. A
well-formed `<tours><tour title="t"/></tours>` had been silent here and drew
`element "tour" incomplete; missing required element "site"` from epubcheck.

305 of the 405 shelf books carry a `<guide>` and the per-book diff is
unchanged, so nothing was invented on real books.

**A stylesheet an SVG content document links now counts as referenced**
(#112). `OPF-097` called it unreferenced on two of epubcheck's own `*-valid`
fixtures, where epubcheck reports nothing at all — an SVG linking its CSS
through `<?xml-stylesheet?>` or through `@import` inside `<style>`.

Two places already walked exactly those forms and neither registered the
reference: one asks only whether the href is remote or a `file:` URL and then
drops it, the other reads the stylesheet for class names and runs for
media-overlay SVGs only. Neither was wrong about its own question — the
reference belonged to a third list that neither had joined. `<link
rel="stylesheet">` is registered with them, though no fixture covers it, since
it is the same walk asking the same question.

## [0.12.4] - 2026-08-27

**Encrypted resources are no longer parsed as their declared type** (#101).
Reported by Doitsu on MobileRead with an obfuscation test book and both
tools' output beside it.

An obfuscated or encrypted resource's bytes are ciphertext. Both tools say
`RSC-004: its content will not be checked`; epubcheck means it, and we said it
and then parsed the file anyway. That produced **fifteen findings epubcheck
does not make** — ten fatal `RSC-016` on three encrypted XHTML documents,
three `CSS-008` on an encrypted stylesheet, and `OPF-029` plus `PKG-021` on an
encrypted PNG.

The ten fatals were the worst part rather than merely the loudest: a fatal
drops the rest of that document's findings, so the false positives were also
hiding whatever was genuinely there.

`Ocf` now records the paths named by a `<CipherReference>` and offers
`read_content`, which declines them. Fifteen of the twenty read sites in
`opf.rs` take a manifest resource's content and use it; the other five are
structural — `container.xml`, the package documents, `encryption.xml` — and
still read normally.

**The filter is deliberately not inside `read`.** The rule is about content,
not about the resource: an encrypted font remains subject to `OPF-097`,
`PKG-026` and everything else, exactly as in epubcheck's output for the same
book, and the encryption checks themselves have to read these entries.

On the reported book the two tools now agree exactly, with nothing on either
side that the other lacks.


Three more, all found the same way and none of them reported by anyone:
by pointing `compare` at **epubcheck's own test fixtures** for the first
time. That corpus is scored by the corpus harness against expectations
written in Gherkin; running the two tools over the same books asks a
different question, and the corpus harness structurally cannot see this
class — its "no other errors" half compares at warning-and-above, and all
three of these are usage-level.

**`srcset` on a `<picture>`'s `<source>` is now read** (#102).
It was wrong in both directions at once, which is how it survived: the
candidate was not counted as *referencing* its target, so `OPF-097` called a
used image unreferenced on five of epubcheck's fixtures, and it was not
checked against the manifest either, so `RSC-008` stayed silent where
epubcheck reports it. Widening the walk then exposed an older bug underneath:
the source-set parser splits on commas, and a base64 `data:` URL contains
them, so its body became a phantom candidate. That had been true for `<img
srcset>` all along and had never fired for want of a fixture.

**A `<link>` target in `META-INF/container.xml` counts as declared** (#103). A
multiple-rendition publication declares its mapping document there rather
than in any package manifest, so the file sits at the container root
belonging to no rendition — and `OPF-003` called every one of them an
undeclared container resource. Six of epubcheck's `renditions-mapping-*`
fixtures showed it.

**`NAV-004` counts sections in fixed-layout content too** (#104). Exempting them was
a false positive on `edupub-fxl-valid`, a fixture whose name says valid.
The exemption is real but belongs to the sectioning-and-headings check, where
it came from: a fixture comment reading "Section with no heading OK in FXL",
which is about headings and not about counting. epubcheck's
`processSectioning` gates on `isLinear` and the EDUPUB profile and nothing
else.

**`RSC-004` is reported per manifest item, not per `<CipherReference>`** (#105).
Same instrument as the three above, and the fourth fixture it turned up.

The note says a file's content will not be checked. epubcheck reports it from
the checker it builds for each manifest item, so a file that is encrypted but
declared in **no** manifest draws `OPF-003` and nothing else — no checker was
ever going to read it, so there is nothing to say this about. We reported it
from `encryption.xml`, which added the note for exactly those files.

`canDecrypt()` reads like a question about the cipher and is not one: every
encryption filter epubcheck has returns `false`, the two font-mangling ones
included, so "encrypted" and "cannot be decrypted" are one condition and no
algorithm test is needed.

Two existing tests had encoded the old scope and failed on the fix — their
control named a file deliberately kept out of the manifest, which under the
correct rule asserts the opposite of it.

Also fixed: `is_encrypted` compared `encryption.xml`'s `URI` values against the
raw name from the zip's central directory without normalizing. Those need not
agree byte for byte — a container written on macOS can hold a decomposed
filename where the document naming it writes the precomposed one — and the
comparison failed **open**, so the resource would have been parsed as its
declared type after all. Non-ASCII names only, so nothing on the shelf or in
the corpus could have shown it.

**A reference epubcheck aborted asks no further questions** (#106). A remote
`<iframe>` drew `RSC-006` *and* `RSC-032`, where epubcheck reports the first
and stops: `checkRemoteReference` ends its `RSC-006` with
`throw new CheckAbortException()`, so the fallback question is never put to a
reference that may not be remote in the first place. Two errors for one
defect, the second aimed at the wrong half — a remote `<iframe>` is not fixed
by giving its target a fallback.

Skipping every remote reference would have been wrong: EPUB 3 permits a remote
`<audio>`/`<video>`, font or spine item, those do not abort, and the fallback
question can still fire there — `audio/x-wav` is remote-legal and is not a
Core Media Type. The suppression therefore reads the set `opf.rs` already
computes to decide `RSC-006` rather than restating the rule.

**`embed@src` and `input@src` join that set** (#107), where epubcheck has had
them all along as `GENERIC` references, subject to `object@data`'s rule: a
remote target is allowed only when the manifest declares it audio, video or a
font. Missing them cost twice over — no `RSC-006` where epubcheck gives one,
and `RSC-032` in its place — and the two defects were each other's cover.

**No `CSS-028` for an empty `@font-face` block** (#108). epubcheck emits that
note from its *declaration* handler, so it gives one per declaration where we
give one per rule; `COVERAGE.md` documents the difference and our count is
always the lower of the two. With no declarations its count is zero and ours
was one, which is not a granularity difference but a note about an embedded
font where there is no font.

**`NAV-004` counts sections in linear spine items only, and the navigation
document is one of them** (#110). Two errors in opposite directions in one
condition, which is why they hid: a manifest document the spine never names
was contributing sections, and the nav — excluded by name — was not
contributing its own. epubcheck marks anything with `spinePosition < 0` as
non-linear, gates `processSectioning` on that, and runs the same handler over
the nav, so its count includes the nav and excludes the stray document. Ours
did the reverse of both, and in most books the two cancelled out.

The same reading finishes yesterday's fixed-layout fix: the EDUPUB structure
and semantics rules are selected only for an item that is neither fixed-layout
nor non-linear, which is where that exemption actually lives.

**The corpus recall denominator drops 607 → 599, and that is an instrument
fix rather than a loss** (#111). The harness wraps a loose fixture into a
minimal book, and for eight EDUPUB scenarios that wrap puts the document
outside the spine — where epubcheck applies those rules only to documents
inside it. Handed the harness's own book, epubcheck reports nothing there
either, so the suite cannot pose its question and those eight had been scored
as hits. They are now skipped, with the reason and the count printed on every
run. The README's published figure moves with it, and says why.

**Every release archive now carries a checksum and a signed build
attestation.** A ninth asset, `SHA256SUMS.txt`, covers all eight archives in
`sha256sum` format, and each archive gets a GitHub build-provenance
attestation verifiable with
`gh attestation verify <archive> --repo veripublica/epubveri`.

The two answer different questions and `docs/INTEGRATING.md` says so plainly:
the checksum proves the download is intact and needs no tooling, but it sits
beside the archives, so it cannot prove much about provenance on its own; the
attestation is signed by GitHub's OIDC identity for this repository and
workflow, and names the commit that produced the file.

**Why now.** The npm package has carried SLSA provenance since 0.7.8 and the
binaries had nothing — the wrong way round, since the binary is the artefact
people actually run. It is also the artefact a Sigil or calibre plugin
downloads on the user's behalf, where the usual reassurance for a small tool
("unzip it and read the source") does not apply at all. Prompted by KevinH's
explanation on MobileRead of why Sigil's plugin ecosystem needs no verification
system: his reason is that plugins are short, pure Python and carry no
binaries. That is true of them and not of ours, which is our gap to close
rather than Sigil's.

Also in the release workflow: it now **asserts that all eight archives are
present** before writing anything. The build jobs upload with
`if-no-files-found: ignore`, so a packaging step that quietly produced nothing
still reported success and `needs: build` did not catch it — the v0.5.10
failure from the other direction, a release published with fewer binaries than
it claims, which looks exactly like a finished release.
## [0.12.3] - 2026-08-26

Eight changes. The first three come from Doitsu's report on MobileRead #248,
and the first of those is the report itself — reproduced, and deliberate. The
next three came from a new audit binary and from sizing an open issue,
rather than from anyone hitting them, and one of those is a false positive we
had been shipping all along. The last two are documentation, and one of them
is a promise made publicly before it was published — the wrong order, and the
reason this release is cut today rather than tomorrow.

**A `<script src>` target stays exempt from the fallback requirement**
(#97). Doitsu found that epubcheck reports `RSC-032` for a `.js` declared
`text/x-javascript` and we do not. EPUB 3.4 exempts resources referenced from
`<script src>` — the spec editor's [w3c/epubcheck#1654][1654], `accepted` —
and epubcheck has not implemented it yet. This is the permissive direction,
which this project ships without a flag, so nothing changes. The reason is
now written next to the list someone would edit, because widening that list
is exactly what happened while investigating the report; the test guarding
the exemption caught it within the hour.

[1654]: https://github.com/w3c/epubcheck/issues/1654

**`iframe@src` and `input@src` are now asked for a fallback, and `input@src`
counts as referencing its target** (#98). Asking which *other* references we
never posed the fallback question to found three real defects. `iframe@src`
was missing outright. `input@src` was gated on `type="image"`, while
epubcheck's `startInput` registers it whatever the type is. And a third,
worse than either and reported by nobody: `is_resource_reference` did not
know about `input@src` either, so an ordinary `<input type="image"
src="cover.png">` drew `OPF-097` claiming nothing referenced `cover.png`.
That one was a false positive on valid HTML5.

Two hand-maintained lists answering different questions about the same markup
had drifted apart. The element list is now a table documented against its
source — the GENERIC registrations in epubcheck's `OPSHandler30` — with
`script`'s deliberate absence recorded in the same place.

**`OPF-090` now names the preferred media type** (#99), also Doitsu's
suggestion: `media-type 'application/vnd.ms-opentype' is a non-preferred (but
valid) Core Media Type; 'font/otf' is preferred`. All six non-preferred types
have a row, verified against epubcheck one book at a time, including the
extension-dependent `application/font-sfnt`. `params[0]` is unchanged and the
preferred spelling is appended, so consumers indexing it are unaffected.


**`OPF-037` no longer fires on EPUB 3 books** (#100). The deprecated
`text/x-oeb1-css` media type draws a warning from epubcheck on an EPUB 2 book
and nothing on an EPUB 3 one; we warned on both.

Worth the paragraph because of *how* it was found. `OPF_037` has one call site
in epubcheck, `OPFChecker.checkItem`, in a base class rather than a `*30` one
— so every "grep the call sites" pass classifies it as version-neutral, which
is what three separate audits did this week. What actually scopes it is one
level up: `OPFChecker30` overrides `checkItem` **without calling `super`**, so
the EPUB 3 path never runs the base method. Same shape as OPF-042.

That is now checked mechanically rather than remembered. A new harness binary
reads epubcheck's own call sites for every ID we emit and reports the two
version-scoped classes, including the override-without-`super` one. It found
this on its first run.


**Eleven ordinary SVG 1.1 element names were missing from our vocabulary, and
their absence was a false positive** (#93). `altGlyph`, `altGlyphDef`,
`altGlyphItem`, `animateColor`, `color-profile`, `definition-src`, the four
`font-face-*` names and `glyphRef` are all plain SVG 1.1; epubcheck accepts
every one of them and we were reporting `RSC-025`. Usage level, so no verdict
moved, but wrong findings on valid markup all the same.

Nothing could have found it from a book: no title on the 405-book shelf uses
SVG fonts, `altGlyph` or a colour profile, so `compare` never had the chance.
It came out of extracting the element declarations from
`schema/20/rng/svg/*.rng` and diffing them against our list — **and the diff
was done before turning that list into an EPUB 2 error, which is the only
reason this shipped as eleven wrong usage notes rather than eleven wrong
errors.**

**SVG required attributes are now checked at both versions** (#93). epubcheck
runs the SVG 1.1 grammar normatively for EPUB 2 and informatively for EPUB 3,
so the identical missing `width` is `RSC-005` there and `RSC-025` usage here.
Only the EPUB 2 half shipped in 0.12.2, because the gap that prompted it was
an EPUB 2 one; the EPUB 3 half was silent until the vocabulary diff went
looking. Both halves are measured against epubcheck one book per row, the two
negatives (`line`, which requires nothing, and a complete shape) included.


**The download surface is documented and promised** (`docs/INTEGRATING.md`).
That file specified the JSON envelope and the exit codes and said nothing
about the other half of the contract — the release archives a plugin actually
downloads. Reading the Sigil plugin's source showed it matching asset
filenames by exact string equality, which made those names an undocumented
interface. They are now written down: eight archives named after the Rust
target triple, stable, added to but never renamed. Two corrections go with
them, stated generically rather than to anyone in particular — resolve through
`releases/latest` rather than the first element of `releases`, which includes
prereleases and drafts, and do not assume a tag parses as three integers.

A second section says **how often to update, and whether to update at all.** A
validator is not an ordinary dependency: new checks mean a book that was clean
yesterday is flagged today with nothing changed by its author. That is correct
behaviour and still a bad surprise unannounced. Pin a version, check no more
often than every few days, and show which epubveri version produced a report.

**The download steps now warn about GitHub's "Source code" archives**
(`docs/USAGE.md`). GitHub adds two of them to every release automatically;
they contain source text rather than the program and nothing in them will run.
`USAGE.md` already named the right file per platform, which helps, but never
said what those extra entries were. Prompted by KevinH on MobileRead,
explaining why Sigil keeps plugin distribution off GitHub — most of his users
are not GitHub users and cannot tell a release zip from a source zip. That is
a failure mode of the release page rather than of anyone's choice, and it
applied to us too.
## [0.12.2] - 2026-08-26

Four more version-scope defects, all found by auditing the neighbourhood of
0.12.1's ten rather than by any book reporting them. As with 0.12.1, no book
epubcheck accepts changes verdict and the 405-book shelf is byte-identical.

**Three more EPUB 3 rules were reaching EPUB 2 books** (#95).

- **`CSS-015`** on a title-less `<link rel="alternate stylesheet">`. epubcheck
  has exactly one call site for it, `OPSHandler30`:1113, so an EPUB 2 book
  cannot earn it.
- **`RSC-029`** on a `data:` URL as a manifest item href — `OPFHandler30`'s
  question, and epubcheck says nothing at all about it at EPUB 2.
- **`RSC-029`** on a `data:` hyperlink, which is the interesting one: **the
  condition is version-neutral and only the ID is not.** epubcheck's EPUB 2
  handler has no `processHyperlink` override, so the same link falls through
  to `ResourceReferencesChecker`, whose hyperlink arm reports **`RSC-010`**.
  Gating RSC-029 alone would have traded a wrong ID for silence, so the EPUB 2
  arm now reports RSC-010 for it.

Three further candidates from the same audit were probed and left alone
because both tools already agree: OPF-014, RSC-006 and RSC-033.

**Inline SVG is now checked for SVG 1.1's required attributes in EPUB 2**
(#93, in part). `schema/20/rng/content.rng` includes the SVG 1.1 modules
directly, so inline SVG in an EPUB 2 content document is validated against
them **normatively** — epubcheck reports RSC-005 errors and we reported
nothing at all. EPUB 3 is the mirror image, and we already matched it: there
the strict grammar runs informatively and inline SVG draws nothing.

The table is every `<attribute>` outside an `<optional>` in the eight
`attlist.*` defines of `svg-shape.rng` and `svg-image.rng` — `path@d`,
`rect@width/height`, `circle@r`, `ellipse@rx/ry`, `polyline@points`,
`polygon@points`, `image@width/height` — and `line`, which requires none.
Each row was then confirmed against epubcheck 5.3.0 on its own book, the two
negatives included.

This is a slice of the gap, not its closure: epubcheck validates the whole
SVG 1.1 grammar there — vocabulary, content models, attribute lists,
datatypes. The slice was taken because it is closed and enumerable, so it
cannot invent a finding epubcheck does not also make. **246 of the shelf's
325 EPUB 2 books carry inline SVG and not one of them gained a finding**,
which is what the rest of that grammar would have to clear before it is worth
attempting.

**The corpus figure is now 607/607, and nothing in the validator changed to
make it so** (#96). The last remaining "miss" was a check-mode artefact.
epubcheck's suite carries the missing-`<package>`-`version` case twice, once
per mode: `-mode opf` expects RSC-005 from the grammar, and the packaged-book
scenario expects **OPF-001** — the one we already passed. We take a packaged
`.epub` only, so the harness wraps the bare `.opf` into a minimal book, which
puts epubcheck into packaged-book mode too; handed that same book it reports
OPF-001 alone, measured.

So the scenario is now scored against epubcheck's own answer for what it was
actually given. The denominator is unchanged at 607 and the substitution
prints itself above the recall figures on every run, because a measurement
that flatters you gets checked less often.

The other route to 607/607 was refused: emitting RSC-005 beside OPF-001 would
have done it, and the packaged-book scenario forbids exactly that in its own
words. Inventing a finding to move a metric is the one thing this measurement
must never do.

What the number no longer records: epubveri has no standalone
package-document mode. That is a scope difference, not a missed check.

## [0.12.1] - 2026-08-26

**Ten EPUB 3 rules were firing on EPUB 2 books**, and chasing them turned up
two further version defects (#91, #92, at the end of this entry). All ten are
checks epubcheck runs only for EPUB 3, and every one is now gated on the
package version. No book epubcheck accepts changes verdict, and the real
shelf of 405 books is byte-identical before and after: the affected markup
— `epub:type`, `epub:trigger`, `epub:switch`, DPUB-ARIA roles, HTML5 microdata,
MathML, viewport metadata, `<script src>` — does not appear in an EPUB 2 book
produced by any normal tool.

**Who this is for, then.** Anyone converting an EPUB 3 to an EPUB 2 and
validating the result: Sigil, Calibre, and any pipeline that downgrades. Those
books carry HTML5-era markup inside an EPUB 2 container, which is exactly the
combination that reached these rules.

- **`OPF-088`, `OPF-086b`, `OPF-087`** — the whole `epub:type` vocabulary
  family. epubcheck routes all three through `VocabUtil`, whose only callers
  are `OPFHandler30`, `OPSHandler30` and `OverlayHandler`.
- **`RSC-017`** on `epub:trigger`, `epub:switch`, and deprecated DPUB-ARIA
  roles — all three rules live only under `schema/30`; `schema/20` has no
  epub:trigger, no epub:switch and no ARIA at all.
- **`RSC-005`** on HTML5 microdata. XHTML 1.1 has no `itemprop`, so on an
  EPUB 2 book the attribute is already an error from the grammar and this
  rule only doubled it.
- **`ACC-009`** on MathML with no alternative text. OPS 2.0.1 contains no
  MathML; `schema/20` never includes a MathML grammar.
- **`HTM-060b`** and its three viewport siblings (`HTM-046`, `HTM-048`,
  `HTM-060a`), which all come from `OPSHandler30` and nowhere else. Fixed
  layout is a rendition property EPUB 2 has no concept of.
- **`RSC-007`** through a `<script src>`. This one is fixed at the *collection*
  site rather than the reporting one, and the distinction matters: RSC-007 is
  not version-specific — `ResourceReferencesChecker` runs for both — but
  epubcheck's EPUB 2 `OPSHandler` registers references for a/area, img,
  object, link and iframe only, so a script pointing at a missing file draws
  nothing there. Gating the report instead would have silenced RSC-007 for
  EPUB 2 entirely, which is a rule the format really has.

**How they were found, because the method is the interesting part.** JSWolf,
on MobileRead: *"changing an ePub3 to ePub2 is pretty good at finding errors in
epubveri"*. The new `downgrade` harness binary makes that systematic — it
re-declares the shelf's 70 EPUB 3 books as EPUB 2, changing nothing but the
`version` attribute, and `compare` then diffs the result against epubcheck.
Seven IDs came back that only we reported, where the same comparison over the
real 385-book shelf returns none. Auditing the neighbourhood those seven landed
in found three more that the run could not see, because no book on the shelf
carries the markup at all. Agreement across those 70 books goes from
**49/70 to 70/70** — no ID reported by either tool alone, and no remaining row
where our count exceeds epubcheck's.

The books it produces are massively invalid on purpose; that is not a problem,
because the question is agreement with epubcheck rather than validity, and
epubcheck is handed the same bytes.

**`OPF-042` is EPUB 2 only, and we had it backwards in both directions**
(#91). `OPFChecker30.checkSpineItem` overrides the base method and never emits
it, so the check belongs to the EPUB 2 path alone — where it is asked on the
media type *first*, as an `if`/`else if` ahead of the fallback question.

- **EPUB 2: we reported none of them.** Ours was nested inside the
  fallback-chain branch, so a spine item with a fallback never reached it. The
  IDPF `haruko-jpeg` sample has twelve `image/jpeg` spine items, each with an
  XHTML fallback: epubcheck reports thirteen findings and we reported zero.
- **The set is six exact media types, not "an image".** `isBlessedStyleType` |
  `isDeprecatedBlessedStyleType` | `isBlessedImageType(_, VERSION_2)` — so
  `text/css` and `text/x-oeb1-css` are in it, and `image/webp` is not (it is in
  the predicate for EPUB 3, which cannot reach the call site).
- **EPUB 3: we reported an ID epubcheck cannot produce there.** A fallback-less
  image spine item drew OPF-042 from us and OPF-043 from epubcheck. Same
  severity, so no verdict moved, but the wrong ID on a book someone may be
  diffing against epubcheck.

The message changes with the condition, since "is an image, not a Content
Document" stopped being true once CSS entered the set.

**A subtree in an undeclared namespace is one finding, not one per element**
(#92). MathML in an EPUB 2 document is the case: `schema/20` never includes a
MathML grammar, so every descendant was scored against the enclosing block
model and reported individually. One real book re-declared as EPUB 2 produced
**243 483 findings from us against epubcheck's 10 475** — the only shape
anywhere in which our RSC-005 count exceeded epubcheck's, which normally cannot
happen because of our cascade suppression. On a nine-element MathML tree we
reported eight where epubcheck reports one, and its message says what it is
doing in its own words: `elements from namespace "…" are not allowed`.

The test is the **namespace**, not the missing element model, and that
distinction is load-bearing: an obsolete `<center>` also has no model in the
grammar, but it is in the XHTML namespace, so its subtree is still walked and
the `<font>` and `<s>` buried inside it are still named. Collapsing on "no
model" would have silenced those and undone what #24 fixed.

## [0.12.0] - 2026-08-24

**Why 0.12.0 and not 0.11.1.** One public function changed shape:
`ocf::check_encryption` takes the publication version as a third argument,
because `encryption.xml`'s content model depends on it. For a pre-1.0 crate
that is a minor bump, the same rule 0.10.0 was cut under. **Nothing moves for
anyone using the CLI, the JSON output or the browser build** — except the one
deliberate change below, where `RSC-004`'s `location` now names the encrypted
file. Unlike 0.10.0 and 0.11.0, this release *is* about the validator: four
real gaps close.

**Four gaps reported on MobileRead pages 16-17, all of them findings we missed
rather than errors we invented** (issues #87-#90). None changes what we say
about a book epubcheck accepts.

**An empty `<tours>` is now an error** (JSWolf, MobileRead #234; #87). OPF
2.0.1 makes the `<tour>` child `oneOrMore`, so epubcheck reports RSC-005
`element "tours" incomplete; missing required element "tour"` and we
accepted the element with any content at all, including none. `<tours>` was
added to the grammar in #58 for the opposite reason - we had been rejecting a
legacy book that used it - and what got added was the element's *existence*
with the permissive placeholder every other legacy child carries. Its `<guide>`
sibling three lines above already required a child, for exactly this reason.

**`RSC-004` now names the encrypted file, not `META-INF/encryption.xml`**
(JSWolf, MobileRead #235; #89). epubcheck reports `font00207.otf` with no
position; we reported the `<CipherReference>` that mentions it, which put a
note about a font under `META-INF/` for anything grouping findings by file. The
finding is a fact about the font, so the location moves and the position goes
with it - there is nothing inside a binary to point at, and an `element_path`
is resolved against the file `location` names. The message, severity, `rule`
and `params` are unchanged. **`location` is part of the JSON envelope**, so
this is visible to tooling; nothing in epubsana consumes RSC-004 (checked).
The RSC-007 sibling deliberately keeps `encryption.xml` and its position: a
reference to nothing has no target file to name.

**`encryption.xml`'s encrypted items are checked, and the rule depends on the
version** (Doitsu, MobileRead #233; #88). An `<enc:EncryptedData>` with no
`<enc:CipherData>` drew nothing: the check asked only which children
`<encryption>` had, and the RSC-004 pass then found no `CipherReference` to
walk and stayed silent too - two checks with the case between them reporting
nothing at all. What the requirement is, though, **inverts between versions**,
so a single rule would have been a false positive on half of all books. Every
cell below was measured against epubcheck 5.3.0 with one book per shape:

| inside `EncryptedData` | EPUB 2 | EPUB 3 |
|---|---|---|
| nothing | missing `enc:EncryptionMethod` | missing `enc:CipherData` |
| `EncryptionMethod` only | *accepted* | missing `enc:CipherData` |
| `CipherData` only | `CipherData` must follow the method | *accepted* |
| empty `CipherData` | expected `CipherReference`/`CipherValue` | same |

`EncryptedKey` takes the same model. Ordering turned out **not** to be
version-specific - both grammars are sequences, so a `CipherData` before the
`EncryptionMethod` is an error at 3.0 too, and the first draft of this had put
that rule in the EPUB 2 arm and left 3.0 silent. Probing the case is what found
it; the assumption read fine. Where no rootfile parses, the version is unknown
and only the version-independent rules run rather than guessing one.

**An EPUB 2 `<package>` no longer accepts any attribute** (JSWolf, MobileRead
#236; #90). `OPF20.package-element` is a closed list of three - `version`,
`unique-identifier`, optional `id` - and we granted a wildcard, so the reported
`xml:lang` and every EPUB 3 attribute passed silently. The requiredness of
`unique-identifier` and the value of `version` stay where they were, in
hand-coded checks, so this adds the list and nothing else. The EPUB 3 grammar
is untouched and asserted so in a test: `prefix` and `xml:lang` are valid on a
3.0 package, and closing both would have traded a legacy gap for a false
positive on the majority of modern books.

Measured on all 312 EPUB 2 books of the 385-book shelf before shipping: **310
carry nothing beyond those three, 2 carry `prefix`** (Calibre's, which
epubcheck rejects for the same reason), and **none carries `xml:lang`** - so
the shelf could not have found the reported case and the grammar is the
evidence. Both new findings match epubcheck exactly, book by book.

KevinH argued in the same thread that epubcheck oversteps here, since XML makes
`xml:lang` available on every element and `dc:language` describes the book
while `xml:lang` describes the package document - a different fact. That is
probably right and is not ours to act on: a book that fails epubcheck has to
fail here too, or a user gating on epubcheck cannot tell our judgement from our
bug. Disagreeing with a rule is grounds for taking it upstream, not for staying
silent about it.

### For epubsana

`opf.package.schema_violation` gains a population. The new `<package>`
attribute findings carry the `attribute "x" is not allowed here` wording your
`fix.epub3_attr_in_epub2_package` site matches on, so that fixer will start
seeing `prefix` and `xml:lang` on EPUB 2 packages where it previously saw
nothing - 2 books on the shared 385-book shelf. No message wording moved; the
population did.

Three new rule slugs, all one message shape each: `ocf.encryption.item_incomplete`,
`ocf.encryption.item_out_of_order`, `ocf.encryption.cipher_data_incomplete`.

Instruments: corpus 606/607 with 0 false positives, `epub-tests` verdict set
byte-identical, hostile clean, and the shelf per-book diff **2 books changed,
both `+1 RSC-005`, both correct**.

## [0.11.0] - 2026-08-22

**A minor bump because the JSON output changes for consumers, not because the
validation did.** Two things move for anything parsing `--format json` or
`--format ids`:

- **`-u`/`--usage` now decides what those formats contain**, as it already did
  for the human report and as epubcheck's own `-u` does. Without it, usage
  findings are absent.
- **The `summary` keys are singular** — `fatal`, `error`, `warning` — and it
  gained `info` and `usage`.

Findings, IDs, severities and verdicts are unchanged. The library API is
untouched and still never filters.

**Nothing ran rustdoc — not the pre-flight, not CI — and six documentation
defects had shipped because of it.** Two doc links pointed at private items, so
they rendered as plain text in the published docs rather than as links (one
written the same day); four bare URLs were not hyperlinks; and one harness
header's indented shell snippet was being compiled as a Rust doctest, which is
how rustdoc treats an indented block. All fixed, and
`RUSTDOCFLAGS="-D warnings" cargo doc` is now a gate in both places.

It is the same class as the `--locked` check added after 0.9.2: a thing that
fails only where nobody was looking. Worth noting it also compiles every fenced
block, so a `text` fence is not decoration.

**`-u`/`--usage` now decides what *every* format contains, not only the human
report** (Doitsu, MobileRead #231). 0.10.0 shipped it as a display filter on the
reasoning that a machine consumer receiving fewer findings than the library
produced cannot recover what it never got. The Sigil plugin's author reported the
inconsistency within hours, and measuring settled it: **epubcheck's own `-u`
gates its JSON too, counts included** — `nUsage` drops to 0 without the flag. A
command line ported between the two tools was returning different data, and one
flag meaning two things depending on `--format` is not a contract anyone should
have to remember.

The filter now runs once, over the report every format is written from, so
`--format json` and `--format ids` follow the human report exactly. Pass `-u` to
get everything — which most tools should, so they can filter in their own UI
without re-running the validator. `--advisory` findings are unaffected, as
before: that flag is their switch.

**The concern behind the original choice is answered better elsewhere.** The
*library* never filters, whatever the CLI was given — that is what actually
protects a consumer like epubsana, three of whose repair rules dispatch on
findings below error severity, and it has its own test. Recorded as
veripublica/epubveri#86, now superseded on the CLI half.

Measured: the shelf's json output without `-u` falls from 50,594 findings to
**45,179**, the difference being exactly the 5,415 usage findings. Verdicts and
exit codes cannot move — usage severity never counted toward either — and the
filter is applied after both are decided.

**The json `summary` gained `info` and `usage` counts, and its keys are now
singular** — `fatal`, `error`, `warning`, `info`, `usage`. Doitsu asked for the
counts and for the naming in the same post: *"information has no plural and
usages doesn't make sense"*. They describe what the output contains, as
epubcheck's do. `fatal`, `info` and `usage` are omitted when zero, like `fatal`
always was; `error` and `warning` are always present. **This renames published
keys**, which is why it is a minor bump rather than a patch. The WASM binding
mirrors the same shape — but never filters, having no flag to filter on.

**Unrelated, and found by a toolchain update rather than by us:** two
`chunks_exact` calls became clippy errors under Rust 1.98, which landed three
hours after 0.10.0 was tagged. Both are pre-existing — v0.10.0 fails its own
clippy gate on that toolchain — and CI has not seen it yet only because
`ubuntu-latest` still ships an older stable. Rewritten with `as_chunks`, stable
since 1.88, which is our declared MSRV. Worth knowing that a release can stop
passing its own gate without anything in the release changing.


## [0.10.0] - 2026-08-22

**An `encryption.xml` pointing at a resource that is no longer in the container
is now an error** (JSWolf, MobileRead #223). Delete an obfuscated font, leave its
`encryption.xml` behind, and epubcheck reports `RSC-007: Referenced resource …
could not be found in the EPUB`; we reported an `INFO` saying the file was
encrypted and called the book **VALID**.

**RSC-007 replaces the encrypted note rather than joining it**, which is what
epubcheck does and is the right way round: a reference to nothing is not a file
whose content was skipped. The test asserts both halves, because adding the error
while leaving the note in place would pass a presence check and tell the reader
two things about one fact, the second of them false.

**The gap had a comment claiming it was covered.** The PKG-026 check skips a
cipher reference whose target is absent, saying *"a missing resource is already
reported elsewhere (RSC-001/004)"* — and nothing was: RSC-004 says a file is
*encrypted*, never that it is missing, and no site emits RSC-001 for this. That
is the silent-skip shape this project keeps meeting, where the case between two
checks reports nothing at all, and the documented fix is to verify rather than
believe the claim. The comment now says what is actually true.

It is also the per-source reference problem again. epubcheck resolves every
registered reference through one path; resolution here is written per source —
NCX `<content src>`, content-document hrefs, `epub:textref`, the `<guide>` after
0.9.14 — and `encryption.xml` was never added to that list. A per-source design
owes a re-enumeration each time a reference kind appears, and nothing fails
loudly when one is forgotten.

All five `encryption.xml` shapes now agree with epubcheck on the ID set and the
count: empty, self-closed, a foreign child, a non-font target, and a missing
target. The false-positive control is the two real encrypted books on the shelf,
which keep all ten of their `RSC-004` notes and gain no error; the full shelf
scan is byte-identical.

**A minor bump because the library API breaks, not because the validation
changed shape.** Two things move for a Rust consumer and nothing at all moves
for the CLI, the JSON envelope or the WASM bindings:

- `report::Message` gained a `violation_kind` field, so struct literals need
  updating.
- `rng::Blame::Text` changed from a tuple variant to a struct variant carrying
  its containing element.

Everything else here is additive or is a display change. Findings, IDs,
severities and verdicts are unchanged on all 385 shelf books — the machine
output is byte-identical before and after the whole day's work.

**The WASM binding now returns the whole `data` slot, and the demo grew an
ordering control.** Through 0.9.x the binding carried its own `Data` struct
holding `params` alone, so `element_path`, `namespaces`, `advisory_basis` — and,
the day it was added, `violation_kind` — reached a CLI consumer and never a
browser one. Two shapes were written separately and only one of them was ever
compared against the envelope, so nothing reported the drift.

It was an omission rather than a decision: nothing about the browser makes those
fields harder to produce. `INTEGRATING.md` claimed the package "returns the same
envelope shape" on the morning of the day this was found, which was the
strongest argument for closing it rather than documenting it.

One shape difference remains and is now stated in three places rather than
discovered: **`data.namespaces` arrives as a JavaScript `Map`, not a plain
object**, because that is how a Rust map crosses the boundary.
`data.namespaces.get("opf")` works; `data.namespaces["opf"]` is silently
`undefined`.

Verified end to end in Node against a fixture carrying one of each shape — a
schema violation with a path and a kind, a usage finding, and an `ADV-001` with
a basis — rather than by reading the generated `.d.ts`. Three tests in the
binding assert the fields are *populated*, not merely present in the type: a
`Data` that compiles and forwards `None` forever would satisfy a weaker test and
would be the same bug.

The demo page gained an **Order** control mirroring the CLI's `--sort`, with the
same default and the same reasoning. It re-draws the table rather than
re-validating — the difference from the advisory checkbox beside it, which
changes which findings exist — and the downloaded JSON is unaffected, exactly as
`--sort` does not reach `--format json`. Its advisory label was also three
families out of date, describing only the unknown-CSS-property check from before
`ADV-003`, `ADV-004`, `ADV-009` and the whole `NEXT-*` family existed.

**The human report is now grouped by severity, most serious first, and
`--sort document` gives you the old order back** (JSWolf, MobileRead #219).
Fatals, then errors, then warnings, then info — so the findings that decide the
verdict arrive together and first. **Inside each group the file order is
unchanged**, so each group still reads top-to-bottom: one pass down the errors,
fix, re-run, then one down the rest.

**This is deliberately not what the reporter asked for, and the thread will be
told why.** JSWolf asked for *warnings* first. Most-severe-first is the opposite
arrangement, and the reason is that the set which makes a book invalid is the
set you act on, which is also what the verdict line counts.

**It is not epubcheck's order either, and that was measured rather than
assumed.** epubcheck does not sort at all — it emits in the order its checks
run, so severities cluster as a side effect. On 23 shelf books carrying both
severities the sequence differs from ours on **10**; where epubcheck prints one
run of warnings then one run of errors, we alternated, and one book broke into
34 alternating blocks. Of those 23, epubcheck happens to come out warnings-first
on 15, errors-first on 3, and interleaved on 5. So there was no existing
arrangement to preserve.

Three boundaries, each of which is the whole point of the others:

- **`--format json` and `--format ids` are always in document order**, whatever
  the user typed. A tool must never receive an order its user chose. Verified
  over all 385 shelf books: the json output is byte-identical with and without
  the flag.
- **The library is untouched.** `sort_by_document_order()` remains canonical and
  `validate_bytes` returns what it always did; this lives in the CLI's rendering.
- **It is a stable sort on the severity rank alone**, not a recomputed
  `(severity, file, line, column)` key. The library already hands over document
  order, so sorting by nothing else preserves it inside each group — and a
  recomputed key would silently stop agreeing with the library's file ordering
  the day that changed. The regression test's fixture is deliberately not in
  file-name order, so a comparator that re-derived the order fails it.

`Severity` is declared most-severe-first, so the rank is the declaration order
and there is no second table to keep in step with it.

Costs nothing measurable: on the worst book on the shelf (3,140 findings) the
whole validation is ~1.86 s and the sort is ~1.3 ms of it.


**Schema violations now carry a machine-readable `violation_kind`**, so a
consumer can group or dispatch on what kind of fault a finding is without
parsing the English message. Six values —
`element_not_allowed`, `incomplete_content`, `missing_attribute`, `stray_text`,
`attribute_not_allowed`, `invalid_attribute_value` — on `Message` in the library
and in `data` in the json envelope. `rule` is unchanged.

**These six were not designed; they were recovered.** `rng::Blame` has exactly
these six states, `push_blame` reads them to pick the right anchor, and
`describe()` then renders them into a sentence — at which point the discriminant
was dropped. epubsana was reconstructing it by slicing the leading words of that
sentence, which re-splits their groups every time a message improves; it moved
twice in the two weeks before this. So "about six kinds" stops being an
observation of 385 books and becomes a property of a type: `Blame::kind` is a
wildcard-free `match`, so a seventh engine state is a compile error rather than a
silent reclassification.

`ViolationKind::ALL` ships with it. The compile error people expect from an
exhaustive enum does **not** fire for the two ways this is actually consumed —
grouping needs `Ord`/`Hash`/`Eq` and dispatch is equality, neither of which is a
`match` — so `ALL` lets a consumer assert the set it knows about and notice a
seventh the moment it resolves a new version. The enum is exhaustive now and is
intended to become `#[non_exhaustive]` at 1.0, which is the one moment adding
that is not itself a break; `ALL` carries the signal across that line.

**`None` is a statement about the rule, never about the finding.** A rule that
carries kinds always sets one — the mapping is total, so no path produces a
kindless schema violation — and every other rule leaves it `None`. Measured over
the shelf: of 50,594 findings, 39,988 carry a kind, **0 schema violations lack
one, 0 findings outside the family have one, and 0 kind-carrying findings lack a
`params[0]`**. All six kinds occur.

**What `params[0]` means, now written down as a contract**, because the group key
is `(violation_kind, params[0])` and half of it was an unwritten assumption:

- attribute kinds — the attribute name **as qualified for display**, carrying the
  conventional prefix for the `epub`, `xml`, `xlink` and `opf` namespaces, bare
  otherwise;
- element kinds and stray text — the **local name** of the element (for stray
  text, of its containing element), never prefixed.

The two spellings never meet inside one group, because the kind already
separates attribute faults from element faults — which is why the asymmetry is
documented rather than removed. Making it uniform would mean moving the
attribute spelling, which is exactly the change that moved under epubsana in
0.9.19.

**And the part a consumer cannot see from the outside: `params[0]` is not a
string that appears in the document.** The prefix is reconstructed from the
namespace, so a book binding `xmlns:e="http://www.idpf.org/2007/ops"` and writing
`e:type` still yields `"epub:type"`. It is an identity token for display and
grouping and must not be used as a lookup key into the source. There is a test
whose fixture asserts the produced string is absent from the file.

One limit, inherited rather than introduced: element names are local, so
`(violation_kind, params[0])` cannot distinguish two namespaces — an SVG `title`
and an XHTML `title` share a key. Grouping on the message text has the same blind
spot, so the token does not add it; #84 made the *message* able to explain such a
collision, but the key still cannot represent it.

Emitted on `ncx.schema_violation` too. It fires on 0 of the 385 shelf books,
which is the reason to include it rather than a reason not to: nothing here has
ever been able to see inside that rule.

**Breaking for library consumers**: `Message` gained a field, so struct literals
need updating. The CLI output is unchanged, and the json addition is additive.


**`Blame::Text` now carries its containing element, closing a path that could
produce a finding with empty `params`** (epubsana, 2026-08-22). The stray-text
arm recovered the parent with `run.parent().filter(is_element)` and fell back to
an unnamed message with **no `params` at all** when that failed. The fallback was
argued unreachable — the walk only reaches a text run from inside an element —
and the argument is almost certainly right: 0 of 385 shelf books, 0 of 209
`epub-tests` publications and 0 corpus scenarios have ever produced it.

It is closed anyway, because "argued unreachable" is not something a written
promise about `params[0]` may rest on, and this project has been wrong in exactly
that shape before. The construction site already had the parent in hand, so the
variant now carries both nodes: the run, which the position and `…/text()[n]`
path come from (#68), and the parent, which the message and `params[0]` name.
The recovery is gone rather than guarded, so there is no branch left to fail.

Behaviour is unchanged and measured to be: the full 385-book shelf scan is
byte-identical, the corpus holds at 606/607 with 0 false positives, and the
message keeps its exact wording — `stray text is not allowed directly in "body";
wrap it in an element` — which matters because epubsana selects on that prefix.

**This is a breaking change to the library API**, since `rng::Blame` is public
and `Text` changed from a tuple variant to a struct variant. Nothing in the CLI,
the JSON envelope or the WASM bindings moves.


**`META-INF/encryption.xml` is now checked for having any content at all**
(JSWolf, MobileRead #221). A book whose obfuscated font had been deleted kept a
childless `<encryption>` element behind; epubcheck reports
`element "encryption" incomplete` and we reported nothing.

The whole rule is one line of epubcheck's `ocf-encryption-30.rnc` —
`element encryption { grammar { … }+ }` with `start = xenc_EncryptedData |
xenc_EncryptedKey`, where the `+` is the cardinality and the `start` is the
vocabulary. So `<encryption>` now requires one or more `EncryptedData` or
`EncryptedKey` children **from the XML Encryption namespace**, and rejects any
other child. Nothing else about xmlenc is validated; the file's scope here stays
"presence and shape".

**Both halves shipped, not just the one that was reported.** For
`<encryption><foo/></encryption>` epubcheck emits two findings — the child is
rejected *and* the element is still incomplete — so implementing only the
emptiness case would have called that shape complete. That is the
gap-between-two-checks pattern this project keeps meeting, where the case no
check owns reports nothing at all. On all three shapes (empty, self-closed,
foreign child) the ID set and the finding count now match epubcheck exactly.

**The evidence is fixtures, and it had to be: real books say almost nothing
here.** Only **2 of the 385 shelf books** carry an `encryption.xml`, and both are
canonical. Two independent negative controls back the rule instead — those two
books, and W3C `epub-tests`' two font-obfuscation publications — and all four
produce zero findings from it. The full shelf scan is byte-identical before and
after: same 50,594 findings. This is an editing accident rather than a producer
bug, which is exactly the class a shelf of finished books cannot contain and a
Sigil or calibre user meets.

Positions differ from epubcheck's, as they do for every finding: its SAX locator
reports the incomplete element at its end tag, ours at the element itself.


**An attribute fault is now reported at the attribute, not at the element
carrying it** (JSWolf, MobileRead #220). `<spine page-progression-direction="ltr"
toc="ncx">` in an EPUB 2 package drew the right error on the right line at
column 1, where the reporter expected column 8 — the first character of the
attribute we had just named in the message.

`element_path` has ended in an `/@name` step since #18; `position` was left
pointing at the element, so the machine half of a finding named the attribute
and the human half did not. That is now one answer instead of two.

**Not a parity change, which is worth stating because it looks like one.**
epubcheck's SAX locator reports an attribute fault at the character *after* the
start tag's `>` — column 53 for a 52-character line in the reduced case — so its
column pointed at neither the element nor the attribute, and ours never matched
it. There was no third position to converge on; there was a more useful one and
a less useful one.

Measured over the 385-book shelf, old build against new: the finding set is
identical — same 50,594 findings, same ids, rules, locations and element paths —
and **11,332 positions move, which is exactly the set of findings whose
`element_path` ends in `/@attr`, all of them and nothing else**. Every move is
forward on the same line (median +55 columns, max +179), across 77 books and
five ids (RSC-005, OPF-088, OPF-086b, RSC-020, OPF-087). The line never changes.

Two consequences for consumers, neither of which changes a verdict:

- **The human report reorders slightly.** `sort_by_document_order()` keys on
  position, so an attribute fault now sorts after its own element's findings
  rather than tying with them — 934 of 50,594 lines change place. This is the
  more accurate document order, and it is the same sort that 0.9.28 made
  deterministic.
- **`position` moves for anything that consumes it.** Within the same element
  and the same line, so a consumer resolving a finding to its element via
  `element_path` is unaffected; one matching a column exactly is not.

No test covered the old column, which is why the bug survived: the line was
always right, so a position-exists or line-only assertion would have passed
throughout. The new test pins the column to the attribute *and* asserts it is no
longer the element's.

**A by-product worth recording: it halved the number of tied positions.** Two
attribute faults on one start tag used to share the element's position by
construction; they now have their own. Across the shelf, findings sharing an
exact position fell from **3,295 in 1,643 groups to 1,449 in 720**.

Ties are where finding order stops being decided by the sort and starts being
decided by the order the checks emitted them — the level below the file ordering
0.9.28 fixed, and equally invisible. Measured rather than assumed: three
separate-process runs over the 32 books holding every remaining tie are
byte-identical, and the largest group (eight OPF-003s at one spot) is driven by
`ocf.names`, a `Vec` in zip entry order. Nothing was wrong, and nothing was
guarding it either, so there is now a test that pins tie order to zip order and
fails when the walk is switched to a `HashSet`.


**A schema violation whose message contradicted itself now says what actually
differs** (#84, BeckyDTP). A content document whose root was a bare `<html>`
with no namespace drew `element "html" is not allowed here; expected "html"` —
true, useless, and the sentence a reader stalls on. The rejected name and the
expected one carry no namespace (the grammar's first-set is local names by
design), so when they collide the namespace is the only thing left that can
differ; the message now names it. epubcheck reports the same fact from the other
end, as `elements from namespace "" are not allowed`.

The clause is **appended**, not substituted: epubsana selects on the `element `
prefix, and rewording would silence a fixer quietly rather than break it loudly.
A message whose names differ is untouched, so the common case gains no noise.

No book on the 385-book shelf produces the collision, so the test carries the
evidence rather than the shelf.


**Usage-severity findings are hidden from the human output by default, as they
are in epubcheck** (`-u`/`--usage` shows them). Asked on MobileRead and answered
there: JSWolf met CSS-028 twice and read it as an error message on correct
content, and Doitsu proposed mirroring epubcheck's flag.

The asymmetry was ours, not epubcheck's, and it was measurable: of 385 real
books, **60 produce nothing but usage findings** — perfectly good books that
printed a median of six lines a reader could reasonably mistake for problems.
The default level now matches epubcheck's exactly: fatal, error, warning and
**info**, with usage excluded — `info` stays, which was read off epubcheck's own
`--help` rather than assumed.

Two boundaries this deliberately does not cross:

- **It is a display filter, never suppression in the library.** Three of
  epubsana's fixers dispatch on rules that fire below error severity; hiding
  those from `validate_bytes` would take them dark silently, with nothing on
  either side reporting it. `--format json` and `--format ids` are likewise
  never filtered — a machine consumer receiving fewer findings than the library
  produced is the same harm in a different coat.
- **`ADV-*`/`NEXT-*` are exempt.** They are emitted at usage severity, so a
  filter written as "severity is not usage" would make `--advisory` print
  nothing at all — the flag would not fail, it would go quietly inert. They
  print whenever present, because `--advisory` already decided that.

The verdict cannot move either way: usage findings never counted toward it.


**The two `media:*` cardinality rules were wrong in both directions at once.**
They had the shifted context 0.9.29 fixed for the `rendition:*` family — firing
once per occurrence rather than once per constraint — and a second defect on top:
they counted only within a `@refines` group, where epubcheck counts globally.
Measured with one probe per shape:

- two `media:active-class` in the same group: we reported twice, epubcheck once;
- two in *different* groups: we reported **nothing**, epubcheck still reports
  once.

So a book could carry the defect and hear nothing from us, or hear about it
twice. The global count fixes both, and the `@refines` sub-rule — which the two
tools already agreed on — is untouched.

## [0.9.29] - 2026-08-21

**Two CSS-004 false positives, found in epubcheck's own CSS test files, which no
instrument here had ever reached.** The corpus holds 24 bare `.css` fixtures —
epubcheck's CSS *parser* unit tests — and they live outside any book, so the
corpus harness (which walks scenarios) never sees them and the shelf has no
stylesheet like them. Wrapping each in a minimal book and diffing both tools
found six differences, four of them the documented CSS-028 and selector-list
granularity. The other two were ours:

- `bom-charset15.css` — a UTF-8 BOM followed by `@charset "iso-8859-15"`. **The
  BOM settles the encoding**; CSS Syntax 3 §3.1 says the decode algorithm
  "gives precedence to a byte order mark, and only uses the fallback when none
  is found". We read the declaration anyway. We checked for a UTF-16 BOM and
  never a UTF-8 one.
- `charset-empty.css` — `@charset '' ;`. **Not an encoding declaration at all**:
  the spec recognises it by an exact byte pattern (`@charset "`, one space, a
  *double* quote, then the label, then `";`) and states outright that "multiple
  spaces, comments, or single quotes … will cause the encoding declaration to
  not be recognized". We were reading it off the parse tree, where a tokenizer
  quite correctly sees a perfectly good at-rule named `charset`.

Both now go through `byte_exact_charset`, matched against the raw bytes, and a
UTF-8 BOM suppresses the check. epubcheck is silent on both fixtures and so are
we; the two that should error still do.

**Not styloria's, and worth stating because the boundary is easy to get wrong.**
Its tokenizer note already says encoding determination happens before it runs
(§3.2) and that it merely tolerates a leftover BOM. Determining a declared
encoding from bytes is the caller's job — and styloria's syntax errors landed at
exactly epubcheck's positions on all 24 files, with no crash.

**RSC-020 now reaches the `<guide>` and CSS `url()` — the two sites that were
closed on the wrong kind of evidence.** Three days ago the five remaining
reference sites were closed because the population was zero across 375 books.
The owner's criticism ended that: a rule can be absent from one person's library
and present everywhere else, and negative evidence is exactly what a
non-representative sample cannot supply.

Acting on it rather than writing it down: **epubcheck's own corpus has no
fixture for these sites either** — checked, all 409 expanded fixtures — which
says something about its test suite rather than about real books. The witness
that settled it was the oracle. A probe book carrying
`<reference href="a b.xhtml">` and `url("i m.png")` draws RSC-020 from epubcheck
at both sites and drew nothing here. Both now agree.

**The question was the problem, not the measurement.** "How often does this
occur" needs real books, and ours are not a random sample. "Does epubcheck
report it" needs a probe, always answers, and is the only question a parity gap
actually turns on. Two of the five sites were real gaps; the shelf and the
corpus had both been asked the wrong thing.

**One divergence stays and is deliberate:** `url(i m.png)` *unquoted*. An
unescaped space makes that an invalid url-token, so no URL is produced and no
reference exists to check; epubcheck's older parser extracts it anyway. Same
class as the CSS-008 empty-declaration divergence — teaching our CSS layer to
recover from invalid syntax in order to match would be the detector serving
parity. The quoted form is valid CSS and agrees exactly.

The shelf is unchanged and the corpus is byte-identical, which is the expected
result for a gap neither could see: the evidence is the probes and two tests,
each verified by disabling its site and watching the assertion name it.

**A closure written on shelf evidence now says so, starting with RSC-020's.**
The owner's criticism, and it lands: *"we are measuring against our library only
and then generalising — we are not the world's library."* The five RSC-020
reference sites were closed on "population zero across 375 books", and that
shelf is one person's collection: mostly Turkish reflowable trade fiction,
almost no fixed-layout, no media overlays, no dictionary or index profile.
Population zero *there* is weak evidence that a case does not exist.

The sharper half is that **the note already carried the caveat and the decision
rested on the measurement anyway** — epubsana's phrase for it is the right one:
a caveat that does not change the decision is decoration. The row now says which
kind of evidence closed it and names the second source for anyone reconsidering:
epubcheck's own corpus, whose fixtures are built to trigger each rule
deliberately, which is exactly the coverage negative evidence needs and exactly
what a non-random sample of real books cannot supply.

No behaviour changes; this is the matrix telling the truth about its own
grounds.
**Seventeen message IDs were spelled with a hyphen where epubcheck prints an
underscore.** `HTM-054`, `HTM-055`, `HTM-056`, `HTM-057`, `HTM-058`, `HTM-059`,
`HTM-061` and `MED-007`, `MED-010`..`MED-018` are now emitted as `HTM_054`,
`MED_013` and so on, matching `MessageId.java` character for character. A
toolchain grepping epubcheck's output for `MED_013` now finds ours too, which is
the entire reason this project adopted epubcheck's ID scheme rather than
inventing one.

epubcheck switches from hyphens to underscores partway down its message list —
`HTM-052` then `HTM_053` — for one contiguous block of 21 IDs, and nothing in
its source explains why. The oddity was already known here and written up
correctly in a comment on `HTM_060A`; it was applied to that pair and to nothing
else, while `HTM_056`/`057`/`059` sat three lines above the comment spelling the
same block with hyphens.

**No instrument could have caught it.** The corpus and `compare` regexes both
accept `[-_]`, so all seventeen scored as matches against epubcheck's
differently-spelled IDs; the coverage matrix normalizes both sides to hyphens on
purpose, since its question is which checks exist rather than how they are
spelled. All 385 shelf books are silent here too — every one of the seventeen is
a fixed-layout viewport or media-overlay check, and no book on the shelf uses
either. A test now pins the set against `MessageId.java`, and the coverage
harness carries a note saying why that question is not its to answer.

If you match on any of these IDs, the spelling changes. No condition, severity,
message or position moves.

**A media overlay's own references now count for OPF-097 — a false positive on
every media-overlay book.** The audio file of such a book is referenced by its
SMIL overlay and by nothing else, which is exactly what the format prescribes.
OPF-097 asked whether any *content document* drew a manifest resource, and a
Media Overlay is not one, so the audio was reported as declared-but-unreferenced.

The cause was ordering rather than a missing rule. A comment at the check
claimed the overlays' audio and text targets were "collected by the overlay
pass"; that pass runs *after* this one, so nothing it collects could ever
arrive in time. A new `smil::resource_refs` answers only the reference question
and reports nothing, and runs before the check.

Found by running W3C's `epub-tests` — 209 reading-system conformance
publications — against both tools for the first time. It fired on 19 of them.
**No book on the 385-book shelf carries an overlay at all**, and epubcheck's
corpus has no scenario that pairs one with this check, so neither instrument
could have found it; the shelf is byte-identical across the fix.

While there: `smil_items` was still ordered by `HashMap` iteration, the third
site of the nondeterminism fixed in 0.9.28. A book with two overlays printed
their findings in a different order on every run.

**The last three false positives W3C's `epub-tests` found, and one false
negative that came with them.** After the media-overlay fix above, four of the
209 reading-system conformance publications still carried an ID epubcheck does
not report. All three causes turned out to be different, and none of them was a
rule that was simply too strict.

- **OPF-003 is a container-level question.** epubcheck asks it once in
  `OCFChecker`, searching *every* package document at the same time, and counts
  a metadata `<link href>` as a declaration alongside a manifest `<item>`. Ours
  ran inside the per-package check with only that package's `<item>`s.
  `ocf-package_multiple` declares three renditions in three directories, so
  each package blamed the other two's files — 18 findings against epubcheck's
  none — and `pkg-linked-records` referenced an ONIX record by `<link>`.
- **A file URL was reported and then dropped.** RSC-030 ended with a
  `continue`, on the reasonable-sounding grounds that it was the whole story
  for a `file:` URL. It is not — the reference still has to be classified, and
  skipping that cost a finding in *each* direction on `pub-file-urls`: a
  correctly declared `remote-resources` property drew OPF-018 "doesn't appear
  to be needed", and three `<iframe>`s in a restricted context drew no RSC-006
  where epubcheck reports one each. epubcheck agrees a file URL is remote
  (`isRemote` is "not `data:` and not same-origin"); it just does not stop
  after saying so. Third instance of this shape in `opf.rs`.
- **A Schematron rule lost its context in the port.** `title.present` is
  `<rule context="h:head"><assert test="exists(h:title)">`, ported as a search
  for the first descendant named `title` anywhere. It therefore fired on a
  document with no `<head>` at all — `pub-xml-external-id` carries a one-line
  `<span>The test fails.</span>` and was told its head should have a title. The
  sibling rule `title.non-empty` has a different context (`h:title`) and is now
  kept separate, and both are namespace-qualified as `h:` means.

On the 209 publications: IDs reported by epubveri alone go from
`{OPF-003: 2, OPF-018: 1, RSC-017: 1}` to **none at all**, exact ID-set
agreement from 172/209 to 195/209, and RSC-006 leaves the list of IDs only
epubcheck reports. The shelf is byte-identical across all three, including the
11 books that report OPF-003, the 2 with file URLs and the 2 that hit the
head/title rule; the corpus is unchanged at 606/607 with 0 false positives.

**A spine SVG's own references now count for OPF-097 — the same per-source gap
as the media overlay, in a second source.** `content_docs` selects on
`application/xhtml+xml`, so a standalone SVG content document's references were
collected by nothing. W3C's `lay-pp-embedded-images-svg` is eight
`<svg><image xlink:href="../images/A.png"/></svg>` plates in the spine: we
called all eight PNGs unreferenced where epubcheck called none of them, nine
findings against its one.

References are gathered per *source* here and per *reference* in epubcheck, so
each new source has to be added by hand and nothing fails loudly when one is
missed. That is now twice, and the two extractors sit side by side behind one
media-type dispatch so the next one joins them rather than being forgotten.

The shelf is byte-identical, including the 15 books that declare
`image/svg+xml` items; the corpus is unchanged.

**A `toc` nav link to a non-Content-Document drew RSC-010 twice.** #78
generalised that check from the two toc paths to every hyperlink and left the
narrower `navdoc` one in place, so both fired on the same reference at the same
position. epubcheck reports one; W3C's `pub-foreign_bad-fallback` is where the
pair showed up side by side. The general site subsumes the narrow one, which is
removed along with the two parameters that existed only to feed it.

The removed check had no test of its own, which is why nothing failed when the
duplicate appeared. Its replacement asserts the **count**, not presence — a
`> 0` assertion would have passed throughout the duplicate's entire lifetime,
which is the lesson #76 already left here.

The shelf cannot speak to this one: **no book on it reports RSC-010 at all**, so
its silence is not evidence. The corpus is unchanged and the finding is verified
against epubcheck on the publication that exposed it.

**A duplicated global `rendition:*` property drew one finding per occurrence
instead of one per constraint.** The Schematron rule is a cardinality assertion
whose context is the *metadata container* — "this property must not occur more
than once" is a single statement about the document — and ours had the context
on the `meta` element, so two duplicates produced two findings where epubcheck
produces one. All five `rendition:*` cardinality rules (`layout`,
`orientation`, `spread`, `flow`, `viewport`) carried the same shifted context.

**Third Schematron context defect this release**, after `title.present`. A rule
copied without its context is a different rule, and the two ways it goes wrong
are now both on record: lose the context and it fires where it does not apply;
shift it down to the item and it fires once per item.

Moving the context up meant the assertion had to become `<= 1` rather than
`= 1`: the rule now runs on *every* package document, where before it only ran
when a matching `meta` existed, so `= 1` would have reported every book that
does not use the property at all. All 385 shelf books are unchanged, which is
the evidence for that half; the corpus is unchanged too.

**A viewport meta in a document outside the spine was never asked about.** The
reflowable-viewport check ran inside the spine-itemref loop, so a manifest XHTML
document with no itemref — a nav document, an unreferenced cover page — was
skipped. epubcheck asks every XHTML content document: its check lives in the
per-document handler, not in the spine pass. HTM_060b was missing entirely on 7
of W3C's 209 publications and undercounted on 4 more; all 13 books that report
it now agree with epubcheck exactly.

Worth knowing for anything else that reasons about layout: **a non-spine
document is reflowable whatever the package says.** The fixed-layout flag is set
only while processing a spine itemref, so an item with no itemref never acquires
it — a package-level `rendition:layout` of `pre-paginated` does not make the nav
document fixed-layout.

The shelf can barely speak to this one: exactly **one** of its 385 books has a
viewport meta in any XHTML document at all, and it is unchanged. The evidence is
the 13 publications, not the shelf's silence.

**An itemref carrying both `rendition:layout` overrides now resolves to
pre-paginated.** The two are mutually exclusive and both tools report that
(RSC-005) — but the document still has to be validated as *something*, and
epubcheck resolves it pre-paginated: its condition reads "contains
pre-paginated, or (not reflowable and the package says pre-paginated)", so the
first disjunct short-circuits. We tested reflowable first, called such a
document reflowable, and skipped its viewport requirement, losing HTM-046 on
W3C's `fxl-spine-overrides_duplicate`.

**An error on a book does not excuse the checks after it from being right.** The
reader still gets a verdict on the rest of the document, and the wrong branch
dropped a real error silently.

This one is restrictive — it moves a document from reflowable to fixed-layout —
so the direction was checked rather than assumed: the corpus is unchanged, and
the shelf is **no witness at all**, since none of its 385 books uses a
`rendition:layout` spine override. The test carries the evidence instead, in
both property orders.

**A nav link with a dangling fragment left the reading-order check entirely**
(JSWolf, MobileRead 2026-08-21). It was dropped from the comparison on the
grounds that a dangling fragment "is already caught elsewhere as a broken
reference". RSC-012 does catch it — but RSC-012 answers *is this fragment
defined* and NAV-011 answers *is the order right*, so such a link vanished from
the ordering question altogether. epubcheck skips only the *document-order*
half for an unresolvable fragment; the spine-order comparison has already run.

On the book he sent, 67 of the links have dangling fragments: epubcheck reported
71 NAV-011 and we reported 5. Both tools now report 71, and the 67 RSC-012 were
identical throughout — his report said exactly that ("the number of errors
matches"), which is what narrowed the search to one rule.

Two more things came with it:

- **Every finding now names the offending link and points at it.** All five used
  to be anchored on the `<nav>` element with no target named, so the output was
  five identical lines and an editor would mark one line five times.
- **The comparison is now a two-level state machine**, matching epubcheck: the
  document-order baseline resets when the spine advances, and a link whose
  fragment did not resolve leaves it untouched. Scanning adjacent pairs got this
  wrong whenever an unresolvable link sat between two resolvable ones — neither
  pair compared, so the two real positions were never checked against each
  other.

This is the fourth time a check has suppressed a case because it believed
another check owned it, and the fourth time the other check owned a *different
question*.

**New `rule` key**: these findings now carry `navdoc.toc.link_out_of_reading_order`,
where the site was previously unkeyed. Nothing consumes NAV-011 today, but a
downstream list of handled rules written against the old world will not contain
it.

The shelf speaks to one direction only, and it is the one that matters for a
change that adds findings: none of its 385 books gained a NAV-011 (none reported
one before either). The corpus is unchanged.

**A `file:` URL inside `@font-face` drew nothing.** The generic `url()` pass
deliberately skips `@font-face` blocks and hands them to the font-face checker,
which asked about the declaration, an empty block and an empty `url()` — but
never about the scheme. Every question the generic pass asks has to be asked
again there or it is asked about nothing, and this one was not: on epubcheck's
own `file-url-in-css-error` fixture it reports two file-URL errors and we
reported the manifest one alone. The predicate is now shared between the two
sites so they cannot drift apart again.

**An `<object>` pointing at a foreign resource with no fallback drew nothing.**
The elements that can reference a foreign resource are enumerated by hand and
`<object>` was never added — the per-source shape once more. epubcheck reports
RSC-032 on its own `foreign-xhtml-object-no-fallback-error` fixture; we reported
nothing at all.

**The fallback is the element's own content**, which is the part that makes this
dangerous to implement carelessly: an `<object>` with real content owes nothing,
and reporting one would be a false positive on the ordinary way to author the
element. epubcheck's fixture turns on the sharper half of that rule — the object
*has* a `<p>` child and the `<p>` is `hidden`, so the fallback is not really
there. An implementation that only asked "does it have child content" would call
that book clean, so the `hidden` rule is not optional.

The shelf cannot speak to this: **none of its 385 books contains an `<object>`
at all.** The fixture and the test are the evidence.

**A `<source>` inside `<audio>`/`<video>` was never asked whether its declared
type matches the manifest.** The `<source>` case required a `<picture>`
ancestor and read `srcset`, so a media source's `src`/`type` pair was covered by
nothing. epubcheck's `type-mismatch-in-audio-warning` fixture is exactly that
shape and drew nothing from us.

**The comparison is now normalized on both sides, and the two normalizations
differ** — this follows epubcheck rather than tidying it up:

- the content-side type loses its parameters, so `type="audio/mpeg; codecs=mp3"`
  against a manifest `audio/mpeg` is a match. Comparing whole strings would
  invent a warning on correct markup, which we were already carrying latently on
  `<object>` and `<embed>`;
- the manifest side keeps its parameters except for `audio/ogg; codecs=opus`,
  which is how an Opus file is legitimately declared while the content writes
  plain `audio/ogg`. epubcheck folds those two by hand and calls it a hack in
  its own source; matched anyway, because an Opus book must not draw a warning
  from one tool and not the other.

One shelf book carries a `<source type>` and is unchanged, so the new coverage
ran and stayed correctly silent. The corpus is unchanged.

## [0.9.28] - 2026-08-21

**The four EPUB 3.4 advisories move to a family of their own: `ADV-005`…`008`
are now `NEXT-005`…`008`.** Same checks, same severity, same flag — but the code
now says which of two very different claims it is making. `NEXT-*` means a
published specification requires it and epubcheck has not implemented it yet, so
it **becomes a real error once epubcheck catches up**. `ADV-*` means no
specification says anything and the book is still wrong; those never become
errors.

**Why the ID rather than a label in the message.** An integrator can route on
`code.startswith("NEXT-")` in any language. The alternative considered was a
bracketed tag in the human line (`USAGE ADV-005 [spec-ahead]: …`) and it was
rejected as the more dangerous option: it adds a second bracket group beside the
location bracket, and every plugin that parses our output — the Sigil one is
Python, the next may not be — would have to handle it, each in its own way. The
line shape is also shared with epubsana now that the formatter lives in the
library. A prefix breaks nothing and needs no parsing.

**Why the numbers were kept, so the family visibly starts at 005.** These four
shipped as `ADV-005`…`008` in 0.9.13/0.9.14 and were announced under those codes
in the CHANGELOG and on MobileRead. Renumbering to `NEXT-001`…`004` would have
severed every one of those references for a tidier first line. `NEXT-005` is
instantly recognisable as what the changelog called `ADV-005`. `NEXT-001`…`004`
will never exist.

**And why now.** epubsana never enables `--advisory`, so nothing dispatches on
these codes today; the Sigil plugin landed yesterday. This is the one window
where the rename costs a measured zero, and it narrows every week.

The distinction was "temporary" in principle, which is what argued against
giving it a name — but the measurement says otherwise. EPUB 3.3 took **2.4
years** from first Working Draft to Recommendation; 3.4 is still a Candidate
Recommendation Draft; epubcheck's own 3.4 milestone is 8 open / 0 closed with no
due date and no validation-code commit since 2025-09-02. "Temporary" here means
years.

`data.advisory_basis` stays and is now derived from the prefix rather than a
lookup table, so the JSON token cannot drift from the code a user sees. Three
tripwires guard the split: a constant's name must agree with its id, the two
families may never reuse a number (they share one sequence), and **a `NEXT-*`
check must cite the specification it is ahead of while an `ADV-*` must not** —
that last one is the only machine check on the thing that can still go wrong,
filing a spec-mandated rule under the wrong prefix. Each was verified by
breaking it deliberately.

**The advisory family's two kinds are machine-readable.** An `ADV-*` finding in
the JSON envelope now carries `data.advisory_basis`, either `spec-ahead` (a
published specification requires it and epubcheck has not implemented it yet —
**temporary by design**, retiring when epubcheck ships the rule) or
`spec-silent` (nothing forbids it, but the book is still wrong — ours
permanently). Today that is four and five.

It matters most to an editor integration, which is no longer hypothetical: "this
becomes an error when epubcheck catches up" and "nobody says this is wrong, but
you may want to look" are different things to put in front of a user, and until
now the difference existed only in `ids.rs` comments.

**In `data` rather than on the shared `Item`**: the envelope is a family-wide
contract and this is a fact about *our* advisory family, which no other tool
has. Two tripwires guard the classification, because a hand-written `match` with
a `_ => None` arm fails safe but *silently* — an unclassified advisory would
simply omit the key, indistinguishable from a non-advisory finding. One asserts
every `ADV-*` constant is classified; the other asserts nothing classified has
stopped existing. Both read the constants out of `ids.rs` rather than listing
them, so they cannot drift from the family they guard, and the first was checked
by adding a fake `ADV-010` and watching it name the id and say what to do.

**The README explains `--advisory`, and the commitment underneath it is now
written down rather than merely honoured.** `ADV-*` reports real defects
epubcheck has no opinion about — either because no specification forbids them,
or because one does and epubcheck has not caught up. The new section says what
that is for, and states the line plainly: **it is opt-in and never changes the
verdict or the exit code.** A book that passes epubcheck passes epubveri, with
or without the flag. That is permanent, not a current limitation — a validator
that quietly rejected books the industry standard accepts would be worse than
useless to anyone whose distributor gates on epubcheck, and holding that line
is exactly what lets the flag be as opinionated as the evidence supports.

It also names the two kinds that live there, because they are not the same
claim: **spec-ahead** (the specification says it, epubcheck has not shipped it
— these stop being advisory when it does) and **spec-silent** (nothing says
anything, but the book is still wrong — these never graduate). Today that is
four and five.

**And the bar for admitting one is no longer a precision rate** (owner's
correction). A rate measured on our shelf is a fact about that shelf — 385
Turkish trade titles, Calibre output and Project Gutenberg — not about the
check, so 10% and 100% should not decide the question. The corpus-independent
test, which was in the record all along and which the rate obscured: **is the
finding true every time it fires**, and is it worded as an observation rather
than a verdict. "These two navigation entries resolve to the same document" is
always true; "one of them is a mistake" sometimes is not, which is why the
message does not say it.

Our own history is the evidence: ADV-009 was first *rejected* on its rate, and
shipped only once a structural fact settled it — `<content>` is mandatory in
`navPoint`, so a section heading has nowhere to point but its first child's
document. That holds in every library, not just this one. The statistic misled;
the structure decided. `CLAUDE.md` now records all three wrong turns that
candidate took, including two of its own numbers that were wrong.

## [0.9.27] - 2026-08-21

**`Message::render_human()` is now in the library, and the CLI calls it instead
of formatting its own line.** epubsana asked for the *per-message* form as the
primitive rather than a whole-report call, and the reasoning generalises: its
output groups findings by rule, and by message shape inside `schema_violation`,
because a flat 3,113-line dump is the experience it exists to improve. A
consumer with only a whole-report call would reimplement the line and drift from
it the first time either side touched a severity word or the location brackets —
silently, with nothing failing. `Report::render_summary()` (the verdict line)
and `Report::render_human()` (findings plus verdict) are built on top.

**Findings now come out in manifest document order, and the same book validated
twice gives byte-identical output. It did not before.** `content_docs` and
`css_items` were built from `items.values()`, and `items` is a `HashMap` —
randomly seeded, so the visit order differed on every run. That order decides
which file's findings arrive first, and `Report::sort_by_document_order` derives
the report's whole file ordering from exactly that. The effect on real books:
**94 of 385 printed their findings in a different order on each run of the same
binary.** Same findings, same byte count, shuffled. Fixing the content documents
took it to 5, all stylesheets; fixing those took it to 0, verified over three
full-shelf runs.

**Nothing here could have found it.** The corpus, the shelf, `compare` and every
existing test compare ID sets or counts, which are order-insensitive by
construction — and `sort_by_document_order`'s own doc comment asserted the
property that was false ("files keep their existing first-seen order — the
spine/processing order the validator already emits them in"). It surfaced from
byte-comparing two runs while verifying the render refactor above, and only
because the control — the *same* binary twice — failed as well, which is what
cleared the refactor and convicted the sort.

**RSC-031 asks whether a remote URL is `https`, not whether it is `http://`.**
epubcheck's condition (`ResourceReferencesChecker`:382-388) warns for any remote
reference in an EPUB 3 whose scheme is neither `https` nor `file`; ours matched
`starts_with("http://")`, so a Calibre/Kobo `url(res:///system/fonts/HelveticaNeue.ttf)`
— the exact shape that made 0.9.20 widen `is_remote_url` — drew the warning
there and nothing here. Both emission sites now share one predicate.

Measured one book per scheme against 5.3.0 at both sites, nine probes: `https`
and a case-shifted `HTTPS` stay silent in both tools, while `http:`, `res:` and
`ftp:` now agree exactly. No overshoot anywhere, which is what makes a
restrictive change safe. epubcheck's two reference-type exemptions (`LINK`,
`HYPERLINK`) need no condition here: hyperlink targets live in a separate set,
and the package document's `<link>` elements reach neither site.

**No instrument here could have found it, and the shelf cannot confirm the
fix**: RSC-031 fires on 0 of 375 shelf books before and after, because none has
a remote URL in CSS — the emission site's own comment already said as much. The
evidence is the nine probes and two new tests, one of which was checked by
restoring the old condition and watching it fail on the `res:` case. It surfaced
while settling an unrelated question for epubsana, not from any measurement of
ours.

**OPF-014's stylesheet site is correctly ungated, and now says so.** It carries
no `is_epub3` while its RSC-031 neighbour does, which reads like an oversight;
epubsana reported a downstream regression and asked. Measured the same day, one
book per version, changing nothing but `version`: epubcheck reports OPF-014 for
both EPUB 2 and EPUB 3. No code change — a comment, so the next reader does not
re-derive it or "fix" it.

**The README now says why there are two tools.** `epubveri` finds, `epubsana`
repairs the defects with exactly one correct answer, and an editor is where the
judgement calls belong — with the part that was never written down anywhere
public: **we deliberately do not write an editor.** Sigil and calibre already
exist and their authors have spent years on them; a validator that competed with
those tools could not be integrated *by* them, and being integrated is worth far
more to someone holding a broken book. The aim was never a hundred percent, and
saying where the hand-work starts is a service rather than an excuse.

epubsana was mentioned exactly once in the whole README before this, in the
licensing section, as "the sibling project" — with nothing about what it does.
The division of labour has been the operative design since the first day of the
project and had no public statement at all.

**One clause of that section was wrong, and epubsana caught it before any of
this shipped.** The draft said epubsana "can be told to confirm every step". It
is the other way round: confirming every fix is the **default**, applying the
provably-safe tier unattended is what you opt into, and — a detail neither
project had written down — when it cannot prompt, it stops rather than guessing.
Read out of their source rather than assumed, which is how the sentence should
have been written in the first place.

Nobody saw the wrong version: it was corrected between commits, and this release
is the first time any of the section is public. Recorded anyway, because
"especially conservative about repair, never mutate silently" is the oldest line
in that project's brief, and describing a guarantee as a feature flag would have
led a plugin author to wire up an unattended run believing that was normal. Two
confident sentences about another tool's behaviour were wrong here in two days;
both repositories are on the same disk, and reading beats assuming.

**A stale FAQ answer is fixed, and it had been contradicting the same
document.** *"Does it support WebAssembly (WASM) yet? Not yet — it's on the
roadmap"* sat 76 lines below a full "Use it in the browser (WASM)" section with
install instructions. WASM shipped in 0.1.0 and the npm package has tracked the
crate version ever since, so the front page had been wrong for about seven
weeks — and newly consequential, with a Sigil plugin now sending people to it.
A second question was added beside it: *will epubveri fix my book?* No, it never
writes to a book at all.

**The "drop-in replacement" answer no longer undersells, and the real-book
comparison is now in the README instead of only being alluded to.** The answer
still says epubveri is not a drop-in replacement — but the reason it gave was a
technical caveat that expired: "further along on structural/packaging
correctness than on some of the deeper content-model checks", written before the
XHTML, SVG and MathML grammars and the element-by-element EPUB 2 content-model
audit all landed. The honest remaining difference is **authority, not
capability**: if something downstream says "must pass epubcheck", passing
epubveri is not the same sentence, and no amount of agreement changes that. The
two absent checks are named as the scope decisions they are.

The status section promised a real-book measurement and never gave one — it
said the corpus "says nothing about how the two tools compare on a real book,
which is measured separately", and stopped there. It now carries the number:
375 books, 373 agreeing on the message-ID set exactly, **no ID epubveri reports
that epubcheck does not**, and epubveri always the lower of the two where counts
differ. Both books that differ are accounted for in one sentence each.

**The README now carries a measured speed comparison, and it corrects the claim
this project has been making about *why* epubveri is faster.** Timed over the
same 20 books on an idle machine, one invocation per book — the shape an editor
plugin or an ingestion pipeline actually uses:

| | per book | the 385-book shelf |
|---|---|---|
| epubcheck 5.3.0 | 2013 ms | ~13 min |
| epubveri | **191 ms** | **~70 s** |

About ten times faster, reaching the same verdict. **The reason is not JVM
startup, which is the thing both this README and `CLAUDE.md` had been implying.**
epubcheck's launch is 70 ms of that 2013 — a little over 3% — and the rest is
the validation work. Our own process startup is 6 ms, from timing 385 books in
one process (68.9 s) against one process per book (71.3 s). So the honest claim
is "the same work in about a tenth of the time", which is stronger than the one
that was being made and happens to be true.

**The real-book comparison is re-measured on the current shelf: 385 books
(2026-08-21), 383 agreeing on the message-ID set exactly**, no ID epubveri
reports that epubcheck does not, and epubveri lower in all 152 rows where the
two report the same ID a different number of times. This was also the first full
run since the RSC-031 widening, which turns out to move nothing on real books —
the ID does not appear anywhere in the diff.

Three stale claims went with it: two "eventually"s about WebAssembly, which has
shipped on npm since `0.1.0`, and the bullet that sold the CLI on avoiding "the
JVM startup cost".



## [0.9.26] - 2026-08-20

**An unencoded space in an NCX `<content src>` is now reported (RSC-020).** A
Calibre book whose files are named `Kamelyali Kadin_split_000.html` drew 32
findings from us and 60 from epubcheck: both tools reported one per manifest
item, and only epubcheck reported the 28 `<navPoint>`s naming the same files.
We now agree on all 60, and on the line of every one.

The cause is structural rather than a missing case. **URL validity is organised
per _source_ here and per _reference_ in epubcheck** — it registers every
reference and runs them through one path, while ours grew a site at a time
(manifest href, then content-document references, then SVG `href`/`xlink:href`)
and the NCX was never added to that list. This is the same shape that left the
`<guide>` out of fragment resolution before 0.9.15, and it is worth stating as a
standing hazard: a per-source design owes a re-enumeration every time a new
reference kind appears, and nothing fails loudly when one is forgotten.

Found by `compare`'s count-gap section, not by its two ID lists — the ID sets
agreed exactly, because we did report RSC-020 on the book, just 28 times too
few. Three of 375 shelf books carried the shape and all three now match
epubcheck; the corpus was byte-identical before and after, and has no fixture
for it.

The finding anchors to the `src` attribute (`…/ncx:content[1]/@src`), matching
what the manifest site does with `@href`. For consumers keyed on `rule`, the
new site is `opf.ncx.content_src_unencoded_space` — a new key, so an allowlist
written against the previous release will not contain it.


**ADV-009 notes two navigation entries that land in the same place.** JSWolf
reported on MobileRead (#195) two `<navPoint>`s sharing a `playOrder` *and* a
`content src`, expecting an error. Both tools are silent and both are right —
epubcheck's `ncx_playOrderMatch` **obliges** two entries pointing at one target
to share a `playOrder`, so "fixing" the duplicate number would make a valid NCX
invalid. The real defect is one level up, where no validator looks: two table-of
contents entries resolving to one document, so one of them names nothing the
reader can reach. Behind `--advisory`, `Usage`, and it never touches the verdict.

**Only *sibling* entries are reported, and that restriction is structural rather
than a tuning.** `<content>` is mandatory inside `navPoint`, so a purely
structural parent — a part heading, an omnibus volume title — has nowhere to
point but its first child's document. That duplicate is the only legal way to
write such a heading, and reporting it would be reporting the format.

Measured across the shelf's 364 NCX files: 12 duplicate targets in 6 books, of
which **8 are parent/child and every one is legitimate** (7 of them in a single
translation of *Der Zauberberg*, whose seven part headings each share a file
with their first chapter). Of the 4 sibling pairs, 3 are genuine — an unreachable
chapter XXXIX, a second author's biography, and a spacecraft diagram — and 1 is
Calibre listing a title page twice. **One false alarm in 375 books**, against the
1-in-16.8 of the ADV-003 version that was rejected for crying wolf.

The message is worded as an observation, not a verdict: the finding is factually
true in all 12 cases — two entries really do resolve to one document — and only
the inference "that is probably a mistake" is sometimes wrong.

**RSC-020's remaining sites are now measured rather than estimated, and the
answer is that they stay empty.** This check is organised per *source* here and
per *reference* in epubcheck, so the useful question is which of our sites it
has joined — RSC-012 runs at all four reference sites (guide, NCX, content
document, media overlay), RSC-020 at three. The five it does not reach — the
guide, the EPUB 3 navigation document, media overlays, CSS `url()` and the
dictionary's search-key-group href — were each scanned across 375 real books
for an interior space, and **the population is zero in every one**. The NCX was
the only remaining site with real books behind it, which is why it was the one
worth closing. `docs/COVERAGE.md` now carries the matrix and the measurement,
so the next reader inherits it instead of re-deriving it.

**A tripwire test now guards `--advisory`'s help text.** The paragraph
enumerates what the flag emits, in prose, and nothing linked that prose to
`ids.rs`. It went stale once already — through 0.9.13/0.9.14 it still described
only the CSS lint from 0.9.0, while the flag had gained four more checks — and
the failure is invisible by construction: a wrong help text breaks nothing, and
no instrument here reads it. Adding an `ADV-*` now fails a test that names the
new id and says to describe it.

**The CSS-008 divergence now says that it changes the verdict.** A stray
semicolon in a declaration block (`a:link {;color:#000}`) is valid CSS — both
*Consume a style block's contents* and *Consume a list of declarations* in CSS
Syntax 3 answer a `<semicolon-token>` with "Do nothing", and CSS 2.1's core
grammar makes every declaration slot optional. epubcheck's older parser reports
it as an ERROR; we stay silent, by a decision already recorded here on
2026-08-17.

What the note did not say is the part a reader most needs: **that ERROR flips
the verdict.** On the one shelf book that contains an empty declaration,
epubcheck reports INVALID and epubveri reports VALID, with no other difference
between the two reports — the only verdict disagreement across all 375 books.
The RSC-016 divergence has always carried the opposite sentence ("the verdict
never differs"), so leaving it unsaid here read as though it did not.

Also re-measured while the numbers were in hand: the empty-declaration
population is 1 book of 375 (was 1 of 346), and the CSS-028 granularity
difference is visible on 118 of 375 (was 32 of 136), with the 4x ratio holding
in 104 of them and 2x/3x in the rest — which is the per-descriptor mechanism
showing through, not an exception to it.

## [0.9.25] - 2026-08-19

**The NCX is validated against a grammar**
([#83](https://github.com/veripublica/epubveri/issues/83)). Doitsu reported on
MobileRead that an empty `<pageList>` and an empty `<navMap>` draw an error
from epubcheck and nothing from us. Both did — and so did fourteen other
shapes, because the NCX's structure was checked in exactly one place (the
`navPoint` content model added in 0.9.24) and the format's other ~26
constraints were not checked at all.

`schemas/ncx.rng` is new — authored from scratch, like the package and XHTML
grammars — and the NCX now goes through the same RELAX NG engine as they do.
Sixteen shapes were measured one book each against epubcheck 5.3.0 and now
agree with it on both the message ID and the number of findings: the empty
containers (`navMap`, `pageList`, `navList`, `navLabel`), a `navTarget` or
`pageTarget` with no `content`, a missing `navMap` or `<head>` `meta`, a
`pageList` nested inside the `navMap` or placed before it, an element or
attribute the format does not define, a `navPoint` with no `id`, a `content`
with no `src`, and markup inside `<text>`.

Two checks rather than a grammar would have closed nine of the format's ~27
constraints and left the rest to arrive one forum report at a time.

**Every "incomplete content" message now says what is missing.** The engine
reported `element "html" has incomplete content` where epubcheck says `element
"html" incomplete; missing required element "body"` — the same divergence on
every XHTML document, not only in the NCX. It now names the element the model
demands next, or lists the alternatives when the model offers a choice, which
is epubcheck's own pair of message forms. On the real-book shelf **6,244 of
6,247 such findings** now name something; before, 32 did. The remaining three
are an empty `<guide>`, whose model admits any element and so has no name to
give.

The text is *appended*, so `has incomplete content` remains a prefix of it and
downstream consumers matching on that phrase are unaffected.

**A RELAX NG loader fix found by the new grammar**: an `ns` written on an
`<attribute>` element was dropped, so `<attribute name="lang"
ns="…/XML/1998/namespace"/>` meant a no-namespace `lang` and a perfectly
ordinary `xml:lang` was rejected. RELAX NG blocks only the *inherited*
namespace for attribute names; one written in place is honoured.

Three constraints in epubcheck's own NCX grammar are deliberately **not**
reproduced, each measured and each in the looser direction: it demands `id` and
`class` together on `text`, `img` and `pageList` (so `<pageList id="x">` alone
draws `missing required attribute "class"` there); it allows at most one
`navLabel` in a `pageList` and fixes the order against the one the format
itself defines. Reproducing those would mean inventing errors on valid books.

**Two false positives we shared with epubcheck, both found by reading its issue
tracker rather than by any instrument here.**

`a11y:contactEmail` is valid accessibility metadata and drew OPF-027
(w3c/epubcheck [#1669](https://github.com/w3c/epubcheck/issues/1669)). EPUB
Accessibility 1.2 added the property on **2025-09-04**, three days after
epubcheck 5.3.0 shipped, which is why that release cannot know it; our own
vocabulary had simply been copied from 1.1.

A resource referenced from a `<video>` — its own `src` or a child `<source>` —
no longer needs a manifest fallback unless it is audio
([#1662](https://github.com/w3c/epubcheck/issues/1662), opened by the EPUB spec
editor). EPUB 3.3 §3.4 exempts *"All video codecs referenced from the HTML
video, including any child source elements"*, unconditionally and by position
rather than by media type; both tools instead tested for a `video/` prefix, so
an HTTP-live-streaming playlist (`application/x-mpegurl`) — which plays
straight from the element and can carry no fallback — was reported as a foreign
resource with none. The type-based exemption stays beside the new positional
one, so nothing that validated before becomes an error, and audio stays
restrictive as the same section requires: a foreign `audio/*` inside a
`<video>` is still reported, and epubcheck agrees on both negatives.

Both are permissive — we accept what we used to reject — which is why they ship
in the default output while restrictive divergences wait behind `--advisory`.
Being wrong permissively costs a false negative nobody can see; being wrong
restrictively puts an invented error in front of every user comparing the two
tools.

**References that precede a parse failure are recovered**
([#73](https://github.com/veripublica/epubveri/issues/73)). A content document
that is not well-formed lost every check below it, so a book with a missing
stylesheet *and* a stray `&` reported only the entity — the user fixed that,
re-ran, and only then met the other two problems. epubcheck's parser is
streaming: it keeps whatever it passed before the failure. This recovers the
same set.

Eight malformation kinds were measured against epubcheck 5.3.0, one book each
(undeclared entity, malformed numeric reference, unclosed element, mismatched
tag, unquoted attribute, stray `<`, duplicate attribute, unknown namespace
prefix). **All eight behave identically in both tools**, before and after — so
this is one rule, not a family, and the classification is what made the design
decidable.

A reference *after* the failure stays lost, because epubcheck loses it too
(measured, not assumed). Claiming more would be a divergence in the direction
that reads as invention.

**The issue's own argument against this fix was backwards**, which is the part
worth carrying: it rejected a text scan because comment and CDATA contents
would be mistaken for references, "paid on every book". The scan runs only in
the `Err` arm — a document that parses is walked as a DOM — so the surface is
confined to books that are already FATAL and INVALID. And `scan_references`
skips comments, CDATA, processing instructions and a DOCTYPE internal subset
by construction, with a `>` inside a quoted value not ending a tag. Those are
sixty lines that exist entirely to not invent references.

One test earned its keep immediately: the comment case passed even with the
comment branch deleted, because `<!--` fell through to the DOCTYPE branch and
skipped to the first `>`, which happened to sit past the decoy. A comment
containing its own `>` discriminates, and only that version fails when the
branch is removed.

**Two CLI messages tell the truth again.** `--advisory`'s help text still
described the flag as emitting "unknown CSS property/descriptor names" — the
whole of it in 0.9.0, and stale from ADV-003 onward. It has since grown a CSS
type-selector lint, the EPUB-2-package-written-in-EPUB-3 advisory, and the four
restrictive EPUB 3.4 rules that shipped in 0.9.13/0.9.14, none of which a
reader of `--help` could learn existed. And passing a *directory* now says so
in words: epubcheck validates an unpacked EPUB with `-mode exp`, so someone
porting an invocation reasonably tries it here, and met `Is a directory` from
the operating system. epubveri takes the packaged `.epub` file, by decision,
and now says that at the door.

## [0.9.24] - 2026-08-19

**A `navPoint` must carry a `navLabel` before its `content`, and a `content`
at all** ([#79](https://github.com/veripublica/epubveri/issues/79)). Nothing
here validated the NCX's *structure*: `ncx.rs` checked playOrder, id
uniqueness, duplicate `navLabel`/`navInfo` and empty text, so
`<navPoint id="x"><content src="…"/></navPoint>` — no label at all — was
accepted and the book came back VALID while epubcheck reported an error.

Reported on MobileRead with epubcheck's output and a 2.4 KB test book, which
is the artefact that makes a report like this cheap to act on.

Three shapes, and the counts are the parity: a `content` before any
`navLabel` is one finding; `<content/>` then `<navLabel/>` is **two** (the
order violation and the missing-label violation are separate there); a
`navPoint` with no `content` is one. Measured one book per shape against
epubcheck 5.3.0, and the grammar read is `schema/20/rng/ncx.rng` — the file
`XMLValidators` actually loads, not the `ncx-old.rng` beside it, whose
`playOrder` is required and would have invented a finding.

Restrictive, so the direction was checked first: corpus 0 false positives and
0 over-reported, and the 356-book shelf — 266 of them EPUB 2 with an NCX — is
byte-identical, so no real book on it has a malformed `navPoint`.

**A `nav` requires an `ol`, and a flat nav must contain one.** The other half
of the same report. `check_nav_content_model` did

```rust
let Some(ol) = children.get(idx) else { return };
```

so a `<nav epub:type="toc"><h1>…</h1></nav>` reported nothing, while the arm
directly below it reported a child that was *present* and wrong. Only the
absent case escaped — the silent-skip shape again (CHANGELOG 0.7.12–0.7.14),
and the fourth instance of it recorded here.

The RSC-017 half is narrower than it first looked. `epub-nav-30.sch`'s
`flat-nav` asserts `count(.//ol) = 1` on a `page-list` or `landmarks` nav, so
zero fails it as well as two — but the *two* end already had an owner here
(`navdoc.nav.nested_sublist_not_allowed`, per nested sublist). A first version
reported both and double-counted a nested landmarks nav; the new rule now
covers `ols == 0` only, and the test asserts counts rather than presence,
which is what caught it.

The reported book now gives 3 RSC-005 and 1 RSC-017 — the same set epubcheck
reports, on the same elements.

**A missing fragment in a `text/html` target is RSC-014, not RSC-012**
([#82](https://github.com/veripublica/epubveri/issues/82)). epubcheck guards
RSC-012 on the target being XHTML or SVG; a `text/html` document is neither,
so the missing id falls through to its reference-type switch and comes out
under the other id. Two sites were wrong in different ways: the NCX and
`<guide>` walks **skipped** such targets entirely, and the content-document
walk resolved them and reported RSC-012.

Found by `compare` on the 356-book shelf — one book whose NCX pointed into a
`text/html` chapter, where epubcheck reported an RSC-014 we did not. That book
now matches exactly (3 RSC-012 + 1 RSC-014) and is the only one that moves.

The NCX site carried a comment saying a dangling fragment into such a document
"draws nothing there". That was measured for RSC-012 and true; the conclusion
was wrong, because nobody had looked for the other id. Same family as 0.9.18's
`text/html` work, and the third site in it.

Not version-dependent, checked at both 2.0 and 3.0. A `text/html` file linking
to *itself* draws neither id from either tool — measured after a first version
of the test asserted otherwise and failed.

**A broken selector list is one CSS-008, not one per selector**
([#81](https://github.com/veripublica/epubveri/issues/81)). `. a, . b, . c { … }`
was three findings here and is one in epubcheck, whose unit is the whole
prelude. Two separately broken rules are still two, in both tools.

Found by `compare` over the 356-book shelf — specifically its **count-gap**
section, which the ID-set diff cannot see: one book reported 22 CSS-008
against epubcheck's 12, from `. h-100, . y-100 { … }` repeated down a
stylesheet. It now reports 12.

**The library keeps its answer and the consumer adapts.** styloria reports one
error per comma-separated selector, settled deliberately in its
[#3](https://github.com/veripublica/styloria/issues/3), and that is the right
granularity for a CSS library; epubcheck parity is this project's concern, not
its. So the collapsing happens here, on both emission paths — the top level
and inside a grouping at-rule, since a half-applied fix is the shape that
keeps recurring.

An existing test asserted the old count with the comment "both halves of a
comma list, which is what the real book carried". True, and never checked
against epubcheck. It now asserts one, with the measurement written next to
it: **an assertion is not a constraint on a change until the oracle has seen
it.**

**A corrupted-but-nonempty container is now named**
([#80](https://github.com/veripublica/epubveri/issues/80)). epubcheck's
`OCFZipChecker` reads a **58-byte** header: a file too short to fill it is
PKG-003, one long enough but not starting with `PK` is PKG-004. We had
PKG-003 for a literally empty file and PKG-004 behind an image sniff, so 36
bytes of text and 200 random bytes both drew the generic PKG-008 alone.

58 and not 30 — the check reaches past the local file header to the
`mimetype` name at offset 30. A comment in `ocf.rs` said 30, and that
misreading is what made the two tools look inconsistent when they agreed; the
boundary is now pinned from both sides (57 bytes is PKG-003, 58 is PKG-004).
Both messages are FATAL either way, so no verdict moves — this is the tool
saying *which* way the file is broken.

`docs/COVERAGE.md` moves PKG-003 and PKG-004 to complete and **PKG-006 to
partial**, which is the same audit finding one more thing than it went looking
for: epubcheck computes PKG-005/PKG-006 from that raw header too, so they
still fire on a container that fails to open, while ours read the parsed zip
and cannot. Measured (`PX…`/`xK…` headers are PKG-006 there, PKG-008 alone
here) and left as its own change.

**Two more nav rules were implemented at one end only** — found by diffing
`epub-nav-30.sch`'s eight patterns against `navdoc.rs` rather than assuming
the list was the gap, since six of the eight turned out to be covered.

- `nav-ocurrence` asserts `count(toc) = 1`, which fails at both ends. We had
  the zero end (`the nav document has no "toc" nav`) and not the other, so a
  document with **two** `toc` navs was accepted.
- `heading-content`'s context is every `h1`–`h6` in the navigation document.
  Ours ran inside the nav content model, on the heading a nav opens with, so
  `<h2>  </h2>` sitting outside any nav was silent. It now runs once over the
  document, which also removes the risk of the old site double-reporting.

**The shelf is thin evidence for the second one, and says so**: 66 nav
documents on it carry **8 headings between them**, so the widened check ran on
eight real headings. That is why the test pins an image-only heading
(`<h2><img alt="Part One"/></h2>`) as valid — `has_text_or_image` is what
keeps the rule off legitimate markup, and the shelf could not have caught it
if it were wrong.

**Scope, measured before implementing:** the nav grammar applies to the
navigation document only. A `<nav><h1>Sidebar</h1></nav>` in an ordinary
content document is clean in epubcheck 5.3.0, so a document-wide rule would
have invented errors on every sidebar nav in the wild.

Unlike most recent work the shelf is not blind here — every EPUB 3 book has a
navigation document — and it is byte-identical across the change. That is a
real pass rather than an unexercised path: 21 shelf books carry a
`landmarks`/`page-list` nav and every one of them has exactly one `ol`, so
the new rule ran on 21 real navs and correctly said nothing.

## [0.9.23] - 2026-08-18

**An SVG `<a xlink:href>` to a missing file now draws RSC-007**
([#77](https://github.com/veripublica/epubveri/issues/77)), closing the last
half of the SVG-anchor inversion 0.9.22 fixed the rest of. All eight anchor
shapes now agree with epubcheck 5.3.0.

The fix was smaller than the issue predicted. The existence check lives in the
bare-name attribute walk, which cannot see a namespaced attribute; the issue
assumed reaching it meant adding the target to the resource set that answers
OPF-097's "is this resource referenced", where a hyperlink does not belong.
It does not — `is_resource_reference` already draws that line and puts an
`a`/`href` pair on the not-consuming side, so feeding the namespaced value
through the same loop gives the check without the side effect. Verified
directly: a manifest item reachable only through an SVG anchor still draws
OPF-097 from both tools.

**An ordinary hyperlink to a non-Content-Document resource is now RSC-010**
([#78](https://github.com/veripublica/epubveri/issues/78)) — `<a
href="styles.css">` and its SVG `xlink:href` equivalent alike. We had the
check on the two toc paths only (the NCX `<content src>` and the nav toc
link), so every other hyperlink was silent; epubcheck runs it for every
hyperlink reference.

It is reported **instead of** RSC-011, never alongside — epubcheck aborts the
reference's remaining checks right after, and our spine-reachability loop was
already skipping those targets with a comment saying so, which is how the
hole had a shape waiting for it.

`docs/COVERAGE.md` moves this row from `Y | Y` to **partial**, which is the
honest direction even though the check grew: the row claimed complete while
its own note described a toc-only implementation, and one cell is still
missing — a media overlay's `<text src>` pointing at a non-blessed type
(`ResourceReferencesChecker`:257). Same overstatement RSC-014 carried before
0.9.22.

The 356-book shelf is byte-identical across this change, so no book on it
hyperlinks a non-Content-Document resource; the evidence is fixtures against
epubcheck 5.3.0.

**The fourth cell is a documented divergence rather than a gap, and we keep
our answer.** For a media overlay's `<text src>` pointing at a non-blessed
target we report **MED-013**; epubcheck reports **RSC-010**. Each reports
exactly one message and both call the book INVALID, so no decision differs.

What settled it was the control: with a *valid* content document as the
overlay's target, both tools report MED-013 **and** MED-010 and agree exactly.
So epubcheck's MED-013 works normally, and its silence in the non-blessed case
comes from the `CheckAbortException` its RSC-010 throws — which drops a
second, unrelated package-level defect (the content document declares
`media-overlay` and the overlay never references it). Matching would mean
reproducing a suppression rather than implementing a check, so we do not.
Filed upstream as [w3c/epubcheck#1679](https://github.com/w3c/epubcheck/issues/1679),
where the same class was accepted and fixed once before (their #221). Same
reasoning as the `&nbsp;` divergence recorded on RSC-016.

## [0.9.22] - 2026-08-18

**RSC-014 is a type-matching check, and it is now implemented for every
reference kind epubcheck compares.** epubcheck types every `id` from the
element carrying it — an SVG `symbol` is SVG_SYMBOL, `linearGradient`,
`radialGradient` and `pattern` are SVG_PAINT, `clipPath` is SVG_CLIP_PATH,
everything else is GENERIC — and then requires each reference's type to
match. We had exactly one cell of that: a *same-document* hyperlink to a
`symbol`. Now:

- a hyperlink to any of the five typed elements, **cross-document as well as
  same-document**;
- an SVG `<a xlink:href="#…">`, which was not read at all;
- `<use xlink:href="#…">`, which may reach a symbol or a generic id;
- `fill`/`stroke="url(#…)"`, which must reach a paint server exactly;
- `cite` on `blockquote`/`q`/`ins`/`del` — **EPUB 3 only**, since epubcheck
  collects it in `OPSHandler30` and the identical EPUB 2 book is clean;
- a media overlay's `<text src="doc.xhtml#…">`.

A fragment that resolves to *nothing* is RSC-012, not RSC-014 — the same
split epubcheck makes — which closed three silent gaps of its own: a dangling
`<use>`, a dangling paint reference and a dangling SVG `<a xlink:href>` all
drew nothing here before.

**Three of epubcheck's cells are dead, and matching that is deliberate.**
Reporting where it is silent is indistinguishable from a false positive to
anyone diffing the two tools, so: `clip-path="url(#…)"` is unchecked (nothing
there ever registers such a reference, and the `case` handling it is
unreachable), and its SVG handlers read `xlink:href` only — so SVG 2's plain
`<use href>` and plain `<a href>` register nothing. That last one we *were*
reporting; an `<a href="#sym">` inside an `<svg>` no longer draws RSC-014.

`marker`, `mask` and `filter` are not on epubcheck's typed list and stay
clean, checked rather than reasoned from the SVG spec's own idea of a
definition element. `docs/COVERAGE.md` moves from a bare `Y | Y` — which
overstated a one-cell implementation — through `~` and back to `Y | Y`,
now with the matrix written out.

Twenty shapes measured, one book per run, against epubcheck 5.3.0. **No
instrument here can see this family**: 0 of 356 shelf books define an SVG
symbol, gradient, pattern or clipPath, and the corpus has no fixture either,
so the enumeration and the new unit tests are the whole evidence. The overlay cell got the
crate's first media-overlay test builder, so it is pinned by a test rather
than by a probe alone.

**An SVG `<a>` is now read through `xlink:href`, and only that.** The walk
matched on element name and always read the plain `href`, so every check in
it was *inverted* against epubcheck for SVG anchors — not only RSC-014.
Measured one book per spelling: a `xlink:href` to a missing file draws
RSC-007 from epubcheck and drew nothing here; the same target as a plain
`href` drew RSC-007 here and nothing there; RSC-020 and the fragment checks
held the same pair. Fixed in both directions except one, which is a false
*negative* and deliberately left for its own change: an SVG
`<a xlink:href="missing.xhtml">` still draws no RSC-007 here, because
reaching it means adding a hyperlink target to the resource-reference set
that also answers OPF-097's "is this resource referenced" question — and a
hyperlink is explicitly not one of those.

Internally, the per-document `id` map now carries each id's kind alongside
the document-order index it already held. A first attempt dropped that index
as unused; two callers do read it (the nav reading-order check and MED-015),
and both are named something the search for it never covered.

## [0.9.21] - 2026-08-18

**`@keyframes` is no longer a CSS syntax error.** A block holds either rules
or declarations, and which one a given at-rule holds was decided here, by a
list of the conditional-group at-rule names. That list had never heard of
`@keyframes`, so its block was read as declarations and `0% { opacity: 0 }`
came back as one malformed declaration — CSS-008, on valid CSS, on a
construct that appears in every animated fixed-layout book. epubcheck reports
nothing for it. `@-webkit-keyframes`, `@-moz-keyframes`, `@starting-style`
and any at-rule newer than the list (`@future { p { color: red } }`) failed
the same way: four shapes, one cause.

Adding the missing names would not have fixed it. `@keyframes` holds rules
whose preludes are `from`, `to` and `0%` — correct under CSS Animations 1 §3
and malformed under Selectors 4 — so routing it through the selector-checking
rule list trades one invented error for two. It needs a third reading, and
which reading an at-rule wants is a fact about CSS rather than about an EPUB
validator, so **the table moved to styloria** (`0.11`, its
[#4](https://github.com/veripublica/styloria/issues/4)) and
`parse_at_rule_block` answers it. An at-rule with no entry there is read as
declarations with a nested rule skipped in silence — the direction the
unknown case has to fail in, since CSS keeps gaining at-rules and no table
stays current.

What did not change: a broken declaration *inside* a keyframe is still
reported (epubcheck reports it too, and a test pins it, so "read it as rules"
cannot be satisfied by not looking inside), nested selectors inside `@media`
keep the check styloria 0.9 brought, and the `rule` slug on a malformed
declaration is still `css.declaration.malformed_shape`.

Measured: the four shapes and 21 neighbouring ones were built as books and
run through epubcheck — we now agree with it on all 25 except the one
divergence already recorded (a `@page` margin at-rule, which its older
parser rejects and current CSS allows) and the nesting shapes, where we
report one finding to its three. Corpus unchanged at 606/607 with 0 false positives,
the 346-book shelf byte-identical, `hostile` clean.

**A nested at-rule inside a style rule is now reported too** — the same
family, found while checking the first, and running the other way. We
reported `a { & b { color: blue } }` and stayed silent on
`.a { @media print { color: blue } }` and `@nest`, which was not a decision:
the declaration walk skipped at-rule chunks, because an at-rule's block may
legitimately hold at-rules (`@page`'s margin rules, `@font-feature-values`).
A style rule's block may not.

Reporting is the right side of that, against the first intuition — that
nesting is modern-but-valid CSS and belongs with the modern *selectors* we
deliberately decline to flag. It does not. EPUB 3.3 supports "CSS as defined
by the CSS Working Group Snapshot", and in the 2026 Snapshot (22 June 2026)
nesting sits in §2.4, *modules with rough interoperability*, explicitly
outside the official definition of CSS; CSS Nesting Level 1 is still a
Working Draft. The selector and empty-declaration precedents point the other
way precisely because *those* are in the official definition. epubcheck
reports all five nesting shapes, three findings to our one.

It carries its own `rule` slug, `css.declaration.nested_at_rule`, rather than
the malformed-shape one: this is not a parse error — CSS Syntax §5.4.2
consumes an at-rule in a declaration list quite happily — but a construct
outside the CSS this format accepts. Different reason, different key. An
at-rule inside an *at-rule's* block stays silent, with a test to keep it
that way.

**The shelf could not have found any of this, and did not**: no book of the
346 contains `@keyframes`, and none contains a nesting marker either. Neither could the corpus — it has no fixture
for one. The enumeration against epubcheck is the whole evidence, the same
way #48's table permutations were.

## [0.9.20] - 2026-08-17

Nine changes, eight of them gaps found by cross-checking the 346-book shelf against
epubcheck rather than by any test — and two of them run in the
false-positive direction, which the shelf could not have shown either.

**A content-document reference is now RSC-001, RSC-007, RSC-008 or silent,
by whether the target is declared and whether it is present.** `css.rs` has
applied that matrix to every `url()` for a long time, and its comment said
the split was "already established for XHTML content-doc references" — only
one of the three cells was. A *declared* target with a missing file drew a
second RSC-007 on top of the manifest pass's RSC-001, and an *undeclared*
target that is present drew nothing at all. SVG needed separate wiring: the
reference walk reads no-namespace attributes and SVG references through
`xlink:href`, so a broken image reference inside an `<svg>` produced no
finding whatsoever. The package document, `mimetype` and `META-INF/*` are
exempt, being structural resources that can never be manifest items.

**A remote resource reached through a linked stylesheet is reported once,
against the stylesheet.** One `@font-face` in one shared sheet produced 10
RSC-008 and 9 RSC-031 on a ten-document book, against epubcheck's single
finding, because every linking document adopted the sheet's URLs as its own.
Removing that also removed the RSC-031 the sheet legitimately earns, so the
https warning now lives on the manifest pass too — EPUB 3 only, since at 2.0
nothing may be remote and the scheme is beside the point.

**In EPUB 2 every remote reference is restricted (RSC-006), not undeclared
(RSC-008).** OPS 2.0.1 has no remote-resource concept at all: there is no
`remote-resources` property to declare and nothing may live outside the
container, so the manifest question never arises.

**`is_remote_url` is now epubcheck's predicate**: any scheme except `data:`
(and `file:`, which RSC-030 owns). It was `http`/`https` only, so
`res:///system/fonts/X.ttf` in a real book's `@font-face` fell between two
checks — external enough to skip local resolution, not remote enough to be
reported — and produced nothing from either. Hyperlink targets are
unaffected: `<a href>` and `@cite` are collected separately and never become
embedded dependencies.

**A percent escape in a host is judged by what it decodes to.** `%40` decodes
to `@`, which is forbidden in a domain — `http://ykykultur%40ykykultur.com.tr`
is an error epubcheck reports and we did not. Our host check allowed `%`
outright, on the reasoning that percent-encoded octets are legitimate; they
are, but only when what they decode to is: `http://ex%C3%BCample.com` decodes
to a real IDN label and stays clean. The decode happens *after* the userinfo
is stripped, never before, or the decoded `@` would let the userinfo strip eat
the host and hide the error.

**The CSS declaration walk moved into styloria** (`0.10`, its
[#4](https://github.com/veripublica/styloria/issues/4)). "Is this a
well-formed declaration" is a CSS question and was being answered here,
because `parse_rule_list` took component values and its declaration twin did
not exist — a caller holding a `{ … }` block has values, not source text. The
new `parse_declaration_list_from_values` closes that asymmetry, and the walk
here is gone.

What did *not* move: `direction`/`unicode-bidi` (CSS-001) and
`position: fixed` (CSS-006) are EPUB rules, not CSS ones — CSS has nothing
against either property — so they stay, now reading styloria's parsed
declarations instead of re-splitting the block. The `rule` slug is unchanged
at `css.declaration.malformed_shape`: the slug is epubveri's key for
consumers, and a crate boundary moving is not their business. Verified
behaviour-preserving — corpus, shelf and tests all identical before and
after.

**RSC-020 now covers an interior space and an empty host, in content
documents as well as the manifest.** It was scoped to the host of an absolute
URL, on the reasoning that the WHATWG parser normalizes a space in the path —
it does, but epubcheck parses every URL a second time through galimatias with
a strict handler that turns those recoverable warnings into errors. The
deferral recorded for this said to revisit it given "real books that actually
contain such URLs"; four do, carrying 32 between them, and all four now agree
exactly. A *trailing* space stays valid, which is a real user report
(patrik's) and still passes — two of the three assertions in the test that
protected it were our own stance rather than epubcheck's, and were corrected
against the oracle.

**A narrow `xsd:anyURI` check.** epubcheck types `href` and its relatives as
`xsd:anyURI` and Jing rejects `http://`, `%zz` and `:` with "value of
attribute … is invalid; must be a URI" — RSC-005, not RSC-020, which matters
because catching it under the wrong id would have read as a new false
positive. Only those three measured shapes are implemented; a space is
explicitly *not* one of them, which is what keeps a check spanning 39 schema
sites from inventing errors.

**OPF-072 counts an element's own text, not its descendants'.** Calibre
writes unescaped `<p>` markup into `dc:description`, and epubcheck calls such
an element empty; we read the text inside the children and stayed silent.

Corpus unchanged throughout: 606/607 exact-ID recall, 0 false positives on
355 should-be-clean cases, 0 over-reported on 255 fixtures — and it earned
its keep here, catching two regressions the moment the reference matrix
landed.

## [0.9.19] - 2026-08-17

Seven fixes, and what they have in common is how they were found rather than
what they touch: none was reachable by the instruments that run on every
release. Five came from cross-checking against epubcheck over 156 books newly
added to the real-book shelf, one from a stylesheet shape that shelf turned
up, and one from a reader on the MobileRead thread who sent epubcheck's output
beside ours. The corpus was byte-identical through all seven.

Most of them are EPUB 2 rules where XHTML 1.1 is stricter than HTML5, so the
EPUB 3 column is asserted alongside the EPUB 2 one in every test: tightening
the shared grammar instead of the EPUB 2 one would invent errors on every
modern book.

**CSS-001 is EPUB 3 only.** `direction` and `unicode-bidi` drew an error at
any version, and epubcheck guards the rule with `version == VERSION_3`
(`CSSHandler.java`), keeping its fixtures under `epub3/`. A real EPUB 2 book
with `<h1 style="direction: inherit">` was therefore told it had an error it
did not have. The neighbouring CSS-006 (`position: fixed`) is *not* guarded
there and is unchanged here — checked in the same pass, so this is the whole
class rather than a sample of it.

**`ol@start`, `ol@type` and `li@value` are now rejected in EPUB 2.** XHTML
1.1 gives `ol.attlist`, `ul.attlist` and `li.attlist` exactly `Common.attrib`;
all three attributes come from `legacy.rng`, which OPS 2.0.1's `content.rng`
never includes — the same reason `align` and `clear` are already errors.
epubcheck reports one RSC-005 each and we reported none, on 4 shelf books.
EPUB 3 is untouched: HTML5 has all three, so the tightening applies only to
the EPUB 2 grammar, and a test asserts both columns plus a clean control.

**`data-*` attributes are now rejected in EPUB 2.** The grammar cannot express
a prefix wildcard, so a `data-` name is suppressed at the report level; that
suppression applied at every version. `data-*` is an HTML5 family and XHTML
1.1 has no such concept, so epubcheck gives a plain RSC-005 in a
`version="2.0"` book. A malformed name (`data-`) still produces exactly one
finding, not one per owning check — HTM-061 also covers that case and
epubcheck reports it once.

**An empty `<tr>` or row group is now an error in EPUB 2.** XHTML 1.1 makes
`tr` `oneOrMore (th|td)` and `thead`/`tfoot`/`tbody` `oneOrMore tr`; HTML5
permits all of them empty, so this joins `ol`/`ul`/`dl` as an EPUB 2-only
content-model rule and EPUB 3 is untouched.

Consumers keying on message IDs: no ID changed meaning, and no ID was split.
The EPUB 2 changes add RSC-005 findings on books that previously reported
none of them; measured across the 336-book shelf they move 9 books, all in
the same direction, and two of those books go from a clean verdict to two
findings each — both of which epubcheck was already reporting.

**A stylesheet's stray declaration no longer draws four CSS-008 findings**
(styloria [#3](https://github.com/veripublica/styloria/issues/3), picked up
here as `styloria 0.9.1`). A file beginning with `text-indent:1.5em;` outside
any rule became one qualified rule's prelude, and the selector walk reported
every token it could not accept — the `:`, the `1.5em`, the `;`, and the
`@page` of the *next* rule, which is well-formed CSS. epubcheck reports one.
The reporting unit upstream is now the comma-separated selector, so the pile
collapses to a single finding while the *deliberate* difference is untouched:
`. h-100, . y-100 { }` is two genuinely broken selectors and still reports
two. `docs/COVERAGE.md` now separates the two cases, because they look
identical in a count diff and only one of them is a defect.

**EPUB 3 package metadata is checked at all now** (MobileRead, thread page
13). A reader sent epubcheck's output beside ours for the same book: 13
findings against our 7, and all six we missed were OPF 2 attributes left on
Dublin Core elements by a converted book — `opf:role`, `opf:file-as`,
`opf:scheme`, `opf:event`. EPUB 3 replaced every one of them with
`<meta refines>`, and our `<metadata>` content model was fully permissive, so
no attribute on any metadata element was being checked.

The two attribute lists are now what `package-30.rnc` specifies and are
deliberately *not* the same: `dc:title`, `dc:creator`, `dc:contributor`,
`dc:subject`, `dc:description`, `dc:publisher`, `dc:relation`, `dc:coverage`,
`dc:rights` and `dc:source` take `id`, `dir` and `xml:lang`, while
`dc:identifier`, `dc:language`, `dc:date`, `dc:type` and `dc:format` take
`id` alone — so `xml:lang` is valid on `dc:title` and an error on
`dc:language`. `<meta>`, `<link>` and foreign metadata elements stay
unconstrained, and EPUB 2 packages are untouched, where these attributes are
legitimate.

**An OPF-namespaced attribute is now named in full in the message.** We
reported `attribute "file-as" is not allowed here` for `opf:file-as`, naming
something that appears nowhere in the reader's file — and we were already
inconsistent with ourselves, since `epub:type` on the same book was reported
with its prefix. Consumers reading the `params` of an
`opf.package.schema_violation` finding will now see `opf:file-as` rather than
`file-as`; the message shape is unchanged. No book on the 336-book shelf
produces such a message today.

All seven fixes leave the corpus exactly where it was: 606/607 exact-ID recall,
0 false positives on 355 should-be-clean cases. Across the 336-book shelf,
**329 books agree with epubcheck on the ID set exactly and not one ID is
reported by epubveri alone.**

## [0.9.18] - 2026-08-14

A false-positive release: four fixes, none of which the corpus or the shelf
could see on its own, and each of the last three found while verifying the
one before it.

`text/html` is a media type Calibre emits and epubcheck has always treated as
a *deprecated content type*: it warns once and then goes on validating the
document. We treated it as a foreign resource instead, and that single
difference cost in both directions at once — we invented errors about the
item, and skipped every check that belonged inside it.

### Fixed

- **`text/html` items in an EPUB 2 book are content documents again** (#72).
  Found by running `--bin compare` over ten books newly added to the shelf:
  eight agreed with epubcheck exactly and one did not. It declares
  `media-type="text/html"` on all 91 of its spine items, and drew **94
  findings epubcheck does not report** — 91 OPF-043, 3 RSC-010, and an
  OPF-032 — while every reference *inside* those 91 documents went unchecked.
  On a minimal book the verdicts differed outright: epubcheck accepted it, we
  rejected it.

  Four things changed, each measured against epubcheck one book at a time:

  - A `text/html` spine item needs no fallback, so no OPF-043 — and **only in
    EPUB 2**. epubcheck's two branches genuinely differ (`OPFChecker`:419
    consults `isDeprecatedBlessedItemType`, `OPFChecker30`:251 does not), so
    the same book is an error at 3.0 and clean at 2.0. Likewise no RSC-010
    for an NCX or nav link pointing at one.
  - A `<guide>` reference to one is no longer OPF-032, but it **is** RSC-032.
    Replacing one with silence would have turned a wrong ID into a false
    negative: epubcheck registers guide references as GENERIC and asks the
    foreign-resource fallback question about them separately. That question
    had never been asked here, so a guide reference to a DTBook or a PDF was
    also missing its RSC-032 — both now reported.
  - The documents are parsed and their references, fragments and DOM-level
    checks run. The one real book hid **91 missing resources** this way, and
    a `text/html` document that is not even well-formed XML used to report
    nothing at all.
  - The XHTML **grammar** stays off them, along with the duplicate-`id` and
    ID-reference checks, which belong to the same validator set in epubcheck
    (`IDUNIQUE_20_SCH` is keyed on `application/xhtml+xml` exactly as the
    grammar is). Measured: the identical document draws three RSC-005
    declared `application/xhtml+xml` and none declared `text/html`. Running
    the grammar over them anyway would have replaced 94 false positives with
    a larger number of them.

- **OPF-035 no longer depends on the file's contents, and is EPUB 2 only.**
  Both halves were wrong, in opposite directions. epubcheck emits it from the
  declared media-type alone without opening the file, so a `text/html` item
  holding something that is not markup drew nothing from us — the one shape
  where the author most needs telling. And `OPFChecker30.checkItem` never
  calls `super`, so the message is unreachable for EPUB 3, where we were
  reporting it. It is now anchored at the manifest item in the package
  document, which is where epubcheck reports it, rather than in the content
  document.

- **A valid image in a format we did not recognise is no longer called
  corrupt** (#75). `sniff_image_type` knew four formats, and anything else
  took the "unrecognised" path, which reports PKG-021 *corrupt* as an ERROR.
  One real book carries a valid little-endian TIFF named `.png` and declared
  `image/png`; we called it corrupt where epubcheck reports PKG-022 *wrong
  file extension* as a WARNING. It is mislabelled twice over and not corrupt
  at all. TIFF (both byte orders) and BMP are now sniffed, with the matching
  file extensions so a correctly-named file stays silent.

  The *unrecognised* path itself was already right and is unchanged: a file
  whose content matches nothing still draws PKG-021 from both tools, measured
  with a garbage file named `.png`. Only the set of formats we can name was
  too small. All six shapes now agree with epubcheck exactly — garbage,
  little- and big-endian TIFF under a `.png` name, TIFF under a `.tif` name,
  BMP, and the real book's image.

- **ID-reference resolution is EPUB 3 only** (#74). We resolved
  `aria-labelledby`, `for` and their relatives against the document's own
  `id` values in every version and reported RSC-005 when the target was
  missing; epubcheck does this for EPUB 3 alone. Its sibling check already
  carried the version condition — this block was simply missed.

  Nothing real is lost: every attribute the block names is absent from XHTML
  1.1 (ARIA entirely, and `for` only via the Forms module, which OPS 2.0.1
  does not include), so in an EPUB 2 book the grammar has already rejected
  the attribute and this only added a second message about the same defect.
  The opposite direction was checked as well — `headers` *is* in XHTML 1.1
  and takes IDREFS, and a dangling `headers` reference draws nothing from
  epubcheck either, so there is no gap to fill.

- **An EPUB 3 dangling ID reference is reported once, not twice** (#76), and
  `aria-details` is no longer reported at all. Two implementations of the same
  rule had always overlapped in EPUB 3; gating one of them for #74 is what
  made the overlap visible. The thinner one — existence only, none of the type
  constraints — was deleted rather than merged, because everything it covered
  is handled by the survivor except `aria-details`, which epubcheck does not
  check.

  Probed one book each, counting RSC-005: a dangling `aria-details` draws
  nothing from epubcheck, while `@form`, `@list`, `label/@for` and `@headers`
  each draw exactly one. Six of the seven shapes now agree exactly; the
  seventh is a known under-report of ours (an element carrying
  `@aria-activedescendant` must also declare `@role`, which is a grammar rule
  we do not implement).

### Notes for consumers

- **`opf.content_document.dangling_id_reference` no longer exists.** The rule
  it keyed was a duplicate of `opf.content_document.idref_unresolved`, which
  survives and covers every case it did (bar `aria-details`, a false
  positive). Nothing keyed on the removed slug can match any more — an
  allowlist naming it needs the surviving slug instead. Not in any known
  downstream consumer's rule list, and the direction is a strict reduction.
- `opf.content_document.duplicate_id` no longer fires for a `text/html`
  document. No message, id or position moves for any other document; this is
  a strict reduction, in the direction of what epubcheck reports.
- The OPF-035 message wording and position both changed (see above). It
  carries no `rule` slug, so nothing can be keyed on it today.

## [0.9.17] - 2026-08-12

A message-wording release: the one sentence a reader meets when their book
will not parse at all was still the XML parser's, and it read backwards.
Nothing about which books pass or fail changes.

### Changed

- **The well-formedness fatal for an unclosed element is now written in our
  own words** (#71). Reported by Doitsu on MobileRead (#177). Underneath, the
  message carried roxmltree's phrasing — `expected 'p' tag, not 'body' at
  12:1` — which calls neither of them an *end* tag, so it reads as though a
  `<p>` element belonged where `<body>` sits. That is backwards: `</body>`
  arrived while `</p>` was still owed. 0.9.16's clause naming the opening line
  was appended to that sentence, so one message ended up in two voices and two
  quoting styles.

  Before and after, on `<p>Lorem ipsum<p>`:

      content document is not well-formed XML: expected 'p' tag, not 'body' at 12:1 (<p> at line 11 is never closed) [OEBPS/ch1.xhtml:12:1]
      content document is not well-formed XML: unclosed <p> at line 11; expected </p> but found </body> [OEBPS/ch1.xhtml:12:1]

  The defect leads, both tags are named as end tags, and the position is no
  longer printed twice. For a void element the close tag is a red herring —
  the author wrote HTML in an XHTML document — so that case keeps its single
  unambiguous fix: `unclosed <hr> at line 8; XHTML requires <hr/>`.

  Applied to **every** file we parse, not only content documents: an unclosed
  element reads the same way in an OPF, an NCX, a media overlay or a
  `container.xml`, and all of them shared the library wording. Every other
  parse error keeps roxmltree's text.

  Message text only — no ID, severity, position or count moves, on any input.
  The rule key is `*.malformed_xml`, which no downstream consumer keys on.

## [0.9.16] - 2026-08-11

Three more false-positive classes, all found here rather than reported —
markup this tool rejected and epubcheck accepts. `<template>` is the one to
upgrade for: it appeared in the grammar nowhere at all, so it was an error in
`<head>`, in phrasing and in flow alike.

Two long-standing gaps close alongside them, both of which had been hiding
behind a broken instrument: OEBPS 1.2 packages were being judged by EPUB 2's
rules, and `<epub:switch>` accepted a shape epubcheck rejects. Corpus
detection recall is now 513/513 and exact-ID recall 606/607.

### Fixed

- **`<epub:switch>` now requires at least one `<epub:case>` and an
  `<epub:default>`.** Both were optional in our grammar, where epubcheck's is
  `case+, default` — so a switch missing either validated clean. These were
  the **only two scenarios in the corpus's 981** where a book epubcheck fails
  drew no error at all from us, and they were invisible while the harness's
  detection-recall denominator was wrong. Detection recall is now 513/513.

  Known granularity difference, documented and left: on the no-case shape we
  report two findings (the misplaced `<epub:default>` and the incomplete
  `<epub:switch>`) where epubcheck merges them into one.

- **A well-formedness fatal now names the line the element was *opened* on.**
  An unterminated `<hr>` at line 81 made the parse fail at line 157, and both
  we and epubcheck pointed at line 157 — the `</body>` that could not match —
  rather than at the tag the author has to fix. The message now reads
  `… (<hr> at line 81 is never closed; XHTML requires <hr/>)`, with the XHTML
  advice added only for HTML's void elements, where writing `<hr>` out of HTML
  habit is the usual cause.

  Reported by JSWolf on MobileRead (#174). Not a bug: our single FATAL was
  correct, and the 74 extra errors epubcheck reports on the same file are one
  defect repeated — its streaming validator keeps validating *inside* the
  element that was never closed, so every following `<p>` is "not allowed
  here". All 75 of its findings are in that one file and 74 carry the same
  message.

- **OEBPS 1.2 packages are flagged, not judged as EPUB 2 (OPF-047).** The
  pre-EPUB format — package in the `openebook.org` namespace, Dublin Core
  inside `<dc-metadata>`, no NCX, `text/x-oeb1-document` content — was being
  validated against EPUB 2's rules, which it does not have. On epubcheck's own
  legacy fixtures we reported **7 errors against its 4 findings**, six of ours
  invented. We now report OPF-047 and skip the rules that do not apply
  (required Dublin Core in the OPF namespace, `spine/@toc`, OPF-043 on
  `text/x-oeb1-document`, OPF-035 on `text/html`, and the OPF-bound package
  grammar). Our findings are now a strict subset of epubcheck's, and the
  verdict agrees.

- **OPF-038 / OPF-039**: a modern media type inside an OEBPS 1.2 package —
  which wants `text/x-oeb1-document` and `text/x-oeb1-css`. These were "out of
  scope" an hour before they were written: they became cheap the moment
  OPF-047 gave the code a way to know it was looking at such a package.

  epubcheck asks the question in two places with different conditions, and
  both are reproduced: `text/html` is OPF-038 unconditionally, while XHTML,
  DTBook and `text/css` are OPF-038/OPF-039 only when the item declares no
  `fallback`. No corpus fixture covers the fallback half, so it was checked
  against 5.3.0 directly — five books, five agreements.

  Still out of scope, deliberately: the `oebpkg12` DTD.

  **`docs/COVERAGE.md` goes 207/210 → 208/210 across the two changes, and one
  step of that is a correction rather than progress.** OPF-047, OPF-038 and
  OPF-039 are now implemented (+3), but OPF-038/OPF-039 had been *counted* as
  implemented all along (-2 when that was found): `ids.rs` declared their
  constants and nothing ever emitted them, which the matrix reads as
  coverage.

- **`<template>` is accepted, and `<script>`/`<template>` are accepted in the
  containers HTML5 admits them in** (EPUB 3). `<template>` was in the grammar
  nowhere at all, so it was an error everywhere — in `<head>`, in phrasing, in
  flow. And a `<ul><script>…</script><li>` was rejected, though HTML5 admits
  "script-supporting elements" in list, table, `picture`, `hgroup` and
  `select` content models.

  Fourteen probes, fourteen false positives against epubcheck 5.3.0; all
  fourteen now agree. The sites are enumerated from epubcheck's own modules
  (`common.elem.script-supporting`), not guessed: `ul`, `ol`, `dl`, `menu`,
  `table`, `thead`, `tbody`, `tfoot`, `tr`, `colgroup`, `picture`, `hgroup`,
  `select`, `optgroup`.

  EPUB 2 is unchanged and still rejects both — XHTML 1.1 has no `<template>`,
  and `<ul><script>` is an error there too, both measured.

- **`<hgroup>` referenced the EPUB 2 `<script>` pattern from the EPUB 3
  grammar**, a cross-version leak found while doing the above. It granted
  XHTML 1.1's attribute set plus `charset`, so an EPUB 3
  `<hgroup><script epub:type="…">` was rejected and a `charset` accepted.

- **"expected one of …" now names what may appear *here*.** The suggestion set
  walked `After(content, rest)` into `rest` whenever `content` was nullable —
  right for a sequence, wrong here, because `rest` is the parent's
  continuation and nothing in it can appear before this element's end tag.
  Inside `<ol>`, whose model is the nullable `zeroOrMore(li)`, the set became
  `li` plus the whole flow vocabulary, overflowed the suggestion cap, and the
  tail disappeared. `<ol>` now says `expected "li"` and `<table>` names its
  six children, both matching epubcheck.

  **On real books this is worth almost nothing, and that was measured**: two
  builds over a 146-book shelf differ by 2 findings out of 8190. It is fixed
  because the set was wrong, not because it was costly — the same conflation
  had already caused a real defect, a position test that read "phrasing is
  allowed here" inside `<ul>`.

## [0.9.15] - 2026-08-10

Two false positives, both found here rather than reported: markup this tool
rejected and epubcheck accepts. The custom-element one is the reason to
upgrade — a book using `<epub-switch>` or any other hyphenated element name
was reported INVALID.

One of the two had been sitting in the corpus harness's own output for
months, inside a standing "4 false positives" figure that was being read as
a constant rather than as four open questions. Only that one was ours; the
other three were the instrument's, and are fixed too.

### Added

- **EDUPUB heading ranks are checked** (`--profile edupub`, RSC-005). A
  heading's rank must match its sectioning depth: a document that starts at
  `h2` must step to `h3` one section in. The rule is *relative* — it derives
  an expected rank for every heading from whichever heading came first — so
  it does not demand an `h1`.

  This was a named gap in `edupub.rs`, deferred on the grounds that "real
  fixtures gave contradictory evidence for the exact depth-counting
  algorithm". They did not disagree; the algorithm was being inferred from
  the fixtures rather than read from epubcheck's `edu-structure.sch`, which
  states it outright — including the two things that made the fixtures look
  contradictory (a different formula when `<body>` acts as a section, and a
  baseline heading that ignores `aside`/`nav` and anything more than one
  sectioning level deep).

- **An index-only document must declare the index on `<body>`**
  (`--profile idx`, RSC-005). Reproduces one quirk of epubcheck on purpose:
  its assert tokenizes on `'/s+'` — a typo for `'\s+'` — so a multi-token
  `epub:type="index frontmatter"` never matches and is reported. Measured
  against 5.3.0 rather than assumed; tokenizing properly here would be a
  silent false negative against the tool we are matched against.

Corpus exact-ID recall **98.8% → 99.5%** (604/607). The three remaining
misses are two OEBPS 1.2 scenarios (out of scope) and one that measures
epubcheck's single-document mode, which this tool does not have.

### Fixed

- **An EDUPUB `role="heading"` with no `aria-level` counts as a heading
  (false positive).** `<body><span role="heading">Top</span>…</body>` drew
  "The body element requires a heading when it is used as an implied
  section"; epubcheck is silent on the same book, its selector being a bare
  `html:*[@role='heading']`. Two predicates here answered "is this a
  heading" differently; there is now one.

- **HTML custom elements are no longer rejected (false positive).**
  `<epub-switch>`, `<my-widget>` — any element in the XHTML namespace whose
  name contains a hyphen — drew RSC-005 "not allowed here" and made the book
  INVALID. epubcheck accepts them and reports nothing at all.

  Neither grammar can express the rule: **no RELAX NG name class says "any
  name containing a hyphen"**. epubcheck rewrites such elements into a private
  namespace before validating so that an `element c:*` pattern can match them;
  we accept them in the derivative engine at the point of rejection instead.
  The name test is epubcheck's own, verbatim — XHTML namespace, contains `-`,
  nothing else — deliberately *not* HTML's stricter
  `PotentialCustomElementName`, which would invent errors on documents
  epubcheck accepts.

  Accepted **only where flow or phrasing content is allowed**, matching
  epubcheck's grammar, which adds them to `common.elem.flow` and
  `common.elem.phrasing` and nowhere else. So they remain errors in `<head>`,
  as a child of `<ul>`, without a hyphen, and **anywhere in EPUB 2** (XHTML
  1.1 has no such concept). Their content is transparent: a `<div>` inside a
  custom element inside a `<p>` is still an error.

  All eight cases were run as real books through epubcheck 5.3.0 and agree.
  That enumeration is the whole evidence: **no book on the 136-book shelf
  contains a custom element**, so neither the shelf nor `compare` could see
  this, and the corpus only ever showed it as one line of a false-positive
  count (now 4 → 3; the remaining three are not ours — see the harness notes).

- **A `<guide>` reference's `#fragment` is now resolved (RSC-012).** A
  `<reference type="toc" href="chapter.xhtml#frag"/>` whose fragment names no
  `id` in the target went unreported, while epubcheck reports it as an error.

  Fragment resolution here had grown one site per *source* — the NCX
  `<content src>`, then content-document hrefs, then `epub:textref` — and the
  guide was never added. epubcheck has no such split: its check is on the
  reference, so every registered reference is resolved by the same code and
  the guide was covered from the start.

  Found by the `compare` harness on a real book whose `<reference type="toc">`
  pointed at an id that lives in a *different* file. Our output for the whole
  package document was empty; epubcheck's was this one error. **1 book of 136
  on the shelf**, one occurrence, and the only shelf change.

  As epubcheck does, the fragment is resolved only against XHTML and SVG
  targets, and a fragment carrying `=`, `:` or `(` (a CFI or media fragment,
  not an id) is left alone.

  **For consumers:** this adds a rule slug, `opf.guide.reference_fragment_not_defined`.
  A downstream allowlist of rules it handles will not contain it.

## [0.9.14] - 2026-08-09

Completes the EPUB 3.4 work list. All eight issues the spec editor filed
against epubcheck for 3.4 (w3c/epubcheck#1616, #1642, #1649–#1654) are still
open there; none is unaddressed here. Three of the eight needed no code —
`its-*` attributes were already valid, resources referenced from `<script>`
were already exempt from the fallback requirement, and #1650's "outdated
features" dissolved on measurement — so this is not eight implementations.

### Added

- **`rendition:layout` accepts `roll`**, EPUB 3.4's webtoon layout
  (w3c/epubcheck#1651). This half is *permissive*, so it ships unflagged:
  accepting `roll` costs a false negative against epubcheck-as-it-is-today
  (5.3.0 still reports it, measured) and removes a false positive against the
  spec-as-it-will-be. Only the global value gains it — #1651 says spine
  overrides may not accompany roll at all, so there is no
  `rendition:layout-roll` spine property.

- **ADV-006 / ADV-007 / ADV-008 — the restrictive half of EPUB 3.4**, all
  opt-in behind `--advisory` and none affecting the verdict, because epubcheck
  has implemented neither #1649 nor #1651 and a side-by-side diff cannot tell
  these from false positives until it does.

  - **ADV-006**: a `rendition:layout-*` spine override beside a `roll` package
    layout (#1651, "no mixing layouts").
  - **ADV-007**: a roll spine's XHTML document declaring no viewport width and
    height, i.e. no ICB dimensions (#1651). Deliberately *not* modelled as
    "roll implies fixed-layout", which would be the truer reading but would
    switch on the viewport checks at **error** severity — restrictive,
    unflagged and counting toward the verdict, which an advisory may not do.
    It asks only whether the dimensions are declared; validating the values
    stays with the existing check, so the two cannot disagree about the same
    document.
  - **ADV-008**: a feature deprecated in 3.4 (#1649) — the
    `rendition:align-x-center` spine property, and the reserved prefixes
    `xsd`/`msv`/`prism` on a `prefix` declaration. Reported on the
    *declaration* rather than on use: a book relying on a reserved mapping
    without declaring it says nothing in the package document.

  **0 of the 125 shelf books draws any of the three.** That is the advisory
  bar met, but it is silence rather than confirmation — no real book uses a
  layout the specification introduced weeks ago — so the evidence is the
  enumeration in the tests, not the shelf.

## [0.9.13] - 2026-08-09

### Fixed

- **A malformed selector inside an `@media` is reported.** It was caught at the
  top level of a stylesheet and silently accepted one grouping at-rule deep, so
  `. foo { }` — a class selector with a space after the dot, which real books
  carry as a find-replace accident — passed unremarked inside a media query.

  The cause was in the CSS parser, not here: CSS Syntax §5.4.2 hands an
  at-rule's block on as a simple block, and nothing inside one was ever
  re-entered as a rule, so the selector check was never reached. Fixed as
  [styloria#2](https://github.com/veripublica/styloria/issues/2) and consumed
  via its new `parse_rule_list` (styloria 0.9).

  Two hand-rolled pieces went with it: this crate no longer works out where a
  nested rule's prelude ends, and it no longer guesses that a nested block is a
  grouping one by whether it contains a block — it asks the at-rule's name, the
  same test the top level already used.

  One book on the 125-book shelf moves, from 1 finding to 22; no other book
  changes. epubcheck reports 12 on it, because it stops at the first error in
  each selector list where we report each — the same granularity difference
  already recorded for CSS-028. Every one of the 22 is a genuine malformed
  selector.

- **A nav or NCX link to a resource with a Content Document fallback is
  allowed** (#168, reported by Doitsu on MobileRead). epubcheck's RSC-010
  condition has three clauses and we had two; the missing one asks whether the
  target declares a `fallback` chain reaching a Content Document. On the IDPF
  `haruko-jpeg` sample — an image-based book whose nav and NCX link straight at
  the JPEGs — epubcheck reports a single usage message and we reported three
  errors. The two tools now agree exactly.

### Added

- **ADV-004 now reads the content documents, not only the package.** Reported
  by DNSB on MobileRead (#169/#170) with a Calibre AZW3→EPUB 2 conversion:
  epubcheck and epubveri disagreed wildly on it, and changing the version
  attribute to `3.0` made most findings vanish — our 429 became 6, epubcheck's
  3432 became 10. That is exactly what ADV-004 exists to say in one line, and
  it stayed silent, because the book's package carries **no** EPUB 3 signal at
  all while its content documents carry 374 `epub:type` attributes and 75 HTML5
  sectioning elements.

  Two content signals join the package ones — `epub:type` used on any element,
  and an HTML5 sectioning element — under the unchanged two-signal threshold,
  and the two halves mix.

  **The signal is *use*, not declaration**, which is measured rather than
  assumed: of 72 EPUB 2 books on the shelf, 8 bind the EPUB 3 `ops` namespace in
  their content documents and only 2 ever use `epub:type` — the other 6 carry an
  unused `xmlns:epub` as producer boilerplate. Keying on the binding would fire
  on all 8, which is the ADV-003 failure mode. Keying on use reports 3 books of
  125, all three genuinely written in EPUB 3.

- **ADV-005: `page-spread-*` on a reflowable document** (EPUB 3.4,
  [w3c/epubcheck#1652](https://github.com/w3c/epubcheck/issues/1652)). Placing
  a page on one side of a spread is meaningless for reflowable content, so
  EPUB 3.4 confines the property to fixed-layout documents. The itemref's own
  `rendition:layout-*` override is folded over the package default, so this
  fires both on a wholly reflowable book and on a single pre-paginated page
  that overrides itself back.

  Advisory-only, behind `--advisory`, and never in the verdict: epubcheck has
  not implemented #1652, so to anyone diffing the two tools this would be
  indistinguishable from a false positive. It becomes an ordinary error once
  epubcheck ships it.

## [0.9.12] - 2026-08-08

### Fixed

- **`alt` is required on `<img>` in EPUB 2, and a missing required attribute no
  longer stops the walk.** Two faults, found together.

  XHTML 1.1's `img.attlist` makes `alt` required; HTML5's `img.attrs.alt?` does
  not, so this is one of the few places EPUB 2 is the stricter version. We had
  it optional on both.

  And the recovery: `#60` taught the derivative to carry on after *incomplete
  content*, but the attribute branch still returned `NotAllowed`, which made the
  sibling walk break. So one `<img>` without `alt` silenced every finding after
  it in that file. On the book that surfaced this — 72 such images — epubcheck
  reports 73 and we reported **2**, one per file.

  Both fixed, and that book now matches epubcheck exactly at 82 = 82. Ten shelf
  books gain findings, every one of them still at or below epubcheck's count,
  and six of the ten now match it exactly.

  Neither was visible to the ID-set diff: the book reports RSC-005 either way.
  It took the count comparison added to `compare` in the same release.

- **A dictionary collection is now checked on its own `role`, not on
  `dc:type`.** epubcheck's `checkCollections`/`checkCollectionsContent` iterate
  the collections and test `collection.hasRole(DICTIONARY)` and nothing else;
  we required the publication to declare `dc:type="dictionary"` first. A book
  with a malformed `<collection role="dictionary">` and no `dc:type` drew
  **nothing at all** from us where epubcheck reports four — including OPF-083,
  a row `docs/COVERAGE.md` marks as implemented. A check that cannot fire is
  worse in the matrix than one that is honestly absent.

  Now 4 = 4 on that shape. The `dc:type`-required finding for the `dict`
  profile is unaffected: its fixture carries no `<collection>` at all.

  Also corrects OPF-083's wording, which said a collection "must contain no
  Search Key Map Document" — the exact opposite of the condition it reports.

- **Five EPUB 2 elements were taking the EPUB 3 global attribute set** (JSWolf,
  MobileRead #165). `img`, `area`, `iframe`, `param` and `script` reached
  `globalAttrs` through a define shared between the two versions, so
  `<img role="x">` and its four siblings sailed through everything #66
  tightened. epubcheck rejects `role` on all five in EPUB 2; we accepted it on
  all five.

  Found by walking the grammar from the EPUB 2 root and listing every define
  that pulls in the EPUB 3 global set — the same reachability question that
  exposed the `map` leak in #69, asked of the whole grammar instead of waiting
  for the next report. One of the five, `area`, became reachable *because* of
  #69's fix.

  A test now asserts the invariant over the schema text, and it was checked in
  both directions: reintroducing the `img` leak fails it.

  `iframe`, `param` and `script` are knowingly left slightly permissive —
  schema/20 gives them `Core.attrib`, `id.attrib` and no common attributes
  respectively, where they now get the full XHTML 1.1 `Common.attrib`. That is
  a false negative rather than a false positive, and the exact lists are
  recorded at the site.

### Changed

- **The `compare` harness diffs finding counts, not just the ID set.** It
  already collected `ID -> count` for both tools and compared only the keys, so
  a book where we report 5 of something and epubcheck reports 500 counted as
  "agreed". First run over 125 books found 47 such divergences, two of them
  unexplained — one is the `<img alt>` fix above. Not shipped to users: the
  harness is a workspace member, not part of the crate.

## [0.9.11] - 2026-08-07

### Added

- **Schematron findings now carry a `rule` slug**, derived from the pattern's
  `@id` — `opf.package.opf_meta_title_type_refines`, and so on. They were the
  only findings emitted with `rule: null`, which put them in the bucket a
  downstream consumer cannot key on; epubsana reported that bucket at 172
  findings across 26 books. It mattered immediately, because 0.9.10's
  `@refines` assertions landed in it and `refines="title"` → `refines="#title"`
  is about as determinate as a repair gets.

  Purely additive — these findings previously had no key at all, so nothing
  that keys on `rule` can break. Counts, IDs, messages and positions are
  unchanged; the corpus and the shelf are byte-identical. Unkeyed findings on
  the 115-book shelf go 291 → 287.

- **Every finding a real book produces now carries a `rule`.** Sixteen further
  check sites were unkeyed — `CSS-028`, `PKG-010`, `OPF-072`, `ACC-009`,
  `OPF-090`, `RSC-004`, `OPF-030`, `NCX-001`, `OPF-052`, `OPF-003`, `NCX-006`,
  `PKG-022`, `PKG-006`, `OPF-096b`, `OPF-053`, `OPF-029` — and most now carry
  `params` as well. Across the 115-book shelf, unkeyed findings go **287 → 0**
  of 23,254.

  Counts, IDs, messages and positions are unchanged; corpus and shelf are
  byte-identical. 124 push sites in the source are still unkeyed, but none of
  them fires on any of the 115 books; they are named one at a time as evidence
  arrives rather than in bulk, because a rule slug is a semi-public key and a
  wrong name shipped is worse than no name at all.

  Two tests hold it: every pattern in both schemas must have a unique `@id`
  (without one a finding would publish a shared `…unnamed_pattern` key, which
  is worse than `None` because it looks real), and the slug derivation is
  pinned.

## [0.9.10] - 2026-08-07

### Fixed

- **`@refines` without its `#` is now an error, as epubcheck reports it**
  (Doitsu, MobileRead #163). `<meta refines="title" property="title-type">`
  drew a warning from us and an ERROR from epubcheck, so the *verdict* differed
  — its book was INVALID and ours VALID, which is the worst shape a divergence
  can take. epubcheck's Schematron compares `@refines` against
  `concat('#', @id)`, so a bare id fails the "must refine a title property"
  assertion. Seven of those assertions are now ported: `title-type`,
  `authority`, `term`, `identifier-type`, `role`, `collection-type` and
  `source-of`.

- **The RSC-017 "use a fragment identifier" warning was far too broad.** It
  fired on any non-fragment `@refines`; epubcheck reports it only when the value
  resolves to an actual manifest item's `href` — the case where a publication
  resource was named instead of an id. A bare id with a missing `#` drew a
  warning epubcheck never gives. The message now names the item id it should
  have pointed at, as epubcheck's does.

## [0.9.9] - 2026-08-07

### Fixed

- **`epub:trigger` is a real EPUB 3 element and we did not have it**
  (Doitsu, MobileRead #161). Every occurrence drew five findings — the element,
  its `action` and `ref`, and two HTM-054 "reserved namespace" errors for the
  `ev:` attributes — where epubcheck draws one deprecation warning. It is now
  transcribed from `schema/30/mod/epub-trigger.rnc`, with the required/optional
  attribute split epubcheck specifies, and `http://www.w3.org/2001/xml-events`
  is a known namespace.

- **`<video>` and `<audio>` are transparent**, so a `<div>` fallback after the
  `<source>` elements is ordinary content. epubcheck models this with two
  variants and takes the flow one at flow level (`video.inner.flow` ends in
  `common.inner.transparent.flow`); we had only the phrasing variant, so the
  standard fallback shape was rejected.

- **Which prefixes are *reserved* now depends on the document declaring them.**
  epubcheck passes a different predefined map per context: the package document
  reserves `a11y`/`dcterms`/`marc`/`media`/`onix`/`rendition`/`schema`/`xsd`, a
  content document reserves only `msv`/`prism`, and a Media Overlay reserves
  none. We used the union everywhere, so `epub:prefix="media: …"` on a content
  document's `<html>` drew a spurious OPF-007.

  On the reported book — the IDPF `cc-shared-culture` sample — these three take
  us from **38 errors to 2**, which is what epubcheck reports.

- **EPUB 2 no longer grants the 195 global attributes XHTML 1.1 does not have**
  (#66). `globalAttrsRest` was shared with EPUB 3, so an EPUB 2 document was
  granted 207 attribute names where XHTML 1.1's `Common.attrib` grants seven.
  The event handlers, RDFa, microdata, ITS and the HTML5 globals are gone;
  every one of the 147 non-ARIA names was measured against epubcheck **one book
  per attribute**, and it rejects all 147; `role` and a sample of `aria-*` were
  measured the same way.

  Two are relocated rather than dropped: XHTML 1.1 gives `tabindex` to
  `a`/`area`/`object` and `xml:space` to `pre`/`script`/`style`, so they move
  onto those elements. That distinction is what the previous slice got wrong,
  when `content` and `accesskey` were removed as "not global" and broke
  `<meta>` and `<a>`, which declare them. The test applied here is absence from
  all 25 modules `content.rng` includes — `<a tabindex>`, `<pre xml:space>` and
  `<object tabindex>` were each checked against epubcheck afterwards.

  The Events module is the headline surprise: `content.rng` never includes
  `events.rng`, the same exclusion that removes Forms, so OPS 2.0.1 has no
  event-handler attributes at all — not even `onload` on `<body>`. A test
  asserting the opposite has been inverted.

  **`role` and the 47 `aria-*` go too**, completing #66. epubcheck rejects all
  48, and the reason is chronological rather than editorial: OPS 2.0.1 was
  finalised 2010-09-04 and WAI-ARIA 1.0 only became a W3C Recommendation on
  2014-03-20, so `schema/20` could not have carried them.

  This half was held back while the shelf could not see the population it would
  affect. Four independent looks then came up empty: 0 of 60 EPUB 2 books on the
  shelf use ARIA and 0 declare any accessibility metadata at all; the DAISY
  Consortium's own accessible EPUB 2 sample uses none; and the one EPUB 3 →
  EPUB 2 conversion on the shelf carried 374 `epub:type` leftovers but no ARIA.
  The EPUB 2-era accessibility tradition was NCX/DTBook. That is not proof of
  absence, and it is not recorded as one — the call was to match epubcheck now
  and fix from a real report if one arrives.

  Blast radius on the 115-book shelf: **one book, 54 findings, all `hidden`,
  and epubcheck reports exactly the same 54.** The ARIA half moves nothing at
  all, which is the same silence that made it undecidable from the shelf.

- **RSC-026 now applies to every reference, not just manifest hrefs.** epubcheck
  performs this check in `URLChecker`, its single URL-resolution point, so it
  lands on every URL it resolves; ours sat on the manifest alone. A content
  document's `src`/`href`, a CSS `url()` and an `@font-face src` that resolve
  above the container root — or are path-absolute — are now reported too.

  The shape real books carry is a stylesheet at the container *root* asking for
  `url(../Fonts/x.ttf)`; one shelf book has eight, and epubcheck reports all
  eight. It is additive with RSC-001/007/008: a leaking reference is both
  outside the container and missing from it, and epubcheck reports both.

- **An unresolvable `unique-identifier` no longer switches off the NCX's other
  checks.** Only NCX-001/NCX-004 need the package identifier — they compare
  `dtb:uid` against it — but the whole NCX block was gated on it, so a book
  whose `unique-identifier` names no `dc:identifier` (already reported as
  OPF-030) also lost RSC-007, RSC-010 and RSC-012 on its NCX. One shelf book had
  three genuinely undefined `<content src="…#frag">` targets and produced
  nothing at all; epubcheck reports all three, and now so do we.

  The familiar shape: a precondition for one check taking unrelated ones down
  with it, and the symptom is silence rather than a wrong answer.

- **RSC-025 is no longer reported on EPUB 2 books.** epubcheck registers exactly
  one non-normative validator — `SVG_30_INFORMATIVE_NVDL`, the full SVG 1.1
  grammar whose findings become usage-level RSC-025 — and its `ValidatorMap`
  pairs that validator with `VERSION_3` alone. An EPUB 2 document, inline SVG or
  standalone, gets `XHTML_20_NVDL`/`SVG_20_NVDL` and no informative pass at all,
  so epubcheck never emits RSC-025 there. We ran it on both versions.

  The case that surfaced it is a real EPUB 2 book with a lowercase `viewbox` and
  `preserveaspectratio`. Those attribute names genuinely are wrong — SVG names
  are case-sensitive — but RSC-025 is the family for epubcheck's *opinion*
  rather than a spec requirement, and in EPUB 2 it has none. Measured four ways
  (inline and standalone × EPUB 2 and EPUB 3); all four now agree.

  With this, **no ID is reported by epubveri and not by epubcheck across the
  whole 104-book shelf** — the false-positive-candidate list is empty, and 101
  of 104 books agree on the exact ID set.

## [0.9.8] - 2026-08-06

### Fixed

- **An empty `dc:identifier` no longer produces three findings where epubcheck
  produces one.** A self-closing `<dc:identifier opf:scheme="UUID"/>` drew
  OPF-072 ("metadata element is empty"), OPF-085 ("'' does not look like a valid
  UUID") and NCX-001 ("dtb:uid does not match ''") on top of the schema's own
  RSC-005. epubcheck reads the element's text as `getPrivateData(TEXT)`, gets
  null, and skips the block that would emit any of them. Only the RSC-005
  remains.

- **OPF-085 now judges only the identifier the package publishes under.**
  epubcheck's single call site sits inside `idAttr.equals(uniqueIdent)`, so a
  secondary `dc:identifier` — a Calibre UUID, an ISBN — is never checked. We
  checked every one, so a book carrying one malformed secondary UUID got a
  warning epubcheck does not give.

- **An empty `dc:language` is OPF-055, not OPF-072 plus RSC-005.** epubcheck
  handles `dc:*` names through an if/else-if chain whose final `else` alone
  reaches OPF-072; `identifier`, `date`, `title` and `language` all take an
  earlier branch. Our exclusion list had only `title` and `date`. The stray
  RSC-005 came from the Schematron requiring a non-empty `dc:language` in both
  versions — `opf20.rng` gives it `DC.metadata-common-content`, so it is EPUB 3
  only, exactly like `dc:title`.

- **`HTM-060` is not an epubcheck message ID.** epubcheck declares `HTM_060a`
  (a secondary viewport meta in a fixed-layout document) and `HTM_060b` (a
  viewport meta in a reflowable one), both USAGE. We emitted a bare `HTM-060` at
  INFO for both cases — an ID no epubcheck output contains, so a toolchain
  filtering on epubcheck's IDs would never match it. Now split, and at USAGE.

- **The three special prefix-mapping faults now carry their own IDs, and only
  one can fire per mapping** (#70). `prefix="_: …"` is OPF-007a, a prefix mapped
  to a default-vocabulary URI is OPF-007b, one mapped to the Dublin Core
  elements namespace is OPF-007c; the bare OPF-007 is left for its real case, a
  reserved prefix redeclared. epubcheck's `VocabUtil.checkPrefixes` is an
  if/else-if chain, so `prefix="_: http://purl.org/dc/elements/1.1/"` gave us two
  findings against its one; it is a chain here too now.

- **A missing EPUB 3 package `<link href>` target is RSC-007w**, the warning
  form epubcheck splits out for exactly that case. The severity was already
  right and only the ID was wrong.

- **A `prefix` attribute is now parsed the way epubcheck parses it** (#70), so
  each malformation reports the ID epubcheck reports: OPF-004a (no prefix before
  the colon), OPF-004b (the prefix is not an NCName), OPF-004c (no colon
  immediately after it), OPF-004d (no space before the URI), OPF-004e (something
  other than a plain space there). The old tokenizer split on whitespace, which
  cannot see the distinction that matters — `prefix="foaf:  URI"` (two spaces)
  is valid, `prefix="foaf:\tURI"` is not.

  Two of these were defects rather than renames: `prefix=": URI"` produced two
  findings where epubcheck produces one, and a prefix that is not an NCName
  (`prefix="1foaf: URI"`) produced **none at all**. Twelve values were measured
  against epubcheck, one book each, and all twelve now agree.

- **An unreferenced remote manifest item is reported** (#70), as RSC-006 — or
  the usage-level RSC-006b when the publication has scripts, since a script
  could fetch it at runtime. We reported nothing at all for this: neither the
  error, nor its usage form, nor the OPF-097 that accompanies it, because the
  unreferenced-item check skipped every external href. Audio, video, fonts and
  Flash stay exempt, as they may legitimately live outside the container.

  This needed remote references to be tracked publication-wide, which they were
  not — a remote font reached only from a CSS `@font-face` or an SVG
  `<font-face-uri>` looked unreferenced. Three corpus fixtures said so before
  the collection was fixed.

- **A misplaced element no longer cascades findings onto everything inside it**
  (#69). When an element was rejected as "not allowed here", its subtree was
  then checked against *the parent's* content model — the model the element had
  just failed. Nested `<span>`s inside a `<span>` misplaced in a `<blockquote>`
  each drew their own error: on one real book we reported 944 where epubcheck
  reports 316, the surplus being entirely depths 2 and 3.

  The subtree is now checked against the misplaced element's **own** model, as
  epubcheck does. That corrects the same bug in the other direction too, which
  is the half worth noting: a `<div>` inside that misplaced `<span>` is legal in
  the blockquote model and illegal in span's, so it was silently *missed*. It is
  now reported. One book went from 16 findings to 243, matching epubcheck
  exactly.

  Elements the grammar defines nowhere (`<center>`, `<font>`, an HTML5 `<nav>`
  in EPUB 2) keep the previous behaviour — they have no model of their own to
  check against, and epubcheck reports their bad contents too (#24).

  Across the 104-book shelf: five books changed, all now at or below epubcheck's
  finding count, and 97 of 104 agree with it on the ID set exactly.

- **EPUB 2's `<map>` no longer admits the EPUB 3 vocabulary.** The EPUB 2 branch
  referenced the EPUB 3 `map`, whose content model is HTML5 flow content, so
  `<section>`/`<figure>`/`<nav>` and friends were accepted four steps from the
  root via `body > p > map`. Known and recorded as a harmless permissiveness
  gap; it stopped being harmless once #69's fix resolved an element's model by
  reachability through the grammar, at which point it also handed EPUB 2 the
  EPUB 3 model for shared element names.

  XHTML 1.1's `id`-required and `alt`-required constraints on `map`/`area` are
  deliberately **not** adopted here — both are restrictive changes with their
  own false-positive risk, and this one is about closing the leak.

- **An obsolete attribute is no longer reported twice.** `clear`, `align` and
  the rest were reported by both the DOM check and the grammar, so `<p
  clear="all">` produced two findings where epubcheck produces one. Pre-existing,
  and widened by #69 (the attributes of a misplaced element never reached the
  grammar before); the grammar's finding is now suppressed when the DOM check is
  verified to have already reported that exact attribute.

### Changed

- **The coverage matrix reads 207 of 210 (~99%), not 191 of 194 (~98%)** (#70).
  `harness/src/coverage.rs` extracted epubcheck's IDs with a regex requiring the
  enum name to end in digits, so all 17 whose name ends in a letter were absent
  from the universe the matrix is built over — 16 of them live checks. The
  denominator was missing them and the published percentage was overstated —
  the honest figure was 196 of 210 before the checks above closed eleven of the
  gaps. The three that remain are the long-standing scope decisions (`OPF-021`
  DTBook, `OPF-047` OEBPS 1.2, the informational `OPF-064`).

  `OPF-004f` is implemented but effectively unreachable: it needs whitespace
  that Guava's `CharMatcher.whitespace()` accepts and that is not one of
  space/tab/CR/LF. Tab-separated mappings are legal, measured.

- **The corpus harness no longer credits us for message IDs we do not emit, so
  the headline recall figure moves from 98.8% to 97.9%.** No check regressed —
  the harness stripped the trailing letter from lettered IDs on the stated but
  unverified grounds that it was "a Gherkin-authoring convention … not part of
  the reported message id". It is part of it: `MessageId.java` declares
  `HTM_060a`/`HTM_060b`/`OPF-004a`…`f`/`OPF-007a`…`c`/`RSC-006b`/`RSC-007w` as
  distinct constants with their own severities. Six scenarios were scored as
  hits for a bare ID epubcheck never prints. Five have since been earned back by
  the OPF-007 and RSC-007w work above, and the last by the `prefix` parser, so
  exact-ID recall is back to 98.8% — this time without the over-crediting.

  The same belief had propagated into the validator: `check_prefix_declaration`
  documented its single-ID design as *confirmed* by the harness stripping "the
  a/b/c Gherkin sub-case suffixes". An instrument is not a source about the
  thing it measures.

- **The `compare` harness now sees epubcheck's lettered IDs at all.** Its
  extraction regex required `[A-Z]+-[0-9]+`, which matches neither `HTM_060b`
  (underscore) nor `OPF-086b` (trailing letter), so those lines were dropped
  from epubcheck's side of the diff and every such ID appeared as one "only we
  report" — a false-positive candidate manufactured by the instrument. Shelf
  agreement goes from 97 to 100 of 104 books, two of the three gained by fixing
  the harness rather than the validator.

## [0.9.7] - 2026-08-05

### Fixed

- **A stray run of text is now reported where it actually is, instead of on the
  element containing it** (#68). Every loose run in a file used to collapse onto
  its parent's single line, column and element path: sixteen findings in one
  real book all said `line 8, col 1` and `/h:html[1]/h:body[1]`. They now carry
  their own positions and `…/text()[n]` paths — identical to what the dedicated
  EPUB 2 rule reported before the grammar absorbed it in 0.7.x.

  No finding was added or removed, so the verdict, the corpus and the shelf are
  all byte-identical; what changed is that a consumer can act on the finding.
  An editor can jump to the text, and a repair tool can wrap *that* run rather
  than guessing which of sixteen candidates was meant. The defect only surfaced
  from downstream use, since every instrument here compares verdicts and counts.

  Internally the blame is a `Blame::Text` variant carrying the run, rather than
  a fourth `ElementFault` carrying its parent — the two disagree about which
  node is at fault, and the type now says which one it is.

## [0.9.6] - 2026-08-04

Three checks epubcheck makes and we did not, one false positive, and the last
implementable rows of the coverage matrix.

### Added

- **A `@font-face` `src` naming a resource the publication does not contain is
  now reported (RSC-007).** Found on a reader's book, where `page_styles.css`
  asked for `fonts/00001.ttf` and no such entry existed: epubcheck reported it
  and we said nothing, because the lookup fell through silently when it missed.

  Which ID applies turns on whether the font is *declared*, not on whether the
  file is *there*: a manifest item whose file is absent is RSC-001's business,
  and only a reference to something nothing declares is RSC-007. Conflating the
  two invents RSC-007s on books that should draw RSC-001 alone.

- **A `<rootfile>` that cannot say where the package document is now gets its
  own message** — OPF-016 when `full-path` is missing, OPF-017 when it is
  empty. Both were previously filtered out in silence, so a `container.xml`
  with a typo'd attribute name reported only "no usable rootfile found" and
  left the reader to work out which of the two mistakes they had made.

  Reported for every `<rootfile>` whatever its media type, as epubcheck does,
  and a whitespace-only path counts as empty.

- **`--advisory` gains ADV-003: a CSS type selector naming an element no
  vocabulary defines.** `h4a { … }` is valid CSS that matches nothing — a typo
  for `h4` or `.h4a` that no tool reports, because nothing is wrong with it as
  CSS. Advisory only; it never affects the verdict, and epubcheck has no
  opinion here.

  The known-name set is derived rather than listed: XHTML names are extracted
  from the grammar this validator itself uses, so the two cannot drift apart.
  **A hyphenated name is always accepted** — HTML requires a custom element's
  name to contain a hyphen, so any such name is legal somewhere and cannot be
  judged from a stylesheet. That single rule is what keeps the check quiet
  enough to be worth having: across 84 real books it produces one finding, and
  that one is genuine.

  Requires styloria 0.8.

### Fixed

- **`<menu>` was rejected in EPUB 3 and should not have been.** HTML5 carries
  it with `<ul>`'s content model and epubcheck accepts it. EPUB 2 is the
  opposite and is unchanged: `menu` lives only in `legacy.rng`, which OPS 2.0.1
  does not include.

### Notes

- The coverage matrix moves to **191 of 194 live epubcheck checks (~98%)**,
  from 189/196. Two of those rows were implemented (OPF-016/017); the other two
  left the denominator after being checked against epubcheck's source *and* its
  test suite — OPF-010 is a dead ID, and PKG-020 is unreachable for whole-EPUB
  input, since the condition it tests is caught earlier and fatally as OPF-002.
  All three remaining gaps are now deliberate scope decisions.

- **RSC-016 is now marked partial rather than complete**, documenting a
  divergence that was already there: for a named HTML entity under an XHTML 1.0
  doctype, epubcheck emits a fatal error per entity and we resolve the entity
  and report the doctype instead. Both tools call such a book invalid; what
  differs is that a fatal there costs every other finding in the file. See
  `docs/COVERAGE.md` for the reasoning.

## [0.9.5] - 2026-08-04

Two more differences from epubcheck, both found by running it against epubveri
over the same books.

### Added

- **Table row groups are now checked for order, and the two EPUB versions want
  opposite orders.** XHTML 1.1 ends its table model in `thead?, tfoot?,
  tbody+`; HTML5 has `thead?, (tbody* | tr+), tfoot?`. So
  `<thead><tfoot><tbody>` is the only valid arrangement in **EPUB 2** and
  `<thead><tbody><tfoot>` the only one in **EPUB 3** — a table that is correct
  in one version is an error in the other. Column groups are ordered with the
  rows too: `<table><tr…/><colgroup…/></table>` is an error in both.

  This is a deliberate exception to the schema's otherwise permissive stance on
  nesting order, taken because the rule is small enough to verify exhaustively:
  all six permutations were built as books in both versions and checked against
  epubcheck — twelve cases, twelve agreements.

### Fixed

- **`CSS-007` no longer fires on EPUB 2 fonts that EPUB 2 allows.** The
  "non-Core-Media-Type font" note was applied version-wide, but EPUB 2 blesses
  a wider set — anything under `font/`, `application/font` or
  `application/x-font`, plus `application/vnd.ms-opentype`. A book declaring
  `application/x-font-truetype` drew an informational note it should not have.
  EPUB 3 is unchanged and still reports it.

## [0.9.4] - 2026-08-04

Six fixes to EPUB 2 validation, found by running **epubcheck itself** against
epubveri over 83 real books and diffing the findings. Two were errors we
reported on valid books; four were things we missed. Agreement on the shelf
went from 74 to 76 of 83 books matching epubcheck's message-ID set exactly.

### Fixed

- **`PKG-005` fired on valid books.** The "mimetype entry must have no extra
  field" check read the ZIP **central directory**; the rule is about the
  **local file header**, which is what epubcheck reads and what a streaming
  reader sees. The two are independent, and tools routinely write an NTFS
  timestamp into the central directory only — one book had 0 bytes local and
  36 central, and was wrongly marked INVALID.

- **`OPF-073` fired on EPUB 2 books.** The DOCTYPE external-identifier check is
  EPUB 3 only; epubcheck's lives in a handler its EPUB 2 path never installs.
  A `version="2.0"` book with a non-SVG-1.1 doctype on an SVG image was
  wrongly marked INVALID.

### Added

- **`id`, `lang` and `xml:lang` are datatype-checked in EPUB 2.** XHTML 1.1
  types `id` as `xsd:ID` (an NCName — no leading digit, no colon) and the
  language attributes as a bare `xsd:language`. One book reported VALID by
  epubveri and 407 errors by epubcheck on these alone; we now match it exactly.

  **EPUB 3 is unchanged and deliberately looser** — HTML5 allows `id="1"` and
  an explicitly empty `lang=""`, so applying the EPUB 2 rule version-wide
  would have invented errors on well-formed EPUB 3 books.

- **`PKG-022` now runs independently of `OPF-029`.** A file mislabelled twice
  over — a `.jpg` name, an `image/jpeg` declaration, PNG bytes — drew only the
  declared-type error, because the file-extension check sat in the other
  branch of the same test. They compare different things and epubcheck reports
  both.

- **EPUB 2's `<spine>` takes only `id` and `toc`.** `page-progression-direction`
  is an EPUB 3 addition and was silently accepted in a 2.0 package.

- **Nine HTML5-only global attributes are rejected in EPUB 2**: `about`,
  `accesskey`, `autocapitalize`, `autofocus`, `content`, `contenteditable`,
  `datatype`, `draggable`, `enterkeyhint`. XHTML 1.1's `Common.attrib` is
  seven attributes and we granted the EPUB 3 set.

  Two of the nine are still accepted where XHTML 1.1 genuinely declares them —
  `content` on `<meta>`, `accesskey` on `<a>` and `<area>` — and ARIA, the
  event handlers, ITS, microdata, RDFa and `role` are untouched.

### Internal

- **A new `compare` harness** runs epubcheck and epubveri over the same books
  and diffs their findings by message ID. Every fix above came from it. It
  needs a JVM and epubcheck's jar, neither of which is a dependency of this
  crate; both live outside the published package.

## [0.9.3] - 2026-08-03

A false-positive fix on `<link>` and `<style>` attributes, in both EPUB 2 and
EPUB 3. Reported by Doitsu on MobileRead (#146).

### Fixed

- **`<link>` had one attribute list serving both grammars**, and it granted
  only `href`, `type` and `sizes`. Measured against epubcheck's own lists by
  building a minimal book per attribute: **EPUB 2 rejected 3 of the 7**
  attributes `link.attlist` allows (`charset`, `hreflang`, `media`), and
  **EPUB 3 rejected 15 of the 19** in `link.attrs` (`media`, `hreflang`, `as`,
  `integrity`, `referrerpolicy`, `crossorigin`, `color`, `disabled`, `scope`,
  `updateviacache`, `workertype`, `imagesrcset`, `imagesizes`,
  `fetchpriority`, `blocking`).

  The two legal sets are **not nested** — XHTML 1.1 has `charset`/`rev`, HTML5
  dropped both and added the fifteen — so the definition is split per version
  rather than widened. Granting the union would have closed the report while
  making each version accept the other's attributes.

  Why it survived this long: `rel` was never in the list at all. It passed
  because RDFa grants it to every element, so the universal
  `<link rel="stylesheet" href="…">` worked and nothing looked wrong until a
  second attribute appeared. It is now explicit rather than a coincidence.

- **`<style media>` was rejected in EPUB 3**, along with `blocking`. 0.8.3
  added `media` to the EPUB 2 copy of this element and missed the EPUB 3 one —
  the same one-directional fix that left `link` shared.

Unchanged on purpose: `sizes` on a `<link>` without `rel="icon"` still errors,
which is epubcheck's own Schematron assert rather than a grammar rule, and
`rev` stays valid on an EPUB 3 `<link>` because RDFa grants it there in
epubcheck's grammar too. Both are pinned by tests, so a future widening of the
attribute lists cannot silently remove them.

Neither instrument could have found this: the epubcheck corpus carries no
fixture with `<link media>` at all, and a scan of 255 real books found zero
uses. Corpus output is byte-identical and the shelf unchanged per book.

### Internal

- **`scripts/preflight.sh`** — runs the publish workflows' guards, plus every
  check this project's own history has earned, *before* the tag is pushed.
  Every guard in CI fires after the tag, which is the wrong side of an
  irreversible upload; 0.9.2's lockfile mismatch would have failed on the
  tagged commit had it not been caught by hand. Checks version agreement
  across both manifests and the CHANGELOG, a clean tree, iCloud conflict
  copies, `git fetch`, whether the version is already published,
  fmt/clippy/`test --locked`/wasm32, `cargo package --list` for stray files,
  and all four instruments.

## [0.9.2] - 2026-08-03

**No library, CLI or WASM changes** — every finding, message and exit code is
identical to 0.9.1. `src/`, `schemas/`, `build.rs` and the wasm bindings are
untouched. A patch, so consumers pick it up (or ignore it) without touching
their manifests.

Measured rather than asserted, because "no change" is the one claim that is
easy to make and easy to get wrong: the epubcheck corpus is byte-identical at
981 scenarios (98.8% exact-ID recall, 4 false positives), and the 73-book
local shelf diffs to no change per book.

### Internal

- **The measurement tooling is Python-free.** `scripts/gen-coverage.py`, the
  last of it, became a third harness binary (`cargo run -p epubveri-harness
  --bin coverage`), after `corpus.py` and the hostile-input pair. Its output
  is byte-identical to the Python original's apart from the line naming the
  generator, which is how the port was verified.

  Two deliberate departures, both closing a hazard rather than copying it: it
  writes `docs/COVERAGE.md` itself instead of being redirected into it (`>`
  truncates the file before the generator produces a byte, so a crash left an
  empty matrix behind a clean-looking shell), and it resolves paths from
  `CARGO_MANIFEST_DIR` rather than the working directory — the trap that made
  `corpus.py` silently produce nothing when run from the repo root.

- **The release guards read the manifest version with `jq`**, not an inline
  `python3` one-liner, in both `publish-crate.yml` and `publish-npm.yml`.
  `cargo metadata` still supplies the JSON, since it is cargo's own parse of
  the manifest — the same source `cargo publish` reads. `jq -e` is
  load-bearing: a bare filter prints nothing and exits 0 when it matches
  nothing, which would compare the tag against an empty string and pass.

- **styloria 0.7.1** is pinned (lockfile only; the `styloria = "0.7"` range
  already admitted it). That release carries no library change either — it is
  the same `jq` fix in styloria's own guard.

## [0.9.1] - 2026-08-03

Validation is now **linear in manifest size**. A 4,000-item package
document went from 42.6 s to 0.52 s, and the worst book on the local shelf
(1,951 items) from 9.4 s to 2.1 s.

No behaviour change: every finding is the same as in 0.9.0, which is what
made this checkable — the corpus stayed byte-identical through all three
fixes, and so did the shelf, per book.

### Fixed

- **The SVG-reference lookup was quadratic** and was the whole of it. The
  `references_svg` half of OPF-015 asked, for each `src`/`href`/`data`/
  `poster` in each content document, whether that target is an SVG manifest
  item — as a scan over the manifest that re-normalized (NFC) every path on
  every attribute. Worse, the scan only stops early when it *finds* an SVG,
  so the worst case was the common one: a book with no SVG at all. 16
  million normalizations on a 4,000-item package, 93% of the run. The set
  of normalized SVG paths is now built once.

- **XPath node-set deduplication was quadratic**, comparing each node
  against every node kept so far. On a 4,000-item package that made
  `//opf:item` an 8-million-comparison node set. Now keyed on node
  identity.

- **`id` uniqueness moved out of Schematron** into `check_duplicate_ids`.
  It was `count($id-set[normalize-space(@id) = normalize-space(current()/@id)]) = 1`
  over `//*[@id]`, which rescans every element for every element; XPath 1.0
  cannot express uniqueness in less than quadratic time. The output is
  byte-identical, deliberately, and `schemas/package.sch` carries a note at
  the place the rule used to be — this is a one-off, not a precedent.

- Schema-level Schematron variables are no longer cloned once per context
  node. Genuinely quadratic when such a variable binds a node set, but
  measured at ~2% here and recorded as such.

### Internal

- Scaling, measured: 1,000/2,000/4,000/8,000 manifest items now cost
  0.13/0.26/0.52/1.02 s — doubling for doubling.
- Four unit tests came with the id-uniqueness port, because its protection
  had to move with it. One caught a real difference immediately:
  `Node::attribute("id")` reads as if it means a no-namespace `id` and does
  not — roxmltree ignores the namespace for a `&str`, so `xml:id` matched
  too. Neither the corpus nor the shelf pairs `xml:id` with `id`, so both
  would have stayed green on a behaviour change.

## [0.9.0] - 2026-08-03

Six ways an ordinary `.epub` could kill the process that validated it, and
one that made it report a hostile book as valid. A validator's inputs are
hostile by definition; nothing here had treated them that way.

None of this was visible to the two instruments this project usually
trusts — the corpus was byte-identical throughout and the shelf unchanged
per book. Both measure *verdicts on well-formed books*, and none of these
inputs is one.

### Fixed

- **Deeply nested XML aborted the process.** roxmltree's tokenizer is
  mutually recursive (`parse_element` ↔ `parse_content`), so nesting costs
  stack in proportion to depth: a **1.1 KB** file was enough — ~15,000 deep
  on the 8 MiB main thread, ~4,000 on a 2 MiB worker, which is what an
  embedder actually runs, and lower again under wasm. A Rust stack overflow
  is `SIGABRT`, **not** a catchable panic, so `catch_unwind` could not save
  a consumer; it has to be refused before the parser sees it.

  The guard is a raw-byte pre-parse scan that skips comments, CDATA
  sections and quoted attribute values. All three matter: nesting hidden in
  a comment would walk straight past it, and a `>` inside an attribute
  value (`<a title="a>b">`) would hide a self-closing slash and over-count
  a *valid* document into a false positive.

  `MAX_XML_DEPTH` is 256, against a measured worst case of **24** across 65
  real books (median 8, p95 11).

- **A compressed entry could be inflated without bound.** `Ocf::read` did a
  bare `read_to_end`, so a **400 KB** EPUB drove **1.3 GB** of peak RSS —
  and still reported `VALID`. The cap sits on the read rather than on the
  central directory, so an entry whose header lies about its uncompressed
  size cannot widen it either. 64 MiB, against a measured worst case of
  2.1 MB (largest single entry across the same 65 books).

- **Deeply nested CSS aborted the process**, in four shapes — nested
  parens, nested `@media`, nested functions, nested `:is()` — each from a
  ~1.2 KB stylesheet. Fixed upstream in styloria 0.7.0, whose parsers are
  now bounded by `MAX_NESTING_DEPTH`; real stylesheets nest **2** deep.

### Added

- **`LIM-001`** — a resource exceeded the size limit and was not checked.

  A new epubveri-owned ID family, owned for the same reason `ADV-*` is:
  epubcheck defines no code here because it has no such limit, and
  inventing a `PKG-0xx` it does not define is what `ids.rs` forbids. Unlike
  `ADV-*` these are **not** advisory — always reported, and they do affect
  the exit code, because each one means a resource went unchecked.

  Refusing a resource is reported, never silent: a bare `None` would be
  indistinguishable from "absent" at all ~25 call sites, and a book with
  real errors would have reported clean — the exact failure the
  0.7.12–0.7.14 audits kept turning up.

- CSS nesting refusals surface as `CSS-008` with their own rule slug,
  `css.stylesheet.nesting_too_deep`. The other CSS-008 slugs say the
  stylesheet is malformed; this one says the parser declined to descend and
  the content below it went unchecked.

### Changed

- **`styloria` 0.6 → 0.7** (the nesting bound above). Embedders pinning
  styloria alongside epubveri need the same bump; nothing in epubveri's own
  API changed.
- **`tsify-next` → `tsify`** in the wasm bindings. RUSTSEC-2025-0048 flags
  the fork as unmaintained, and the roles have swapped since it was chosen:
  upstream resumed (0.5.6, Oct 2025) while the fork's last release was Apr
  2025. Same version, same `js` feature, same attribute — the generated
  `.d.ts` is unchanged. No API change for JS consumers.
- 15 dependency patch updates.

### Internal

- **`cargo audit` runs in CI**, in its own job. Its result depends on the
  RustSec advisory database rather than on any commit here, so it can turn
  red without this repo changing; a separate job means the red X names the
  cause. Warnings do not fail it, only advisories. The tree is currently
  clean of both.

### Known issue

- **Validation is superlinear in manifest size.** 500 items take 0.37 s,
  4,000 take 39 s — doubling the manifest roughly sextuples the time. It is
  the RELAX NG derivative engine over the manifest's repeated `item`
  children, not the content documents (a book with 4,000 resources and one
  document costs the same as one with 4,000 documents) and not Schematron
  (EPUB 2, which has no package Schematron, shows the identical curve).

  Pre-existing, not a regression in this release. It bites one book in 73
  on the local shelf — 1,951 items, 9.4 s — while the median book (35
  items) is instant. Next on the list.

  **Fixed in 0.9.1 — and the diagnosis above was wrong twice.** It was not
  the RELAX NG derivative engine (that pattern stays a constant 39 nodes
  across 4,000 children), and EPUB 2 *does* run the package Schematron, so
  the identical curve there was never evidence of anything. Left standing
  rather than quietly edited, because a shipped release's notes are a
  record: see 0.9.1 for what it actually was.

## [0.8.6] - 2026-07-31

Four package vocabularies that were never checked, and the first two EPUB
3.4 core media types.

### Added

- **`image/avif` and `image/jxl` are Core Media Types.** Both are in EPUB
  3.4's core media types table (spec change log: AVIF 06-Oct-2025, JPEG XL
  23-Jan-2026), so a manifest item declaring either no longer needs a
  fallback and an `<img>` referencing one is no longer `RSC-032`. EPUB
  3.4's audio additions — Opus in an MP4 container, and the codec-bearing
  type for AAC LC — needed no change, since `audio/mp4` was already
  accepted and the media-type parameter is stripped before comparison;
  they are now covered by a test so that cannot silently regress.

  This ships **ahead of epubcheck**, which has an open issue for AVIF and
  none for JXL. The practical difference: a publication targeting EPUB 3.3
  that uses AVIF draws a fallback error there and none here.

- **Unknown package property names are now reported (`OPF-027`)** in the
  four positions that had no vocabulary check at all. Manifest
  `item/@properties` was already checked; these are the rest:

  | position | vocabulary |
  |---|---|
  | `meta/@property` | the 16 unprefixed names, plus `pageBreakSource` (EPUB 3.4) |
  | `meta/@property`, `media:` | `active-class`, `duration`, `narrator`, `playback-active-class` |
  | `itemref/@properties` | `page-spread-left`/`-right`, plus the 18 `rendition:` overrides |
  | `link/@rel` | the 9 defined keywords |

  A typo such as `belongs-to-colection` used to pass silently. **A
  prefixed name is deliberately left alone** — an author-declared prefix
  carries a vocabulary this tool cannot know, and an *undeclared* prefix
  is `OPF-028`, a different message. The five deprecated `link/@rel`
  keywords remain members of their vocabulary, so they keep drawing only
  their existing `OPF-086` deprecation warning rather than gaining a
  second finding.

  EPUB 3 only: neither `property` nor `itemref/@properties` is an EPUB 2
  attribute, and the EPUB 2 package grammar already reports them.

## [0.8.5] - 2026-07-31

Two EPUB 2 content-model gaps, from one MobileRead post by Doitsu (#140)
and the audit it prompted. Both are findings epubcheck reports and we did
not; nothing here changes EPUB 3 behaviour.

### Fixed

- **`<blockquote>` is block-level in an EPUB 2 document.** XHTML 1.1 gives
  it the same `Block.model` as `<body>`, so an inline element, a `<br/>` or
  loose text directly inside one is an `RSC-005`, and an empty one is
  reported as incomplete. The reported case — an `<a>`, a `<br/>`, a
  `<span>` and bare text inside a `<blockquote>` — now draws all five of
  the errors epubcheck draws, where it previously drew none.

  **`<noscript>` has the same model** and had the same gap. Those two and
  `<body>` are the only three elements in the whole XHTML 1.1 module set
  that use `Block.model`, so this closes the class rather than the case.

- **`<math>` is no longer accepted in an EPUB 2 document.** OPS 2.0.1 has
  no MathML — the EPUB 2 schema set never includes a MathML grammar — so
  epubcheck reports `<math>` there as `RSC-005`, and the expected-element
  list it prints names `svg` and no `math`. Ours listed `math`, which was
  both a wrong suggestion and a missing error: MathML had been in the
  EPUB 2 element pool since that pool was first written, and was the one
  element in it with no basis in the EPUB 2 schemas. EPUB 3 is unaffected.

  The MathML content-model checks are deliberately *not* switched off for
  EPUB 2 to match: every `<math>` in an EPUB 2 document is now an error
  from the grammar itself, so the extra detail can only ever land on a
  document that is already invalid — and suppressing a check on the
  grounds that another one covers the case is the shape behind three
  earlier silent-skip defects.

## [0.8.4] - 2026-07-30

Three findings epubcheck reports and we did not, all from one MobileRead
post by Doitsu (#138).

### Added

- **`mathml` is now cross-checked against the manifest, in both
  directions.** A content document containing `<math>` without the `mathml`
  property draws `OPF-014`; the property declared on a document with no
  MathML draws `OPF-015`. The property was already *accepted* (declaring it
  never drew `OPF-027`), so it was the one member of the item-property
  vocabulary that no rule looked at.

- **Attribute vocabulary for SVG subtrees** (`RSC-025`, usage). An
  unprefixed attribute that SVG 1.1 has no concept of is now reported —
  `<image alt="cover image">`, HTML's `alt` reaching into an SVG cover
  page, being the reported case. Like the existing element check this is
  usage-level and a flat vocabulary rather than a per-element table, so an
  attribute used on the wrong SVG element still passes. Two case-mangled
  spellings on a 65-book shelf (`viewbox`, `preserveaspectratio`) are the
  only other things it found there.

### Fixed

- **`<html class="calibre">` is now an error in an EPUB 2 document**, as are
  `id`/`style`/`title` on `html`, `head` or `title`. XHTML 1.1 builds those
  three elements from `I18n.attrib` alone — `dir`, `lang`, `xml:lang`, plus
  `version` on `html` and `profile` on `head` — and not from
  `Common.attrib`, so none of them takes `class`. This is calibre's own
  output, so expect it on real EPUB 2 books: two of the 65 on the local
  shelf gained findings, 61 of them in one book.

## [0.8.3] - 2026-07-28

### Fixed

- **Six XHTML 1.1 attributes are no longer rejected in EPUB 2 documents**:
  `style@media`, `meta@scheme`, `base@target`, `head@profile`,
  `html@version` and `q@cite`. `<style type="text/css" media="screen">` is
  ordinary markup; the rest are rarer but equally valid.

  Found by diffing our EPUB 2 attribute lists against epubcheck's own
  XHTML 1.1 modules, rather than by waiting for someone to report them —
  which is how the two attributes in 0.8.2 arrived. The same audit found a
  larger gap in the other direction (attributes we accept and XHTML 1.1 does
  not); that one is deliberately not in this release, because it is a
  tightening whose effect on real books has to be measured first.

### Internal

- CI now gates on `cargo clippy -D warnings`, and the 41 findings that stood
  in the way are cleared. Twenty of them were doc-comment list continuations
  that rustdoc was rendering outside their list item, so that lint was
  reporting a real documentation defect. No behaviour change.

## [0.8.2] - 2026-07-28

Two false positives removed and four EPUB 2 checks added, all of them from
MobileRead reports — Doitsu's CSS case, and DNSB posting epubcheck's output
next to ours for the same book.

### Fixed

- **An attribute selector inside `@media` is no longer a CSS syntax error.**
  `@media print { a[href^="http"] { … } }` drew CSS-008: the walk over a
  grouping at-rule's block treated the `[…]` of a *selector* as a rule body
  and read its contents as declarations. Reported by Doitsu with a namespaced
  selector, but the namespace was incidental — every attribute selector in
  that position was affected.
- **NAV-001 is no longer emitted.** It is unreachable in epubcheck: the only
  call site needs an EPUB 2 book whose manifest item carries `properties`,
  and only the EPUB 3 handler parses that attribute. We were reporting a
  finding epubcheck cannot make. An EPUB 2 book carrying a navigation
  document is still reported — through the content model, where `<nav>` is
  not part of XHTML 1.1, which is how epubcheck reports it.
- **`epub:type` and `meta@charset` are rejected in EPUB 2.** Both are EPUB 3
  spellings; the EPUB 2 branch had been reusing the EPUB 3 attribute pools.
- **An EPUB 2 package document is checked against the EPUB 2 shapes.** A
  `<meta>` needs `name` and `content` and must be empty; `properties` is not
  an attribute of `item` or `itemref`. That grammar had no version switch at
  all, so an EPUB 2 package was being held to EPUB 3's rules.
- **An EPUB 2 `<body>` must hold at least one block element**, so a document
  whose every child is rejected now says so, rather than listing the children
  alone.
- Attributes are named in full in diagnostics: `epub:type` was being reported
  as `attribute "type"`, which on an `<a>` sends the reader to an attribute
  that is perfectly legal there.

### Added

- **ADV-004** (advisory, opt-in): a package document that declares EPUB 2 but
  is written in EPUB 3. It reports the pile of findings such a book already
  draws as one diagnosis, naming the signals it counted. Suggested by JSWolf;
  the books DNSB described are the case it is for.

## [0.8.1] - 2026-07-27

`<hgroup>` matched to epubcheck's content model, in both directions.

### Fixed

- **`<hgroup>` accepts `<p>`** — a heading with a subtitle paragraph
  (`<h1>Frankenstein</h1><p>Or: The Modern Prometheus</p>`) was drawing
  RSC-005. That is the canonical modern shape and epubcheck accepts it.
  Reported by Doitsu on MobileRead.

### Changed

- **`<hgroup>` now holds exactly one heading**, which is the other half of
  epubcheck's model: since 2022 the subtitle is a `<p>`, not a second
  heading. The older `<h1>` + `<h2>` pairing was accepted here and is
  rejected by epubcheck, so books written to the old model will start
  drawing RSC-005 — the same finding epubcheck gives them. Surfacing it is
  all a validator can do; correcting the markup belongs to whatever produces
  the book.

## [0.8.0] - 2026-07-26

A minor bump rather than a patch: the library API changed. See **Breaking**
below — embedders need `..Options::default()` in a `Options` literal, and
`opf::check` now takes `&Options`. The CLI and the WASM bindings are
source-compatible; nothing an existing invocation or JS call does changes
meaning.

### Added

- **`-v` / `--epub-version <2|2.0|3|3.0>`: validate against a version the book
  doesn't declare** (#61), the same flag epubcheck spells `-v`. On a
  disagreement **PKG-001** reports it and the requested version wins — as
  epubcheck does, so a ported invocation means the same thing here. Expect a
  long report when the two disagree: a 3.0 book checked as 2.0 breaks a great
  many EPUB 2 rules, all of them really one finding. Exposed to embedders as
  `Options::epub_version` and as `validate()`'s fourth argument in the WASM
  bindings.
- Coverage is now **190 of 197 live epubcheck checks (~96%)**.

### Breaking

- `Options` gained a field, so a struct literal that lists every field no
  longer compiles — add `..Options::default()` (which is now what this crate's
  own helpers do).
- `opf::check` takes `&Options` in place of its separate `profile` and
  `advisory` parameters. This is the second time that signature had to change
  for a new option, and each change breaks embedders over something they don't
  care about; passing the struct means the next option costs them nothing.

### Fixed

- **PKG-023 now keys on the version being validated against**, not the one the
  package document declares — so forcing an EPUB 3 book to EPUB 2 with `-v`
  correctly reports that its profile does not apply. (It landed in 0.7.14
  keyed on the declared version, which was right until `-v` existed.)

## [0.7.14] - 2026-07-26

Two files that could be skipped in silence, and the coverage matrix reaching
~96%.

### Added

- **PKG-023**: asking for a validation profile (`--profile`) on an EPUB 2 book
  now says the profile doesn't apply, instead of silently ignoring it.
  Profiles are an EPUB 3 feature.
- **PKG-018**: an input path that doesn't exist is reported as a finding with
  its epubcheck message ID — so the JSON output carries it like any other —
  rather than as a bare stderr line. The exit code is unchanged (`2`).

### Changed

- **Coverage is now 189 of 197 live epubcheck checks (~96%)**, up from
  187/198. Two of those come from the checks above; the third move is
  PKG-015 leaving the denominator. It is a dead ID: "unable to read EPUB
  contents" exists only in epubcheck's severity table and translation
  bundles, with no Java source line that emits it and no scenario expecting
  it. PKG-001 stays counted as a gap, and its note now says why it is a
  missing *feature* rather than a missing check — it can only fire when the
  caller demands a specific EPUB version that disagrees with the book's own,
  and epubveri has no version-override flag.

### Fixed

An audit of every place a check stays quiet on the grounds that another check
owns the case. Two of those arrangements had a gap, and a gap between two
checks reports *nothing* — the one failure a user cannot notice.

- **A malformed numeric character reference no longer silences a whole
  content document.** `&#0;`, `&#;`, `&#zz;` and an unterminated `&#38` all
  fail the XML parse as entity errors, and the parse-failure branch suppressed
  entity errors on the grounds that the raw entity scan reports them — but
  that scan reads named references only. Measured: a book with a broken image
  reference *and* a `&#0;` reported VALID. The suppression now asks whether a
  finding was actually produced instead of assuming the other check covers the
  class, so this cannot recur in a shape nobody thought of.
- **A malformed NCX, Media Overlay, `META-INF/metadata.xml` or rendition
  mapping document is now reported (RSC-016) instead of skipped.** Nothing
  else parses any of these four files, so a `</navMapX>` typo took every NCX
  check with it and the book validated clean. `META-INF/container.xml`,
  `encryption.xml` and `signatures.xml` already reported; these four were the
  outliers.

## [0.7.13] - 2026-07-26

The headline is a silent one: a content document could be dropped from every
check because of a single `&nbsp;`. Also completes MathML validation and two
CSS/schema checks.

### Fixed

- **An element with incomplete content no longer stops its siblings being
  checked.** A body containing four empty `<ol>`/`<ul>`/`<table>`/`<dl>`
  reported one error where epubcheck reports four, so the same file had to be
  re-run once per fix (#60).
- **A missing `<title>` is now an error on EPUB 2**, matching XHTML 1.1, whose
  `head` content model simply *is* `title`. It stays a warning on EPUB 3,
  where epubcheck's own rule is a warning-level Schematron assertion.

- **A content document using `&nbsp;` under an XHTML 1.0 doctype is no longer
  skipped entirely.** The entity was reported as undeclared — a *fatal* — and
  the document then failed to parse, so every other check on it was silently
  dropped. On one real book the affected file produced 15 fatals and one other
  finding where its siblings each produced around 300. XHTML 1.0 Strict and
  Transitional declare the HTML named entities just as 1.1 does; a separate
  question from whether the doctype is the one EPUB 2 wants, which is still
  reported as `HTM-004`.

### Added

- **MathML Presentation content models are now validated** — `mfrac` takes
  exactly two children, `msubsup` exactly three, rows and cells exist only
  inside their container, and so on. Attribute values are left unconstrained.
  Verified against two independently-produced real books carrying 257,000
  MathML elements between them, both of which stay clean.
- **Malformed `U+` unicode-ranges are reported as `CSS-008`** — a run of more
  than six hex digits, which is more than any code point needs. This was the
  last item in `CSS-008`, so that check is now complete against epubcheck's
  live CSS error surface.

### Changed

- styloria dependency `0.5` → `0.6`.

## [0.7.12] - 2026-07-26

Three fixes from one MobileRead report by Doitsu. One of them is serious: a
single stray character could silently disable validation of a whole content
document.

### Fixed

- **A bare `&` no longer makes a content document skip validation entirely.**
  `<p>Tom & Jerry</p>` is malformed XML, but the error fell between two
  checks — the parse-failure path defers entity errors to the raw-text scan,
  and that scan skipped a `&` with no entity name after it. The document then
  failed to parse and *every* check on it was quietly dropped, so a book with
  other real errors in the same file could still report as valid. It is now
  reported as `RSC-016`, phrased as the fix rather than as the parser's
  complaint: **"a bare '&' must be written as '&amp;'"**.
- **`properties="svg"` is no longer an error when the document only
  *references* an SVG.** The property is required when a document contains SVG
  markup and merely *permitted* when it links to an SVG resource, so declaring
  it on a document whose only SVG is an `<img src="…svg"/>` is optional, not
  wrong. A missing declaration on a document with inline `<svg>` is still
  `OPF-014`.

### Added

- **Empty `<ol>`, `<ul>`, `<dl>` and `<table>` are reported on EPUB 2.**
  XHTML 1.1 requires content in all four. HTML5 does not, so this applies to
  EPUB 2 books only — an empty list in an EPUB 3 book stays valid.

## [0.7.11] - 2026-07-26

Three false positives and a duplicate-reporting fix, all found by running
epubveri over real books rather than test fixtures. No verdict changes on the
epubcheck test corpus.

### Fixed

- **HTML5-only elements are no longer reported twice on EPUB 2 books.** A
  `<figure>` or `<section>` was flagged once by a hand-coded list and once by
  the EPUB 2 grammar, at the same line and column, with two different
  wordings. On a real book with 47 `<figure>` elements that was 94 errors
  where there should have been 47. The grammar rejects all 26 elements of the
  hand-coded list, in every position, so the duplicate is gone and the
  remaining message is the more useful one — it names what was expected.
- **Valid Presentation MathML is no longer reported as Content MathML.** Nine
  elements were missing from the recognised vocabulary: the elementary-
  mathematics family (`mstack`, `mlongdiv`, `msrow`, `msline`, `msgroup`,
  `mscarries`, `mscarry` — long division and column arithmetic) and the
  alignment markers (`maligngroup`, `malignmark`). A textbook typesetting a
  long division was told its markup was the wrong kind of MathML.
- **`epub:type` in SVG now uses epubcheck's own allowlist** of renderable
  elements instead of an inverted list, which had let it pass on `marker`,
  `linearGradient`, `clipPath`, `mask`, `pattern` and others.

### Added

- **`id` is checked against HTML5's rule** — one or more characters, none of
  them whitespace — so an empty `id=""` or one containing a space is now
  reported. Measured first: across 61 real books and 28,778 `id` values there
  was not one violation, so this is a guard rather than a change in verdicts.

## [0.7.10] - 2026-07-26

**If you validate EPUB 2 books, upgrade.** Seven false positives fixed — cases
where epubveri rejected markup that epubcheck accepts. All were EPUB 3 rules
leaking into the EPUB 2 branch, found by auditing our rules against
epubcheck's own `schema/20` instead of waiting for reports. No verdict changes
on the epubcheck test corpus.

### Fixed

- **`<a name="…">` no longer draws `RSC-005`.** The classic pre-`id` anchor
  form, so internal links in a large share of older EPUB 2 books were being
  rejected. OPS 2.0.1 assembles `a`'s attribute list from four modules and
  epubveri carried only one of them; `target`, `charset`, `rel`, `rev`,
  `shape`, `coords` and `usemap` were missing for the same reason, and `img`
  was missing `longdesc`, `name`, `shape` and `coords`.
- **An empty `<title>` is no longer an error on EPUB 2.** XHTML 1.1 types
  `<title>` as text, which permits empty content; epubcheck asserts non-empty
  only for EPUB 3.
- **`lang` and `xml:lang` may differ on EPUB 2.** XHTML 1.1 declares them as
  independent attributes with nothing tying their values together.
- **A nested `<dfn>` is no longer an error on EPUB 2** — that rule is EPUB 3
  only.
- **The legacy `<tours>` package child is accepted**, as `opf20.rng` allows.

### Added

- **Six NCX checks**, completing epubcheck's `ncx.sch`: the `playOrder`
  sequence must start at 1 and have no gaps, elements naming the same target
  must share a `playOrder`, a `pageTarget`'s `value`+`type` combination must
  be unique, and sibling `navLabel`/`navInfo` elements must not repeat an
  `xml:lang`. All `RSC-005`, and all reported per offending element.
- `scripts/scan-shelf.sh` — runs epubveri over a directory of real EPUBs and
  summarises verdicts, error-level findings, and the number of *distinct*
  messages per ID. The corpus cannot find false positives; real books can.

## [0.7.9] - 2026-07-25

One new class of finding, from work in the sibling `styloria` crate. No
verdict changes on the epubcheck test corpus.

### Added

- **Malformed CSS selectors are now reported as `CSS-008`.** styloria 0.5
  reads a qualified rule's prelude as a selector list, so `a > > b { }`,
  `[href=] { }` and `, p { }` are flagged instead of silently accepted. The
  check is syntactic only — it never asks whether an element, pseudo-class or
  attribute *name* is real — and is deliberately permissive about anything
  newer than it (`&` nesting, `::part()`, unknown pseudo names, and the whole
  inside of `:not()`/`:is()`/`:has()`/`:nth-child()`), because a wrong error
  on a valid stylesheet costs more than a missed one. Findings carry their own
  `css.stylesheet.invalid_selector` rule, so "the selector is malformed" and
  "the declarations are malformed" are distinguishable even though epubcheck
  reports both as `CSS-008`.

### Fixed

- **A stylesheet starting with a UTF-8 BOM is no longer mis-parsed.** The BOM
  was left in the token stream, which turned a following `@charset` into a
  qualified rule's prelude and cascaded errors through the rest of the file.
  Fixed in styloria 0.5's tokenizer; `decode_bytes` already handled UTF-16
  BOMs, and this was the UTF-8 case falling through.

### Changed

- styloria dependency `0.4` → `0.5`.

## [0.7.8] - 2026-07-25

Six message IDs from a sweep of the coverage matrix (`docs/COVERAGE.md` — now
187 of 198 live epubcheck checks, ~94%), plus a false positive found while
implementing one of them. No verdict changes on the epubcheck test corpus.

### Added

- **`HTM-045`** (usage) — an empty `href=""` resolves to the containing
  document. Legal, so a hint rather than an error.
- **`OPF-067`** — a resource that is both a metadata `<link>` target and a
  manifest item. As in epubcheck, only when that item is not in the spine.
- **`PKG-017`** (warning, EPUB 2) / **`PKG-024`** (usage, EPUB 3) — the
  container's own file extension is not `.epub`. The existing `PKG-016` still
  covers the right extension in the wrong case (`.EPUB`).
- **`OPF-005`** — a `prefix` declaration ending in a prefix name with no URI
  after it. Reported *instead of* the `OPF-004` syntax error, matching
  epubcheck.
- **`OPF-006`** — a `prefix` declaration whose URI half does not parse.
  Deliberately conservative, matching Java's `new URI(...)`: illegal
  characters and malformed percent-escapes only.

### Fixed

- **`OPF-052` no longer rejects valid `dc:contributor` roles.** The check ran
  on `dc:creator` *and* `dc:contributor`; epubcheck only ever checks
  `dc:creator`, so a contributor role it accepts was an error here.
- **`OPF-052` now checks membership in the real MARC relator list** (the 273
  codes epubcheck itself carries, plus its `oth.` escape hatch) rather than
  approximating it as "three lowercase ASCII letters", which accepted any
  invented code such as `xyz`.

### Changed

- **`Report` gained an `epub_version` field** — the `version` the package
  document declared, or `None` when no OPF was reached. `PKG-017`/`PKG-024`
  need it (the ID and severity depend on the EPUB version, but the filename
  they judge is known only to the file-level entry point, which never sees an
  OPF); consumers get it too. Additive, and `Report` still derives `Default`,
  but code constructing a `Report` by struct literal will need updating.
- **Four IDs reclassified in `docs/COVERAGE.md` as not-live checks**, verified
  against epubcheck's source: `OPF-011` and `OPF-036` are dead IDs (the former
  commented out in favour of the `RSC-005` we already emit, the latter with no
  call site at all), `RSC-024` is the non-normative half of a pair we have no
  counterpart for, and `OPF-021`'s only call site is DTBook content, not the
  OPF as its note had claimed.

## [0.7.7] - 2026-07-24

**No changes to the validator.** Identical checks, identical behaviour,
identical results on the epubcheck test corpus. Upgrading from 0.7.6 gains
nothing on that front; this release exists to exercise the new tag-triggered
release pipeline, and to correct two packaging details below.

### Changed

- Releases now publish from CI. A `v*` tag builds the binaries and publishes
  to both crates.io and npm, authenticating by **trusted publishing (OIDC)**
  rather than stored tokens — nothing long-lived to leak, and nothing typed by
  hand. Guards run before each irreversible upload: the tag must agree with
  the manifests, an already-published version is a no-op, and the test suite
  must pass against the tagged commit.
- **The npm package now carries a provenance attestation**, generated
  automatically when publishing from CI, so consumers can verify which commit
  and workflow produced the `.wasm` they are running.
- **The npm package no longer reports itself as a dirty build.** 0.7.6 was
  built on a machine with an unrelated uncommitted file present, so
  `version()` returned `0.7.6+45ae97a.dirty`; the code was exactly the tagged
  source, but the suffix said otherwise. CI checks the tag out clean, and the
  local publish script now refuses to build a release from a dirty tree.

## [0.7.6] - 2026-07-24

A single fix from a MobileRead bug report. No new checks; EPUB 3 behaviour
and the epubcheck test-corpus numbers are unchanged.

### Fixed

- **XHTML 1.1 table attributes no longer draw `RSC-005` on EPUB 2** (#47,
  reported by Doitsu on MobileRead). The EPUB 2 table subtree carried the
  global attributes only, so the Tables Module's own attributes were rejected
  on valid markup: `colspan`/`rowspan`/`headers`/`scope`/`abbr`/`axis` on a
  cell, `span`/`width` on a `col`/`colgroup`, `align`/`char`/`charoff`/`valign`
  on rows and row groups, and `summary`/`width`/`cellspacing`/`cellpadding`/
  `frame`/`rules` on the table. `table/@border` was restricted to HTML5's
  `""`/`"1"` and now takes any value, as XHTML 1.1's `Pixels` does. Same class
  of gap as #43. EPUB 3 keeps HTML5's stricter rules unchanged — `width` on a
  `<col>` is still an error there, as in epubcheck.

## [0.7.5] - 2026-07-24

More checks from the coverage-matrix review (`docs/COVERAGE.md` — now ~90% of
live epubcheck checks). Additive; no verdict changes on the epubcheck test
corpus.

### Added

- **`OPF-044`** — a spine item with a non-content media type whose fallback
  chain exists but never reaches a content document is now reported separately
  from **`OPF-043`** (no fallback at all), matching epubcheck's two IDs. Both
  stay errors; the ID just sharpens.
- **`NAV-004`–`NAV-008`** — EDUPUB nav-completeness (usage): the navigation
  document's heading hierarchy is incomplete (`NAV-004`), or content documents
  contain `<audio>`/`<figure>`/`<table>`/`<video>` but the nav has no matching
  `loa`/`loi`/`lot`/`lov` list (`NAV-005`–`NAV-008`). Gated on
  `dc:type="edupub"`, like the existing `NAV-003`.

### Fixed

- **Image error mapping now matches epubcheck.** `MED-004` is reserved for a
  file too short to contain a 4-byte header; a ≥4-byte file whose header
  matches no known format is a declared/actual mismatch (`OPF-029`), not
  `MED-004`, as it was before. `PKG-021` still covers unreadable content.

## [0.7.4] - 2026-07-24

Three new checks and a broader `CSS-008`, found while building a per-message-ID
coverage matrix against epubcheck (`docs/COVERAGE.md` — now ~87% of live
epubcheck checks). All additive; no verdict changes on the epubcheck test
corpus.

### Added

- **`CSS-006`** — a `position: fixed` declaration is now flagged (usage),
  matching epubcheck (a valid CSS property EPUB discourages, like the existing
  `CSS-001` `direction`/`unicode-bidi` check).
- **`NAV-001`** — an EPUB 3 navigation document (`properties="nav"`) declared
  in an **EPUB 2** publication is now flagged; the nav document is not a valid
  EPUB 2 construct.
- **`NCX-004`** — leading/trailing whitespace in the NCX `dtb:uid` metadata is
  now flagged (usage), for both EPUB 2 and EPUB 3.

### Changed

- **`CSS-008` now also detects unterminated rules and blocks**, not just bad
  string/url tokens, by consuming styloria 0.4's new `syntax_errors` API
  (which surfaces the parse errors its recovering parser previously discarded
  silently). Still a subset of epubcheck's full CSS-parser error surface.
- Depends on **styloria 0.4** (up from 0.3).

## [0.7.3] - 2026-07-24

### Fixed

- **`epub:type` terms from the dictionary, comics and structure vocabularies
  are no longer reported as `OPF-088`.** The default (unprefixed) `epub:type`
  vocabulary is epubcheck's *aggregate* of the Structure, Data-Navigation,
  Dictionary, Index and Comics vocabularies; 29 current terms from it were
  missing, so valid values such as `biblioref`, `dictionary`, `dictentry`,
  `part-of-speech`, `balloon` and `panel` were wrongly flagged
  "not in the default vocabulary." The misspelled `concludingsentence` is
  also corrected to `concluding-sentence` (the real term). (Reported by
  Doitsu on the MobileRead forum.)
- **`img`/`iframe`/`object` `width` and `height` percentages are no longer
  reported as `RSC-005` in EPUB 2.** XHTML 1.1 types these as the `Length`
  datatype (pixels *or* a percentage), which epubcheck's EPUB 2 schema
  accepts as free text, so `<img width="50%">` is valid there. The stricter
  HTML5 rule (a non-negative integer only) still applies in EPUB 3, where a
  percentage remains an error — matching epubcheck on both. (Reported by
  Doitsu on the MobileRead forum.)

### Changed

- **`OPF-096` now points at the offending `<itemref linear="no">`** rather
  than the package root, and its wording spells out the cause — a non-linear
  content document with no hyperlink pointing to it is unreachable from the
  reading order. (Reported by Doitsu on the MobileRead forum.)

## [0.7.2] - 2026-07-24

### Fixed

- **A space in a URL's path or query is no longer reported as `RSC-020`.** A
  trailing space in a content-document `<a href>` query — e.g.
  `https://www.youtube.com/watch?v=…XFc. ` — was wrongly reported as a hard
  error while epubcheck stayed silent. EPUB references the WHATWG URL
  Standard, whose parser strips leading/trailing spaces and percent-encodes
  an interior path/query space, so such a URL is a *valid URL string* in
  practice; epubcheck's own fixtures accept it. A space in the **host** (which
  genuinely can't be parsed, e.g. `www.example .com`) is still an error, as is
  a space in a manifest item's local file path. (Reported by patrik on the
  MobileRead forum.)

## [0.7.1] - 2026-07-24

### Fixed

- **`file:` URLs in a standalone SVG's stylesheet forms are now flagged
  (`RSC-030`).** `file:` URLs were already rejected in CSS `url()`/`@import`
  and in XHTML content-document `href`/`src`, but a standalone SVG content
  document scans its own stylesheet forms — the `<?xml-stylesheet?>` PI, an
  inline `<style>`'s `@import`, and a `<link rel="stylesheet">` — and those
  were only checked for *remote* URLs (`RSC-006`), never `file:`. **Verdict-
  affecting on that specific input**: an SVG that slipped a `file:` stylesheet
  reference through is now correctly reported invalid, matching epubcheck.
  (Corpus exact-ID recall 599 → 600/607.)

## [0.7.0] - 2026-07-24

Follow-ups to the 0.6.0 attribute-allowlist work. Two of them change
verdicts (see each entry), which is why this is a minor bump.

### Fixed

- **Scripted-content detection (`OPF-014`/`OPF-015`) now keys off the right
  things.** `has_script` — the flag behind the "scripted" content-property
  checks — used to be set by the mere presence of any `<input>`/`<button>`/
  `<select>`/`<textarea>` element, and never looked at `on*` event-handler
  attributes. So `<input required>` wrongly reported `OPF-014` while
  `<span onclick="…">` did not. It now matches epubcheck: a document is
  scripted when it has a `<form>` element, javascript (a `<script>`), or any
  `on*` event-handler attribute — not the bare presence of a form control.
  **This changes verdicts**: a book whose only "scripting" was a form control
  is no longer treated as scripted (so a declared-but-otherwise-unused
  `scripted` property now draws `OPF-015`, and an undeclared one no longer
  draws `OPF-014`), while a book that scripts only through `on*` handlers is
  now correctly detected. (#37, found during the 0.6.0 epic.)

### Added

- **`<dialog>` and `<search>`** are now recognized (EPUB 3). Both are HTML5
  flow-content elements epubcheck accepts but the grammar was missing
  entirely — `dialog` (with `open`) and `search`. **This changes verdicts**:
  a document using either was reported invalid before and is now accepted.
  EPUB 3 only; XHTML 1.1 (EPUB 2) predates both, so they stay rejected there.
  (#40.)

### Internal

- Regression-guard tests for two engine properties that 0.6.0's cutover
  settled but left untested: `<anyName>` matches names in any namespace while
  `<nsName ns="">` matches only unnamespaced ones (#38, spec-correct — not a
  bug), and a heavily-attributed element validates in constant time now that
  removing the permissive wildcard eliminated the attribute-matching
  ambiguity that used to make it exponential (#39). No behavior change.

## [0.6.0] - 2026-07-23

Unknown and mistyped XHTML attributes are now rejected (#31). **This changes
verdicts, broadly**: an attribute name that isn't valid on its element — a
made-up one (`fake`) or a typo of a real one (`clas` for `class`) — is now
reported invalid (`RSC-005`), matching epubcheck, where every earlier release
accepted it silently. `<p fake="fake" clas="header">` draws two `RSC-005`
errors, one per attribute. This was reported as a production blocker by Doitsu
on the MobileRead forum (#110): a validator has to reliably reject invalid
attributes, not just a hand-picked obsolete set (the narrower thing 0.5.17
did). Unknown *elements* were already rejected; this closes the same gap for
attributes. The epubcheck corpus is unchanged (verdict-neutral there — its
fixtures are all valid attributes), but real books with an attribute typo will
newly fail, which is the point.

### Added

- **A complete, closed per-element attribute allowlist for XHTML content
  documents.** The grammar previously leaned on a permissive wildcard that
  accepted any attribute name outside a small denylist; that wildcard is gone.
  Every element now carries its real HTML5 attribute set, mirrored from
  epubcheck's own schema (facts/spec data, not copyrightable expression — the
  same clean-room stance as the rest of `schemas/`; our message wording stays
  ours). This spans the global attributes (HTML5 globals, the full WAI-ARIA
  1.2 state/property set, RDFa 1.1, microdata, the `on*` event-handler set,
  web-components `is`/`slot`), every element's own attributes (forms, media,
  tables, hyperlinks, lists, object/embed, `ins`/`del`, and the rest), and the
  namespaced families (`xml:*`, `epub:*`).
- **Arbitrary foreign-namespaced attributes are accepted** (e.g. a
  `custom:attribute` in a document-declared namespace), matching epubcheck —
  via a general rule for "any attribute in a non-empty namespace except the
  ones with their own rule", not a fixed name list. This also covers
  `ssml:ph`/`ssml:alphabet`.
- **`data-*` attributes** are accepted without a per-name rule (RELAX NG name
  classes can't express a `data-` prefix wildcard); a *malformed* `data-*`
  name is still reported by `HTM-061`, unchanged.

### Fixed

- **All offending attributes on an element are now reported, not just the
  first.** The content-model validator blamed the first bad attribute and
  stopped; it now recovers and checks the rest, so `<p fake="fake"
  clas="header">` reports both. (Invisible before this release — with the old
  wildcard, an element could realistically have at most one blamed attribute.)
- **A pathological validation slowdown** that the allowlist work surfaced:
  an attribute matchable by *both* an element's own rule and the old wildcard
  was genuine grammar ambiguity, which the derivative validator explored as
  separate branches per attribute — `O(2ⁿ)` in the number of such attributes
  on one element. Removing the wildcard eliminates the ambiguity entirely;
  validation is linear again.

### Notes

- **EPUB 2 and EPUB 3 both.** The same cutover applies to the XHTML 1.1 and
  HTML5 content models.
- Some attribute *values* are validated permissively (name-level, not the
  exact HTML5 value grammar) — e.g. `role` accepts any token rather than the
  closed ARIA/DPUB-ARIA role vocabulary, and the ARIA `aria-*` values aren't
  range-checked. Rejecting unknown *names* was the goal here; tighter
  per-value validation is future work.

## [0.5.18] - 2026-07-20

### Fixed

- **Findings are now listed in document order.** They were emitted in
  check-execution order (grammar pass, then Schematron, then the hand-coded and
  CSS passes), so a book with many findings — especially several of the same
  kind scattered down a file — came out interleaved by *which check* found each
  one rather than by *where* it is, and any check backed by a hash container
  added a nondeterministic shuffle on top. Findings within each file are now
  sorted by line and column (files keep their reading order), and two runs over
  the same book produce identical output. (rantanplan, MobileRead.)

## [0.5.17] - 2026-07-20

### Added

- **Obsolete/presentational HTML attributes are now flagged (RSC-005),
  matching EPUBCheck.** `link`/`vlink`/`clear` (removed with no valid host) are
  rejected on any element; `width`/`size` on `<hr>` are rejected while staying
  valid on their real hosts (`width` on `<img>`, `size` on `<input>`); and
  `name` on `<a>` is rejected under the EPUB 2 content model only — EPUBCheck's
  XHTML5 (EPUB 3) schema still permits it, so flagging it there would
  over-report. Every occurrence is reported at its own attribute, matching
  EPUBCheck's per-attribute output for both EPUB 2 and EPUB 3. (Doitsu,
  MobileRead #107.)

## [0.5.16] - 2026-07-20

### Fixed

- **Bare text directly in an EPUB 2 `<body>` was reported twice.** A hand-coded
  check (from 0.5.7, before an EPUB 2 content model existed) and the EPUB 2
  RELAX NG grammar (0.5.13) both flagged the same stray text. Removed the
  hand-coded check — the grammar is the single source of truth, and it also
  covers bare text in any other block-only element. (Doitsu, MobileRead.)

### Changed

- **The content-model "text not allowed" message is now plainer.** It read
  `character data is not allowed in element "X"`, which an average ebook
  creator won't parse; it now reads `stray text is not allowed directly in
  "X"; wrap it in an element` — saying what's wrong and what to do. Applies to
  both EPUB 2 and EPUB 3 (e.g. loose text in a `<ul>`). (Doitsu/JSWolf.)

## [0.5.15] - 2026-07-19

The EPUB 3 XHTML **content model** is now validated comprehensively (#13).
**This changes verdicts**: a book that nests elements illegally, or references
a missing id, is now reported invalid (RSC-005) where earlier releases passed
it — matching epubcheck. The corpus is unchanged (verdict-neutral there), but
real books with these mistakes will newly fail.

### Added

- **Content-model nesting rules (RSC-005, EPUB 3), via a new XHTML Schematron.**
  Constraints a grammar can't express — a node must / must not have a given
  ancestor: interactive content not inside `<a>`/`<button>` (including a nested
  `<a>`); the disallowed-descendant pairs (`form`-in-`form`, `label`-in-`label`,
  `header`/`footer`/`address` nesting, `audio`/`video` nesting, `dfn`,
  `table`-in-`caption`, `meter`/`progress`); and the required-ancestor rules
  (`area`→`map`, `img[@ismap]`→`a[@href]`). (Element/text *placement* was
  already caught by the RELAX NG grammar.)
- **IDREF/IDREFS resolution (RSC-005, EPUB 3).** Every id referenced by an ARIA
  relationship (`aria-describedby`/`-labelledby`/`-controls`/`-flowto`/`-owns`),
  `@form`, `input/@list`, `label/@for`, `output/@for`, `@headers`,
  `@aria-activedescendant`, or a MathML `@xref`/`@indenttarget` must resolve —
  and, where typed, to the right kind of element (`@form`→a `form`,
  `label/@for`→a labelable element, `@headers`→a `th` in the same table,
  `@aria-activedescendant`→a descendant, …).
- **Structural attribute rules (RSC-005, EPUB 3):** duplicate `map` name, a
  `map`'s `@id` must equal its `@name`, a `select` without `@multiple` may have
  at most one selected `option`, `@sizes` only on a `rel="icon"` link, one
  `<meta charset>` per document, no nested `ssml:ph`, and the `<track>`
  label/`default` rules.
- **wasm: the opt-in `--advisory` flag is now exposed.** `validate()` takes a
  third argument, `advisory?: boolean`; off by default, so existing
  two-argument callers get a byte-identical report. The browser demo gained an
  "Advisory checks" toggle.

## [0.5.14] - 2026-07-18

Two additions, both opt-in or usage-level, so no book's verdict changes: an
EPUB 2 empty-metadata usage note (OPF-072), and an opt-in CSS advisory layer.

### Added

- **OPF-072 (usage): an empty `dc:*` metadata element in an EPUB 2 package.**
  A `dc:` element (other than `dc:title`/`dc:date`, which have their own rules)
  whose content is empty or whitespace-only now draws OPF-072 at usage severity,
  matching epubcheck. EPUB 2 only. (#95, reported by Doitsu on the MobileRead
  forum.)
- **Opt-in CSS advisory checks (`--advisory`), via `styloria` 0.3.** Enables
  tool-owned advisory findings epubcheck has no verdict on — currently unknown
  CSS **property** names (`ADV-001`) and unknown at-rule **descriptor** names
  (`ADV-002`, e.g. a bogus `@font-face` descriptor), in stylesheets, inline
  `<style>` blocks, and `style="…"` attributes. Always `Usage` severity, so
  they never affect the exit code, and **off by default** — with the flag off
  the output is byte-identical (the corpus is unchanged: 599/607, 0 over-
  reported). `ADV-*` is a deliberately distinct, epubveri-owned message family:
  matching epubcheck on verdicts means not inventing a `CSS-0xx` it does not
  define. Library API: `Options { profile, advisory }` with
  `validate_bytes_with_options` / `validate_path_with_options`.
- Dependency: `styloria` 0.2 → 0.3 (adds its `validate` layer).

## [0.5.13] - 2026-07-17

An EPUB 2 content model - EPUB 2 books are no longer validated against HTML5 -
plus the rest of Tier-C's message detail. **This changes verdicts for EPUB 2
books**: see the note under the first entry.

### Fixed / Changed

- **EPUB 2 content documents now use an EPUB 2 (XHTML 1.1 + OPS 2.0.1) content
  model.** They were validated against the EPUB 3 (HTML5) grammar, which is wrong
  in both directions: `<big>`/`<tt>`/`<acronym>` are valid XHTML 1.1 but removed
  in HTML5 (we flagged them), and `<s>`/`<u>` and the HTML5 additions are the
  reverse (we missed them). Now they match epubcheck. (#24, reported by Doitsu on
  the MobileRead forum.)

  **Heads-up — this newly flags common shapes.** XHTML 1.1 is block-level under
  `<body>` (and `<p>` takes inline content only), so a `<br>` or `<span>` directly
  under `<body>`, or a block element inside a `<p>`, is now an `RSC-005` error.
  This is very common - Calibre and similar tools produce it - and on two real
  EPUB 2 books it went from 0 findings to 401 and 25. This is **not a regression**:
  those documents are XHTML 1.1 invalid and epubcheck reports them identically. The
  point is parity; repair is a separate tool's job. (#13, reported by Doitsu.)

- **`RSC-005` messages now name what was expected, and tell a bad attribute name
  from a bad value.** A content-model rejection reads `element "span" is not allowed
  here; expected one of "address", "blockquote", … "ul"` where the model is a
  genuine constraint (epubcheck lists these too). And an attribute failure now
  distinguishes `attribute "x" is not allowed here` (unknown name) from `value of
  attribute "dir" is invalid: "sideways"` (known name, bad value) - the two were
  one message before. (Tier-C.)

- **`OPF-018` is downgraded to the usage-level `OPF-018b` for scripted content** -
  a declared-but-unused `remote-resources` property can't be disproven when a
  script might fetch remotely, so epubcheck reports it as usage, and now so do we.
  (#27.)

## [0.5.12] - 2026-07-17

Fixes a false-fatal regression epubsana reported (#25), plus everything found by
auditing for its kind and by two new corpus-harness scoring checks. Corpus recall
599/607.

### Fixed

- **EPUB 2 documents with a `[` in the body no longer draw a false fatal.** 0.5.11's
  DTD-entity injection searched the whole document for the DOCTYPE's internal subset
  rather than the DOCTYPE itself, so a `[1]` footnote marker was mistaken for it and
  the entity declarations were injected into the body, breaking the parse. 78 false
  fatals across 11 valid books on a real shelf; the DOCTYPE is now scanned, not
  searched, and the same bug in two other DOCTYPE readers is fixed with it. (#25,
  reported by epubsana.)

- **`OPF-043` is now an error** (was a warning) - a spine item the reading system
  can't render, with no fallback, is a hole in the reading order; epubcheck's
  severity table and fixture both say error.

- **A valid EPUB 2 DTBook book is no longer flagged `OPF-043`.** The content types
  allowed directly in the spine are version-specific - XHTML or SVG in EPUB 3, but
  XHTML or DTBook (`application/x-dtbook+xml`) in EPUB 2 - and we applied the EPUB 3
  set to all. (Surfaced by the `OPF-043` severity fix.)

### Changed

- **`element_path` (JSON) now pins the attribute or text run a finding is about** -
  eight content-document findings about a specific attribute end their path in
  `/@name` instead of stopping at the element, and loose-text findings end in
  `/text()[n]`. (#20.)

- **The corpus harness now scores two more things** (internal, but it's how the
  above were found): over-reporting (findings on a book that expects one specific
  thing) and severity agreement (an id reported at the wrong severity). Both drove
  real fixes; both are now clean. Plus invariant tests over the rule tables - the
  class of bug no fixture can reach. (#26.)

## [0.5.11] - 2026-07-17

Doitsu's EPUB 2 test case and JSWolf's unused-resource request, both from the MobileRead
thread, plus a position fix and a licensing gap found along the way. Corpus recall is
unchanged (600/607) — and could not move for any of it, which is becoming the pattern
worth naming: epubcheck's corpus scores none of these, either because the finding is
usage-level (invisible to a metric that checks the expected ID was reported, not that
nothing extra was) or because it has no scenario for the rule at all.

### Added

- **`OPF-097`: a manifest resource that no content document uses** (usage). An unused font
  or image is almost always dead weight left by an earlier revision; the book stays valid
  and the note says which. (Requested by JSWolf on the MobileRead forum, for unused fonts
  and images specifically. epubcheck has the rule; epubveri simply hadn't implemented it —
  no new message ID was invented.)

  "Referenced" is narrower than it sounds, and the narrowness is the rule: a **hyperlink
  does not count**. Only references that embed or load a resource do — an image drawn, a
  stylesheet applied, a font loaded, a media overlay attached. What is exempt is what the
  *container* consumes rather than a document: the spine, the nav document, the NCX.

  Note the message says "no **content document** references it", and the precision is
  deliberate: a `properties="cover-image"` cover with no cover page is reported, because it
  is referenced by the package document and used by the reading system, but drawn by
  nothing. epubcheck reports it too. The note is factually true; what to do about it is the
  author's call, which is why this is usage and not advice.

### Fixed

- **`OPF-096` no longer fires on EPUB 2.** "Non-linear content is not reachable from the
  reading order" is an EPUB 3 requirement; EPUB 2.0.1 has none, so we were inventing an
  error on books epubcheck passes. Three independent signals agreed: every `OPF-096`
  fixture in epubcheck's corpus lives under `epub3/`, epubcheck stays silent on a real
  EPUB 2 book that we flagged, and the note already in our own code cited epubcheck's
  *EPUB 3* checker as where the rule came from. Same class as #9, #21 and #24 — an EPUB 3
  rule leaking into EPUB 2. (Reported by Doitsu on the MobileRead forum.)

- **Duplicate NCX `playOrder` values are now reported** (`RSC-005`). `playOrder` is the
  reading position, so two elements claiming the same one while pointing elsewhere is a
  contradiction; epubcheck flagged four on a real book where epubveri flagged none. The
  exception is what stops this being a plain duplicate scan: elements naming the *same*
  target may share a value, since that is one position reached by two routes. Every
  colliding element is reported, not one arbitrary member. (Reported by Doitsu on the
  MobileRead forum.)

- **`OPF-062` (usage) is now reported for Adobe's `page-map` spine extension.** The
  attribute already drew an `RSC-005`; the two say different things — one that the document
  is invalid, the other *which* non-standard feature is in use, which is the part that tells
  an author whether they meant it. (Reported by Doitsu on the MobileRead forum.)

- **Positions reported for EPUB 2 documents with DTD-declared entities are now exact.**
  0.5.10 made those documents parse by injecting `<!ENTITY>` declarations before the
  DOCTYPE's closing `>`. That adds no newline, so line numbers were always right — but
  inserting text on a line pushes whatever follows it on that line to the right, and the
  claim that "nothing is ever anchored there" was an assumption, not a fact. Measured: for
  a document whose DOCTYPE and `<html>` share a line, the root element's column was
  reported 25 too far. The shift cannot be avoided (fitting the declarations inside the
  DOCTYPE's own footprint leaves room for about three entities; skipping the injection
  sends the document back to not parsing at all, silently skipping every check on it), so
  it is corrected instead: the injection reports which line it moved, past which column,
  by how much, and the content-document walk subtracts it from every finding before the
  report is handed out. Matters most to tools that edit by position — a column that is
  right for our parser and wrong for the file on disk is worse than no column.

- **The npm package now ships the commercial-license text.** `@veripublica/epubveri-wasm`
  carried `LICENSE` (the AGPL) but not `LICENSE-COMMERCIAL.md`, so an npm consumer saw the
  AGPL text and no word of the `LicenseRef-veripublica-Commercial` half its own
  `package.json` declares. Two causes, both now handled: `wasm-pack` only collects licenses
  from the crate directory (the files are copied into `epubveri-wasm/`), and npm only
  always-packs a license file whose name is `license` plus a *dotted* extension — the
  hyphen in `LICENSE-COMMERCIAL.md` did not match, so it was dropped. The copy is named
  `LICENSE.COMMERCIAL.md`; the dot is functional, not style (see
  `epubveri-wasm/README.md`). `wasm-pack`'s own `files` list can't be relied on here: it
  writes `package.json` before it copies the licenses, so a clean build never lists them.

## [0.5.10] - 2026-07-17

Doitsu's MobileRead report and epubsana's #23, both of which found rules that
were wrong in ways the corpus scores green. Recall is unchanged (600/607) — no
verdict moves except #23's, which stops inventing 1079 errors and starts
reporting 163 real ones.

### Fixed

- **A deprecated `epub:type` value is no longer also reported as unknown.** `sidebar`
  and `note` drew both `OPF-088` ("is not in the default vocabulary") and `OPF-086b`
  ("is deprecated") — claims that cannot both hold, since knowing a term is deprecated
  means knowing the term. The vocabulary allowlist and the deprecated list lived in
  different modules and had drifted apart; 7 of the 13 deprecated terms were missing
  from the allowlist. Both now live in one module and the "is this a known term?"
  answer derives from both, so the contradiction cannot be stated. An invariant test
  over the whole table then found an eighth case nobody had reported: `figure` was in
  neither list, so `<figure epub:type="figure">` drew a false `OPF-088` too.
  (Reported by Doitsu on the MobileRead forum.)

- **`OPF-087` now states the actual rule, and catches the cases it was missing.** The
  Structural Semantics Vocabulary gives `table`, `table-row`, `table-cell`, `list`,
  `list-item`, `figure` and `aside` an HTML usage context of *"Not Allowed"* — they
  identify escapable/skippable structure on a media overlay's `seq`/`par` and mean
  nothing on an HTML element. epubveri instead read this as *"the value restates the
  semantic of its host element"* (`ol` + `list`, `table` + `table`, …), which agreed
  with epubcheck on every count of its own test fixture — that fixture only ever pairs
  each term with its matching element — but is not the rule: `<div epub:type="list">`
  went unreported entirely. (Reported by Doitsu on the MobileRead forum.)

  Corpus recall is unchanged (600/607) for both, and cannot move: it checks that the
  expected ID was reported, not that nothing extra was, so a spurious usage-level
  message is invisible to it — and its one `OPF-087` fixture is exactly the case where
  the wrong rule and the right one agree.

- **`CSS-007` now says what is actually wrong, and where.** It read *"font 'X' is a
  foreign resource, exempt from requiring a fallback"* — which describes the rule that
  does *not* fire (fonts never need a fallback), buries the one that does, and reads as
  a report of a non-problem. It now names the offending media type (e.g. the
  widespread-but-never-registered `application/x-font-opentype`) and points at the
  `@font-face` `src` that names the font, rather than at the stylesheet as a whole.
  (Reported by Doitsu on the MobileRead forum.)

- **`CSS-029` now points at the stylesheet the class name is written in**, and fires
  once per place it is written. It pointed at the content document that merely *links*
  that stylesheet — a file the class name does not appear in — and repeated itself once
  per linking document. (Reported by Doitsu on the MobileRead forum.)

- **CSS findings inside an inline `<style>` now report the line in the document.** They
  reported the line within the *extracted style text* against the document's path — a
  `direction` property on line 7 of a content document came out as line 3, where the
  reader finds `<head>`. One root cause behind every CSS rule (`CSS-001`, `-008`,
  `-019`, `-007`, `-028`); a linked stylesheet was never affected, since its offsets
  are file offsets. Where the style text isn't a verbatim slice of the document (a
  CDATA section, several text nodes, expanded entities) no offset can be mapped, so the
  finding falls back to the `<style>` element's own position rather than a confidently
  wrong line. (Found while fixing the above.)

### Added

- **`OPF-086b` now names what to use instead of a deprecated `epub:type`** — e.g.
  `sidebar` → a bare HTML `aside` element, `note` → the `footnote` semantic, `warning` →
  the `notice` semantic. The EPUB SSV names a replacement for 5 of its 13 deprecated
  terms; the other 8 say only that they are deprecated, rather than inventing advice.
  (Prompted by Doitsu on the MobileRead forum.)

- **`CSS-028`** (usage): notes each `@font-face` declaration, as real epubcheck does, so
  a reader comparing the two outputs isn't left wondering which tool missed an embedded
  font.

- **EPUB 2 content documents whose DOCTYPE declares the XHTML entities now parse
  (`&nbsp;` and friends).** An EPUB 2 content document references an external DTD
  (XHTML 1.1 / OEB 1.2) that declares the standard HTML named entities, but the
  parser never fetches an external DTD — so `&nbsp;`, the single most ordinary
  thing in a real EPUB 2, failed the parse as an unknown entity. Nothing is
  fetched now either: the entity set is fixed and known, so the referenced ones
  are declared inline before parsing (positions are unaffected — no line shifts).
  Measured on a real 171-book shelf, this affected **690 of 7207 content documents
  (10%), across 48 of 171 books — every one of them valid**. Two things followed
  from it, and both are fixed:
  - **1079 invented `RSC-012` errors** (86% of all `RSC-012` on that shelf, across
    31 books): an unparseable document's id map was built with
    `unwrap_or_default()`, turning *"I could not read this"* into *"this has no
    ids"*, so every fragment pointing into it was reported undefined — against ids
    that were plainly there. "I could not check" and "I checked, and it's absent"
    are now distinct, and only the latter reports.
  - **163 real findings that were never reported** (157 of them `RSC-005`
    `empty_title`): a document that fails to parse has *every* check on it
    silently skipped, so the book validates clean.

  This was the seam between two changes that were each right on their own: #12 made
  a parse failure report `RSC-016` but deliberately let the entity scan own
  entity-reference failures, and 0.5.8 (correctly) stopped that scan reporting
  DTD-declared entities in EPUB 2. Each deferred to the other, so nothing reported
  it — reopening the exact class #12 set out to close, this time silently.
  Reporting these documents as malformed would have been the wrong fix: they are
  valid, and it would have resurrected the false positive 0.5.8 removed.
  (Reported by epubsana, with measurements, in #23.)

  Corpus recall is unchanged (600/607): epubcheck's own corpus is mostly EPUB 3 and
  contains no document of this shape — which is why this survived to 0.5.9.

## [0.5.9] - 2026-07-16

Two more MobileRead forum fixes: an EPUB 2 false positive on the content-type
`<meta>`, and a better source location for `RSC-011`. Corpus recall is unchanged
(600/607) — neither changes a valid/invalid verdict.

### Fixed

- **EPUB 2 content documents are no longer flagged for a valid `<meta http-equiv="Content-Type">`.**
  The rule requiring the `content` attribute to be exactly `text/html; charset=utf-8`
  is an HTML5 (encoding-declaration-state) rule, so it applies to EPUB 3 only. EPUB 2
  content is XHTML 1.1, served as `application/xhtml+xml`, where
  `content="application/xhtml+xml; charset=utf-8"` is the correct form; epubcheck never
  flags it there. It was firing for EPUB 2 too (`RSC-005`) — a false positive. Both
  encoding-declaration checks are now gated to EPUB 3, and a duplicate copy of one of
  them (which also double-reported on EPUB 3) was removed. (Reported by Doitsu on the
  MobileRead forum. Same class as the EPUB-3-rule-leaking-into-EPUB-2 defect in #9.)

### Changed

- **`RSC-011` ("hyperlinked but not listed in the spine") now points at the source link.**
  It used to anchor at the OPF package root (`content.opf:2:1`) because it only knew the
  resolved target; it now anchors at the `<a>` element that creates the hyperlink — the
  right file, its `line:column`, and (in JSON) a `data.element_path` — matching where
  epubcheck locates it. Verdict is unchanged. (Reported by Doitsu on the MobileRead forum.)

## [0.5.8] - 2026-07-15

Two fixes from MobileRead forum reports: a fatal false positive on EPUB 2 named
character entities, and clearer `RSC-005` content-model messages. Corpus recall
is unchanged (600/607) — neither changes a valid/invalid verdict.

### Changed

- **`RSC-005` content-model messages now name the offending element or attribute.**
  A schema violation used to read as a blanket "content document does not conform
  to the EPUB XHTML content-model schema"; it now says *what* is wrong — e.g.
  `element "p" is not allowed here`, `character data is not allowed in element
  "ol"`, `element "x" is missing a required attribute`, `element "x" has
  incomplete content`, or `attribute "y" is not allowed here` — in the style of
  epubcheck's own RSC-005 wording. The offending name is also surfaced as a
  structured `data.params` entry alongside the existing `data.element_path`, so
  the detail is visible in the plain CLI output, not only in the JSON envelope.
  (Reported by Doitsu on the MobileRead forum. Naming the *expected* element as
  well — epubcheck's "…; expected element "li"" — remains future work.)

### Fixed

- **EPUB 2 named character entities (`&nbsp;`, `&eacute;`, `&copy;`, …) no longer
  raise a spurious `FATAL RSC-016`.** An EPUB 2 XHTML content document pulls the
  full set of standard HTML named entities in through its external DTD (XHTML 1.1
  or OEB 1.2), referenced by the DOCTYPE; because the underlying XML parser does
  not resolve external DTDs, every such reference was being reported as an
  undeclared entity — a fatal false positive epubcheck never emits, and a painful
  one since `&nbsp;` is ubiquitous (especially in French ebooks and `<p>&nbsp;</p>`
  spacing). These references are now accepted when the document carries a
  recognized EPUB 2 XHTML/OEB DOCTYPE. Genuinely undeclared entities still fail,
  and EPUB 3 is unchanged (it requires numeric references). (Reported by Doitsu,
  confirmed by KevinH, on the MobileRead forum.)

## [0.5.7] - 2026-07-15

Two content-model reporting improvements, both grounded in forum feedback.
Corpus recall is unchanged (600/607, 1.1% false positives) — these change
*what* and *how much* is reported, not the valid/invalid verdict.

### Added

- **Bare text directly in `<body>` is now flagged in EPUB 2.** XHTML 1.1's `body`
  content model is block-level only, so loose text there is a content-model error
  (`RSC-005`) — one per text run, with a real `line:column`. epubcheck reports
  this; epubveri was silently missing it. (EPUB 3, whose HTML5 body allows flow
  content including text, is unaffected.) The unambiguous EPUB 2 half of the
  bare-text discussion on the MobileRead forum.

### Changed

- **Content-model (`RSC-005`) failures now report every offending node, not just
  the first.** A list like `<ol><p>…</p><p>…</p></ol>` used to draw one finding;
  it now reports each misplaced child, each with its own `line:column` and element
  path — matching epubcheck's per-node output. (Reported by Doitsu on the
  MobileRead forum. A valid-but-empty list is still fine, so only the misplaced
  children are flagged, not the list element itself.)

## [0.5.6] - 2026-07-15

Sharpens the machine-readable locations added in 0.5.5: schema (`RSC-005`)
content-model findings now point at the exact offending node, and the
`element_path` form is corrected so it actually resolves in the XPath engine most
consumers use. Both build on 0.5.5's `data.element_path`; corpus recall is
unchanged (600/607, 1.1% false positives).

### Added

- **`RSC-005` content-model findings now carry a real `line:column` and
  `element_path`.** The RELAX NG engine reports *which* node collapsed the
  content model, so an OPF or XHTML schema violation points at the offending
  element — or the offending **attribute** (`…/@name`) when the violation is
  attribute-level — instead of anchoring the whole document at its root.
  ([issue #17](https://github.com/veripublica/epubveri/issues/17), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **`data.element_path` now binds every namespaced name to a non-empty prefix.**
  The 0.5.5 form left default-namespaced names bare and recorded the URI under an
  empty-string key, which is not resolvable in libxml2 / `lxml` (XPath 1.0 has no
  default namespace). Each namespace URI now gets a bound prefix — a readable
  well-known one for the common EPUB namespaces (`opf`, `dc`, `h` for XHTML,
  `svg`, …) or a generated `ns…` — so a path resolves directly with
  `root.xpath(path, namespaces=data["namespaces"])`.
  ([issue #18](https://github.com/veripublica/epubveri/issues/18), reported by
  Jens Tröger.)

## [0.5.5] - 2026-07-15

Adds a **machine-resolvable node path** to JSON findings, so an automated
consumer (an editor plugin, or a pipeline like Bookalope's) can jump straight to
the offending node instead of re-deriving it from a line/column — plus a
real-world false-positive fix. Both are additive: exact-ID recall against the
epubcheck corpus is unchanged (600/607, 1.1% false positives).

### Added

- **`data.element_path` (with `data.namespaces`) on node-anchored findings.** A
  rooted, XPath-style path with 1-based sibling indices — e.g.
  `/package[1]/spine[1]/itemref[2]`, or, when the finding is about a specific
  attribute, `…/dc:contributor[1]/@opf:role`. Names carry the source prefix as
  authored (a default-namespaced element stays bare); because EPUB documents are
  always namespaced and XPath 1.0 has no default-namespace concept, a
  `namespaces` prefix→URI map (the default namespace under the `""` key) travels
  alongside so a strict engine can resolve the path. Emitted across the
  node-anchored OPF and content-document checks, and — where a finding is about
  an attribute — pinning it directly (`@href`, `@prefix`, `@epub:prefix`, …).
  This lives in the tool-owned `data` slot, so it is purely additive: a consumer
  that ignores the field sees unchanged output.
  ([issue #18](https://github.com/veripublica/epubveri/issues/18), requested by
  Jens Tröger, mirroring the upstream ask on epubcheck.)

### Fixed

- **No more false `RSC-005` on a navigation-document index landmark.** A nav link
  like `<a epub:type="index" href="index.xhtml">Index</a>` was wrongly treated as
  an index *structure* and required to contain an `index-entry-list`. Matching
  epubcheck, the index content-model check now runs only on documents *declared*
  as an index (a manifest `properties="index"` item, a document linked from an
  index `<collection>`, or `dc:type="index"`), never on a document that merely
  *contains* an `epub:type="index"` element. A document actually declared an
  index is still validated.
  ([issue #19](https://github.com/veripublica/epubveri/issues/19), reported by
  Doitsu on the MobileRead forum.)

## [0.5.4] - 2026-07-15

A **foundation refresh**: the toolchain baseline and both behaviour-bearing
dependencies (the ZIP reader and the XML parser) move to current versions, all
verified behaviour-neutral against the full epubcheck corpus (600/607 exact-ID
recall and the 1.1% false-positive rate are unchanged, byte for byte) — plus one
real-world false-positive fix. Shipping these alone, before the next feature
work, so any field report can be attributed cleanly.

### Fixed

- **`PKG-025` no longer flags ordinary metadata files in `META-INF/`.** Files
  like Apple's `com.apple.ibooks.display-options.xml` or calibre's bookmark
  files drew an Error ("publication resource stored inside META-INF") and
  wrongly invalidated common real-world books. Per the OCF spec — and confirmed
  against epubcheck's own test fixture — the error is only for a
  **manifest-declared** resource stored in META-INF (e.g.
  `<item href="../META-INF/image.jpeg">`); undeclared container-level metadata
  is permitted and now stays silent. The declared case still errors.
  ([issue #16](https://github.com/veripublica/epubveri/issues/16), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **Minimum supported Rust version is now 1.88** (declared via `rust-version`,
  so older toolchains get a clear error — and a modern Cargo resolver simply
  keeps them on 0.5.3 rather than breaking). Raised by the `zip` upgrade below;
  1.88 is also the stabilization floor of let-chains, which the codebase now
  uses throughout.
- **`zip` 2.4.2 → 8.6.0** — the ZIP reader an EPUB validator feeds on. This
  buys six majors of reader robustness accumulated upstream (malformed
  EOCD/central-directory detection, panic-safety on malformed input) and aligns
  the whole veripublica family on one `zip` major, so tools embedding the
  `epubveri` crate don't compile two copies. Verified behaviour-neutral:
  malformed-archive verdicts (`PKG-003`/`PKG-004`/`PKG-008`), exit codes, and
  the corpus are unchanged. Two notes: `PKG-008`'s free-text message now embeds
  the new zip version's error wording (the `id`/`rule`/`params` machine contract
  is untouched), and the crate remains pure Rust (the deflate backend is now
  `zlib-rs`; no C dependencies — verified).
- **`roxmltree` 0.20 → 0.21** — the XML parser under every document epubveri
  reads. 0.21 changed bare-string attribute lookup to match by local name
  (ignoring the namespace), which would silently confuse e.g. `lang` with
  `xml:lang`; epubveri instead pins the intended semantics explicitly — every
  namespace-less attribute access goes through a new internal accessor that is
  version-independent **by construction**, verified neutral on 0.20 first and
  then on 0.21 (the full corpus, including the scenarios that caught the
  difference, is identical).
- **The codebase moved to Rust edition 2024**, and ~90 nested `if let` sites
  were collapsed into let-chains (net −95 lines) — internal only; no
  user-facing behaviour, CLI, or JSON change.

## [0.5.3] - 2026-07-13

### Added

- **Deprecated metadata `<link>` relationship keywords are now flagged.** The
  legacy per-format record keywords (`marc21xml-record`, `mods-record`,
  `onix-record`, `xmp-record`) — superseded by the generic `record` keyword with
  a `properties` attribute — and `xml-signature` now draw a warning-level
  `OPF-086`, matching epubcheck (EPUB 3 §D.4.1).

### Changed

- **Library: `epubveri::envelope` (the `--format json` types) is now generic
  over its two tool-owned slots** — the per-input `summary` and per-item `data`
  — so the whole veripublica tool family can build one envelope shape from these
  reference types. epubveri's own types stay the defaults and `Envelope::new`
  keeps its signature, so existing callers are unaffected and the JSON epubveri
  emits is byte-for-byte unchanged. A library-only addition: the CLI, its output,
  and the WASM binding are untouched.

## [0.5.2] - 2026-07-12

### Fixed

- **Malformed content documents are no longer silently accepted.** A content
  document that was not well-formed XML — for example an unclosed `<p>` — was
  skipped without a word, so the book validated clean (a false negative). It is
  now reported as a fatal `RSC-016` at the exact line and column, the same way a
  malformed package document already was. Undeclared/malformed named-entity
  references (e.g. `&nbsp;` with no declaration) keep their existing single
  `RSC-016` and are not double-reported.
  ([issue #12](https://github.com/veripublica/epubveri/issues/12), reported by
  Doitsu on the MobileRead forum.)

### Changed

- **A deprecated `epub:type` value is now reported as usage-level `OPF-086b`**
  (previously info-level `OPF-086`), matching epubcheck — which distinguishes
  the usage-level `OPF-086b` for a deprecated semantic from the warning-level
  `OPF-086` it uses for deprecated rendition/viewport properties. The set of
  deprecated values and the `endnote`-inside-`endnotes` exemption are unchanged.

## [0.5.1] - 2026-07-12

### Fixed

- **Two EPUB 3-only metadata rules no longer fire on EPUB 2 books.** An EPUB 2
  package with more than one `dc:date` — the common creation/modification pair
  that tools like Sigil and Calibre write — was wrongly reported `RSC-005`
  *"element 'dc:date' not allowed here (only one dc:date element is allowed)"*
  and shown INVALID; and a legacy OpenType font drew a spurious `OPF-090`
  *"non-preferred Core Media Type"*. Both are EPUB 3 concepts (EPUB 2
  legitimately carries several `dc:date` elements distinguished by `opf:event`,
  and Core Media Types are an EPUB 3 notion), so they are now scoped to EPUB 3
  — an EPUB 2 book validates exactly as epubcheck does.
  ([issue #9](https://github.com/veripublica/epubveri/issues/9), reported on the
  Sigil PageEdit User Guide.)

## [0.5.0] - 2026-07-11

This release adopts the **[veripublica CLI convention
v0.4](https://github.com/veripublica/conventions)** — the shared command-line
and machine-output contract the tool family follows. The invocation changes, so
this is a breaking release (a minor bump, per pre-1.0 SemVer). epubveri now
states *"Conforms to veripublica conventions v0.4"* in its `--help`.
([tracking issue #8](https://github.com/veripublica/epubveri/issues/8).)

### Changed

- **The input is now passed with `-i`/`--input`, never as a positional path.**
  `epubveri book.epub` becomes `epubveri -i book.epub`. A bare path is now a
  usage error that shows the corrected form. **Repeat `-i` to validate several
  books in one run** — each is reported, and the exit code is the worst across
  them.
- **Unrecognized input fails loudly (exit `2`) instead of being silently
  misread.** An unknown flag, an out-of-set `--format`/`--profile` value, or the
  same single-valued option given twice now stops with a short message pointing
  at `--help`, rather than being swallowed as a file name or falling back to a
  default.
- **Findings now carry epubcheck's five severity levels** —
  `fatal | error | warning | info | usage` — instead of folding fatals into
  errors and usage-level notes into info. Only `error` and `fatal` make a book
  invalid; `warning`/`info`/`usage` are reported but do not. Fifteen conditions
  (e.g. a missing or unreadable OPF, a corrupt container, malformed XML) are now
  `fatal`, and twenty advisory notes (e.g. `OPF-090`, `OPF-003`, `RSC-025`) are
  now `usage`, matching epubcheck's own classification.
- **Exit codes are clarified.** A broken-but-readable file — even one that isn't
  a valid ZIP — now gets a *verdict* (a `fatal` finding, exit `1`); exit `2` is
  reserved for the tool being unable to run or read an input at all.

### Added

- **`--format json`** — the shared veripublica machine envelope
  ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md)):
  one JSON object with the tool, version, convention key, aggregate status, and
  one self-contained object per input carrying its findings. The
  `epubveri-wasm` binding returns the same per-input shape, and the browser demo
  can **download it as `<book-name>.epubveri.json`** — byte-for-byte what the CLI
  emits. ([issue #11](https://github.com/veripublica/epubveri/issues/11).)
- **`-V`/`--version` carries git build metadata** — `0.5.0+<short-hash>`, with
  `.dirty` when built from a modified tree, falling back silently to the plain
  version when there is no checkout (a crates.io build). The CLI's `-V`, the wasm
  `version()`, and the demo footer all print this one string, so a bug report
  from any surface pins the exact source. ([conventions issue #20].)
- **`--help` gained an EXAMPLES section, an EXIT CODES summary, and the
  conformance line**; usage errors now point the reader at `--help`.

### Fixed

- **`epubveri -v` (and any unknown flag) now reports a real usage error** —
  `error: unexpected option '-v' (see --help)` — instead of the misleading
  `cannot read -v`. ([issue #7](https://github.com/veripublica/epubveri/issues/7).)

### Demo

- The in-browser WASM demo adopted the shared **family-web template v2**,
  which fixes two live accessibility defects (a keyboard-unreachable drop zone
  and a verdict chip failing WCAG AA contrast) and colours all five severity
  levels. ([issue #10](https://github.com/veripublica/epubveri/issues/10).)

## [0.4.4] - 2026-07-08

### Added

- **CSS findings now report an exact line/column**, closing epubveri's last
  position gap — CSS was the only finding family that could carry just a
  file name. Every CSS finding (`CSS-001`, `CSS-002`, `CSS-008`, `CSS-019`,
  and the `RSC-001`/`RSC-007`/`RSC-008`/`RSC-030` resource references found
  inside stylesheets) now points at the offending token, e.g. `CSS-001: use
  of the 'direction' property is not recommended [OEBPS/style.css:3:3]`.
  Built on [styloria](https://github.com/veripublica/styloria) 0.2's new
  source-span parse tree. ([issue
  #1](https://github.com/veripublica/epubveri/issues/1), requested by Kevin
  Hendricks for Sigil integration.)

### Fixed

- **Non-linear content reachability (`OPF-096`) now matches epubcheck's
  self-link rule.** A `linear="no"` spine item is reachable if any hyperlink
  points at it — *including a link the document makes to itself* (a nav's
  landmarks self-link such as `href="nav.xhtml"`, or a fragment-only
  `href="#…"`), which is how epubcheck has always treated it. The 0.4.2
  release instead exempted the toc nav categorically; that was
  over-correction — it wrongly silenced a non-linear nav that nothing links
  to. The nav is no longer special-cased: a self-linking nav still passes,
  and a genuinely unreachable one is flagged, exactly as epubcheck does.
  ([issue #1](https://github.com/veripublica/epubveri/issues/1), thanks to
  Kevin Hendricks for the pinpointed behavior.)

## [0.4.3] - 2026-07-08

### Fixed

- **Media-query (and other conditional-group) stylesheets were wrongly
  flooded with `CSS-008` "CSS syntax error"** ([issue
  #5](https://github.com/veripublica/epubveri/issues/5), reported by DNSB
  against a Vellum-generated book). The block of a conditional-group
  at-rule (`@media`, `@supports`, `@container`, …) holds nested *rules*,
  not declarations; each nested rule's selector was being mis-read as a
  malformed declaration, so a stylesheet fired one false `CSS-008` per
  `@media` block. Such blocks are now walked as rule lists — the
  declarations inside the nested rules are still checked, and a genuinely
  malformed declaration (or an unclosed qualified rule) is still reported.

### Changed

- **Malformed-XML findings now report the exact line/column where parsing
  failed.** A not-well-formed OPF package document (`RSC-016`) or
  `META-INF` container/encryption/signatures file (`RSC-005`) previously
  reported only the file name; each now points at the precise spot the XML
  parser gave up, which makes these findings directly actionable for a
  producer fixing them programmatically. (Position coverage across all
  finding call sites is now ~82%; the remainder — CSS checks and
  whole-container/ZIP-structure checks — have no single line to point at.)

## [0.4.2] - 2026-07-08

### Fixed

- **`dc:date` full timestamps were wrongly rejected** ([issue
  #4](https://github.com/veripublica/epubveri/issues/4), reported by
  JSWolf). A value like `2025-04-24T17:00:00Z` — a valid W3C-DTF (ISO 8601)
  timestamp — was flagged `OPF-054` ("doesn't conform to ISO 8601"). The
  date validator only accepted the date-only forms (`YYYY`, `YYYY-MM`,
  `YYYY-MM-DD`); it now also accepts a full timestamp (`T`, a time, and a
  `Z` or `±hh:mm` timezone designator).
- **A non-linear navigation document was wrongly flagged as unreachable**
  ([issue #5](https://github.com/veripublica/epubveri/issues/5), reported
  by DNSB). A nav (toc) document placed in the spine as `linear="no"` with
  no hyperlink pointing at it triggered `OPF-096` ("non-linear content is
  not reachable from the reading order"). The navigation document is always
  reachable through the reading system's own navigation controls, so it is
  now exempt from this check (matching epubcheck 5.3). Genuinely-unreachable
  non-linear *content* documents are still reported.

### Changed

- **Schematron-derived findings now carry line/column positions.** These
  were the one documented family that reported only a file path after
  0.2.0's position work; each now points at the exact element the rule
  matched (e.g. an empty `<meta property="">`). Completes the line/column
  coverage requested in [issue
  #1](https://github.com/veripublica/epubveri/issues/1).

## [0.4.1] - 2026-07-07

### Fixed

- The `opf-meta-property-not-empty` Schematron rule (behind `RSC-005`,
  "value of attribute 'property' is invalid (must not be empty)") was
  scoped to `opf:meta` — *every* `<meta>` element — instead of
  `opf:meta[@property]`. This meant any legacy, `property`-less `<meta>`
  (e.g. the extremely common OPF2-style `<meta name="cover"
  content="..."/>`) was wrongly flagged, since an absent `@property`
  normalizes to an empty string too. The corpus's own fixture for this
  rule only ever exercised `property=""` / `property="   "`, so the gap
  wasn't caught by the recall metric. Rescoped to match the rule's actual
  intent: only meta elements that already carry a `property` attribute are
  checked for emptiness.

Thanks to forum user **DNSB** ([MobileRead
thread](https://www.mobileread.com/forums/showthread.php?t=374286)) for
the report, via [issue #1](https://github.com/veripublica/epubveri/issues/1).

## [0.4.0] - 2026-07-06

### Added

- The `rule`/`params` sub-code introduced in 0.3.0 (for `RSC-005` only) is
  now populated at **every message ID with 2 or more call sites** across
  the crate — 36 additional IDs (`RSC-006` through `RSC-033`, `OPF-001`
  through `OPF-092`, `CSS-008`/`CSS-015`, `HTM-004`/`HTM-057`/`HTM-060`,
  `PKG-007`/`008`/`009`/`012`), on top of the `RSC-005` sites already
  done. IDs used at exactly one call site are left as-is — `id` alone is
  already unambiguous there.
- New `Report::push_rule` method (alongside the existing `push`/`push_at`/
  `push_at_pos`/`push_at_rule`/`push_full`) for the handful of sites with
  a `rule`/`params` pair but no `location` at all — a whole-container
  failure (corrupt/empty ZIP) detected before any file is identified.

## [0.3.0] - 2026-07-06

### Added

- Every diagnostic can now carry a stable, semantic **sub-code** (`rule`)
  and the **values interpolated into its message** (`params`), alongside
  the existing epubcheck-compatible `id`. This exists because a single
  `id` — especially `RSC-005`, epubcheck's generic RelaxNG/Schematron
  catch-all — covers many structurally unrelated conditions with only the
  rendered English sentence to tell them apart. `Message` gained
  `rule: Option<&'static str>` (e.g. `"opf.spine.duplicate_itemref"`) and
  `params: Vec<String>`. `rule` is populated at every `RSC-005` call site
  in the crate except a handful where no stable sub-code is derivable yet
  (schematron-derived output, and a few "input didn't parse as XML at
  all" cases) — other message IDs don't have `rule` populated yet and are
  a candidate for the same treatment later.
- Additive: `Report::push_full` (with position) and `Report::push_at_rule`
  (without) sit alongside the existing `push`/`push_at`/`push_at_pos`,
  which are unchanged. The WASM bindings expose the same fields.

## [0.2.1] - 2026-07-06

### Fixed

- `OPF-096` ("non-linear spine content isn't reachable from the reading
  order") is now downgraded to a usage-level `OPF-096b` when the book uses
  scripting anywhere — matching real epubcheck, which allows for script
  adding navigation/hyperlinks dynamically that static analysis can't see.
  Previously always reported as a hard error, which could misfire on a
  legitimate pattern such as a `nav.xhtml` placed in the spine as
  `linear="no"` in a scripted book.

Thanks to forum user **DNSB** ([MobileRead
thread](https://www.mobileread.com/forums/showthread.php?t=374286)) for
finding this. See [issue #3](https://github.com/veripublica/epubveri/issues/3).

## [0.2.0] - 2026-07-06

### Added

- Every diagnostic can now carry an exact source **position** (line and
  column), not just a bare file path. `Message` gained a new
  `position: Option<Position>` field; the CLI's human-readable output now
  shows `path:line:col` when a position is available (`--format ids` is
  unaffected). The WASM bindings expose the same `Position` type.
- This is additive: `Report::push_at_pos` sits alongside the existing
  `push`/`push_at` methods, which are unchanged. Position is populated at
  the large majority of check sites; a documented minority (schematron-
  generated findings, CSS-based checks, ZIP-archive-entry-level checks,
  and a few "input didn't parse at all" cases) have no coherent position
  to report and correctly stay `None`.

### Fixed

- `frontmatter` is a valid EPUB 3 Structural Semantics vocabulary term
  (sibling to `bodymatter`/`backmatter`) but was incorrectly flagged as
  unknown vocabulary.

Thanks to [Kevin Hendricks](https://github.com/kevinhendricks) (author of
the Sigil EPUB editor) for the detailed bug report that prompted both of
these fixes.

## [0.1.0] - 2026-07-04

Initial real release to [crates.io](https://crates.io/crates/epubveri) and
npm (`@veripublica/epubveri-wasm`) — a pure-Rust EPUB validator covering
OCF/OPF/manifest/spine integrity, content-document checks (XHTML, SVG,
MathML, CSS), navigation documents, and the Media Overlays, EDUPUB,
Dictionaries & Glossaries, Indexes, Previews, and Multiple-Renditions
extension specifications. At the time of this release: 98.8% exact
message-ID recall and 98.9% clean-file recall against epubcheck's own test
corpus.
