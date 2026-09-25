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

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

/// An element in the XHTML namespace: what `h:*` selects.
fn is_h(n: roxmltree::Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(XHTML_NS)
}

fn is_h_named(n: roxmltree::Node, names: &[&str]) -> bool {
    is_h(n) && names.contains(&n.tag_name().name())
}

fn has_any_type(n: roxmltree::Node, tokens: &[&str]) -> bool {
    tokens.iter().any(|t| has_type_token(n, t))
}

fn has_type_attr(n: roxmltree::Node) -> bool {
    n.attribute((EPUB_NS, "type")).is_some()
}

/// The nearest XHTML ancestor (not the node itself) that `pick` accepts:
/// `ancestor::h:*[pick][1]`.
fn nearest_ancestor<'a>(
    n: roxmltree::Node<'a, 'a>,
    pick: impl Fn(roxmltree::Node<'a, 'a>) -> bool,
) -> Option<roxmltree::Node<'a, 'a>> {
    n.ancestors().skip(1).find(|a| is_h(*a) && pick(*a))
}

fn any_ancestor(n: roxmltree::Node, pick: impl Fn(roxmltree::Node) -> bool) -> bool {
    n.ancestors().skip(1).any(|a| is_h(a) && pick(a))
}

/// A `<ul>` that is an entry list, said or implied: typed
/// `index-entry-list`, or untyped inside an `index` and outside any
/// `index-headnotes`. The Schematron spells this out in five places; it is
/// one predicate.
fn is_entry_list_ul(ul: roxmltree::Node) -> bool {
    is_h_named(ul, &["ul"])
        && (has_type_token(ul, "index-entry-list")
            || (!has_type_attr(ul)
                && any_ancestor(ul, |a| has_type_token(a, "index"))
                && !any_ancestor(ul, |a| has_type_token(a, "index-headnotes"))))
}

/// An `<li>` of an entry list: an entry, said or implied.
fn is_implied_entry_li(li: roxmltree::Node) -> bool {
    is_h_named(li, &["li"]) && li.parent().is_some_and(is_entry_list_ul)
}

/// The descendants an element *owns*: those whose nearest ancestor matching
/// `stop` is this element. With `stop` = "has an `epub:type` or is a `<ul>`"
/// this is `idx-xhtml.sch`'s `$semchilds`, and it is not "descendants": an
/// index built from two `index-group`s owns no entry list, because each list
/// belongs to its group.
fn owned_descendants<'a>(
    el: roxmltree::Node<'a, 'a>,
    stop: impl Fn(roxmltree::Node<'a, 'a>) -> bool + Copy,
) -> Vec<roxmltree::Node<'a, 'a>> {
    el.descendants()
        .filter(|n| is_h(*n) && *n != el)
        .filter(|n| nearest_ancestor(*n, stop) == Some(el))
        .collect()
}

const SECTIONING: &[&str] = &["section", "article", "aside", "nav"];
const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
/// The elements `index-term` may sit on: HTML's phrasing content, as
/// epubcheck lists it.
const PHRASING: &[&str] = &[
    "a", "abbr", "area", "audio", "b", "bdi", "bdo", "br", "button", "canvas", "cite", "code",
    "data", "datalist", "del", "dfn", "em", "embed", "i", "iframe", "img", "input", "ins", "kbd",
    "label", "map", "mark", "math", "meter", "noscript", "object", "output", "progress", "q",
    "ruby", "s", "samp", "script", "select", "small", "span", "strong", "sub", "sup", "svg",
    "textarea", "time", "u", "var", "video",
];

/// The same, where only an *implied* entry (`<li>`) counts - the shape the
/// `index-term`, `index-locator` and `index-locator-range` rules use.
fn under_implied_entry_li(n: roxmltree::Node) -> bool {
    nearest_ancestor(n, |a| is_h_named(a, &["li"]) || has_type_attr(a))
        .is_some_and(is_implied_entry_li)
}

