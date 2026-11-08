//! EPUB Multiple-Rendition Publications 1.0 checks
//! (`http://idpf.org/epub/renditions/multiple/`). Genuinely publication-
//! level (not scoped to any one rendition's OPF), so this is called once
//! from `lib.rs::validate_bytes`, not from `opf::check` - covers
//! `META-INF/metadata.xml` (the publication-level metadata file, also
//! read by `edupub::check_multi_rendition_dc_type` for its own,
//! EDUPUB-specific purpose), `rendition:*` selection attributes on
//! `<rootfile>` elements, and the optional Rendition Mapping Document
//! referenced via `container.xml`'s `<links>` element (a part of
//! container.xml never parsed before this increment).

use crate::ids::*;
use crate::ocf::{Ocf, parse_error_detail, parse_xml};
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;
use crate::xmlext::XmlTokens;

const RENDITION_NS: &str = "http://www.idpf.org/2013/rendition";
const EPUB_NS: &str = "http://www.idpf.org/2007/ops";
const CONTAINER: &str = "META-INF/container.xml";
const METADATA: &str = "META-INF/metadata.xml";

/// Called once for the whole publication when `opf_paths.len() > 1`
/// (checked by the caller) - everything here is genuinely about the
/// publication as a whole, not any single rendition's OPF.
pub(crate) fn check(ocf: &mut Ocf, report: &mut Report) {
    let Some(bytes) = ocf.read(CONTAINER) else {
        return;
    };
    let text = crate::encoding::decode_xml(&bytes);
    let Ok(doc) = parse_xml(&text) else {
        return;
    };

    check_metadata_file(ocf, report);
    check_mapping_document(ocf, &doc, report);
}

/// RSC-019 (warning) if `META-INF/metadata.xml` is missing entirely;
/// otherwise RSC-005 if its `dcterms:modified` doesn't occur exactly
/// once. Deliberately not routed through `schemas/package.sch` - that
/// schema is scoped to `opf:package` documents, and metadata.xml's root
/// is a different element in the `http://www.idpf.org/2013/metadata`
/// namespace entirely.
fn check_metadata_file(ocf: &mut Ocf, report: &mut Report) {
    let Some(bytes) = ocf.read(METADATA) else {
        report.push_at(
            RSC_019,
            Severity::Warning,
            "a multiple-rendition publication should have a META-INF/metadata.xml file",
            CONTAINER,
        );
        return;
    };
    let text = crate::encoding::decode_xml(&bytes);
    let doc = match parse_xml(&text) {
        Ok(doc) => doc,
        Err(e) => {
            // This file is parsed here and nowhere else, so returning quietly
            // meant a malformed metadata.xml was indistinguishable from a
            // correct one - including from the `dcterms:modified` check just
            // below, which then never ran.
            report.push_full(
                RSC_016,
                Severity::Fatal,
                format!(
                    "{METADATA} is not well-formed XML: {}",
                    parse_error_detail(&text, &e)
                ),
                METADATA,
                Position::of_parse_error(&e),
                "renditions.metadata_malformed_xml",
                Vec::new(),
            );
            return;
        }
    };
    let count = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attr_no_ns("property") == Some("dcterms:modified")
        })
        .count();
    if count != 1 {
        report.push_node(
            RSC_005,
            Severity::Error,
            "the dcterms:modified meta element must occur exactly once",
            METADATA,
            doc.root_element(),
            "renditions.metadata.dcterms_modified_cardinality",
            vec![count.to_string()],
        );
    }
}

/// The container document's Schematron rules (Multiple-Rendition
/// Publications 1.0), which epubcheck runs on every EPUB 3 `container.xml`
/// beside the schema, one rendition or several. Called from
/// `ocf::check_container_schema`, under its conditions; the attribute values
/// themselves are the schema's (`schemas/container.rng`).
///
/// - a `rendition:accessMode` naming one mode twice: RSC-005;
/// - a `rootfile` after the first among its siblings with no `rendition:*`
///   attribute: RSC-017, a warning. Counted per `rootfiles` element and over
///   every rootfile, whatever its media type, as `rootfile[position() > 1]`
///   counts;
/// - more than one mapping link in one `links`: RSC-005, on the `links`;
/// - a mapping link whose media type is not XHTML: RSC-005.
///
/// We had the second and the last two only when several package documents
/// were declared, counted across the whole document and over package
/// rootfiles only, with the mapping link found by a `rel` token: so a second
/// `rootfiles` drew a warning epubcheck does not give, and a single-rendition
/// book drew none of the four.
pub(crate) fn check_container_rules(doc: &roxmltree::Document, report: &mut Report) {
    let in_container = |n: &roxmltree::Node, name: &str| {
        n.is_element()
            && n.tag_name().name() == name
            && n.tag_name().namespace() == Some(crate::rng::CONTAINER_NS)
    };
    for rf in doc.descendants().filter(|n| in_container(n, "rootfile")) {
        if let Some(modes) = rf.attribute((RENDITION_NS, "accessMode")) {
            // `count(tokenize(@a, $mode)) = 3`: the mode occurs exactly twice.
            let twice = modes.xml_tokens().any(|m| modes.matches(m).count() == 2);
            if twice {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "the accessMode attribute contains a duplicate value",
                    CONTAINER,
                    rf,
                    "renditions.rootfile.duplicate_access_mode",
                    vec![modes.to_string()],
                );
            }
        }
        let later = rf
            .prev_siblings()
            .any(|s| s != rf && in_container(&s, "rootfile"));
        if later && !rf.attributes().any(|a| a.namespace() == Some(RENDITION_NS)) {
            report.push_node(
                RSC_017,
                Severity::Warning,
                "at least one rendition selection attribute should be specified for each non-first rootfile element",
                CONTAINER,
                rf,
                "renditions.rootfile.missing_selection_attribute",
                Vec::new(),
            );
        }
    }
    for links in doc.descendants().filter(|n| in_container(n, "links")) {
        let mapping: Vec<_> = links
            .children()
            .filter(|l| in_container(l, "link") && is_mapping_rel(l))
            .collect();
        if mapping.len() > 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "the Container Document must not reference more than one mapping document",
                CONTAINER,
                links,
                "renditions.mapping.multiple_mapping_documents",
                Vec::new(),
            );
        }
        for l in mapping {
            if l.attr_no_ns("media-type").map(normalize_space).as_deref()
                != Some("application/xhtml+xml")
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "the media type of Rendition Mapping Documents must be \"application/xhtml+xml\"",
                    CONTAINER,
                    l,
                    "renditions.mapping.wrong_media_type",
                    Vec::new(),
                );
            }
        }
    }
}

