//! XML declaration / DOCTYPE / entity / encoding checks, plus a handful of
//! attribute- and element-level content-document checks. `roxmltree`
//! exposes no structured API for the XML declaration's version, DOCTYPE
//! entities, or a document's original encoding (confirmed by reading its
//! public API) — those need small hand-written raw byte/text scanners
//! (`check_raw`), same "no new dependency" style as `smil.rs`'s
//! clock-value parser and `layout.rs`'s viewport grammar, not a full
//! DTD/XML-declaration parser. The rest (`check_dom`) works off the
//! already-parsed document.

use crate::ids::*;
use crate::report::{Report, Severity};

/// Raw byte/text scans on a content document — independent of whether it
/// parses as well-formed XML, so these still fire even when e.g. a UTF-16
/// garbling (before the `css::decode_bytes` fix) would otherwise have
/// broken the parse.
///
/// EPUB3-only: confirmed via the real corpus that all of these checks
/// (`content-document-xhtml.feature`, under `epub3/`) don't apply to
/// EPUB2's XHTML content model, which is its own, more lenient, XHTML
/// 1.1-DTD-based spec section — an EPUB2 content document legitimately
/// declares `<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" ...>`,
/// which would otherwise be misflagged as HTM-004's "obsolete" doctype (a
/// real false positive found via the corpus, not by inspection: dozens of
/// EPUB2 fixtures across the corpus reuse exactly this doctype as their
/// standard template).
pub(crate) fn check_raw(bytes: &[u8], text: &str, path: &str, is_epub3: bool, report: &mut Report) {
    if !is_epub3 {
        return;
    }
    if crate::css::has_utf16_bom(bytes) {
        report.push_at(
            HTM_058,
            Severity::Error,
            "content document is not UTF-8 encoded",
            path,
        );
    }

    if let Some(decl_end) = text
        .trim_start()
        .strip_prefix("<?xml")
        .and_then(|rest| rest.find("?>"))
    {
        let decl = &text.trim_start()[..decl_end + "<?xml".len() + "?>".len()];
        if decl.contains("version=\"1.1\"") || decl.contains("version='1.1'") {
            report.push_at(
                HTM_001,
                Severity::Error,
                "XML declaration must not use version 1.1",
                path,
            );
        }
    }

    check_doctype(text, path, report);
}

/// Finds the full `<!DOCTYPE ...>` declaration, correctly skipping past
/// any `>` inside an internal subset (`[...]`) to find the declaration's
/// *own* closing `>`.
fn extract_doctype(text: &str) -> Option<&str> {
    let start = text.find("<!DOCTYPE")?;
    let after = &text[start..];
    let search_from = match after.find('[') {
        Some(bracket) => bracket + after[bracket..].find(']')?,
        None => 0,
    };
    let end = after[search_from..].find('>')?;
    Some(&after[..search_from + end + 1])
}

fn check_doctype(text: &str, path: &str, report: &mut Report) {
    let Some(doctype) = extract_doctype(text) else {
        return;
    };
    if doctype.contains(" PUBLIC ") {
        report.push_at(
            HTM_004,
            Severity::Error,
            "DOCTYPE has an obsolete PUBLIC identifier",
            path,
        );
    }
    if let (Some(open), Some(close)) = (doctype.find('['), doctype.rfind(']')) {
        if close > open {
            let subset = &doctype[open + 1..close];
            for (i, _) in subset.match_indices("<!ENTITY") {
                let rest = &subset[i..];
                let Some(end) = rest.find('>') else { continue };
                let decl = &rest[..end];
                if decl.contains("SYSTEM") || decl.contains("PUBLIC") {
                    report.push_at(
                        HTM_003,
                        Severity::Error,
                        "entity is declared external (SYSTEM/PUBLIC)",
                        path,
                    );
                }
            }
        }
    }
}

