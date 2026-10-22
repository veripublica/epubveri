//! CSS checks, via the `styloria` parser (a sibling project,
//! `github.com/veripublica/styloria` — a pure-Rust CSS3 tokenizer/core-
//! grammar parser/serializer with no selector or property-value grammar
//! yet):
//! - `CSS-008`: the syntax errors styloria's error-recovering parser
//!   recovered from — bad string/url tokens and unterminated rules/blocks,
//!   surfaced by styloria 0.4's `syntax_errors` — plus in-block malformed
//!   declaration *shapes* (an `ident` with no `:`), which the parser leaves
//!   as raw component values, so `check_declaration_shapes_spanned` splits
//!   and flags those itself. Still a subset of every malformation
//!   epubcheck's own CSS parser reports (a recovering parser accepts some
//!   constructs epubcheck rejects).
//! - `CSS-019`/`CSS-002`: an empty `@font-face` declaration block, or one
//!   whose `src` is an empty `url()`.
//! - A generic `url()` resource-resolution pass (covers `@import`,
//!   `@font-face src`, `background`, etc. uniformly — reported as
//!   **RSC-001**, matching the existing XHTML broken-reference check's
//!   message shape, since a missing resource is a missing resource
//!   regardless of which document type found it) — this also reaches
//!   nested rules inside e.g. `@media` blocks, since the walk below
//!   descends into every block styloria parses.
//!
//! Every pass here walks styloria's one parse tree, which carries a span on
//! every node (styloria 0.12), so every CSS finding reports the
//! exact `line:column` of the offending token — the last finding family in
//! epubveri that used to carry only a file path (issue #1; Kevin Hendricks /
//! Sigil asked for CSS positions specifically). The pub helpers
//! `opf.rs` calls (`stylesheet_urls`, `import_targets`, `selector_class_names`)
//! read the same tree and drop the spans they have no use for.

use std::collections::{HashMap, HashSet};

use styloria::{
    Block, BlockItem, ComponentValue, Declaration, DiagnosticKind, Rule, SimpleSelector, Span,
    Spanned, Stylesheet, SyntaxError, SyntaxErrorKind, Token, UrlContext, UrlForm, UrlReference,
    validate_parsed_stylesheet,
};

use crate::ids::*;
use crate::opf::{is_external, nfc, resolve};
use crate::report::{LocationIndex, Position, Report, Severity};

/// A stylesheet's text, decoded as CSS Syntax 3 §3.2 says: a byte order
/// mark, else a byte-exact `@charset`, else UTF-8. styloria does the decoding;
/// the BOM is not part of the text, so offsets count from after it.
///
/// **This follows the spec, not epubcheck, on purpose.** epubcheck sniffs
/// `@charset` loosely (any whitespace, either quote), hands the label to
/// Java's decoder as written, and so reads a BOM-less `@charset "utf-16be"`
/// file as UTF-16, where the spec says such a declaration is lying and the
/// file is UTF-8. A stylesheet in anything but UTF-8 or UTF-16 is CSS-004
/// either way, so the verdict does not hang on how it is decoded.
///
/// No protocol and no environment encoding: a container carries no
/// Content-Type, and a manifest stylesheet is checked once, whichever document
/// links it. A label styloria does not decode (the multi-byte legacy
/// encodings) falls back to lossy UTF-8, which is what every stylesheet got
/// before; the spec would decode it, so that is a known gap, not a choice.
pub(crate) fn decode_bytes(bytes: &[u8]) -> String {
    // A UTF-32BE byte order mark, which CSS Syntax does not know: the
    // Encoding Standard has no UTF-32, so the spec reads these bytes as UTF-8
    // and every character comes with three NULs, a CSS-008 apiece. epubcheck
    // decodes them as UTF-32BE (Java has the charset) and reports CSS-004
    // once, so we decode them too and report the same (see `check`). The
    // little-endian mark starts with the UTF-16LE one, which wins in both
    // tools: CSS-003, then whatever the misread text holds.
    if let [0, 0, 0xFE, 0xFF, rest @ ..] = bytes {
        return crate::encoding::decode_utf32(rest, true);
    }
    match styloria::decode_stylesheet(bytes, None, None) {
        Ok(decoded) => decoded.text.into_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// True if `bytes` starts with a UTF-16 byte-order mark (big- or
/// little-endian): CSS-003.
fn has_utf16_bom(bytes: &[u8]) -> bool {
    bytes.len() >= 2
        && ((bytes[0] == 0xFE && bytes[1] == 0xFF) || (bytes[0] == 0xFF && bytes[1] == 0xFE))
}

/// Whether an `@charset` value names UTF-8 or UTF-16 — the two encodings a
/// CSS resource may declare (CSS-004; a UTF-16 one additionally draws
/// CSS-003, since UTF-8 is what it *should* be).
///
/// The byte-order variants count: `UTF-16BE` and `UTF-16LE` are UTF-16, so
/// matching the name literally reports a corpus fixture that declares
/// `UTF-16BE` as if it had declared Latin-1 (issue #26).
fn is_utf8_or_utf16(charset: &str) -> bool {
    let c = charset.trim();
    c.eq_ignore_ascii_case("utf-8")
        || c.eq_ignore_ascii_case("utf-16")
        || c.eq_ignore_ascii_case("utf-16be")
        || c.eq_ignore_ascii_case("utf-16le")
}

/// Where a stylesheet's text physically sits, so a byte offset within it
/// can be turned into a position in the file the author actually opens.
///
/// A standalone `.css` file is the easy case: its offsets *are* file
/// offsets. An inline `<style>` is not - the text handed to the CSS parser
/// is the element's content, extracted out of the document, so an offset
/// into it says nothing about where that byte is in the file. Reporting one
/// as if it did is how a `direction` property on line 7 of a content
/// document came to be reported as line 3, where the reader finds `<head>`.
#[derive(Clone, Copy)]
pub(crate) enum CssOrigin<'a> {
    /// A standalone stylesheet: offsets into the CSS are offsets into the
    /// file. Carries the file's raw bytes where the caller has them, since
    /// the encoding checks (CSS-003/004) are exactly the ones that only mean
    /// anything for a real file - an inline `<style>`'s encoding was already
    /// resolved as part of its XHTML document long before its text got here.
    File { bytes: Option<&'a [u8]> },
    /// An inline `<style>` whose extracted text was found verbatim in `doc`
    /// starting at `base`, so CSS offsets shift onto document offsets.
    Inline { doc: &'a str, base: usize },
    /// An inline `<style>` whose extracted text is *not* a verbatim slice of
    /// the document - it came from several text nodes, a CDATA section, or
    /// had entity references expanded - so no offset within it can be
    /// mapped. Every finding falls back to the `<style>` element's own
    /// position: less precise, but it points at a real place in the file
    /// rather than a confidently wrong one.
    Opaque(Position),
}

impl CssOrigin<'_> {
    /// The position, in the file named alongside the finding, of byte
    /// `offset` within `css`.
    pub(crate) fn position(&self, css: &str, offset: usize) -> Position {
        match self {
            CssOrigin::File { .. } => Position::of_offset(css, offset),
            CssOrigin::Inline { doc, base } => Position::of_offset(doc, base + offset),
            CssOrigin::Opaque(p) => *p,
        }
    }
}

/// Where an inline `<style>`'s extracted `css` text sits within `doc`.
///
/// Verbatim-slice check rather than trust: `css` is a concatenation of the
/// element's text descendants, which equals a plain slice of the source only
/// when there is nothing in between and nothing was unescaped. Asking
/// whether the concatenation really is the slice at `base` settles
/// single-node-ness, CDATA and entity expansion in one comparison, so a
/// position is offered only when it is exact.
pub(crate) fn inline_origin<'a>(doc: &'a str, css: &str, style: roxmltree::Node) -> CssOrigin<'a> {
    let base = style
        .descendants()
        .find(|n| n.is_text())
        .map(|n| n.range().start);
    match base {
        Some(base) if doc.get(base..base + css.len()) == Some(css) => {
            CssOrigin::Inline { doc, base }
        }
        _ => CssOrigin::Opaque(Position::of(style)),
    }
}

/// What a stylesheet's urls resolve against: the directory it sits in, and
/// what the publication holds and declares.
#[derive(Clone, Copy)]
struct Book<'a> {
    base_dir: &'a str,
    name_index: &'a HashMap<String, String>,
    manifest_paths: &'a HashSet<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    css: &str,
    css_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    manifest_paths: &HashSet<String>,
    origin: CssOrigin,
    advisory: bool,
    is_epub3: bool,
    report: &mut Report,
) {
    let book = Book {
        base_dir,
        name_index,
        manifest_paths,
    };
    check_in(
        css,
        css_path,
        Some(book),
        origin,
        advisory,
        is_epub3,
        report,
    );
}

/// [`check`] for a stylesheet that belongs to no book, positions counted in
/// `css` and every finding located at `label`: what `crate::check_style_text`
/// reports.
///
/// **Where each url leads is the one thing left out** (missing, undeclared,
/// leaking: RSC-007/008/026), because only a book can answer it. A url wrong
/// on its face, a `file:` url or an empty `@font-face` src, is still
/// reported. Everything else here is a question about the text alone, and it
/// is the same code asking it.
pub(crate) fn check_without_book(
    css: &str,
    label: &str,
    advisory: bool,
    is_epub3: bool,
    report: &mut Report,
) {
    check_in(
        css,
        label,
        None,
        CssOrigin::File { bytes: None },
        advisory,
        is_epub3,
        report,
    );
}

fn check_in(
    css: &str,
    css_path: &str,
    book: Option<Book>,
    origin: CssOrigin,
    advisory: bool,
    is_epub3: bool,
    report: &mut Report,
) {
    // Span-carrying parse: every finding below points at the exact
    // line:column of the offending token (via `Position::of_offset`, the
    // same byte-offset→line:col helper the rest of epubveri uses, so CSS
    // positions count columns in chars just like every other finding).
    // A stylesheet can carry a finding per rule, so positions come from an
    // index over the text they are counted in (see `LocationIndex`).
    let _positions = match origin {
        CssOrigin::File { .. } => Some(LocationIndex::scope(css)),
        CssOrigin::Inline { doc, .. } => Some(LocationIndex::scope(doc)),
        CssOrigin::Opaque(_) => None,
    };
    let (sheet, syntax_errs) = styloria::parse_stylesheet(css);

    // Encoding checks only make sense for a standalone CSS file - see
    // `CssOrigin::File`, which is why the bytes live there rather than
    // arriving as a separate argument no other origin could ever supply.
    if let CssOrigin::File { bytes: Some(bytes) } = origin {
        if bytes.starts_with(&[0, 0, 0xFE, 0xFF]) {
            report.push_at(
                CSS_004,
                Severity::Error,
                "stylesheet is UTF-32BE encoded, which is not utf-8 or utf-16",
                css_path,
            );
        }
        if has_utf16_bom(bytes) {
            report.push_at(
                CSS_003,
                Severity::Warning,
                "stylesheet is UTF-16 encoded",
                css_path,
            );
        }
        // **A BOM outranks the declaration, and a declaration that is not
        // byte-exact is not a declaration at all.** Both come from CSS Syntax
        // 3 §3.1: the decode algorithm "gives precedence to a byte order mark
        // (BOM), and only uses the fallback when none is found", and the
        // encoding declaration is recognised by an exact byte pattern —
        // `@charset "` with one space and a *double* quote, then the label,
        // then `";`. The spec says outright that "multiple spaces, comments,
        // or single quotes … will cause the encoding declaration to not be
        // recognized".
        //
        // We were doing neither, and epubcheck's own CSS test files are what
        // showed it (2026-08-21): `bom-charset15.css` is a UTF-8 BOM followed
        // by `@charset "iso-8859-15"` — the BOM wins, so there is nothing to
        // report, and we reported CSS-004. `charset-empty.css` is
        // `@charset '' ;` — single quotes and a space before the semicolon, so
        // not an encoding declaration in the first place, and we reported
        // CSS-004 for it too. epubcheck is silent on both. **Two false
        // positives, found by running the 24 bare CSS fixtures no instrument
        // here had ever reached** — the shelf has no such stylesheet and the
        // corpus harness only walks CSS that lives inside a book.
        //
        // styloria's `charset_rule` reads exactly that pattern off the raw
        // bytes, and a BOM in front of it already stops the match.
        if let Some(charset) = styloria::charset_rule(bytes)
            && !is_utf8_or_utf16(charset)
        {
            report.push_at(
                CSS_004,
                Severity::Error,
                format!("@charset value '{charset}' is not utf-8 or utf-16"),
                css_path,
            );
        }
    }

    // **A rule whose block never closed does not get its declarations
    // second-guessed.** An unclosed `{` swallows everything after it — the
    // next rule's selector and braces included — so what the parser finds
    // inside it is not what the author wrote as one block, and every shape
    // complaint it produces is a consequence of the one defect the parser has
    // already reported as `UnterminatedBlock`.
    //
    // Measured on `content-css-syntax-error`, which has two unclosed blocks:
    // styloria reports exactly two errors and so does epubcheck; the third
    // finding was ours, a `css.declaration.malformed_shape` on the `p {` that
    // the first unclosed block had absorbed. The count difference was never
    // styloria's error recovery — its answer already matched — and calling it
    // a CSS-crate granularity difference sent a fix to the wrong repository.
    // Parity with epubcheck is this consumer's business, not the library's.
    //
    // Positions still differ from epubcheck's and that is left alone: it
    // points at the token that got confused (or at EOF), we point at the `{`
    // that was never closed, which is the one an author has to fix.
    //
    // It holds at any depth: a rule inside `@media` whose block a broken
    // string left open is the same case one level down.
    let unterminated: Vec<usize> = syntax_errs
        .iter()
        .filter(|e| e.kind == SyntaxErrorKind::UnterminatedBlock)
        .map(|e| e.span.start)
        .collect();
    let ctx = Ctx {
        css,
        css_path,
        origin,
        is_epub3,
        unterminated: &unterminated,
    };
    // Spans whose inner syntax errors are already accounted for: see
    // `Quiet`.
    let mut quiet = Quiet::default();
    for rule in &sheet.rules {
        match &rule.node {
            Rule::Qualified(q) => {
                // A style rule with no selector at all - a stray `{ … }` after
                // a complete rule. CSS Syntax parses it as a qualified rule
                // with an empty prelude, and Selectors requires at least one
                // selector, so epubcheck reports CSS-008 (measured, one book).
                // We were silent: the declarations inside parse fine and
                // nothing asked whether anything selected them.
                if q.prelude.iter().all(|v| is_blank_component(&v.node)) {
                    report.push_full(
                        CSS_008,
                        Severity::Error,
                        "a style rule must have a selector",
                        css_path,
                        origin.position(css, rule.span.start),
                        "css.rule.missing_selector",
                        Vec::new(),
                    );
                }
                walk_rule_block(rule, &q.block, ctx, &mut quiet, report);
            }
            Rule::At(a) => {
                if let Some(block) = &a.block {
                    if a.name.eq_ignore_ascii_case("font-face") {
                        check_font_face(block, a.name_span, ctx, report);
                    }
                    walk_at_rule_block(&a.name, &block.node, ctx, &mut quiet, report);
                }
            }
        }
    }

    // CSS-008: the syntax errors styloria's (error-recovering) parser
    // recovered from, anywhere in the tree, less the ones a walk above has
    // already answered for (`quiet`).
    report_syntax_errors(&syntax_errs, &sheet, &quiet, ctx, advisory, report);
    {
        // A top-level `@font-face` block's urls are `check_font_face`'s, which
        // asks its own questions of them.
        let urls = sheet_urls(&sheet)
            .into_iter()
            .filter(|u| u.under != Under::FontFaceBlock);
        for u in urls {
            let nested = u.nested;
            let u = u.url;
            let url = u.node;
            let pos = origin.position(css, u.span.start);
            let mut push = |id, text: String, rule| {
                push_url_finding(
                    report, nested, advisory, id, text, css_path, pos, rule, &url,
                )
            };
            if is_file_url_str(&url) {
                push(
                    RSC_030,
                    format!("'{url}' is a file URL, which is not allowed"),
                    "css.url.file_scheme_not_allowed",
                );
                continue;
            }
            if is_external(&url) {
                continue;
            }
            // Where the url leads is a question only a book can answer, so a
            // stylesheet checked on its own (`check_without_book`) stops
            // here. A `file:` url, above, is wrong whatever document holds
            // it, as it is in `check_font_face`.
            let Some(Book {
                base_dir,
                name_index,
                manifest_paths,
            }) = book
            else {
                continue;
            };
            // RSC-026: the url() resolves above the container root, or is
            // path-absolute. epubcheck applies this in `URLChecker`, its single
            // URL-resolution point, so every CSS url() goes through it too - we
            // had it on manifest hrefs only. Additive with the RSC-001/007/008
            // split below: a leaking url is both outside the container and
            // missing from it, and epubcheck reports both.
            //
            // The shape that found this is a stylesheet at the container *root*
            // asking for `url(../Fonts/x.ttf)`, which real books do carry.
            if crate::opf::href_leaks_container_root(base_dir, &url) {
                push(
                    RSC_026,
                    format!("'{url}' leaks outside the container"),
                    "css.url.leaks_container_root",
                );
            }
            let resolved = nfc(&resolve(base_dir, &url));
            let declared = manifest_paths.contains(&resolved);
            let present = name_index.contains_key(&resolved);
            // Real corpus finding, mirrors the same RSC-001/007/008 split
            // already established for XHTML content-doc references: RSC-001
            // is only for a manifest-*declared* resource whose file is
            // missing; an *undeclared* target is RSC-008 if the file still
            // genuinely exists in the container, or RSC-007 if it doesn't
            // exist at all - confirmed via three distinctly-named real
            // fixtures (`content-css-import-not-present-error`,
            // `content-css-import-not-declared-error`,
            // `content-css-url-not-present-error`), and applies uniformly to
            // every CSS url() construct (`@import`, `background`, etc.), not
            // just `@import`.
            match (declared, present) {
                (true, false) => {
                    // **Nothing here: the manifest walk has already said it.**
                    // RSC-001 is per *resource* in epubcheck, not per reference —
                    // its `PublicationResourceChecker` visits each publication
                    // resource once — so a declared-but-absent file named by an
                    // `@import` draws one finding there and drew two here, on
                    // `content-css-import-not-present-error`. Being `declared` is
                    // exactly the condition that guarantees the manifest item was
                    // visited and reported, which the fixture shows directly: the
                    // surviving finding is the manifest one, at `package.opf`.
                    //
                    // The other three arms stay. `RSC_008`/`RSC_007` are about the
                    // *reference*, which is this walk's business and which the
                    // manifest cannot see.
                }
                (false, true) => {
                    push(
                        RSC_008,
                        format!("resource '{url}' is not declared in the manifest"),
                        "css.url.undeclared_resource",
                    );
                }
                (false, false) => {
                    push(
                        RSC_007,
                        format!("references a missing resource '{url}'"),
                        "css.url.missing_resource",
                    );
                }
                (true, true) => {}
            }
        }
    }

    // Opt-in advisory pass (--advisory): unknown property/descriptor names,
    // which epubcheck does not check. Off by default, so the default output is
    // byte-identical. Positions map through `origin` like every CSS finding.
    if advisory {
        let type_names = styloria::selector_references(&sheet)
            .into_iter()
            .filter(|r| r.top_level && !r.in_argument)
            .filter_map(|r| match r.selector.node {
                SimpleSelector::Type(name) => Some(name.local),
                _ => None,
            });
        {
            for name in type_names {
                if is_known_element_name(&name.node) {
                    continue;
                }
                report.push_full(
                    ADV_003,
                    Severity::Usage,
                    format!(
                        "'{}' is not an element in any vocabulary this document can use; \
                         the selector matches nothing",
                        name.node
                    ),
                    css_path,
                    origin.position(css, name.span.start),
                    "css.selector.unknown_element",
                    vec![name.node.to_string()],
                );
            }
        }
        for d in validate_parsed_stylesheet(&sheet) {
            // A misplaced at-rule inside a block is already CSS-008.
            if d.kind == DiagnosticKind::MisplacedAtRule
                && !sheet
                    .rules
                    .iter()
                    .any(|r| matches!(&r.node, Rule::At(a) if a.name_span == d.span))
            {
                continue;
            }
            let Some((id, rule, text, params)) = advisory_fields(&d) else {
                continue;
            };
            report.push_full(
                id,
                Severity::Usage,
                text,
                css_path,
                origin.position(css, d.span.start),
                rule,
                params,
            );
        }
    }
}

