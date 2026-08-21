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
//!   nested rules inside e.g. `@media` blocks for free, since styloria's
//!   core grammar represents a nested rule's `{ ... }` as an ordinary
//!   `ComponentValue::Block` that the walk below already recurses into.
//!
//! The finding-emitting pass (`check`) walks styloria's **span-carrying**
//! parse tree (`styloria::spanned`), so every CSS finding now reports the
//! exact `line:column` of the offending token — the last finding family in
//! epubveri that used to carry only a file path (issue #1; Kevin Hendricks /
//! Sigil asked for CSS positions specifically). The position-less pub
//! helpers below (`stylesheet_urls`, `import_targets`, `selector_class_names`,
//! `font_face_src_urls_spanned`) are still consumed by `opf.rs` off the plain
//! `styloria::Stylesheet`, so they keep the plain parser — they don't need
//! positions.

use std::collections::{HashMap, HashSet};

use styloria::{
    BlockKind, ComponentValue, DiagnosticKind, Parser, Rule, Span, Spanned, Token, spanned,
    validate_declaration_list, validate_stylesheet,
};

use crate::ids::*;
use crate::opf::{is_external, nfc, resolve};
use crate::report::{Position, Report, Severity};

/// Decode raw CSS bytes, honoring a UTF-16 BOM if present. Without this, a
/// legitimately UTF-16-encoded stylesheet (real, and `@charset`-declarable
/// per CSS) read as if it were UTF-8 produces garbage (stray NUL bytes and
/// `U+FFFD`s between every character), which then looks like a syntax error
/// to every check below — a false positive caused by the wrong encoding,
/// not by the CSS. Non-UTF-16 input still falls back to lossy UTF-8, same
/// as before. (Full `@charset`-vs-actual-encoding *mismatch* warnings —
/// CSS-003/004 — are still out of scope; this is just "don't corrupt valid
/// UTF-16 input before parsing it.")
pub(crate) fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        decode_utf16(&bytes[2..], true)
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        decode_utf16(&bytes[2..], false)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// True if `bytes` starts with a UTF-16 byte-order mark (big- or
/// little-endian). Shared by `decode_bytes` above and by `htm.rs`'s
/// HTM-058 (non-UTF-8 content document) check.
pub(crate) fn has_utf16_bom(bytes: &[u8]) -> bool {
    bytes.len() >= 2
        && ((bytes[0] == 0xFE && bytes[1] == 0xFF) || (bytes[0] == 0xFF && bytes[1] == 0xFE))
}

/// A UTF-8 byte order mark. Its presence settles the encoding, so any
/// `@charset` after it is decoration — CSS Syntax 3 §3.1 gives the BOM
/// precedence and only falls back to the declaration when none is found.
pub(crate) fn has_utf8_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
}

/// The encoding label of a **byte-exact** `@charset` declaration, per CSS
/// Syntax 3 §3.1: the literal bytes `@charset "`, then the label, then `";`.
///
/// Matched against the raw bytes rather than the parsed stylesheet on purpose.
/// A tokenizer sees `@charset '' ;` as a perfectly good at-rule named
/// `charset`; the *encoding declaration* it is not, because the spec says
/// "multiple spaces, comments, or single quotes … will cause the encoding
/// declaration to not be recognized". Reading it off the parse tree accepts
/// all three and reports a charset nobody declared.
///
/// The pattern must appear at the very start and within the first 1024 bytes,
/// which the fixed prefix and the label scan below enforce between them.
pub(crate) fn byte_exact_charset(bytes: &[u8]) -> Option<String> {
    const PREFIX: &[u8] = b"@charset \"";
    let rest = bytes.strip_prefix(PREFIX)?;
    let end = rest.iter().take(1024).position(|&b| b == b'"')?;
    if rest.get(end + 1) != Some(&b';') {
        return None;
    }
    std::str::from_utf8(&rest[..end]).ok().map(str::to_owned)
}

