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
    check_play_order(&d, ncx_path, report);
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

/// `playOrder` is optional, but where present it must be unique across
/// every `navPoint`/`navTarget`/`pageTarget` in the document - it *is* the
/// reading order, so two elements claiming the same position is a
/// contradiction.
///
/// The one exception is the reason this can't be a plain duplicate scan:
/// elements that point at the **same target** may share a playOrder, since
/// they name one position reached by two routes (a navPoint and the
/// pageTarget for the same page, say). So a value is only a violation when
/// the elements carrying it disagree about where they go, and then every one
/// of them is reported - the defect is the collision, not one arbitrary
/// member of it, and a reader given a single line would have to hunt for the
/// other. Matches epubcheck, which reports each colliding element.
///
/// (Reported missing by Doitsu on the MobileRead forum: epubcheck flagged
/// four elements on a real EPUB 2 book where epubveri flagged none.)
fn check_play_order(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    use std::collections::HashMap;

    // playOrder -> the elements claiming it, each with the target it names.
    let mut claims: HashMap<&str, Vec<(roxmltree::Node, String)>> = HashMap::new();
    for n in doc.descendants().filter(|n| {
        n.is_element() && matches!(n.tag_name().name(), "navPoint" | "navTarget" | "pageTarget")
    }) {
        let Some(order) = n.attr_no_ns("playOrder") else {
            continue;
        };
        let target = n
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "content")
            .and_then(|c| c.attr_no_ns("src"))
            .unwrap_or_default()
            .trim()
            .to_string();
        claims.entry(order).or_default().push((n, target));
    }

    // Collected first, then reported in document order: `claims` is keyed by
    // a hash, so reporting straight out of it would order the findings
    // differently from run to run. epubcheck reports these in document
    // order, and so should we - a report that reshuffles itself between
    // identical runs is one nobody can diff.
    let mut offenders: Vec<(roxmltree::Node, &str)> = Vec::new();
    for (order, holders) in &claims {
        if holders.len() < 2 {
            continue;
        }
        let first = &holders[0].1;
        if holders.iter().all(|(_, t)| t == first) {
            // One position, reached by several routes - legitimate.
            continue;
        }
        offenders.extend(holders.iter().map(|(n, _)| (*n, *order)));
    }
    offenders.sort_by_key(|(n, _)| n.range().start);
    for (n, order) in offenders {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!(
                "identical playOrder value '{order}' on elements that do not refer to the same target"
            ),
            ncx_path,
            n,
            "ncx.play_order.duplicate",
            vec![order.to_string()],
        );
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

    /// Reports (rule, line) for every finding, so a test can assert *which*
    /// elements were named rather than just how many.
    fn run_at(ncx: &str) -> Vec<(Option<&'static str>, u32)> {
        let mut report = Report::new();
        check(ncx, "toc.ncx", "uid", &mut report);
        report
            .messages
            .iter()
            .map(|m| (m.rule, m.position.map(|p| p.line).unwrap_or(0)))
            .collect()
    }

    const PLAY_ORDER_NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="uid"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Cover</text></navLabel>
      <content src="cover.xhtml"/>
    </navPoint>
    <navPoint id="n2" playOrder="2">
      <navLabel><text>Ch1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
  </navMap>
  <pageList id="pl">
    <navLabel><text>Pages</text></navLabel>
    <pageTarget id="p1" type="normal" value="1" playOrder="1">
      <navLabel><text>1</text></navLabel>
      <content src="chapter1.xhtml#page_1"/>
    </pageTarget>
    <pageTarget id="p2" type="normal" value="2" playOrder="2">
      <navLabel><text>2</text></navLabel>
      <content src="chapter1.xhtml#page_2"/>
    </pageTarget>
  </pageList>
</ncx>"#;

    /// `playOrder` is the reading position, so two elements claiming the same
    /// one while pointing somewhere different is a contradiction. Every
    /// colliding element is named: the defect is the collision, and a reader
    /// handed one line would have to hunt for its partner.
    ///
    /// Reported missing by Doitsu (MobileRead): epubcheck flags four elements
    /// on this shape, epubveri flagged none.
    #[test]
    fn duplicate_play_order_reports_every_colliding_element() {
        let got = run_at(PLAY_ORDER_NCX);
        let dups: Vec<u32> = got
            .iter()
            .filter(|(r, _)| *r == Some("ncx.play_order.duplicate"))
            .map(|(_, line)| *line)
            .collect();
        // The two navPoints and the two pageTargets, in document order.
        assert_eq!(dups, vec![6, 10, 17, 21], "got {got:?}");
    }

    /// Document order, every time. The grouping is keyed by a hash, so
    /// reporting straight out of it reshuffles the findings between
    /// identical runs — which was the first version's actual behaviour.
    #[test]
    fn duplicate_play_order_is_reported_in_a_stable_order() {
        let first = run_at(PLAY_ORDER_NCX);
        for _ in 0..8 {
            assert_eq!(run_at(PLAY_ORDER_NCX), first);
        }
    }

    /// The exception that stops this being a plain duplicate scan: one
    /// position reached by two routes is legitimate, so a shared playOrder
    /// whose elements name the *same* target must stay silent.
    #[test]
    fn same_play_order_pointing_at_the_same_target_is_valid() {
        let ncx = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="uid"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Ch1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
  </navMap>
  <pageList id="pl">
    <navLabel><text>Pages</text></navLabel>
    <pageTarget id="p1" type="normal" value="1" playOrder="1">
      <navLabel><text>1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </pageTarget>
  </pageList>
</ncx>"#;
        assert!(
            !run_at(ncx)
                .iter()
                .any(|(r, _)| *r == Some("ncx.play_order.duplicate")),
            "one position reached by two routes is not a collision"
        );
    }

    /// playOrder is optional; a document that omits it entirely has nothing
    /// to collide.
    #[test]
    fn absent_play_order_is_not_a_collision() {
        let ncx = PLAY_ORDER_NCX
            .replace(" playOrder=\"1\"", "")
            .replace(" playOrder=\"2\"", "");
        assert!(
            !run_at(&ncx)
                .iter()
                .any(|(r, _)| *r == Some("ncx.play_order.duplicate"))
        );
    }

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
