//! Fixed-layout (`rendition:layout-pre-paginated`) content-dimension checks:
//! the XHTML `<meta name="viewport">` mini-grammar and the SVG `viewBox`
//! requirement. Plain XML/attribute parsing, no new parser needed.
//!
//! The grammar is EPUB 3.3 appendix F (`#app-viewport-meta-syntax`): after
//! XML whitespace normalization, properties are separated by any run of `,`,
//! `;` or space, `=` may carry one space on either side, and a property may
//! have no value. Only the **first** `<meta name="viewport">` in a document is
//! checked (additional ones are usage-flagged, not validated). `width` and
//! `height` must be a positive CSS 2 number or `device-width` /
//! `device-height` (section 8.2.2.6, `#sec-fxl-icb-html`); other properties
//! (e.g. `initial-scale`) are ignored.

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

/// Whether `n` is a `<meta>` whose `name` is `viewport` after XML whitespace
/// normalization (appendix F), so `name=" viewport "` counts.
fn is_viewport_meta(n: &roxmltree::Node) -> bool {
    n.is_element()
        && n.tag_name().name() == "meta"
        && n.attr_no_ns("name").map(crate::xmlext::trim_xml_space) == Some("viewport")
}

/// Checks a fixed-layout XHTML content document's viewport declaration.
pub(crate) fn check_xhtml_viewport(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let metas: Vec<_> = d.descendants().filter(is_viewport_meta).collect();

    if metas.is_empty() {
        report.push_at_pos(
            HTM_046,
            Severity::Error,
            "fixed-layout content document has no viewport meta element",
            path,
            Position::of(d.root_element()),
        );
        return;
    }

    for m in metas.iter().skip(1) {
        report.push_node(
            HTM_060A,
            Severity::Usage,
            "additional viewport meta elements are not checked",
            path,
            *m,
            "layout.viewport.additional_meta_ignored",
            Vec::new(),
        );
    }

    check_viewport_content(
        metas[0].attr_no_ns("content").unwrap_or(""),
        path,
        metas[0],
        report,
    );
}

/// A reflowable document's viewport metadata isn't validated at all - just
/// usage-flagged if present (confirmed via the real corpus: "no other
/// errors or warnings are reported" alongside the usage finding).
pub(crate) fn check_reflowable_viewport(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let viewport = d.descendants().find(is_viewport_meta);
    if let Some(n) = viewport {
        report.push_node(
            HTM_060B,
            Severity::Usage,
            "viewport metadata is not checked in reflowable content documents",
            path,
            n,
            "layout.viewport.reflowable_not_checked",
            Vec::new(),
        );
    }
}

/// Checks a fixed-layout SVG content document's `viewBox` declaration.
pub(crate) fn check_svg_viewbox(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let root = d.root_element();
    if root.tag_name().name() == "svg" && root.attr_no_ns("viewBox").is_none() {
        report.push_at_pos(
            HTM_048,
            Severity::Error,
            "fixed-layout SVG has no viewBox attribute",
            path,
            Position::of(root),
        );
    }
}

/// One property of a viewport `content` value: its name, and its value if
/// it has an `=`.
type ViewportProperty = (String, Option<String>);

/// Parses a viewport `content` value by appendix F's grammar. `Err` carries
/// the reason the value is not of that form.
fn parse_viewport(content: &str) -> Result<Vec<ViewportProperty>, &'static str> {
    // Whitespace normalization first: every requirement applies after it.
    // Then the assign production's optional spaces around `=` are folded in,
    // so the only spaces left are separators.
    let norm = crate::xmlext::normalize_xml_space(content)
        .replace(" =", "=")
        .replace("= ", "=");
    if norm.is_empty() {
        return Ok(Vec::new());
    }
    let is_sep = |c: char| matches!(c, ',' | ';' | ' ');
    if norm.starts_with(is_sep) {
        return Err("it starts with a separator");
    }
    if norm.ends_with(is_sep) {
        return Err("it ends with a separator");
    }
    let mut props = Vec::new();
    for piece in norm.split(is_sep).filter(|p| !p.is_empty()) {
        match piece.split_once('=') {
            None => props.push((piece.to_string(), None)),
            Some((_, value)) if value.contains('=') => {
                return Err("a value contains '='");
            }
            Some(("", _)) => return Err("a property has no name"),
            Some((_, "")) => return Err("a property has '=' but no value"),
            Some((name, value)) => props.push((name.to_string(), Some(value.to_string()))),
        }
    }
    Ok(props)
}