/// RSC-005: the structure EPUB Indexes gives its `epub:type` terms, which
/// epubcheck checks with `idx-xhtml.sch` in every XHTML content document of
/// an index publication or an index-declared document (and, per its
/// `OPSChecker`, of an EDUPUB publication).
///
/// Written from the specification's structure and epubcheck's observable
/// behaviour, one rule per assertion, and checked against epubcheck 5.4.0 a
/// book per rule. Two of its quirks are reproduced because the verdict
/// depends on them: an `index-group`'s entry-list test counts every owned
/// `<ul>` whatever its type (it tests an `epub-type` attribute, which never
/// exists), and an index's own entry list may be a bare `<ul>`.
pub(crate) fn check_index_rules(doc: &roxmltree::Document, path: &str, report: &mut Report) {
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
    let typed_or_ul = |a: roxmltree::Node| has_type_attr(a) || is_h_named(a, &["ul"]);
    for n in doc.descendants().filter(|n| is_h(*n)) {
        // index
        if has_type_token(n, "index") {
            if !is_h_named(n, &["body"]) && !is_h_named(n, SECTIONING) {
                push!(
                    n,
                    "indexes.index.element",
                    "\"index\" is allowed only on \"body\" or a sectioning element"
                );
            }
            let sem = owned_descendants(n, typed_or_ul);
            if sem.iter().filter(|c| is_h_named(**c, HEADINGS)).count() > 1 {
                push!(
                    n,
                    "indexes.index.headings",
                    "an \"index\" has more than one heading of its own"
                );
            }
            if sem
                .iter()
                .filter(|c| has_type_token(**c, "index-headnotes"))
                .count()
                > 1
            {
                push!(
                    n,
                    "indexes.index.headnotes",
                    "an \"index\" has more than one \"index-headnotes\""
                );
            }
            let groups = sem
                .iter()
                .filter(|c| has_type_token(**c, "index-group"))
                .count();
            let lists = sem
                .iter()
                .filter(|c| is_h_named(**c, &["ul"]) || has_type_token(**c, "index-entry-list"))
                .count();
            if !(if groups > 0 { lists == 0 } else { lists == 1 }) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "An \"index\" must contain one and only one \"index-entry-list\" \
                     (possibly implied) or one or more \"index-group\"s",
                    path,
                    n,
                    "indexes.content_model.wrong_entry_list_count",
                    vec![lists.to_string(), groups.to_string()],
                );
            }
        }
        // index-headnotes
        if has_type_token(n, "index-headnotes") {
            if !is_h_named(n, &["header"]) && !is_h_named(n, SECTIONING) {
                push!(
                    n,
                    "indexes.headnotes.element",
                    "\"index-headnotes\" is allowed only on \"header\" or a sectioning element"
                );
            }
            if !nearest_ancestor(n, has_type_attr).is_some_and(|a| has_type_token(a, "index")) {
                push!(
                    n,
                    "indexes.headnotes.parent",
                    "\"index-headnotes\" is allowed only directly within an \"index\""
                );
            }
        }
        // index-legend
        if has_type_token(n, "index-legend") {
            if !is_h_named(n, &["dl"]) && !is_h_named(n, SECTIONING) {
                push!(
                    n,
                    "indexes.legend.element",
                    "\"index-legend\" is allowed only on \"dl\" or a sectioning element"
                );
            }
            if !nearest_ancestor(n, has_type_attr)
                .is_some_and(|a| has_type_token(a, "index-headnotes"))
            {
                push!(
                    n,
                    "indexes.legend.parent",
                    "\"index-legend\" is allowed only directly within an \"index-headnotes\""
                );
            }
        }
        // index-group
        if has_type_token(n, "index-group") {
            if !is_h_named(n, SECTIONING) {
                push!(
                    n,
                    "indexes.group.element",
                    "\"index-group\" is allowed only on a sectioning element"
                );
            }
            if !nearest_ancestor(n, has_type_attr).is_some_and(|a| has_type_token(a, "index")) {
                push!(
                    n,
                    "indexes.group.parent",
                    "\"index-group\" is allowed only directly within an \"index\""
                );
            }
            let own = owned_descendants(n, typed_or_ul);
            if own.iter().filter(|c| is_h_named(**c, HEADINGS)).count() > 1 {
                push!(
                    n,
                    "indexes.group.headings",
                    "an \"index-group\" has more than one heading of its own"
                );
            }
            if own.iter().filter(|c| is_h_named(**c, &["ul"])).count() != 1 {
                push!(
                    n,
                    "indexes.group.entry_list",
                    "an \"index-group\" must hold exactly one entry list"
                );
            }
        }
        // index-entry-list, said or implied
        if has_type_token(n, "index-entry-list") || (is_h_named(n, &["ul"]) && is_entry_list_ul(n))
        {
            if !is_h_named(n, &["ul"]) {
                push!(
                    n,
                    "indexes.entry_list.element",
                    "\"index-entry-list\" is allowed only on \"ul\""
                );
            }
            let placed = nearest_ancestor(n, has_type_attr)
                .is_some_and(|a| has_any_type(a, &["index", "index-group", "index-entry"]))
                || any_ancestor(n, is_implied_entry_li);
            if !placed {
                push!(
                    n,
                    "indexes.entry_list.parent",
                    "an entry list is allowed only within an \"index\", an \"index-group\" or an \"index-entry\""
                );
            }
            if !n.children().any(|c| is_h_named(c, &["li"])) {
                push!(
                    n,
                    "indexes.entry_list.empty",
                    "an entry list has no entries"
                );
            }
        }
        // index-entry, said or implied
        if has_type_token(n, "index-entry") || is_implied_entry_li(n) {
            if !is_h_named(n, &["li"]) {
                push!(
                    n,
                    "indexes.entry.element",
                    "\"index-entry\" is allowed only on \"li\""
                );
            }
            if !n.parent().is_some_and(is_entry_list_ul) {
                push!(
                    n,
                    "indexes.entry.parent",
                    "an entry is allowed only directly within an entry list"
                );
            }
            let own = owned_descendants(n, |a| has_type_attr(a) || is_h_named(a, &["li"]));
            let count =
                |pick: &dyn Fn(roxmltree::Node) -> bool| own.iter().filter(|c| pick(**c)).count();
            let terms = count(&|c| {
                has_type_token(c, "index-term")
                    && !has_any_type(c, &["index-xref-related", "index-xref-preferred"])
            });
            if terms != 1 {
                push!(
                    n,
                    "indexes.entry.term",
                    format!("an entry must have exactly one \"index-term\", not {terms}")
                );
            }
            let bare_ul = |c: roxmltree::Node| is_h_named(c, &["ul"]) && !has_type_attr(c);
            let targets = count(&|c| {
                bare_ul(c)
                    || has_any_type(
                        c,
                        &[
                            "index-entry-list",
                            "index-editor-note",
                            "index-locator-list",
                            "index-locator",
                            "index-locator-range",
                            "index-xref-preferred",
                            "index-xref-related",
                        ],
                    )
            });
            if targets == 0 {
                push!(
                    n,
                    "indexes.entry.no_target",
                    "an entry leads nowhere: it has no sub-entries, locator, cross-reference or editor note"
                );
            }
            if count(&|c| bare_ul(c) || has_type_token(c, "index-entry-list")) > 1 {
                push!(
                    n,
                    "indexes.entry.entry_lists",
                    "an entry has more than one list of sub-entries"
                );
            }
            let locator_lists = count(&|c| has_type_token(c, "index-locator-list"));
            if locator_lists > 0
                && count(&|c| has_any_type(c, &["index-locator", "index-locator-range"])) > 0
            {
                push!(
                    n,
                    "indexes.entry.locators_mixed",
                    "an entry has both an \"index-locator-list\" and loose locators"
                );
            }
            if locator_lists > 1 {
                push!(
                    n,
                    "indexes.entry.locator_lists",
                    "an entry has more than one \"index-locator-list\""
                );
            }
            if count(&|c| has_type_token(c, "index-editor-note")) > 1 {
                push!(
                    n,
                    "indexes.entry.editor_notes",
                    "an entry has more than one \"index-editor-note\""
                );
            }
            if count(&|c| has_type_token(c, "index-xref-preferred")) > 0
                && count(&|c| has_type_token(c, "index-xref-related")) > 0
            {
                push!(
                    n,
                    "indexes.entry.xrefs_mixed",
                    "an entry has both an \"index-xref-preferred\" and an \"index-xref-related\""
                );
            }
        }
        // Not checked, on purpose: that `index-editor-note`,
        // `index-locator-list` and the two cross-reference types sit within an
        // entry. epubcheck declares that rule once, as an abstract pattern
        // instantiated per type, and it never fires - each of the four
        // outside an entry is silent there (probed against 5.4.0,
        // 2026-09-25). Checking it would report what epubcheck does not.
        // index-term
        if has_type_token(n, "index-term") {
            if !is_h_named(n, PHRASING) {
                push!(
                    n,
                    "indexes.term.element",
                    "\"index-term\" is allowed only on a phrasing element"
                );
            }
            let placed = n.parent().is_some_and(|p| {
                is_h(p)
                    && has_any_type(
                        p,
                        &["index-entry", "index-xref-related", "index-xref-preferred"],
                    )
            }) || under_implied_entry_li(n);
            if !placed {
                push!(
                    n,
                    "indexes.term.parent",
                    "\"index-term\" is allowed only within an entry or a cross-reference"
                );
            }
        }
        // index-locator-list
        if has_type_token(n, "index-locator-list") {
            if !is_h_named(n, &["ul"]) {
                push!(
                    n,
                    "indexes.locator_list.element",
                    "\"index-locator-list\" is allowed only on \"ul\""
                );
            }
            let has_locator = n.descendants().any(|d| {
                is_h(d)
                    && d != n
                    && (has_type_token(d, "index-locator")
                        || (is_h_named(d, &["a"]) && !has_type_attr(d)))
            });
            if !has_locator {
                push!(
                    n,
                    "indexes.locator_list.empty",
                    "an \"index-locator-list\" holds no locator"
                );
            }
        }
        // index-locator, said or implied
        let in_locator_group = any_ancestor(n, |a| {
            has_any_type(a, &["index-locator-list", "index-locator-range"])
        });
        if has_type_token(n, "index-locator") || (is_h_named(n, &["a"]) && in_locator_group) {
            if !is_h_named(n, &["a"]) {
                push!(
                    n,
                    "indexes.locator.element",
                    "\"index-locator\" is allowed only on \"a\""
                );
            }
            if !(in_locator_group || under_implied_entry_li(n)) {
                push!(
                    n,
                    "indexes.locator.parent",
                    "\"index-locator\" is allowed only within an entry, an \"index-locator-list\" or an \"index-locator-range\""
                );
            }
        }
        // index-locator-range
        if has_type_token(n, "index-locator-range") {
            if !(any_ancestor(n, |a| has_type_token(a, "index-locator-list"))
                || under_implied_entry_li(n))
            {
                push!(
                    n,
                    "indexes.locator_range.parent",
                    "\"index-locator-range\" is allowed only within an entry or an \"index-locator-list\""
                );
            }
            let links = n
                .descendants()
                .filter(|d| *d != n && is_h_named(*d, &["a"]))
                .count();
            if !(1..=2).contains(&links) {
                push!(
                    n,
                    "indexes.locator_range.links",
                    format!("an \"index-locator-range\" must hold one or two links, not {links}")
                );
            }
        }
        // index-xref-preferred / -related
        if has_any_type(n, &["index-xref-preferred", "index-xref-related"]) {
            let has_term = n
                .descendants()
                .any(|d| is_h(d) && has_any_type(d, &["index-term", "index-term-category"]));
            if !has_term {
                push!(
                    n,
                    "indexes.xref.term",
                    "a cross-reference names no \"index-term\" or \"index-term-category\""
                );
            }
        }
        // index-term-category
        if has_type_token(n, "index-term-category") {
            if !is_h_named(n, &["a"]) {
                push!(
                    n,
                    "indexes.term_category.element",
                    "\"index-term-category\" is allowed only on \"a\""
                );
            }
            let within = n.ancestors().any(|a| {
                is_h(a) && has_any_type(a, &["index-xref-related", "index-xref-preferred"])
            });
            if !within {
                push!(
                    n,
                    "indexes.term_category.parent",
                    "\"index-term-category\" is allowed only within a cross-reference"
                );
            }
        }
    }
}

