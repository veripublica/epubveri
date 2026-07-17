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

pub use derive::{Blame, ElementFault, Grammar, validate_node, validate_node_report, validate_xml};
pub use load::{load, load_from_define};
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

/// Load the built-in **EPUB 3** XHTML (HTML5) content-document grammar.
pub fn xhtml_grammar() -> Grammar {
    load(XHTML_RNG).expect("built-in xhtml.rng must parse")
}

/// Load the built-in **EPUB 2** (XHTML 1.1 + OPS 2.0.1) content-document
/// grammar - the same schema, entered at its EPUB 2 root so it shares all the
/// version-independent machinery and differs only in the element pool (issue
/// #24). See the EPUB 2 section of `schemas/xhtml.rng`.
pub fn xhtml_grammar_epub2() -> Grammar {
    load_from_define(XHTML_RNG, "htmlEl-epub2").expect("built-in xhtml.rng epub2 root must parse")
}

#[cfg(test)]
mod tests {
    #[test]
    fn epub2_grammar_probe() {
        let g = crate::rng::xhtml_grammar_epub2();
        let doc = |body: &str| {
            format!(
                "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head><body>{body}</body></html>"
            )
        };
        let cases = [
            (
                "big (valid XHTML1.1, removed HTML5)",
                "<p>x <big>b</big> y</p>",
                true,
            ),
            ("tt", "<p><tt>code</tt></p>", true),
            ("acronym", "<p><acronym>WWW</acronym></p>", true),
            (
                "font (invalid)",
                "<p><font color=\"red\">x</font></p>",
                false,
            ),
            (
                "s (valid HTML5, invalid XHTML1.1)",
                "<p><s>x</s></p>",
                false,
            ),
            (
                "u (valid HTML5, invalid XHTML1.1)",
                "<p><u>x</u></p>",
                false,
            ),
            ("strike (invalid)", "<p><strike>x</strike></p>", false),
            ("center (invalid)", "<center><p>x</p></center>", false),
            ("section (HTML5 only)", "<section><p>x</p></section>", false),
            ("nav (HTML5 only)", "<nav><p>x</p></nav>", false),
            ("audio (HTML5 only)", "<p><audio src=\"a.mp3\"/></p>", false),
            (
                "ordinary p/em/strong",
                "<p>Hello <em>world</em> and <strong>bold</strong>.</p>",
                true,
            ),
            ("table", "<table><tr><td>c</td></tr></table>", true),
            ("ol>li", "<ol><li>x</li></ol>", true),
        ];
        for (label, body, want_valid) in cases {
            let xml = doc(body);
            let d = crate::ocf::parse_xml(&xml).unwrap();
            let bl = crate::rng::validate_node_report(&g, d.root_element());
            let valid = bl.is_empty();
            let mark = if valid == want_valid { "OK " } else { "XX " };
            eprintln!("{mark}[{}] valid={valid} (want {want_valid})", label);
        }
    }

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

