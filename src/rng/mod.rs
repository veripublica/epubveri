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

pub use derive::{
    AttributeFault, Blame, ElementFault, Grammar, validate_node, validate_node_report, validate_xml,
};
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

/// Load the built-in **EPUB 2** package-document grammar - the same schema
/// entered at its EPUB 2 root, mirroring how the XHTML grammar splits (#63).
pub fn package_grammar_epub2() -> Grammar {
    load_from_define(PACKAGE_RNG, "packageEl-epub2")
        .expect("built-in package.rng epub2 root must parse")
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

    /// XHTML 1.1 builds `html`/`head`/`title` from `I18n.attrib` alone -
    /// no `Common.attrib`, so no `id`, `class`, `style` or `title` on any
    /// of the three. `<html class="calibre">` is calibre's own output and
    /// draws an RSC-005 from epubcheck (Doitsu, MobileRead #138); we were
    /// granting the EPUB 3 global set there.
    #[test]
    fn epub2_html_head_and_title_take_only_the_i18n_attributes() {
        let g = crate::rng::xhtml_grammar_epub2();
        let valid = |xml: &str| {
            let d = crate::ocf::parse_xml(xml).unwrap();
            crate::rng::validate_node_report(&g, d.root_element()).is_empty()
        };
        let doc = |html_attrs: &str, head_attrs: &str, title_attrs: &str| {
            format!(
                "<html xmlns=\"http://www.w3.org/1999/xhtml\"{html_attrs}>\
                 <head{head_attrs}><title{title_attrs}>t</title></head>\
                 <body><p>x</p></body></html>"
            )
        };

        // I18n.attrib (xml:lang, lang, and `dir` interleaved in by
        // bdo.rng), plus each element's own extra: `version` on html,
        // `profile` on head.
        assert!(valid(&doc(
            " lang=\"en\" xml:lang=\"en\" dir=\"ltr\" version=\"-//W3C//DTD XHTML 1.1//EN\"",
            " profile=\"http://example.org/p\" lang=\"en\"",
            " xml:lang=\"en\""
        )));

        // Common.attrib members, which none of the three inherit.
        for attrs in [" class=\"calibre\"", " id=\"top\"", " style=\"color:red\""] {
            assert!(!valid(&doc(attrs, "", "")), "html{attrs} must be rejected");
            assert!(!valid(&doc("", attrs, "")), "head{attrs} must be rejected");
            assert!(!valid(&doc("", "", attrs)), "title{attrs} must be rejected");
        }

        // Not the wider #66 tightening: `class` stays valid everywhere else
        // in the EPUB 2 pool, since those elements do take Common.attrib.
        assert!(valid(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
             <body class=\"b\"><p class=\"c\">x</p></body></html>"
        ));
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
                // Named for the parent it sits in, since a text run has no
                // name of its own - "text(in ol)" reads as what it is.
                Blame::Text(n) => format!(
                    "text(in {})",
                    n.parent().map_or("?", |p| p.tag_name().name())
                ),
                Blame::Attribute(_, a, _) => format!("@{}", a.name()),
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
        let doc = roxmltree::Document::parse("<ol a=\"1\">loose<p>x</p></ol>").unwrap();
        let ol = doc.root_element();
        let p = ol.children().find(|n| n.is_element()).unwrap();
        let loose = ol.children().find(roxmltree::Node::is_text).unwrap();
        let a = ol.attributes().next().unwrap();

        let cases: [(Blame, &str); 5] = [
            (
                Blame::Element(p, ElementFault::NotAllowed(Vec::new())),
                "element \"p\" is not allowed here",
            ),
            (
                // #68: the blame carries the text run, and the message still
                // names the parent - the two are different nodes on purpose.
                Blame::Text(loose),
                "stray text is not allowed directly in \"ol\"; wrap it in an element",
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
                Blame::Attribute(ol, a, AttributeFault::NotAllowed),
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
        "xmlns:epub=\"http://www.idpf.org/2007/ops\" ",
        // `epub:trigger`'s attributes live here (#161). An unused namespace
        // declaration is inert, so every other test is unaffected.
        "xmlns:ev=\"http://www.w3.org/2001/xml-events\""
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

    /// #43 (Doitsu): `<img width="50%">` is the XHTML 1.1 `Length` datatype
    /// (pixels or percentage), which epubcheck's schema/20 types as free
    /// text - so it is valid in EPUB 2 and must not draw RSC-005. The HTML5
    /// integer rule still applies in EPUB 3, where a percentage is an error.
    #[test]
    fn img_percentage_dimensions_epub2_only() {
        let body = "<p><img src=\"c.png\" alt=\"c\" width=\"50%\" height=\"100%\"/></p>";
        let doc = |ns: &str, b: &str| {
            format!("<html {ns}><head><title>t</title></head><body>{b}</body></html>")
        };
        // EPUB 2: percentage is fine.
        let g2 = xhtml_grammar_epub2();
        let x2 = doc(XHTML_NS_DECLS, body);
        assert!(
            validate_node_report(&g2, roxmltree::Document::parse(&x2).unwrap().root_element())
                .is_empty(),
            "EPUB 2 img width/height percentage must be accepted"
        );
        // EPUB 3: percentage is an error (integer only).
        let g3 = xhtml_grammar();
        let x3 = doc(XHTML_NS_DECLS, body);
        assert!(
            !validate_node_report(&g3, roxmltree::Document::parse(&x3).unwrap().root_element())
                .is_empty(),
            "EPUB 3 img width/height percentage must be rejected"
        );
        // A plain integer stays valid in both.
        let intbody = "<p><img src=\"c.png\" alt=\"c\" width=\"50\" height=\"100\"/></p>";
        let xi = doc(XHTML_NS_DECLS, intbody);
        assert!(
            validate_node_report(&g3, roxmltree::Document::parse(&xi).unwrap().root_element())
                .is_empty(),
            "EPUB 3 img integer dimensions must stay valid"
        );
    }

    /// HTML5 types `id` as `datatype.html5.token` — one or more characters,
    /// none of them whitespace — where we had it as free text. An empty
    /// `id=""` and one containing a space are the two shapes that catches.
    ///
    /// XHTML 1.1 is stricter still (`xsd:ID`, an NCName), but `globalAttrs`
    /// is shared by both versions and this applies the looser rule to both.
    /// Every valid NCName is also a valid html5 token, so EPUB 2 can only
    /// under-report here, never gain a wrong error — the `id="1"` case below
    /// pins that direction deliberately.
    #[test]
    fn id_must_be_a_non_empty_whitespace_free_token() {
        let doc = |id: &str| {
            format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
                 <body><p id=\"{id}\">x</p></body></html>"
            )
        };
        let ok = |id: &str| {
            validate_node_report(
                &xhtml_grammar(),
                roxmltree::Document::parse(&doc(id)).unwrap().root_element(),
            )
            .is_empty()
        };
        assert!(ok("ok-id"), "an ordinary id");
        assert!(ok("_x.y-z"), "punctuation HTML5 permits");
        assert!(
            ok("1"),
            "HTML5 allows a digit-initial id; only XHTML 1.1 does not"
        );
        assert!(!ok("a b"), "whitespace is not allowed in an id");
        assert!(!ok(""), "an empty id is not allowed");
    }

    /// #58, OPF round: `<tours>` is a legacy OPF 2.0 package child that
    /// `opf20.rng` lists explicitly in `OPF20.package-content`. It had been
    /// left out because no corpus fixture exercises it - the wrong test: the
    /// corpus not covering something says nothing about what the schema
    /// allows, and a real legacy EPUB 2 book using it was being rejected.
    #[test]
    fn legacy_tours_package_child_is_accepted() {
        let with = MIN_OPF.replace(
            "<spine><itemref idref=\"nav\"/></spine>",
            "<spine><itemref idref=\"nav\"/></spine>\
             <tours><tour id=\"t\" title=\"T\"><site title=\"s\" href=\"a.xhtml\"/></tour></tours>",
        );
        assert!(
            validate_xml(&package_grammar(), &with).unwrap(),
            "opf20.rng allows tours as a package child"
        );
        // An unrecognised OPF-namespaced child is still a violation - the
        // point is that `tours` is real, not that the model went loose.
        let bogus = MIN_OPF.replace(
            "<spine><itemref idref=\"nav\"/></spine>",
            "<spine><itemref idref=\"nav\"/></spine><hello/>",
        );
        assert!(!validate_xml(&package_grammar(), &bogus).unwrap());
    }

    /// MathML 3 Presentation content models. The arity rules are the point:
    /// `mfrac` takes exactly two children, `msubsup` exactly three, and rows
    /// and cells exist only inside their container.
    ///
    /// This work was deferred twice for want of cover - epubcheck's 12 valid
    /// MathML fixtures touch about 11 of 39 elements, all in trivial shapes.
    /// It became measurable once two independently-produced real books
    /// carrying 257k live MathML elements reached the shelf, and *those* are
    /// the real test: a wrong arity rule rejects thousands of equations
    /// there. Both must stay clean.
    #[test]
    fn mathml_presentation_content_models() {
        const NS: &str = "xmlns=\"http://www.w3.org/1998/Math/MathML\"";
        let doc = |m: &str| {
            format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
                 <body><p><math {NS}>{m}</math></p></body></html>"
            )
        };
        let ok = |m: &str| {
            validate_node_report(
                &xhtml_grammar(),
                roxmltree::Document::parse(&doc(m)).unwrap().root_element(),
            )
            .is_empty()
        };
        for good in [
            "<mfrac><mi>a</mi><mi>b</mi></mfrac>",
            "<msubsup><mi>a</mi><mi>b</mi><mi>c</mi></msubsup>",
            "<munderover><mo>x</mo><mi>i</mi><mi>n</mi></munderover>",
            "<mtable><mtr><mtd><mi>x</mi></mtd></mtr></mtable>",
            "<mrow></mrow>",
            "<msqrt><mi>a</mi><mi>b</mi></msqrt>",
            "<mmultiscripts><mi>R</mi><mi>i</mi><none/></mmultiscripts>",
            "<mstack><msrow><mn>1</mn></msrow><msline/></mstack>",
            "<semantics><mrow><mi>a</mi></mrow><annotation>x</annotation></semantics>",
            "<mtext>a<mglyph/>b</mtext>",
        ] {
            assert!(ok(good), "must be accepted: {good}");
        }
        for bad in [
            "<mfrac><mi>a</mi></mfrac>",
            "<mfrac><mi>a</mi><mi>b</mi><mi>c</mi></mfrac>",
            "<msubsup><mi>a</mi><mi>b</mi></msubsup>",
            "<mtable><mi>x</mi></mtable>",
            "<mtr><mtd><mi>x</mi></mtd></mtr>",
            "<mtd><mi>x</mi></mtd>",
            "<maction></maction>",
            "<mspace><mi>x</mi></mspace>",
        ] {
            assert!(!ok(bad), "must be rejected: {bad}");
        }
    }

    /// A missing `<title>` is an RSC-005 *error* in EPUB 2 and an RSC-017
    /// *warning* in EPUB 3, and the difference is in the schemas rather than
    /// in taste: XHTML 1.1's `head.content` is simply `title`, so the grammar
    /// requires it, while epubcheck's EPUB 3 rule is a Schematron assertion
    /// whose message begins with "WARNING:" — a prefix its error handler maps
    /// to RSC-017. We had the EPUB 3 behaviour on both.
    #[test]
    fn head_requires_a_title_only_on_epub2() {
        let head_only = |g: &Grammar| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&format!(
                    "<html {XHTML_NS_DECLS}><head></head><body><p>x</p></body></html>"
                ))
                .unwrap()
                .root_element(),
            )
            .is_empty()
        };
        assert!(
            !head_only(&xhtml_grammar_epub2()),
            "EPUB 2 requires <title>"
        );
        assert!(
            head_only(&xhtml_grammar()),
            "EPUB 3 leaves it to the RSC-017 warning, so the grammar accepts it"
        );
        // A head that has one is fine in both, and the rest of its contents
        // are unaffected by the split.
        let full = |g: &Grammar| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&format!(
                    "<html {XHTML_NS_DECLS}><head><title>t</title>\
                     <meta name=\"a\" content=\"b\"/><style>p{{}}</style></head>\
                     <body><p>x</p></body></html>"
                ))
                .unwrap()
                .root_element(),
            )
            .is_empty()
        };
        assert!(full(&xhtml_grammar_epub2()));
        assert!(full(&xhtml_grammar()));
    }

    /// #60: an element with incomplete content used to stop its *siblings*
    /// from being checked, so a body of four empty containers reported one
    /// error where epubcheck reports four.
    ///
    /// The recovery takes the continuation the derivative already holds —
    /// `end_tag_deriv` turns `After(content, rest)` into `rest` only when
    /// `content` is nullable, and this takes `rest` regardless — so it
    /// resumes at exactly the right position rather than guessing one. The
    /// second half of this test is the guard that matters: one failure must
    /// still produce exactly one finding, because a resume point that is even
    /// slightly wrong shows up as a cascade of invented siblings.
    #[test]
    fn incomplete_content_does_not_stop_the_siblings() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let count = |b: &str| {
            validate_node_report(
                &xhtml_grammar_epub2(),
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .len()
        };
        assert_eq!(
            count("<ol></ol><ul></ul><table></table><dl></dl>"),
            4,
            "each independent failure reported once"
        );
        // One failure stays one finding - no cascade into what follows it.
        assert_eq!(count("<ol></ol><p>fine</p><p>also fine</p>"), 1);
        assert_eq!(count("<p>fine</p><ol></ol><p>also fine</p>"), 1);
        // And valid siblings after a failure are still *checked*, not merely
        // tolerated: an obsolete element following one must still be caught.
        assert_eq!(count("<ol></ol><p><font>x</font></p>"), 2);
    }

    /// XHTML 1.1 requires content in `ol`/`ul` (`oneOrMore li`), `dl`
    /// (`oneOrMore (dt|dd)`) and `table` (its model ends in `tbody+ | tr+`),
    /// so an empty one is an error — reported by Doitsu, MobileRead #126.
    ///
    /// **HTML5 relaxed all four to zero-or-more**, so the EPUB 3 grammar must
    /// stay permissive. Making this version-wide would have traded a missed
    /// error for a false positive, which is the worse trade; the EPUB 3 half
    /// of this test is the guard against that.
    #[test]
    fn empty_lists_and_tables_are_an_error_only_on_epub2() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let ok = |g: &Grammar, b: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .is_empty()
        };
        let two = xhtml_grammar_epub2();
        let three = xhtml_grammar();
        for empty in ["<ol></ol>", "<ul></ul>", "<dl></dl>", "<table></table>"] {
            assert!(!ok(&two, empty), "EPUB 2 rejects {empty}");
            assert!(ok(&three, empty), "EPUB 3 accepts {empty}");
        }
        // Populated ones stay valid in both.
        //
        // The columns-after-rows shape used to be asserted valid here, on the
        // grounds that this rule was cardinality and not sequence. #48 made
        // the table model ordered, and epubcheck agrees: it rejects
        // `<table><tr/><colgroup/></table>` in *both* versions. The assertion
        // was encoding our own permissive stance rather than epubcheck's
        // behaviour, so it moved to the rejected list below.
        for full in [
            "<ol><li>x</li></ol>",
            "<ul><li>x</li></ul>",
            "<dl><dt>t</dt><dd>d</dd></dl>",
            "<table><tr><td>v</td></tr></table>",
            "<table><colgroup><col/></colgroup><tr><td>v</td></tr></table>",
        ] {
            assert!(ok(&two, full), "EPUB 2 accepts {full}");
            assert!(ok(&three, full), "EPUB 3 accepts {full}");
        }
        let cols_after = "<table><tr><td>v</td></tr><colgroup><col/></colgroup></table>";
        assert!(!ok(&two, cols_after), "EPUB 2 rejects columns after rows");
        assert!(!ok(&three, cols_after), "EPUB 3 rejects columns after rows");
    }

    /// #65: a rejected element nested inside another rejected element must
    /// still be named. #60 fixed the flat-sibling case by taking the
    /// `After(content, rest)` continuation; the issue's remaining half was
    /// that nesting needs recovery to resume at more than one level.
    ///
    /// **Attempted reproduction 2026-08-04 and could not:** three shapes —
    /// plain nesting, a `<nav>` inside an `<li>` inside a `<nav>`, and a body
    /// whose every child is a rejected `<nav>` — all name every occurrence
    /// that exists. epubcheck reports roughly twice the number of elements
    /// present in the same files, which is its double-reporting inside an
    /// invalid container, not us under-reporting. The issue's "five reported,
    /// two by us" predates #60 and the `Block.model` body fix.
    ///
    /// Pinned rather than closed on a guess: if the recovery ever stops
    /// descending again, this fails.
    #[test]
    fn every_nested_rejected_element_is_named() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let two = xhtml_grammar_epub2();
        let count = |b: &str| {
            validate_node_report(
                &two,
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .iter()
            .filter(|b| b.describe().0.contains(r#"element "nav" is not allowed"#))
            .count()
        };

        // Two levels, twice over: four <nav>, four reports.
        assert_eq!(
            count("<nav><nav><p>a</p></nav></nav><p>x</p><nav><nav><p>b</p></nav></nav>"),
            4,
            "flat and nested together"
        );
        // A rejected element buried inside a valid one inside a rejected one.
        assert_eq!(
            count("<nav><ol><li><nav><p>a</p></nav></li></ol></nav>"),
            2,
            "nesting through a valid intermediate"
        );
        // Three levels deep.
        assert_eq!(
            count("<nav><nav><nav><p>a</p></nav></nav></nav>"),
            3,
            "three levels"
        );
    }

    /// #48: table row groups are ORDERED, and the two versions want opposite
    /// orders. XHTML 1.1's model ends in `thead?, tfoot?, tbody+`; HTML5's is
    /// `thead?, (tbody* | tr+), tfoot?`. So `<thead><tfoot><tbody>` is the
    /// only valid arrangement in EPUB 2 and `<thead><tbody><tfoot>` the only
    /// one in EPUB 3 — a shape that is valid in one version is an error in
    /// the other.
    ///
    /// This is a deliberate exception to the schema's "permissive on nesting
    /// order" stance, and it was taken only because the input space is small
    /// enough to settle by enumeration rather than by argument: all six
    /// permutations were built as books in both versions and handed to
    /// epubcheck 5.3.0, twelve runs, and our answers now match all twelve.
    ///
    /// No book on the 83-title shelf has a table carrying both `tfoot` and
    /// `tbody`, so the shelf cannot judge this either way — the enumeration
    /// is the evidence, not the shelf's silence.
    #[test]
    fn table_row_groups_are_ordered_and_the_orders_differ_by_version() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let two = xhtml_grammar_epub2();
        let three = xhtml_grammar();
        let ok = |g: &Grammar, b: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .is_empty()
        };
        let head = "<thead><tr><td>h</td></tr></thead>";
        let body = "<tbody><tr><td>b</td></tr></tbody>";
        let foot = "<tfoot><tr><td>f</td></tr></tfoot>";
        let table =
            |parts: [&str; 3]| format!("<table>{}{}{}</table>", parts[0], parts[1], parts[2]);

        // The one valid arrangement per version — and each is invalid in the
        // other, which is the half that a single-version test would miss.
        let xhtml11 = table([head, foot, body]);
        let html5 = table([head, body, foot]);
        assert!(ok(&two, &xhtml11), "EPUB 2 accepts thead,tfoot,tbody");
        assert!(!ok(&three, &xhtml11), "EPUB 3 rejects thead,tfoot,tbody");
        assert!(ok(&three, &html5), "EPUB 3 accepts thead,tbody,tfoot");
        assert!(!ok(&two, &html5), "EPUB 2 rejects thead,tbody,tfoot");

        // Every other permutation is invalid in both.
        for parts in [
            [body, head, foot],
            [body, foot, head],
            [foot, head, body],
            [foot, body, head],
        ] {
            let t = table(parts);
            assert!(!ok(&two, &t), "EPUB 2 rejects {t}");
            assert!(!ok(&three, &t), "EPUB 3 rejects {t}");
        }

        // Shapes with no row groups at all are untouched by the ordering.
        for t in [
            "<table><tr><td>v</td></tr></table>",
            "<table><caption>c</caption><tr><td>v</td></tr></table>",
        ] {
            assert!(ok(&two, t), "EPUB 2 accepts {t}");
            assert!(ok(&three, t), "EPUB 3 accepts {t}");
        }
    }

    /// #66, first slice: the nine HTML5-only global attributes that XHTML 1.1
    /// does not have. Chosen by measurement, not by reading — each of the 213
    /// attributes our shared global set grants was put on its own `<p>` in one
    /// EPUB 2 book and handed to epubcheck 5.3.0, which rejects 56: these nine
    /// and the 47 `aria-*`. It *accepts* all 80 event handlers, the whole ITS
    /// set, microdata, most of RDFa and `role`, so the "209 extra" figure this
    /// project carried was wrong about both the size and the composition.
    ///
    /// `aria-*` is deliberately still granted — a separate, larger decision.
    ///
    /// **Two of the nine are per-element attributes in XHTML 1.1, and removing
    /// them from the globals broke those elements.** `content` is on `<meta>`
    /// (meta.rng) — ten false positives on one shelf book before it was put
    /// back — and `accesskey` is on `<a>` and `<area>` (hypertext.rng,
    /// csismap.rng, both included by content.rng; the form modules that also
    /// declare it are not). "Not global" and "not valid anywhere" are
    /// different questions, and a probe that puts an attribute on a `<p>` only
    /// answers the first. The shelf caught `content` and could not have caught
    /// `accesskey`: no book on it uses `<a accesskey>`.
    #[test]
    fn epub2_drops_the_html5_only_globals() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let two = xhtml_grammar_epub2();
        let three = xhtml_grammar();
        let ok = |g: &Grammar, b: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .is_empty()
        };

        for a in [
            "about",
            "accesskey",
            "autocapitalize",
            "autofocus",
            "content",
            "contenteditable",
            "datatype",
            "draggable",
            "enterkeyhint",
        ] {
            let el = format!(r#"<p {a}="x">t</p>"#);
            assert!(!ok(&two, &el), "EPUB 2 rejects {a} as a global");
            assert!(ok(&three, &el), "EPUB 3 accepts {a}");
        }

        // Still granted where XHTML 1.1 actually declares them.
        assert!(
            ok(&two, r##"<p><a href="#x" accesskey="k">t</a></p>"##),
            "EPUB 2 keeps accesskey on <a>"
        );

        // ARIA and `role` went with them in the end - see
        // `xhtml_grammar_epub2_has_no_role_or_aria`. The event handlers are
        // the clearest case: `content.rng` never includes `events.rng`, the
        // same exclusion that removes Forms, so OPS 2.0.1 has no
        // event-handler attributes at all.
        assert!(
            !ok(&two, r#"<p role="doc-footnote" aria-label="x">t</p>"#),
            "OPS 2.0.1 predates WAI-ARIA by four years"
        );
        assert!(
            ok(&three, r#"<p role="doc-footnote" aria-label="x">t</p>"#),
            "EPUB 3 has them"
        );
        assert!(
            !ok(&two, r#"<p onclick="f()">t</p>"#),
            "the Events module is not in OPS 2.0.1"
        );
        assert!(ok(&three, r#"<p onclick="f()">t</p>"#), "EPUB 3 has them");
    }

    /// OPF 2.0.1's `<spine>` takes only `id` and `toc`; EPUB 3 added
    /// `page-progression-direction`. Our EPUB 2 package grammar used the same
    /// attribute wildcard as the EPUB 3 one, so an EPUB 3 attribute in a 2.0
    /// package drew nothing. Found in the output diff patrik posted at
    /// MobileRead #148 and confirmed against epubcheck 5.3.0 locally.
    ///
    /// The EPUB 3 half is the guard: `page-progression-direction` is valid
    /// there, and epubcheck reports zero errors on the same book declared 3.0.
    #[test]
    fn epub2_spine_takes_only_id_and_toc() {
        // `dcterms:modified` is an EPUB 3 construct and must not appear in the
        // 2.0 package - a synthetic fixture that carries it is invalid input,
        // and charging its findings to the rule under test is the mistake
        // `harness/src/wrap.rs` once made against the whole corpus.
        let pkg = |version: &str, spine_attrs: &str| {
            let modified = if version == "3.0" {
                r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#
            } else {
                ""
            };
            format!(
                r#"<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="id">urn:uuid:1</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language>
{modified}</metadata>
<manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine {spine_attrs}><itemref idref="c"/></spine></package>"#
            )
        };
        let ok = |g: &Grammar, version: &str, attrs: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&pkg(version, attrs))
                    .unwrap()
                    .root_element(),
            )
            .is_empty()
        };
        let two = package_grammar_epub2();
        let three = package_grammar();

        assert!(ok(&two, "2.0", r#"toc="ncx""#), "EPUB 2 accepts toc");
        assert!(ok(&two, "2.0", r#"id="s" toc="ncx""#), "EPUB 2 accepts id");
        assert!(
            !ok(&two, "2.0", r#"toc="ncx" page-progression-direction="rtl""#),
            "EPUB 2 rejects page-progression-direction"
        );
        assert!(
            ok(&three, "3.0", r#"page-progression-direction="rtl""#),
            "EPUB 3 accepts page-progression-direction"
        );
    }

    /// `id` and `lang`/`xml:lang` datatypes, which differ by version — the
    /// gap the epubcheck differ found on 2026-08-04 (one book we called VALID
    /// and epubcheck gave 407 errors: 127 digit-initial ids, 280 empty langs).
    ///
    /// **The EPUB 3 half is the whole risk.** HTML5 types `id` as
    /// `datatype.html5.token` (`\S+`, so `id="1"` is fine) and
    /// `common.data.langcode` as `"" | xsd:language` — the empty string is
    /// explicitly valid. XHTML 1.1 is stricter on both (`xsd:ID`, i.e. an
    /// NCName; and a bare `xsd:language`). Applying the EPUB 2 rule
    /// version-wide would invent errors on every EPUB 3 book using `lang=""`
    /// or a numeric id, which is why the datatypes are split rather than
    /// tightened in the shared define.
    #[test]
    fn id_and_lang_datatypes_are_stricter_on_epub2() {
        let doc = |a: &str| {
            format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head><body><p {a}>x</p></body></html>"
            )
        };
        let two = xhtml_grammar_epub2();
        let three = xhtml_grammar();
        let ok = |g: &Grammar, a: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&doc(a)).unwrap().root_element(),
            )
            .is_empty()
        };

        // XHTML 1.1 only: an NCName cannot start with a digit or contain a
        // colon, and a language code has no empty form.
        for a in [r#"id="1""#, r#"id="a:b""#, r#"lang="""#, r#"xml:lang="""#] {
            assert!(!ok(&two, a), "EPUB 2 rejects {a}");
            assert!(ok(&three, a), "EPUB 3 accepts {a}");
        }

        // Valid in both, so the rule cannot have been implemented as "reject
        // everything".
        for a in [
            r#"id="a1""#,
            r#"id="_x""#,
            r#"lang="en""#,
            r#"xml:lang="tr-TR""#,
        ] {
            assert!(ok(&two, a), "EPUB 2 accepts {a}");
            assert!(ok(&three, a), "EPUB 3 accepts {a}");
        }

        // Invalid in both: html5's token is `\S+`, so empty and
        // whitespace-bearing ids fail the looser rule too.
        for a in [r#"id="""#, r#"id="a b""#] {
            assert!(!ok(&two, a), "EPUB 2 rejects {a}");
            assert!(!ok(&three, a), "EPUB 3 rejects {a}");
        }
    }

    /// `<link>` and `<style>` attribute sets, per version. Reported by Doitsu
    /// (MobileRead #146) as `<link media>` drawing RSC-005 on both versions;
    /// the report understated it, as they usually do. `link` had one shared
    /// definition serving both grammars and granting only href/type/sizes,
    /// so EPUB 2 was missing 3 of its 7 legal attributes and EPUB 3 fifteen
    /// of nineteen.
    ///
    /// **The two sets are not nested**, which is why one definition could not
    /// serve both: XHTML 1.1 has `charset`/`rev`, HTML5 dropped them and added
    /// the fifteen. So the negative halves below are the real content of this
    /// test — granting the union would "fix" the report and quietly make each
    /// version accept the other's attributes.
    ///
    /// `rev` is deliberately absent from the negative list: it is granted to
    /// every element by the RDFa global set, in epubcheck's grammar as well as
    /// ours (`common.attrs.rdfa.rev?`), so it is legal on an EPUB 3 `link`
    /// through a different route. `rel` arrives the same way, which is exactly
    /// what hid this bug — the common `<link rel="stylesheet" href="...">`
    /// passed, so nothing looked wrong until a second attribute appeared.
    #[test]
    fn link_and_style_attributes_per_version() {
        let doc = |h: &str| {
            format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title>{h}</head><body><p>x</p></body></html>"
            )
        };
        let two = xhtml_grammar_epub2();
        let three = xhtml_grammar();
        let ok = |g: &Grammar, h: &str| {
            validate_node_report(
                g,
                roxmltree::Document::parse(&doc(h)).unwrap().root_element(),
            )
            .is_empty()
        };
        let link = |a: &str| format!(r#"<link href="s.css" rel="stylesheet" {a}/>"#);

        // epubcheck schema/20/rng/xhtml/link.rng, `link.attlist`.
        for a in [
            r#"charset="utf-8""#,
            r#"hreflang="en""#,
            r#"type="text/css""#,
            r#"rev="stylesheet""#,
            r#"media="all""#,
        ] {
            assert!(ok(&two, &link(a)), "EPUB 2 link accepts {a}");
        }
        // epubcheck schema/30/mod/html5/meta.rnc, `link.attrs`.
        for a in [
            r#"media="all""#,
            r#"hreflang="en""#,
            r#"as="style""#,
            r#"integrity="sha384-x""#,
            r#"referrerpolicy="no-referrer""#,
            r#"crossorigin="anonymous""#,
            r##"color="#fff""##,
            r#"disabled="disabled""#,
            r#"scope="/""#,
            r#"updateviacache="none""#,
            r#"workertype="classic""#,
            r#"imagesrcset="a.png 1x""#,
            r#"imagesizes="100vw""#,
            r#"fetchpriority="high""#,
            r#"blocking="render""#,
        ] {
            assert!(ok(&three, &link(a)), "EPUB 3 link accepts {a}");
        }

        // Negative: neither version inherits the other's, and neither list
        // became a wildcard.
        assert!(
            !ok(&three, &link(r#"charset="utf-8""#)),
            "EPUB 3 link rejects charset (HTML5 dropped it)"
        );
        for a in [
            r#"crossorigin="anonymous""#,
            r#"as="style""#,
            r#"integrity="x""#,
        ] {
            assert!(!ok(&two, &link(a)), "EPUB 2 link rejects {a}");
        }
        for g in [&two, &three] {
            assert!(!ok(g, &link(r#"wibble="x""#)), "link is not a wildcard");
            assert!(
                !ok(g, r#"<style frobnicate="x">p{color:red}</style>"#),
                "style is not a wildcard"
            );
        }

        // `style` is type/media/blocking in HTML5 (meta.rnc:254). `media` was
        // added to the EPUB 2 copy in 0.8.3 and the EPUB 3 copy was missed -
        // the same one-directional fix that left `link` shared.
        for s in [
            r#"<style media="all">p{color:red}</style>"#,
            r#"<style blocking="render">p{color:red}</style>"#,
        ] {
            assert!(ok(&three, s), "EPUB 3 style accepts {s}");
        }
        assert!(
            ok(&two, r#"<style media="all">p{color:red}</style>"#),
            "EPUB 2 style keeps media (0.8.3)"
        );

        // The `sizes` Schematron assert (only on rel="icon", matching
        // epubcheck's epub-xhtml-30.sch:310) lives outside the grammar, so
        // granting `sizes` here must not be read as weakening it. Both shapes
        // are grammar-valid; `xhtml.sch` is what separates them.
        assert!(
            ok(&three, &link(r#"sizes="16x16""#)),
            "grammar allows sizes"
        );
    }

    /// #58, grammar round: the rest of XHTML 1.1's `a` and `img` attribute
    /// sets. OPS 2.0.1 assembles `a.attlist` from four included modules and
    /// we carried only the hypertext module's half, so `<a name="x">` - the
    /// classic pre-`id` anchor form, and the commonest of these in real
    /// EPUB 2 books - drew RSC-005. Same class as #47, one element over.
    ///
    /// The negative half is the point: the attributes XHTML 1.1 does *not*
    /// have must stay rejected. `legacy.rng` is not among content.rng's
    /// includes, so the presentational set never became legal in EPUB 2.
    #[test]
    fn xhtml11_a_and_img_attributes_epub2() {
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let ok = |b: &str| {
            validate_node_report(
                &xhtml_grammar_epub2(),
                roxmltree::Document::parse(&doc(b)).unwrap().root_element(),
            )
            .is_empty()
        };
        // nameident.rng, target.rng, hypertext.rng, csismap.rng.
        assert!(ok("<p><a name=\"anchor\">x</a></p>"), "a/@name");
        assert!(
            ok("<p><a href=\"x\" target=\"_blank\">x</a></p>"),
            "a/@target"
        );
        assert!(
            ok("<p><a href=\"x\" charset=\"utf-8\" rel=\"next\" rev=\"prev\">x</a></p>"),
            "a link attrs"
        );
        assert!(
            ok("<p><a href=\"x\" shape=\"rect\" coords=\"1,2\">x</a></p>"),
            "a image-map attrs"
        );
        // image.rng + nameident.rng.
        assert!(
            ok("<p><img src=\"i.png\" alt=\"a\" longdesc=\"d.html\" name=\"n\"/></p>"),
            "img/@longdesc,@name"
        );

        // Still rejected - presentational attributes XHTML 1.1 drops, whose
        // module content.rng never includes.
        for bad in [
            "<hr width=\"50%\"/>",
            "<hr noshade=\"noshade\"/>",
            "<p align=\"center\">x</p>",
            "<ul type=\"disc\"><li>x</li></ul>",
            "<table><tr><td bgcolor=\"#fff\">v</td></tr></table>",
            "<table><tr><td nowrap=\"nowrap\">v</td></tr></table>",
        ] {
            assert!(!ok(bad), "must stay rejected: {bad}");
        }
    }

    /// #47 (Doitsu): the XHTML 1.1 Tables Module's own attributes - colspan/
    /// rowspan on a cell, span/width on a col - drew RSC-005 on EPUB 2, where
    /// the whole table subtree carried globalAttrs only. epubcheck's
    /// schema/20 accepts them, so this exact table must validate clean.
    /// `width` on `<col>` is obsolete in HTML5, so EPUB 3 still rejects it -
    /// that half of the report was correct behaviour.
    #[test]
    fn xhtml11_table_attributes_epub2() {
        let table = "<table border=\"1\">\
             <colgroup>\
               <col span=\"2\" width=\"100\" style=\"background-color: #f0f0f0;\"/>\
               <col width=\"80\"/>\
             </colgroup>\
             <thead><tr><th colspan=\"3\">Quarterly Report</th></tr></thead>\
             <tfoot><tr><td colspan=\"2\">Total</td><td>215</td></tr></tfoot>\
             <tbody><tr><td rowspan=\"2\">January</td><td>Widgets</td></tr></tbody>\
             </table>";
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let x = doc(table);
        assert!(
            validate_node_report(
                &xhtml_grammar_epub2(),
                roxmltree::Document::parse(&x).unwrap().root_element()
            )
            .is_empty(),
            "EPUB 2 must accept the XHTML 1.1 table attribute set"
        );

        // The rest of the module: table presentation, cell alignment, the
        // header-association attributes, and the wider XHTML 1.1 `scope`.
        let full = "<table summary=\"s\" width=\"80%\" cellpadding=\"2\" cellspacing=\"0\" \
             frame=\"box\" rules=\"all\" border=\"2\">\
             <tbody align=\"char\" char=\".\" charoff=\"2\" valign=\"baseline\">\
             <tr align=\"justify\"><th scope=\"rowgroup\" abbr=\"a\" axis=\"x\" id=\"h\">H</th>\
             <td headers=\"h\" valign=\"middle\">v</td></tr></tbody></table>";
        let xf = doc(full);
        assert!(
            validate_node_report(
                &xhtml_grammar_epub2(),
                roxmltree::Document::parse(&xf).unwrap().root_element()
            )
            .is_empty(),
            "EPUB 2 must accept the full Tables Module attribute set"
        );

        // Still strict where epubcheck is: the enumerated values are checked,
        // and EPUB 3 keeps HTML5's rules (no `width` on `<col>`).
        let bad = doc("<table><tr><td valign=\"sideways\">v</td></tr></table>");
        assert!(
            !validate_node_report(
                &xhtml_grammar_epub2(),
                roxmltree::Document::parse(&bad).unwrap().root_element()
            )
            .is_empty(),
            "an out-of-vocabulary valign must still be rejected"
        );
        let x3 = doc("<table><colgroup><col width=\"80\"/></colgroup>\
             <tr><td>v</td></tr></table>");
        assert!(
            !validate_node_report(
                &xhtml_grammar(),
                roxmltree::Document::parse(&x3).unwrap().root_element()
            )
            .is_empty(),
            "EPUB 3 must keep rejecting the obsolete col/@width"
        );
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

    /// Doitsu, MobileRead #161, against the IDPF `cc-shared-culture` sample
    /// (CC-licensed, so this markup can be quoted).
    ///
    /// Three separate defects, five findings per `<epub:trigger>` and one per
    /// `<video>` fallback, against epubcheck's zero.
    #[test]
    fn epub_trigger_and_transparent_media_content() {
        // 1. `epub:trigger` is a real EPUB 3 element
        //    (schema/30/mod/epub-trigger.rnc), added to `common.elem.flow`.
        //    We had it nowhere, so the element and its `action`/`ref` were
        //    each rejected. RSC-017 still reports the deprecation elsewhere.
        assert!(ok(
            &xhtml_grammar(),
            &xhtml_doc(
                "<p id=\"video1\">v</p>\
                 <epub:trigger ev:observer=\"pause\" ev:event=\"click\" \
                 action=\"pause\" ref=\"video1\"/>"
            )
        ));
        // The attribute grammar is exact rather than permissive: `action` is
        // an enumeration and both `ev:` attributes are required.
        assert!(!ok(
            &xhtml_grammar(),
            &xhtml_doc(
                "<p id=\"v\">v</p>\
                 <epub:trigger ev:observer=\"a\" ev:event=\"c\" action=\"wiggle\" ref=\"v\"/>"
            )
        ));
        assert!(!ok(
            &xhtml_grammar(),
            &xhtml_doc("<p id=\"v\">v</p><epub:trigger action=\"pause\" ref=\"v\"/>")
        ));

        // 2. `<video>`/`<audio>` are transparent: at flow level epubcheck's
        //    `video.inner.flow` ends in `common.inner.transparent.flow`, so a
        //    `<div>` fallback after the `<source>`s is ordinary. We modelled
        //    only the phrasing variant.
        for media in ["video", "audio"] {
            let body = format!(
                "<{media} controls=\"\"><source src=\"a.mp4\" type=\"video/mp4\"/>\
                 <div class=\"errmsg\"><p>no support</p></div></{media}>"
            );
            assert!(ok(&xhtml_grammar(), &xhtml_doc(&body)), "{media}");
        }
    }

    /// #69. A misplaced element whose *name* the grammar does have is
    /// descended into against **its own** content model, not against the
    /// position it was rejected at.
    ///
    /// `<blockquote>` takes XHTML 1.1's `Block.model`, so a `<span>` in it is
    /// misplaced - but a `<span>` inside that `<span>` is perfectly ordinary,
    /// and scoring it against the *blockquote* model reported it too. On one
    /// real book that turned epubcheck's 316 findings into 944: depth 1 was
    /// right and depths 2 and 3 were invented. epubcheck reports the
    /// outermost only, and the whole nest is one defect.
    #[test]
    fn a_misplaced_element_does_not_cascade_onto_its_descendants() {
        let g = xhtml_grammar_epub2();
        assert_eq!(
            fail_locals(
                &g,
                &format!(
                    "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>\
                     <blockquote><span>a<span>b<span>c</span></span></span></blockquote>\
                     </body></html>"
                )
            ),
            ["span", "blockquote"],
            "the outermost span once, then blockquote's own incomplete content"
        );
    }

    /// The same rule read in the other direction, and the half that is a
    /// *missing* finding rather than an invented one: `<div>` is legal in the
    /// `<blockquote>` model the span was rejected against, and illegal in
    /// span's own. Checking the subtree against the parent's model therefore
    /// stayed silent on it, where epubcheck reports it. One fix, both
    /// directions - which is why the two halves are pinned together.
    #[test]
    fn a_misplaced_element_subtree_is_judged_by_its_own_model() {
        let g = xhtml_grammar_epub2();
        assert_eq!(
            fail_locals(
                &g,
                &format!(
                    "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>\
                     <blockquote><span><div>x</div></span></blockquote></body></html>"
                )
            ),
            ["span", "div", "blockquote"],
            "span misplaced, and the div inside it illegal in span's own model"
        );
        // ...and a child that IS legal in that model stays silent.
        assert_eq!(
            fail_locals(
                &g,
                &format!(
                    "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>\
                     <blockquote><span><em>x</em></span></blockquote></body></html>"
                )
            ),
            ["span", "blockquote"],
            "em is ordinary inside a span, misplaced or not"
        );
    }

    /// EPUB 2's `<map>` must not be a door into the EPUB 3 vocabulary.
    ///
    /// It was one until #69: the EPUB 2 branch referenced the EPUB 3 `mapEl`,
    /// whose content is `flowContent`, so `html > body > p > map` put every
    /// HTML5 element four steps from the EPUB 2 root. That was known and
    /// judged harmless as a permissiveness gap - and it stopped being
    /// harmless the moment anything resolved an element's model by
    /// reachability, which is exactly what the two tests above do.
    #[test]
    fn epub2_map_does_not_admit_the_epub3_vocabulary() {
        let two = xhtml_grammar_epub2();
        let body = "<p><map name=\"m\"><section>x</section></map></p>";
        assert!(
            fail_locals(
                &two,
                &format!(
                    "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
                     <body>{body}</body></html>"
                )
            )
            .contains(&"section".to_string()),
            "an HTML5 element inside an EPUB 2 <map> is still an HTML5 element"
        );
        // EPUB 3's own map is untouched: flow content there is correct.
        assert!(ok(
            &xhtml_grammar(),
            &xhtml_doc("<p><map name=\"m\"><span>x</span></map></p>")
        ));
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

    /// The other half of Tier-C: an attribute whose *name* isn\'t allowed and
    /// an allowed attribute with an invalid *value* are different problems and
    /// read differently. "not allowed here" is wrong for the second - the
    /// value is a real thing to quote, and the name is fine.
    #[test]
    fn attribute_faults_distinguish_bad_name_from_bad_value() {
        let g = xhtml_grammar();
        let describe1 = |body: &str| {
            let x = xhtml_doc(body);
            let d = roxmltree::Document::parse(&x).unwrap();
            validate_node_report(&g, d.root_element())
                .into_iter()
                .map(|b| b.describe().0)
                .collect::<Vec<_>>()
        };
        // An obsolete/removed attribute name.
        assert_eq!(
            describe1("<p contextmenu=\"x\">hi</p>"),
            ["attribute \"contextmenu\" is not allowed here"]
        );
        // A permitted attribute (dir) with a value outside its enumeration.
        assert_eq!(
            describe1("<p dir=\"sideways\">hi</p>"),
            ["value of attribute \"dir\" is invalid: \"sideways\""]
        );
        // The valid value draws nothing.
        assert!(describe1("<p dir=\"rtl\">hi</p>").is_empty());
    }

    /// The value-error carries name then value as structured params, and pins
    /// the attribute itself (`@name`) like the name-error does.
    #[test]
    fn invalid_value_params_and_pinning() {
        let g = xhtml_grammar();
        let x = xhtml_doc("<p dir=\"sideways\">hi</p>");
        let d = roxmltree::Document::parse(&x).unwrap();
        let blames = validate_node_report(&g, d.root_element());
        assert_eq!(blames.len(), 1);
        assert!(matches!(
            blames[0],
            Blame::Attribute(_, _, AttributeFault::InvalidValue)
        ));
        let (_, params) = blames[0].describe();
        assert_eq!(params, ["dir", "sideways"]);
        assert_eq!(blames[0].attribute().map(|a| a.name()), Some("dir"));
    }

    /// #13 (Doitsu, MobileRead): XHTML 1.1 body is block-level, so loose text
    /// and inline elements directly under it are content-model errors. HTML5
    /// (EPUB 3) treats the same as valid flow content, so this is EPUB 2 only.
    /// The suggestion is the real block set, which epubcheck lists in full.
    #[test]
    fn epub2_body_is_block_level() {
        let g = xhtml_grammar_epub2();
        let doc = |b: &str| {
            format!("<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>")
        };
        let report = |b: &str| {
            let x = doc(b);
            validate_node_report(&g, roxmltree::Document::parse(&x).unwrap().root_element())
                .into_iter()
                .map(|bl| bl.describe().0)
                .collect::<Vec<_>>()
        };
        // A <span> directly under body: rejected, with the block set named.
        let r = report("<p>a</p><span>x</span>");
        assert!(
            r.iter()
                .any(|m| m.contains("element \"span\" is not allowed here")
                    && m.contains("expected one of")
                    && m.contains("\"blockquote\"")
                    && m.contains("\"ul\"")),
            "got {r:?}"
        );
        // Loose text under body.
        assert!(
            report("<p>a</p>loose text")
                .iter()
                .any(|m| m.contains("stray text is not allowed directly in \"body\"")),
            "loose text under body must be flagged"
        );
        // A bare <br> under body (the common 1Q84 shape) is rejected too -
        // <br> is inline.
        assert!(!report("<p>a</p><br/><p>b</p>").is_empty());
        // But a body of only block elements is fine.
        assert!(report("<h1>T</h1><p>a</p><ul><li>x</li></ul>").is_empty());
    }

    /// Issue #68: each stray run is blamed on *itself*, not on the element
    /// containing it, so several runs in one parent stay tellable apart.
    ///
    /// This is the whole point of `Blame::Text` and nothing else asserts it.
    /// Before the fix all three findings below carried the same node - the
    /// `<body>` - which meant the same line, the same column and the same
    /// element path. Detection was unaffected, so **neither the corpus nor the
    /// shelf could see the difference**; only a consumer trying to act on a
    /// finding could, which is how it went unnoticed. Assert on the nodes
    /// rather than the message: the message names the parent by design, so it
    /// is identical for all three either way and would pass on the old code.
    #[test]
    fn stray_text_is_blamed_on_the_run_not_its_parent() {
        let g = xhtml_grammar_epub2();
        let x = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body>one<p>a</p>two<p>b</p>three</body></html>"
        );
        let doc = roxmltree::Document::parse(&x).unwrap();
        let blames = validate_node_report(&g, doc.root_element());

        let runs: Vec<_> = blames
            .iter()
            .filter(|b| b.is_text())
            .map(super::Blame::node)
            .collect();
        assert_eq!(runs.len(), 3, "one blame per stray run");
        for n in &runs {
            assert!(n.is_text(), "the blamed node must be the run itself");
        }
        // The three are distinct nodes with distinct text - the property that
        // makes them individually addressable.
        let texts: Vec<_> = runs.iter().map(|n| n.text().unwrap_or("")).collect();
        assert_eq!(texts, ["one", "two", "three"]);

        // And the path built from them pins each run, rather than resolving up
        // to the shared parent. This is what a consumer actually reads.
        let paths: Vec<_> = runs
            .iter()
            .map(|n| crate::xmlext::node_path_text(*n).path)
            .collect();
        assert_eq!(
            paths,
            [
                "/h:html[1]/h:body[1]/text()[1]",
                "/h:html[1]/h:body[1]/text()[2]",
                "/h:html[1]/h:body[1]/text()[3]",
            ]
        );
    }

    /// XHTML 1.1 `<p>` (and headings, address, dt) take inline content only,
    /// so a block element inside one is an error - `<p><div>` is a common
    /// authoring mistake epubcheck reports. `<div>`/`<li>`/table cells stay
    /// flow (permissive), so ordinary nesting is untouched.
    #[test]
    fn epub2_p_is_inline_only() {
        let g = xhtml_grammar_epub2();
        let ok2 = |b: &str| {
            let x = format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>"
            );
            validate_node_report(&g, roxmltree::Document::parse(&x).unwrap().root_element())
                .is_empty()
        };
        assert!(!ok2("<p><div>x</div></p>"), "block inside p is an error");
        assert!(
            !ok2("<h2><p>x</p></h2>"),
            "block inside a heading is an error"
        );
        assert!(
            ok2("<p>Hi <em>t</em> <span>s</span></p>"),
            "inline in p is fine"
        );
        assert!(
            ok2("<div><p>x</p> and <span>text</span></div>"),
            "div takes flow"
        );
        assert!(ok2("<ul><li><p>x</p> t</li></ul>"), "li takes flow");
    }

    /// Doitsu, MobileRead #140: XHTML 1.1 gives `blockquote` the same
    /// `Block.model` as `body`, so inline elements, `<br/>` and loose text
    /// directly inside one are content-model errors. `noscript` has the same
    /// model (`schema/20/rng/xhtml/script.rng`); those three elements are the
    /// only users of `Block.model` in the whole module set.
    #[test]
    fn epub2_blockquote_is_block_level() {
        let g = xhtml_grammar_epub2();
        let ok2 = |b: &str| {
            let x = format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>"
            );
            validate_node_report(&g, roxmltree::Document::parse(&x).unwrap().root_element())
                .is_empty()
        };
        // The reported shape: an anchor, a <br/>, a <span> and bare text.
        assert!(
            !ok2("<blockquote><a href=\"#x\">t</a></blockquote>"),
            "inline in blockquote is an error"
        );
        assert!(!ok2("<blockquote><br/></blockquote>"));
        assert!(!ok2("<blockquote><span>t</span></blockquote>"));
        assert!(!ok2("<blockquote>loose text</blockquote>"));
        // `Block.model` is oneOrMore, so an empty blockquote is "incomplete".
        assert!(!ok2("<blockquote/>"));
        assert!(
            !ok2("<noscript>t</noscript>"),
            "noscript is Block.model too"
        );
        // Block children are fine, and the EPUB 3 model stays flow.
        assert!(ok2("<blockquote><p>t</p></blockquote>"));
        assert!(ok2(
            "<blockquote cite=\"u\"><div>t</div><ul><li>x</li></ul></blockquote>"
        ));
        assert!(ok2("<noscript><p>t</p></noscript>"));
        assert!(ok(
            &xhtml_grammar(),
            &xhtml_doc("<blockquote>flow is valid in HTML5</blockquote>")
        ));
    }

    /// OPS 2.0.1 has no MathML - `schema/20` never includes a MathML grammar -
    /// so `<math>` in an EPUB 2 document is an error, while SVG (hooked into
    /// `Block.class`/`Inline.class` by `content.rng`) is fine. EPUB 3 keeps
    /// both.
    #[test]
    fn epub2_has_no_mathml_but_has_svg() {
        let g = xhtml_grammar_epub2();
        let ok2 = |b: &str| {
            let x = format!(
                "<html {XHTML_NS_DECLS}><head><title>t</title></head><body>{b}</body></html>"
            );
            validate_node_report(&g, roxmltree::Document::parse(&x).unwrap().root_element())
                .is_empty()
        };
        let math = "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mi>x</mi></math>";
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
        assert!(!ok2(&format!("<p>{math}</p>")), "math is not in OPS 2.0.1");
        assert!(!ok2(math), "nor at block level");
        assert!(ok2(&format!("<p>{svg}</p>")), "svg is inline in OPS 2.0.1");
        assert!(ok2(svg), "and block");
        assert!(
            ok(&xhtml_grammar(), &xhtml_doc(&format!("<p>{math}</p>"))),
            "EPUB 3 keeps MathML"
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
    fn xhtml_grammar_rejects_unknown_and_mistyped_attributes() {
        // The whole point of #31: a made-up name and a typo of a real one
        // must both be rejected now that the wildcard is gone (Doitsu,
        // MobileRead #110). Each should be its own blamed attribute.
        let xml = xhtml_doc("<p fake=\"fake\" clas=\"header\">*</p>");
        let locs = fail_locals(&xhtml_grammar(), &xml);
        assert_eq!(locs, vec!["@fake", "@clas"]);
    }

    #[test]
    fn xhtml_grammar_epub2_rejects_unknown_and_mistyped_attributes() {
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><p fake=\"fake\" clas=\"header\">*</p></body></html>"
        );
        let locs = fail_locals(&xhtml_grammar_epub2(), &xml);
        assert_eq!(locs, vec!["@fake", "@clas"]);
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

    // #34 slice A: newly-enumerated global attribute names. Still
    // wildcard-covered today (verdict-neutral by construction, so a bad
    // value can't be asserted rejected yet - only that a real-shape value
    // is accepted, same as before).

    #[test]
    fn xhtml_grammar_accepts_microdata_attributes() {
        let xml = xhtml_doc(concat!(
            "<div itemscope=\"itemscope\" itemtype=\"https://schema.org/Book\" ",
            "itemid=\"urn:isbn:0000\"><p itemprop=\"name\">T</p></div>",
            "<div itemref=\"a b\"></div>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_url_shaped_itemprop() {
        // HTML5 microdata allows an itemprop value to be an absolute URL,
        // not just a plain token (epubcheck corpus fixture
        // microdata-valid.xhtml: itemprop="http://example.com/color" and
        // itemprop="name http://example.com/fn" - a mixed list). This
        // regressed once already (NMTOKEN rejected the "/"), see the
        // itemprop definition's comment in schemas/xhtml.rng.
        let xml = xhtml_doc(concat!(
            "<p itemprop=\"http://example.com/color\">black</p>",
            "<h1 itemprop=\"name http://example.com/fn\">Hedral</h1>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_rdfa_prefix_on_html() {
        // Pulled forward from #34 slice C - see the `prefix` definition's
        // comment in schemas/xhtml.rng. Matches epubcheck corpus fixture
        // microdata-valid.xhtml, which combines RDFA `prefix` with
        // microdata attributes on the same document.
        let xml = "<html ".to_string()
            + XHTML_NS_DECLS
            + " prefix=\"foaf: http://xmlns.com/foaf/0.1/\">\
               <head><title>T</title></head><body/></html>";
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_web_component_attributes() {
        let xml = xhtml_doc("<p is=\"x-highlight\" slot=\"body\">hi</p>");
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_interaction_editing_attributes() {
        let xml = xhtml_doc(concat!(
            "<p draggable=\"true\" inputmode=\"numeric\" enterkeyhint=\"go\" ",
            "autocapitalize=\"words\" popover=\"auto\">hi</p>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_autofocus_and_nonce() {
        let xml = xhtml_doc("<p autofocus=\"autofocus\" nonce=\"abc123\">hi</p>");
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_role_and_aria_globals() {
        let xml = xhtml_doc(concat!(
            "<p role=\"note\" aria-label=\"x\" aria-hidden=\"true\" ",
            "aria-describedby=\"y\" aria-live=\"polite\">hi</p>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    /// EPUB 2 has no `role` and no `aria-*` (#66).
    ///
    /// This asserted the opposite while the ARIA half of #66 was held back.
    /// The chronology is why epubcheck rejects all 48: OPS 2.0.1 was finalised
    /// 2010-09-04, WAI-ARIA 1.0 became a Recommendation 2014-03-20, so
    /// `schema/20` could not have carried them.
    #[test]
    fn xhtml_grammar_epub2_has_no_role_or_aria() {
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><p role=\"note\" aria-label=\"x\">hi</p></body></html>"
        );
        assert!(!ok(&xhtml_grammar_epub2(), &xml));
        // The EPUB 3 half is the guard: they are ordinary globals there.
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #34 slice B: on* event-handler attributes.

    #[test]
    fn xhtml_grammar_accepts_generic_event_handlers() {
        let xml = xhtml_doc(concat!(
            "<button onclick=\"doIt()\" onmouseover=\"hi()\">go</button>",
            "<img src=\"a.png\" alt=\"\" onerror=\"fallback()\"/>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_body_only_window_events_on_body() {
        // onunload/onpageshow/etc. (epubcheck's body.attrs.on*, mod/html5/
        // meta.rnc) are properly scoped to <body> as of the #36 cutover -
        // see bodyOnlyEvents in schemas/xhtml.rng.
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body onload=\"init()\" onunload=\"cleanup()\" onpageshow=\"show()\">\
             <p>hi</p></body></html>"
        );
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_rejects_body_only_window_events_elsewhere() {
        // The other half of the same story: now that the wildcard is gone,
        // onunload genuinely isn't allowed outside <body> - it's not a
        // generic event handler (onclick et al are global; this family
        // isn't).
        let xml = xhtml_doc("<p onunload=\"x()\">hi</p>");
        assert!(!ok(&xhtml_grammar(), &xml));
    }

    /// EPUB 2 has no event-handler attributes at all (#66).
    ///
    /// This test used to assert the opposite. `content.rng` includes 25
    /// modules and `events.rng` is not among them - the same exclusion that
    /// removes the Forms module - so `onload` and `onclick` are declared
    /// nowhere in OPS 2.0.1, not even on `<body>`. epubcheck rejects both,
    /// measured one book per attribute; the assertion was encoding our own
    /// permissiveness rather than epubcheck's behaviour.
    #[test]
    fn xhtml_grammar_epub2_has_no_event_handlers() {
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body onload=\"init()\"><p onclick=\"hi()\">hi</p></body></html>"
        );
        assert!(!ok(&xhtml_grammar_epub2(), &xml));
        // The EPUB 3 half is the guard: they are ordinary globals there.
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #34 slice C: RDFA 1.1 global attributes.

    #[test]
    fn xhtml_grammar_accepts_rdfa_attributes() {
        let xml = xhtml_doc(concat!(
            "<div about=\"#me\" typeof=\"foaf:Person\" vocab=\"http://xmlns.com/foaf/0.1/\">",
            "<p property=\"foaf:name\" datatype=\"xsd:string\">Baris</p>",
            "<a rev=\"foaf:knows\" resource=\"#you\" href=\"#you\">friend</a>",
            "<span property=\"foaf:topic\" inlist=\"\" content=\"x\">t</span>",
            "</div>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_rel_anywhere() {
        // `rel` is now a genuine global (RDFA, #33 slice 3 - see the
        // comment above its definition in schemas/xhtml.rng for why it
        // moved from a per-element <a>/<area> attribute to one shared
        // global one). Accepted both on <a> and on a plain element,
        // matching real epubcheck (RDFA grants `rel` everywhere, not just
        // on <a>).
        let xml = xhtml_doc(concat!(
            "<a href=\"x\" rel=\"nofollow\">x</a>",
            "<span rel=\"license\">y</span>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #35: xml:*/epub:* namespaced attribute families.

    #[test]
    fn xhtml_grammar_accepts_xml_base_and_space() {
        let xml = xhtml_doc(concat!(
            "<blockquote xml:base=\"http://example.com/\" xml:space=\"preserve\">",
            "  quoted   text  ",
            "</blockquote>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_epub_prefix_on_html() {
        let xml = "<html ".to_string()
            + XHTML_NS_DECLS
            + " epub:prefix=\"myvocab: http://example.com/vocab#\">\
               <head><title>T</title></head><body/></html>";
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #33: forms vocabulary completion (input/select/textarea/button),
    // against real gaps found auditing epubcheck's web-forms(2).rnc.

    #[test]
    fn xhtml_grammar_accepts_input_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<input type=\"text\" required=\"required\" min=\"1\" max=\"10\" step=\"1\" ",
            "pattern=\"[0-9]+\" multiple=\"multiple\" accept=\"image/*\" autocomplete=\"off\" ",
            "size=\"20\" maxlength=\"50\" minlength=\"1\" readonly=\"readonly\" ",
            "src=\"x.png\" alt=\"x\" dirname=\"x.dir\" capture=\"user\" height=\"20\" ",
            "width=\"20\" formaction=\"x\" formmethod=\"post\" formnovalidate=\"formnovalidate\" ",
            "formtarget=\"_blank\" formenctype=\"multipart/form-data\"/>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_select_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<select required=\"required\" name=\"x\" size=\"3\" autocomplete=\"off\">",
            "<option>a</option></select>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_textarea_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<textarea required=\"required\" name=\"x\" rows=\"4\" cols=\"40\" wrap=\"soft\" ",
            "placeholder=\"p\" maxlength=\"200\" minlength=\"0\" readonly=\"readonly\" ",
            "autocomplete=\"off\" dirname=\"x.dir\"></textarea>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_button_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<button name=\"x\" value=\"v\" formaction=\"x\" formmethod=\"post\" ",
            "formnovalidate=\"formnovalidate\" formtarget=\"_blank\" ",
            "formenctype=\"multipart/form-data\" popovertarget=\"x\" ",
            "popovertargetaction=\"toggle\">go</button>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #33 slice 2: a/area/img/ins/del attribute completion.

    #[test]
    fn xhtml_grammar_accepts_a_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<a href=\"x\" download=\"file.pdf\" hreflang=\"en\" ping=\"http://x/\" ",
            "referrerpolicy=\"no-referrer\">link</a>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_epub2_accepts_a_download_and_hreflang_not_ping() {
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><p><a href=\"x\" download=\"f\" hreflang=\"en\">link</a></p></body></html>"
        );
        assert!(ok(&xhtml_grammar_epub2(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_area_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<map name=\"m\"><area shape=\"rect\" coords=\"0,0,10,10\" href=\"x\" ",
            "alt=\"a\" download=\"f\" rel=\"nofollow\" ping=\"http://x/\"/></map>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_img_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<img src=\"x.png\" alt=\"\" loading=\"lazy\" decoding=\"async\" ",
            "crossorigin=\"anonymous\" referrerpolicy=\"no-referrer\" ismap=\"ismap\"/>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_ins_del_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<ins cite=\"http://x/\" datetime=\"2026-07-23\">added</ins>",
            "<del cite=\"http://x/\" datetime=\"2026-07-23\">removed</del>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_epub2_accepts_ins_del_attribute_completion() {
        let xml = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><p><ins cite=\"http://x/\" datetime=\"2026-07-23\">a</ins>\
             <del cite=\"http://x/\" datetime=\"2026-07-23\">r</del></p></body></html>"
        );
        assert!(ok(&xhtml_grammar_epub2(), &xml));
    }

    // #33 slice 3: media/object/remaining-forms attribute completion.

    #[test]
    fn xhtml_grammar_accepts_audio_video_source_completion() {
        let xml = xhtml_doc(concat!(
            "<audio muted=\"muted\" crossorigin=\"anonymous\">",
            "<source src=\"a.mp3\" type=\"audio/mpeg\"/></audio>",
            "<video preload=\"auto\" muted=\"muted\" crossorigin=\"anonymous\" ",
            "playsinline=\"playsinline\">",
            "<source src=\"a.mp4\" width=\"640\" height=\"480\"/></video>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_object_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<object data=\"x.svg\" type=\"image/svg+xml\" usemap=\"#m\" ",
            "name=\"o\" form=\"f\"></object>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_accepts_remaining_forms_attribute_completion() {
        let xml = xhtml_doc(concat!(
            "<fieldset name=\"fs\"><legend>L</legend></fieldset>",
            "<output for=\"x\" name=\"out\"></output>",
            "<select><optgroup label=\"g\" disabled=\"disabled\">",
            "<option label=\"o\">a</option></optgroup></select>",
            "<meter value=\"5\" min=\"0\" max=\"10\" low=\"2\" high=\"8\" optimum=\"5\">5</meter>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    // #40: <dialog> and <search> - HTML5-only elements the grammar was
    // missing entirely.

    #[test]
    fn xhtml_grammar_accepts_dialog_and_search() {
        let xml = xhtml_doc(concat!(
            "<dialog open=\"open\"><p>hi</p></dialog>",
            "<search><p>find</p></search>"
        ));
        assert!(ok(&xhtml_grammar(), &xml));
    }

    #[test]
    fn xhtml_grammar_epub2_rejects_dialog_and_search() {
        // HTML5-only - XHTML 1.1 (EPUB 2) predates both, so they must stay
        // rejected there, same as <section>/<nav>/etc.
        let dialog = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><dialog><p>hi</p></dialog></body></html>"
        );
        assert!(!ok(&xhtml_grammar_epub2(), &dialog));
        let search = format!(
            "<html {XHTML_NS_DECLS}><head><title>t</title></head>\
             <body><search><p>find</p></search></body></html>"
        );
        assert!(!ok(&xhtml_grammar_epub2(), &search));
    }

    #[test]
    fn heavily_attributed_element_validates_fast() {
        // #39 regression guard: the exponential-time att_deriv path was
        // driven by an attribute matchable by *both* an element's own rule
        // and the old permissive wildcard (genuine grammar ambiguity). The
        // wildcard is gone (#36), so every attribute now matches exactly one
        // rule and there is no ambiguity to explore. An element carrying a
        // large simultaneous attribute set - which used to hang for tens of
        // minutes - must validate essentially instantly; if the exponential
        // path ever came back, this test would hang and fail in CI.
        let xml = xhtml_doc(concat!(
            "<input type=\"text\" name=\"n\" value=\"v\" required=\"required\" ",
            "min=\"1\" max=\"9\" step=\"1\" pattern=\"[0-9]+\" multiple=\"multiple\" ",
            "accept=\"*\" autocomplete=\"off\" size=\"5\" maxlength=\"9\" minlength=\"1\" ",
            "readonly=\"readonly\" src=\"x\" alt=\"a\" dirname=\"d\" capture=\"user\" ",
            "height=\"1\" width=\"1\" formaction=\"x\" formmethod=\"post\" ",
            "formnovalidate=\"formnovalidate\" formtarget=\"_blank\" ",
            "id=\"i\" class=\"c\" title=\"t\" lang=\"en\" dir=\"ltr\" role=\"textbox\" ",
            "aria-label=\"x\" onclick=\"f()\"/>"
        ));
        // (data-* is deliberately grammar-invalid - it's accepted at the
        // report level in opf.rs, not by the grammar - so it's left out of
        // this pure-grammar check.)
        assert!(ok(&xhtml_grammar(), &xml));
    }
}