/// RSC-005: an `epub:type="index"` holds **either** exactly one entry list
/// **or** one or more `index-group`s, and not both.
///
/// `idx-xhtml.sch`, pattern `index`:
///
/// ```text
/// if ($semchilds[tokenize(@epub:type,'\s+')='index-group'])
/// then empty($semchilds[self::h:ul or tokenize(@epub:type,'\s+')='index-entry-list'])
/// else count($semchilds[self::h:ul or tokenize(@epub:type,'\s+')='index-entry-list'])=1
/// ```
///
/// This used to be "exactly one `index-entry-list` descendant", which was
/// wrong three ways and right by accident in a fourth. Measured against
/// epubcheck 5.3.0, one book per shape, with `properties="index"` on the item
/// so its index Schematron actually runs:
///
/// | index holds | epubcheck | old rule |
/// |---|---|---|
/// | one entry list | valid | valid |
/// | one `index-group` | valid | valid *by accident* - one list, one level down |
/// | **two `index-group`s** | valid | **error** |
/// | **a bare `<ul>`** | valid | **error** |
/// | a group *and* a list | error | error |
/// | nothing | error | error |
///
/// The two errors are the false positives; the accident is why one group
/// never showed up. Both come from counting descendants instead of
/// [`semantic_children`], and "possibly implied" is the `self::h:ul` term - a
/// `<ul>` is an entry list whether or not it says so.
pub(crate) fn check_content_model(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    check_body_declaration(doc, path, report);
    check_index_rules(doc, path, report);
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

    /// Either one entry list or one or more groups, never both — and a bare
    /// `<ul>` is an entry list.
    ///
    /// Every row measured against epubcheck 5.3.0, one book per shape, with
    /// `properties="index"` on the manifest item: without that the index
    /// Schematron does not run at all (`OPSChecker.validatorMap`), and a first
    /// probe that omitted it had both tools silent on all seven shapes and
    /// looked like agreement.
    #[test]
    fn an_index_holds_one_entry_list_or_some_groups() {
        let el = r#"<ul epub:type="index-entry-list"><li>a</li></ul>"#;
        let grp = format!(r#"<section epub:type="index-group">{el}</section>"#);
        let count = |inner: &str| -> usize {
            let xml = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>t</title></head>
<body><p>outside</p><section epub:type="index">{inner}</section></body></html>"#
            );
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let mut report = Report::default();
            super::check_content_model(&doc, "c.xhtml", &mut report);
            report
                .messages
                .iter()
                .filter(|m| m.rule == Some("indexes.content_model.wrong_entry_list_count"))
                .count()
        };

        assert_eq!(count(el), 0, "one entry list");
        assert_eq!(count(&grp), 0, "one group");
        assert_eq!(count(&format!("{grp}{grp}")), 0, "two groups");
        assert_eq!(count("<ul><li>a</li></ul>"), 0, "a bare ul is implied");
        assert_eq!(
            count(&format!("{grp}{el}")),
            1,
            "a group and a list is both"
        );
        assert_eq!(count("<p>nothing</p>"), 1, "neither");
        assert_eq!(count(&format!("{el}{el}")), 1, "two lists");
    }
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

    /// Rule keys `check_index_rules` gives a body fragment, sorted.
    fn index_rules(inner: &str) -> Vec<&'static str> {
        let xml = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>t</title></head><body><p id="p">x</p>{inner}</body></html>"#
        );
        let doc = crate::ocf::parse_xml(&xml).expect("fixture parses");
        let mut report = Report::new();
        super::check_index_rules(&doc, "ch1.xhtml", &mut report);
        let mut v: Vec<_> = report.messages.iter().filter_map(|m| m.rule).collect();
        v.sort();
        v
    }

    /// One case per `idx-xhtml.sch` assertion, each measured against
    /// epubcheck 5.4.0 on a real book (ids and the element they land on) on
    /// 2026-09-25: 52 books, all equal.
    #[test]
    fn index_rules_match_epubcheck_case_by_case() {
        const T: &str = r#"<span epub:type="index-term">t</span>"#;
        const L: &str = r##"<a epub:type="index-locator" href="#p">1</a>"##;
        let entry = |inner: &str| format!(r#"<li epub:type="index-entry">{inner}</li>"#);
        let list = |inner: &str| format!(r#"<ul epub:type="index-entry-list">{inner}</ul>"#);
        let index = |inner: &str| format!(r#"<section epub:type="index">{inner}</section>"#);
        let basic = index(&list(&entry(&format!("{T}{L}"))));
        let ok = |inner: String| assert!(index_rules(&inner).is_empty(), "{inner}");
        let is = |inner: String, want: &[&str]| assert_eq!(index_rules(&inner), want, "{inner}");

        ok(basic.clone());
        ok(index(&format!("<ul><li>{T}{L}</li></ul>")));
        ok(index(&format!(
            r#"<section epub:type="index-group"><h2>A</h2><ul><li>{T}{L}</li></ul></section><section epub:type="index-group"><ul><li>{T}{L}</li></ul></section>"#
        )));
        ok(index(&list(&entry(&format!(
            r##"{T}<span epub:type="index-locator-range"><a href="#p">1</a>-<a href="#p">2</a></span>"##
        )))));

        is(
            format!(
                r#"<div epub:type="index">{}</div>"#,
                list(&entry(&format!("{T}{L}")))
            ),
            &["indexes.index.element"],
        );
        is(
            index(&format!(
                "<h2>a</h2><h2>b</h2>{}",
                list(&entry(&format!("{T}{L}")))
            )),
            &["indexes.index.headings"],
        );
        is(
            index("<p>x</p>"),
            &["indexes.content_model.wrong_entry_list_count"],
        );
        is(
            index(&format!(
                r#"<section epub:type="index-group"><h2>a</h2><h3>b</h3>{}</section>"#,
                list(&entry(&format!("{T}{L}")))
            )),
            &["indexes.group.headings"],
        );
        is(
            format!("{basic}{}", list(&entry(&format!("{T}{L}")))),
            &["indexes.entry_list.parent"],
        );
        is(
            index(r#"<ul epub:type="index-entry-list"></ul>"#),
            &["indexes.entry_list.empty"],
        );
        is(index(&list(&entry(L))), &["indexes.entry.term"]);
        is(
            index(&list(&entry(&format!("{T}{T}{L}")))),
            &["indexes.entry.term"],
        );
        is(index(&list(&entry(T))), &["indexes.entry.no_target"]);
        is(
            index(&list(&entry(&format!(
                r##"{T}<span epub:type="index-locator-range"><a href="#p">1</a><a href="#p">2</a><a href="#p">3</a></span>"##
            )))),
            &["indexes.locator_range.links"],
        );
        is(
            index(&list(&entry(&format!(
                r#"{T}<span epub:type="index-xref-preferred">see u</span>"#
            )))),
            &["indexes.xref.term"],
        );
        is(
            format!(r##"{basic}<p><a epub:type="index-term-category" href="#p">c</a></p>"##),
            &["indexes.term_category.parent"],
        );
        // Silent in epubcheck, whose abstract "entry child" pattern never
        // fires: an editor note, a locator list or a cross-reference outside
        // an entry.
        ok(format!(
            r#"{basic}<p><span epub:type="index-editor-note">n</span></p>"#
        ));
        ok(format!(
            r##"{basic}<ul epub:type="index-locator-list"><li><a href="#p">1</a></li></ul>"##
        ));
        ok(format!(
            r#"{basic}<p><span epub:type="index-xref-preferred"><span epub:type="index-term">u</span></span></p>"#
        ));
    }
}