/// A DOCTYPE in the OPF only makes sense if its declared root name matches
/// the OPF's own root element, `package`. Confirmed via the real corpus:
/// the legacy OEB 1.2 `<!DOCTYPE package PUBLIC "...OEB 1.2 Package//EN" ...>`
/// is explicitly *valid* (root name "package" matches), while
/// `<!DOCTYPE html PUBLIC "...OEB 1.2 Document//EN" ...>` is invalid (root
/// name "html" doesn't match) — the PUBLIC identifier itself isn't what's
/// being judged, just whether the declared root is sane for this document.
pub(crate) fn check_opf_doctype(text: &str, opf_path: &str, report: &mut Report) {
    let Some(start) = text.find("<!DOCTYPE") else {
        return;
    };
    let after = text[start + "<!DOCTYPE".len()..].trim_start();
    let root_name = after
        .split(|c: char| c.is_whitespace() || c == '>' || c == '[')
        .next()
        .unwrap_or("");
    if root_name != "package" {
        report.push_at(
            HTM_009,
            Severity::Error,
            format!("OPF document's DOCTYPE root '{root_name}' does not match <package>"),
            opf_path,
        );
    }
}

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SSML_NS: &str = "http://www.w3.org/2001/10/synthesis";

/// Legitimate w3.org/idpf.org namespaces content documents actually use
/// (XHTML itself, EPUB's own `epub:` ops namespace, XML/XLink, SVG,
/// MathML) — HTM-054 is about *custom* attributes impersonating a
/// w3.org/idpf.org affiliation, not these standard, expected ones.
const KNOWN_NAMESPACES: [&str; 7] = [
    XHTML_NS,
    "http://www.idpf.org/2007/ops",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/1999/xlink",
    SSML_NS,
    "http://www.w3.org/2000/svg",
    "http://www.w3.org/1998/Math/MathML",
];

