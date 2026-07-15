//! EPUB Dictionaries & Glossaries 1.0 checks (http://idpf.org/epub/dict/).
//! Content-document-level and Search-Key-Map-document-level rules live
//! here; package-level (dc:type/collection/manifest) cross-referencing
//! needs the OPF's own manifest/container context and lives in `opf.rs`.

use crate::ids::*;
use crate::report::{Report, Severity};
use crate::xmlext::NodeExt;

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

fn has_type_token(n: roxmltree::Node, token: &str) -> bool {
    n.attribute((EPUB_NS, "type"))
        .is_some_and(|t| t.split_whitespace().any(|tok| tok == token))
}

/// True if any element in this content document carries an
/// `epub:type="dictionary"` token - used both to apply the dictionary
/// content model below, and - at the publication level - to detect a
/// dictionary via content even when the package doesn't declare it
/// (OPF-079).
pub(crate) fn has_dictionary_marker(doc: &roxmltree::Document) -> bool {
    doc.descendants()
        .any(|n| n.is_element() && has_type_token(n, "dictionary"))
}

/// Content model for an `epub:type="dictionary"` element: at least one
/// `<article>` child (a real fixture with none at all triggers this), and
/// each such article ("dictionary entry") needs at least one `<dfn>`
/// descendant (a real fixture with an article but no dfn triggers this
/// second, independent finding).
pub(crate) fn check_content_doc(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && has_type_token(*n, "dictionary"))
    {
        let articles: Vec<_> = n
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "article")
            .collect();
        if articles.is_empty() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "A \"dictionary\" must have at least one article child",
                path,
                n,
                "dict.content_document.no_articles",
                Vec::new(),
            );
            continue;
        }
        for article in articles {
            let has_dfn = article
                .descendants()
                .any(|d| d.is_element() && d.tag_name().name() == "dfn");
            if !has_dfn {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "A dictionary entry must have at least one \"dfn\" descendant",
                    path,
                    article,
                    "dict.content_document.article_missing_dfn",
                    Vec::new(),
                );
            }
        }
    }
}

/// A Search Key Map document (`<search-key-map>`) must contain at least
/// one `<search-key-group>` (a real fixture with none at all triggers
/// this). Returns each group's `href` so the caller (which has the
/// manifest/container context this module doesn't) can cross-reference
/// the targets.
pub(crate) fn check_skm(doc: &roxmltree::Document, path: &str, report: &mut Report) -> Vec<String> {
    let root = doc.root_element();
    let groups: Vec<_> = root
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "search-key-group")
        .collect();
    if groups.is_empty() {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"search-key-map\" incomplete; missing required element \"search-key-group\"",
            path,
            root,
            "dict.search_key_map.no_groups",
            Vec::new(),
        );
    }
    groups
        .iter()
        .filter_map(|g| g.attr_no_ns("href"))
        .map(String::from)
        .collect()
}