pub(crate) fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|c| {
        if big_endian {
            u16::from_be_bytes([c[0], c[1]])
        } else {
            u16::from_le_bytes([c[0], c[1]])
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
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
    // Span-carrying parse: every finding below points at the exact
    // line:column of the offending token (via `Position::of_offset`, the
    // same byte-offset→line:col helper the rest of epubveri uses, so CSS
    // positions count columns in chars just like every other finding).
    let (sheet, syntax_errs) = spanned::parse_stylesheet_with_errors(css);

    // Encoding checks only make sense for a standalone CSS file - see
    // `CssOrigin::File`, which is why the bytes live there rather than
    // arriving as a separate argument no other origin could ever supply.
    if let CssOrigin::File { bytes: Some(bytes) } = origin {
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
        // Not styloria's: its tokenizer note says encoding determination
        // happens before it runs (§3.2) and it merely tolerates a leftover
        // BOM. Determining the declared encoding from bytes is the caller's
        // job, which is here.
        if !has_utf8_bom(bytes)
            && let Some(charset) = byte_exact_charset(bytes)
            && !is_utf8_or_utf16(&charset)
        {
            report.push_at(
                CSS_004,
                Severity::Error,
                format!("@charset value '{charset}' is not utf-8 or utf-16"),
                css_path,
            );
        }
    }

    // Each collected item keeps the span of the token that produced it, so
    // the deferred RSC-00x findings below can report its position.
    let mut urls: Vec<Spanned<String>> = Vec::new();
    for rule in &sheet.rules {
        match &rule.node {
            spanned::Rule::Qualified(q) => {
                collect_urls_spanned(&q.prelude, &mut urls);
                collect_urls_spanned(&q.block.node.values, &mut urls);
                check_declaration_shapes_spanned(
                    &q.block.node.values,
                    css,
                    css_path,
                    origin,
                    is_epub3,
                    report,
                );
            }
            spanned::Rule::At(a) => {
                collect_urls_spanned(&a.prelude, &mut urls);
                if let Some(block) = &a.block {
                    if a.name.eq_ignore_ascii_case("font-face") {
                        check_font_face_spanned(
                            &block.node.values,
                            a.name_span,
                            css,
                            css_path,
                            origin,
                            report,
                        );
                    } else {
                        collect_urls_spanned(&block.node.values, &mut urls);
                    }
                    check_at_rule_block_spanned(
                        &a.name,
                        &block.node.values,
                        css,
                        css_path,
                        origin,
                        is_epub3,
                        report,
                    );
                }
                if a.name.eq_ignore_ascii_case("import")
                    && let Some(target) = import_target_spanned(&a.prelude)
                {
                    urls.push(target);
                }
            }
        }
    }

    // CSS-008: the syntax errors styloria's (error-recovering) parser
    // recovered from - bad string/url tokens, and unterminated rules/blocks -
    // now surfaced by styloria 0.4's `syntax_errors` rather than re-derived
    // here. In-block malformed declaration *shapes* aren't in this set (a
    // rule block is parsed as raw component values; the declaration split
    // happens in `check_declaration_shapes_spanned` below, which still emits
    // its own CSS-008 for those).
    for e in collapse_selector_errors(&syntax_errs, &sheet.rules) {
        report.push_full(
            CSS_008,
            Severity::Error,
            "CSS syntax error",
            css_path,
            origin.position(css, e.span.start),
            syntax_error_slug(e.kind),
            Vec::new(),
        );
    }
    for u in urls {
        let url = u.node;
        let pos = origin.position(css, u.span.start);
        if url.trim_start().starts_with("file:") {
            report.push_full(
                RSC_030,
                Severity::Error,
                format!("'{url}' is a file URL, which is not allowed"),
                css_path,
                pos,
                "css.url.file_scheme_not_allowed",
                vec![url.clone()],
            );
            continue;
        }
        if is_external(&url) {
            continue;
        }
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
            report.push_full(
                RSC_026,
                Severity::Error,
                format!("'{url}' leaks outside the container"),
                css_path,
                pos,
                "css.url.leaks_container_root",
                vec![url.clone()],
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
                report.push_full(
                    RSC_001,
                    Severity::Error,
                    format!("references a missing resource '{url}'"),
                    css_path,
                    pos,
                    "css.url.declared_resource_missing",
                    vec![url.clone()],
                );
            }
            (false, true) => {
                report.push_full(
                    RSC_008,
                    Severity::Error,
                    format!("resource '{url}' is not declared in the manifest"),
                    css_path,
                    pos,
                    "css.url.undeclared_resource",
                    vec![url.clone()],
                );
            }
            (false, false) => {
                report.push_full(
                    RSC_007,
                    Severity::Error,
                    format!("references a missing resource '{url}'"),
                    css_path,
                    pos,
                    "css.url.missing_resource",
                    vec![url.clone()],
                );
            }
            (true, true) => {}
        }
    }

    // Opt-in advisory pass (--advisory): unknown property/descriptor names,
    // which epubcheck does not check. Off by default, so the default output is
    // byte-identical. Positions map through `origin` like every CSS finding.
    if advisory {
        for r in styloria::spanned::parse_stylesheet(css).rules {
            let styloria::spanned::Rule::Qualified(q) = &r.node else {
                continue;
            };
            for name in styloria::type_selector_names(&q.prelude) {
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
        for d in validate_stylesheet(css) {
            let (id, rule, text, params) = advisory_fields(&d);
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
fn advisory_fields(d: &styloria::Diagnostic) -> (&'static str, &'static str, String, Vec<String>) {
    match d.kind {
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
    }
}

/// A `style="..."` attribute value is a plain declaration list (no
/// enclosing braces) - reuses `check_declaration_shapes` (built for a CSS
/// rule's block contents) by wrapping the text in a throwaway rule so
/// styloria's existing tokenizer/parser produces the same
/// `&[ComponentValue]` shape, rather than adding a new styloria entry
/// point for a one-off caller.
pub(crate) fn check_style_attribute(
    value: &str,
    path: &str,
    advisory: bool,
    is_epub3: bool,
    report: &mut Report,
) {
    let wrapped = format!("x{{{value}}}");
    let sheet = Parser::parse_stylesheet(&wrapped);
    if let Some(Rule::Qualified(q)) = sheet.rules.first() {
        check_declaration_shapes(&q.block.values, path, is_epub3, report);
    }

    // Opt-in advisory pass. A style attribute is a bare declaration list;
    // styloria 0.3 validates one directly. No document byte-offset is available
    // here, so the finding anchors at the file (path), not a line:column.
    if advisory {
        for d in validate_declaration_list(value) {
            let (id, rule, text, params) = advisory_fields(&d);
            report.push_at_rule(id, Severity::Usage, text, path, rule, params);
        }
    }
}

const FLAGGED_PROPERTIES: [&str; 2] = ["direction", "unicode-bidi"];

fn is_effectively_empty_spanned(values: &[Spanned<spanned::ComponentValue>]) -> bool {
    values
        .iter()
        .all(|v| matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)))
}

/// Collapse a rule's `InvalidSelector` errors to one, keeping every other
/// kind untouched.
///
/// styloria reports one per comma-separated selector, which is the right
/// granularity for a CSS library and was settled deliberately in its #3.
/// epubcheck's unit is the whole selector *list*: `. a, . b, . c { … }` is
/// three findings there and one here (#81). A real book made the gap visible:
/// `. h-100, . y-100 { … }` repeated down a stylesheet gave 22 CSS-008
/// against epubcheck's 12. The CSS really is broken either way, but inventing
/// ten extra errors on one file reads exactly like a false positive to anyone
/// diffing the two tools.
///
/// The library keeps its answer; the consumer adapts. That is this project's
/// standing rule about where epubcheck parity belongs.
///
/// Grouping is by the rule whose span encloses the error. Errors arrive
/// sorted by position and rule spans do not overlap at one level, so
/// remembering the last claimed rule is enough - no map needed.
fn collapse_selector_errors<'a>(
    errors: &'a [spanned::SyntaxError],
    rules: &[Spanned<spanned::Rule>],
) -> Vec<&'a spanned::SyntaxError> {
    let mut out = Vec::new();
    let mut claimed: Option<(usize, usize)> = None;
    for e in errors {
        if e.kind != spanned::SyntaxErrorKind::InvalidSelector {
            out.push(e);
            continue;
        }
        let owner = rules
            .iter()
            .find(|r| r.span.start <= e.span.start && e.span.start < r.span.end)
            .map(|r| (r.span.start, r.span.end));
        match owner {
            // A second bad selector in a rule already reported.
            Some(o) if claimed == Some(o) => {}
            Some(o) => {
                claimed = Some(o);
                out.push(e);
            }
            // No enclosing rule: not a selector-list error in practice, and
            // dropping it would be the silent-skip trade this project keeps
            // having to undo.
            None => out.push(e),
        }
    }
    out
}

/// The `rule` slug for one of styloria's syntax errors. Every one of them is
/// CSS-008 to epubcheck; the slug is where a consumer can tell them apart.
///
/// Shared by the top-level pass and the nested one, so a rule inside an
/// `@media` is keyed the same as the identical rule outside it - the two had
/// no reason to differ, and only one of them existed before styloria 0.9.
fn syntax_error_slug(kind: spanned::SyntaxErrorKind) -> &'static str {
    match kind {
        spanned::SyntaxErrorKind::BadString | spanned::SyntaxErrorKind::BadUrl => {
            "css.stylesheet.bad_token"
        }
        spanned::SyntaxErrorKind::UnterminatedRule
        | spanned::SyntaxErrorKind::UnterminatedBlock => "css.stylesheet.unterminated",
        spanned::SyntaxErrorKind::MalformedDeclaration
        | spanned::SyntaxErrorKind::UnexpectedToken => "css.stylesheet.malformed",
        // styloria 0.5 reads a qualified rule's prelude as a selector
        // list. Its own slug, so the finding says which half of the rule
        // was wrong: epubcheck reports both as CSS-008, but "the selector
        // is malformed" and "the declarations are malformed" send an
        // author to different places.
        spanned::SyntaxErrorKind::InvalidSelector => "css.stylesheet.invalid_selector",
        // Its own slug for the same reason as the selector one: epubcheck
        // reports every CSS parse problem as CSS-008, but "the range is
        // malformed" points somewhere quite different from "the block is".
        spanned::SyntaxErrorKind::InvalidUnicodeRange => "css.stylesheet.invalid_unicode_range",
        // styloria 0.7's nesting bound. Its own slug because this one is
        // not a defect in the CSS the way the others are - it says the
        // parser declined to descend further, and the stylesheet below
        // that point went unchecked. Real stylesheets nest 2 deep, so
        // reaching 256 means generated or hostile input; reporting it
        // under a shared "malformed" slug would hide which it was.
        spanned::SyntaxErrorKind::NestingTooDeep => "css.stylesheet.nesting_too_deep",
    }
}

