//! EDUPUB profile checks (`http://idpf.org/epub/profile/edupub/`),
//! triggered by `<dc:type>edupub</dc:type>` — either in a single-rendition
//! book's own OPF, or in `META-INF/metadata.xml` (a separate,
//! publication-level metadata file used only for multi-rendition
//! packages). Deliberately narrow: only the checks confirmed by real
//! corpus fixtures (HTML5 microdata attributes, the page-list/pagination-
//! source cross-reference, and the multi-rendition `dc:type` cardinality
//! checks wired in `opf.rs`) — not the full EDUPUB conformance suite
//! (sectioning rules, accessibility metadata, etc.), which the corpus
//! itself only exercises indirectly via `-valid` fixtures with no
//! dedicated error codes to target.

use crate::ids::*;
use crate::report::{Report, Severity};

pub(crate) fn is_edupub(dc_type: Option<&str>) -> bool {
    dc_type == Some("edupub")
}

/// HTM-051: HTML5 microdata items (rooted at an `itemscope` attribute)
/// aren't allowed in an edupub content document. Only `itemscope` is
/// checked, not `itemtype`/`itemprop` independently - confirmed via the
/// real corpus fixture, which has both an `itemscope`-bearing element and
/// a separate `itemprop`-only element (a property *of* that same item,
/// not a second item) but expects exactly one finding, not two.
pub(crate) fn check_content_doc(d: &roxmltree::Document, path: &str, report: &mut Report) {
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.attribute("itemscope").is_some() {
            report.push_at(
                HTM_051,
                Severity::Warning,
                "HTML5 microdata items are not allowed in an edupub content document",
                path,
            );
        }
    }
}

/// NAV-003 / OPF-066: an edupub publication that identifies a print-source
/// for pagination (`dc:source` + `<meta property="source-of"
/// refines="#...">pagination</meta>`) must have a `page-list` nav, and
/// vice versa - a `page-list` nav implies a print-source should be named.
pub(crate) fn check_page_list(
    has_pagination_source: bool,
    has_page_list_nav: bool,
    opf_path: &str,
    report: &mut Report,
) {
    match (has_pagination_source, has_page_list_nav) {
        (true, false) => {
            report.push_at(
                NAV_003,
                Severity::Error,
                "a pagination source is identified but the navigation document has no page-list nav",
                opf_path,
            );
        }
        (false, true) => {
            report.push_at(
                OPF_066,
                Severity::Error,
                "a page-list nav is present but no print-source for pagination is identified",
                opf_path,
            );
        }
        _ => {}
    }
}

const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

fn elem_text(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect::<String>()
        .trim()
        .to_string()
}

fn dc_type_of(ocf: &mut crate::ocf::Ocf, path: &str) -> Option<String> {
    let bytes = ocf.read(path)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = crate::ocf::parse_xml(&text).ok()?;
    doc.descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "type"
                && n.tag_name().namespace() == Some(DC_NS)
        })
        .map(elem_text)
}

/// Multi-rendition `dc:type` cardinality (both RSC-005): a multi-rendition
/// publication is "edupub" if *either* `META-INF/metadata.xml` (the
/// publication-level metadata) or *any* rendition's own OPF declares
/// `dc:type=edupub` - confirmed via the real corpus fixtures, where the
/// "publication-level missing" scenario has metadata.xml's own dc:type
/// commented out while *both* renditions still declare edupub (proving
/// the trigger isn't "metadata.xml always needs a dc:type", which would
/// have been a false positive on every ordinary, non-edupub multi-
/// rendition package). Once a publication is edupub by that definition,
/// every level (metadata.xml and each rendition) must declare it too;
/// whichever level doesn't gets its own RSC-005. Checked once for the
/// whole publication (not per-rendition, unlike the other EDUPUB checks)
/// since it needs `metadata.xml`, which `opf::check` never sees.
pub(crate) fn check_multi_rendition_dc_type(
    ocf: &mut crate::ocf::Ocf,
    opf_paths: &[String],
    report: &mut Report,
) {
    const METADATA: &str = "META-INF/metadata.xml";
    if !ocf.has(METADATA) {
        return;
    }
    let pub_dc_type = dc_type_of(ocf, METADATA);
    let rendition_dc_types: Vec<(String, Option<String>)> = opf_paths
        .iter()
        .map(|p| (p.clone(), dc_type_of(ocf, p)))
        .collect();

    let is_edupub_pub = is_edupub(pub_dc_type.as_deref())
        || rendition_dc_types
            .iter()
            .any(|(_, t)| is_edupub(t.as_deref()));
    if !is_edupub_pub {
        return;
    }

    if !is_edupub(pub_dc_type.as_deref()) {
        report.push_at(
            RSC_005,
            Severity::Error,
            "META-INF/metadata.xml is missing the publication-level dc:type",
            METADATA,
        );
    }
    for (opf_path, dc_type) in &rendition_dc_types {
        if !is_edupub(dc_type.as_deref()) {
            report.push_at(
                RSC_005,
                Severity::Error,
                "this rendition is missing dc:type for an edupub multi-rendition publication",
                opf_path.clone(),
            );
        }
    }
}
