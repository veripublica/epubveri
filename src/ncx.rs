//! NCX (EPUB 2 table of contents) content checks. Plain XML, so
//! `roxmltree` (via `ocf::parse_xml`) handles it directly — no new parser
//! needed. Before this, the NCX file was only checked for existence and
//! correct media-type (via the spine `toc` attribute, `OPF-050`); its
//! internal structure was never parsed.

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) fn check(ncx_xml: &str, ncx_path: &str, package_uid: &str, report: &mut Report) {
    let Ok(d) = crate::ocf::parse_xml(ncx_xml) else {
        return;
    };
    let root = d.root_element();

    if let Some(head) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "head")
        && let Some(meta) = head.children().find(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attr_no_ns("name") == Some("dtb:uid")
        })
        && let Some(content) = meta.attr_no_ns("content")
        && content.trim() != package_uid.trim()
    {
        report.push_at_pos(
            NCX_001,
            Severity::Error,
            format!(
                "dtb:uid '{}' does not match the package's identifier '{}'",
                content.trim(),
                package_uid.trim()
            ),
            ncx_path,
            Position::of(meta),
        );
    }

    if let Some(doc_title) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "docTitle")
    {
        check_empty_text(doc_title, ncx_path, report);
    }

    for nav_label in d
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "navLabel")
    {
        check_empty_text(nav_label, ncx_path, report);
    }

    check_id_attributes(&d, ncx_path, report);
    check_page_target_types(&d, ncx_path, report);
}

/// Every `id` attribute anywhere in the NCX must be a valid XML NCName
/// (confirmed via a real fixture using `np:1`, invalid only because of the
/// colon) and unique document-wide (confirmed via a real fixture where
/// `navMap` and `navPoint` share one value, reported once *per* colliding
/// element - 2 findings for 2 elements, not 1 for the pair).
fn check_id_attributes(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    let mut by_id: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            if !is_valid_ncname(id) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"id\" is invalid: '{id}'"),
                    ncx_path,
                    n,
                    "ncx.ids.invalid_ncname",
                    vec![id.to_string()],
                );
            }
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id")
            && by_id.get(id).copied().unwrap_or(0) > 1
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("The \"id\" attribute does not have a unique value: '{id}'"),
                ncx_path,
                n,
                "ncx.ids.duplicate_id",
                vec![id.to_string()],
            );
        }
    }
}

fn is_valid_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A `pageTarget`'s `type` must be one of the three DAISY-defined values.
fn check_page_target_types(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "pageTarget")
    {
        if let Some(ty) = n.attr_no_ns("type")
            && !matches!(ty, "front" | "normal" | "special")
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("value of attribute \"type\" is invalid: '{ty}'"),
                ncx_path,
                n,
                "ncx.page_target.invalid_type",
                vec![ty.to_string()],
            );
        }
    }
}

fn check_empty_text(container: roxmltree::Node, ncx_path: &str, report: &mut Report) {
    let Some(text_el) = container
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "text")
    else {
        return;
    };
    // `Node::text()` returns content for comment nodes too, not just text
    // nodes - filter to real text first (same gap fixed for the
    // title-empty check in a prior increment).
    let text: String = text_el
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    if text.trim().is_empty() {
        report.push_at_pos(
            NCX_006,
            Severity::Usage,
            "empty text label",
            ncx_path,
            Position::of(text_el),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ncx: &str, uid: &str) -> Vec<&'static str> {
        let mut report = Report::new();
        check(ncx, "toc.ncx", uid, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    const CLEAN: &str = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
        <head><meta name="dtb:uid" content="NOID"/></head>
        <docTitle><text>Moby Dick</text></docTitle>
        <navMap>
            <navPoint id="np-1" playOrder="1">
                <navLabel><text>Loomings</text></navLabel>
                <content src="content_001.xhtml"/>
            </navPoint>
        </navMap>
    </ncx>"#;

    #[test]
    fn clean_ncx_no_findings() {
        assert!(run(CLEAN, "NOID").is_empty());
    }

    #[test]
    fn uid_match_allows_surrounding_whitespace() {
        assert!(run(CLEAN, "  NOID  ").is_empty());
    }

    #[test]
    fn uid_mismatch_errors() {
        let findings = run(CLEAN, "something-else");
        assert!(findings.contains(&NCX_001));
    }

    #[test]
    fn empty_doc_title_and_nav_label() {
        let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
            <head><meta name="dtb:uid" content="NOID"/></head>
            <docTitle><text></text></docTitle>
            <navMap>
                <navPoint id="np-1" playOrder="1">
                    <navLabel><text></text></navLabel>
                    <content src="content_001.xhtml"/>
                </navPoint>
            </navMap>
        </ncx>"#;
        let findings = run(ncx, "NOID");
        assert_eq!(findings, vec![NCX_006, NCX_006]);
    }

    #[test]
    fn comment_only_label_counts_as_empty() {
        let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
            <head><meta name="dtb:uid" content="NOID"/></head>
            <docTitle><text><!--empty--></text></docTitle>
            <navMap>
                <navPoint id="np-1" playOrder="1">
                    <navLabel><text>Loomings</text></navLabel>
                    <content src="content_001.xhtml"/>
                </navPoint>
            </navMap>
        </ncx>"#;
        let findings = run(ncx, "NOID");
        assert_eq!(findings, vec![NCX_006]);
    }
}