/// Walk an at-rule's block, whatever it holds — nested rules for a
/// conditional-group rule or `@keyframes`, declarations for everything else.
///
/// **Which of those a given at-rule holds is styloria's question now** (its
/// issue #4). This function used to consult a `GROUPING_AT_RULES` list kept
/// here, and the trouble with a CSS table living in an EPUB validator is not
/// theoretical: the list knew the conditional-group rules and had never
/// heard of `@keyframes`, so a keyframe block was read as declarations and
/// `0% { opacity: 0 }` became a malformed declaration. That is CSS-008 on
/// valid CSS, on a construct in every animated fixed-layout book, and
/// epubcheck reports nothing there. `@-webkit-keyframes`, `@starting-style`
/// and any at-rule newer than the list failed the same way.
///
/// Adding the names would not have fixed it, which is the part worth
/// keeping: `@keyframes` holds rules whose preludes are `from`/`to`/`0%`,
/// correct under CSS Animations 1 §3 and malformed under Selectors 4, so
/// routing it through a selector-validating rule list trades one invented
/// error for two. It needed a third reading, and a third reading is a fact
/// about CSS rather than about this validator.
///
/// Nested rules still get the prelude check styloria 0.9's `parse_rule_list`
/// brought (its issue #2): `. foo { }` was once reported at the top level
/// and silently accepted one `@media` deep. That gap was found by
/// `compare`'s count diff rather than by a user — one shelf book where
/// epubcheck reported 11 CSS-008 and we reported 0, every one a selector
/// inside an `@media`, invisible in the totals because declaration errors in
/// the same blocks were reported normally.
fn check_at_rule_block_spanned(
    name: &str,
    block_values: &[Spanned<spanned::ComponentValue>],
    css: &str,
    css_path: &str,
    origin: CssOrigin,
    is_epub3: bool,
    report: &mut Report,
) {
    let (contents, errors) = styloria::parse_at_rule_block(name, block_values);
    match contents {
        // A declaration block is reported exactly as a style rule's is,
        // including the `rule` slug: a malformed declaration is the same
        // finding inside `@font-face` as outside it, and a consumer keying on
        // the slug has no reason to care which held it.
        styloria::BlockContents::Declarations(items) => {
            report_declarations(
                &items,
                &errors,
                DeclarationsIn::AtRule,
                css,
                css_path,
                origin,
                is_epub3,
                report,
            );
        }
        styloria::BlockContents::Rules(rules) => {
            for e in collapse_selector_errors(&errors, &rules) {
                report.push_full(
                    CSS_008,
                    Severity::Error,
                    "CSS syntax error",
                    css_path,
                    origin.position(css, e.span.start),
                    syntax_error_slug(e.kind),
                    Vec::new(),
                );
            }
            for r in &rules {
                match &r.node {
                    spanned::Rule::Qualified(q) => {
                        check_declaration_shapes_spanned(
                            &q.block.node.values,
                            css,
                            css_path,
                            origin,
                            is_epub3,
                            report,
                        );
                    }
                    spanned::Rule::At(a) => {
                        let Some(block) = &a.block else { continue };
                        check_at_rule_block_spanned(
                            &a.name,
                            &block.node.values,
                            css,
                            css_path,
                            origin,
                            is_epub3,
                            report,
                        );
                    }
                }
            }
        }
    }
}

