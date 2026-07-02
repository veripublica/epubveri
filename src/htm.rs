//! XML declaration / DOCTYPE / entity / encoding checks, plus a handful of
//! attribute- and element-level content-document checks. `roxmltree`
//! exposes no structured API for the XML declaration's version, DOCTYPE
//! entities, or a document's original encoding (confirmed by reading its
//! public API) — those need small hand-written raw byte/text scanners
//! (`check_raw`), same "no new dependency" style as `smil.rs`'s
//! clock-value parser and `layout.rs`'s viewport grammar, not a full
//! DTD/XML-declaration parser. The rest (`check_dom`) works off the
//! already-parsed document.

use crate::ids::*;
use crate::report::{Report, Severity};

/// Raw byte/text scans on a content document — independent of whether it
/// parses as well-formed XML, so these still fire even when e.g. a UTF-16
/// garbling (before the `css::decode_bytes` fix) would otherwise have
/// broken the parse.
///
/// EPUB3-only: confirmed via the real corpus that all of these checks
/// (`content-document-xhtml.feature`, under `epub3/`) don't apply to
/// EPUB2's XHTML content model, which is its own, more lenient, XHTML
/// 1.1-DTD-based spec section — an EPUB2 content document legitimately
/// declares `<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" ...>`,
/// which would otherwise be misflagged as HTM-004's "obsolete" doctype (a
/// real false positive found via the corpus, not by inspection: dozens of
/// EPUB2 fixtures across the corpus reuse exactly this doctype as their
/// standard template).
pub(crate) fn check_raw(bytes: &[u8], text: &str, path: &str, is_epub3: bool, report: &mut Report) {
    if !is_epub3 {
        return;
    }
    if crate::css::has_utf16_bom(bytes) {
        report.push_at(
            HTM_058,
            Severity::Error,
            "content document is not UTF-8 encoded",
            path,
        );
    }

    if let Some(decl_end) = text
        .trim_start()
        .strip_prefix("<?xml")
        .and_then(|rest| rest.find("?>"))
    {
        let decl = &text.trim_start()[..decl_end + "<?xml".len() + "?>".len()];
        if decl.contains("version=\"1.1\"") || decl.contains("version='1.1'") {
            report.push_at(
                HTM_001,
                Severity::Error,
                "XML declaration must not use version 1.1",
                path,
            );
        }
    }

    check_doctype(text, path, report);
    check_entities(text, path, report);
}

/// A minimal, well-formedness-only entity-reference scanner. Runs on the
/// raw text regardless of whether the document parses as XML at all —
/// `roxmltree` simply fails to parse a document with a malformed or
/// undeclared entity reference (confirmed: neither of the two real corpus
/// fixtures for this parse successfully today), so this is the only place
/// these two conditions can be caught. Numeric character references
/// (`&#39;`/`&#x27;`) are always well-formed and out of scope here — only
/// named references (`&foo;`) are checked.
fn check_entities(text: &str, path: &str, report: &mut Report) {
    let declared = declared_entity_names(text);
    const PREDEFINED: &[&str] = &["amp", "lt", "gt", "apos", "quot"];
    // `&foo;` inside a comment or CDATA section is literal text, not a
    // real entity reference (confirmed via a real corpus fixture titled
    // exactly this) - mask any '&' found there so the scan below skips it,
    // without disturbing any other byte offset in the text.
    let masked = mask_comments_and_cdata(text);
    let text = masked.as_str();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(rel) = text[i..].find('&') {
        let amp = i + rel;
        let after = &text[amp + 1..];
        if after.starts_with('#') {
            i = amp + 1;
            continue;
        }
        let name_len = after
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        if name_len == 0 {
            i = amp + 1;
            continue;
        }
        let name = &after[..name_len];
        let terminated = bytes.get(amp + 1 + name_len) == Some(&b';');
        if !terminated {
            report.push_at(
                RSC_016,
                Severity::Error,
                format!("entity reference '&{name}' must end with the ';' delimiter"),
                path,
            );
        } else if !PREDEFINED.contains(&name) && !declared.iter().any(|d| d == name) {
            report.push_at(
                RSC_016,
                Severity::Error,
                format!("entity '{name}' was referenced, but not declared"),
                path,
            );
        }
        i = amp + 1 + name_len;
    }
}

