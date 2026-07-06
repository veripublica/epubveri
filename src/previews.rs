//! EPUB Previews 1.0 checks (http://idpf.org/epub/previews/).

use std::collections::HashMap;

use crate::ids::*;
use crate::report::{Position, Report, Severity};

fn elem_text(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect::<String>()
        .trim()
        .to_string()
}

/// §2.4/2.5 Preview Identification: a confirmed Preview publication
/// (`dc:type="preview"`) should (warning) identify its source publication
/// via `dc:source`, and that source must not be the publication's own
/// package identifier (confirmed via a real fixture using the exact same
/// text as `dc:identifier`).
pub(crate) fn check_preview_publication(
    is_preview_pub: bool,
    profile: Option<&str>,
    metadata: Option<roxmltree::Node>,
    package_identifier_text: Option<&str>,
    opf_path: &str,
    report: &mut Report,
) {
    if !is_preview_pub {
        // The 'preview' CLI profile forces treatment as a Preview
        // publication for the purpose of *this one* gating check only -
        // a real fixture (a full EPUB with zero other preview content or
        // metadata at all) expects exactly this one finding and nothing
        // else, not the source-identification checks below cascading on
        // content that was never meant to satisfy them.
        if profile == Some("preview") {
            report.push_at(
                RSC_005,
                Severity::Error,
                "An EPUB Preview publication must have a \"preview\" dc:type",
                opf_path,
            );
        }
        return;
    }
    let Some(md) = metadata else { return };
    let source = md
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "source")
        .map(elem_text);
    match source {
        None => {
            report.push_at_pos(
                RSC_017,
                Severity::Warning,
                "An EPUB Preview publication should link back to its source Publication",
                opf_path,
                Position::of(md),
            );
        }
        Some(text) => {
            if package_identifier_text.is_some_and(|id| id == text) {
                report.push_at_pos(
                    RSC_005,
                    Severity::Error,
                    "A Preview Publication must not use the same package identifier as its source Publication",
                    opf_path,
                    Position::of(md),
                );
            }
        }
    }
}

/// §3.4 Preview Collections: a `<collection role="preview">` must
/// contain exactly one child `<collection role="manifest">` and at least
/// one direct child `<link>` (the preview's own entry points - distinct
/// from the nested manifest collection's own `<link>`s, which follow
/// different rules entirely and are exempt from the generic metadata-link
/// checks elsewhere, confirmed via `preview-embedded-valid`). Each entry-
/// point link must resolve to a real XHTML Content Document (OPF-075
/// otherwise) and must not use an EPUB CFI fragment (OPF-076).
pub(crate) fn check_embedded_preview(
    pkg: &roxmltree::Node,
    items: &HashMap<String, (String, String)>,
    base_dir: &str,
    opf_path: &str,
    report: &mut Report,
) {
    for coll in pkg.children().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "collection"
            && n.attribute("role") == Some("preview")
    }) {
        let manifest_count = coll
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "collection"
                    && n.attribute("role") == Some("manifest")
            })
            .count();
        if manifest_count != 1 {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                "A preview collection must include exactly one child \"manifest\" collection",
                opf_path,
                Position::of(coll),
            );
        }
        let links: Vec<_> = coll
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
            .collect();
        if links.is_empty() {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                "A preview collection must include at least one child \"link\" element",
                opf_path,
                Position::of(coll),
            );
        }
        for link in links {
            let Some(href) = link.attribute("href") else {
                continue;
            };
            if href.contains("epubcfi(") {
                report.push_at_pos(
                    OPF_076,
                    Severity::Error,
                    "a preview link must not use an EPUB CFI fragment",
                    opf_path,
                    Position::of(link),
                );
            }
            if crate::opf::is_external(href) {
                continue;
            }
            let path_part = href.split(['#', '?']).next().unwrap_or(href);
            let resolved = crate::opf::nfc(&crate::opf::resolve(base_dir, path_part));
            let is_xhtml = items
                .values()
                .any(|(p, mt)| crate::opf::nfc(p) == resolved && mt == "application/xhtml+xml");
            if !is_xhtml {
                report.push_at_pos(
                    OPF_075,
                    Severity::Error,
                    "a preview link must target an XHTML Content Document",
                    opf_path,
                    Position::of(link),
                );
            }
        }
    }
}