/// Span-carrying twin of [`check_declaration_shapes`] used by the
/// finding-emitting `check` pass, so CSS-008 (malformed declaration) and
/// CSS-001 (flagged property) point at the exact token. The plain
/// [`check_declaration_shapes`] is kept for `check_style_attribute`, whose
/// fragment-relative offsets don't map back to a document position.
fn check_declaration_shapes_spanned(
    block_values: &[Spanned<spanned::ComponentValue>],
    css: &str,
    css_path: &str,
    origin: CssOrigin,
    is_epub3: bool,
    report: &mut Report,
) {
    // The declaration walk lives in styloria now (its issue #4). This used to
    // split on semicolons and decide "is this `ident :`" here, which is a CSS
    // syntax question sitting outside the CSS crate - and the reason it sat
    // here was an API asymmetry rather than a decision: `parse_rule_list`
    // took component values, its declaration twin did not exist, and a caller
    // holding a block has values and not source text.
    //
    // The `rule` slug stays `css.declaration.malformed_shape` rather than
    // becoming styloria's kind name. The slug is epubveri's key for
    // consumers; the crate boundary moving is not their business.
    let (items, errors) = styloria::parse_declaration_list_from_values(block_values);
    report_declarations(
        &items,
        &errors,
        DeclarationsIn::StyleRule,
        css,
        css_path,
        origin,
        is_epub3,
        report,
    );
}

