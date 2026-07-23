//! SVG content-model checks, confirmed against the real corpus's error
//! *and* valid fixtures for `foreignObject`/`title`/generic SVG content:
//!
//! - `foreignObject`'s content must be ordinary XHTML flow content - reuses
//!   the *existing*, already-tested `schemas/xhtml.rng` flow-content
//!   grammar via a wrap+reparse trick (`Node::range()` gives the exact
//!   original-text byte span of any node, so the inner content can be
//!   reconstructed verbatim and re-validated - no RNG engine changes).
//! - `title`'s content model is far more permissive (a bare `<body>`, even
//!   a whole embedded `<html>` document, are valid title content per a
//!   real fixture) - just a recursive non-XHTML-namespace check, plus one
//!   narrow real HTML5 rule (`href` only valid on a/area/link/base).
//! - Everything else inside `<svg>` gets a generic, usage-level (`RSC-025`)
//!   element-vocabulary check - real epubcheck reports SVG conformance
//!   issues as USAGE, not errors (confirmed via a dedicated fixture).

use std::collections::HashMap;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const EPUB_OPS_NS: &str = "http://www.idpf.org/2007/ops";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Real SVG 1.1 element vocabulary. A false negative here is far safer
/// than a false positive, since `RSC-025` findings are usage-level (Info).
const SVG_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "image",
    "switch",
    "foreignObject",
    "title",
    "desc",
    "metadata",
    "a",
    "style",
    "script",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "path",
    "text",
    "tspan",
    "textPath",
    "tref",
    "marker",
    "pattern",
    "mask",
    "clipPath",
    "filter",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "linearGradient",
    "radialGradient",
    "stop",
    "animate",
    "animateMotion",
    "animateTransform",
    "set",
    "mpath",
    "view",
    "cursor",
    "font",
    "font-face",
    "glyph",
    "missing-glyph",
    "hkern",
    "vkern",
];

fn is_recognized_element(name: &str) -> bool {
    SVG_ELEMENTS.contains(&name)
}

/// `RSC-025` (usage): an SVG-namespaced element not in the known
/// vocabulary. Stops descending at `foreignObject`/`title` boundaries
/// (their own, separate content models apply instead - checked via
/// `check_foreign_object`/`check_title_content`) and only ever looks at
/// SVG-namespaced children, so foreign content nested inside (embedded
/// RDF in `<metadata>`, etc.) is never touched by this check.
pub(crate) fn check_vocabulary(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for child in svg_root.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(SVG_NS) {
            continue;
        }
        let name = child.tag_name().name();
        if !is_recognized_element(name) {
            report.push_at_pos(
                RSC_025,
                Severity::Usage,
                format!("element \"{name}\" not allowed here"),
                path,
                Position::of(child),
            );
        }
        if matches!(name, "foreignObject" | "title") {
            continue;
        }
        check_vocabulary(child, path, report);
    }
}

/// `epub:type` is disallowed on non-visual/metadata SVG elements - `title`/
/// `desc`/`defs`/`tref` (confirmed via one real fixture testing all four
/// at once, plus an unrecognized element, expecting exactly 5 findings)
/// - and on any unrecognized element; it's allowed everywhere else,
/// including the `<svg>` root itself (confirmed via a real "valid"
/// fixture using it on a dozen ordinary shape/text elements plus the
/// root). Any *other* `epub:*`-namespaced attribute (i.e. anything other
/// than `epub:type`) is always disallowed, regardless of element.
const EPUB_TYPE_FORBIDDEN_ELEMENTS: &[&str] = &["title", "desc", "defs", "tref"];

pub(crate) fn check_epub_attributes(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for attr in svg_root.attributes() {
        check_one_epub_attribute(svg_root, attr, path, report);
    }
    for child in svg_root
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(SVG_NS))
    {
        check_epub_attributes_rec(child, path, report);
    }
}

fn check_epub_attributes_rec(n: roxmltree::Node, path: &str, report: &mut Report) {
    for attr in n.attributes() {
        check_one_epub_attribute(n, attr, path, report);
    }
    if matches!(n.tag_name().name(), "foreignObject" | "title") {
        return;
    }
    for child in n
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(SVG_NS))
    {
        check_epub_attributes_rec(child, path, report);
    }
}