/// `normalize-space(@rel) = 'mapping'`: the whole value, not one token of it.
/// `rel="mapping foo"` is no mapping link to epubcheck's Schematron, and none
/// to `OCFContainerFileHandler` either, which trims and compares.
pub(crate) fn is_mapping_rel(link: &roxmltree::Node) -> bool {
    link.attr_no_ns("rel").map(normalize_space).as_deref() == Some("mapping")
}

fn normalize_space(s: &str) -> String {
    s.xml_tokens().collect::<Vec<_>>().join(" ")
}

/// The Rendition Mapping Document is optional - referenced via
/// container.xml's `<links><link rel="mapping" href="..."
/// media-type="..."/></links>`, entirely outside any OPF manifest. Only
/// checked when such a link actually exists.
fn check_mapping_document(ocf: &mut Ocf, container_doc: &roxmltree::Document, report: &mut Report) {
    // The link `OCFContainerFileHandler.processMappingDoc` registers: `rel`
    // trimmed to `mapping`, a non-empty `href`. The Schematron rules about
    // such links are `check_container_rules`'; a mapping document of the
    // wrong media type is not graded as XHTML, as before.
    let Some(link) = container_doc.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "link"
            && n.tag_name().namespace() == Some(crate::rng::CONTAINER_NS)
            && is_mapping_rel(n)
            && n.attr_no_ns("href").is_some_and(|h| !h.trim().is_empty())
    }) else {
        return;
    };
    if link.attr_no_ns("media-type") != Some("application/xhtml+xml") {
        return;
    }
    let Some(href) = link.attr_no_ns("href") else {
        return;
    };
    let Some(bytes) = ocf.read(href) else {
        return;
    };
    // Graded as XHTML (`MappingDocumentChecker` is given
    // `application/xhtml+xml`), and once: the content-document walk has
    // already graded it if the mapping document is also in a manifest.
    if !report.messages.iter().any(|m| {
        m.location.as_deref() == Some(href)
            && m.rule.is_some_and(|r| r.starts_with("xml.encoding."))
    }) {
        crate::encoding::check(&bytes, href, "application/xhtml+xml", report);
    }
    let text = crate::encoding::decode_xml(&bytes);
    let doc = match parse_xml(&text) {
        Ok(doc) => doc,
        Err(e) => {
            // A mapping document may also be listed in a rendition's
            // manifest, in which case the content-document walk - which runs
            // before this, from `validate_bytes_with_options` - has already
            // reported the parse failure. It may equally not be, in which
            // case this is the only place that can. Asking the report which
            // world we are in beats assuming: assuming it was reported is
            // how a file goes silent, and assuming it wasn't is a duplicate
            // Fatal on one defect.
            let already_reported = report.messages.iter().any(|m| {
                m.location.as_deref() == Some(href)
                    && m.rule.is_some_and(|r| r.ends_with("malformed_xml"))
            });
            if !already_reported {
                report.push_full(
                    RSC_016,
                    Severity::Fatal,
                    format!(
                        "rendition mapping document is not well-formed XML: {}",
                        parse_error_detail(&text, &e)
                    ),
                    href,
                    Position::of_parse_error(&e),
                    "renditions.mapping_malformed_xml",
                    Vec::new(),
                );
            }
            return;
        }
    };

    let has_version_meta = doc.descendants().any(|n| {
        n.is_element()
            && n.tag_name().name() == "meta"
            && n.attr_no_ns("name") == Some("epub.multiple.renditions.version")
            && n.attr_no_ns("content") == Some("1.0")
    });
    if !has_version_meta {
        report.push_node(
            RSC_005,
            Severity::Error,
            "a meta tag with the name \"epub.multiple.renditions.version\" and value \"1.0\" is required",
            href,
            doc.root_element(),
            "renditions.mapping.missing_version_meta",
            Vec::new(),
        );
    }

    let navs: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "nav")
        .collect();
    let resource_map_count = navs
        .iter()
        .filter(|n| n.attribute((EPUB_NS, "type")) == Some("resource-map"))
        .count();
    if resource_map_count != 1 {
        report.push_node(
            RSC_005,
            Severity::Error,
            "a Rendition Mapping Document must contain exactly one \"resource-map\" nav element",
            href,
            doc.root_element(),
            "renditions.mapping.wrong_resource_map_count",
            vec![resource_map_count.to_string()],
        );
    }
    for nav in &navs {
        if nav.attribute((EPUB_NS, "type")).is_none() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "a nav element of a Rendition Mapping Document must identify its nature in an epub:type attribute",
                href,
                *nav,
                "renditions.mapping.nav_missing_epub_type",
                Vec::new(),
            );
        }
    }
}
