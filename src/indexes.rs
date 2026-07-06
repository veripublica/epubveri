//! EPUB Indexes 1.0 checks (http://idpf.org/epub/idx/). Package-level
//! `<collection role="index"|"index-group">` structure lives here (needs
//! only the OPF tree + manifest, no OCF/content access); content-document
//! detection (`epub:type="index"`) and its content model also live here,
//! but the whole-publication/manifest-property/collection-link
//! cross-referencing that decides *which* documents must have one needs
//! the OPF's own manifest/collection context and is wired from `opf.rs`.

use std::collections::HashSet;

use crate::ids::*;
use crate::report::{Position, Report, Severity};

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

fn has_type_token(n: roxmltree::Node, token: &str) -> bool {
    n.attribute((EPUB_NS, "type"))
        .is_some_and(|t| t.split_whitespace().any(|tok| tok == token))
}

/// Every `epub:type="index"` element in a content document.
pub(crate) fn index_elements<'a>(doc: &'a roxmltree::Document<'a>) -> Vec<roxmltree::Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.is_element() && has_type_token(*n, "index"))
        .collect()
}

/// RSC-005: each `epub:type="index"` element must contain exactly one
/// `epub:type="index-entry-list"` descendant (confirmed via a real
/// fixture with zero, and every "valid" fixture having exactly one).
pub(crate) fn check_content_model(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    for idx in index_elements(doc) {
        let count = idx
            .descendants()
            .filter(|n| n.is_element() && has_type_token(*n, "index-entry-list"))
            .count();
        if count != 1 {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                "An \"index\" must contain one and only one \"index-entry-list\"",
                path,
                Position::of(idx),
            );
        }
    }
}

/// Every resolved path linked (via `<link href>`) from a `<collection
/// role="index">` or `role="index-group">`, recursively - used by the
/// caller (which has the manifest map) to know which content documents
/// must themselves declare `epub:type="index"`.
pub(crate) fn linked_paths(pkg: &roxmltree::Node, base_dir: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    for coll in pkg.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "collection"
            && matches!(n.attribute("role"), Some("index") | Some("index-group"))
    }) {
        for link in coll
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
        {
            if let Some(href) = link.attribute("href") {
                if !crate::opf::is_external(href) {
                    paths.insert(crate::opf::nfc(&crate::opf::resolve(base_dir, href)));
                }
            }
        }
    }
    paths
}

fn check_links_are_xhtml(
    coll: roxmltree::Node,
    items: &std::collections::HashMap<String, (String, String)>,
    base_dir: &str,
    opf_path: &str,
    report: &mut Report,
) {
    for link in coll
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "link")
    {
        let Some(href) = link.attribute("href") else {
            continue;
        };
        if crate::opf::is_external(href) {
            continue;
        }
        let resolved = crate::opf::nfc(&crate::opf::resolve(base_dir, href));
        if let Some((_, mt)) = items.values().find(|(p, _)| crate::opf::nfc(p) == resolved) {
            if mt != "application/xhtml+xml" {
                report.push_at_pos(
                    OPF_071,
                    Severity::Error,
                    "Index collections must only contain resources pointing to XHTML Content Documents",
                    opf_path,
                    Position::of(link),
                );
            }
        }
    }
}

fn check_index_group(
    coll: roxmltree::Node,
    items: &std::collections::HashMap<String, (String, String)>,
    base_dir: &str,
    opf_path: &str,
    report: &mut Report,
) {
    if coll
        .children()
        .any(|n| n.is_element() && n.tag_name().name() == "collection")
    {
        report.push_at_pos(
            RSC_005,
            Severity::Error,
            "An \"index-group\" collection must not have child collections",
            opf_path,
            Position::of(coll),
        );
    }
    check_links_are_xhtml(coll, items, base_dir, opf_path, report);
}

fn check_index_collection(
    coll: roxmltree::Node,
    items: &std::collections::HashMap<String, (String, String)>,
    base_dir: &str,
    opf_path: &str,
    report: &mut Report,
) {
    for sub in coll
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "collection")
    {
        if sub.attribute("role") == Some("index-group") {
            check_index_group(sub, items, base_dir, opf_path, report);
        } else {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                "An \"index\" collection must not have sub-collections other than \"index-group\"",
                opf_path,
                Position::of(sub),
            );
        }
    }
    check_links_are_xhtml(coll, items, base_dir, opf_path, report);
}

/// §2.3.2.2 Multi-File Index(es) and the `collection` element: a top-level
/// `<collection role="index-group">` must be nested inside a `role=
/// "index"` collection (confirmed via a real fixture placing it at the
/// package's own top level instead); an `index` collection may only
/// nest `index-group` sub-collections; an `index-group` may not nest any
/// further sub-collections at all; and every collection's own `<link>`
/// targets must resolve to a real XHTML Content Document manifest item.
pub(crate) fn check_collections(
    pkg: &roxmltree::Node,
    items: &std::collections::HashMap<String, (String, String)>,
    base_dir: &str,
    opf_path: &str,
    report: &mut Report,
) {
    for coll in pkg
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "collection")
    {
        match coll.attribute("role") {
            Some("index-group") => {
                report.push_at_pos(
                    RSC_005,
                    Severity::Error,
                    "An \"index-group\" collection must be a child of an \"index\" collection",
                    opf_path,
                    Position::of(coll),
                );
            }
            Some("index") => check_index_collection(coll, items, base_dir, opf_path, report),
            _ => {}
        }
    }
}
