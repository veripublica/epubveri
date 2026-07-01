//! A pure-Rust, derivative-based RELAX NG validation engine (phase 1 of the
//! schema engine). This module is the reusable core; it is **not yet wired into
//! the validator** — the next increment loads real RNG schema files and maps
//! failures to `RSC-005`. For now it is exercised by unit tests with a tiny
//! hand-built grammar and the real (simplified) `container.xml` grammar,
//! constructed via the builder API (so there is no schema-file provenance
//! question yet).

pub mod datatype;
pub mod derive;
pub mod load;
pub mod pattern;

pub use derive::{validate_node, validate_xml, Grammar};
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
}
