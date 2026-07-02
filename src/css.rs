//! CSS checks, via the `styloria` parser (a sibling project,
//! `github.com/veripublica/styloria` — a pure-Rust CSS3 tokenizer/core-
//! grammar parser/serializer with no selector or property-value grammar
//! yet). Everything here is built on that existing phase-1 output only:
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

use std::collections::{HashMap, HashSet};

use styloria::{ComponentValue, Parser, Rule, Token};

use crate::ids::*;
use crate::opf::{is_external, nfc, resolve};
use crate::report::{Report, Severity};

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

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
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

pub(crate) fn check(
    css: &str,
    css_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    raw_bytes: Option<&[u8]>,
    report: &mut Report,
) {
    let sheet = Parser::parse_stylesheet(css);

    // Encoding checks only make sense for a standalone CSS file — an inline
    // <style> block's encoding is already resolved as part of its XHTML
    // document by the time we see its text, so `raw_bytes` is `None` there.
    if let Some(bytes) = raw_bytes {
        if has_utf16_bom(bytes) {
            report.push_at(
                CSS_003,
                Severity::Warning,
                "stylesheet is UTF-16 encoded",
                css_path,
            );
        }
        if let Some(charset) = sheet.rules.iter().find_map(|r| match r {
            Rule::At(a) if a.name.eq_ignore_ascii_case("charset") => charset_value(&a.prelude),
            _ => None,
        }) {
            if !charset.eq_ignore_ascii_case("utf-8") && !charset.eq_ignore_ascii_case("utf-16") {
                report.push_at(
                    CSS_004,
                    Severity::Error,
                    format!("@charset value '{charset}' is not utf-8 or utf-16"),
                    css_path,
                );
            }
        }
    }

    let mut bad_tokens = 0usize;
    let mut urls: Vec<String> = Vec::new();
    for rule in &sheet.rules {
        match rule {
            Rule::Qualified(q) => {
                count_bad_tokens(&q.prelude, &mut bad_tokens);
                count_bad_tokens(&q.block.values, &mut bad_tokens);
                collect_urls(&q.prelude, &mut urls);
                collect_urls(&q.block.values, &mut urls);
                check_declaration_shapes(&q.block.values, css_path, report);
            }
            Rule::At(a) => {
                count_bad_tokens(&a.prelude, &mut bad_tokens);
                collect_urls(&a.prelude, &mut urls);
                if let Some(block) = &a.block {
                    count_bad_tokens(&block.values, &mut bad_tokens);
                    if a.name.eq_ignore_ascii_case("font-face") {
                        check_font_face(&block.values, css_path, report);
                    } else {
                        collect_urls(&block.values, &mut urls);
                    }
                    check_declaration_shapes(&block.values, css_path, report);
                }
                if a.name.eq_ignore_ascii_case("import") {
                    if let Some(target) = import_target(&a.prelude) {
                        urls.push(target);
                    }
                }
            }
        }
    }

    for _ in 0..bad_tokens {
        report.push_at(CSS_008, Severity::Error, "CSS syntax error", css_path);
    }
    for url in urls {
        if is_external(&url) {
            continue;
        }
        let resolved = resolve(base_dir, &url);
        if !name_index.contains_key(&nfc(&resolved)) {
            report.push_at(
                RSC_001,
                Severity::Error,
                format!("references a missing resource '{url}'"),
                css_path,
            );
        }
    }
}

fn charset_value(prelude: &[ComponentValue]) -> Option<String> {
    prelude.iter().find_map(|v| match v {
        ComponentValue::Token(Token::String(s)) => Some(s.to_string()),
        _ => None,
    })
}

const FLAGGED_PROPERTIES: [&str; 2] = ["direction", "unicode-bidi"];

fn is_effectively_empty(values: &[ComponentValue]) -> bool {
    values
        .iter()
        .all(|v| matches!(v, ComponentValue::Token(Token::Whitespace)))
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
            report.push_at(CSS_008, Severity::Error, "CSS syntax error", css_path);
        } else if let Some(ComponentValue::Token(Token::Ident(name))) = first {
            if FLAGGED_PROPERTIES
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

fn check_font_face(block_values: &[ComponentValue], css_path: &str, report: &mut Report) {
    if is_effectively_empty(block_values) {
        report.push_at(
            CSS_019,
            Severity::Warning,
            "@font-face has an empty declaration block",
            css_path,
        );
        return;
    }
    for chunk in block_values.split(|v| matches!(v, ComponentValue::Token(Token::Semicolon))) {
        let mut iter = chunk
            .iter()
            .filter(|v| !matches!(v, ComponentValue::Token(Token::Whitespace)));
        let Some(ComponentValue::Token(Token::Ident(name))) = iter.next() else {
            continue;
        };
        if !name.eq_ignore_ascii_case("src") {
            continue;
        }
        let Some(ComponentValue::Token(Token::Colon)) = iter.next() else {
            continue;
        };
        let mut src_urls = Vec::new();
        collect_urls(chunk, &mut src_urls);
        if src_urls.iter().any(|u| u.is_empty()) {
            report.push_at(
                CSS_002,
                Severity::Error,
                "@font-face 'src' has an empty url()",
                css_path,
            );
        }
    }
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

fn count_bad_tokens(values: &[ComponentValue], out: &mut usize) {
    for v in values {
        match v {
            ComponentValue::Token(Token::BadString | Token::BadUrl) => *out += 1,
            ComponentValue::Function { args, .. } => count_bad_tokens(args, out),
            ComponentValue::Block(b) => count_bad_tokens(&b.values, out),
            _ => {}
        }
    }
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
        if let Rule::At(a) = rule {
            if a.name.eq_ignore_ascii_case("import") {
                collect_urls(&a.prelude, &mut urls);
            }
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
                if a.name.eq_ignore_ascii_case("import") {
                    if let Some(target) = import_target(&a.prelude) {
                        urls.push(target);
                    }
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
                if let [ComponentValue::Token(Token::Delim('.')), ComponentValue::Token(Token::Ident(name))] =
                    pair
                {
                    names.insert(name.to_string());
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
        check(css, "style.css", "OEBPS", name_index, None, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    fn run_bytes(bytes: &[u8]) -> Vec<&'static str> {
        let text = decode_bytes(bytes);
        let mut report = Report::new();
        check(
            &text,
            "style.css",
            "OEBPS",
            &HashMap::new(),
            Some(bytes),
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
        let mut idx = empty_index();
        idx.insert("OEBPS/font.woff".to_string(), "OEBPS/font.woff".to_string());
        let css = "@font-face { font-family: X; src: url(font.woff); } body { color: red; }";
        assert!(run(css, &idx).is_empty());
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
    fn missing_import_target() {
        let findings = run("@import \"missing.css\";", &empty_index());
        assert!(findings.contains(&RSC_001));
    }

    #[test]
    fn missing_background_url_nested_in_media() {
        let css = "@media screen { body { background: url(missing.png); } }";
        let findings = run(css, &empty_index());
        assert!(findings.contains(&RSC_001));
    }

    #[test]
    fn external_urls_are_not_checked() {
        let css = "body { background: url(https://example.com/x.png); }";
        assert!(run(css, &empty_index()).is_empty());
    }
}