/// What holds a declaration list, which decides whether an at-rule inside it
/// is misplaced.
///
/// `@page { @top-center { … } }` and `@font-feature-values { @styleset { … } }`
/// are ordinary CSS: an at-rule's block may contain at-rules. A *style
/// rule's* block may not — the only at-rule that can appear there is a
/// nested one, which is CSS Nesting, and nesting sits in §2.4 of the CSS
/// Snapshot ("modules with rough interoperability"), not in the official
/// definition of CSS that EPUB 3.3 defers to. epubcheck reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclarationsIn {
    StyleRule,
    AtRule,
}

/// Report a parsed declaration list: CSS-008 for what would not parse, then
/// the two EPUB rules about the declarations that did.
///
/// Beyond outright `BadString`/`BadUrl` tokens, real-world "CSS syntax
/// error" cases are more often a malformed *declaration* — one that isn't
/// shaped `ident: ...;` (e.g. `span.bold: bold;`, where the stray `.`
/// breaks the name into two tokens with no colon following the first) — or
/// an unclosed rule that swallows a subsequent rule whole (a `{`-block
/// that's missing its `}` makes everything up to the next real `}`,
/// including what was meant to be an unrelated sibling rule, part of the
/// unclosed block's own contents, which then obviously doesn't parse as a
/// clean declaration list either). Both reach styloria as "this
/// semicolon-delimited chunk doesn't start with `ident :`."
#[allow(clippy::too_many_arguments)]
fn report_declarations(
    items: &[styloria::spanned::DeclarationListItem],
    errors: &[spanned::SyntaxError],
    held_by: DeclarationsIn,
    css: &str,
    css_path: &str,
    origin: CssOrigin,
    is_epub3: bool,
    report: &mut Report,
) {
    for e in errors {
        report.push_full(
            CSS_008,
            Severity::Error,
            "CSS syntax error",
            css_path,
            origin.position(css, e.span.start),
            "css.declaration.malformed_shape",
            Vec::new(),
        );
    }
    for item in items {
        let d = match item {
            styloria::spanned::DeclarationListItem::Declaration(d) => d,
            // A nested at-rule. Its own slug rather than the malformed-shape
            // one: this is not a parse error — CSS Syntax §5.4.2 consumes an
            // at-rule in a declaration list quite happily — but a construct
            // outside the CSS that EPUB 3.3 accepts. Different reason,
            // different key.
            styloria::spanned::DeclarationListItem::AtRule(a)
                if held_by == DeclarationsIn::StyleRule =>
            {
                report.push_full(
                    CSS_008,
                    Severity::Error,
                    "CSS syntax error",
                    css_path,
                    origin.position(css, a.node.name_span.start),
                    "css.declaration.nested_at_rule",
                    Vec::new(),
                );
                continue;
            }
            styloria::spanned::DeclarationListItem::AtRule(_) => continue,
        };
        let name = &d.node.name;
        // CSS-001 is EPUB 3 only. epubcheck guards it with
        // `if (version == EPUBVersion.VERSION_3)` (CSSHandler.java:288) and
        // keeps its fixtures under `src/test/resources/epub3/`; its two
        // neighbours in the same method - CSS-006 below and the @font-face
        // work - are not guarded, so this is the whole class, not a sample.
        // We had no gate at all, which invented an error on an EPUB 2 book
        // carrying `<h1 style="direction: inherit">`.
        //
        // Both of these stay here rather than moving with the walk: "is
        // `direction` discouraged" is an EPUB rule, not a CSS one. CSS has
        // nothing against either property.
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
                origin.position(css, d.node.name_span.start),
            );
        } else if name.eq_ignore_ascii_case("position")
            && d.node
                .value
                .iter()
                .find(|v| !matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)))
                .is_some_and(|v| {
                    matches!(&v.node, spanned::ComponentValue::Token(Token::Ident(x))
                        if x.eq_ignore_ascii_case("fixed"))
                })
        {
            // CSS-006: `position: fixed` (matches epubcheck, which compares
            // the first value component to "fixed", case-insensitively).
            report.push_at_pos(
                CSS_006,
                Severity::Usage,
                "use of 'position: fixed' is not recommended".to_string(),
                css_path,
                origin.position(css, d.node.name_span.start),
            );
        }
    }
}