    /// Every node `validate_node_report` blames, as an element's local name or
    /// `@name` for an attribute, in document order — empty if the document is
    /// valid (issues #17/#18: name *which* nodes, pin attributes, and report
    /// *all* of them, not just the first).
    fn fail_locals(g: &Grammar, xml: &str) -> Vec<String> {
        let doc = roxmltree::Document::parse(xml).unwrap();
        validate_node_report(g, doc.root_element())
            .into_iter()
            .map(|b| match b {
                Blame::Element(n, _) => n.tag_name().name().to_string(),
                Blame::Attribute(_, a) => format!("@{}", a.name()),
            })
            .collect()
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

    #[test]
    fn blame_describe_names_the_offending_node() {
        let doc = roxmltree::Document::parse("<ol a=\"1\"><p>x</p></ol>").unwrap();
        let ol = doc.root_element();
        let p = ol.children().find(|n| n.is_element()).unwrap();
        let a = ol.attributes().next().unwrap();

        let cases: [(Blame, &str); 5] = [
            (
                Blame::Element(p, ElementFault::NotAllowed(Vec::new())),
                "element \"p\" is not allowed here",
            ),
            (
                Blame::Element(ol, ElementFault::TextNotAllowed),
                "character data is not allowed in element \"ol\"",
            ),
            (
                Blame::Element(ol, ElementFault::MissingAttribute),
                "element \"ol\" is missing a required attribute",
            ),
            (
                Blame::Element(ol, ElementFault::IncompleteContent),
                "element \"ol\" has incomplete content",
            ),
            (
                Blame::Attribute(ol, a),
                "attribute \"a\" is not allowed here",
            ),
        ];
        for (blame, want) in &cases {
            let (text, params) = blame.describe();
            assert_eq!(text, *want);
            // the offending name is also surfaced as a structured param
            assert_eq!(params.len(), 1);
        }
        // accessor sanity: attribute-level blame exposes both node and attr
        assert!(cases[4].0.attribute().is_some());
        assert_eq!(cases[4].0.node(), ol);
        assert!(cases[0].0.attribute().is_none());
        assert_eq!(cases[0].0.node(), p);
    }

    /// The message text actually reaches the RSC-005 finding: a stray `<p>`
    /// directly in `<ol>` names the element, not a blanket "does not conform"
    /// (forum #78).
    #[test]
    fn toy_grammar_blame_message_names_element() {
        let g = note_grammar();
        let doc = roxmltree::Document::parse("<note><to>x</to><extra/></note>").unwrap();
        let blames = validate_node_report(&g, doc.root_element());
        let (text, _) = blames[0].describe();
        // Tier-C: the toy `note` model expects `from` at this position, so
        // the message names it.
        assert_eq!(
            text,
            "element \"extra\" is not allowed here; expected \"from\""
        );
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

    /// Tier-C: a "not allowed here" finding names what *would* have fit when
    /// the position's model is tight enough for the list to be a real
    /// constraint. `<html>` wants `head` then `body`, so a document that puts
    /// `body` where `head` belongs gets told exactly that.
    #[test]
    fn not_allowed_names_the_expected_element_when_the_set_is_small() {
        let g = xhtml_grammar();
        let xml = format!("<html {XHTML_NS_DECLS}><body/></html>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let blames = validate_node_report(&g, doc.root_element());
        let texts: Vec<String> = blames.iter().map(|b| b.describe().0).collect();
        assert!(
            texts.iter().any(|t| t.contains("expected \"head\"")),
            "expected a \"head\" suggestion; got {texts:?}"
        );
        // The suggestion also travels as structured params, for machine
        // consumers and i18n.
        let params: Vec<String> = blames.iter().flat_map(|b| b.describe().1).collect();
        assert!(params.iter().any(|p| p == "head"), "got {params:?}");
    }

    /// ...and stays silent when the model is permissive. Our grammar shares
    /// one large pool for flow content, so `<ul><div>` sits at a position that
    /// admits 80-odd names - not a suggestion anyone can use, so the bare
    /// message stands rather than dumping the pool.
    #[test]
    fn not_allowed_omits_the_list_when_the_set_is_huge() {
        let g = xhtml_grammar();
        let xml = xhtml_doc("<ul><div>x</div></ul>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        for b in validate_node_report(&g, doc.root_element()) {
            let (text, _) = b.describe();
            if text.contains("\"div\"") {
                assert!(
                    !text.contains("expected"),
                    "a permissive position must not list its pool; got: {text}"
                );
            }
        }
    }

    /// The suggestion order is deterministic - sorted, not
    /// pattern-traversal order - so the message never changes between runs.
    #[test]
    fn expected_list_is_sorted_and_deduplicated() {
        assert_eq!(
            super::derive::one_of(&["td".to_string(), "th".to_string()]),
            "one of \"td\", \"th\""
        );
        assert_eq!(super::derive::one_of(&["head".to_string()]), "\"head\"");
    }

    /// EPUB 2 (XHTML 1.1 + OPS 2.0.1) vocabulary differs from HTML5 in both
    /// directions, and this is issue #24's whole point (Doitsu, MobileRead).
    /// `big`/`tt`/`acronym` are valid here but removed in HTML5; `s`/`u` and
    /// every HTML5 addition are the reverse. Both fall out of the vocabulary
    /// with no per-element code.
    #[test]
    fn epub2_grammar_matches_the_xhtml11_vocabulary() {
        let g = xhtml_grammar_epub2();
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let ok2 = |b: &str| {
            let x = doc(b);
            validate_node_report(&g, roxmltree::Document::parse(&x).unwrap().root_element())
                .is_empty()
        };
        // Valid in XHTML 1.1, removed in HTML5 - false positives before #24.
        for b in [
            "<p><big>b</big></p>",
            "<p><tt>c</tt></p>",
            "<p><acronym>W</acronym></p>",
        ] {
            assert!(ok2(b), "should be valid in EPUB 2: {b}");
        }
        // Invalid in XHTML 1.1. `s`/`u` are valid HTML5, which is exactly the
        // false negative Doitsu reported; the rest are invalid in both.
        for b in [
            "<p><font color=\"red\">x</font></p>",
            "<p><s>x</s></p>",
            "<p><u>x</u></p>",
            "<p><strike>x</strike></p>",
            "<center><p>x</p></center>",
            // HTML5 additions, none in OPS 2.0.1.
            "<section><p>x</p></section>",
            "<nav><p>x</p></nav>",
            "<p><mark>x</mark></p>",
            "<figure><p>x</p></figure>",
        ] {
            assert!(!ok2(b), "should be invalid in EPUB 2: {b}");
        }
        // Ordinary content the two versions share stays valid.
        for b in [
            "<p>Hi <em>there</em> <strong>bold</strong>.</p>",
            "<ol><li>a</li></ol>",
            "<table><tr><td>c</td></tr></table>",
            "<blockquote><p>q</p></blockquote>",
        ] {
            assert!(ok2(b), "should be valid in EPUB 2: {b}");
        }
    }

    /// A rejected container is not the end of the story: recovery descends
    /// into it and reports the bad elements nested inside, too. Doitsu\'s
    /// case is an obsolete `<center>` wrapping obsolete `<font>`/`<s>`/… -
    /// epubcheck names each, and reporting only the `<center>` would hide the
    /// rest (issue #24). The container\'s own loose text is not re-reported,
    /// though - it went down with the container.
    #[test]
    fn recovery_descends_into_a_rejected_container() {
        let g = xhtml_grammar_epub2();
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>\
             <center><p>text <font>x</font> and <s>y</s></p></center></body></html>"
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let named: Vec<String> = validate_node_report(&g, doc.root_element())
            .into_iter()
            .filter_map(|b| match b {
                Blame::Element(n, ElementFault::NotAllowed(_)) => {
                    Some(n.tag_name().name().to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            named,
            ["center", "font", "s"],
            "the container and its bad contents"
        );
    }

    /// The flip side: descending must not re-report the rejected container\'s
    /// text as a loose-text error. `<ol><span>x</span></ol>` blames the
    /// `<span>` once - not a second time for the `x` inside it.
    #[test]
    fn recovery_descent_does_not_double_report_text() {
        assert_eq!(
            fail_locals(&xhtml_grammar(), &xhtml_doc("<ol><span>x</span></ol>")),
            ["span"]
        );
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
        assert!(fail_locals(&xhtml_grammar(), &xhtml_doc("<p>ok</p>")).is_empty());
        // A `<span>` where the content model does not allow it (inside `<ol>`,
        // which takes list items) is blamed at the span itself, not `<html>`.
        assert_eq!(
            fail_locals(&xhtml_grammar(), &xhtml_doc("<ol><span>x</span></ol>")),
            ["span"]
        );
        // An obsolete element is blamed at itself.
        assert_eq!(
            fail_locals(&xhtml_grammar(), &xhtml_doc("<keygen/>")),
            ["keygen"]
        );
        // An attribute-level violation pins the attribute itself (#18), so the
        // finding can target `@name` rather than only the containing element.
        assert_eq!(
            fail_locals(&xhtml_grammar(), &xhtml_doc("<p contextmenu=\"x\">hi</p>")),
            ["@contextmenu"]
        );
    }

    #[test]
    fn report_lists_every_offending_node_not_just_the_first() {
        // Doitsu's MobileRead case: two <p> where <li> belongs. Recovery must
        // report *both*, not stop at the first (issues #17/#18). The `<ol>`
        // itself isn't flagged — an empty list is valid, so the errors are the
        // two misplaced children, exactly what epubcheck points at.
        assert_eq!(
            fail_locals(
                &xhtml_grammar(),
                &xhtml_doc("<ol><p>one</p><p>two</p></ol>")
            ),
            ["p", "p"]
        );
        // A stray element amid otherwise-valid siblings is reported without
        // dragging the valid ones (or the container) down with it.
        assert_eq!(
            fail_locals(
                &xhtml_grammar(),
                &xhtml_doc("<ol><li>a</li><p>bad</p><li>c</li></ol>")
            ),
            ["p"]
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
