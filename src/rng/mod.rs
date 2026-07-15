//! A pure-Rust, derivative-based RELAX NG validation engine. It is the reusable
//! core behind the `RSC-005` schema checks: [`package_grammar`] and
//! [`xhtml_grammar`] (loaded from the committed, from-scratch `schemas/*.rng`)
//! back the OPF and XHTML content-model validation in `opf.rs`, and
//! [`validate_node_report`] names *which* node collapsed the model so the
//! finding carries a real `line:column` and element path (issue #17), not just
//! a whole-document verdict. The `container.xml` grammar ([`container_grammar`])
//! is built via the pattern API instead of a schema file.

pub mod datatype;
pub mod derive;
pub mod load;
pub mod pattern;

pub use derive::{Grammar, validate_node, validate_node_report, validate_xml};
pub use load::load;
pub use pattern::*;

/// The OCF container namespace.
pub const CONTAINER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:container";

/// A simplified RELAX NG grammar for `META-INF/container.xml`, built via the
/// pattern API. Covers the structure our hand-coded check relies on: a
/// `container` root (version="1.0") holding a `rootfiles` element with one or
/// more `rootfile` entries that each carry `full-path` and `media-type`.
/// (Optional `links` and foreign content are intentionally omitted for now.)
pub fn container_grammar() -> Grammar {
    let rootfile = element(
        qname(CONTAINER_NS, "rootfile"),
        group(
            attribute(local_name("full-path"), data(Datatype::Token)),
            attribute(local_name("media-type"), data(Datatype::Token)),
        ),
    );
    let rootfiles = element(qname(CONTAINER_NS, "rootfiles"), one_or_more(rootfile));
    Grammar::single(element(
        qname(CONTAINER_NS, "container"),
        group(
            attribute(local_name("version"), value(Datatype::Token, "1.0")),
            rootfiles,
        ),
    ))
}

/// Our own EPUB package-document RNG, embedded at build time (committed under
/// the project license; authored from scratch — not derived from epubcheck/W3C).
pub const PACKAGE_RNG: &str = include_str!("../../schemas/package.rng");

/// Load the built-in package-document grammar.
pub fn package_grammar() -> Grammar {
    load(PACKAGE_RNG).expect("built-in package.rng must parse")
}

/// Our own EPUB XHTML content-document RNG, embedded at build time (committed
/// under the project license; authored from scratch — not derived from
/// epubcheck/W3C). See `schemas/xhtml.rng` for the scope/design notes.
pub const XHTML_RNG: &str = include_str!("../../schemas/xhtml.rng");

