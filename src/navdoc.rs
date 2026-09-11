//! EPUB 3 §7 Navigation Document content model: the four defined `nav`
//! types (`toc`/`page-list`/`landmarks`, plus any other named type) each
//! restrict their own content - an optional heading, then a required
//! `<ol>` of `<li>`s whose own content model is `(a|span), ol?`. A `<nav>`
//! with no `epub:type` at all is completely unrestricted (confirmed via a
//! real fixture using arbitrary markup in one).

use crate::ids::*;
use crate::report::{Report, Severity};
use crate::xmlext::NodeExt;

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

fn nav_type<'a>(nav: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    nav.attribute((EPUB_NS, "type"))
}

/// Is `n` inside an `<ol>` that is itself inside `nav`?
///
/// The `//html:ol//` step of epubcheck's anchor- and span-label contexts. A
/// nav's own heading is not in the list, so its contents are not label text.
fn within_ol(nav: roxmltree::Node, n: roxmltree::Node) -> bool {
    n.ancestors()
        .skip(1)
        .take_while(|a| *a != nav)
        .any(|a| a.is_element() && a.tag_name().name() == "ol")
}

/// Does a nav label (`<a>`/`<span>`) or a heading carry text?
///
/// epubcheck asks one expression in all three places (`epub-nav-30.sch`,
/// patterns `link-labels`, `span-labels` and `heading-content`):
///
/// ```text
/// string-length(normalize-space(string-join(.|./html:img/@alt|.//@aria-label))) > 0
/// ```
///
/// Three sources, and this is a port of all three rather than of the first:
///
/// 1. the element's own string value - every descendant text node;
/// 2. the `alt` of an `<img>` that is a **direct child**, not a descendant;
/// 3. an `aria-label` on the element **or** on any descendant (`.//@attr`
///    steps through `descendant-or-self`, so the element's own counts).
///
/// **Blank means XML-blank, not Unicode-blank.** `normalize-space()` strips
/// exactly space, tab, CR and LF, so `<span>&#160;</span>` is a label with
/// text in it. This used `str::trim`, whose Unicode `White_Space` includes
/// NO-BREAK SPACE, and so reported "must contain text" on a book epubcheck
/// passes - 3 books of an external 2,798-book run, 2 of them changing verdict.
/// [`is_xml_blank`] exists for exactly this trap and names four sites it was
/// written for; this was the fifth and it was missed.
///
/// **The `<img>` half was wrong in the other direction, and on written
/// evidence that misread its own fixture.** The note here used to say an
/// `<img>` descendant is enough "confirmed via a real fixture: two `<img>`
/// elements with no text at all, one even with an empty `alt`". The fixture is
/// `content-model-a-multiple-images-valid.xhtml`, and what makes it valid is
/// the *second* image's `alt="some text"` - not the presence of an image. So
/// an `<img>` with no `alt`, or an empty one, or one wrapped in a `<span>`,
/// was a label we passed and epubcheck rejects.
///
/// All of it measured against epubcheck 5.3.0, one book per shape: `&#160;`
/// alone, `aria-label` on the anchor, and `aria-label` on a descendant are
/// **valid**; an empty `alt`, an absent `alt`, and a correctly-labelled `<img>`
/// nested one level down are each **RSC-005**.
fn has_label_text(n: roxmltree::Node) -> bool {
    let texty = |s: &str| !crate::xmlext::is_xml_blank(s);
    let text = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .any(texty);
    let child_img_alt = || {
        n.children()
            .filter(|c| c.is_element() && c.tag_name().name() == "img")
            .filter_map(|c| c.attr_no_ns("alt"))
            .any(texty)
    };
    // roxmltree's `descendants()` yields the node itself first, which is the
    // `descendant-or-self` step the XPath needs - do not narrow it to children.
    let aria_label = || {
        n.descendants()
            .filter_map(|d| d.attr_no_ns("aria-label"))
            .any(texty)
    };
    text || child_img_alt() || aria_label()
}

