//! EPUB Region-Based Navigation 1.0 checks
//! (`http://idpf.org/epub/renditions/region-nav/`), triggered by a
//! manifest item with `properties="data-nav"` (the "Data Navigation
//! Document"). Plain XML, no new parser needed.

use crate::ids::*;
use crate::report::{Position, Report, Severity};

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
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "a \"nav\" element in a Data Navigation Document must have an \"epub:type\" attribute",
                    path,
                    Position::of(nav),
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
/// status (NAV-009).
pub(crate) fn collect_targets(nav_el: roxmltree::Node) -> Vec<String> {
    nav_el
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
        .filter_map(|n| n.attribute("href"))
        .map(String::from)
        .collect()
}

/// The region-based nav's own content model, reverse-engineered from the
/// real corpus fixture (see CLAUDE.md's dated notes for the derivation):
/// the `<nav>` must contain exactly one element child, an `<ol>`; each
/// `<li>`'s first element child must be `<a>` or `<span>`; a `<span>` must
/// contain exactly two `<a>` elements; an `<a>` may be followed by at most
/// one more child, which must be an `<ol>` (nested sub-regions); an `<a>`
/// containing actual text content (not just e.g. a `<meta>` annotation) is
/// a warning, not an error.
pub(crate) fn check_content_model(nav_el: roxmltree::Node, path: &str, report: &mut Report) {
    let element_children: Vec<_> = nav_el.children().filter(|n| n.is_element()).collect();
    if element_children.len() != 1 || element_children[0].tag_name().name() != "ol" {
        report.push_full(
            RSC_005,
            Severity::Error,
            "a region-based nav element must contain exactly one child ol element",
            path,
            Position::of(nav_el),
            "regionnav.content_model.expected_single_ol",
            Vec::new(),
        );
    }
    // Still walk whatever <ol> is present (even alongside other, already-
    // flagged stray children) - confirmed against the real corpus fixture,
    // which reports both the container-level violation *and* every
    // violation found inside the ol, not one or the other.
    if let Some(ol) = element_children
        .iter()
        .find(|n| n.tag_name().name() == "ol")
    {
        check_ol(*ol, path, report);
    }
}

fn check_ol(ol: roxmltree::Node, path: &str, report: &mut Report) {
    for li in ol
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "li")
    {
        check_li(li, path, report);
    }
}

fn check_li(li: roxmltree::Node, path: &str, report: &mut Report) {
    let children: Vec<_> = li.children().filter(|n| n.is_element()).collect();
    let Some(first) = children.first() else {
        report.push_full(
            RSC_005,
            Severity::Error,
            "the first child of a region-based nav list item must be either an \"a\" or \"span\" element",
            path,
            Position::of(li),
            "regionnav.li.missing_label",
            Vec::new(),
        );
        return;
    };
    match first.tag_name().name() {
        "a" => {
            check_a_label(*first, path, report);
            if children.len() > 1 {
                if children.len() != 2 || children[1].tag_name().name() != "ol" {
                    report.push_full(
                        RSC_005,
                        Severity::Error,
                        "the first child of a region-based nav list item can only be followed by a single \"ol\" element",
                        path,
                        Position::of(li),
                        "regionnav.li.a_followed_by_invalid_sibling",
                        Vec::new(),
                    );
                } else {
                    check_ol(children[1], path, report);
                }
            }
        }
        "span" => {
            let a_count = first
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "a")
                .count();
            if a_count != 2 {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "\"span\" elements in region-based navs must contain exactly two \"a\" elements",
                    path,
                    Position::of(*first),
                    "regionnav.li.span_wrong_anchor_count",
                    vec![a_count.to_string()],
                );
            }
        }
        _ => {
            report.push_full(
                RSC_005,
                Severity::Error,
                "the first child of a region-based nav list item must be either an \"a\" or \"span\" element",
                path,
                Position::of(*first),
                "regionnav.li.missing_label",
                Vec::new(),
            );
        }
    }
}

fn check_a_label(a: roxmltree::Node, path: &str, report: &mut Report) {
    let has_text = a
        .descendants()
        .filter(|n| n.is_text())
        .any(|n| n.text().is_some_and(|t| !t.trim().is_empty()));
    if has_text {
        report.push_at_pos(
            RSC_017,
            Severity::Warning,
            "\"a\" elements in region-based navs should not contain text labels",
            path,
            Position::of(a),
        );
    }
}