/// One finding about a stylesheet url, an error. A url inside another
/// function (`nested`) is one epubcheck never reads, so the finding is ours
/// alone and goes behind `--advisory` as NEXT-018, keeping its rule key and
/// params for the tools that dispatch on them.
#[allow(clippy::too_many_arguments)]
fn push_url_finding(
    report: &mut Report,
    nested: bool,
    advisory: bool,
    id: &'static str,
    text: String,
    path: &str,
    pos: Position,
    rule: &'static str,
    url: &str,
) {
    let params = vec![url.to_string()];
    if !nested {
        report.push_full(id, Severity::Error, text, path, pos, rule, params);
    } else if advisory {
        report.push_full(
            NEXT_018,
            Severity::Usage,
            format!("{text} (it sits inside another function, where epubcheck does not read urls; {id} once it does)"),
            path,
            pos,
            rule,
            params,
        );
    }
}

/// Is this a name a type selector could legitimately match?
///
/// **Derived, not listed.** The XHTML names come out of `XHTML_RNG` itself, so
/// this cannot drift from the grammar the validator actually uses; SVG and
/// MathML reuse the lists their own checks already carry. A hand-written
/// fourth copy would be the thing that goes stale.
///
/// **A hyphen means yes, always.** HTML requires a custom element's name to
/// contain one (`<my-widget>`), so any hyphenated name is a legal element
/// somewhere and cannot be judged from here. That single rule is what makes
/// this check low-noise enough to exist: without it every author component
/// would be flagged.
fn is_known_element_name(name: &str) -> bool {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    let lower = name.to_ascii_lowercase();
    if lower.contains('-') {
        return true;
    }
    let set = NAMES.get_or_init(|| {
        let mut set: HashSet<String> = HashSet::new();
        // Every `element name="…"` the XHTML grammar declares, both versions.
        let rng = crate::rng::XHTML_RNG;
        let mut rest = rng;
        while let Some(i) = rest.find("element name=\"") {
            rest = &rest[i + 14..];
            if let Some(j) = rest.find('"') {
                let n = &rest[..j];
                // `epub:trigger` and friends: the local name is what a CSS
                // type selector writes.
                set.insert(n.rsplit(':').next().unwrap_or(n).to_ascii_lowercase());
                rest = &rest[j..];
            }
        }
        // Elements HTML once had or still has but our grammars do not carry:
        // the presentational set OPS 2.0.1 excludes with `legacy.rng`, plus a
        // few current-but-rare ones. **Styling them is not a typo** — an
        // author writing `center { … }` is targeting real markup, and the
        // shelf proved the point: of the first eight findings this check
        // produced, five were `center`, `strike` and `rtc`. Without this the
        // rule flags legacy stylesheets rather than mistakes.
        const HISTORICAL: &[&str] = &[
            "acronym",
            "applet",
            "basefont",
            "bgsound",
            "big",
            "blink",
            "center",
            "dir",
            "font",
            "frame",
            "frameset",
            "isindex",
            "keygen",
            "listing",
            "marquee",
            "menuitem",
            "nobr",
            "noembed",
            "noframes",
            "plaintext",
            "rb",
            "rtc",
            "spacer",
            "strike",
            "tt",
            "xmp",
        ];
        set.extend(HISTORICAL.iter().map(|e| e.to_string()));
        set.extend(
            crate::svg::SVG_ELEMENTS
                .iter()
                .map(|e| e.to_ascii_lowercase()),
        );
        set.extend(
            crate::mathml::PRESENTATION_ELEMENTS
                .iter()
                .map(|e| e.to_ascii_lowercase()),
        );
        set
    });
    set.contains(&lower)
}

/// The `(id, rule, text, params)` for one styloria advisory diagnostic. `Usage`
/// severity and a tool-owned `ADV-*` id, so it is advisory only and never moves
/// the verdict. Shared by the stylesheet and `style="…"` attribute paths.
///
/// `None` for a diagnostic no rule reports yet.
fn advisory_fields(
    d: &styloria::Diagnostic,
) -> Option<(&'static str, &'static str, String, Vec<String>)> {
    Some(match d.kind {
        DiagnosticKind::UnknownProperty => (
            ADV_001,
            "css.property.unknown",
            format!("'{}' is not a recognized CSS property", d.name),
            vec![d.name.clone()],
        ),
        DiagnosticKind::UnknownDescriptor { at_rule } => (
            ADV_002,
            "css.descriptor.unknown",
            format!("'{}' is not a recognized descriptor for @{at_rule}", d.name),
            vec![d.name.clone(), at_rule.to_string()],
        ),
        // **Not a rule yet, so nothing to report.** epubcheck checks no CSS
        // value, so this can only ever be an `ADV-*`, and an advisory has to
        // be true every time it fires: every Invalid on the shelf gets read by
        // hand before one ships. Until then the diagnostic has no id, no
        // text and no consumer.
        DiagnosticKind::InvalidValue => return None,
        // styloria 0.16's at-rule and keyframe checks. epubcheck 5.4.0 is
        // silent on every one of them; see the ADV-019..023 note in `ids.rs`.
        DiagnosticKind::UnknownAtRule => (
            ADV_020,
            "css.at_rule.unknown",
            format!(
                "'@{}' is not an at-rule any CSS specification defines, so browsers ignore \
                 the whole rule",
                d.name
            ),
            vec![d.name.clone()],
        ),
        // Only ever a top-level one here: `check` drops the ones inside a
        // block, which `walk_at_rule_block` reports as CSS-008.
        DiagnosticKind::MisplacedAtRule => (
            ADV_021,
            "css.at_rule.misplaced",
            // Which rules may come first differs between the two and between
            // specifications (`@layer` statements); the text claims only what
            // holds whenever styloria reports one.
            format!(
                "'@{}' comes after a rule it must precede, so browsers ignore it",
                d.name
            ),
            vec![d.name.clone()],
        ),
        DiagnosticKind::UnknownMediaFeature => (
            ADV_022,
            "css.media_feature.unknown",
            // Not "the query never matches": Media Queries 4 is three-valued,
            // and `(min-widht: 1px) or (color)` still holds on a colour screen.
            format!(
                "'{}' is not a media feature any specification defines, so browsers never \
                 treat this test as true",
                d.name
            ),
            vec![d.name.clone()],
        ),
        DiagnosticKind::ImportantInKeyframe => (
            ADV_023,
            "css.keyframe.important_ignored",
            format!(
                "'{}' is marked !important inside a keyframe, where CSS Animations ignores \
                 the declaration",
                d.name
            ),
            vec![d.name.clone()],
        ),
    })
}

/// A `style="..."` attribute value: the contents of a block with no braces,
/// which is what styloria's `parse_block_contents` reads.
///
/// Findings here anchor at the file, not a line:column: the attribute value
/// reaches us unescaped, so an offset into it is not an offset into the
/// document.
///
/// **Only a malformed item is CSS-008 here, as it was before styloria 0.12.**
/// That is a declaration that is not `name: value` (styloria's
/// `MalformedDeclaration` / `UnexpectedToken`) or a rule where a declaration
/// belongs. A broken string or url inside an otherwise well-shaped
/// declaration was never reported on this path, and widening that is a
/// question for epubcheck parity, not a side effect of a parser change.
pub(crate) fn check_style_attribute(
    value: &str,
    path: &str,
    advisory: bool,
    is_epub3: bool,
    report: &mut Report,
) {
    let (items, errors) = styloria::parse_block_contents(value);
    for e in errors.iter().filter(|e| {
        matches!(
            e.kind,
            SyntaxErrorKind::MalformedDeclaration | SyntaxErrorKind::UnexpectedToken
        )
    }) {
        report.push_at_rule(
            CSS_008,
            Severity::Error,
            syntax_error_text(value, e, None),
            path,
            "css.declaration.malformed_shape",
            Vec::new(),
        );
    }
    for item in &items {
        match item {
            BlockItem::Declaration(d) => {
                check_style_attribute_declaration(&d.node, path, is_epub3, report)
            }
            BlockItem::Rule(r) => {
                let text = match value.get(r.span.start..r.span.end).and_then(quote_css) {
                    Some(q) => {
                        format!("CSS syntax error: '{q}' is not a 'property: value' declaration")
                    }
                    None => "CSS syntax error".to_string(),
                };
                report.push_at_rule(
                    CSS_008,
                    Severity::Error,
                    text,
                    path,
                    "css.declaration.malformed_shape",
                    Vec::new(),
                );
            }
        }
    }

    // Opt-in advisory pass. No document byte-offset is available here, so the
    // finding anchors at the file (path), not a line:column.
    if advisory {
        for d in styloria::validate_parsed_block(&items) {
            let Some((id, rule, text, params)) = advisory_fields(&d) else {
                continue;
            };
            report.push_at_rule(id, Severity::Usage, text, path, rule, params);
        }
    }
}

/// The EPUB rules about one declaration in a `style` attribute: the
/// stylesheet walk's [`check_declaration`] without positions.
fn check_style_attribute_declaration(
    d: &Declaration,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    let name = &d.name;
    // `OBS-001`, the same rule the stylesheet walk applies. A `style`
    // attribute is where W3C's own `css-epub-hyphens`,
    // `css-epub-text-align-last` and `css-epub-word-break` tests put
    // their prefixed properties, and the epub-tests run was the only
    // instrument that could see the omission: no stylesheet on any
    // shelf here carries one.
    if name.starts_with("-epub-") {
        report.push_at_rule(
            OBS_001,
            Severity::Usage,
            format!("usage of the CSS prefixed property '{name}' is outdated"),
            path,
            "css.outdated_prefixed_property",
            vec![name.to_string()],
        );
    } else if name.eq_ignore_ascii_case("text-transform") && has_epub_fullwidth(&d.value) {
        report.push_at_rule(
            OBS_001,
            Severity::Usage,
            "usage of the CSS prefixed value '-epub-fullwidth' is outdated",
            path,
            "css.outdated_prefixed_value",
            vec!["-epub-fullwidth".to_string()],
        );
    }
    if is_epub3
        && FLAGGED_PROPERTIES
            .iter()
            .any(|p| name.eq_ignore_ascii_case(p))
    {
        report.push_at(
            CSS_001,
            Severity::Error,
            format!("use of the '{name}' property is not recommended"),
            path,
        );
    } else if name.eq_ignore_ascii_case("position") && is_position_fixed(&d.value) {
        report.push_at(
            CSS_006,
            Severity::Usage,
            "use of 'position: fixed' is not recommended".to_string(),
            path,
        );
    }
}

const FLAGGED_PROPERTIES: [&str; 2] = ["direction", "unicode-bidi"];

/// What every walk below needs about the stylesheet it is reporting on.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    css: &'a str,
    css_path: &'a str,
    origin: CssOrigin<'a>,
    is_epub3: bool,
    /// Where each `UnterminatedBlock` starts, in order.
    unterminated: &'a [usize],
}

impl Ctx<'_> {
    /// Whether a `{` that never closed starts inside `span`: a binary search,
    /// since a hostile sheet can hold one per rule.
    fn never_closed(&self, span: Span) -> bool {
        let i = self.unterminated.partition_point(|u| *u < span.start);
        self.unterminated.get(i).is_some_and(|u| *u < span.end)
    }
}

/// Spans of the tree whose inner shape errors a walk has already answered
/// for, so `report_syntax_errors` does not report them a second time.
///
/// **This is how the 0.11 behaviour survives the 0.12 tree.** styloria
/// 0.11 left a nested rule unparsed: `p { a { … } }` was one malformed
/// declaration and nothing inside it was read. 0.12 parses it as a rule,
/// with its own selector and declarations, and reports what is wrong
/// inside. epubveri still reports the nested rule once (CSS Nesting is not
/// in the CSS EPUB defers to; see `walk_style_block`), so what styloria
/// finds inside it is a consequence of that one finding, not more findings.
/// The same holds for a block that never closed, and for rules inside an
/// at-rule whose block holds descriptors, which 0.11 skipped in silence.
///
/// Only the shape kinds are quieted. A broken string, url or unicode-range,
/// an unclosed `{` and the nesting guard are about tokens, and were reported
/// wherever they sat.
#[derive(Default)]
struct Quiet(Vec<Span>);

impl Quiet {
    fn push(&mut self, span: Span) {
        self.0.push(span);
    }

    fn covers(&self, e: &SyntaxError) -> bool {
        matches!(
            e.kind,
            SyntaxErrorKind::MalformedDeclaration
                | SyntaxErrorKind::UnexpectedToken
                | SyntaxErrorKind::InvalidSelector
                | SyntaxErrorKind::InvalidSelectorStructure
        ) && self
            .0
            .iter()
            .any(|s| s.start <= e.span.start && e.span.start < s.end)
    }
}

/// The innermost rule anywhere in `rules` whose span holds `offset`.
///
/// Rules and the items of a block are in source order and do not overlap at
/// one level, so each level is a binary search: a linear one per syntax
/// error made a stylesheet of 100,000 malformed rules quadratic.
fn innermost_rule<'r, 'c>(
    rules: &'r [Spanned<Rule<'c>>],
    offset: usize,
) -> Option<&'r Spanned<Rule<'c>>> {
    let i = rules.partition_point(|r| r.span.end <= offset);
    let rule = rules.get(i).filter(|r| r.span.start <= offset)?;
    Some(
        rule.node
            .block()
            .and_then(|b| innermost_in_block(&b.node, offset))
            .unwrap_or(rule),
    )
}

fn innermost_in_block<'r, 'c>(
    items: &'r [BlockItem<'c>],
    offset: usize,
) -> Option<&'r Spanned<Rule<'c>>> {
    let i = items.partition_point(|it| it.span().end <= offset);
    let BlockItem::Rule(rule) = items.get(i).filter(|it| it.span().start <= offset)? else {
        return None;
    };
    Some(
        rule.node
            .block()
            .and_then(|b| innermost_in_block(&b.node, offset))
            .unwrap_or(rule),
    )
}

/// A rule's selector as written, and the byte offset in `css` where it
/// starts. `None` for an at-rule or an empty prelude.
fn prelude_source<'c>(css: &'c str, rule: &Spanned<Rule>) -> Option<(usize, &'c str)> {
    let Rule::Qualified(q) = &rule.node else {
        return None;
    };
    let start = q.prelude.first()?.span.start;
    let end = q.prelude.last()?.span.end;
    Some((start, css.get(start..end)?))
}