/// `hidden` is an HTML5 boolean attribute - only an empty value or the
/// literal string "hidden" are conforming.
fn check_hidden_attrs(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(v) = n.attr_no_ns("hidden")
            && !matches!(v, "" | "hidden")
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                "value of attribute \"hidden\" is invalid",
                path,
                n,
                "navdoc.hidden_attribute.invalid_value",
                vec![v.to_string()],
            );
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
        // No element child at all, so there is no label. This returned
        // silently until Doitsu reported it (MobileRead 374286 #281):
        // `<li></li>` inside a `page-list` nav drew nothing from us and
        // `element "li" incomplete` from epubcheck.
        //
        // **Widening the report is what found the rest of the class.** His
        // case was the empty one; probing the boundary showed epubcheck also
        // rejects whitespace-only (same single message) and text-only — and
        // for text it gives *two*, one for the text standing where the label
        // should be and one for the label still being absent. All four shapes
        // fell through this one `return`.
        if let Some(text) = li
            .children()
            .find(|c| c.is_text() && c.text().is_some_and(|t| !t.trim().is_empty()))
        {
            // One per `<li>`, which is what epubcheck gives. Several text
            // runs with no element between them needs a comment or a PI to
            // split them, and the second run is the same mistake as the
            // first.
            report.push_node_text(
                RSC_005,
                Severity::Error,
                "text not allowed here; expected element \"a\" or \"span\"",
                path,
                text,
                "navdoc.li.stray_text",
                Vec::new(),
            );
        }
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"li\" incomplete; expected element \"a\" or \"span\"",
            path,
            li,
            "navdoc.li.missing_label",
            Vec::new(),
        );
        return;
    };
    let label_name = label.tag_name().name();
    if !matches!(label_name, "a" | "span") {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("element \"{label_name}\" not allowed yet; expected element \"a\" or \"span\""),
            path,
            *label,
            "navdoc.li.invalid_label",
            vec![label_name.to_string()],
        );
        return;
    }
    let nested_ol = children.get(1).filter(|c| c.tag_name().name() == "ol");
    if label_name == "span" && nested_ol.is_none() {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"li\" incomplete; missing required element \"ol\"",
            path,
            li,
            "navdoc.li.span_missing_ol",
            Vec::new(),
        );
    }
    if let Some(ol) = nested_ol {
        if matches!(ty, "page-list" | "landmarks") {
            report.push_node(
                RSC_017,
                Severity::Warning,
                format!("the \"{ty}\" nav must have no nested sublists"),
                path,
                *ol,
                "navdoc.nav.nested_sublist_not_allowed",
                vec![ty.to_string()],
            );
        }
        check_ol(*ol, ty, path, report);
    }
}

/// An `<ol>` (top-level or nested) must have at least one `<li>`.
///
/// The message names what is missing, like its `<li>` sibling above and like
/// epubcheck: `element "ol" incomplete; missing required element "li"`. It said
/// only the first half until DNSB's file (MobileRead #268) put the two outputs
/// side by side - the same finding on the same line, ours telling the author
/// less about it.
fn check_ol(ol: roxmltree::Node, ty: &str, path: &str, report: &mut Report) {
    let lis: Vec<_> = ol
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "li")
        .collect();
    if lis.is_empty() {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"ol\" incomplete; missing required element \"li\"",
            path,
            ol,
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
    if let Some(first) = children.first()
        && matches!(
            first.tag_name().name(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        )
    {
        heading = Some(*first);
        idx = 1;
    }
    match heading {
        // The empty-heading check is not here: `heading-content`'s context is
        // every `h1`-`h6` in the navigation document, not only the one a nav
        // opens with, so it runs once over the whole document in `check`.
        // Doing it here missed `<h2>  </h2>` sitting outside any nav, which
        // epubcheck reports (#79).
        Some(_) => {}
        None if !matches!(ty, "toc" | "page-list" | "landmarks") => {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("the \"{ty}\" nav must have a heading"),
                path,
                nav,
                "navdoc.nav.missing_heading",
                vec![ty.to_string()],
            );
        }
        None => {}
    }
    // `epub-nav-30.sch`'s `flat-nav`: a `page-list` or `landmarks` nav asserts
    // `count(.//ol) = 1`, so both zero and two or more fail it. Reported as a
    // warning, and independent of the missing-`ol` error below - the book in
    // #79 draws both, its landmarks nav having no `ol` at all, which is why
    // "should contain only a single ol" appears next to "missing required
    // element ol" and looks odd.
    if matches!(ty, "page-list" | "landmarks") {
        let ols = nav
            .descendants()
            .filter(|d| d.is_element() && d.tag_name().name() == "ol")
            .count();
        // Only the *zero* case. `count(.//ol) = 1` fails at both ends, but
        // the other end already has an owner: `check_li` reports
        // `navdoc.nav.nested_sublist_not_allowed` per nested sublist. Adding
        // this one unconditionally double-reported a nested landmarks nav -
        // caught by the test below, which is why it asserts counts and not
        // just presence.
        if ols == 0 {
            report.push_node(
                RSC_017,
                Severity::Warning,
                format!("the \"{ty}\" nav must contain an ol descendant"),
                path,
                nav,
                "navdoc.nav.not_flat",
                vec![ty.to_string()],
            );
        }
    }
    // A `nav` requires an `ol` (`epub-nav-30.rnc`: `html5.headings.class?,
    // epub.nav.ol`). This used to be `else { return }` - the arm below
    // reports a child that is present and is not an `ol`, so the *absent*
    // case escaped in silence, which is the one shape a user cannot notice
    // (CHANGELOG 0.7.12-0.7.14). Found via #79, where a `<nav><h1>…</h1></nav>`
    // came back VALID here and drew RSC-005 from epubcheck.
    let Some(ol) = children.get(idx) else {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"nav\" is incomplete; it requires an \"ol\" element",
            path,
            nav,
            "navdoc.nav.missing_ol",
            Vec::new(),
        );
        return;
    };
    if ol.tag_name().name() != "ol" {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("element \"{}\" not allowed here", ol.tag_name().name()),
            path,
            *ol,
            "navdoc.nav.expected_ol",
            vec![ol.tag_name().name().to_string()],
        );
        return;
    }
    check_ol(*ol, ty, path, report);
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
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "Missing epub:type attribute on anchor inside \"landmarks\" nav",
                    path,
                    a,
                    "navdoc.landmarks.missing_epub_type",
                    Vec::new(),
                );
            }
            Some(types) => {
                if let Some(href) = a.attr_no_ns("href") {
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
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "Another landmark was found with the same epub:type and same reference",
                    path,
                    *node_i,
                    "navdoc.landmarks.duplicate_entry",
                    Vec::new(),
                );
            }
        }
    }
}

