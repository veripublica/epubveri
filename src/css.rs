//! CSS checks, via the `styloria` parser (a sibling project,
//! `github.com/veripublica/styloria` — a pure-Rust CSS3 tokenizer/core-
//! grammar parser/serializer with no selector or property-value grammar
//! yet):
//! - `CSS-008`: any `BadString`/`BadUrl` token anywhere in the stylesheet
//!   (styloria's tokenizer never hard-fails, so these tokens are the
//!   signal that *something* was malformed).
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

use styloria::{ComponentValue, Parser, Rule, Span, Spanned, Token, spanned};

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

pub(crate) fn check(
    css: &str,
    css_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    manifest_paths: &HashSet<String>,
    origin: CssOrigin,
    report: &mut Report,
) {
    // Span-carrying parse: every finding below points at the exact
    // line:column of the offending token (via `Position::of_offset`, the
    // same byte-offset→line:col helper the rest of epubveri uses, so CSS
    // positions count columns in chars just like every other finding).
    let sheet = spanned::parse_stylesheet(css);

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
        if let Some(charset) = sheet.rules.iter().find_map(|r| match &r.node {
            spanned::Rule::At(a) if a.name.eq_ignore_ascii_case("charset") => {
                charset_value_spanned(&a.prelude)
            }
            _ => None,
        }) && !is_utf8_or_utf16(&charset)
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
    // the deferred CSS-008 / RSC-00x findings below can report its position.
    let mut bad_spans: Vec<Span> = Vec::new();
    let mut urls: Vec<Spanned<String>> = Vec::new();
    for rule in &sheet.rules {
        match &rule.node {
            spanned::Rule::Qualified(q) => {
                collect_bad_spans(&q.prelude, &mut bad_spans);
                collect_bad_spans(&q.block.node.values, &mut bad_spans);
                collect_urls_spanned(&q.prelude, &mut urls);
                collect_urls_spanned(&q.block.node.values, &mut urls);
                check_declaration_shapes_spanned(
                    &q.block.node.values,
                    css,
                    css_path,
                    origin,
                    report,
                );
            }
            spanned::Rule::At(a) => {
                collect_bad_spans(&a.prelude, &mut bad_spans);
                collect_urls_spanned(&a.prelude, &mut urls);
                if let Some(block) = &a.block {
                    collect_bad_spans(&block.node.values, &mut bad_spans);
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
                    // A conditional-group at-rule (`@media`, `@supports`, …)
                    // contains nested *rules*, not declarations - its block
                    // must be walked as a rule list, or every nested rule's
                    // selector gets mis-flagged as a malformed declaration
                    // (issue #5: Vellum media-query stylesheets fired CSS-008
                    // once per `@media` block). Every other at-rule
                    // (`@font-face`, `@page`, …) has a declaration block.
                    if GROUPING_AT_RULES
                        .iter()
                        .any(|g| a.name.eq_ignore_ascii_case(g))
                    {
                        check_rule_list_block_spanned(
                            &block.node.values,
                            css,
                            css_path,
                            origin,
                            report,
                        );
                    } else {
                        check_declaration_shapes_spanned(
                            &block.node.values,
                            css,
                            css_path,
                            origin,
                            report,
                        );
                    }
                }
                if a.name.eq_ignore_ascii_case("import")
                    && let Some(target) = import_target_spanned(&a.prelude)
                {
                    urls.push(target);
                }
            }
        }
    }

    for span in bad_spans {
        report.push_full(
            CSS_008,
            Severity::Error,
            "CSS syntax error",
            css_path,
            origin.position(css, span.start),
            "css.stylesheet.bad_token",
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
}

/// A `style="..."` attribute value is a plain declaration list (no
/// enclosing braces) - reuses `check_declaration_shapes` (built for a CSS
/// rule's block contents) by wrapping the text in a throwaway rule so
/// styloria's existing tokenizer/parser produces the same
/// `&[ComponentValue]` shape, rather than adding a new styloria entry
/// point for a one-off caller.
pub(crate) fn check_style_attribute(value: &str, path: &str, report: &mut Report) {
    let wrapped = format!("x{{{value}}}");
    let sheet = Parser::parse_stylesheet(&wrapped);
    if let Some(Rule::Qualified(q)) = sheet.rules.first() {
        check_declaration_shapes(&q.block.values, path, report);
    }
}

fn charset_value_spanned(prelude: &[Spanned<spanned::ComponentValue>]) -> Option<String> {
    prelude.iter().find_map(|v| match &v.node {
        spanned::ComponentValue::Token(Token::String(s)) => Some(s.to_string()),
        _ => None,
    })
}

const FLAGGED_PROPERTIES: [&str; 2] = ["direction", "unicode-bidi"];

fn is_effectively_empty_spanned(values: &[Spanned<spanned::ComponentValue>]) -> bool {
    values
        .iter()
        .all(|v| matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)))
}

