//! EPUB 3 §7 Navigation Document content model: the four defined `nav`
//! types (`toc`/`page-list`/`landmarks`, plus any other named type) each
//! restrict their own content - an optional heading, then a required
//! `<ol>` of `<li>`s whose own content model is `(a|span), ol?`. A `<nav>`
//! with no `epub:type` at all is completely unrestricted (confirmed via a
//! real fixture using arbitrary markup in one).

use std::collections::HashMap;

use crate::ids::*;
use crate::report::{Position, Report, Severity};

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

fn nav_type<'a>(nav: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    nav.attribute((EPUB_NS, "type"))
}

/// A label (`<a>`/`<span>`) has real content if it has non-whitespace text
/// anywhere inside it, or an `<img>` descendant (confirmed via a real
/// fixture: two `<img>` elements with no text at all, one even with an
/// empty `alt`, are still a valid non-empty label).
fn has_text_or_image(n: roxmltree::Node) -> bool {
    let has_text = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .any(|t| !t.trim().is_empty());
    has_text
        || n.descendants()
            .any(|d| d.is_element() && d.tag_name().name() == "img")
}

/// `hidden` is an HTML5 boolean attribute - only an empty value or the
/// literal string "hidden" are conforming.
fn check_hidden_attrs(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(v) = n.attribute("hidden") {
            if !matches!(v, "" | "hidden") {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "value of attribute \"hidden\" is invalid",
                    path,
                    Position::of(n),
                    "navdoc.hidden_attribute.invalid_value",
                    vec![v.to_string()],
                );
            }
        }
    }
}

/// One `<li>`'s content model: its first element child must be `<a>` or
/// `<span>` (the "label"); anything else (e.g. a bare nested `<ol>`) is
/// "not allowed yet". A `<span>` label has no link of its own, so a
/// nested `<ol>` sub-navigation is *required* right after it; an `<a>`
/// label may optionally be followed by one. `page-list`/`landmarks`
/// specifically don't allow nested sublists at all (a warning, not a
/// content-model error - confirmed via a real fixture using otherwise-
/// correct ordering that still gets flagged).
fn check_li(li: roxmltree::Node, ty: &str, path: &str, report: &mut Report) {
    let children: Vec<_> = li.children().filter(|c| c.is_element()).collect();
    let Some(label) = children.first() else {
        return;
    };
    let label_name = label.tag_name().name();
    if !matches!(label_name, "a" | "span") {
        report.push_full(
            RSC_005,
            Severity::Error,
            format!("element \"{label_name}\" not allowed yet; expected element \"a\" or \"span\""),
            path,
            Position::of(*label),
            "navdoc.li.invalid_label",
            vec![label_name.to_string()],
        );
        return;
    }
    let nested_ol = children.get(1).filter(|c| c.tag_name().name() == "ol");
    if label_name == "span" && nested_ol.is_none() {
        report.push_full(
            RSC_005,
            Severity::Error,
            "element \"li\" incomplete; missing required element \"ol\"",
            path,
            Position::of(li),
            "navdoc.li.span_missing_ol",
            Vec::new(),
        );
    }
    if let Some(ol) = nested_ol {
        if matches!(ty, "page-list" | "landmarks") {
            report.push_full(
                RSC_017,
                Severity::Warning,
                format!("the \"{ty}\" nav must have no nested sublists"),
                path,
                Position::of(*ol),
                "navdoc.nav.nested_sublist_not_allowed",
                vec![ty.to_string()],
            );
        }
        check_ol(*ol, ty, path, report);
    }
}

/// An `<ol>` (top-level or nested) must have at least one `<li>`.
fn check_ol(ol: roxmltree::Node, ty: &str, path: &str, report: &mut Report) {
    let lis: Vec<_> = ol
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "li")
        .collect();
    if lis.is_empty() {
        report.push_full(
            RSC_005,
            Severity::Error,
            "element \"ol\" incomplete",
            path,
            Position::of(ol),
            "navdoc.ol.empty",
            Vec::new(),
        );
        return;
    }
    for li in lis {
        check_li(li, ty, path, report);
    }
}

/// A restricted `<nav>`'s own children: `[heading]? <ol>`. `toc`/`page-
/// list`/`landmarks` don't require a heading; any *other* named type does
/// (confirmed via a real fixture pair - the same "lot" nav valid with a
/// heading, invalid without one).
fn check_nav_content_model(nav: roxmltree::Node, ty: &str, path: &str, report: &mut Report) {
    let children: Vec<_> = nav.children().filter(|c| c.is_element()).collect();
    let mut idx = 0;
    let mut heading = None;
    if let Some(first) = children.first() {
        if matches!(
            first.tag_name().name(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        ) {
            heading = Some(*first);
            idx = 1;
        }
    }
    match heading {
        Some(h) => {
            let text: String = h
                .descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .collect();
            if text.trim().is_empty() {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "Heading elements must contain text",
                    path,
                    Position::of(h),
                    "navdoc.heading.empty_text",
                    Vec::new(),
                );
            }
        }
        None if !matches!(ty, "toc" | "page-list" | "landmarks") => {
            report.push_full(
                RSC_005,
                Severity::Error,
                format!("the \"{ty}\" nav must have a heading"),
                path,
                Position::of(nav),
                "navdoc.nav.missing_heading",
                vec![ty.to_string()],
            );
        }
        None => {}
    }
    let Some(ol) = children.get(idx) else { return };
    if ol.tag_name().name() != "ol" {
        report.push_full(
            RSC_005,
            Severity::Error,
            format!("element \"{}\" not allowed here", ol.tag_name().name()),
            path,
            Position::of(*ol),
            "navdoc.nav.expected_ol",
            vec![ol.tag_name().name().to_string()],
        );
        return;
    }
    check_ol(*ol, ty, path, report);
}

