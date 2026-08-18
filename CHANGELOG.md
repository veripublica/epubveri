# Changelog

All notable changes to `epubveri` (and the `epubveri-wasm` bindings, which
track the same version) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
epubveri is pre-1.0, so breaking changes land as minor-version bumps
(`0.x.0`), per [Cargo's SemVer compatibility
rules](https://doc.rust-lang.org/cargo/reference/semver.html).

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