/// Report styloria's syntax errors as CSS-008 — except a selector whose only
/// fault is a class name that is not a CSS identifier (`.-`, `.-1`), a bad
/// url, and what `quiet` covers.
///
/// **The verdict on a class name is epubcheck's.** Its scanner reads `.`
/// followed by any CSS 2.1 `{name}` as a class (`CssScanner._classname`), so
/// `span.-` passes; Selectors, CSS 2.1 and every browser want an
/// *identifier*, which `-` and `-1` are not, and drop the whole rule. The
/// spec is on our side and the verdict is not ours to move, so the finding
/// becomes ADV-012 behind `--advisory` — a reader's 2,798-book library had
/// one book flip on it. "Only fault" is tested, not assumed: the prelude is
/// re-validated with each such class name swapped for a placeholder
/// identifier, and anything still wrong keeps its CSS-008.
///
/// **One finding per selector list.** styloria reports one `InvalidSelector`
/// per comma-separated selector, which is the right granularity for a CSS
/// library and was settled deliberately in its #3. epubcheck's unit is the
/// whole selector *list*: `. a, . b, . c { … }` is three findings there and
/// one here (#81). A real book made the gap visible: `. h-100, . y-100 { … }`
/// repeated down a stylesheet gave 22 CSS-008 against epubcheck's 12. Errors
/// arrive sorted, and one rule's selector errors are contiguous, so
/// remembering the last rule reported is enough. The library keeps its
/// answer; the consumer adapts.
///
/// The slug says where the error sat: styloria's docs pin
/// `MalformedDeclaration` and `UnexpectedToken` to the inside of a block,
/// which is the `css.declaration.malformed_shape` epubveri has always used
/// for them, and every other kind to `syntax_error_slug`.
fn report_syntax_errors(
    errors: &[SyntaxError],
    sheet: &Stylesheet,
    quiet: &Quiet,
    ctx: Ctx,
    advisory: bool,
    report: &mut Report,
) {
    let Ctx {
        css,
        css_path,
        origin,
        ..
    } = ctx;
    let mut claimed: Option<Span> = None;
    for e in errors {
        if quiet.covers(e) {
            continue;
        }
        match e.kind {
            // Not a syntax error to epubcheck: its scanner reads the url, and
            // the url checks report what is wrong with it. See
            // `bad_url_target`. What CSS says about it is ADV-015, which
            // leaves the verdict alone.
            SyntaxErrorKind::BadUrl => {
                if advisory && let Some(token) = css.get(e.span.start..e.span.end) {
                    report.push_full(
                        ADV_015,
                        Severity::Usage,
                        bad_url_text(token),
                        css_path,
                        origin.position(css, e.span.start),
                        "css.url.bad_url_token",
                        vec![token.to_string()],
                    );
                }
                continue;
            }
            // A top-level `--foo:hover { … }`, which CSS Syntax drops without
            // naming a parse error. epubcheck is silent on it and so were we.
            SyntaxErrorKind::DroppedCustomPropertyRule => continue,
            // styloria 0.13: every token is a selector piece but the shape
            // breaks Selectors (`#1a`, `[x]p`, `p::before span`,
            // `:nth-child(foo)`). epubcheck 5.4.0's grammar accepts all of
            // them, so a CSS-008 here would be ours alone; what CSS says is
            // ADV-017. Skipped before `claimed`, so `#1a, . b` still reports
            // the `. b`. styloria drops a bad argument of a forgiving
            // `:is()`/`:where()` without an error, so every one reported
            // here does drop the whole rule.
            SyntaxErrorKind::InvalidSelectorStructure => {
                if advisory && let Some(token) = css.get(e.span.start..e.span.end) {
                    let prelude = innermost_rule(&sheet.rules, e.span.start)
                        .and_then(|r| prelude_source(css, r))
                        .map(|(_, p)| p);
                    report.push_full(
                        ADV_017,
                        Severity::Usage,
                        selector_structure_text(token, prelude),
                        css_path,
                        origin.position(css, e.span.start),
                        "css.stylesheet.invalid_selector_structure",
                        vec![token.to_string()],
                    );
                }
                continue;
            }
            // styloria 0.16: a pseudo-class or pseudo-element no CSS
            // specification defines (`a:hovr`, `p::befor`, a bare `p:not`).
            // Selectors 4 makes the selector invalid and browsers drop the
            // rule, but epubcheck 5.4.0 accepts every one of them, so this is
            // ADV-019 for the reason ADV-017 is: skipped before `claimed`.
            SyntaxErrorKind::UnknownPseudo => {
                if advisory && let Some(token) = css.get(e.span.start..e.span.end) {
                    let prelude = innermost_rule(&sheet.rules, e.span.start)
                        .and_then(|r| prelude_source(css, r))
                        .map(|(_, p)| p);
                    report.push_full(
                        ADV_019,
                        Severity::Usage,
                        unknown_pseudo_text(token, prelude),
                        css_path,
                        origin.position(css, e.span.start),
                        "css.selector.unknown_pseudo",
                        vec![token.to_string()],
                    );
                }
                continue;
            }
            // Spelled out rather than `_`, so a kind styloria adds breaks
            // the build here, where CSS-008 or not is decided.
            SyntaxErrorKind::BadString
            | SyntaxErrorKind::UnterminatedRule
            | SyntaxErrorKind::UnterminatedBlock
            | SyntaxErrorKind::MalformedDeclaration
            | SyntaxErrorKind::UnexpectedToken
            | SyntaxErrorKind::InvalidSelector
            | SyntaxErrorKind::InvalidUnicodeRange
            | SyntaxErrorKind::NestingTooDeep => {}
        }
        let owner = (e.kind == SyntaxErrorKind::InvalidSelector)
            .then(|| innermost_rule(&sheet.rules, e.span.start))
            .flatten();
        if let Some(rule) = owner {
            if claimed == Some(rule.span) {
                continue;
            }
            claimed = Some(rule.span);
        }
        let prelude = owner.and_then(|r| prelude_source(css, r));
        if let Some(classes) = prelude.and_then(epubcheck_only_class_names) {
            if advisory {
                for (start, end) in classes {
                    let class = &css[start..end];
                    report.push_full(
                        ADV_012,
                        Severity::Usage,
                        format!(
                            "'{class}' is not a class selector: a class name must be a CSS \
                             identifier, so browsers ignore this whole rule"
                        ),
                        css_path,
                        origin.position(css, start),
                        "css.selector.class_not_identifier",
                        vec![class.to_string()],
                    );
                }
            }
            continue;
        }
        let slug = match e.kind {
            SyntaxErrorKind::MalformedDeclaration | SyntaxErrorKind::UnexpectedToken => {
                "css.declaration.malformed_shape"
            }
            kind => syntax_error_slug(kind),
        };
        report.push_full(
            CSS_008,
            Severity::Error,
            syntax_error_text(css, e, prelude.map(|(_, p)| p)),
            css_path,
            origin.position(css, e.span.start),
            slug,
            Vec::new(),
        );
    }
}

/// The class names epubcheck accepts and CSS does not in a selector, given as
/// it is written and where it starts, as byte ranges of the stylesheet (dot
/// included) — or `None` when there are none, or when the selector is still
/// invalid without them.
fn epubcheck_only_class_names((start, prelude): (usize, &str)) -> Option<Vec<(usize, usize)>> {
    let (relaxed, found) = relax_class_names(prelude);
    if found.is_empty() {
        return None;
    }
    let (_, errors) = styloria::parse_stylesheet(&format!("{relaxed}{{}}"));
    // A structure error left over (`.1a#2b` → `.xa#2b`) is one epubcheck
    // accepts too, so it does not make the class name a CSS-008.
    let still_invalid = errors
        .iter()
        .any(|e| e.kind != SyntaxErrorKind::InvalidSelectorStructure);
    (!still_invalid).then(|| {
        found
            .into_iter()
            .map(|(s, e)| (start + s, start + e))
            .collect()
    })
}

/// `prelude` with every class name epubcheck's scanner accepts but CSS does
/// not replaced by a same-length placeholder identifier, and where each was.
///
/// epubcheck's rule, from `CssScanner`: a `.` starts a class when the next
/// character is a `{nmchar}` (`[_a-zA-Z0-9-]` or non-ASCII) — unless it is a
/// digit, which makes the `.` part of a number (`.5`), as does a run of
/// digits before it (`2.5`). The class then runs over every `{nmchar}`.
/// Strings, comments and attribute brackets are skipped, and a class name
/// with an escape in it is left alone (it would take CSS's escape rules to
/// judge, and no book has needed that).
fn relax_class_names(prelude: &str) -> (String, Vec<(usize, usize)>) {
    fn nmchar(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '-' || !c.is_ascii()
    }
    fn is_ident(name: &str) -> bool {
        let mut cs = name.chars();
        let start = |c: char| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii();
        match (cs.next(), cs.next()) {
            (Some('-'), Some(c)) => c == '-' || start(c),
            (Some(c), _) => start(c),
            (None, _) => false,
        }
    }
    let mut out = prelude.to_string();
    let mut found = Vec::new();
    let b = prelude.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = prelude[i + 2..]
                    .find("*/")
                    .map_or(b.len(), |p| i + 2 + p + 2);
            }
            b'[' => i = prelude[i..].find(']').map_or(b.len(), |p| i + p + 1),
            b'.' => {
                let before = prelude[..i]
                    .chars()
                    .rev()
                    .take_while(|c| nmchar(*c))
                    .collect::<String>();
                let is_number = !before.is_empty() && before.bytes().all(|c| c.is_ascii_digit());
                let name_len: usize = prelude[i + 1..]
                    .chars()
                    .take_while(|c| nmchar(*c))
                    .map(char::len_utf8)
                    .sum();
                let name = &prelude[i + 1..i + 1 + name_len];
                let escaped = prelude[i + 1 + name_len..].starts_with('\\');
                if !is_number
                    && !name.is_empty()
                    && !name.starts_with(|c: char| c.is_ascii_digit())
                    && !escaped
                    && !is_ident(name)
                {
                    out.replace_range(i + 1..i + 1 + name_len, &"x".repeat(name_len));
                    found.push((i, i + 1 + name_len));
                }
                i += 1 + name_len;
            }
            _ => i += 1,
        }
    }
    (out, found)
}

/// The `rule` slug for one of styloria's syntax errors. Every one of them is
/// CSS-008 to epubcheck; the slug is where a consumer can tell them apart.
///
/// Shared by the top-level pass and the nested one, so a rule inside an
/// `@media` is keyed the same as the identical rule outside it - the two had
/// no reason to differ, and only one of them existed before styloria 0.9.
fn syntax_error_slug(kind: SyntaxErrorKind) -> &'static str {
    match kind {
        SyntaxErrorKind::BadString | SyntaxErrorKind::BadUrl => "css.stylesheet.bad_token",
        SyntaxErrorKind::UnterminatedRule | SyntaxErrorKind::UnterminatedBlock => {
            "css.stylesheet.unterminated"
        }
        SyntaxErrorKind::MalformedDeclaration | SyntaxErrorKind::UnexpectedToken => {
            "css.stylesheet.malformed"
        }
        // styloria 0.5 reads a qualified rule's prelude as a selector
        // list. Its own slug, so the finding says which half of the rule
        // was wrong: epubcheck reports both as CSS-008, but "the selector
        // is malformed" and "the declarations are malformed" send an
        // author to different places.
        SyntaxErrorKind::InvalidSelector => "css.stylesheet.invalid_selector",
        // Never a CSS-008: `report_syntax_errors` makes it ADV-017 before
        // asking, under this same key.
        SyntaxErrorKind::InvalidSelectorStructure => "css.stylesheet.invalid_selector_structure",
        // Never a CSS-008: `report_syntax_errors` makes it ADV-019 first.
        SyntaxErrorKind::UnknownPseudo => "css.selector.unknown_pseudo",
        // Its own slug for the same reason as the selector one: epubcheck
        // reports every CSS parse problem as CSS-008, but "the range is
        // malformed" points somewhere quite different from "the block is".
        SyntaxErrorKind::InvalidUnicodeRange => "css.stylesheet.invalid_unicode_range",
        // styloria 0.7's nesting bound. Its own slug because this one is
        // not a defect in the CSS the way the others are - it says the
        // parser declined to descend further, and the stylesheet below
        // that point went unchecked. Real stylesheets nest 2 deep, so
        // reaching 256 means generated or hostile input; reporting it
        // under a shared "malformed" slug would hide which it was.
        SyntaxErrorKind::NestingTooDeep => "css.stylesheet.nesting_too_deep",
        // Never reported: `report_syntax_errors` skips it before asking.
        SyntaxErrorKind::DroppedCustomPropertyRule => "css.stylesheet.dropped_rule",
    }
}

/// The text of a CSS-008: what kind of syntax error, and the source it is
/// about.
///
/// It was a bare "CSS syntax error" for every kind until a reader comparing
/// the two tools on their library pointed out that epubcheck says which token
/// it choked on and we said nothing (Reddit, 2026-10-02). The kind was known
/// all along, but only the `rule` slug carried it, where a person reading the
/// report never looks. The wording is ours; the facts are the span styloria
/// already reports, which is the offending token itself.
fn syntax_error_text(css: &str, e: &SyntaxError, selector: Option<&str>) -> String {
    use SyntaxErrorKind as K;
    let token = quote_css(&css[e.span.start..e.span.end]);
    let detail = match (e.kind, &token) {
        (K::InvalidSelector | K::InvalidSelectorStructure, _) => {
            // The token alone can be a lone `.` or even a space (`p, {`), so
            // the whole selector is what tells the reader which rule it is.
            let selector = selector.and_then(quote_css);
            match (selector, &token) {
                (Some(s), Some(t)) if s != *t => format!("invalid selector '{s}' at '{t}'"),
                (Some(s), _) => format!("invalid selector '{s}'"),
                (None, Some(t)) => format!("invalid selector at '{t}'"),
                (None, None) => "invalid selector".to_string(),
            }
        }
        (K::UnterminatedBlock, Some(t)) => format!("'{t}' is never closed"),
        (K::UnterminatedRule, _) => "the stylesheet ends before this rule's { } block".to_string(),
        (K::BadString, Some(t)) => format!("string '{t}' is broken by an unescaped line break"),
        (K::BadUrl, Some(t)) => format!(
            "'{t}' is malformed: an unquoted url() cannot contain unescaped spaces, \
             quotes or parentheses"
        ),
        (K::InvalidUnicodeRange, Some(t)) => {
            format!("unicode-range '{t}' has more than six hex digits")
        }
        (K::NestingTooDeep, _) => format!(
            "blocks nested more than {} deep; what they hold was not checked",
            styloria::MAX_NESTING_DEPTH
        ),
        // Never reported: `report_syntax_errors` makes it ADV-019 first.
        (K::UnknownPseudo, Some(t)) => format!("'{t}' is not a known pseudo-class"),
        (K::MalformedDeclaration, Some(t)) => format!("'{t}' is not followed by ':'"),
        (K::UnexpectedToken, Some(t)) => format!("'{t}' where a declaration was expected"),
        // Never reported: `report_syntax_errors` skips it before asking.
        (K::DroppedCustomPropertyRule, Some(t)) => format!("'{t}' is dropped"),
        // Every span above is a token, which is never blank; this is the
        // floor for one that is, not a shape anything is known to produce.
        (_, None) => return "CSS syntax error".to_string(),
    };
    format!("CSS syntax error: {detail}")
}

/// The text of an ADV-017: the selector, the piece where its shape breaks,
/// and what that costs. `token` is never blank (styloria spans a piece of
/// the selector), but the selector can be missing or the piece itself.
fn selector_structure_text(token: &str, selector: Option<&str>) -> String {
    let t = quote_css(token).unwrap_or_else(|| token.to_string());
    let at = match selector.and_then(quote_css) {
        Some(s) if s != t => format!("'{s}' at '{t}'"),
        Some(s) => format!("'{s}'"),
        None => format!("'{t}'"),
    };
    format!("selector {at} is not valid under CSS Selectors, so browsers ignore this whole rule")
}

/// The text of an ADV-019: the selector and the pseudo no specification
/// defines in that form. `token` is the `:name` or `::name(…)` as written.
fn unknown_pseudo_text(token: &str, selector: Option<&str>) -> String {
    let t = quote_css(token).unwrap_or_else(|| token.to_string());
    let in_selector = match selector.and_then(quote_css) {
        Some(s) if s != t => format!(" in selector '{s}'"),
        _ => String::new(),
    };
    format!(
        "'{t}'{in_selector} is not a pseudo-class or pseudo-element any CSS specification \
         defines in this form, so browsers ignore this whole rule"
    )
}