fn check_viewport_content(content: &str, path: &str, node: roxmltree::Node, report: &mut Report) {
    let props = match parse_viewport(content) {
        Ok(props) => props,
        Err(reason) => {
            report.push_at_pos(
                HTM_047,
                Severity::Error,
                format!("viewport content '{content}' is not valid syntax: {reason}"),
                path,
                Position::of(node),
            );
            return;
        }
    };

    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (key, value) in &props {
        let key = key.as_str();
        if !matches!(key, "width" | "height") {
            continue;
        }
        *seen.entry(key).or_insert(0) += 1;
        match value {
            Some(value) => {
                if !is_valid_viewport_dimension(key, value) {
                    report.push_node(
                        HTM_057,
                        Severity::Error,
                        format!("viewport '{key}' value '{value}' is not valid"),
                        path,
                        node,
                        "layout.viewport.invalid_value",
                        vec![key.to_string(), value.to_string()],
                    );
                }
            }
            None => {
                report.push_node(
                    HTM_057,
                    Severity::Error,
                    format!("viewport '{key}' has no value"),
                    path,
                    node,
                    "layout.viewport.missing_value",
                    vec![key.to_string()],
                );
            }
        }
    }

    for key in ["width", "height"] {
        match seen.get(key) {
            None => {
                report.push_at_pos(
                    HTM_056,
                    Severity::Error,
                    format!("viewport is missing the '{key}' key"),
                    path,
                    Position::of(node),
                );
            }
            Some(&n) if n > 1 => {
                report.push_at_pos(
                    HTM_059,
                    Severity::Error,
                    format!("viewport '{key}' key appears more than once"),
                    path,
                    Position::of(node),
                );
            }
            _ => {}
        }
    }
}