fn check_one_epub_attribute(
    n: roxmltree::Node,
    attr: roxmltree::Attribute,
    path: &str,
    report: &mut Report,
) {
    if attr.namespace() != Some(EPUB_OPS_NS) {
        return;
    }
    if attr.name() == "type" {
        let name = n.tag_name().name();
        if EPUB_TYPE_FORBIDDEN_ELEMENTS.contains(&name) || !is_recognized_element(name) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "attribute \"epub:type\" not allowed here",
                path,
                n,
                "svg.epub_attributes.type_not_allowed",
                Vec::new(),
            );
        }
    } else if attr.name() == "prefix" {
        // A real, legitimate attribute - checked separately, in full,
        // by `opf::check_prefix_declaration`/`check_prefix_placement`
        // (confirmed via a real fixture declaring `epub:prefix` on an
        // SVG root and expecting zero findings).
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("attribute \"epub:{}\" not allowed here", attr.name()),
            path,
            n,
            "svg.epub_attributes.attribute_not_allowed",
            vec![attr.name().to_string()],
        );
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

/// `RSC-005`: every `id` attribute anywhere in the SVG document must be a
/// valid XML NCName (a real fixture uses `id="1"`, invalid because it
/// starts with a digit) and unique document-wide (a real fixture shares
/// one id between two elements, reported once *per* colliding element -
/// the same "per-element not per-pair" convention already used
/// elsewhere in this project, e.g. NCX id duplication).
pub(crate) fn check_ids(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    let mut by_id: HashMap<&str, u32> = HashMap::new();
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            if !is_valid_ncname(id) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"id\" is invalid: '{id}'"),
                    path,
                    n,
                    "svg.ids.invalid_ncname",
                    vec![id.to_string()],
                );
            }
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id")
            && by_id.get(id).copied().unwrap_or(0) > 1
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("Duplicate \"id\" value '{id}'"),
                path,
                n,
                "svg.ids.duplicate_id",
                vec![id.to_string()],
            );
        }
    }
}

/// `ACC-011` (usage): an SVG `<a>` link with no accessible label at all -
/// no `xlink:title` attribute, no `<title>` child, no `aria-label`, and
/// no real text content anywhere inside it (confirmed via a real fixture
/// exercising all four labeling mechanisms as valid, plus a fifth `<a>`
/// with none of them).
pub(crate) fn check_link_labels(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for a in svg_root.descendants().filter(|n| {
        n.is_element() && n.tag_name().name() == "a" && n.tag_name().namespace() == Some(SVG_NS)
    }) {
        let has_label = a.attribute((XLINK_NS, "title")).is_some()
            || a.attr_no_ns("aria-label").is_some()
            || a.children()
                .any(|c| c.is_element() && c.tag_name().name() == "title")
            || a.descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .any(|t| !t.trim().is_empty());
        if !has_label {
            report.push_at_pos(
                ACC_011,
                Severity::Usage,
                "SVG link has no accessible label",
                path,
                Position::of(a),
            );
        }
    }
}

/// A real HTML5 rule: `href` is only a valid attribute on
/// `a`/`area`/`link`/`base` - `schemas/xhtml.rng`'s attribute handling is
/// deliberately permissive (a global catch-all pattern, not a per-element
/// attribute allowlist - see `anyOtherAttr`'s own doc comment), so this
/// isn't caught by the flow-content grammar and needs its own check.
fn check_href_attribute(n: roxmltree::Node, path: &str, report: &mut Report) {
    let name = n.tag_name().name();
    if !matches!(name, "a" | "area" | "link" | "base") && n.has_attr_no_ns("href") {
        report.push_node(
            RSC_005,
            Severity::Error,
            "attribute \"href\" not allowed here",
            path,
            n,
            "svg.content_model.href_not_allowed",
            Vec::new(),
        );
    }
}

