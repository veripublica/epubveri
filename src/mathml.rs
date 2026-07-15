//! MathML content-model checks - reverse-engineered from the real corpus's
//! 5 error fixtures plus 8 matched valid fixtures for `annotation-xml`
//! (several rules only resolve by diffing matched pairs):
//!
//! - A `<math>` element's own content (outside any `annotation`/
//!   `annotation-xml`) must be Presentation MathML - using Content MathML
//!   elements (`apply`, `cn`, ...) directly is `RSC-005`.
//! - `annotation-xml`'s `encoding` attribute must be one of a closed, real
//!   enumeration; an unrecognized value is `RSC-005` and short-circuits
//!   every other check below (the real fixture expects exactly one
//!   finding, not a cascade).
//! - `name` is required, and constrained to exactly `"contentequiv"`,
//!   *only* when `encoding` is a Content-MathML value - confirmed via a
//!   real fixture where an XHTML-encoded annotation omits `name`
//!   entirely and is valid.
//! - When `encoding` is a Presentation-MathML value, the annotation's own
//!   content must also be Presentation MathML (reuses the same allowlist
//!   walk as the top-level check).

use crate::ids::*;
use crate::report::{Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";

/// Real Presentation MathML (MathML3 §3) element vocabulary. A false
/// negative is far safer than a false positive here - this list is
/// deliberately generous.
const PRESENTATION_ELEMENTS: &[&str] = &[
    "mi",
    "mn",
    "mo",
    "mtext",
    "mspace",
    "ms",
    "mglyph",
    "mrow",
    "mfrac",
    "msqrt",
    "mroot",
    "mstyle",
    "merror",
    "mpadded",
    "mphantom",
    "mfenced",
    "menclose",
    "msub",
    "msup",
    "msubsup",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "none",
    "mtable",
    "mtr",
    "mlabeledtr",
    "mtd",
    "maction",
    "semantics",
];

const CONTENT_ENCODINGS: &[&str] = &["MathML-Content", "application/mathml-content+xml"];
const PRESENTATION_ENCODINGS: &[&str] = &[
    "MathML-Presentation",
    "application/mathml-presentation+xml",
    "MathML",
];
const XHTML_ENCODINGS: &[&str] = &["application/xhtml+xml", "text/html"];
const SVG_ENCODINGS: &[&str] = &["SVG1.1"];

/// Entry point: checks a `<math>` element's own top-level content, plus
/// every `annotation-xml` found anywhere within it (however deeply nested
/// under `semantics`).
pub(crate) fn check_math_element(math: roxmltree::Node, path: &str, report: &mut Report) {
    check_presentation_content(math, path, report);
    for anno in math.descendants().skip(1).filter(|n| {
        n.is_element()
            && n.tag_name().name() == "annotation-xml"
            && n.tag_name().namespace() == Some(MATHML_NS)
    }) {
        check_annotation_xml(anno, path, report);
    }
}

/// Walks `node`'s MathML-namespaced children, **not recursing past**
/// `annotation`/`annotation-xml` (their content has its own, separate
/// rules - `check_annotation_xml`). Anything else not in
/// `PRESENTATION_ELEMENTS` is `RSC-005`.
fn check_presentation_content(node: roxmltree::Node, path: &str, report: &mut Report) {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(MATHML_NS) {
            continue;
        }
        let name = child.tag_name().name();
        if matches!(name, "annotation" | "annotation-xml") {
            continue;
        }
        if !PRESENTATION_ELEMENTS.contains(&name) {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("element \"{name}\" not allowed here"),
                path,
                child,
                "mathml.presentation.unrecognized_element",
                vec![name.to_string()],
            );
            // Don't recurse further into an already-rejected subtree -
            // confirmed via the real corpus fixture, which reports exactly
            // one finding per invalid Content-MathML subtree (its own
            // nested elements aren't separately flagged too).
            continue;
        }
        check_presentation_content(child, path, report);
    }
}