/// Load the built-in XHTML content-document grammar.
pub fn xhtml_grammar() -> Grammar {
    load(XHTML_RNG).expect("built-in xhtml.rng must parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_OPF: &str = concat!(
        "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" ",
        "unique-identifier=\"id\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">",
        "<metadata><dc:identifier id=\"id\">x</dc:identifier>",
        "<dc:title>T</dc:title><dc:language>en</dc:language></metadata>",
        "<manifest><item id=\"nav\" href=\"nav.xhtml\" ",
        "media-type=\"application/xhtml+xml\" properties=\"nav\"/></manifest>",
        "<spine><itemref idref=\"nav\"/></spine></package>"
    );

    #[test]
    fn package_grammar_accepts_minimal_opf() {
        assert!(validate_xml(&package_grammar(), MIN_OPF).unwrap());
    }

    #[test]
    fn package_grammar_rejects_item_without_href() {
        let bad = MIN_OPF.replace(" href=\"nav.xhtml\"", "");
        assert!(!validate_xml(&package_grammar(), &bad).unwrap());
    }

    #[test]
    fn package_grammar_rejects_missing_manifest() {
        let bad = concat!(
            "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\">",
            "<metadata/><spine><itemref idref=\"x\"/></spine></package>"
        );
        assert!(!validate_xml(&package_grammar(), bad).unwrap());
    }

    // A tiny grammar to isolate engine correctness from container specifics:
    //   element note { element to { text }, element from { text }? }
    fn note_grammar() -> Grammar {
        let to = element(local_name("to"), text());
        let from = element(local_name("from"), text());
        Grammar::single(element(local_name("note"), group(to, optional(from))))
    }

    fn ok(g: &Grammar, xml: &str) -> bool {
        validate_xml(g, xml).unwrap()
    }

    /// The local name of the node `validate_node_report` blames, or `None` if
    /// the document is valid (issue #17: a failure names *which* node).
    fn fail_local(g: &Grammar, xml: &str) -> Option<String> {
        let doc = roxmltree::Document::parse(xml).unwrap();
        validate_node_report(g, doc.root_element()).map(|n| n.tag_name().name().to_string())
    }

    #[test]
    fn toy_grammar_accepts_valid() {
        let g = note_grammar();
        assert!(ok(&g, "<note><to>x</to></note>"));
        assert!(ok(&g, "<note><to>x</to><from>y</from></note>"));
        // whitespace between elements is ignored
        assert!(ok(&g, "<note>\n  <to>x</to>\n  <from>y</from>\n</note>"));
    }

    #[test]
    fn toy_grammar_rejects_invalid() {
        let g = note_grammar();
        assert!(!ok(&g, "<note></note>")); // missing required <to>
        assert!(!ok(&g, "<note><from>y</from></note>")); // <from> without <to>
        assert!(!ok(&g, "<note><to>x</to><extra/></note>")); // undeclared element
        assert!(!ok(
            &g,
            "<note><to>x</to><from>y</from><from>z</from></note>"
        )); // two <from>
    }

    const CVALID: &str = concat!(
        "<container version=\"1.0\" ",
        "xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">",
        "<rootfiles>",
        "<rootfile full-path=\"OEBPS/content.opf\" ",
        "media-type=\"application/oebps-package+xml\"/>",
        "</rootfiles></container>"
    );

    #[test]
    fn container_grammar_accepts_valid() {
        assert!(ok(&container_grammar(), CVALID));
    }

    #[test]
    fn container_grammar_rejects_bad_version() {
        let xml = CVALID.replace("version=\"1.0\"", "version=\"2.0\"");
        assert!(!ok(&container_grammar(), &xml));
    }

    #[test]
    fn container_grammar_rejects_missing_rootfile_attr() {
        let xml = CVALID.replace(" media-type=\"application/oebps-package+xml\"", "");
        assert!(!ok(&container_grammar(), &xml));
    }

    #[test]
    fn container_grammar_rejects_no_rootfile() {
        let xml = concat!(
            "<container version=\"1.0\" ",
            "xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">",
            "<rootfiles></rootfiles></container>"
        );
        assert!(!ok(&container_grammar(), xml));
    }

    #[test]
    fn container_grammar_rejects_extra_attribute() {
        let xml = CVALID.replace("<rootfiles>", "<rootfiles bogus=\"x\">");
        assert!(!ok(&container_grammar(), &xml));
    }

    const XHTML_NS_DECLS: &str = concat!(
        "xmlns=\"http://www.w3.org/1999/xhtml\" ",
        "xmlns:epub=\"http://www.idpf.org/2007/ops\""
    );

    fn xhtml_doc(body: &str) -> String {
        format!(
            "<html {XHTML_NS_DECLS}><head><title>T</title><meta charset=\"utf-8\"/></head>\
             <body>{body}</body></html>"
        )
    }

    #[test]
    fn xhtml_grammar_accepts_valid_content_doc() {
        let xml = xhtml_doc("<p epub:type=\"chapter\">Hello <em>world</em>.</p>");
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_obsolete_element() {
        let xml = xhtml_doc("<keygen/>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn report_names_the_offending_node_not_the_root() {
        // issue #17: a content-model failure must point at *which* node
        // collapsed the model, so the RSC-005 gets a real line:column.
        // Valid → no blame.
        assert_eq!(fail_local(&xhtml_grammar(), &xhtml_doc("<p>ok</p>")), None);
        // A `<span>` where the content model does not allow it (inside `<ol>`,
        // which takes list items) is blamed at the span itself, not `<html>`.
        assert_eq!(
            fail_local(&xhtml_grammar(), &xhtml_doc("<ol><span>x</span></ol>")).as_deref(),
            Some("span")
        );
        // An obsolete element is blamed at itself.
        assert_eq!(
            fail_local(&xhtml_grammar(), &xhtml_doc("<keygen/>")).as_deref(),
            Some("keygen")
        );
    }

    #[test]
    fn xhtml_grammar_rejects_obsolete_attribute() {
        let xml = xhtml_doc("<p contextmenu=\"x\">hi</p>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_style_in_body() {
        let xml = xhtml_doc("<style>p{color:red}</style>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_epub_switch_case_then_default() {
        let xml = xhtml_doc(concat!(
            "<epub:switch><epub:case required-namespace=\"http://www.w3.org/1998/Math/MathML\">",
            "<p>case</p></epub:case><epub:default><p>default</p></epub:default></epub:switch>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_epub_switch_default_before_case() {
        let xml = xhtml_doc(concat!(
            "<epub:switch><epub:default><p>default</p></epub:default>",
            "<epub:case required-namespace=\"http://www.w3.org/1998/Math/MathML\">",
            "<p>case</p></epub:case></epub:switch>"
        ));
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_epub_switch_multiple_defaults() {
        let xml = xhtml_doc(concat!(
            "<epub:switch><epub:default><p>a</p></epub:default>",
            "<epub:default><p>b</p></epub:default></epub:switch>"
        ));
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_time_nested_in_time() {
        let xml = xhtml_doc("<p><time>outer<time>inner</time></time></p>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_invalid_table_border() {
        let xml = xhtml_doc("<table border=\"5\"><tr><td>x</td></tr></table>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_valid_table_border() {
        let xml = xhtml_doc("<table border=\"1\"><tr><td>x</td></tr></table>");
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_epub_type_on_meta() {
        let xml = "<html ".to_string()
            + XHTML_NS_DECLS
            + "><head><title>T</title>\
               <meta epub:type=\"toc\" charset=\"utf-8\"/></head><body/></html>";
        assert!(!ok(&xhtml_grammar(), &xml));
    }
}