/// Beyond outright `BadString`/`BadUrl` tokens, real-world "CSS syntax
/// error" cases are more often a malformed *declaration* — one that isn't
/// shaped `ident: ...;` (e.g. `span.bold: bold;`, where the stray `.`
/// breaks the name into two tokens with no colon following the first) — or
/// an unclosed rule that swallows a subsequent rule whole (a `{`-block
/// that's missing its `}` makes everything up to the next real `}`,
/// including what was meant to be an unrelated sibling rule, part of the
/// unclosed block's own contents, which then obviously doesn't parse as a
/// clean declaration list either). Both show up here as "this
/// semicolon-delimited chunk doesn't start with `ident :`."
/// Conditional-group at-rules: their block holds nested *rules*, not
/// declarations (CSS Conditional Rules / Nesting). Names are matched
/// without the leading `@`, case-insensitively.
const GROUPING_AT_RULES: &[&str] = &[
    "media",
    "supports",
    "container",
    "layer",
    "scope",
    "document",
    "-moz-document",
];

/// Walk the block of a conditional-group at-rule, which holds nested rules
/// rather than declarations. For each nested rule's block, check the
/// declarations inside it: a further grouping at-rule (a nested `@media`)
/// recurses as a rule list; anything else is a qualified rule whose block
/// is an ordinary declaration list. The rule *preludes* (selectors /
/// conditions) are deliberately not shape-checked here - they are not
/// declarations (issue #5). A nested block is recognised as a grouping one
/// by containing a block of its own, mirroring how the top level dispatches.
fn check_rule_list_block_spanned(
    block_values: &[Spanned<spanned::ComponentValue>],
    css: &str,
    css_path: &str,
    origin: CssOrigin,
    report: &mut Report,
) {
    for v in block_values {
        if let spanned::ComponentValue::Block(b) = &v.node {
            if b.values
                .iter()
                .any(|nv| matches!(&nv.node, spanned::ComponentValue::Block(_)))
            {
                check_rule_list_block_spanned(&b.values, css, css_path, origin, report);
            } else {
                check_declaration_shapes_spanned(&b.values, css, css_path, origin, report);
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
    report: &mut Report,
) {
    for chunk in
        block_values.split(|v| matches!(&v.node, spanned::ComponentValue::Token(Token::Semicolon)))
    {
        let mut iter = chunk
            .iter()
            .filter(|v| !matches!(&v.node, spanned::ComponentValue::Token(Token::Whitespace)));
        let first = iter.next();
        let malformed = match first.map(|f| &f.node) {
            None => false,
            Some(spanned::ComponentValue::Token(Token::Ident(_))) => !matches!(
                iter.next().map(|v| &v.node),
                Some(spanned::ComponentValue::Token(Token::Colon))
            ),
            Some(_) => true,
        };
        if malformed {
            if let Some(f) = first {
                report.push_full(
                    CSS_008,
                    Severity::Error,
                    "CSS syntax error",
                    css_path,
                    origin.position(css, f.span.start),
                    "css.declaration.malformed_shape",
                    Vec::new(),
                );
            }
        } else if let Some(f) = first
            && let spanned::ComponentValue::Token(Token::Ident(name)) = &f.node
            && FLAGGED_PROPERTIES
                .iter()
                .any(|p| name.eq_ignore_ascii_case(p))
        {
            report.push_at_pos(
                CSS_001,
                Severity::Error,
                format!("use of the '{name}' property is not recommended"),
                css_path,
                origin.position(css, f.span.start),
            );
        }
        // A malformed chunk can still contain a nested block (e.g. an
        // unclosed rule swallowing a whole well-formed sibling rule) —
        // recurse so declarations inside it still get checked too.
        for v in chunk {
            if let spanned::ComponentValue::Block(b) = &v.node {
                check_declaration_shapes_spanned(&b.values, css, css_path, origin, report);
            }
        }
    }
}

fn check_declaration_shapes(block_values: &[ComponentValue], css_path: &str, report: &mut Report) {
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
        } else if let Some(ComponentValue::Token(Token::Ident(name))) = first
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
        }
        // A malformed chunk can still contain a nested block (e.g. an
        // unclosed rule swallowing a whole well-formed sibling rule) —
        // recurse so declarations inside it still get checked too.
        for v in chunk {
            if let ComponentValue::Block(b) = v {
                check_declaration_shapes(&b.values, css_path, report);
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
    report.push_at_pos(
        CSS_028,
        Severity::Usage,
        "@font-face declaration",
        css_path,
        origin.position(css, name_span.start),
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

/// Collect the span of every `BadString`/`BadUrl` token in the tree (the
/// span-carrying twin of the old bad-token *count*), so each CSS-008 can
/// report the exact position of the malformed token.
fn collect_bad_spans(values: &[Spanned<spanned::ComponentValue>], out: &mut Vec<Span>) {
    for v in values {
        match &v.node {
            spanned::ComponentValue::Token(Token::BadString | Token::BadUrl) => out.push(v.span),
            spanned::ComponentValue::Function { args, .. } => collect_bad_spans(args, out),
            spanned::ComponentValue::Block(b) => collect_bad_spans(&b.values, out),
            _ => {}
        }
    }
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
mod tests {
    use super::*;

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
            &mut report,
        );
        report.messages.iter().map(|m| m.id).collect()
    }

    fn empty_index() -> HashMap<String, String> {
        HashMap::new()
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

    #[test]
    fn font_face_position_points_at_at_rule() {
        // CSS-019 has no token inside an empty block, so it anchors at the
        // `@font-face` keyword on line 2.
        let css = "body { color: red; }\n@font-face {}";
        let pos = pos_of(&run_report(css, &empty_index()), CSS_019);
        assert_eq!(pos.line, 2);
    }
}
