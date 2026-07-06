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
use crate::ocf::{parse_xml, Ocf};
use crate::report::{Position, Report, Severity};

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
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = parse_xml(&text) else {
        return;
    };

    check_metadata_file(ocf, report);
    check_rendition_selection(&doc, report);
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
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = parse_xml(&text) else {
        return;
    };
    let count = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("property") == Some("dcterms:modified")
        })
        .count();
    if count != 1 {
        report.push_full(
            RSC_005,
            Severity::Error,
            "the dcterms:modified meta element must occur exactly once",
            METADATA,
            Position::of(doc.root_element()),
            "renditions.metadata.dcterms_modified_cardinality",
            vec![count.to_string()],
        );
    }
}

/// Every `rendition:*` attribute on a `<rootfile>` is a "rendition
/// selection attribute". Only `media`/`layout` are real (confirmed via
/// the EPUB Multiple-Renditions spec); anything else is RSC-005. A
/// non-first rootfile with none at all is RSC-017 (the first is the
/// default rendition and needs none).
fn check_rendition_selection(container_doc: &roxmltree::Document, report: &mut Report) {
    let rootfiles: Vec<_> = container_doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "rootfile"
                && n.attribute("media-type") == Some("application/oebps-package+xml")
        })
        .collect();
    for (i, rf) in rootfiles.iter().enumerate() {
        let selection_attrs: Vec<_> = rf
            .attributes()
            .filter(|a| a.namespace() == Some(RENDITION_NS))
            .collect();
        if i > 0 && selection_attrs.is_empty() {
            report.push_full(
                RSC_017,
                Severity::Warning,
                "at least one rendition selection attribute should be specified for each non-first rootfile element",
                CONTAINER,
                Position::of(*rf),
                "renditions.rootfile.missing_selection_attribute",
                Vec::new(),
            );
        }
        for attr in &selection_attrs {
            match attr.name() {
                "media" => {
                    let value = attr.value();
                    if !(value.contains('(') && value.contains(')')) {
                        report.push_full(
                            RSC_005,
                            Severity::Error,
                            "value of attribute \"rendition:media\" is invalid",
                            CONTAINER,
                            Position::of(*rf),
                            "renditions.rootfile.invalid_media_selection",
                            vec![value.to_string()],
                        );
                    }
                }
                "layout" => {}
                other => {
                    report.push_full(
                        RSC_005,
                        Severity::Error,
                        format!("attribute \"rendition:{other}\" not allowed here"),
                        CONTAINER,
                        Position::of(*rf),
                        "renditions.rootfile.unknown_selection_attribute",
                        vec![other.to_string()],
                    );
                }
            }
        }
    }
}

/// The Rendition Mapping Document is optional - referenced via
/// container.xml's `<links><link rel="mapping" href="..."
/// media-type="..."/></links>`, entirely outside any OPF manifest. Only
/// checked when such a link actually exists.
fn check_mapping_document(ocf: &mut Ocf, container_doc: &roxmltree::Document, report: &mut Report) {
    let mapping_links: Vec<_> = container_doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "link"
                && n.attribute("rel")
                    .is_some_and(|r| r.split_whitespace().any(|t| t == "mapping"))
        })
        .collect();
    if let Some(second) = mapping_links.get(1) {
        report.push_full(
            RSC_005,
            Severity::Error,
            "the Container Document must not reference more than one mapping document",
            CONTAINER,
            Position::of(*second),
            "renditions.mapping.multiple_mapping_documents",
            Vec::new(),
        );
    }
    let Some(link) = mapping_links.first() else {
        return;
    };
    if link.attribute("media-type") != Some("application/xhtml+xml") {
        report.push_full(
            RSC_005,
            Severity::Error,
            "the media type of Rendition Mapping Documents must be \"application/xhtml+xml\"",
            CONTAINER,
            Position::of(*link),
            "renditions.mapping.wrong_media_type",
            Vec::new(),
        );
        return;
    }
    let Some(href) = link.attribute("href") else {
        return;
    };
    let Some(bytes) = ocf.read(href) else {
        return;
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = parse_xml(&text) else {
        return;
    };

    let has_version_meta = doc.descendants().any(|n| {
        n.is_element()
            && n.tag_name().name() == "meta"
            && n.attribute("name") == Some("epub.multiple.renditions.version")
            && n.attribute("content") == Some("1.0")
    });
    if !has_version_meta {
        report.push_full(
            RSC_005,
            Severity::Error,
            "a meta tag with the name \"epub.multiple.renditions.version\" and value \"1.0\" is required",
            href,
            Position::of(doc.root_element()),
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
        report.push_full(
            RSC_005,
            Severity::Error,
            "a Rendition Mapping Document must contain exactly one \"resource-map\" nav element",
            href,
            Position::of(doc.root_element()),
            "renditions.mapping.wrong_resource_map_count",
            vec![resource_map_count.to_string()],
        );
    }
    for nav in &navs {
        if nav.attribute((EPUB_NS, "type")).is_none() {
            report.push_full(
                RSC_005,
                Severity::Error,
                "a nav element of a Rendition Mapping Document must identify its nature in an epub:type attribute",
                href,
                Position::of(*nav),
                "renditions.mapping.nav_missing_epub_type",
                Vec::new(),
            );
        }
    }
}
