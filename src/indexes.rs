//! EPUB Indexes 1.0 checks (<http://idpf.org/epub/idx/>). Package-level
//! `<collection role="index"|"index-group">` structure lives here (needs
//! only the OPF tree + manifest, no OCF/content access); content-document
//! detection (`epub:type="index"`) and its content model also live here,
//! but the whole-publication/manifest-property/collection-link
//! cross-referencing that decides *which* documents must have one needs
//! the OPF's own manifest/collection context and is wired from `opf.rs`.

use std::collections::HashSet;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

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
    check_body_declaration(doc, path, report);
    for idx in index_elements(doc) {
        let count = idx
            .descendants()
            .filter(|n| n.is_element() && has_type_token(*n, "index-entry-list"))
            .count();
        if count != 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "An \"index\" must contain one and only one \"index-entry-list\"",
                path,
                idx,
                "indexes.content_model.wrong_entry_list_count",
                vec![count.to_string()],
            );
        }
    }
}

/// RSC-005: a document holding *nothing but* index content must declare the
/// index on its `<body>` (`idx-xhtml-index.sch`, pattern `index-only`).
///
/// "Nothing but index content" is the Schematron's own test, not a paraphrase:
/// no descendant element has non-blank text of its **own** (`text()`, so
/// direct child text nodes only) unless some ancestor already carries
/// `epub:type="index"`. The corpus fixture leans on both halves - a
/// `<span> </span>` outside the index is whitespace and does not count, while
/// the `<h2>` inside it is excluded by its ancestor.
///
/// **The `epub:type` comparison is exact, and that is deliberate.**
/// epubcheck's assert reads `tokenize(@epub:type,'/s+')='index'` - a forward
/// slash where every other line in the file has a backslash. As a regular
/// expression `/s+` matches a literal "/" followed by "s"es, which occurs in
/// no real attribute, so the value is never split and the whole string is
/// compared. Measured against 5.3.0 with one book per case: `epub:type="index"`
/// passes, absent fails, and **`epub:type="index frontmatter"` fails too**.
/// Tokenizing properly here would be a silent false negative against the tool
/// we are matched against, so the typo is reproduced rather than corrected -
/// switch this to `has_type_token` if epubcheck ever fixes it.
fn check_body_declaration(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    let Some(body) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return;
    };
    let has_non_index_text = body.descendants().filter(|n| n.is_element()).any(|e| {
        let own_text = e
            .children()
            .filter(|c| c.is_text())
            .filter_map(|c| c.text())
            .any(|t| !t.trim().is_empty());
        own_text
            && !e
                .ancestors()
                .skip(1)
                .any(|a| a.is_element() && has_type_token(a, "index"))
    });
    if has_non_index_text {
        return;
    }
    if body.attribute((EPUB_NS, "type")) != Some("index") {
        report.push_node(
            RSC_005,
            Severity::Error,
            "The document contains only index content, so its \"body\" element must have the epub:type \"index\"",
            path,
            body,
            "indexes.content_model.body_not_declared",
            Vec::new(),
        );
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
            && matches!(n.attr_no_ns("role"), Some("index") | Some("index-group"))
    }) {
        for link in coll
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
        {
            if let Some(href) = link.attr_no_ns("href")
                && !crate::opf::is_external(href)
            {
                paths.insert(crate::opf::nfc(&crate::opf::resolve(base_dir, href)));
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
        let Some(href) = link.attr_no_ns("href") else {
            continue;
        };
        if crate::opf::is_external(href) {
            continue;
        }
        let resolved = crate::opf::nfc(&crate::opf::resolve(base_dir, href));
        if let Some((_, mt)) = items.values().find(|(p, _)| crate::opf::nfc(p) == resolved)
            && mt != "application/xhtml+xml"
        {
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
        report.push_node(
            RSC_005,
            Severity::Error,
            "An \"index-group\" collection must not have child collections",
            opf_path,
            coll,
            "indexes.collection.index_group_has_subcollections",
            Vec::new(),
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
        if sub.attr_no_ns("role") == Some("index-group") {
            check_index_group(sub, items, base_dir, opf_path, report);
        } else {
            report.push_node(
                RSC_005,
                Severity::Error,
                "An \"index\" collection must not have sub-collections other than \"index-group\"",
                opf_path,
                sub,
                "indexes.collection.invalid_index_subcollection",
                Vec::new(),
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
        match coll.attr_no_ns("role") {
            Some("index-group") => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "An \"index-group\" collection must be a child of an \"index\" collection",
                    opf_path,
                    coll,
                    "indexes.collection.orphan_index_group",
                    Vec::new(),
                );
            }
            Some("index") => check_index_collection(coll, items, base_dir, opf_path, report),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_findings(body_attrs: &str, body: &str) -> Vec<String> {
        let xml = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\">\
             <head><title>t</title></head><body{body_attrs}>{body}</body></html>"
        );
        let d = crate::ocf::parse_xml(&xml).unwrap();
        let mut report = Report::new();
        check_body_declaration(&d, "c.xhtml", &mut report);
        report.messages.iter().map(|m| m.text.clone()).collect()
    }

    const INDEX: &str = "<div><section epub:type=\"index\"><h2>I</h2>\
         <ul epub:type=\"index-entry-list\"><li>\
         <span epub:type=\"index-term\">t</span></li></ul></section></div>";

    /// Each case was run as a real book through epubcheck 5.3.0 with
    /// `-profile idx`, one book per case.
    #[test]
    fn an_index_only_document_must_declare_the_index_on_body() {
        assert_eq!(body_findings("", INDEX).len(), 1, "undeclared body");
        assert_eq!(
            body_findings(" epub:type=\"index\"", INDEX).len(),
            0,
            "declared body"
        );
        // Whitespace-only text outside the index does not make the document
        // "mixed" - the corpus fixture relies on exactly this.
        assert_eq!(
            body_findings("", &format!("<div><span> </span></div>{INDEX}")).len(),
            1,
            "whitespace outside the index still counts as index-only"
        );
        // Real text outside the index does: the rule no longer applies, so an
        // undeclared body is fine.
        assert_eq!(
            body_findings("", &format!("<p>Preface</p>{INDEX}")).len(),
            0,
            "mixed content is out of scope for this rule"
        );
    }

    /// epubcheck's assert tokenizes on `'/s+'` - a typo for `'\s+'` - so a
    /// multi-token `epub:type` never matches and the document is reported.
    /// Measured, not assumed: `epub:type="index frontmatter"` draws the error
    /// from 5.3.0. Reproduced deliberately; see `check_body_declaration`.
    #[test]
    fn a_multi_token_body_epub_type_is_reported_as_epubcheck_does() {
        assert_eq!(
            body_findings(" epub:type=\"index frontmatter\"", INDEX).len(),
            1
        );
    }
}
