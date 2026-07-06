//! Fixed-layout (`rendition:layout-pre-paginated`) content-dimension checks:
//! the XHTML `<meta name="viewport">` mini-grammar and the SVG `viewBox`
//! requirement. Plain XML/attribute parsing, no new parser needed.
//!
//! Grounded in the real corpus (read-only, for understanding — same
//! clean-room stance as every prior increment): only the **first**
//! `<meta name="viewport">` in a document is checked at all (additional
//! ones are usage-flagged, not validated); the recognized keys are
//! `width`/`height`, with `device-width`/`device-height` as the only valid
//! keyword values; unrecognized keys (e.g. `initial-scale`) are silently
//! ignored, matching the real corpus's own fixtures.

use crate::ids::*;
use crate::report::{Position, Report, Severity};

/// Checks a fixed-layout XHTML content document's viewport declaration.
pub(crate) fn check_xhtml_viewport(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let metas: Vec<_> = d
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("name") == Some("viewport")
        })
        .collect();

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
        report.push_full(
            HTM_060,
            Severity::Info,
            "additional viewport meta elements are not checked",
            path,
            Position::of(*m),
            "layout.viewport.additional_meta_ignored",
            Vec::new(),
        );
    }

    check_viewport_content(
        metas[0].attribute("content").unwrap_or(""),
        path,
        metas[0],
        report,
    );
}

/// A reflowable document's viewport metadata isn't validated at all - just
/// usage-flagged if present (confirmed via the real corpus: "no other
/// errors or warnings are reported" alongside the usage finding).
pub(crate) fn check_reflowable_viewport(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let viewport = d.descendants().find(|n| {
        n.is_element() && n.tag_name().name() == "meta" && n.attribute("name") == Some("viewport")
    });
    if let Some(n) = viewport {
        report.push_full(
            HTM_060,
            Severity::Info,
            "viewport metadata is not checked in reflowable content documents",
            path,
            Position::of(n),
            "layout.viewport.reflowable_not_checked",
            Vec::new(),
        );
    }
}

/// Checks a fixed-layout SVG content document's `viewBox` declaration.
pub(crate) fn check_svg_viewbox(d: &roxmltree::Document, path: &str, report: &mut Report) {
    let root = d.root_element();
    if root.tag_name().name() == "svg" && root.attribute("viewBox").is_none() {
        report.push_at_pos(
            HTM_048,
            Severity::Error,
            "fixed-layout SVG has no viewBox attribute",
            path,
            Position::of(root),
        );
    }
}

fn check_viewport_content(content: &str, path: &str, node: roxmltree::Node, report: &mut Report) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut has_blank_value = false;

    for piece in content.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match piece.split_once('=') {
            Some((key, value)) => {
                let key = key.trim();
                let value = value.trim();
                if !matches!(key, "width" | "height") {
                    continue;
                }
                *seen.entry(key).or_insert(0) += 1;
                if value.is_empty() {
                    has_blank_value = true;
                } else if !is_valid_viewport_value(key, value) {
                    report.push_full(
                        HTM_057,
                        Severity::Error,
                        format!("viewport '{key}' value '{value}' is not valid"),
                        path,
                        Position::of(node),
                        "layout.viewport.invalid_value",
                        vec![key.to_string(), value.to_string()],
                    );
                }
            }
            None => {
                if matches!(piece, "width" | "height") {
                    *seen.entry(piece).or_insert(0) += 1;
                    report.push_full(
                        HTM_057,
                        Severity::Error,
                        format!("viewport '{piece}' has no value"),
                        path,
                        Position::of(node),
                        "layout.viewport.missing_value",
                        vec![piece.to_string()],
                    );
                }
            }
        }
    }

    if has_blank_value {
        report.push_at_pos(
            HTM_047,
            Severity::Error,
            "viewport content has a blank value",
            path,
            Position::of(node),
        );
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

pub(crate) fn is_valid_viewport_value(key: &str, value: &str) -> bool {
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
        assert_eq!(ids, vec![HTM_060, HTM_060]);
    }

    #[test]
    fn reflowable_viewport_is_usage_only_not_an_error() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="width=600"/></head><body/></html>"#;
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_reflowable_viewport(&d, "content.xhtml", &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_060]);
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