/// `source` as a message quotes it: whitespace runs folded to one space (a
/// selector list is often written one selector per line) and long text cut
/// with an ellipsis. `None` when nothing but whitespace is left.
fn quote_css(source: &str) -> Option<String> {
    const MAX_CHARS: usize = 60;
    let folded = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.is_empty() {
        return None;
    }
    if folded.chars().count() <= MAX_CHARS {
        return Some(folded);
    }
    let cut: String = folded.chars().take(MAX_CHARS).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// Whitespace only - what an empty selector prelude looks like once the
/// tokenizer has kept everything. Comments are not component values in
/// styloria's output, so they need no arm here.
fn is_blank_component(v: &ComponentValue) -> bool {
    matches!(v, ComponentValue::Token(Token::Whitespace))
}

/// Walk a style rule's block: the EPUB rules for each declaration, and one
/// CSS-008 for each rule nested in it.
///
/// **A nested rule is one finding, and what is inside it is not read.** The
/// only rules that can sit in a style rule's block are nested ones, which is
/// CSS Nesting, and nesting is in §2.4 of the CSS Snapshot ("modules with
/// rough interoperability"), not in the official definition of CSS that EPUB
/// 3.3 defers to. epubcheck reports it. styloria 0.11 read `p { a { … } }`
/// as one malformed declaration; 0.12 follows the 2026 CSS Syntax CRD and
/// reads a rule, so the finding is ours to make now, and `quiet` keeps what
/// styloria finds inside the rule from becoming more findings. A nested
/// at-rule had its own slug already; a nested style rule gets the matching
/// one.
fn walk_style_block(items: &[BlockItem], ctx: Ctx, quiet: &mut Quiet, report: &mut Report) {
    let Ctx {
        css,
        css_path,
        origin,
        ..
    } = ctx;
    for item in items {
        match item {
            BlockItem::Declaration(d) => check_declaration(d, ctx, report),
            BlockItem::Rule(r) => {
                quiet.push(r.span);
                let (text, at, slug) = match &r.node {
                    Rule::At(a) => (
                        format!(
                            "CSS syntax error: at-rule '{}' inside a style rule",
                            &css[a.name_span.start..a.name_span.end]
                        ),
                        a.name_span.start,
                        "css.declaration.nested_at_rule",
                    ),
                    Rule::Qualified(_) => (
                        match prelude_source(css, r).and_then(|(_, p)| quote_css(p)) {
                            Some(s) => format!("CSS syntax error: rule '{s}' inside a style rule"),
                            None => "CSS syntax error: a rule inside a style rule".to_string(),
                        },
                        r.span.start,
                        "css.declaration.nested_rule",
                    ),
                };
                report.push_full(
                    CSS_008,
                    Severity::Error,
                    text,
                    css_path,
                    origin.position(css, at),
                    slug,
                    Vec::new(),
                );
            }
        }
    }
}

/// Walk a style rule's block — unless a `{` inside the rule never closed, in
/// which case what the parser found in the block is the unclosed brace's
/// doing, and the block is quieted instead (see `check`).
fn walk_rule_block(
    rule: &Spanned<Rule>,
    block: &Block,
    ctx: Ctx,
    quiet: &mut Quiet,
    report: &mut Report,
) {
    if ctx.never_closed(rule.span) {
        quiet.push(block.span);
    } else {
        walk_style_block(&block.node, ctx, quiet, report);
    }
}

/// Walk an at-rule's block: the EPUB rules for each declaration, a style
/// rule's walk for each rule nested in it, and the same walk again for each
/// nested at-rule.
///
/// **Which at-rule holds what is not asked here.** styloria 0.12 reads every
/// block the same way (CSS Syntax CRD 2026-10-01, §5.5.5), so a block is
/// whatever is written in it, and this walk reports on that. It used to ask
/// styloria's table whether a block held rules or declarations, and before
/// that a `GROUPING_AT_RULES` list kept here, and the trouble with a CSS table
/// living in an EPUB validator was not theoretical. That list had never heard
/// of `@keyframes`, so `0% { opacity: 0 }` became a malformed declaration:
/// CSS-008 on valid CSS, in every animated fixed-layout book. A `@keyframes`
/// block is now rules like any other, and styloria does not read their
/// preludes as selectors.
///
/// **`@font-face` is the one exception, because epubcheck makes it one.** A
/// rule inside `@font-face { … }` draws CSS-008 there (measured, 5.4.0), so
/// it is reported as it would be in a style rule. Elsewhere, a declaration
/// sitting directly in `@media` is not reported: epubcheck is silent on it,
/// and styloria 0.11's reading of it as an unterminated rule was a false
/// positive here.
///
/// Nested rules still get the prelude check styloria 0.9's `parse_rule_list`
/// brought (its issue #2): `. foo { }` was once reported at the top level
/// and silently accepted one `@media` deep. That gap was found by
/// `compare`'s count diff rather than by a user — one shelf book where
/// epubcheck reported 11 CSS-008 and we reported 0, every one a selector
/// inside an `@media`, invisible in the totals because declaration errors in
/// the same blocks were reported normally.
fn walk_at_rule_block(
    name: &str,
    items: &[BlockItem],
    ctx: Ctx,
    quiet: &mut Quiet,
    report: &mut Report,
) {
    if name.eq_ignore_ascii_case("font-face") {
        // A nested at-rule here was silent before 0.12 and has not been
        // measured against epubcheck, so it stays silent; only the rule,
        // which was, is reported.
        for item in items {
            match item {
                BlockItem::Rule(r) if matches!(r.node, Rule::At(_)) => quiet.push(r.span),
                _ => walk_style_block(std::slice::from_ref(item), ctx, quiet, report),
            }
        }
        return;
    }
    for item in items {
        match item {
            BlockItem::Declaration(d) => check_declaration(d, ctx, report),
            BlockItem::Rule(r) => match &r.node {
                Rule::Qualified(q) => walk_rule_block(r, &q.block, ctx, quiet, report),
                Rule::At(a) => {
                    // **Three at-rules exist only at the top level**, and
                    // epubcheck 5.4.0 reports each of them inside an `@media`
                    // or `@supports` block as CSS-008 ("not allowed here"),
                    // while accepting every other at-rule there, unknown ones
                    // included (probed 2026-10-22). CSS agrees: Cascade 5 and
                    // CSS Namespaces place `@import` and `@namespace` before
                    // every other rule, and `@charset` is not a rule at all
                    // but bytes at the start of the file. We were silent.
                    if ["charset", "import", "namespace"]
                        .iter()
                        .any(|n| a.name.eq_ignore_ascii_case(n))
                    {
                        let name = &ctx.css[a.name_span.start..a.name_span.end];
                        report.push_full(
                            CSS_008,
                            Severity::Error,
                            format!(
                                "CSS syntax error: '{name}' is only allowed at the top level \
                                 of a stylesheet"
                            ),
                            ctx.css_path,
                            ctx.origin.position(ctx.css, a.name_span.start),
                            "css.stylesheet.misplaced_at_rule",
                            Vec::new(),
                        );
                        continue;
                    }
                    let Some(block) = &a.block else { continue };
                    // **`@font-face` is `@font-face` wherever it sits.** The
                    // top-level walk calls this and the nested one did not, so
                    // `@media all { @font-face { … } }` skipped every
                    // `@font-face` rule - CSS-028 among them, which epubcheck
                    // reports there (measured, one book). A conditional group
                    // is a container, not a different language.
                    if a.name.eq_ignore_ascii_case("font-face") {
                        check_font_face(block, a.name_span, ctx, report);
                    }
                    walk_at_rule_block(&a.name, &block.node, ctx, quiet, report);
                }
            },
        }
    }
}

/// The EPUB rules about one declaration: `OBS-001`, `CSS-001`, `CSS-006`.
///
/// None of them is a CSS rule — CSS has nothing against `direction` — which
/// is why they live here rather than in styloria.
fn check_declaration(d: &Spanned<Declaration>, ctx: Ctx, report: &mut Report) {
    let Ctx {
        css,
        css_path,
        origin,
        is_epub3,
        ..
    } = ctx;
    let name = &d.node.name;
    let at = origin.position(css, d.node.name_span.start);
    // `OBS-001`: EPUB 3.4 marks the `-epub-` prefixed properties, and the
    // `-epub-fullwidth` value of `text-transform`, as outdated
    // (epubcheck 5.4.0, `CSSHandler.java`:277-287). Usage severity and
    // ungated by version — epubcheck asks this before its own EPUB 3
    // branch below, so an EPUB 2 stylesheet gets it too.
    if name.starts_with("-epub-") {
        report.push_full(
            OBS_001,
            Severity::Usage,
            format!("usage of the CSS prefixed property '{name}' is outdated"),
            css_path,
            at,
            "css.outdated_prefixed_property",
            vec![name.to_string()],
        );
    } else if name.eq_ignore_ascii_case("text-transform") && has_epub_fullwidth(&d.node.value) {
        report.push_full(
            OBS_001,
            Severity::Usage,
            "usage of the CSS prefixed value '-epub-fullwidth' is outdated",
            css_path,
            at,
            "css.outdated_prefixed_value",
            vec!["-epub-fullwidth".to_string()],
        );
    }
    // CSS-001 is EPUB 3 only. epubcheck guards it with
    // `if (version == EPUBVersion.VERSION_3)` (CSSHandler.java:288) and
    // keeps its fixtures under `src/test/resources/epub3/`; its two
    // neighbours in the same method - CSS-006 below and the @font-face
    // work - are not guarded, so this is the whole class, not a sample.
    // We had no gate at all, which invented an error on an EPUB 2 book
    // carrying `<h1 style="direction: inherit">`.
    if is_epub3
        && FLAGGED_PROPERTIES
            .iter()
            .any(|p| name.eq_ignore_ascii_case(p))
    {
        report.push_at_pos(
            CSS_001,
            Severity::Error,
            format!("use of the '{name}' property is not recommended"),
            css_path,
            at,
        );
    } else if name.eq_ignore_ascii_case("position") && is_position_fixed(&d.node.value) {
        // CSS-006: `position: fixed` (matches epubcheck, which compares
        // the first value component to "fixed", case-insensitively).
        report.push_at_pos(
            CSS_006,
            Severity::Usage,
            "use of 'position: fixed' is not recommended".to_string(),
            css_path,
            at,
        );
    }
}

/// A `text-transform` value naming `-epub-fullwidth` at its top level.
fn has_epub_fullwidth(value: &[Spanned<ComponentValue>]) -> bool {
    value.iter().any(|v| {
        matches!(&v.node, ComponentValue::Token(Token::Ident(x))
            if x.eq_ignore_ascii_case("-epub-fullwidth"))
    })
}

/// A `position` value whose first component is `fixed`, as epubcheck reads
/// it. styloria trims a value's leading whitespace, so the first component is
/// the first word.
fn is_position_fixed(value: &[Spanned<ComponentValue>]) -> bool {
    value
        .iter()
        .find(|v| !matches!(&v.node, ComponentValue::Token(Token::Whitespace)))
        .is_some_and(|v| {
            matches!(&v.node, ComponentValue::Token(Token::Ident(x))
                if x.eq_ignore_ascii_case("fixed"))
        })
}

/// A `file:` URL, by scheme. Shared so the generic `url()` pass and the
/// `@font-face` one cannot drift apart — they are two sites asking one
/// question, which is exactly how the `@font-face` gap opened.
fn is_file_url_str(url: &str) -> bool {
    crate::url::trim_url(url).starts_with("file:")
}

fn check_font_face(block: &Block, name_span: Span, ctx: Ctx, report: &mut Report) {
    let Ctx {
        css,
        css_path,
        origin,
        ..
    } = ctx;
    // CSS-028 (usage): purely informational - real epubcheck notes every
    // `@font-face` it sees, so a reader comparing the two outputs isn't
    // left wondering which tool missed an embedded font. Anchored at the
    // `@font-face` keyword; nothing about the rule is wrong.
    //
    // **An empty block gets none**, and that is the one place our granularity
    // difference changes sign. epubcheck reports CSS-028 from its *declaration*
    // handler, inside `if (inFontFace)`, so it gives one per declaration and we
    // give one per rule - a difference documented in `COVERAGE.md`, where our
    // count is always the lower of the two. With no declarations at all its
    // count is zero and ours was one, which is a false positive rather than a
    // granularity difference; `content-css-font-face-empty-error` is
    // epubcheck's own fixture of it, and there the whole stylesheet is
    // `@font-face {\n}`.
    //
    // Reported before the emptiness test rather than after it, which is why
    // this hid: the early `return` below is what makes the block empty *and*
    // makes it look handled.
    //
    // "Empty" is whitespace between the braces, read off the source rather
    // than off the parsed items: a lone `;` is no item to styloria but was
    // never an empty block here.
    let empty = block_is_blank(css, block);
    if !empty {
        report.push_full(
            CSS_028,
            Severity::Usage,
            "@font-face declaration",
            css_path,
            origin.position(css, name_span.start),
            "css.font_face.declared",
            Vec::new(),
        );
    }
    if empty {
        // An empty block has no token to point at, so anchor CSS-019 at the
        // `@font-face` keyword itself.
        report.push_at_pos(
            CSS_019,
            Severity::Warning,
            "@font-face has an empty declaration block",
            css_path,
            origin.position(css, name_span.start),
        );
        return;
    }
    for src in font_face_src_declarations(&block.node) {
        let src_urls = declaration_urls(src);
        // RSC-030 has to be asked here as well as in the generic `urls` pass,
        // because that pass deliberately skips `@font-face` blocks and hands
        // them to this function — so every question it asks about a url has
        // to be asked again here or it is asked about nothing. epubcheck
        // reports two file-url errors on its own `file-url-in-css-error`
        // fixture (the manifest item and the `src`); we reported the manifest
        // one alone.
        for u in src_urls.iter().filter(|u| is_file_url_str(&u.node)) {
            report.push_full(
                RSC_030,
                Severity::Error,
                format!("'{}' is a file URL, which is not allowed", u.node),
                css_path,
                origin.position(css, u.span.start),
                "css.url.file_scheme_not_allowed",
                vec![u.node.clone()],
            );
        }
        if let Some(empty) = src_urls.iter().find(|u| u.node.is_empty()) {
            report.push_at_pos(
                CSS_002,
                Severity::Error,
                "@font-face 'src' has an empty url()",
                css_path,
                origin.position(css, empty.span.start),
            );
        }
    }
}

/// Nothing but whitespace (and comments, which the tokenizer drops) between
/// a block's braces. A block that never closed has no `}` to leave out.
fn block_is_blank(css: &str, block: &Block) -> bool {
    let inner = css
        .get(block.span.start..block.span.end)
        .unwrap_or("")
        .strip_prefix('{')
        .unwrap_or("");
    let inner = inner.strip_suffix('}').unwrap_or(inner);
    styloria::Tokenizer::new(inner).all(|t| matches!(t, Token::Whitespace))
}

/// The `src` declarations of a `@font-face` block, each as its block item.
fn font_face_src_declarations<'b, 'c>(
    items: &'b [BlockItem<'c>],
) -> impl Iterator<Item = &'b BlockItem<'c>> {
    items.iter().filter(
        |item| matches!(item, BlockItem::Declaration(d) if d.node.name.eq_ignore_ascii_case("src")),
    )
}

/// The urls in one declaration (a block item), as [`checked_url`] reads them.
fn declaration_urls(declaration: &BlockItem) -> Vec<Spanned<String>> {
    styloria::block_url_references(std::slice::from_ref(declaration))
        .iter()
        .filter_map(|r| checked_url(r))
        .collect()
}

/// The `url()` target of every `@font-face`'s `src` declaration, each with
/// the span of the token it came from - unlike the generic url pass in
/// `check` (which deliberately skips `@font-face` blocks, handling them via
/// `check_font_face` instead), this is used by the CSS-007 non-standard-font
/// cross-reference in `opf.rs`, which needs each font's own resolved
/// manifest media-type to decide whether it's a Core Media Type.
///
/// Spans are carried so CSS-007 can point at the `src` url that names the
/// font, rather than at the stylesheet as a whole - "some font in this file
/// is wrong" leaves the reader to find which, and a stylesheet can declare
/// many. Top-level `@font-face` rules only, as the checks always read them.
pub(crate) fn font_face_src_urls_spanned(css: &str) -> Vec<Spanned<String>> {
    let (sheet, _) = styloria::parse_stylesheet(css);
    let mut out = Vec::new();
    for rule in &sheet.rules {
        let Rule::At(a) = &rule.node else {
            continue;
        };
        if !a.name.eq_ignore_ascii_case("font-face") {
            continue;
        }
        let Some(block) = &a.block else { continue };
        for src in font_face_src_declarations(&block.node) {
            out.extend(declaration_urls(src));
        }
    }
    out.retain(|u| !u.node.is_empty());
    out
}

/// The top-level rule a url sits in, which is what tells the url passes
/// apart: `check` hands a `@font-face` block to `check_font_face`,
/// `stylesheet_urls` leaves `@namespace` out, `import_targets` wants only
/// `@import`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Under {
    Import,
    Namespace,
    /// The block of a `@font-face`; its prelude counts as [`Under::Other`].
    FontFaceBlock,
    Other,
}

/// One url the checks read, and where it sits.
struct SheetUrl {
    url: Spanned<String>,
    under: Under,
    /// `@import "a.css"`: the bare-string form.
    bare: bool,
    /// Inside another function or a block: see [`nesting_spans`].
    nested: bool,
}

/// One url a stylesheet references, for the package-level checks in
/// `opf.rs`.
pub(crate) struct CssUrl {
    pub(crate) url: String,
    /// Inside another function (`image-set(url(a.png) 1x)`), which epubcheck
    /// never reads: it counts as a use of the resource, and nothing in the
    /// default output may be asked of it (NEXT-018).
    pub(crate) nested: bool,
}

/// The spans of the outermost functions (other than `url()` and `src()`) and
/// simple blocks in every rule prelude and declaration value, sorted. A url
/// inside one is one epubcheck never reads: `CSSHandler.registerURIs` looks at
/// a declaration's top-level components only.
fn nesting_spans(sheet: &Stylesheet) -> Vec<Span> {
    fn values(vs: &[Spanned<ComponentValue>], out: &mut Vec<Span>) {
        for v in vs {
            match &v.node {
                ComponentValue::Function { name, .. }
                    if name.eq_ignore_ascii_case("url") || name.eq_ignore_ascii_case("src") => {}
                ComponentValue::Function { .. } | ComponentValue::Block(_) => out.push(v.span),
                _ => {}
            }
        }
    }
    fn rule(r: &Spanned<Rule>, out: &mut Vec<Span>) {
        values(r.node.prelude(), out);
        if let Some(block) = r.node.block() {
            for item in &block.node {
                match item {
                    BlockItem::Declaration(d) => values(&d.node.value, out),
                    BlockItem::Rule(r) => rule(r, out),
                }
            }
        }
    }
    let mut out = Vec::new();
    for r in &sheet.rules {
        rule(r, &mut out);
    }
    out.sort_by_key(|s| s.start);
    out
}