fn check_annotation_xml(anno: roxmltree::Node, path: &str, report: &mut Report) {
    let encoding = anno.attr_no_ns("encoding");
    let is_content = encoding.is_some_and(|e| CONTENT_ENCODINGS.contains(&e));
    let is_presentation = encoding.is_some_and(|e| PRESENTATION_ENCODINGS.contains(&e));
    let is_xhtml = encoding.is_some_and(|e| XHTML_ENCODINGS.contains(&e));
    let is_svg = encoding.is_some_and(|e| SVG_ENCODINGS.contains(&e));

    if encoding.is_some() && !(is_content || is_presentation || is_xhtml || is_svg) {
        report.push_node(
            RSC_005,
            Severity::Error,
            "value of attribute \"encoding\" is invalid; must be equal to one of \
             \"MathML-Content\", \"MathML-Presentation\", \"application/xhtml+xml\", \
             \"text/html\", \"SVG1.1\""
                .to_string(),
            path,
            anno,
            "mathml.annotation_xml.invalid_encoding",
            vec![encoding.unwrap_or("").to_string()],
        );
        // Avoid a cascading, contradictory content-model finding once the
        // encoding itself is already known to be bogus.
        return;
    }

    if is_content {
        match anno.attr_no_ns("name") {
            None => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "element \"annotation-xml\" missing required attribute \"name\"",
                    path,
                    anno,
                    "mathml.annotation_xml.missing_name",
                    Vec::new(),
                );
            }
            Some(n) if n != "contentequiv" => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "value of attribute \"name\" is invalid",
                    path,
                    anno,
                    "mathml.annotation_xml.invalid_name",
                    vec![n.to_string()],
                );
            }
            _ => {}
        }
    }

    if is_presentation {
        check_presentation_content(anno, path, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Report;

    fn doc(xml: &str) -> roxmltree::Document<'_> {
        crate::ocf::parse_xml(xml).unwrap()
    }

    fn ids(report: &Report) -> Vec<&'static str> {
        report.messages.iter().map(|m| m.id).collect()
    }

    fn math_of<'a>(d: &'a roxmltree::Document<'a>) -> roxmltree::Node<'a, 'a> {
        d.descendants()
            .find(|n| n.tag_name().name() == "math" && n.tag_name().namespace() == Some(MATHML_NS))
            .unwrap()
    }

    #[test]
    fn rejects_content_mathml_at_top_level() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<apply><csymbol cd=\"arith1\">times</csymbol><ci>x</ci></apply>",
            "</math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert_eq!(ids(&report), vec![RSC_005]);
    }

    #[test]
    fn accepts_presentation_mathml() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<mrow><mi>a</mi><mo>+</mo><mi>b</mi></mrow>",
            "</math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn annotation_xml_content_encoding_requires_name() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mrow><mi>a</mi></mrow>",
            "<annotation-xml encoding=\"MathML-Content\"><apply><and/></apply></annotation-xml>",
            "</semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert_eq!(ids(&report), vec![RSC_005]);
    }

    #[test]
    fn annotation_xml_content_encoding_name_must_be_contentequiv() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mrow><mi>a</mi></mrow>",
            "<annotation-xml encoding=\"MathML-Content\" name=\"alternate-representation\">",
            "<apply><and/></apply></annotation-xml>",
            "</semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert_eq!(ids(&report), vec![RSC_005]);
    }

    #[test]
    fn annotation_xml_content_encoding_valid_with_contentequiv() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mrow><mi>a</mi></mrow>",
            "<annotation-xml name=\"contentequiv\" encoding=\"MathML-Content\">",
            "<apply><and/></apply></annotation-xml>",
            "</semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn annotation_xml_presentation_encoding_rejects_content_mathml() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mrow><mi>a</mi></mrow>",
            "<annotation-xml encoding=\"MathML-Presentation\" name=\"contentequiv\">",
            "<apply><and/></apply></annotation-xml>",
            "</semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert_eq!(ids(&report), vec![RSC_005]);
    }

    #[test]
    fn annotation_xml_xhtml_encoding_needs_no_name() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mfrac><mi>a</mi><mi>b</mi></mfrac>",
            "<annotation-xml encoding=\"application/xhtml+xml\">",
            "<span xmlns=\"http://www.w3.org/1999/xhtml\">a over b</span>",
            "</annotation-xml></semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn annotation_xml_invalid_encoding_reports_once() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mfrac><mi>a</mi><mi>b</mi></mfrac>",
            "<annotation-xml encoding=\"application/xml+xhtml\" name=\"alternate-representation\">",
            "<span xmlns=\"http://www.w3.org/1999/xhtml\">a over b</span>",
            "</annotation-xml></semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert_eq!(ids(&report), vec![RSC_005]);
    }

    #[test]
    fn annotation_xml_svg_encoding_is_lenient() {
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">",
            "<semantics><mfrac><mi>a</mi><mi>b</mi></mfrac>",
            "<annotation-xml encoding=\"SVG1.1\" name=\"alternate-representation\">",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><desc>d</desc></svg>",
            "</annotation-xml></semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn annotation_xml_xhtml_encoding_permits_nested_math() {
        // A real corpus fixture nests <math> inside an xhtml-encoded
        // annotation and is explicitly valid, despite the fixture's own
        // comment suggesting real epubcheck's Schematron might otherwise
        // object - no scenario enforces that here, so this must stay lenient.
        let xml = concat!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" xmlns:x=\"http://www.w3.org/1999/xhtml\">",
            "<semantics><mrow><mi>a</mi></mrow>",
            "<annotation-xml encoding=\"application/xhtml+xml\" name=\"alternate-representation\">",
            "<x:p>text</x:p><math><mtext>sin(x)+5</mtext></math>",
            "</annotation-xml></semantics></math>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_math_element(math_of(&d), "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }
}