/// DOM/attribute-level checks on an already-parsed content document.
/// EPUB3-only, same reasoning as `check_raw` above (all confirmed from the
/// `epub3/06-content-document/content-document-xhtml.feature` section).
pub(crate) fn check_dom(d: &roxmltree::Document, path: &str, is_epub3: bool, report: &mut Report) {
    if !is_epub3 {
        return;
    }
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.tag_name().namespace() == Some(XHTML_NS)
            && matches!(node.tag_name().name(), "base" | "embed" | "rp")
        {
            report.push_at(
                HTM_055,
                Severity::Info,
                format!("'{}' is a discouraged construct", node.tag_name().name()),
                path,
            );
        }

        for attr in node.attributes() {
            match attr.namespace() {
                Some(ns) if ns == SSML_NS && attr.name() == "ph" => {
                    if attr.value().trim().is_empty() {
                        report.push_at(
                            HTM_007,
                            Severity::Warning,
                            "ssml:ph must not be empty",
                            path,
                        );
                    }
                }
                Some(ns) if !KNOWN_NAMESPACES.contains(&ns) && reserved_namespace_host(ns) => {
                    report.push_at(
                        HTM_054,
                        Severity::Error,
                        format!("attribute uses a reserved namespace '{ns}'"),
                        path,
                    );
                }
                None => {
                    if let Some(rest) = attr.name().strip_prefix("data-") {
                        if !is_valid_data_attr_suffix(rest) {
                            report.push_at(
                                HTM_061,
                                Severity::Error,
                                format!("'data-{rest}' is not a valid data-* attribute name"),
                                path,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn reserved_namespace_host(uri: &str) -> bool {
    let after_scheme = uri.split("://").nth(1).unwrap_or(uri);
    let host = after_scheme.split('/').next().unwrap_or(after_scheme);
    host == "w3.org"
        || host.ends_with(".w3.org")
        || host == "idpf.org"
        || host.ends_with(".idpf.org")
}

fn is_valid_data_attr_suffix(rest: &str) -> bool {
    !rest.is_empty() && !rest.starts_with('-') && !rest.chars().any(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_raw(text: &str) -> Vec<&'static str> {
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    fn run_dom(xhtml: &str) -> Vec<&'static str> {
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_dom(&d, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    #[test]
    fn xml_11_declaration_errors() {
        assert_eq!(run_raw("<?xml version=\"1.1\"?><html/>"), vec![HTM_001]);
        assert!(run_raw("<?xml version=\"1.0\"?><html/>").is_empty());
    }

    #[test]
    fn utf16_bom_errors() {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend("<html/>".encode_utf16().flat_map(|c| c.to_be_bytes()));
        let mut report = Report::new();
        check_raw(&bytes, "<html/>", "content.xhtml", true, &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_058]);
    }

    #[test]
    fn obsolete_public_doctype_errors() {
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        assert_eq!(run_raw(text), vec![HTM_004]);
    }

    #[test]
    fn legacy_compat_doctype_is_valid() {
        let text = r#"<!DOCTYPE html SYSTEM "about:legacy-compat"><html/>"#;
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn epub2_xhtml11_dtd_doctype_is_not_flagged() {
        // A real false positive found via the corpus, not by inspection:
        // dozens of EPUB2 fixtures across the corpus legitimately use this
        // exact doctype as their standard XHTML 1.1 content-document
        // template - HTM-004 only applies to EPUB3's stricter content
        // model.
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", false, &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn external_entity_errors() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo SYSTEM \"sample.dtd\">\n]><html/>";
        assert_eq!(run_raw(text), vec![HTM_003]);
    }

    #[test]
    fn internal_only_entity_is_valid() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo \"foo\">\n]><html/>";
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn reserved_namespace_attribute_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:w3="http://example.w3.org" xmlns:idpf="http://example.idpf.org" xmlns:ok="http://example.org/w3.org">
            <body w3:attr="disallowed" idpf:attr="disallowed" ok:attr="allowed"/>
        </html>"#;
        let ids = run_dom(xhtml);
        assert_eq!(ids, vec![HTM_054, HTM_054]);
    }

    #[test]
    fn known_epub_and_xhtml_namespaces_are_not_reserved() {
        // epub:type is a legitimate, standard attribute - not a custom
        // attribute impersonating a w3.org/idpf.org affiliation. A real
        // false positive found via scripts/spike.py's nav fixture
        // (epub:type="toc"), not by inspection.
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
            <body><nav epub:type="toc"/></body>
        </html>"#;
        assert!(run_dom(xhtml).is_empty());
    }

    #[test]
    fn discouraged_elements() {
        for el in ["base", "embed", "rp"] {
            let xhtml = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><{el}/></body></html>"#
            );
            assert_eq!(run_dom(&xhtml), vec![HTM_055], "element {el}");
        }
    }

    #[test]
    fn ssml_ph_empty_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:ssml="http://www.w3.org/2001/10/synthesis">
            <body><span ssml:ph=" "></span><span ssml:ph="ok"></span></body>
        </html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_007]);
    }

    #[test]
    fn invalid_data_attrs() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div data-=""/>
            <div data--test=""/>
            <div data-ERR=""/>
            <div data-ok=""/>
        </body></html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_061, HTM_061, HTM_061]);
    }

    #[test]
    fn opf_doctype_errors() {
        let mut report = Report::new();
        check_opf_doctype(
            "<!DOCTYPE html PUBLIC \"x\" \"y\"><package/>",
            "content.opf",
            &mut report,
        );
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_009]);

        let mut report = Report::new();
        check_opf_doctype("<package/>", "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn opf_doctype_with_matching_root_name_is_valid() {
        // A real false positive found via the corpus: the legacy OEB 1.2
        // *Package* doctype's root name ("package") matches the OPF's own
        // root element, so it's explicitly valid - only a root-name
        // mismatch (e.g. an "html" doctype in a "package" document) is an
        // actual error; the PUBLIC identifier itself isn't what's judged.
        let text = r#"<!DOCTYPE package PUBLIC "+//ISBN 0-9673008-1-9//DTD OEB 1.2 Package//EN" "http://openebook.org/dtds/oeb-1.2/oebpkg12.dtd"><package/>"#;
        let mut report = Report::new();
        check_opf_doctype(text, "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn clean_document_no_findings() {
        let xhtml = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body data-ok="1"/></html>"#;
        assert!(run_raw(xhtml).is_empty());
        assert!(run_dom(xhtml).is_empty());
    }
}
