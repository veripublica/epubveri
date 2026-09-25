//! EPUB Previews 1.0 checks (<http://idpf.org/epub/previews/>).

use std::collections::HashMap;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

fn elem_text(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect::<String>()
        .trim()
        .to_string()
}

/// §2.4/2.5 Preview Identification (`preview-pub-opf.sch`), which epubcheck
/// attaches on the preview profile or on any dc:type matched
/// case-insensitively (`OPFChecker`'s validator map). Its three rules:
/// a dc:type of exactly `preview`; a `dc:source` without `refines` (warning);
/// and each such `dc:source` differing from the package's own identifier,
/// reported at the `dc:source`.
///
/// A forced profile runs all three. This used to stop after the first,
/// fitted to a fixture; epubcheck 5.4.0 reports the source warning on the
/// same book as well (measured 2026-09-25).
pub(crate) fn check_preview_publication(
    dc_types: &[String],
    profile: Option<&str>,
    metadata: Option<roxmltree::Node>,
    package_identifier_text: Option<&str>,
    opf_path: &str,
    report: &mut Report,
) {
    let applies =
        profile == Some("preview") || dc_types.iter().any(|t| t.eq_ignore_ascii_case("preview"));
    if !applies {
        return;
    }
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some(md) = metadata else {
        if !dc_types.iter().any(|t| t == "preview") {
            report.push_at_rule(
                RSC_005,
                Severity::Error,
                "An EPUB Preview publication must have a \"preview\" dc:type",
                opf_path,
                "previews.metadata.missing_dc_type",
                Vec::new(),
            );
        }
        return;
    };
    if !dc_types.iter().any(|t| t == "preview") {
        report.push_node(
            RSC_005,
            Severity::Error,
            "An EPUB Preview publication must have a \"preview\" dc:type",
            opf_path,
            md,
            "previews.metadata.missing_dc_type",
            Vec::new(),
        );
    }
    let sources: Vec<_> = md
        .children()
        .filter(|n| {
            n.is_element() && n.tag_name().name() == "source" && n.attr_no_ns("refines").is_none()
        })
        .collect();
    if sources.is_empty() {
        report.push_node(
            RSC_017,
            Severity::Warning,
            "An EPUB Preview publication should link back to its source Publication",
            opf_path,
            md,
            "previews.metadata.missing_source_link",
            Vec::new(),
        );
    }
    let own = norm(package_identifier_text.unwrap_or(""));
    for source in sources {
        if norm(&elem_text(source)) == own {
            report.push_node(
                RSC_005,
                Severity::Error,
                "A Preview Publication must not use the same package identifier as its source Publication",
                opf_path,
                source,
                "previews.metadata.identifier_matches_source",
                Vec::new(),
            );
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
            && n.attr_no_ns("role") == Some("preview")
    }) {
        let manifest_count = coll
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "collection"
                    && n.attr_no_ns("role") == Some("manifest")
            })
            .count();
        if manifest_count != 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "A preview collection must include exactly one child \"manifest\" collection",
                opf_path,
                coll,
                "previews.collection.wrong_manifest_count",
                vec![manifest_count.to_string()],
            );
        }
        let links: Vec<_> = coll
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
            .collect();
        if links.is_empty() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "A preview collection must include at least one child \"link\" element",
                opf_path,
                coll,
                "previews.collection.no_links",
                Vec::new(),
            );
        }
        for link in links {
            let Some(href) = link.attr_no_ns("href") else {
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