/// Every url in a stylesheet the checks read, in source order: styloria's
/// [`url_references`](styloria::url_references), kept to the reach these
/// checks have always had (see [`checked_url`]). A bare string counts only as
/// the target of a top-level `@import`.
fn sheet_urls(sheet: &Stylesheet) -> Vec<SheetUrl> {
    let tops: Vec<(Span, Under, Option<Span>)> = sheet
        .rules
        .iter()
        .map(|rule| match &rule.node {
            Rule::At(a) if a.name.eq_ignore_ascii_case("import") => {
                (rule.span, Under::Import, None)
            }
            Rule::At(a) if a.name.eq_ignore_ascii_case("namespace") => {
                (rule.span, Under::Namespace, None)
            }
            Rule::At(a) if a.name.eq_ignore_ascii_case("font-face") => {
                (rule.span, Under::Other, a.block.as_ref().map(|b| b.span))
            }
            _ => (rule.span, Under::Other, None),
        })
        .collect();
    let under = |at: usize| {
        let i = tops.partition_point(|(span, ..)| span.start <= at);
        match i.checked_sub(1).map(|i| tops[i]) {
            Some((_, _, Some(block))) if block.start <= at && at < block.end => {
                Under::FontFaceBlock
            }
            Some((span, under, _)) if at < span.end => under,
            _ => Under::Other,
        }
    };
    let nesting = nesting_spans(sheet);
    let nested = |span: Span| {
        let i = nesting.partition_point(|n| n.start <= span.start);
        i.checked_sub(1)
            .is_some_and(|i| nesting[i].start < span.start && span.end <= nesting[i].end)
    };
    styloria::url_references(sheet)
        .iter()
        .filter_map(|r| {
            let under = under(r.span.start);
            let bare = r.form == UrlForm::String;
            if bare && !(under == Under::Import && r.context == UrlContext::Import) {
                return None;
            }
            Some(SheetUrl {
                url: checked_url(r)?,
                under,
                bare,
                nested: nested(r.span),
            })
        })
        .collect()
}

/// The url a check reads from one of styloria's references, or `None` where
/// the reference is beyond what the checks have read so far.
///
/// styloria finds more than the token scan it replaced. `src("…")` and the
/// strings inside `image-set()` stay unread: whether they are URLs an EPUB
/// reading system fetches is unsettled in the spec, and epubcheck reads
/// neither, so we follow it (probe set 2026-10-21_css-reach, r2/r3). A `<bad-url-token>` is read as epubcheck
/// reads it ([`bad_url_target`]).
fn checked_url(r: &UrlReference) -> Option<Spanned<String>> {
    let url = match r.form {
        // `url( "a.png" )` counts, whitespace and all: epubcheck's scanner
        // reads `url\({w}{string}{w}\)` as one token, and CSS Syntax makes it a
        // `url()` function holding the string. Reading only the unspaced form
        // called a file used that way unreferenced (OPF-097) and missed it
        // missing (RSC-007), where epubcheck says neither (probe set
        // 2026-10-21_css-reach, r1/m1).
        UrlForm::UrlToken | UrlForm::UrlFunction | UrlForm::String => r.url.to_string(),
        UrlForm::BadUrl => bad_url_target(&r.url),
        UrlForm::SrcFunction => return None,
    };
    Some(Spanned::new(url, r.span))
}

/// The target epubcheck reads out of an unquoted `url( … )` that CSS calls a
/// `<bad-url-token>`, given the token's source text.
///
/// **The verdict on such a url is epubcheck's.** CSS Syntax §4.3.6 makes
/// `url(a b.png)`, `url(q'q.png)` and `url(p(p.png)` bad-url tokens, which a
/// browser drops along with their declaration. epubcheck's scanner
/// (`CssScanner._uri`) has no such state: past `url(` and any whitespace it
/// reads every character up to the first `)`, trims trailing whitespace, and
/// hands the rest on as an ordinary URL. So `a b.png` draws RSC-020 there, a
/// missing target RSC-007, and `q'q.png` naming a file in the container draws
/// nothing at all. We reported CSS-008 for all three, which failed a book
/// epubcheck passes; this reads the url the way it does instead, and the
/// usual url checks take it from there (measured against 5.4.0).
fn bad_url_target(token: &str) -> String {
    let inner = token.find('(').map_or("", |at| &token[at + 1..]);
    let inner = inner.trim_start_matches(is_css_whitespace);
    let inner = inner.find(')').map_or(inner, |end| &inner[..end]);
    inner.trim_end_matches(is_css_whitespace).to_string()
}

/// The text of an ADV-015, given a `<bad-url-token>`'s source.
///
/// Every claim in it holds wherever the token sits. CSS Syntax §4.3.6 makes
/// it a bad-url token whatever surrounds it, and no CSS grammar accepts one,
/// so a browser never fetches it. Whether the declaration, the rule or only
/// a descriptor is what gets dropped depends on where it is, so the text
/// does not say which.
fn bad_url_text(token: &str) -> String {
    let shown = quote_css(token).unwrap_or_else(|| "url(".to_string());
    format!(
        "'{shown}' is not a url() a browser can read: an unquoted url cannot contain \
         unescaped spaces, quotes, parentheses, control characters or a \
         backslash before a line break, so it is \
         never loaded (epubcheck reads it as '{}')",
        bad_url_target(token)
    )
}

/// CSS Syntax's whitespace: space, tab and the newlines. Not
/// `char::is_whitespace`, which also takes in U+00A0 and friends.
fn is_css_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// Just the `url()` target(s) of top-level `@import` rules, not every `url()`
/// in the sheet (unlike `stylesheet_urls` below) - used where callers need
/// to tell "this points at another stylesheet to also parse" apart from
/// an ordinary resource reference like `background: url(x.png)` (e.g.
/// `opf.rs`'s SVG active-class CSS scan, CSS-029/030, which needs to
/// merge an `@import`ed sheet's own selector class names, not just note
/// its existence as a used resource). The bare-string form is not read here.
pub(crate) fn import_targets(sheet: &Stylesheet) -> Vec<String> {
    sheet_urls(sheet)
        .into_iter()
        .filter(|u| u.under == Under::Import && !u.bare)
        .map(|u| u.url.node)
        .collect()
}

/// Every `url()` reference anywhere in a stylesheet (rule preludes,
/// declaration blocks, `@import` targets, nested blocks) - shared by
/// `check`'s own resource-resolution pass and, in `opf.rs`, the
/// remote-resources content-property scan (OPF-014/018), so a document's
/// remote references aren't just its raw attribute values but also its
/// own CSS.
///
/// @namespace's "url(...)" declares an XML namespace URI for selectors (e.g.
/// `@namespace xlink url('http://www.w3.org/1999/xlink')`) - it's never a
/// fetchable resource reference, unlike every other at-rule that can carry a
/// url().
pub(crate) fn stylesheet_urls(sheet: &Stylesheet) -> Vec<CssUrl> {
    sheet_urls(sheet)
        .into_iter()
        .filter(|u| u.under != Under::Namespace)
        .map(|u| CssUrl {
            url: u.url.node,
            nested: u.nested,
        })
        .collect()
}

/// Class names used as selectors in a stylesheet's style rules — e.g.
/// `.foo, .bar { ... }` yields `{"foo", "bar"}` — at any depth (inside
/// `@media` as well), but not in the arguments of `:not()` and the like.
///
/// That is epubcheck's reach for CSS-029: its `selectors` callback fires for
/// every style rule, `@media` ones included, and its class search does not
/// enter pseudo-class arguments (probe set 2026-10-21_css-reach, c2–c4). The
/// spec would count the arguments too; reporting there is stricter than
/// epubcheck, so it would be an advisory.
pub(crate) fn selector_class_names(sheet: &Stylesheet) -> HashSet<String> {
    class_names(sheet).map(|n| n.node).collect()
}

/// Every class selector in `css`, each with its span - the same reach as
/// [`selector_class_names`], keeping where it was written.
///
/// CSS-029 needs this: the class name it reports on lives in the
/// stylesheet, so pointing at the content document that merely links that
/// stylesheet sends the reader to a file the name does not appear in.
pub(crate) fn selector_class_names_spanned(css: &str) -> Vec<Spanned<String>> {
    class_names(&styloria::parse_stylesheet(css).0).collect::<Vec<_>>()
}

fn class_names(sheet: &Stylesheet) -> impl Iterator<Item = Spanned<String>> {
    styloria::selector_references(sheet)
        .into_iter()
        .filter(|r| !r.in_argument)
        .filter_map(|r| match r.selector.node {
            SimpleSelector::Class(name) => Some(Spanned::new(name.into_owned(), r.selector.span)),
            _ => None,
        })
}

#[cfg(test)]
mod adv003_tests {
    use super::is_known_element_name;

    /// #28 (JSWolf, MobileRead #92): `h4a` is a type selector for an element
    /// that exists nowhere — valid CSS that matches nothing, so a typo for
    /// `h4` or `.h4a` is invisible without a lint.
    ///
    /// **The rule's whole difficulty is not flagging real names**, and the
    /// shelf measured it: the first version produced eight findings on 84
    /// books, of which five were `center`, `strike` and `rtc` — real elements
    /// an author may legitimately style. After the historical list the count
    /// is one, and that one (`tdiv`) is genuine. A lint that fires once on a
    /// real corpus is the goal, not a defect.
    #[test]
    fn only_names_no_vocabulary_defines_are_unknown() {
        // The reported case, and a clear invention.
        assert!(!is_known_element_name("h4a"));
        assert!(!is_known_element_name("zzz"));

        // XHTML, from the grammar itself.
        for n in ["h4", "div", "p", "span", "table", "body", "menu"] {
            assert!(is_known_element_name(n), "{n} is XHTML");
        }
        // SVG and MathML reuse their own checks' lists.
        assert!(is_known_element_name("circle"));
        assert!(is_known_element_name("mfrac"));
        // Obsolete but real: styling them is not a mistake.
        for n in ["center", "strike", "font", "tt", "rtc", "rb"] {
            assert!(is_known_element_name(n), "{n} is historical but real");
        }
        // Any hyphenated name is a legal custom element and unjudgeable here.
        assert!(is_known_element_name("my-widget"));
        assert!(is_known_element_name("x-"));
        // Case-insensitive, since CSS type selectors are for HTML.
        assert!(is_known_element_name("DIV"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed selector inside a conditional-group at-rule.
    ///
    /// It was reported at the top level and silently accepted one `@media`
    /// deep: styloria kept an at-rule's block as raw component values, so
    /// nothing inside was re-entered as a rule and the selector check was
    /// never reached (styloria#2, fixed in its 0.9 `parse_rule_list`).
    ///
    /// Found by `compare`'s count diff, not a user - one shelf book where
    /// epubcheck reported 11 of these and we reported 0. Its declaration
    /// errors *were* reported from the same blocks, which is what hid it.
    #[test]
    fn a_malformed_selector_inside_a_group_at_rule_is_reported() {
        let idx = HashMap::new();
        let count = |css: &str| run(css, &idx).iter().filter(|i| **i == CSS_008).count();

        // The same defect at both depths, and it now reports at both.
        assert_eq!(count(". foo { color: red }"), 1);
        assert_eq!(count("@media print { . foo { color: red } }"), 1);
        assert_eq!(
            count("@media print { @media (min-width: 0) { . foo { color: red } } }"),
            1
        );
        // A comma list is **one** finding, not one per half. This asserted 2
        // until #81, on the reasoning that the real book carried both halves
        // - true, and never checked against epubcheck, which reports the
        // selector *list* once. Measured at 5.3.0: one book per shape, and a
        // shelf book went 22 -> 12 to match. An assertion is not a constraint
        // on a change until the oracle has seen it.
        assert_eq!(count("@media print { . a, . b { color: red } }"), 1);

        // Slugged as a selector problem, the same as at the top level -
        // epubcheck calls every CSS parse error CSS-008, so the slug is the
        // only thing telling a consumer which half of the rule was wrong.
        let report = run_report("@media print { . foo { color: red } }", &idx);
        assert_eq!(
            report
                .messages
                .iter()
                .find(|m| m.id == CSS_008)
                .and_then(|m| m.rule),
            Some("css.stylesheet.invalid_selector")
        );
    }

    /// Every CSS-008 says what kind of syntax error it is and quotes the
    /// source it is about, rather than the bare "CSS syntax error" each one
    /// used to be. One shape per kind and per call site.
    #[test]
    fn a_css_syntax_error_says_what_and_where() {
        let idx = HashMap::new();
        let texts = |css: &str| -> Vec<String> {
            run_report(css, &idx)
                .messages
                .into_iter()
                .filter(|m| m.id == CSS_008)
                .map(|m| m.text)
                .collect()
        };
        let one = |css: &str| {
            let t = texts(css);
            assert_eq!(t.len(), 1, "{css:?}: {t:?}");
            t.into_iter().next().unwrap()
        };
        assert_eq!(
            one(". foo { color: red }"),
            "CSS syntax error: invalid selector '. foo' at '.'"
        );
        // The offending token is a space here; the selector is what says
        // which rule, and a space is not worth quoting.
        assert_eq!(
            one("p,\n  { color: red }"),
            "CSS syntax error: invalid selector 'p,'"
        );
        assert_eq!(
            one("@media print { h1,\n h2 >>> b { color: red } }"),
            "CSS syntax error: invalid selector 'h1, h2 >>> b' at '>'"
        );
        assert_eq!(
            one("p { color: red"),
            "CSS syntax error: '{' is never closed"
        );
        assert_eq!(
            one("@font-face { font-family: x; unicode-range: U+1234567; }"),
            "CSS syntax error: unicode-range 'U+1234567' has more than six hex digits"
        );
        assert_eq!(
            one("p { color red; }"),
            "CSS syntax error: 'color' is not followed by ':'"
        );
        assert_eq!(
            one("h1 { 12px: x }"),
            "CSS syntax error: '12px' where a declaration was expected"
        );
        assert_eq!(
            one("p { @media print { color: red } }"),
            "CSS syntax error: at-rule '@media' inside a style rule"
        );
        assert!(texts("p { content: \"abc\n\" }").contains(
            &"CSS syntax error: string '\"abc' is broken by an unescaped line break".to_string()
        ));

        // A style attribute says the same, quoting the attribute's own text;
        // it has no position in the document, so none is given.
        let mut report = Report::new();
        check_style_attribute("color red", "doc.xhtml", false, true, &mut report);
        let t: Vec<_> = report.messages.iter().filter(|m| m.id == CSS_008).collect();
        assert_eq!(t.len(), 1);
        assert_eq!(
            t[0].text,
            "CSS syntax error: 'color' is not followed by ':'"
        );
        assert_eq!(t[0].position, None);
    }

    /// A `<bad-url-token>` is read as epubcheck reads it and goes to the url
    /// checks, never to CSS-008. The two silent shapes are the reason:
    /// epubcheck passes them, and a CSS-008 error failed the book here.
    #[test]
    fn a_bad_url_is_read_as_epubcheck_reads_it() {
        let files = ["OEBPS/q'q.png", "OEBPS/p(p.png", "OEBPS/ok.png"];
        let idx: HashMap<_, _> = files
            .iter()
            .map(|f| (f.to_string(), f.to_string()))
            .collect();
        let manifest: HashSet<_> = files.iter().map(|f| f.to_string()).collect();
        let ids = |css: &str| {
            let mut report = Report::new();
            check(
                css,
                "style.css",
                "OEBPS",
                &idx,
                &manifest,
                CssOrigin::File { bytes: None },
                false,
                true,
                &mut report,
            );
            // Usage aside: CSS-028 names the @font-face, not a fault.
            report
                .messages
                .iter()
                .filter(|m| m.severity != Severity::Usage)
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        for silent in [
            "p { background: url(q'q.png) }",
            "p { background: url(p(p.png) }",
            "p { background: url(  q'q.png  ) }",
            "@font-face { font-family: f; src: url(q'q.png) }",
        ] {
            assert!(ids(silent).is_empty(), "{silent:?}: {:?}", ids(silent));
        }
        // A missing target is RSC-007, inside @media as well.
        for missing in [
            "p { background: url(ok.png x) }",
            "@media print { p { background: url(gone x.png) } }",
        ] {
            assert_eq!(ids(missing), vec![RSC_007], "{missing:?}");
        }

        // What CSS says about it is ADV-015, behind --advisory only, once per
        // token, and never an error.
        let css = "p { background: url(q'q.png) }\n@media print { a { b: url(x y) } }";
        let adv = run_advisory(css);
        let found: Vec<_> = adv.messages.iter().filter(|m| m.id == ADV_015).collect();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|m| m.severity == Severity::Usage));
        assert_eq!(
            found[0].text,
            "'url(q'q.png)' is not a url() a browser can read: an unquoted url cannot \
             contain unescaped spaces, quotes, parentheses, control characters or a \
             backslash before a line break, so it is never loaded (epubcheck reads it \
             as 'q'q.png')"
        );
        assert_eq!(found[0].params, vec!["url(q'q.png)".to_string()]);
        assert!(
            !run_report(css, &idx)
                .messages
                .iter()
                .any(|m| m.id == ADV_015)
        );

        assert_eq!(bad_url_target("url(a b.png)"), "a b.png");
        assert_eq!(bad_url_target("url(\n a b.png \t)"), "a b.png");
        assert_eq!(bad_url_target("URL(p(p.png)"), "p(p.png");
        // At EOF there is no `)`: everything after the whitespace is kept.
        assert_eq!(bad_url_target("url(a b.png"), "a b.png");
        // U+00A0 is not CSS whitespace, so it is part of the url.
        assert_eq!(bad_url_target("url(\u{a0}a b)"), "\u{a0}a b");
    }

    /// The unspanned walk, which `opf.rs` uses to count references, sees a
    /// bad url too - otherwise its target is "declared but never referenced"
    /// (OPF-097), which epubcheck does not say.
    #[test]
    fn stylesheet_urls_includes_a_bad_url_target() {
        let css = "p { background: url(q'q.png) } @media print { a { b: url(x y.png) } }";
        assert_eq!(url_list(css), vec!["q'q.png", "x y.png"]);
        assert_eq!(url_list("p { background: url(ok.png) }"), vec!["ok.png"]);
    }

    /// NEXT-018: a url inside another function, which epubcheck never reads.
    /// Silent by default, its own finding under `--advisory` with the rule key
    /// the top-level url gets; still a use of the resource. Probe set
    /// 2026-10-21_css-reach, m4.
    #[test]
    fn a_url_inside_another_function_is_next_018() {
        let css = "p { background-image: image-set(url(missing.png) 1x) }";
        assert!(run(css, &empty_index()).is_empty());
        let report = run_advisory(css);
        let found: Vec<_> = report
            .messages
            .iter()
            .map(|m| (m.id, m.severity, m.rule))
            .collect();
        assert_eq!(
            found,
            [(NEXT_018, Severity::Usage, Some("css.url.missing_resource"))]
        );
        // The same url at the top level is the plain error, flag or not.
        let top = run_advisory("p { background-image: url(missing.png) }");
        assert_eq!(top.messages.len(), 1);
        assert_eq!(top.messages[0].id, RSC_007);
        // Still counted as used, for OPF-097.
        let urls = stylesheet_urls(&styloria::parse_stylesheet(css).0);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "missing.png");
        assert!(urls[0].nested);
    }

    /// The urls `stylesheet_urls` lists for `css`, flags dropped.
    fn url_list(css: &str) -> Vec<String> {
        stylesheet_urls(&styloria::parse_stylesheet(css).0)
            .into_iter()
            .map(|u| u.url)
            .collect()
    }

    #[test]
    fn a_quoted_css_snippet_is_folded_and_capped() {
        assert_eq!(quote_css("  "), None);
        assert_eq!(quote_css("h1,\n\t h2").as_deref(), Some("h1, h2"));
        let long = "a".repeat(80);
        let q = quote_css(&long).unwrap();
        assert_eq!(q.chars().count(), 61);
        assert!(q.ends_with('…'));
        // Cut on a char boundary, never mid-code-point.
        let wide = "é".repeat(80);
        assert_eq!(quote_css(&wide).unwrap().chars().count(), 61);
    }

    /// The two shapes that must stay silent, both of which are ordinary CSS
    /// that an earlier version of this walk reported.
    #[test]
    fn valid_css_inside_a_group_at_rule_stays_silent() {
        let idx = HashMap::new();
        let count = |css: &str| run(css, &idx).iter().filter(|i| **i == CSS_008).count();

        // An attribute selector: a `[]` block belongs to the *prelude*, and
        // reading it as a rule body reported CSS-008 on every `img[alt]`
        // inside an `@media` (Doitsu, MobileRead). `:nth-child(2n)` was
        // unaffected then - CSS Syntax makes `name(` a function token, not a
        // block - which is exactly what hid the shape, so it is pinned too.
        assert_eq!(count("@media print { img[alt] { color: red } }"), 0);
        assert_eq!(count("@media print { p:nth-child(2n) { color: red } }"), 0);
        // A nested at-rule's prelude is a condition, not a selector list.
        assert_eq!(
            count("@media print { @media (min-width: 0) { p { color: red } } }"),
            0
        );
        // A non-grouping at-rule nested inside a grouping one still holds
        // declarations, so its body must not be read as a rule list.
        assert_eq!(
            count("@media print { @font-face { font-family: x; src: url(x.ttf) } }"),
            0
        );
    }

    #[test]
    fn selector_class_names_basic() {
        let sheet = styloria::parse_stylesheet(".foo { color: red; }").0;
        assert_eq!(
            selector_class_names(&sheet),
            HashSet::from(["foo".to_string()])
        );
    }

    #[test]
    fn selector_class_names_comma_list() {
        let sheet = styloria::parse_stylesheet(".foo, .bar { color: red; }").0;
        assert_eq!(
            selector_class_names(&sheet),
            HashSet::from(["foo".to_string(), "bar".to_string()])
        );
    }

    #[test]
    fn selector_class_names_no_class() {
        let sheet = styloria::parse_stylesheet("body { color: red; } #id { color: blue; }").0;
        assert!(selector_class_names(&sheet).is_empty());
    }

    /// epubcheck's CSS-029 reach: a style rule inside `@media` counts, a
    /// pseudo-class argument does not (probe set 2026-10-21_css-reach).
    #[test]
    fn selector_class_names_reach_media_but_not_arguments() {
        let sheet =
            styloria::parse_stylesheet("@media screen { .in-media {} } p:not(.in-not) {}").0;
        assert_eq!(
            selector_class_names(&sheet),
            HashSet::from(["in-media".to_string()])
        );
    }

    /// `url( "a.png" )` is a url, as epubcheck's scanner and CSS Syntax both
    /// read it; `src("…")` and an `image-set()` string are not read, as in
    /// epubcheck (probe set 2026-10-21_css-reach).
    #[test]
    fn stylesheet_urls_reach() {
        let css = r#"a { b: url( "spaced.png" ) } c { d: src("src.png") }
                     e { f: image-set("set.png" 1x) }"#;
        assert_eq!(url_list(css), vec!["spaced.png"]);
    }

    #[test]
    fn selector_class_names_empty_stylesheet() {
        let sheet = styloria::parse_stylesheet("").0;
        assert!(selector_class_names(&sheet).is_empty());
    }

    fn run(css: &str, name_index: &HashMap<String, String>) -> Vec<&'static str> {
        let mut report = Report::new();
        check(
            css,
            "style.css",
            "OEBPS",
            name_index,
            &HashSet::new(),
            CssOrigin::File { bytes: None },
            false,
            true,
            &mut report,
        );
        report.messages.iter().map(|m| m.id).collect()
    }