/// `RSC-005`: any descendant not in the XHTML namespace ("elements from
/// namespace X are not allowed"), plus `check_href_attribute`. Confirmed
/// this is NOT a flow-content check: a real valid fixture uses a bare
/// `<body>`, and even a whole embedded `<html>` document, as title
/// content.
pub(crate) fn check_title_content(title: roxmltree::Node, path: &str, report: &mut Report) {
    // `descendants()` includes the node itself first - skip it (title's
    // own namespace is SVG, not XHTML, and isn't part of its own content).
    for n in title.descendants().skip(1).filter(|n| n.is_element()) {
        let ns = n.tag_name().namespace();
        if ns != Some(XHTML_NS) {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "elements from namespace \"{}\" are not allowed",
                    ns.unwrap_or("")
                ),
                path,
                n,
                "svg.title.foreign_namespace",
                vec![ns.unwrap_or("").to_string()],
            );
            continue;
        }
        check_href_attribute(n, path, report);
    }
}

/// Re-validates `foreignObject`'s inner content against the existing
/// XHTML flow-content grammar. Reconstructs the exact inner XML via
/// `Node::range()` (the original-text byte span of each child), wraps it
/// in a synthetic document that carries forward every namespace binding
/// from the real document's root (so prefixed content, e.g. `xlink:...`,
/// still resolves), re-parses, and validates via the same
/// `crate::rng::xhtml_grammar()` used for whole content documents - no
/// RNG engine changes needed.
///
/// EPUB3-only: a real EPUB2 fixture (`svg-foreignObject-switch-valid.xhtml`,
/// titled "body allowed inside foreignObject") explicitly permits a bare
/// `<body>` as foreignObject content, unlike EPUB3's own
/// `svg-foreignObject-with-body-error` fixture, which flags the exact same
/// shape as an error - EPUB2's OPS/XHTML content model is its own, more
/// lenient spec section, same precedent as several other EPUB3-only checks
/// in `htm.rs`/`opf.rs`.
pub(crate) fn check_foreign_object(
    fo: roxmltree::Node,
    text: &str,
    root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    wrap_in_body: bool,
    report: &mut Report,
) {
    if !is_epub3 {
        return;
    }
    let mut children = fo.children();
    let Some(first) = children.next() else {
        return;
    };
    let last = fo.children().last().unwrap_or(first);
    let inner = &text[first.range().start..last.range().end];

    // Every *prefixed* namespace binding from the real document's root
    // carries forward, so prefixed content inside the foreignObject still
    // resolves - but the wrapper's own *default* (unprefixed) namespace
    // is always forced to XHTML, regardless of what `root` itself
    // declares. When `root` is an XHTML document's own root, its default
    // already is XHTML, so this changes nothing there - but when `root`
    // is a standalone SVG document's own `<svg>` element (the other real
    // call site), its default is the SVG namespace, and copying it
    // verbatim would put the synthetic `<html>`/`<body>` wrapper itself
    // in the SVG namespace, failing the XHTML grammar check on every
    // single foreignObject regardless of its actual (valid) content - a
    // real bug only ever exposed once standalone SVG single-document
    // checks started actually running through this code path.
    let mut ns_decls = String::new();
    for ns in root.namespaces() {
        match ns.name() {
            // "xml" is always implicitly bound to the fixed XML namespace
            // URI - redeclaring it is unnecessary and, if anything went
            // slightly wrong upstream, a needless source of a parse error.
            Some("xml") => continue,
            Some(prefix) => ns_decls.push_str(&format!(" xmlns:{prefix}=\"{}\"", ns.uri())),
            None => {}
        }
    }
    // Embedded (foreignObject inside an XHTML document's own inline SVG):
    // there's already an ambient XHTML `<body>` in scope, so the content
    // is ordinary flow content and gets wrapped in a synthetic `<body>`
    // (confirmed: a real fixture explicitly flags a *literal* `<body>`
    // element appearing here as its own error, "element \"body\" not
    // allowed here" - body-inside-body). Standalone (a top-level SVG
    // content document with no ambient XHTML context at all): the
    // content itself must directly *be* a single `<body>` element (real
    // fixtures confirm both "non-body content" and "more than one body"
    // are their own distinct errors) - so it replaces the body slot
    // instead of being wrapped inside another one.
    let wrapped = if wrap_in_body {
        format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"{ns_decls}><head><title>t</title></head><body>{inner}</body></html>"
        )
    } else {
        format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"{ns_decls}><head><title>t</title></head>{inner}</html>"
        )
    };
    let Ok(doc) = crate::ocf::parse_xml(&wrapped) else {
        return;
    };
    if !crate::rng::validate_node(&crate::rng::xhtml_grammar(), doc.root_element()) {
        // Genuine catch-all, same caveat as opf.rs's RNG-backed checks:
        // the grammar doesn't expose which rule failed. This now also
        // covers `href` on a non-a/area/link/base host - #33 excepted
        // `href` from the wildcard (needed for a/area's own explicit
        // rules to be unambiguous, see #39), so the grammar itself rejects
        // it anywhere else. A separate `check_href_attribute` pass used to
        // be the only thing catching this inside foreignObject; running
        // both now double-reports the exact same defect (caught by
        // foreign_object_rejects_invalid_attribute expecting a single
        // RSC-005) - removed here, kept in check_title_content above,
        // which doesn't re-validate against the grammar and still needs
        // its own check.
        report.push_node(
            RSC_005,
            Severity::Error,
            "foreignObject content does not conform to the EPUB XHTML content-model schema",
            path,
            fo,
            "svg.foreign_object.schema_violation",
            Vec::new(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Report;

    fn doc(xml: &str) -> roxmltree::Document<'_> {
        crate::ocf::parse_xml(xml).unwrap()
    }

    const XHTML_OPEN: &str = concat!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" ",
        "xmlns:svg=\"http://www.w3.org/2000/svg\" ",
        "xmlns:xlink=\"http://www.w3.org/1999/xlink\">"
    );

    #[test]
    fn foreign_object_rejects_body_element() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <body><div>disallowed</div></body>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_rejects_invalid_attribute() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <p href=\"#error\">Hello</p>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_rejects_non_flow_content() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <title>Hello</title>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_accepts_flow_content() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <p>Hello</p>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn foreign_object_accepts_whitespace_only() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject> \
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn foreign_object_body_allowed_in_epub2() {
        // A real EPUB2 fixture, titled exactly "body allowed inside
        // foreignObject" - EPUB2's OPS/XHTML content model is more lenient
        // than EPUB3's here.
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <body><div>Part I:</div></body>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            false,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn title_rejects_foreign_namespace_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><not:html xmlns:not=\"https://example.org\">x</not:html></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn title_rejects_nested_foreign_namespace_inside_xhtml_body() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><body xmlns=\"http://www.w3.org/1999/xhtml\">",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Inner</title></svg>",
            "</body></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        // Only the nested svg (and its own nested title) are foreign - the
        // xhtml <body> itself must not be flagged.
        assert!(!report.messages.is_empty());
    }

    #[test]
    fn title_accepts_bare_body_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><body xmlns=\"http://www.w3.org/1999/xhtml\">text</body></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn title_rejects_href_on_span() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><span href=\"#error\" xmlns=\"http://www.w3.org/1999/xhtml\">t</span></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn title_accepts_plain_text() {
        let xml =
            concat!("<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Plain text</title></svg>");
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn vocabulary_rejects_unknown_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title>Title</title><foo>Invalid</foo>",
            "</svg>"
        );
        let d = doc(xml);
        let svg_root = d.root_element();
        let mut report = Report::new();
        check_vocabulary(svg_root, "c.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_025]
        );
    }

    #[test]
    fn vocabulary_accepts_svg_own_anchor_with_xlink() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">",
            "<desc>Example</desc>",
            "<a xlink:href=\"https://example.org\" xlink:title=\"example\" target=\"_blank\" rel=\"noreferrer\">link</a>",
            "</svg>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_vocabulary(d.root_element(), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn vocabulary_ignores_foreign_namespaced_metadata_content() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<metadata><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description/></rdf:RDF></metadata>",
            "</svg>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_vocabulary(d.root_element(), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }
}