fn check_declaration_shapes(
    block_values: &[ComponentValue],
    css_path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    for chunk in block_values.split(|v| matches!(v, ComponentValue::Token(Token::Semicolon))) {
        let mut iter = chunk
            .iter()
            .filter(|v| !matches!(v, ComponentValue::Token(Token::Whitespace)));
        let first = iter.next();
        let malformed = match first {
            None => false,
            Some(ComponentValue::Token(Token::Ident(_))) => {
                !matches!(iter.next(), Some(ComponentValue::Token(Token::Colon)))
            }
            Some(_) => true,
        };
        if malformed {
            report.push_at_rule(
                CSS_008,
                Severity::Error,
                "CSS syntax error",
                css_path,
                "css.declaration.malformed_shape",
                Vec::new(),
            );
        } else if let Some(ComponentValue::Token(Token::Ident(name))) = first {
            if is_epub3
                && FLAGGED_PROPERTIES
                    .iter()
                    .any(|p| name.eq_ignore_ascii_case(p))
            {
                report.push_at(
                    CSS_001,
                    Severity::Error,
                    format!("use of the '{name}' property is not recommended"),
                    css_path,
                );
            } else if name.eq_ignore_ascii_case("position")
                && matches!(
                    iter.next(),
                    Some(ComponentValue::Token(Token::Ident(v))) if v.eq_ignore_ascii_case("fixed")
                )
            {
                report.push_at(
                    CSS_006,
                    Severity::Usage,
                    "use of 'position: fixed' is not recommended".to_string(),
                    css_path,
                );
            }
        }
        // A malformed chunk can still contain a nested block (e.g. an
        // unclosed rule swallowing a whole well-formed sibling rule) —
        // recurse so declarations inside it still get checked too.
        for v in chunk {
            if let ComponentValue::Block(b) = v
                && b.kind == BlockKind::Curly
            {
                check_declaration_shapes(&b.values, css_path, is_epub3, report);
            }
        }
    }
}

fn check_font_face_spanned(
    block_values: &[Spanned<spanned::ComponentValue>],
    name_span: Span,
    css: &str,
    css_path: &str,
    origin: CssOrigin,
    report: &mut Report,
) {
    // CSS-028 (usage): purely informational - real epubcheck notes every
    // `@font-face` it sees, so a reader comparing the two outputs isn't
    // left wondering which tool missed an embedded font. Anchored at the
    // `@font-face` keyword; nothing about the rule is wrong.
    report.push_full(
        CSS_028,
        Severity::Usage,
        "@font-face declaration",
        css_path,
        origin.position(css, name_span.start),
        "css.font_face.declared",
        Vec::new(),
    );
    if is_effectively_empty_spanned(block_values) {
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
    for chunk in
        block_values.split(|v| matches!(&v.node, spanned::ComponentValue::Token(Token::Semicolon)))
    {
        let mut iter = chunk
            .iter()
            .filter(|v| !matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)));
        let Some(f) = iter.next() else { continue };
        let spanned::ComponentValue::Token(Token::Ident(name)) = &f.node else {
            continue;
        };
        if !name.eq_ignore_ascii_case("src") {
            continue;
        }
        let Some(colon) = iter.next() else { continue };
        if !matches!(&colon.node, spanned::ComponentValue::Token(Token::Colon)) {
            continue;
        }
        let mut src_urls = Vec::new();
        collect_urls_spanned(chunk, &mut src_urls);
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

/// The `url()` target of every `@font-face`'s `src` declaration, each with
/// the span of the token it came from - unlike the generic `collect_urls`
/// pass (which deliberately skips `@font-face` blocks, handling them via
/// `check_font_face` instead), this is used by the CSS-007 non-standard-font
/// cross-reference in `opf.rs`, which needs each font's own resolved
/// manifest media-type to decide whether it's a Core Media Type.
///
/// Spans are carried so CSS-007 can point at the `src` url that names the
/// font, rather than at the stylesheet as a whole - "some font in this file
/// is wrong" leaves the reader to find which, and a stylesheet can declare
/// many.
pub(crate) fn font_face_src_urls_spanned(css: &str) -> Vec<Spanned<String>> {
    let sheet = spanned::parse_stylesheet(css);
    let mut out = Vec::new();
    for rule in &sheet.rules {
        let spanned::Rule::At(a) = &rule.node else {
            continue;
        };
        if !a.name.eq_ignore_ascii_case("font-face") {
            continue;
        }
        let Some(block) = &a.block else { continue };
        for chunk in block
            .node
            .values
            .split(|v| matches!(&v.node, spanned::ComponentValue::Token(Token::Semicolon)))
        {
            let mut iter = chunk
                .iter()
                .filter(|v| !matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)));
            let Some(f) = iter.next() else { continue };
            let spanned::ComponentValue::Token(Token::Ident(name)) = &f.node else {
                continue;
            };
            if !name.eq_ignore_ascii_case("src") {
                continue;
            }
            let Some(colon) = iter.next() else { continue };
            if !matches!(&colon.node, spanned::ComponentValue::Token(Token::Colon)) {
                continue;
            }
            collect_urls_spanned(chunk, &mut out);
        }
    }
    out.retain(|u| !u.node.is_empty());
    out
}