/// Blanks out (with spaces, preserving every other byte offset) any '&'
/// found inside `<!-- -->` comments or `<![CDATA[ ]]>` sections, so
/// `check_entities`'s scan never mistakes literal comment/CDATA text for a
/// real entity reference.
fn mask_comments_and_cdata(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>")] {
        let mut i = 0;
        while let Some(rel) = text[i..].find(open) {
            let start = i + rel + open.len();
            let Some(end_rel) = text[start..].find(close) else {
                break;
            };
            let end = start + end_rel;
            for b in &mut out[start..end] {
                if *b == b'&' {
                    *b = b' ';
                }
            }
            i = end + close.len();
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Entity names declared in the DOCTYPE's internal subset
/// (`<!ENTITY name "...">`), so a legitimately custom-declared entity
/// reference isn't misflagged as unknown.
fn declared_entity_names(text: &str) -> Vec<String> {
    let Some(start) = text.find("<!DOCTYPE") else {
        return Vec::new();
    };
    let after = &text[start..];
    let Some(open) = after.find('[') else {
        return Vec::new();
    };
    let Some(close) = after[open..].find(']') else {
        return Vec::new();
    };
    let subset = &after[open + 1..open + close];
    let mut names = Vec::new();
    let mut i = 0;
    while let Some(rel) = subset[i..].find("<!ENTITY") {
        let rest = subset[i + rel + "<!ENTITY".len()..].trim_start();
        let name_len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        if name_len > 0 {
            names.push(rest[..name_len].to_string());
        }
        i += rel + "<!ENTITY".len();
    }
    names
}

/// Finds the full `<!DOCTYPE ...>` declaration, correctly skipping past
/// any `>` inside an internal subset (`[...]`) to find the declaration's
/// *own* closing `>`.
fn extract_doctype(text: &str) -> Option<&str> {
    let start = text.find("<!DOCTYPE")?;
    let after = &text[start..];
    let search_from = match after.find('[') {
        Some(bracket) => bracket + after[bracket..].find(']')?,
        None => 0,
    };
    let end = after[search_from..].find('>')?;
    Some(&after[..search_from + end + 1])
}

fn check_doctype(text: &str, path: &str, report: &mut Report) {
    let Some(doctype) = extract_doctype(text) else {
        return;
    };
    if doctype.contains(" PUBLIC ") {
        report.push_at(
            HTM_004,
            Severity::Error,
            "DOCTYPE has an obsolete PUBLIC identifier",
            path,
        );
    }
    if let (Some(open), Some(close)) = (doctype.find('['), doctype.rfind(']')) {
        if close > open {
            let subset = &doctype[open + 1..close];
            for (i, _) in subset.match_indices("<!ENTITY") {
                let rest = &subset[i..];
                let Some(end) = rest.find('>') else { continue };
                let decl = &rest[..end];
                if decl.contains("SYSTEM") || decl.contains("PUBLIC") {
                    report.push_at(
                        HTM_003,
                        Severity::Error,
                        "entity is declared external (SYSTEM/PUBLIC)",
                        path,
                    );
                }
            }
        }
    }
}

/// A DOCTYPE in the OPF only makes sense if its declared root name matches
/// the OPF's own root element, `package`. Confirmed via the real corpus:
/// the legacy OEB 1.2 `<!DOCTYPE package PUBLIC "...OEB 1.2 Package//EN" ...>`
/// is explicitly *valid* (root name "package" matches), while
/// `<!DOCTYPE html PUBLIC "...OEB 1.2 Document//EN" ...>` is invalid (root
/// name "html" doesn't match) — the PUBLIC identifier itself isn't what's
/// being judged, just whether the declared root is sane for this document.
pub(crate) fn check_opf_doctype(text: &str, opf_path: &str, report: &mut Report) {
    let Some(start) = text.find("<!DOCTYPE") else {
        return;
    };
    let after = text[start + "<!DOCTYPE".len()..].trim_start();
    let root_name = after
        .split(|c: char| c.is_whitespace() || c == '>' || c == '[')
        .next()
        .unwrap_or("");
    if root_name != "package" {
        report.push_at(
            HTM_009,
            Severity::Error,
            format!("OPF document's DOCTYPE root '{root_name}' does not match <package>"),
            opf_path,
        );
    }
}

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SSML_NS: &str = "http://www.w3.org/2001/10/synthesis";

/// Legitimate w3.org/idpf.org namespaces content documents actually use
/// (XHTML itself, EPUB's own `epub:` ops namespace, XML/XLink, SVG,
/// MathML) — HTM-054 is about *custom* attributes impersonating a
/// w3.org/idpf.org affiliation, not these standard, expected ones.
const KNOWN_NAMESPACES: [&str; 7] = [
    XHTML_NS,
    "http://www.idpf.org/2007/ops",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/1999/xlink",
    SSML_NS,
    "http://www.w3.org/2000/svg",
    "http://www.w3.org/1998/Math/MathML",
];

/// DOM/attribute-level checks on an already-parsed content document.
/// EPUB3-only, same reasoning as `check_raw` above (all confirmed from the
/// `epub3/06-content-document/content-document-xhtml.feature` section).
pub(crate) fn check_dom(d: &roxmltree::Document, path: &str, is_epub3: bool, report: &mut Report) {
    if !is_epub3 {
        return;
    }
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.tag_name().namespace() == Some(XHTML_NS)
            && matches!(node.tag_name().name(), "base" | "embed" | "rp")
        {
            report.push_at(
                HTM_055,
                Severity::Info,
                format!("'{}' is a discouraged construct", node.tag_name().name()),
                path,
            );
        }

        for attr in node.attributes() {
            match attr.namespace() {
                Some(ns) if ns == SSML_NS && attr.name() == "ph" => {
                    if attr.value().trim().is_empty() {
                        report.push_at(
                            HTM_007,
                            Severity::Warning,
                            "ssml:ph must not be empty",
                            path,
                        );
                    }
                }
                Some(ns) if !KNOWN_NAMESPACES.contains(&ns) && reserved_namespace_host(ns) => {
                    report.push_at(
                        HTM_054,
                        Severity::Error,
                        format!("attribute uses a reserved namespace '{ns}'"),
                        path,
                    );
                }
                None => {
                    if let Some(rest) = attr.name().strip_prefix("data-") {
                        if !is_valid_data_attr_suffix(rest) {
                            report.push_at(
                                HTM_061,
                                Severity::Error,
                                format!("'data-{rest}' is not a valid data-* attribute name"),
                                path,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn reserved_namespace_host(uri: &str) -> bool {
    let after_scheme = uri.split("://").nth(1).unwrap_or(uri);
    let host = after_scheme.split('/').next().unwrap_or(after_scheme);
    host == "w3.org"
        || host.ends_with(".w3.org")
        || host == "idpf.org"
        || host.ends_with(".idpf.org")
}

fn is_valid_data_attr_suffix(rest: &str) -> bool {
    !rest.is_empty() && !rest.starts_with('-') && !rest.chars().any(|c| c.is_ascii_uppercase())
}

/// The HTML5 `<time datetime>` microsyntax - reverse-engineered directly
/// from the real corpus's exhaustive valid/invalid fixture pairs (not from
/// memory of the spec text), since several of its rules are non-obvious:
/// the separator between date and time may be either "T" or a literal
/// space; an offset may be "+HHMM" or "+HH:MM" but never combined with a
/// trailing "Z"; a fractional-seconds part (in a plain time, a global
/// date-time, or a duration) is capped at 1-3 digits even though nothing
/// else here is digit-count-limited; and a "duration" string has two
/// entirely different valid shapes - an ISO-8601-like "P...T..." form, or
/// a bare whitespace-separated sequence of "<number><unit>" components
/// with no "P"/"T" markers at all (both are exercised by real fixtures).
pub(crate) fn is_valid_html5_datetime(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || !s.is_ascii() {
        return false;
    }
    is_valid_year(s)
        || is_valid_month(s)
        || is_valid_date(s)
        || is_valid_yearless_date(s)
        || is_valid_time(s)
        || is_valid_week(s)
        || is_valid_global_datetime(s)
        || is_valid_duration(s)
}

fn all_digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

fn digit_range(s: &str, lo: u32, hi: u32) -> bool {
    all_digits(s, 2) && s.parse::<u32>().is_ok_and(|n| n >= lo && n <= hi)
}

fn is_valid_year(s: &str) -> bool {
    all_digits(s, 4)
}

fn is_valid_month(s: &str) -> bool {
    let Some((y, m)) = s.split_once('-') else {
        return false;
    };
    all_digits(y, 4) && digit_range(m, 1, 12)
}

fn is_valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    matches!(parts.as_slice(), [y, m, d] if all_digits(y, 4) && digit_range(m, 1, 12) && digit_range(d, 1, 31))
}

fn is_valid_yearless_date(s: &str) -> bool {
    let s = s.strip_prefix("--").unwrap_or(s);
    let parts: Vec<&str> = s.split('-').collect();
    matches!(parts.as_slice(), [m, d] if digit_range(m, 1, 12) && digit_range(d, 1, 31))
}

fn is_valid_week(s: &str) -> bool {
    let Some((y, w)) = s.split_once("-W") else {
        return false;
    };
    all_digits(y, 4) && digit_range(w, 1, 53)
}

fn valid_seconds(s: &str) -> bool {
    match s.split_once('.') {
        Some((whole, frac)) => {
            digit_range(whole, 0, 59)
                && !frac.is_empty()
                && frac.len() <= 3
                && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => digit_range(s, 0, 59),
    }
}

fn is_valid_time(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.as_slice() {
        [h, m] => digit_range(h, 0, 23) && digit_range(m, 0, 59),
        [h, m, sec] => digit_range(h, 0, 23) && digit_range(m, 0, 59) && valid_seconds(sec),
        _ => false,
    }
}

/// A full date, "T" or a single space, a time, and an optional "Z" or
/// numeric UTC offset (never both).
fn is_valid_global_datetime(s: &str) -> bool {
    if s.len() < 11 {
        return false;
    }
    let (date_part, rest) = s.split_at(10);
    if !is_valid_date(date_part) {
        return false;
    }
    if rest[0..1] != *"T" && rest[0..1] != *" " {
        return false;
    }
    let time_and_offset = &rest[1..];
    if time_and_offset.starts_with(' ') {
        return false;
    }
    if let Some(time_part) = time_and_offset.strip_suffix('Z') {
        return is_valid_time(time_part);
    }
    if time_and_offset.len() >= 6 {
        let (maybe_time, off) = time_and_offset.split_at(time_and_offset.len() - 6);
        if (off.starts_with('+') || off.starts_with('-'))
            && &off[3..4] == ":"
            && digit_range(&off[1..3], 0, 23)
            && digit_range(&off[4..6], 0, 59)
        {
            return is_valid_time(maybe_time);
        }
    }
    if time_and_offset.len() >= 5 {
        let (maybe_time, off) = time_and_offset.split_at(time_and_offset.len() - 5);
        if (off.starts_with('+') || off.starts_with('-'))
            && digit_range(&off[1..3], 0, 23)
            && digit_range(&off[3..5], 0, 59)
        {
            return is_valid_time(maybe_time);
        }
    }
    is_valid_time(time_and_offset)
}

fn is_valid_duration(s: &str) -> bool {
    match s.strip_prefix('P') {
        Some(rest) => is_valid_p_duration(rest),
        None => is_valid_flat_duration(s),
    }
}

/// Consumes leading `<digits><unit>` runs from `cursor` in the given unit
/// order, returning `None` if any unit's preceding digit-run isn't purely
/// digits (which also naturally rejects out-of-order units, since a unit
/// letter appearing "too early" ends up embedded inside a supposedly
/// all-digit span for a later unit).
fn consume_units<'a>(mut cursor: &'a str, units: &[char], any: &mut bool) -> Option<&'a str> {
    for unit in units {
        if let Some(idx) = cursor.find(*unit) {
            let num = &cursor[..idx];
            if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *any = true;
            cursor = &cursor[idx + 1..];
        }
    }
    Some(cursor)
}

fn is_valid_p_duration(rest: &str) -> bool {
    let (date_part, time_part) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut any = false;
    let Some(leftover) = consume_units(date_part, &['Y', 'M', 'D'], &mut any) else {
        return false;
    };
    if !leftover.is_empty() {
        return false;
    }
    if let Some(t) = time_part {
        if t.is_empty() {
            return false;
        }
        let mut any_time = false;
        let Some(leftover) = consume_units(t, &['H', 'M'], &mut any_time) else {
            return false;
        };
        let leftover = match leftover.strip_suffix('S') {
            Some(rest_s) if !rest_s.is_empty() => {
                let ok = match rest_s.split_once('.') {
                    Some((whole, frac)) => {
                        !whole.is_empty()
                            && whole.bytes().all(|b| b.is_ascii_digit())
                            && !frac.is_empty()
                            && frac.len() <= 3
                            && frac.bytes().all(|b| b.is_ascii_digit())
                    }
                    None => rest_s.bytes().all(|b| b.is_ascii_digit()),
                };
                if !ok {
                    return false;
                }
                any_time = true;
                ""
            }
            _ => leftover,
        };
        if !leftover.is_empty() || !any_time {
            return false;
        }
        any = true;
    }
    any
}

fn is_valid_duration_component(token: &str) -> bool {
    let Some(unit) = token.chars().next_back() else {
        return false;
    };
    if !matches!(unit, 'Y' | 'M' | 'W' | 'D' | 'H' | 'S') {
        return false;
    }
    let num = &token[..token.len() - unit.len_utf8()];
    match num.split_once('.') {
        Some((whole, frac)) => {
            !whole.is_empty()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && !frac.is_empty()
                && frac.len() <= 3
                && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()),
    }
}

fn is_valid_flat_duration(s: &str) -> bool {
    let mut any = false;
    for token in s.split_whitespace() {
        if !is_valid_duration_component(token) {
            return false;
        }
        any = true;
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_raw(text: &str) -> Vec<&'static str> {
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    fn run_dom(xhtml: &str) -> Vec<&'static str> {
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_dom(&d, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    #[test]
    fn xml_11_declaration_errors() {
        assert_eq!(run_raw("<?xml version=\"1.1\"?><html/>"), vec![HTM_001]);
        assert!(run_raw("<?xml version=\"1.0\"?><html/>").is_empty());
    }

    #[test]
    fn utf16_bom_errors() {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend("<html/>".encode_utf16().flat_map(|c| c.to_be_bytes()));
        let mut report = Report::new();
        check_raw(&bytes, "<html/>", "content.xhtml", true, &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_058]);
    }

    #[test]
    fn obsolete_public_doctype_errors() {
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        assert_eq!(run_raw(text), vec![HTM_004]);
    }

    #[test]
    fn legacy_compat_doctype_is_valid() {
        let text = r#"<!DOCTYPE html SYSTEM "about:legacy-compat"><html/>"#;
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn epub2_xhtml11_dtd_doctype_is_not_flagged() {
        // A real false positive found via the corpus, not by inspection:
        // dozens of EPUB2 fixtures across the corpus legitimately use this
        // exact doctype as their standard XHTML 1.1 content-document
        // template - HTM-004 only applies to EPUB3's stricter content
        // model.
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", false, &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn external_entity_errors() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo SYSTEM \"sample.dtd\">\n]><html/>";
        assert_eq!(run_raw(text), vec![HTM_003]);
    }

    #[test]
    fn internal_only_entity_is_valid() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo \"foo\">\n]><html/>";
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn reserved_namespace_attribute_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:w3="http://example.w3.org" xmlns:idpf="http://example.idpf.org" xmlns:ok="http://example.org/w3.org">
            <body w3:attr="disallowed" idpf:attr="disallowed" ok:attr="allowed"/>
        </html>"#;
        let ids = run_dom(xhtml);
        assert_eq!(ids, vec![HTM_054, HTM_054]);
    }

    #[test]
    fn known_epub_and_xhtml_namespaces_are_not_reserved() {
        // epub:type is a legitimate, standard attribute - not a custom
        // attribute impersonating a w3.org/idpf.org affiliation. A real
        // false positive found via scripts/spike.py's nav fixture
        // (epub:type="toc"), not by inspection.
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
            <body><nav epub:type="toc"/></body>
        </html>"#;
        assert!(run_dom(xhtml).is_empty());
    }

    #[test]
    fn discouraged_elements() {
        for el in ["base", "embed", "rp"] {
            let xhtml = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><{el}/></body></html>"#
            );
            assert_eq!(run_dom(&xhtml), vec![HTM_055], "element {el}");
        }
    }

    #[test]
    fn ssml_ph_empty_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:ssml="http://www.w3.org/2001/10/synthesis">
            <body><span ssml:ph=" "></span><span ssml:ph="ok"></span></body>
        </html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_007]);
    }

    #[test]
    fn invalid_data_attrs() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div data-=""/>
            <div data--test=""/>
            <div data-ERR=""/>
            <div data-ok=""/>
        </body></html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_061, HTM_061, HTM_061]);
    }

    #[test]
    fn opf_doctype_errors() {
        let mut report = Report::new();
        check_opf_doctype(
            "<!DOCTYPE html PUBLIC \"x\" \"y\"><package/>",
            "content.opf",
            &mut report,
        );
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_009]);

        let mut report = Report::new();
        check_opf_doctype("<package/>", "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn opf_doctype_with_matching_root_name_is_valid() {
        // A real false positive found via the corpus: the legacy OEB 1.2
        // *Package* doctype's root name ("package") matches the OPF's own
        // root element, so it's explicitly valid - only a root-name
        // mismatch (e.g. an "html" doctype in a "package" document) is an
        // actual error; the PUBLIC identifier itself isn't what's judged.
        let text = r#"<!DOCTYPE package PUBLIC "+//ISBN 0-9673008-1-9//DTD OEB 1.2 Package//EN" "http://openebook.org/dtds/oeb-1.2/oebpkg12.dtd"><package/>"#;
        let mut report = Report::new();
        check_opf_doctype(text, "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn clean_document_no_findings() {
        let xhtml = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body data-ok="1"/></html>"#;
        assert!(run_raw(xhtml).is_empty());
        assert!(run_dom(xhtml).is_empty());
    }

    #[test]
    fn datetime_rejects_all_25_real_invalid_values() {
        // Every one of `time-error.xhtml`'s 25 real corpus datetime values,
        // each individually confirmed invalid.
        const INVALID: &[&str] = &[
            "201",
            "09999001",
            "01--31",
            "---12-31",
            "3123-05-31-12",
            "2019-01-25T 12:12:12Z",
            "2019-01-2512:12:12",
            "2019-01-25A12:12:12Z",
            "2019-01-25T12:12:12-0500Z",
            "2019-01-25 12:12:12-05 00",
            "2019-01-25 12:12:12.33777",
            "2018-W522",
            "2019W01",
            "08::40",
            "19:24:291",
            "14:08:59.999999",
            "P32DT",
            "P32D223T12H",
            "P23DT32M12H",
            "PT2.1112S",
            "PT12H9",
            "P12T431M",
            "9123W12",
            "  1231 23D  ",
            "343HD",
        ];
        for v in INVALID {
            assert!(!is_valid_html5_datetime(v), "expected invalid: {v}");
        }
    }

    #[test]
    fn datetime_accepts_all_real_valid_values() {
        // Every one of `time-valid.xhtml`'s real corpus datetime values.
        const VALID: &[&str] = &[
            "2019",
            "0001",
            "01-31",
            "02-28",
            "--12-31",
            "3123-05-31",
            "1200-08",
            "2019-01-25T12:12:12Z",
            "2019-01-25 12:12:12",
            "2019-01-25 12:12:12Z",
            "2019-01-25 12:12:12-0500",
            "2019-01-25 12:12:12-05:00",
            "2019-01-25 12:12:12.777",
            "2018-W52",
            "2019-W01",
            "08:40",
            "19:24:29",
            "14:08:59.999",
            "P32D",
            "P32DT12H",
            "P23DT12H32M1231S",
            "PT12H23M12.112S",
            "PT12H",
            "PT431M",
            "PT12.433S",
            "9123W",
            "  123123D  ",
            "343H",
            "1M",
            "12S",
            "12.12S",
            "123W 123H   32D 12S",
            "2014-03",
        ];
        for v in VALID {
            assert!(is_valid_html5_datetime(v), "expected valid: {v}");
        }
    }

    #[test]
    fn entity_missing_semicolon_and_unknown_name() {
        let mut report = Report::new();
        check_entities("&amp ", "content.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );

        let mut report = Report::new();
        check_entities("&foo;", "content.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );
    }

    #[test]
    fn entity_predefined_and_declared_are_valid() {
        let mut report = Report::new();
        check_entities("&amp; &lt; &gt;", "content.xhtml", &mut report);
        assert!(report.messages.is_empty());

        let mut report = Report::new();
        check_entities(
            "<!DOCTYPE html [<!ENTITY foo \"bar\">]><p>&foo;</p>",
            "content.xhtml",
            &mut report,
        );
        assert!(report.messages.is_empty());
    }
}