/// Entry point, called once for the actual nav document.
///
/// It used to take the manifest map and the fallback map for its own RSC-010
/// on `toc` links. That check was generalised to every hyperlink in #78
/// (`opf.rs`, `opf.content_document.hyperlink_not_content_document`) and the
/// narrower one was left in place, so a nav link to a non-Content-Document
/// drew the finding **twice** — caught by W3C's `pub-foreign_bad-fallback`,
/// where epubcheck reports one and we reported two at the same position.
/// The general site subsumes this one; the parameters left with it.
pub(crate) fn check(doc: &roxmltree::Document, path: &str, dir: &str, report: &mut Report) {
    check_hidden_attrs(doc, path, report);

    let navs: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "nav")
        .collect();

    // `nav-ocurrence` asserts `count(toc) = 1`, which fails at both ends. We
    // had the zero end only, so a nav document with two `toc` navs was
    // accepted here and RSC-005 from epubcheck (#79). The `page-list` and
    // `landmarks` asserts below are `< 2` and so have no zero end to miss.
    for h in doc.descendants().filter(|n| {
        n.is_element() && matches!(n.tag_name().name(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
    }) {
        if !has_label_text(h) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "heading elements must contain text",
                path,
                h,
                "navdoc.heading.empty_text",
                Vec::new(),
            );
        }
    }
    if navs.iter().filter(|n| nav_type(**n) == Some("toc")).count() > 1 {
        report.push_node(
            RSC_005,
            Severity::Error,
            "the nav document has more than one \"toc\" nav",
            path,
            doc.root_element(),
            "navdoc.document.multiple_toc",
            Vec::new(),
        );
    }
    if !navs.iter().any(|n| nav_type(*n) == Some("toc")) {
        report.push_node(
            RSC_005,
            Severity::Error,
            "the nav document has no \"toc\" nav",
            path,
            doc.root_element(),
            "navdoc.document.missing_toc",
            Vec::new(),
        );
    }
    let page_lists: Vec<_> = navs
        .iter()
        .filter(|n| nav_type(**n) == Some("page-list"))
        .collect();
    if let Some(second) = page_lists.get(1) {
        report.push_node(
            RSC_005,
            Severity::Error,
            "Multiple occurrences of the \"page-list\" nav element",
            path,
            **second,
            "navdoc.document.multiple_page_list",
            Vec::new(),
        );
    }
    let landmarks: Vec<_> = navs
        .iter()
        .filter(|n| nav_type(**n) == Some("landmarks"))
        .collect();
    if let Some(second) = landmarks.get(1) {
        report.push_node(
            RSC_005,
            Severity::Error,
            "Multiple occurrences of the \"landmarks\" nav element",
            path,
            **second,
            "navdoc.document.multiple_landmarks",
            Vec::new(),
        );
    }

    for nav in navs {
        let Some(ty) = nav_type(nav) else {
            continue; // no epub:type at all - unrestricted content model
        };
        check_nav_content_model(nav, ty, path, report);
        // **Both label rules are scoped to the list, not to the whole nav.**
        // epubcheck's contexts are `html:nav[@epub:type]//html:ol//html:a`
        // and `…//html:ol//html:span` (`epub-nav-30.sch`), and we walked every
        // descendant instead — so an empty `<span>` inside the nav's *heading*
        // drew "Spans within nav elements must contain text" on top of the
        // heading rule that already covers it. Two findings for one empty
        // element, against epubcheck's one, on
        // `content-model-heading-empty-error.xhtml`.
        //
        // The heading rule is the one that keeps that case: its context is
        // `html:h1|…|html:h6` with no nav scoping at all, so nothing goes
        // quiet here.
        for a in nav
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "a")
            .filter(|n| within_ol(nav, *n))
        {
            if !has_label_text(a) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "Anchors within nav elements must contain text",
                    path,
                    a,
                    "navdoc.label.empty_anchor",
                    Vec::new(),
                );
            }
        }
        for span in nav
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "span")
            .filter(|n| within_ol(nav, *n))
        {
            if !has_label_text(span) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "Spans within nav elements must contain text",
                    path,
                    span,
                    "navdoc.label.empty_span",
                    Vec::new(),
                );
            }
        }
        if ty == "landmarks" {
            check_landmarks(nav, dir, path, report);
        }
    }
}