/// RSC-010: a `toc` nav's link must target a real Content Document -
/// confirmed via a real fixture linking to a plain image instead.
fn check_toc_links(
    nav: roxmltree::Node,
    dir: &str,
    items: &HashMap<String, (String, String)>,
    path: &str,
    report: &mut Report,
) {
    for a in nav
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
    {
        let Some(href) = a.attribute("href") else {
            continue;
        };
        if crate::opf::is_external(href) {
            continue;
        }
        let path_part = href.split(['#', '?']).next().unwrap_or(href);
        let resolved = crate::opf::nfc(&crate::opf::resolve(dir, path_part));
        if let Some((_, mt)) = items.values().find(|(p, _)| crate::opf::nfc(p) == resolved) {
            if mt != "application/xhtml+xml" && mt != "image/svg+xml" {
                report.push_full(
                    RSC_010,
                    Severity::Error,
                    format!("toc nav link '{href}' does not target a Content Document"),
                    path,
                    Position::of(a),
                    "navdoc.toc.link_not_content_document",
                    vec![href.to_string()],
                );
            }
        }
    }
}

/// `landmarks`-specific rules: every entry needs an `epub:type` (reported
/// once per missing occurrence), and no two entries may share both an
/// `epub:type` token and their target resource (reported once per
/// offending entry, confirmed via a real 2-entry-collision fixture
/// expecting exactly 2 findings) - entries with the same type but
/// *different* targets are explicitly valid.
fn check_landmarks(nav: roxmltree::Node, dir: &str, path: &str, report: &mut Report) {
    let mut entries: Vec<(Vec<&str>, String, roxmltree::Node)> = Vec::new();
    for a in nav
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
    {
        match a.attribute((EPUB_NS, "type")) {
            None => {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "Missing epub:type attribute on anchor inside \"landmarks\" nav",
                    path,
                    Position::of(a),
                    "navdoc.landmarks.missing_epub_type",
                    Vec::new(),
                );
            }
            Some(types) => {
                if let Some(href) = a.attribute("href") {
                    let (path_part, frag) = match href.split_once('#') {
                        Some((p, f)) => (p, Some(f)),
                        None => (href, None),
                    };
                    let resolved = if crate::opf::is_external(path_part) {
                        path_part.to_string()
                    } else {
                        crate::opf::nfc(&crate::opf::resolve(dir, path_part))
                    };
                    let key = match frag {
                        Some(f) => format!("{resolved}#{f}"),
                        None => resolved,
                    };
                    entries.push((types.split_whitespace().collect(), key, a));
                }
            }
        }
    }
    let mut reported = vec![false; entries.len()];
    for i in 0..entries.len() {
        for j in 0..entries.len() {
            if i == j || reported[i] {
                continue;
            }
            let (types_i, key_i, node_i) = &entries[i];
            let (types_j, key_j, _) = &entries[j];
            if key_i == key_j && types_i.iter().any(|t| types_j.contains(t)) {
                reported[i] = true;
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "Another landmark was found with the same epub:type and same reference",
                    path,
                    Position::of(*node_i),
                    "navdoc.landmarks.duplicate_entry",
                    Vec::new(),
                );
            }
        }
    }
}

/// Entry point, called once for the actual nav document. `items` is the
/// manifest's id -> (resolved path, media-type) map, needed for RSC-010.
pub(crate) fn check(
    doc: &roxmltree::Document,
    path: &str,
    dir: &str,
    items: &HashMap<String, (String, String)>,
    report: &mut Report,
) {
    check_hidden_attrs(doc, path, report);

    let navs: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "nav")
        .collect();

    if !navs.iter().any(|n| nav_type(*n) == Some("toc")) {
        report.push_full(
            RSC_005,
            Severity::Error,
            "the nav document has no \"toc\" nav",
            path,
            Position::of(doc.root_element()),
            "navdoc.document.missing_toc",
            Vec::new(),
        );
    }
    let page_lists: Vec<_> = navs
        .iter()
        .filter(|n| nav_type(**n) == Some("page-list"))
        .collect();
    if let Some(second) = page_lists.get(1) {
        report.push_full(
            RSC_005,
            Severity::Error,
            "Multiple occurrences of the \"page-list\" nav element",
            path,
            Position::of(**second),
            "navdoc.document.multiple_page_list",
            Vec::new(),
        );
    }
    let landmarks: Vec<_> = navs
        .iter()
        .filter(|n| nav_type(**n) == Some("landmarks"))
        .collect();
    if let Some(second) = landmarks.get(1) {
        report.push_full(
            RSC_005,
            Severity::Error,
            "Multiple occurrences of the \"landmarks\" nav element",
            path,
            Position::of(**second),
            "navdoc.document.multiple_landmarks",
            Vec::new(),
        );
    }

    for nav in navs {
        let Some(ty) = nav_type(nav) else {
            continue; // no epub:type at all - unrestricted content model
        };
        check_nav_content_model(nav, ty, path, report);
        for a in nav
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "a")
        {
            if !has_text_or_image(a) {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "Anchors within nav elements must contain text",
                    path,
                    Position::of(a),
                    "navdoc.label.empty_anchor",
                    Vec::new(),
                );
            }
        }
        for span in nav
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "span")
        {
            if !has_text_or_image(span) {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "Spans within nav elements must contain text",
                    path,
                    Position::of(span),
                    "navdoc.label.empty_span",
                    Vec::new(),
                );
            }
        }
        if ty == "toc" {
            check_toc_links(nav, dir, items, path, report);
        }
        if ty == "landmarks" {
            check_landmarks(nav, dir, path, report);
        }
    }
}