    /// Run `check` and return the full report, for tests that assert on the
    /// `line:column` position now carried by every CSS finding.
    fn run_report(css: &str, name_index: &HashMap<String, String>) -> Report {
        let mut report = Report::new();
        check(
            css,
            "style.css",
            "OEBPS",
            name_index,
            &HashSet::new(),
            CssOrigin::File { bytes: None },
            false,
            // EPUB 3: the CSS-001 tests below assert the flagged-property
            // findings, which only exist at that version.
            true,
            &mut report,
        );
        report
    }

    fn pos_of(report: &Report, id: &str) -> Position {
        report
            .messages
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.position)
            .unwrap_or_else(|| panic!("no {id} finding with a position"))
    }

    fn run_bytes(bytes: &[u8]) -> Vec<&'static str> {
        let text = decode_bytes(bytes);
        let mut report = Report::new();
        check(
            &text,
            "style.css",
            "OEBPS",
            &HashMap::new(),
            &HashSet::new(),
            CssOrigin::File { bytes: Some(bytes) },
            false,
            true,
            &mut report,
        );
        report.messages.iter().map(|m| m.id).collect()
    }

    /// A class name epubcheck's scanner accepts and CSS does not (`.-`,
    /// `.-1`) is not CSS-008: the verdict follows epubcheck. With
    /// `--advisory` it is ADV-012, since browsers drop the rule. Anything
    /// else wrong with the same selector keeps its CSS-008.
    #[test]
    fn a_class_name_that_is_not_an_identifier_is_adv_012_not_css_008() {
        let idx = empty_index();
        for css in [
            "span.- { color: red }",
            "span.-1 { color: red }",
            ".- { color: red }",
            "span.- , p { color: red }",
            "h1.-.x { color: red }",
            "@media all { span.- { color: red } }",
        ] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
            let r = run_advisory(css);
            let adv: Vec<_> = r.messages.iter().filter(|m| m.id == ADV_012).collect();
            assert_eq!(adv.len(), 1, "{css}: {:?}", r.messages);
            assert_eq!(adv[0].severity, Severity::Usage);
            assert_eq!(adv[0].rule, Some("css.selector.class_not_identifier"));
            assert!(adv[0].params[0].starts_with(".-"), "{css}");
            assert!(!r.messages.iter().any(|m| m.id == CSS_008), "{css}");
        }
        // epubcheck rejects these too, so they stay CSS-008, with no advisory.
        for css in [
            "span.1 { color: red }",
            "span. { color: red }",
            "span.- > > p { color: red }",
        ] {
            assert!(run(css, &idx).contains(&CSS_008), "{css}");
            assert!(
                !run_advisory(css).messages.iter().any(|m| m.id == ADV_012),
                "{css}"
            );
        }
        // Identifiers are untouched either way.
        for css in [
            "span.-x { color: red }",
            "span.-- { color: red }",
            "p.x- { }",
        ] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
            assert!(
                !run_advisory(css).messages.iter().any(|m| m.id == ADV_012),
                "{css}"
            );
        }
    }

    /// ADV-017: a selector whose every token is a selector piece but whose
    /// shape breaks Selectors. epubcheck 5.4.0 accepts each of these (probed
    /// 2026-10-18), so they are never CSS-008.
    #[test]
    fn a_selector_shape_css_rejects_is_adv_017_not_css_008() {
        let idx = empty_index();
        for (css, token) in [
            ("#1a { color: red }", "#1a"),
            ("[x]p { color: red }", "p"),
            (".a* { color: red }", "*"),
            ("p::before.x { color: red }", ".x"),
            ("p::before span { color: red }", "::before"),
            (":nth-child(foo) { color: red }", "nth-child(foo)"),
            (":not(#1a) { color: red }", "#1a"),
            ("@media all { #1a { color: red } }", "#1a"),
        ] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
            let r = run_advisory(css);
            let adv: Vec<_> = r.messages.iter().filter(|m| m.id == ADV_017).collect();
            assert_eq!(adv.len(), 1, "{css}: {:?}", r.messages);
            assert_eq!(adv[0].severity, Severity::Usage);
            assert_eq!(
                adv[0].rule,
                Some("css.stylesheet.invalid_selector_structure")
            );
            assert_eq!(adv[0].params, vec![token.to_string()], "{css}");
            assert!(!r.messages.iter().any(|m| m.id == CSS_008), "{css}");
        }
        // A token error in another selector of the list is still CSS-008,
        // beside the advisory for the one that is only misshapen.
        let r = run_advisory("#1a, . b { color: red }");
        assert_eq!(r.messages.iter().filter(|m| m.id == CSS_008).count(), 1);
        assert_eq!(r.messages.iter().filter(|m| m.id == ADV_017).count(), 1);
        // An empty `:not()` has no token out of place, but epubcheck reports
        // it, so it is CSS-008 (styloria 0.13 classes it so); `:has()` is not.
        for css in [":not() { color: red }", ":not( ) { color: red }"] {
            assert!(run(css, &idx).contains(&CSS_008), "{css}");
            assert!(!run_advisory(css).messages.iter().any(|m| m.id == ADV_017));
        }
        assert!(!run(":has() { color: red }", &idx).contains(&CSS_008));
        // A forgiving list drops a bad argument and keeps the rule, so there
        // is nothing to say; valid selectors are untouched.
        for css in [
            ":is(#1a, p) { color: red }",
            ":where(p::before span) { color: red }",
            "p.x, [a=x i], :nth-child(2n of p) { color: red }",
        ] {
            let r = run_advisory(css);
            assert!(
                !r.messages
                    .iter()
                    .any(|m| m.id == ADV_017 || m.id == CSS_008),
                "{css}: {:?}",
                r.messages
            );
        }
        // Inside a nested rule the rule itself is the one finding.
        let r = run_advisory("p { #1a { color: red } }");
        assert!(
            !r.messages.iter().any(|m| m.id == ADV_017),
            "{:?}",
            r.messages
        );
    }

    /// styloria 0.16's five spec checks. epubcheck 5.4.0 is silent on every
    /// shape here (probed 2026-10-22), so each is an `ADV-*` under
    /// `--advisory` and never a CSS-008; vendor-prefixed names and the
    /// forgiving `:is()` stay silent.
    #[test]
    fn styloria_016_spec_checks_are_advisories_not_css_008() {
        let idx = empty_index();
        for (css, id, rule, param) in [
            (
                "a:hovr { color: red }",
                ADV_019,
                "css.selector.unknown_pseudo",
                ":hovr",
            ),
            (
                "p::befor { color: red }",
                ADV_019,
                "css.selector.unknown_pseudo",
                "::befor",
            ),
            (
                "p:not { color: red }",
                ADV_019,
                "css.selector.unknown_pseudo",
                ":not",
            ),
            (
                "@media all { a:hovr { color: red } }",
                ADV_019,
                "css.selector.unknown_pseudo",
                ":hovr",
            ),
            (
                "@medai screen { p { color: red } }",
                ADV_020,
                "css.at_rule.unknown",
                "medai",
            ),
            ("@foo;", ADV_020, "css.at_rule.unknown", "foo"),
            (
                "p { color: red }\n@import \"a.css\";",
                ADV_021,
                "css.at_rule.misplaced",
                "import",
            ),
            (
                "p { color: red }\n@namespace \"u\";",
                ADV_021,
                "css.at_rule.misplaced",
                "namespace",
            ),
            (
                "@media (min-widht: 1px) { p { color: red } }",
                ADV_022,
                "css.media_feature.unknown",
                "min-widht",
            ),
            (
                "@keyframes k { from { color: red !important } }",
                ADV_023,
                "css.keyframe.important_ignored",
                "color",
            ),
        ] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
            let r = run_advisory(css);
            let adv: Vec<_> = r.messages.iter().filter(|m| m.id == id).collect();
            assert_eq!(adv.len(), 1, "{css}: {:?}", r.messages);
            assert_eq!(adv[0].severity, Severity::Usage);
            assert_eq!(adv[0].rule, Some(rule), "{css}");
            assert_eq!(adv[0].params, vec![param.to_string()], "{css}");
            assert!(!r.messages.iter().any(|m| m.id == CSS_008), "{css}");
        }
        for css in [
            "a:hover, p::before, p:first-child, li::marker { color: red }",
            "a:-webkit-any-link, p::-epub-foo { color: red }",
            ":is(a:hovr, p) { color: red }",
            "@page :first { margin: 1em }",
            "@-moz-document url-prefix() { p { color: red } }",
            "@media amzn-kf8, (-webkit-min-device-pixel-ratio: 2) { p { color: red } }",
            "@media (min-width: 600px) and (orientation: portrait) { p { color: red } }",
            "@keyframes k { from { color: red } }",
            // In order: an unknown at-rule or a dropped style rule before
            // `@import` is not a rule it has to follow.
            "@foo;\n@import \"a.css\";",
            "a:hovr { color: red }\n@import \"a.css\";",
            "@charset \"utf-8\";\n@layer a;\n@import \"a.css\";\n@namespace \"u\";\np { color: red }",
        ] {
            let r = run_advisory(css);
            let unexpected: Vec<_> = r
                .messages
                .iter()
                .filter(|m| {
                    [ADV_021, ADV_022, ADV_023, CSS_008].contains(&m.id)
                        || (m.id == ADV_019 && !css.contains("hovr"))
                        || (m.id == ADV_020 && !css.contains("@foo"))
                })
                .collect();
            assert!(unexpected.is_empty(), "{css}: {unexpected:?}");
        }
    }

    /// `@charset`, `@import` and `@namespace` inside a block: epubcheck
    /// 5.4.0 reports each as CSS-008 in `@media` and `@supports`, and
    /// accepts every other at-rule there (probed 2026-10-22). With
    /// `--advisory` it is still the one finding, not an ADV-021 beside it.
    #[test]
    fn a_top_level_only_at_rule_inside_a_block_is_css_008() {
        let idx = empty_index();
        for css in [
            "@media screen { @import \"a.css\"; }",
            "@media screen { @namespace \"u\"; }",
            "@media screen { @charset \"utf-8\"; }",
            "@supports (display: grid) { @import \"a.css\"; }",
        ] {
            let r = run_report(css, &idx);
            let found: Vec<_> = r.messages.iter().filter(|m| m.id == CSS_008).collect();
            assert_eq!(found.len(), 1, "{css}: {:?}", r.messages);
            assert_eq!(found[0].rule, Some("css.stylesheet.misplaced_at_rule"));
            assert!(
                found[0].text.contains("only allowed at the top level"),
                "{css}"
            );
            let adv = run_advisory(css);
            assert_eq!(
                adv.messages.iter().filter(|m| m.id == CSS_008).count(),
                1,
                "{css}"
            );
            assert!(!adv.messages.iter().any(|m| m.id == ADV_021), "{css}");
        }
        for css in [
            "@media screen { @media print { p { color: red } } }",
            "@media screen { @supports (display: grid) { p { color: red } } }",
            "@media screen { @page { margin: 1em } }",
            "@media screen { @keyframes k { from { color: red } } }",
            "@media screen { @layer base { p { color: red } } }",
            "@media screen { @foo { p { color: red } } }",
        ] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
        }
    }

    /// The ADV-012 re-parse must not let a structure error, which epubcheck
    /// accepts too, turn a class name it accepts into a CSS-008.
    #[test]
    fn a_structure_error_beside_an_epubcheck_only_class_keeps_adv_012() {
        let idx = empty_index();
        for css in [".-1a#2b { color: red }", ".-1a::before span { color: red }"] {
            assert!(!run(css, &idx).contains(&CSS_008), "{css}");
            let r = run_advisory(css);
            assert!(r.messages.iter().any(|m| m.id == ADV_012), "{css}");
        }
    }

    #[test]
    fn relax_class_names_follows_epubcheck_scanner() {
        let found = |p: &str| super::relax_class_names(p).1;
        assert_eq!(found("span.-"), vec![(4, 6)]);
        assert_eq!(super::relax_class_names("h1.-1 a").0, "h1.xx a");
        assert!(found("span.x").is_empty());
        assert!(found("span.-x").is_empty());
        assert!(found(".1").is_empty()); // a number to epubcheck
        assert!(found("[title='.-']").is_empty());
        assert!(found("/* .- */ p").is_empty());
        assert!(found("p.-\\31").is_empty()); // escapes are left alone
    }

    fn empty_index() -> HashMap<String, String> {
        HashMap::new()
    }

    /// Run `check` with the advisory pass enabled and return the report.
    fn run_advisory(css: &str) -> Report {
        let mut report = Report::new();
        check(
            css,
            "style.css",
            "OEBPS",
            &empty_index(),
            &HashSet::new(),
            CssOrigin::File { bytes: None },
            true,
            true,
            &mut report,
        );
        report
    }

    #[test]
    fn advisory_off_by_default_emits_no_adv() {
        // The default path (advisory = false) must be byte-identical: an unknown
        // property draws nothing.
        let findings = run("p { font-eight: bold; }", &empty_index());
        assert!(!findings.iter().any(|id| id.starts_with("ADV-")));
    }

    #[test]
    fn advisory_flags_unknown_property_as_adv001() {
        let report = run_advisory("p { font-eight: bold; }");
        let m = report
            .messages
            .iter()
            .find(|m| m.id == ADV_001)
            .expect("ADV-001 emitted");
        assert_eq!(m.severity, Severity::Usage);
        assert_eq!(m.rule, Some("css.property.unknown"));
        assert_eq!(m.params, vec!["font-eight".to_string()]);
        assert!(m.position.is_some(), "carries a line:column");
    }

    #[test]
    fn advisory_flags_unknown_descriptor_as_adv002() {
        // `color` is a real property but not a @font-face descriptor.
        let report = run_advisory("@font-face { font-family: F; color: red }");
        let m = report
            .messages
            .iter()
            .find(|m| m.id == ADV_002)
            .expect("ADV-002 emitted");
        assert_eq!(m.severity, Severity::Usage);
        assert_eq!(m.rule, Some("css.descriptor.unknown"));
        assert_eq!(m.params, vec!["color".to_string(), "font-face".to_string()]);
    }

    /// styloria reports an invalid value, and no rule takes it yet: the
    /// output stays what it was until one is decided. Both halves are
    /// asserted, so a styloria that stopped reporting it would show here
    /// instead of passing silently.
    #[test]
    fn an_invalid_value_reports_nothing_yet() {
        let css = "p { font-style: bold }";
        let (sheet, _) = styloria::parse_stylesheet(css);
        assert!(
            validate_parsed_stylesheet(&sheet)
                .iter()
                .any(|d| d.kind == DiagnosticKind::InvalidValue),
            "styloria calls `font-style: bold` invalid"
        );
        let report = run_advisory(css);
        assert!(report.messages.is_empty(), "{:?}", report.messages);
        let mut report = Report::new();
        check_style_attribute("font-style: bold", "doc.xhtml", true, true, &mut report);
        assert!(report.messages.is_empty(), "{:?}", report.messages);

        // A descriptor value too (styloria 0.15): `font-family` in
        // `@font-face` takes one family, so a fallback list is invalid there.
        // It changes nothing in the report.
        let fallback = r#"@font-face { font-family: "Arial", sans-serif; src: url(f.woff) }"#;
        let (sheet, _) = styloria::parse_stylesheet(fallback);
        assert!(
            validate_parsed_stylesheet(&sheet)
                .iter()
                .any(|d| d.kind == DiagnosticKind::InvalidValue && d.name == "font-family"),
            "styloria calls a font-family list in @font-face invalid"
        );
        let ids = |css| -> Vec<_> { run_advisory(css).messages.iter().map(|m| m.id).collect() };
        assert_eq!(
            ids(fallback),
            ids(r#"@font-face { font-family: "Arial"; src: url(f.woff) }"#)
        );
    }

    #[test]
    fn advisory_is_silent_on_valid_and_exempt_css() {
        // Known property, vendor-prefixed, and custom property: all clean.
        let report = run_advisory("p { color: red; -webkit-hyphens: auto; --x: 1 }");
        assert!(!report.messages.iter().any(|m| m.id.starts_with("ADV-")));
    }

    #[test]
    fn advisory_checks_style_attributes() {
        let mut report = Report::new();
        check_style_attribute("font-eight: bold", "doc.xhtml", true, true, &mut report);
        assert!(report.messages.iter().any(|m| m.id == ADV_001));
        // ...and off by default:
        let mut off = Report::new();
        check_style_attribute("font-eight: bold", "doc.xhtml", false, true, &mut off);
        assert!(!off.messages.iter().any(|m| m.id.starts_with("ADV-")));
    }

    /// An attribute selector inside a grouping at-rule is a *prelude*, not a
    /// rule body. Walking into its `[]` block and reading the contents as
    /// declarations reported CSS-008 on ordinary CSS — `img[alt]` inside an
    /// `@media` — which is what Doitsu hit on MobileRead (his case was the
    /// namespaced `img[epub|type~="…"]`, but the namespace was incidental:
    /// every attribute selector in that position was affected).
    ///
    /// `()` never had the bug, because CSS Syntax makes `name(` a function
    /// token rather than a simple block. That asymmetry is why a
    /// `:nth-child(2n)` in the same position looked fine and disguised how
    /// wide the defect was — so it is asserted here too.
    #[test]
    fn a_selector_prelude_inside_an_at_rule_is_not_a_declaration_list() {
        for css in [
            r#"@media all { img[alt] { color: red } }"#,
            r#"@media print { a[href^="http"] { color: red } }"#,
            r#"@media all { li:nth-child(2n) { color: red } }"#,
            r#"@media all and (prefers-color-scheme: dark) {
                 img[epub|type~="se:image.color-depth.black-on-transparent"] {
                   filter: invert(100%);
                 }
               }"#,
            r#"@supports (display: grid) { .a[data-x="1"] { color: red } }"#,
            r#"@media all { @media print { p[lang] { color: red } } }"#,
        ] {
            assert_eq!(run(css, &empty_index()), Vec::<&str>::new(), "{css}");
        }
        // The check it was doing correctly is untouched: a real malformed
        // declaration inside a grouping at-rule is still reported.
        assert!(run("@media all { p { span.bold: bold } }", &empty_index()).contains(&CSS_008));
    }

    #[test]
    fn direction_property_flagged() {
        let findings = run("body { direction: rtl; }", &empty_index());
        assert!(findings.contains(&CSS_001));
    }

    #[test]
    fn unicode_bidi_property_flagged() {
        let findings = run("body { unicode-bidi: bidi-override; }", &empty_index());
        assert!(findings.contains(&CSS_001));
    }

    /// CSS-001 is EPUB 3 only: epubcheck guards it with
    /// `if (version == EPUBVersion.VERSION_3)`. We had no gate, which
    /// invented an error on a real EPUB 2 book carrying
    /// `<h1 style="direction: inherit">`. Neither the corpus nor the shelf
    /// protects this - the shelf found it once, and only because the
    /// `compare` harness had epubcheck's answer to diff against - so the
    /// EPUB 2 half is asserted here.
    #[test]
    fn css001_is_epub3_only() {
        let epub2 = |css: &str| {
            let mut report = Report::new();
            check(
                css,
                "style.css",
                "OEBPS",
                &empty_index(),
                &HashSet::new(),
                CssOrigin::File { bytes: None },
                false,
                false,
                &mut report,
            );
            report.messages.iter().filter(|m| m.id == CSS_001).count()
        };
        assert_eq!(epub2("body { direction: rtl; }"), 0);
        assert_eq!(epub2("body { unicode-bidi: bidi-override; }"), 0);
        // The EPUB 3 side still fires, and CSS-006 - the unguarded rule
        // sitting in the same `else if` chain - is unaffected at EPUB 2.
        assert!(run("body { direction: rtl; }", &empty_index()).contains(&CSS_001));
        let mut report = Report::new();
        check(
            "p { position: fixed; }",
            "style.css",
            "OEBPS",
            &empty_index(),
            &HashSet::new(),
            CssOrigin::File { bytes: None },
            false,
            false,
            &mut report,
        );
        assert!(report.messages.iter().any(|m| m.id == CSS_006));
    }

    /// The same gate has to hold for a `style` attribute, which reaches the
    /// non-spanned shape check by a different path.
    #[test]
    fn css001_is_epub3_only_in_a_style_attribute() {
        let count = |is_epub3: bool| {
            let mut report = Report::new();
            check_style_attribute("direction: rtl", "doc.xhtml", false, is_epub3, &mut report);
            report.messages.iter().filter(|m| m.id == CSS_001).count()
        };
        assert_eq!(count(false), 0);
        assert_eq!(count(true), 1);
    }

    #[test]
    fn unterminated_block_and_bad_token_are_css008() {
        // styloria 0.4 surfaces both; each maps to CSS-008.
        assert!(run("a { color: red", &empty_index()).contains(&CSS_008)); // unterminated block
        assert!(run("a { content: \"oops\n }", &empty_index()).contains(&CSS_008)); // bad string
    }

    #[test]
    fn position_fixed_flagged_css006() {
        // Flagged (case-insensitive on both name and value), matching
        // epubcheck's CSS-006.
        assert!(run("div { position: fixed; }", &empty_index()).contains(&CSS_006));
        assert!(run("div { POSITION: Fixed }", &empty_index()).contains(&CSS_006));
        // Any other position value is fine.
        assert!(!run("div { position: absolute; }", &empty_index()).contains(&CSS_006));
        assert!(!run("div { position: relative; }", &empty_index()).contains(&CSS_006));
    }

    #[test]
    fn utf16_stylesheet_warns() {
        let css = "body { color: red; }";
        let mut be_bytes = vec![0xFE, 0xFF];
        for c in css.encode_utf16() {
            be_bytes.extend_from_slice(&c.to_be_bytes());
        }
        let findings = run_bytes(&be_bytes);
        assert!(findings.contains(&CSS_003));
    }

    #[test]
    fn utf8_stylesheet_no_encoding_warning() {
        let findings = run_bytes(b"body { color: red; }");
        assert!(!findings.contains(&CSS_003));
    }

    /// A BOM outranks `@charset`, and a declaration that is not byte-exact is
    /// not a declaration at all.
    ///
    /// Both from CSS Syntax 3 §3.1, and both were false positives until
    /// 2026-08-21. **They were found by running epubcheck's 24 bare CSS test
    /// files** — fixtures no instrument here had ever reached, because the
    /// shelf has no such stylesheet and the corpus harness only walks CSS that
    /// lives inside a book. epubcheck is silent on both and we were not.
    ///
    /// The controls matter more than the cases: a real non-UTF-8 declaration
    /// must still error, or this "fix" is just the check switched off.
    #[test]
    fn a_bom_outranks_charset_and_a_loose_charset_is_not_one() {
        // The two fixtures, in the bytes they actually carry.
        let bom_then_charset = {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(b"@charset \"iso-8859-15\";\n.a { color: red }");
            v
        };
        assert!(
            !run_bytes(&bom_then_charset).contains(&CSS_004),
            "a UTF-8 BOM settles the encoding; the declaration after it is decoration"
        );
        assert!(
            !run_bytes(b"@charset '' ;\ndiv { color: green }").contains(&CSS_004),
            "single quotes and a space before the semicolon: not an encoding declaration"
        );
        // Controls — the check must still bite.
        assert!(
            run_bytes(b"@charset \"iso-8859-15\";\n.a { color: red }").contains(&CSS_004),
            "byte-exact and not utf-8/16: still an error"
        );
        assert!(
            !run_bytes(b"@charset \"utf-8\";\n.a { color: red }").contains(&CSS_004),
            "byte-exact and utf-8: fine"
        );
        // The byte-exact matcher itself, at its edges.
        assert_eq!(
            styloria::charset_rule(b"@charset \"utf-8\";"),
            Some("utf-8")
        );
        assert_eq!(
            styloria::charset_rule(b"@charset  \"utf-8\";"),
            None,
            "two spaces"
        );
        assert_eq!(
            styloria::charset_rule(b"@charset 'utf-8';"),
            None,
            "single quotes"
        );
        assert_eq!(
            styloria::charset_rule(b"@charset \"utf-8\" ;"),
            None,
            "space before ;"
        );
        assert_eq!(
            styloria::charset_rule(b"\n@charset \"utf-8\";"),
            None,
            "not at the start"
        );
    }

    /// A UTF-32BE byte order mark: CSS-004 once, and the text decoded, as
    /// epubcheck does. Read as the spec's UTF-8 it would be NULs and a
    /// CSS-008 per rule (probe `charset-upstream/e-bom-utf32be`).
    #[test]
    fn a_utf32be_stylesheet_is_css_004_and_decoded() {
        let css = "a { color: red }";
        let mut bytes = vec![0, 0, 0xFE, 0xFF];
        for c in css.chars() {
            bytes.extend_from_slice(&u32::from(c).to_be_bytes());
        }
        assert_eq!(decode_bytes(&bytes), css);
        assert_eq!(run_bytes(&bytes), vec![CSS_004]);
    }

    #[test]
    fn non_utf8_16_charset_errors() {
        let findings = run_bytes(b"@charset \"ISO-8859-1\";\nbody { color: red; }");
        assert!(findings.contains(&CSS_004));
    }

    /// `UTF-16BE`/`UTF-16LE` *are* UTF-16. Matching the name literally
    /// reported a stylesheet declaring `UTF-16BE` as if it had declared
    /// Latin-1 — on epubcheck's own fixture, which expects the UTF-16
    /// warning and nothing else.
    #[test]
    fn utf16_byte_order_variants_are_utf16() {
        for cs in ["UTF-16", "UTF-16BE", "utf-16le", "UTF-8", " utf-8 "] {
            let css = format!("@charset \"{cs}\";\nbody {{ color: red; }}");
            assert!(
                !run_bytes(css.as_bytes()).contains(&CSS_004),
                "'{cs}' is a permitted encoding"
            );
        }
        for cs in ["ISO-8859-1", "windows-1252", "utf-32", "utf-16x"] {
            let css = format!("@charset \"{cs}\";\nbody {{ color: red; }}");
            assert!(
                run_bytes(css.as_bytes()).contains(&CSS_004),
                "'{cs}' is not utf-8 or utf-16"
            );
        }
    }

    #[test]
    fn utf8_charset_is_fine() {
        let findings = run_bytes(b"@charset \"utf-8\";\nbody { color: red; }");
        assert!(!findings.contains(&CSS_004));
    }

    #[test]
    fn decode_bytes_handles_utf16_bom() {
        let css = "body { color: red; }";
        let mut be_bytes = vec![0xFE, 0xFF];
        for c in css.encode_utf16() {
            be_bytes.extend_from_slice(&c.to_be_bytes());
        }
        assert_eq!(decode_bytes(&be_bytes), css);

        let mut le_bytes = vec![0xFF, 0xFE];
        for c in css.encode_utf16() {
            le_bytes.extend_from_slice(&c.to_le_bytes());
        }
        assert_eq!(decode_bytes(&le_bytes), css);

        // plain UTF-8 (no BOM) still falls back correctly
        assert_eq!(decode_bytes(css.as_bytes()), css);
    }

    #[test]
    fn clean_stylesheet_no_findings() {
        let idx = empty_index();
        let css = "body { color: red; } .foo { margin: 0; }";
        assert!(run(css, &idx).is_empty());
    }

    /// A clean `@font-face` draws exactly one thing: the informational
    /// CSS-028 noting the declaration is there. It is not a defect - real
    /// epubcheck reports the same usage note for every `@font-face` - so
    /// nothing else may fire alongside it.
    #[test]
    fn clean_font_face_draws_only_the_css_028_usage_note() {
        let mut idx = empty_index();
        idx.insert("OEBPS/font.woff".to_string(), "OEBPS/font.woff".to_string());
        let css = "@font-face { font-family: X; src: url(font.woff); } body { color: red; }";
        assert_eq!(run(css, &idx), vec![CSS_028]);
    }

    /// An **empty** `@font-face` draws CSS-019 and no CSS-028.
    ///
    /// epubcheck's own `content-css-font-face-empty-error` fixture, whose
    /// entire stylesheet is `@font-face {\n}` — found by running `compare`
    /// over its test corpus.
    ///
    /// This is the one place our granularity difference changes sign, which
    /// is why it survived. epubcheck reports CSS-028 from its *declaration*
    /// handler, so it gives one per declaration where we give one per rule,
    /// and `COVERAGE.md` records that our count is always the lower of the
    /// two. With no declarations its count is zero and ours was one: not a
    /// granularity difference at all, but a note about a font where there is
    /// no font.
    ///
    /// The second assertion is the guard. Suppressing CSS-028 whenever
    /// CSS-019 fires, or simply moving the call, would satisfy the first and
    /// silence every real `@font-face` in the corpus.
    #[test]
    fn an_empty_font_face_draws_no_css_028() {
        let idx = empty_index();
        assert_eq!(run("@font-face {\n}", &idx), vec![CSS_019]);
        assert_eq!(
            run("@font-face {\n}\n@font-face { font-family: A; }", &idx),
            vec![CSS_019, CSS_028],
            "the empty rule is silent and its non-empty neighbour is not"
        );
    }

    /// One note per declaration, not one per stylesheet.
    #[test]
    fn css_028_fires_once_per_font_face() {
        let mut idx = empty_index();
        idx.insert("OEBPS/a.woff".to_string(), "OEBPS/a.woff".to_string());
        idx.insert("OEBPS/b.woff".to_string(), "OEBPS/b.woff".to_string());
        let css = "@font-face { font-family: A; src: url(a.woff); }\n\
                   @font-face { font-family: B; src: url(b.woff); }";
        assert_eq!(run(css, &idx), vec![CSS_028, CSS_028]);
    }

    #[test]
    fn malformed_declaration_shape() {
        // a stray '.' breaks the property name into two tokens with no
        // colon following the first — not a BadString/BadUrl token, but
        // still a real syntax error.
        let findings = run("body { span.bold: bold; }", &empty_index());
        assert!(findings.contains(&CSS_008));
    }

    #[test]
    fn unclosed_rule_swallows_sibling_rule() {
        let css = "body {\n  color: black;\n\np {\n  font-size: 1em;\n}\n";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&CSS_008));
    }

    #[test]
    fn media_query_nested_rules_are_not_syntax_errors() {
        // Issue #5: a Vellum-style `@media` block holds nested rules, whose
        // selectors must not be mis-read as malformed declarations.
        let css = "@media screen and (max-width: 420px) {\n\
                   \x20 div.list-text-feature { padding-right: 0px; }\n\
                   \x20 blockquote.verse { padding-left: 1.5em; }\n\
                   }";
        assert!(run(css, &empty_index()).is_empty());
    }

    /// A keyframe block holds rules, and its preludes are keyframe
    /// selectors rather than selectors. We read it as a declaration list
    /// until styloria 0.11, so `0% { opacity: 0 }` came back as one
    /// malformed declaration: CSS-008 on valid CSS, on a construct in every
    /// animated fixed-layout book. epubcheck reports nothing for any of
    /// these — each was built as a book and run through it.
    ///
    /// Not a shelf finding, and it could not have been one: no book of the
    /// 346 contains `@keyframes` at all.
    /// #81: a broken selector *list* is one CSS-008, not one per selector.
    ///
    /// styloria reports per comma-separated selector (its #3, deliberate and
    /// right for a CSS library); epubcheck's unit is the whole prelude. The
    /// adaptation belongs here rather than there — parity with epubcheck is
    /// the consumer's concern. Counts measured one book per shape against
    /// 5.3.0; a real shelf book went from 22 findings to epubcheck's 12.
    #[test]
    fn a_broken_selector_list_is_one_finding() {
        let n = |css: &str| run(css, &empty_index()).len();
        assert_eq!(n(". a { color: red }"), 1);
        assert_eq!(n(". a, . b { color: red }"), 1);
        assert_eq!(n(". a, . b, . c { color: red }"), 1);
        // Separate rules stay separate — the collapse is per rule, and this
        // is the assertion that keeps it from swallowing a whole stylesheet.
        assert_eq!(n(". a { color: red }\n. b { color: blue }"), 2);
        // The same inside a grouping at-rule, which reaches the other
        // emission site: a half-applied fix is this project's recurring
        // shape.
        assert_eq!(n("@media print { . a, . b { color: red } }"), 1);
        assert_eq!(
            n("@media print { . a, . b { color: red } . c, . d { color: blue } }"),
            2
        );
        assert!(run("p, div { color: red }", &empty_index()).is_empty());
    }

    #[test]
    fn a_keyframes_block_is_not_a_syntax_error() {
        for css in [
            "@keyframes spin { 0% { opacity: 0 } 100% { opacity: 1 } }",
            "@keyframes spin { from { opacity: 0 } to { opacity: 1 } }",
            "@-webkit-keyframes spin { 50% { opacity: .5 } }",
            "@-moz-keyframes spin { 50% { opacity: .5 } }",
            "@keyframes spin { bogus-sel { opacity: 0 } }",
            "@media print { @keyframes spin { 0% { opacity: 0 } } }",
            "@starting-style { .a { opacity: 0 } }",
        ] {
            assert!(run(css, &empty_index()).is_empty(), "{css}");
        }
    }

    /// The half that must not go with it: a broken declaration *inside* a
    /// keyframe is still reported, and epubcheck reports it too. Without
    /// this, "read the block as rules" could be satisfied by not looking
    /// inside at all — and the fix would have traded a false positive for a
    /// false negative with nothing to notice.
    #[test]
    fn a_bad_declaration_inside_a_keyframe_is_still_reported() {
        assert_eq!(
            run("@keyframes spin { 0% { color red } }", &empty_index()),
            vec![CSS_008]
        );
        assert_eq!(
            run("@starting-style { .a { color red } }", &empty_index()),
            vec![CSS_008]
        );
    }

    /// CSS Nesting inside a style rule. We reported the nested *style rule*
    /// form and stayed silent on the nested *at-rule* form, which was not a
    /// decision — the declaration walk skipped at-rule chunks, because an
    /// at-rule's block may legitimately hold at-rules. A style rule's may
    /// not.
    ///
    /// Reporting is the right side here, against the intuition that nesting
    /// is modern-but-valid CSS and so should be tolerated the way modern
    /// selectors are. EPUB 3.3 supports "CSS as defined by the CSS Working
    /// Group Snapshot"; in the 2026 Snapshot nesting sits in §2.4, *modules
    /// with rough interoperability*, explicitly outside the official
    /// definition of CSS. epubcheck reports all of these (three findings to
    /// our one, our usual lower multiplicity).
    #[test]
    fn a_nested_rule_in_a_style_rule_is_reported() {
        for css in [
            "a { color: red; & b { color: blue } }",
            ".a { color: red; .b { color: blue } }",
            "a { color: red; &:hover { color: blue } }",
            ".a { color: red; @media print { color: blue } }",
            ".a { color: red; @nest & b { color: blue } }",
        ] {
            assert_eq!(run(css, &empty_index()), vec![CSS_008], "{css}");
        }
    }

    /// The other half, and the reason the at-rule case had been skipped in
    /// the first place: an at-rule *inside an at-rule's block* is ordinary
    /// CSS and must stay silent. `@page`'s margin at-rules are the shape
    /// epubcheck's older parser rejects and we deliberately do not.
    #[test]
    fn an_at_rule_inside_an_at_rule_block_stays_silent() {
        for css in [
            "@page { margin: 1em; @top-center { content: \"x\" } }",
            "@font-feature-values Fnt { @styleset { nice: 1; } }",
        ] {
            assert!(run(css, &empty_index()).is_empty(), "{css}");
        }
    }

    /// An at-rule nobody has a table entry for holds whatever is written in
    /// it, and a nested rule inside it is not blamed. This is the direction
    /// the unknown case has to fail in: CSS keeps gaining at-rules, so any
    /// table is permanently behind the language, and a validator must not
    /// turn that into an error on a valid stylesheet. A malformed
    /// declaration is still caught — epubcheck agrees on both halves.
    #[test]
    fn an_unknown_at_rule_holding_rules_is_not_a_syntax_error() {
        assert!(run("@future { p { color: red } }", &empty_index()).is_empty());
        assert_eq!(run("@future { color red }", &empty_index()), vec![CSS_008]);
    }

    /// **A deliberate divergence from epubcheck, in the spec's favour (owner's
    /// decision, 2026-10-07).** CSS Syntax §5.5.6: a top-level `{}`-block is
    /// allowed as the *whole* value of a declaration, so `p { color: { red }; }`
    /// is a well-formed declaration. Whether `color` takes such a value is a
    /// property-grammar question, not a syntax error. epubcheck 5.4.0 reports
    /// CSS-008 twice and fails the book; we report nothing, measured on the
    /// same text. The mixed shape is a syntax error in both, and stays one
    /// here: a block followed by more (`color: red {}`, `font: {} bar`) makes
    /// the item a rule. Do not "fix" the first assertion to match epubcheck
    /// without the owner; it was asked and answered.
    #[test]
    fn a_block_as_the_whole_value_is_a_declaration_as_the_spec_says() {
        for css in [
            "p { color: { red }; }",
            "p { color: {} }",
            "p { --x: { a: b }; }",
        ] {
            assert!(run(css, &empty_index()).is_empty(), "{css}");
        }
        for css in ["p { color: red {} }", "p { font: {} bar; color: red }"] {
            assert!(run(css, &empty_index()).contains(&CSS_008), "{css}");
        }
    }

    /// What styloria 0.12 (CSS Syntax CRD 2026-10-01) changed, each against
    /// epubcheck 5.4.0 on the same text.
    #[test]
    fn what_the_2026_block_parse_changed() {
        let report = |css: &str| run_report(css, &empty_index());
        let css008 = |css: &str| -> Vec<(String, &'static str)> {
            report(css)
                .messages
                .into_iter()
                .filter(|m| m.id == CSS_008)
                .map(|m| (m.text, m.rule.unwrap_or_default()))
                .collect()
        };
        // A declaration directly in `@media`: silent in epubcheck, and was
        // CSS-008 here (0.11 read it as a rule that never got its block).
        assert!(css008("@media print { color: red }").is_empty());
        // A rule inside `@font-face`, and a value with a `{}` in it: both
        // CSS-008 in epubcheck, and both missed here before.
        assert_eq!(
            css008("@font-face { font-family: f; p { color: red } }"),
            vec![(
                "CSS syntax error: rule 'p' inside a style rule".to_string(),
                "css.declaration.nested_rule"
            )]
        );
        assert_eq!(css008("p { color: red {} }").len(), 1);
        // A nested rule is one finding however broken its own selector and
        // block are; what styloria finds inside it is quieted.
        assert_eq!(
            css008("p { a. q { color red; . r { } } }"),
            vec![(
                "CSS syntax error: rule 'a. q' inside a style rule".to_string(),
                "css.declaration.nested_rule"
            )]
        );
        // The declaration after a nested rule is read now (0.11 swallowed it
        // up to the `;`), so the EPUB rules reach it.
        assert!(
            report("p { a { } direction: rtl; }")
                .messages
                .iter()
                .any(|m| m.id == CSS_001)
        );
        // A block a broken string left open inside `@media` is quieted like
        // one at the top level: the string and the two `{` are the findings.
        let kinds: Vec<_> = css008("@media print { p { content: \"a\n\" } . s { } }\n. t { }")
            .into_iter()
            .map(|(_, slug)| slug)
            .collect();
        assert_eq!(
            kinds,
            vec![
                "css.stylesheet.unterminated",
                "css.stylesheet.unterminated",
                "css.stylesheet.bad_token",
                "css.stylesheet.bad_token"
            ]
        );
    }

    #[test]
    fn nested_media_queries_are_not_syntax_errors() {
        // A grouping at-rule nested inside another must recurse, not flag.
        let css = "@supports (display: grid) {\n\
                   \x20 @media screen {\n\
                   \x20   p.body { color: red; }\n\
                   \x20 }\n\
                   }";
        assert!(run(css, &empty_index()).is_empty());
    }

    #[test]
    fn empty_font_face_block() {
        let findings = run("@font-face {}", &empty_index());
        assert!(findings.contains(&CSS_019));
    }

    #[test]
    fn empty_font_face_src_url() {
        let css = "@font-face { font-family: X; src: url(''); }";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&CSS_002));
    }

    #[test]
    fn bad_string_token_reported() {
        // an unterminated string is a BadString token at the tokenizer level
        let css = "body { content: \"unterminated\n }";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&CSS_008));
    }

    /// A string broken by a line break is read as CSS Syntax reads it, and
    /// that is three findings where epubcheck reports one. Kept on purpose
    /// (owner's decision, 2026-10-02): the spec ends the string at the line
    /// break, so the quote meant to close it opens a second string, which
    /// swallows the `}`, and that block really is never closed. Each finding
    /// is something a browser does. epubcheck instead skips from the line
    /// break to the next `;`, `{` or `}` (`CssScanner._string`, then
    /// `reader.forward(TERMINATOR)`), a recovery no specification describes.
    /// The verdict is the same either way.
    #[test]
    fn a_broken_string_is_read_as_the_spec_reads_it() {
        let report = run_report("p { content: \"abc\n\" }\n", &empty_index());
        let texts: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.id == CSS_008)
            .map(|m| m.text.as_str())
            .collect();
        assert_eq!(
            texts,
            vec![
                "CSS syntax error: '{' is never closed",
                "CSS syntax error: string '\"abc' is broken by an unescaped line break",
                "CSS syntax error: string '\" }' is broken by an unescaped line break",
            ]
        );
    }

    #[test]
    fn missing_import_target_undeclared_and_absent() {
        // Real corpus finding: an undeclared *and* absent target is
        // RSC-007, not RSC-001 (RSC-001 is only for a manifest-declared
        // resource whose file is missing - see the tests below).
        let findings = run("@import \"missing.css\";", &empty_index());
        assert!(findings.contains(&RSC_007));
    }

    /// An unclosed block is **one** finding, however much it swallows.
    ///
    /// A `{` with no `}` absorbs everything after it — the next rule's
    /// selector and braces included — so the "declarations" inside it are not
    /// declarations, and every shape complaint they produce is a consequence
    /// of the one defect styloria has already reported as
    /// `UnterminatedBlock`. On epubcheck's `content-css-syntax-error`, which
    /// has two unclosed blocks, styloria reports two errors and so does
    /// epubcheck; the third was ours.
    ///
    /// **The controls matter more than the first case here.** Suppressing too
    /// eagerly would take a genuine malformed declaration with it, so a bad
    /// shape in a *closed* block, and the plain unterminated block on its own,
    /// are both asserted.
    #[test]
    fn an_unclosed_block_does_not_multiply_its_findings() {
        let idx = empty_index();
        let count = |css: &str| run(css, &idx).iter().filter(|i| **i == CSS_008).count();
        assert_eq!(
            count("a {\n  color: red;\n\nb { color: blue }\n"),
            1,
            "the unclosed block is the defect; the rule it ate is not a second one"
        );
        assert_eq!(
            count("a { color: red"),
            1,
            "control: an unclosed block with nothing after it still reports"
        );
        assert_eq!(
            count("a { span.bold: bold }\n"),
            1,
            "control: a malformed declaration in a closed block is still reported"
        );
        assert_eq!(
            count("a { color: red }\nb { color: blue }\n"),
            0,
            "control: valid CSS stays silent"
        );
    }

    #[test]
    fn missing_background_url_nested_in_media() {
        let css = "@media screen { body { background: url(missing.png); } }";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&RSC_007));
    }

    /// A declared-but-absent `@import` target is reported by the **manifest**
    /// walk, not here.
    ///
    /// This asserted `vec![RSC_001]` from this module, which is what made one
    /// missing file two findings: epubcheck's RSC-001 is per publication
    /// *resource*, so it says it once. The assertion is now that this walk is
    /// silent — and the two arms either side of it, which are about the
    /// *reference* rather than the resource, still speak.
    #[test]
    fn import_target_declared_but_file_missing_is_left_to_the_manifest() {
        let mut manifest_paths = HashSet::new();
        manifest_paths.insert("OEBPS/missing.css".to_string());
        let mut report = Report::new();
        check(
            "@import \"missing.css\";",
            "style.css",
            "OEBPS",
            &empty_index(),
            &manifest_paths,
            CssOrigin::File { bytes: None },
            false,
            true,
            &mut report,
        );
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert!(
            ids.is_empty(),
            "the manifest item's own RSC-001 is the whole answer: {ids:?}"
        );
    }

    #[test]
    fn import_target_undeclared_but_file_present_is_rsc008() {
        let mut name_index = HashMap::new();
        name_index.insert(
            "OEBPS/present.css".to_string(),
            "OEBPS/present.css".to_string(),
        );
        let mut report = Report::new();
        check(
            "@import \"present.css\";",
            "style.css",
            "OEBPS",
            &name_index,
            &HashSet::new(),
            CssOrigin::File { bytes: None },
            false,
            true,
            &mut report,
        );
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![RSC_008]);
    }

    #[test]
    fn external_urls_are_not_checked() {
        let css = "body { background: url(https://example.com/x.png); }";
        assert!(run(css, &empty_index()).is_empty());
    }

    #[test]
    fn css001_carries_property_position() {
        // The `direction` property is on line 2, starting at column 3.
        let css = "body {\n  direction: rtl;\n}";
        let pos = pos_of(&run_report(css, &empty_index()), CSS_001);
        assert_eq!((pos.line, pos.column), (2, 3));
    }

    #[test]
    fn css008_malformed_declaration_carries_position() {
        // The stray-dot declaration `span.bold: bold;` is on line 2, col 3.
        let css = "body {\n  span.bold: bold;\n}";
        let pos = pos_of(&run_report(css, &empty_index()), CSS_008);
        assert_eq!((pos.line, pos.column), (2, 3));
    }

    #[test]
    fn css008_bad_token_carries_position() {
        // An unterminated string is a BadString token; it starts at the
        // `content` value on line 2.
        let css = "body {\n  content: \"unterminated\n }";
        let pos = pos_of(&run_report(css, &empty_index()), CSS_008);
        assert_eq!(pos.line, 2);
    }

    #[test]
    fn rsc_url_finding_carries_position() {
        // A missing background image nested in a media query - the RSC-007
        // should point at the `url(...)` on line 2.
        let css = "@media screen {\n  body { background: url(missing.png); }\n}";
        let pos = pos_of(&run_report(css, &empty_index()), RSC_007);
        assert_eq!(pos.line, 2);
    }

    /// RSC-026: a url() that resolves above the container root. epubcheck
    /// applies this in `URLChecker`, its single URL-resolution point, so it
    /// lands on every url it resolves; we had it on manifest hrefs only.
    ///
    /// The shape that found it is a stylesheet at the container *root*
    /// asking for `url(../Fonts/x.ttf)` — one shelf book, eight of them.
    /// Additive with the RSC-001/007/008 split: epubcheck reports both.
    #[test]
    fn a_url_escaping_the_container_root_is_rsc_026() {
        let leaks = |base: &str, css: &str| {
            let mut report = Report::new();
            check(
                css,
                "s.css",
                base,
                &empty_index(),
                &HashSet::new(),
                CssOrigin::File { bytes: None },
                false,
                true,
                &mut report,
            );
            report.messages.iter().filter(|m| m.id == RSC_026).count()
        };
        // From the container root, `..` escapes immediately.
        assert_eq!(leaks("", "p { background: url(../x.png); }"), 1);
        // A path-absolute url is the other half of the same rule.
        assert_eq!(leaks("", "p { background: url(/x.png); }"), 1);
        // One directory deep, a single `..` lands back on the root and is
        // fine - the check is about escaping, not about `..` appearing.
        assert_eq!(leaks("OEBPS", "p { background: url(../x.png); }"), 0);
        assert_eq!(leaks("OEBPS", "p { background: url(../../x.png); }"), 1);
        // Ordinary relative and remote urls say nothing.
        assert_eq!(leaks("OEBPS", "p { background: url(img/x.png); }"), 0);
        assert_eq!(
            leaks("", "p { background: url(https://example.org/x.png); }"),
            0
        );
    }

    #[test]
    fn font_face_position_points_at_at_rule() {
        // CSS-019 has no token inside an empty block, so it anchors at the
        // `@font-face` keyword on line 2.
        let css = "body { color: red; }\n@font-face {}";
        let pos = pos_of(&run_report(css, &empty_index()), CSS_019);
        assert_eq!(pos.line, 2);
    }
}