/// A viewport meta `width` / `height` value: a positive CSS 2 number
/// (`syndata.html#numbers`: an optional sign, then digits, or digits, a dot
/// and at least one digit) or the matching `device-*` keyword.
fn is_valid_viewport_dimension(key: &str, value: &str) -> bool {
    if (key == "width" && value == "device-width") || (key == "height" && value == "device-height")
    {
        return true;
    }
    let unsigned = value.strip_prefix('+').unwrap_or(value);
    let (int, frac) = match unsigned.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (unsigned, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let well_formed = match frac {
        None => !int.is_empty() && digits(int),
        Some(frac) => !frac.is_empty() && digits(int) && digits(frac),
    };
    well_formed && unsigned.bytes().any(|b| (b'1'..=b'9').contains(&b))
}

/// The deprecated `rendition:viewport` property's value check, which keeps
/// its own looser form (digits with at most one dot, or a `device-*`
/// keyword); not appendix F's.
pub(crate) fn is_valid_rendition_viewport_value(key: &str, value: &str) -> bool {
    if (key == "width" && value == "device-width") || (key == "height" && value == "device-height")
    {
        return true;
    }
    let mut has_digit = false;
    let mut has_dot = false;
    for c in value.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if c == '.' && !has_dot {
            has_dot = true;
        } else {
            return false;
        }
    }
    has_digit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_xhtml(content: &str) -> Vec<&'static str> {
        let xhtml = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="{content}"/></head><body/></html>"#
        );
        let d = crate::ocf::parse_xml(&xhtml).unwrap();
        let mut report = Report::new();
        check_xhtml_viewport(&d, "content.xhtml", &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    #[test]
    fn valid_forms_produce_no_findings() {
        assert!(run_xhtml("width=600,height=1200").is_empty());
        assert!(run_xhtml("width=600.999,height=1200.5").is_empty());
        assert!(run_xhtml("width=device-width,height=device-height").is_empty());
        assert!(run_xhtml("  width\t=\t600,\n  height\t=\t1200  ").is_empty());
    }

    #[test]
    fn appendix_f_separators_are_comma_semicolon_or_space() {
        assert!(run_xhtml("width=1200;height=600").is_empty());
        assert!(run_xhtml("width=1200 height=600").is_empty());
        assert!(run_xhtml("width=1200, ;height=600").is_empty());
        assert!(run_xhtml("width=1200,,,height=600").is_empty());
        assert!(run_xhtml("width = 1200 ; height = 600").is_empty());
    }

    #[test]
    fn valueless_properties_are_allowed_by_the_grammar() {
        assert!(run_xhtml("width=1200, height=600, user-scalable").is_empty());
        assert!(run_xhtml("p1 v1, width=1200, height=600").is_empty());
        // A valueless width is grammatical but not a positive number.
        assert_eq!(run_xhtml("width 1200, height=600"), vec![HTM_057]);
    }

    #[test]
    fn syntax_errors_are_one_htm_047() {
        for content in [
            ",width=1200,height=600",
            "width=1200,height=600;",
            "width==1200,height=600",
            "width=1200=1,height=600",
            "=1200,width=1200,height=600",
        ] {
            assert_eq!(run_xhtml(content), vec![HTM_047], "{content}");
        }
    }

    #[test]
    fn dimensions_must_be_positive_css2_numbers() {
        assert_eq!(run_xhtml("width=1200, height=0"), vec![HTM_057]);
        assert_eq!(run_xhtml("width=0.0, height=600"), vec![HTM_057]);
        assert_eq!(run_xhtml("width=-5, height=600"), vec![HTM_057]);
        assert_eq!(run_xhtml("width=1200., height=600"), vec![HTM_057]);
        assert_eq!(run_xhtml("width=1e3, height=600"), vec![HTM_057]);
        assert!(run_xhtml("width=+1200, height=.5").is_empty());
        assert!(run_xhtml("width=0.5, height=0600").is_empty());
    }

    #[test]
    fn padded_viewport_name_is_normalized() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name=" viewport " content="width=1200, height=600"/></head><body/></html>"#;
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_xhtml_viewport(&d, "content.xhtml", &mut report);
        assert!(report.messages.is_empty());
        let mut report = Report::new();
        check_reflowable_viewport(&d, "content.xhtml", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_060B]);
    }

    #[test]
    fn missing_viewport_meta() {
        let d = crate::ocf::parse_xml(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head/><body/></html>"#,
        )
        .unwrap();
        let mut report = Report::new();
        check_xhtml_viewport(&d, "content.xhtml", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_046]);
    }

    #[test]
    fn missing_width_or_height_key() {
        assert_eq!(run_xhtml("height=600"), vec![HTM_056]);
        assert_eq!(run_xhtml("width=600"), vec![HTM_056]);
    }

    #[test]
    fn bare_key_with_no_equals() {
        assert_eq!(run_xhtml("width=600,height"), vec![HTM_057]);
    }

    #[test]
    fn units_suffixed_value_is_invalid_per_key() {
        let ids = run_xhtml("width=600px,height=1200px");
        assert_eq!(ids, vec![HTM_057, HTM_057]);
    }

    #[test]
    fn blank_value_after_equals_is_one_syntax_error() {
        assert_eq!(run_xhtml("width=600,height=\t"), vec![HTM_047]);
        assert_eq!(run_xhtml("height=705, width="), vec![HTM_047]);
    }

    #[test]
    fn duplicate_width_and_height() {
        let ids = run_xhtml("width=600,height=1200,width=device-width,height=device-height");
        assert_eq!(ids, vec![HTM_059, HTM_059]);
    }

    #[test]
    fn extra_viewport_metas_are_usage_only() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head>
            <meta name="viewport" content="width=600,height=1200"/>
            <meta name="viewport" content="width=600,height=400"/>
            <meta name="viewport" content="width=,height=10px"/>
        </head><body/></html>"#;
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_xhtml_viewport(&d, "content.xhtml", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_060A, HTM_060A]);
    }

    #[test]
    fn reflowable_viewport_is_usage_only_not_an_error() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="width=600"/></head><body/></html>"#;
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_reflowable_viewport(&d, "content.xhtml", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_060B]);
    }

    #[test]
    fn svg_missing_viewbox() {
        let d = crate::ocf::parse_xml(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"/>"#,
        )
        .unwrap();
        let mut report = Report::new();
        check_svg_viewbox(&d, "cover.svg", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_048]);
    }

    #[test]
    fn svg_with_viewbox_is_valid() {
        let d = crate::ocf::parse_xml(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 100 100"/>"#).unwrap();
        let mut report = Report::new();
        check_svg_viewbox(&d, "cover.svg", &mut report);
        assert!(report.messages.is_empty());
    }
}
