//! EPUB Dictionaries & Glossaries 1.0 checks (<http://idpf.org/epub/dict/>).
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

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

fn is_h(n: roxmltree::Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(XHTML_NS)
}

fn is_h_named(n: roxmltree::Node, names: &[&str]) -> bool {
    is_h(n) && names.contains(&n.tag_name().name())
}

/// The first element typed `dictionary` in a content document. Its presence
/// marks the document as dictionary content (OPF-078/079), and it is where
/// epubcheck places OPF-079.
pub(crate) fn first_dictionary_marker<'a>(
    doc: &'a roxmltree::Document<'a>,
) -> Option<roxmltree::Node<'a, 'a>> {
    doc.descendants()
        .find(|n| n.is_element() && has_type_token(*n, "dictionary"))
}

/// RSC-005: the structure EPUB Dictionaries gives its `epub:type` terms
/// (`dict-xhtml.sch`), which epubcheck checks in the XHTML content documents
/// of a dictionary publication or under the dictionary profile - and nowhere
/// else. We used to check the first two rules in every document of every
/// book, so a non-dictionary book with a `section` typed `dictionary` drew an
/// error epubcheck never gives (probed against 5.4.0, 2026-09-25).
///
/// One rule per assertion, compared with epubcheck a book per rule.
pub(crate) fn check_content_doc(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    macro_rules! push {
        ($n:expr, $rule:expr, $text:expr) => {
            report.push_node(
                RSC_005,
                Severity::Error,
                String::from($text),
                path,
                $n,
                $rule,
                Vec::new(),
            )
        };
    }
    for n in doc.descendants().filter(|n| is_h(*n)) {
        if has_type_token(n, "dictionary") {
            if !is_h_named(n, &["body", "section"]) {
                push!(
                    n,
                    "dict.content_document.dictionary_element",
                    "\"dictionary\" is allowed only on \"body\" or \"section\""
                );
            }
            if !n.children().any(|c| is_h_named(c, &["article"])) {
                push!(
                    n,
                    "dict.content_document.no_articles",
                    "A \"dictionary\" must have at least one article child"
                );
            }
        }
        // An entry: an article directly within a dictionary, or anything
        // typed `dictentry`.
        let in_dictionary = n
            .parent()
            .is_some_and(|p| is_h(p) && has_type_token(p, "dictionary"));
        if (is_h_named(n, &["article"]) && in_dictionary) || has_type_token(n, "dictentry") {
            if !is_h_named(n, &["article"]) {
                push!(
                    n,
                    "dict.content_document.entry_element",
                    "\"dictentry\" is allowed only on \"article\""
                );
            }
            // A `dfn` outside any condensed entry - its ancestors checked all
            // the way up, as the Schematron does.
            let has_dfn = n.descendants().any(|d| {
                is_h_named(d, &["dfn"])
                    && !d
                        .ancestors()
                        .skip(1)
                        .any(|a| is_h(a) && has_type_token(a, "condensed-entry"))
            });
            if !has_dfn {
                push!(
                    n,
                    "dict.content_document.article_missing_dfn",
                    "A dictionary entry must have at least one \"dfn\" descendant"
                );
            }
        }
        if has_type_token(n, "condensed-entry") {
            if !is_h_named(n, &["aside"]) {
                push!(
                    n,
                    "dict.content_document.condensed_element",
                    "\"condensed-entry\" is allowed only on \"aside\""
                );
            }
            let placed = n.parent().is_some_and(|p| {
                is_h_named(p, &["article"])
                    && p.parent()
                        .is_some_and(|g| is_h(g) && has_type_token(g, "dictionary"))
            });
            if !placed {
                push!(
                    n,
                    "dict.content_document.condensed_parent",
                    "a \"condensed-entry\" is allowed only directly within a dictionary entry"
                );
            }
            let hidden = n
                .attr_no_ns("hidden")
                .is_some_and(|v| v.is_empty() || v.eq_ignore_ascii_case("hidden"));
            if !hidden {
                push!(
                    n,
                    "dict.content_document.condensed_not_hidden",
                    "a \"condensed-entry\" must be hidden (\"hidden\" set to \"hidden\" or empty)"
                );
            }
        }
        if has_type_token(n, "part-of-speech-list") && !is_h_named(n, &["ol"]) {
            push!(
                n,
                "dict.content_document.pos_list_element",
                "\"part-of-speech-list\" is allowed only on \"ol\""
            );
        }
        let in_pos_list = is_h_named(n, &["li"])
            && n.parent().is_some_and(|p| {
                is_h_named(p, &["ol"]) && has_type_token(p, "part-of-speech-list")
            });
        if has_type_token(n, "part-of-speech-group") || in_pos_list {
            let owned = n.descendants().filter(|d| is_h(*d) && *d != n).any(|d| {
                has_type_token(d, "part-of-speech")
                    && d.ancestors().skip(1).find(|a| {
                        is_h(*a)
                            && (a.attribute((EPUB_NS, "type")).is_some() || is_h_named(*a, &["li"]))
                    }) == Some(n)
            });
            if !owned {
                push!(
                    n,
                    "dict.content_document.pos_group_empty",
                    "a part-of-speech group names no \"part-of-speech\""
                );
            }
        }
        if has_type_token(n, "sense-list") && !is_h_named(n, &["ol"]) {
            push!(
                n,
                "dict.content_document.sense_list_element",
                "\"sense-list\" is allowed only on \"ol\""
            );
        }
        if has_type_token(n, "tran-info") {
            let sibling_tran = n
                .parent()
                .is_some_and(|p| p.children().any(|c| is_h(c) && has_type_token(c, "tran")));
            if !sibling_tran {
                push!(
                    n,
                    "dict.content_document.tran_info_alone",
                    "a \"tran-info\" has no sibling typed \"tran\""
                );
            }
        }
        if has_type_token(n, "idiom") && !is_h_named(n, &["dfn"]) {
            push!(
                n,
                "dict.content_document.idiom_element",
                "\"idiom\" is allowed only on \"dfn\""
            );
        }
        if has_type_token(n, "phrase-list") && !is_h_named(n, &["ol", "ul"]) {
            push!(
                n,
                "dict.content_document.phrase_list_element",
                "\"phrase-list\" is allowed only on \"ol\" or \"ul\""
            );
        }
        let in_phrase_list = is_h_named(n, &["li"])
            && n.parent()
                .is_some_and(|p| is_h(p) && has_type_token(p, "phrase-list"));
        if has_type_token(n, "phrase-group") || in_phrase_list {
            let has = n
                .descendants()
                .skip(1)
                .any(|d| is_h(d) && (has_type_token(d, "example") || has_type_token(d, "idiom")));
            if !has {
                push!(
                    n,
                    "dict.content_document.phrase_group_empty",
                    "a phrase group holds no \"idiom\" or \"example\""
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
pub(crate) fn check_skm(
    doc: &roxmltree::Document,
    path: &str,
    report: &mut Report,
) -> Vec<(String, crate::report::Position)> {
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
        .filter_map(|g| {
            g.attr_no_ns("href")
                .map(|h| (h.to_string(), crate::report::Position::of(*g)))
        })
        .collect()
}

#[cfg(test)]
mod content_rule_tests {
    use crate::report::Report;

    fn rules(body: &str) -> Vec<&'static str> {
        let xml = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>t</title></head><body>{body}</body></html>"#
        );
        let d = crate::ocf::parse_xml(&xml).unwrap();
        let mut report = Report::new();
        super::check_content_doc(&d, "c.xhtml", &mut report);
        let mut v: Vec<_> = report.messages.iter().filter_map(|m| m.rule).collect();
        v.sort();
        v
    }

    /// One case per `dict-xhtml.sch` rule, each compared with epubcheck 5.4.0
    /// on a book of its own (37 dictionary books equal in ids and lines,
    /// 2026-09-25).
    #[test]
    fn dictionary_rules_match_epubcheck() {
        let d = |inner: &str| format!(r#"<section epub:type="dictionary">{inner}</section>"#);
        const E: &str = "<article><dfn>w</dfn></article>";
        assert!(rules(&d(E)).is_empty());
        assert_eq!(
            rules(&format!(r#"<div epub:type="dictionary">{E}</div>"#)),
            ["dict.content_document.dictionary_element"]
        );
        assert_eq!(rules(&d("<p>x</p>")), ["dict.content_document.no_articles"]);
        assert_eq!(
            rules(&d("<article><p>x</p></article>")),
            ["dict.content_document.article_missing_dfn"]
        );
        // A dfn inside the condensed entry does not count.
        assert_eq!(
            rules(&d(
                r#"<article><aside epub:type="condensed-entry" hidden=""><dfn>w</dfn></aside></article>"#
            )),
            ["dict.content_document.article_missing_dfn"]
        );
        assert_eq!(
            rules(&d(
                r#"<article><dfn>w</dfn><aside epub:type="condensed-entry">c</aside></article>"#
            )),
            ["dict.content_document.condensed_not_hidden"]
        );
        assert!(rules(&d(r#"<article><dfn>w</dfn><aside epub:type="condensed-entry" hidden="HIDDEN">c</aside></article>"#)).is_empty());
        assert_eq!(
            rules(&d(
                r#"<article><dfn>w</dfn><ol epub:type="part-of-speech-list"><li><span>n</span></li></ol></article>"#
            )),
            ["dict.content_document.pos_group_empty"]
        );
        assert_eq!(
            rules(&d(
                r#"<article><dfn>w</dfn><p><span epub:type="tran-info">i</span></p></article>"#
            )),
            ["dict.content_document.tran_info_alone"]
        );
        assert_eq!(
            rules(&d(
                r#"<article><dfn>w</dfn><ul epub:type="phrase-list"><li>no</li></ul></article>"#
            )),
            ["dict.content_document.phrase_group_empty"]
        );
    }
}