/// `@import`'s target is either a bare string (`@import "foo.css";`) or a
/// `url()` (`@import url(foo.css);`, already covered by the generic
/// `collect_urls` pass) — only the bare-string form needs special-casing
/// here, since a generic scanner can't tell a URL string apart from any
/// other string literal without knowing it's specifically in `@import`'s
/// prelude.
fn import_target(prelude: &[ComponentValue]) -> Option<String> {
    prelude.iter().find_map(|v| match v {
        ComponentValue::Token(Token::String(s)) => Some(s.to_string()),
        _ => None,
    })
}

/// Span-carrying twin of [`collect_urls`]: each collected `url()` target
/// keeps the span of the `url(...)` token/function it came from, so the
/// deferred RSC-00x resource findings can report its position. The whole
/// `url(...)` span is used (not just the inner string) so the caret lands
/// on the construct a reader looks for.
fn collect_urls_spanned(
    values: &[Spanned<spanned::ComponentValue>],
    out: &mut Vec<Spanned<String>>,
) {
    for v in values {
        match &v.node {
            spanned::ComponentValue::Token(Token::Url(s)) => {
                out.push(Spanned::new(s.to_string(), v.span))
            }
            spanned::ComponentValue::Function { name, args } => {
                if name.eq_ignore_ascii_case("url") {
                    if let Some(first) = args.first()
                        && let spanned::ComponentValue::Token(Token::String(s)) = &first.node
                    {
                        out.push(Spanned::new(s.to_string(), v.span));
                    }
                } else {
                    collect_urls_spanned(args, out);
                }
            }
            spanned::ComponentValue::Block(b) => collect_urls_spanned(&b.values, out),
            _ => {}
        }
    }
}

/// Span-carrying twin of [`import_target`] for `@import "foo.css";` (the
/// bare-string form). The `url()` form is already covered by
/// [`collect_urls_spanned`].
fn import_target_spanned(prelude: &[Spanned<spanned::ComponentValue>]) -> Option<Spanned<String>> {
    prelude.iter().find_map(|v| match &v.node {
        spanned::ComponentValue::Token(Token::String(s)) => {
            Some(Spanned::new(s.to_string(), v.span))
        }
        _ => None,
    })
}

fn collect_urls(values: &[ComponentValue], out: &mut Vec<String>) {
    for v in values {
        match v {
            ComponentValue::Token(Token::Url(s)) => out.push(s.to_string()),
            ComponentValue::Function { name, args } => {
                if name.eq_ignore_ascii_case("url") {
                    if let Some(ComponentValue::Token(Token::String(s))) = args.first() {
                        out.push(s.to_string());
                    }
                } else {
                    collect_urls(args, out);
                }
            }
            ComponentValue::Block(b) => collect_urls(&b.values, out),
            _ => {}
        }
    }
}

/// Just the target(s) of top-level `@import` rules, not every `url()` in
/// the sheet (unlike `stylesheet_urls` below) - used where callers need
/// to tell "this points at another stylesheet to also parse" apart from
/// an ordinary resource reference like `background: url(x.png)` (e.g.
/// `opf.rs`'s SVG active-class CSS scan, CSS-029/030, which needs to
/// merge an `@import`ed sheet's own selector class names, not just note
/// its existence as a used resource).
pub(crate) fn import_targets(sheet: &styloria::Stylesheet) -> Vec<String> {
    let mut urls = Vec::new();
    for rule in &sheet.rules {
        if let Rule::At(a) = rule
            && a.name.eq_ignore_ascii_case("import")
        {
            collect_urls(&a.prelude, &mut urls);
        }
    }
    urls
}

/// Every `url()` reference anywhere in a stylesheet (rule preludes,
/// declaration blocks, `@import` targets, nested blocks) - shared by
/// `check`'s own resource-resolution pass and, in `opf.rs`, the
/// remote-resources content-property scan (OPF-014/018), so a document's
/// remote references aren't just its raw attribute values but also its
/// own CSS.
pub(crate) fn stylesheet_urls(sheet: &styloria::Stylesheet) -> Vec<String> {
    let mut urls = Vec::new();
    for rule in &sheet.rules {
        match rule {
            Rule::Qualified(q) => {
                collect_urls(&q.prelude, &mut urls);
                collect_urls(&q.block.values, &mut urls);
            }
            Rule::At(a) => {
                // @namespace's "url(...)" declares an XML namespace URI
                // for selectors (e.g. `@namespace xlink
                // url('http://www.w3.org/1999/xlink')`) - it's never a
                // fetchable resource reference, unlike every other at-rule
                // that can carry a url().
                if a.name.eq_ignore_ascii_case("namespace") {
                    continue;
                }
                collect_urls(&a.prelude, &mut urls);
                if let Some(block) = &a.block {
                    collect_urls(&block.values, &mut urls);
                }
                if a.name.eq_ignore_ascii_case("import")
                    && let Some(target) = import_target(&a.prelude)
                {
                    urls.push(target);
                }
            }
        }
    }
    urls
}

