//! EPUB Region-Based Navigation 1.0 checks
//! (`http://idpf.org/epub/renditions/region-nav/`), triggered by a
//! manifest item with `properties="data-nav"` (the "Data Navigation
//! Document"). Plain XML, no new parser needed.

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

/// Walks every `<nav>` in the Data Navigation Document: RSC-005 if any
/// lacks an `epub:type`. Returns the one with `epub:type="region-based"`,
/// if present, for the caller to run `check_content_model`/the NAV-009
/// fixed-layout cross-check on.
pub(crate) fn check_data_nav_doc<'a>(
    d: &'a roxmltree::Document,
    path: &str,
    report: &mut Report,
) -> Option<roxmltree::Node<'a, 'a>> {
    let mut region_based = None;
    for nav in d
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "nav")
    {
        match nav.attribute((EPUB_NS, "type")) {
            None => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "a \"nav\" element in a Data Navigation Document must have an \"epub:type\" attribute",
                    path,
                    nav,
                    "regionnav.data_nav.nav_missing_epub_type",
                    Vec::new(),
                );
            }
            Some("region-based") => region_based = Some(nav),
            Some(_) => {}
        }
    }
    region_based
}

/// HTM-052: a `<nav epub:type="region-based">` found outside the
/// designated Data Navigation Document is misplaced - region-based
/// navigation only belongs there.
pub(crate) fn check_misplaced(d: &roxmltree::Document, path: &str, report: &mut Report) {
    for n in d.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "nav"
            && n.attribute((EPUB_NS, "type")) == Some("region-based")
    }) {
        report.push_at_pos(
            HTM_052,
            Severity::Error,
            "region-based navigation must be defined in the Data Navigation Document, not here",
            path,
            Position::of(n),
        );
    }
}

/// The `href` of every `<a>` inside a region-based nav, in document order
/// - for the caller to resolve and cross-check against fixed-layout
///   status (NAV-009).
pub(crate) fn collect_targets(nav_el: roxmltree::Node) -> Vec<String> {
    nav_el
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
        .filter_map(|n| n.attr_no_ns("href"))
        .map(String::from)
        .collect()
}

/// The region-based nav's content model, `datanav-xhtml.sch`'s four
/// region-based patterns. Each is its own pattern there, so each applies to
/// every element it names anywhere inside the nav - every `li`, every `a`,
/// every `span` - and reports at that element. An earlier version walked the
/// lists instead, so a list item it did not reach, a link inside a `span`,
/// and a `span` holding an extra non-link element all went unasked; and it
/// reported a bad first child at the child, where epubcheck points at the
/// `li` (compared with 5.4.0, 2026-09-25).
pub(crate) fn check_content_model(nav_el: roxmltree::Node, path: &str, report: &mut Report) {
    fn elements<'a, 'i>(n: roxmltree::Node<'a, 'i>) -> Vec<roxmltree::Node<'a, 'i>> {
        n.children().filter(|c| c.is_element()).collect()
    }
    let top = elements(nav_el);
    if !(top.len() == 1 && top[0].tag_name().name() == "ol") {
        report.push_node(
            RSC_005,
            Severity::Error,
            "a region-based nav element must contain exactly one child ol element",
            path,
            nav_el,
            "regionnav.content_model.expected_single_ol",
            Vec::new(),
        );
    }
    for n in nav_el
        .descendants()
        .filter(|n| n.is_element() && *n != nav_el)
    {
        match n.tag_name().name() {
            "li" => {
                let children = elements(n);
                let first_ok = children
                    .first()
                    .is_some_and(|f| matches!(f.tag_name().name(), "a" | "span"));
                if !first_ok {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "the first child of a region-based nav list item must be either an \"a\" or \"span\" element",
                        path,
                        n,
                        "regionnav.li.missing_label",
                        Vec::new(),
                    );
                }
                if children.len() > 1
                    && !(children.len() == 2 && children[1].tag_name().name() == "ol")
                {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "the first child of a region-based nav list item can only be followed by a single \"ol\" element",
                        path,
                        n,
                        "regionnav.li.a_followed_by_invalid_sibling",
                        Vec::new(),
                    );
                }
            }
            "a" => check_a_label(n, path, report),
            "span" => {
                let children = elements(n);
                let links = children
                    .iter()
                    .filter(|c| c.tag_name().name() == "a")
                    .count();
                if !(children.len() == 2 && links == 2) {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "\"span\" elements in region-based navs must contain exactly two \"a\" elements",
                        path,
                        n,
                        "regionnav.li.span_wrong_anchor_count",
                        vec![links.to_string()],
                    );
                }
            }
            _ => {}
        }
    }
}

fn check_a_label(a: roxmltree::Node, path: &str, report: &mut Report) {
    let has_text = a
        .descendants()
        .filter(|n| n.is_text())
        .any(|n| n.text().is_some_and(|t| !t.trim().is_empty()));
    if has_text {
        report.push_node(
            RSC_017,
            Severity::Warning,
            "\"a\" elements in region-based navs should not contain text labels",
            path,
            a,
            "regionnav.li.anchor_has_text_label",
            Vec::new(),
        );
    }
}