/// Class names used as selectors in a stylesheet's top-level qualified
/// rules — e.g. `.foo, .bar { ... }` yields `{"foo", "bar"}`. Only
/// top-level rule preludes are scanned, not nested at-rule blocks (the
/// real media-overlay class fixtures this supports are flat, unnested
/// CSS); a class selector is a `Token::Delim('.')` immediately followed
/// by `Token::Ident(name)` in the raw prelude token stream — styloria's
/// phase-1 output has no selector grammar, so this is a token-level scan,
/// same style as `collect_urls` above.
pub(crate) fn selector_class_names(sheet: &styloria::Stylesheet) -> HashSet<String> {
    let mut names = HashSet::new();
    for rule in &sheet.rules {
        if let Rule::Qualified(q) = rule {
            for pair in q.prelude.windows(2) {
                if let [
                    ComponentValue::Token(Token::Delim('.')),
                    ComponentValue::Token(Token::Ident(name)),
                ] = pair
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

/// Every class selector in `css`, each with the span of the name token -
/// the same token-level scan as [`selector_class_names`], keeping where it
/// was written.
///
/// CSS-029 needs this: the class name it reports on lives in the
/// stylesheet, so pointing at the content document that merely links that
/// stylesheet sends the reader to a file the name does not appear in.
pub(crate) fn selector_class_names_spanned(css: &str) -> Vec<Spanned<String>> {
    let sheet = spanned::parse_stylesheet(css);
    let mut names = Vec::new();
    for rule in &sheet.rules {
        if let spanned::Rule::Qualified(q) = &rule.node {
            for pair in q.prelude.windows(2) {
                if let [dot, ident] = pair
                    && matches!(&dot.node, spanned::ComponentValue::Token(Token::Delim('.')))
                    && let spanned::ComponentValue::Token(Token::Ident(name)) = &ident.node
                {
                    names.push(Spanned::new(name.to_string(), dot.span));
                }
            }
        }
    }
    names
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
        let sheet = Parser::parse_stylesheet(".foo { color: red; }");
        assert_eq!(
            selector_class_names(&sheet),
            HashSet::from(["foo".to_string()])
        );
    }

    #[test]
    fn selector_class_names_comma_list() {
        let sheet = Parser::parse_stylesheet(".foo, .bar { color: red; }");
        assert_eq!(
            selector_class_names(&sheet),
            HashSet::from(["foo".to_string(), "bar".to_string()])
        );
    }

    #[test]
    fn selector_class_names_no_class() {
        let sheet = Parser::parse_stylesheet("body { color: red; } #id { color: blue; }");
        assert!(selector_class_names(&sheet).is_empty());
    }

    #[test]
    fn selector_class_names_empty_stylesheet() {
        let sheet = Parser::parse_stylesheet("");
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
            byte_exact_charset(b"@charset \"utf-8\";").as_deref(),
            Some("utf-8")
        );
        assert_eq!(
            byte_exact_charset(b"@charset  \"utf-8\";"),
            None,
            "two spaces"
        );
        assert_eq!(
            byte_exact_charset(b"@charset 'utf-8';"),
            None,
            "single quotes"
        );
        assert_eq!(
            byte_exact_charset(b"@charset \"utf-8\" ;"),
            None,
            "space before ;"
        );
        assert_eq!(
            byte_exact_charset(b"\n@charset \"utf-8\";"),
            None,
            "not at the start"
        );
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

    /// An at-rule styloria has no table entry for is read as declarations,
    /// and a nested rule inside it is not blamed. This is the direction the
    /// unknown case has to fail in: CSS keeps gaining at-rules, so any
    /// table is permanently behind the language, and a validator must not
    /// turn that into an error on a valid stylesheet. A malformed
    /// declaration is still caught — epubcheck agrees on both halves.
    #[test]
    fn an_unknown_at_rule_holding_rules_is_not_a_syntax_error() {
        assert!(run("@future { p { color: red } }", &empty_index()).is_empty());
        assert_eq!(run("@future { color red }", &empty_index()), vec![CSS_008]);
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

    #[test]
    fn missing_import_target_undeclared_and_absent() {
        // Real corpus finding: an undeclared *and* absent target is
        // RSC-007, not RSC-001 (RSC-001 is only for a manifest-declared
        // resource whose file is missing - see the tests below).
        let findings = run("@import \"missing.css\";", &empty_index());
        assert!(findings.contains(&RSC_007));
    }

    #[test]
    fn missing_background_url_nested_in_media() {
        let css = "@media screen { body { background: url(missing.png); } }";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&RSC_007));
    }

    #[test]
    fn import_target_declared_but_file_missing_is_rsc001() {
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
        assert_eq!(ids, vec![RSC_001]);
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
