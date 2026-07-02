//! OPF package-document checks: version, required metadata, manifest/spine
//! integrity, declared media-types, the EPUB 3 nav doc, and broken internal
//! references from content documents.

use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;

use crate::ids::*;
use crate::ocf::{parse_xml, Ocf};
use crate::report::{Report, Severity};

/// Directory portion of a container path ("OEBPS/x.opf" -> "OEBPS", "x.opf" -> "").
fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decode `%XX` escapes in a single path segment.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Unicode NFC normalization, so href and ZIP entry names compare equal
/// regardless of precomposed/decomposed form.
pub(crate) fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Resolve an href relative to `base_dir` into a container path.
/// Drops fragments/queries; collapses "." and ".."; honors a leading "/";
/// percent-decodes each segment. (Caller NFC-normalizes for comparison.)
pub(crate) fn resolve(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let href = href.split('?').next().unwrap_or(href);

    let mut parts: Vec<String> = Vec::new();
    if !href.starts_with('/') && !base_dir.is_empty() {
        parts.extend(
            base_dir
                .split('/')
                .filter(|p| !p.is_empty())
                .map(String::from),
        );
    }
    for p in href.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(percent_decode(p)),
        }
    }
    parts.join("/")
}

/// True for hrefs we should not resolve against the container (remote/special).
pub(crate) fn is_external(href: &str) -> bool {
    let href = href.trim();
    href.is_empty()
        || href.starts_with('#')
        || href.contains("://")
        || href.starts_with("data:")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
}

/// Maps every `id` attribute in a document to its element's document-order
/// index, for reading-order comparisons (media-overlay text order vs. the
/// content doc's DOM order; the nav toc's fragment order vs. the same).
fn dom_id_order(d: &roxmltree::Document) -> HashMap<String, usize> {
    let mut order = HashMap::new();
    for (i, n) in d.descendants().filter(|n| n.is_element()).enumerate() {
        if let Some(id) = n.attribute("id") {
            order.entry(id.to_string()).or_insert(i);
        }
    }
    order
}

/// Default-vocabulary prefixes EPUB reserves, and the exact URI each is
/// reserved for - the union of every (name, URI) pair confirmed by the
/// real corpus fixtures, both the package-level ones (EPUB 3 appendix D.2
/// default package-metadata vocabularies) and the two content-document
/// ones from a separate fixture (msv, prism), applied uniformly to both
/// attribute locations rather than guessing at a context-specific split
/// beyond what's evidenced. Redeclaring a reserved prefix to its own
/// correct default URI is explicitly allowed (confirmed via
/// `prefix-mapping-reserved-valid.{opf,xhtml}`) - only an override to a
/// *different* URI is a violation.
const RESERVED_PREFIXES: &[(&str, &str)] = &[
    ("a11y", "http://www.idpf.org/epub/vocab/package/a11y/#"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("marc", "http://id.loc.gov/vocabulary/"),
    ("media", "http://www.idpf.org/epub/vocab/overlays/#"),
    (
        "onix",
        "http://www.editeur.org/ONIX/book/codelists/current.html#",
    ),
    ("rendition", "http://www.idpf.org/vocab/rendition/#"),
    ("schema", "http://schema.org/"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("msv", "http://www.idpf.org/epub/vocab/structure/magazine/#"),
    (
        "prism",
        "http://www.prismstandard.org/specifications/3.0/PRISM_CV_Spec_3.0.htm#",
    ),
];

/// The 4 rendition:X (layout/orientation/spread/flow) spine-override
/// families, plus page-spread-* (which also accepts an unprefixed form,
/// confirmed via `rendition-page-spread-itemref-unprefixed-valid.opf`):
/// more than one token sharing the same family in a single itemref's
/// `properties` is RSC-005 "mutually exclusive", regardless of which
/// specific values conflict (confirmed via the real fixtures - each uses
/// a different value pair, but the shape is always "count > 1"). Also
/// flags the itemref-override form of the deprecated `rendition:spread`
/// "portrait" value (OPF-086), same as the global-value check in
/// `schemas/package.sch`.
fn check_itemref_rendition_conflicts(props: &str, path: &str, report: &mut Report) {
    let tokens: Vec<&str> = props.split_whitespace().collect();
    for kind in ["layout", "orientation", "spread", "flow"] {
        let prefix = format!("rendition:{kind}-");
        if tokens.iter().filter(|t| t.starts_with(&prefix)).count() > 1 {
            report.push_at(
                RSC_005,
                Severity::Error,
                format!("rendition:{kind} spine override values are mutually exclusive"),
                path.to_string(),
            );
        }
    }
    if tokens
        .iter()
        .filter(|t| t.starts_with("page-spread-") || t.starts_with("rendition:page-spread-"))
        .count()
        > 1
    {
        report.push_at(
            RSC_005,
            Severity::Error,
            "page-spread-* spine override values are mutually exclusive",
            path.to_string(),
        );
    }
    if tokens.iter().any(|t| *t == "rendition:spread-portrait") {
        report.push_at(
            OPF_086,
            Severity::Warning,
            "the \"portrait\" value of the \"rendition:spread\" property is deprecated",
            path.to_string(),
        );
    }
}

/// OPF-007: a `prefix` (or `epub:prefix`) attribute redeclares one of the
/// reserved default-vocabulary prefixes above to a *different* URI. One
/// warning per occurrence (the corpus counts "once for each reserved
/// prefix", not deduplicated).
fn check_reserved_prefixes(prefix_attr: &str, path: &str, report: &mut Report) {
    let tokens: Vec<&str> = prefix_attr.split_whitespace().collect();
    let mut i = 0;
    while i + 1 < tokens.len() {
        let name = tokens[i].trim_end_matches(':');
        let uri = tokens[i + 1];
        if let Some((_, default_uri)) = RESERVED_PREFIXES.iter().find(|(n, _)| *n == name) {
            if uri != *default_uri {
                report.push_at(
                    OPF_007,
                    Severity::Warning,
                    format!("the '{name}' prefix is reserved and must not be redeclared"),
                    path.to_string(),
                );
            }
        }
        i += 2;
    }
}

pub fn check(ocf: &mut Ocf, opf_path: &str, report: &mut Report) {
    let bytes = match ocf.read(opf_path) {
        Some(b) => b,
        None => {
            report.push(
                OPF_002,
                Severity::Error,
                format!("OPF package document not found: {opf_path}"),
            );
            return;
        }
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    crate::htm::check_opf_doctype(&text, opf_path, report);
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at(
                RSC_005,
                Severity::Error,
                format!("OPF is not well-formed XML: {e}"),
                opf_path,
            );
            return;
        }
    };

    let pkg = doc.root_element();
    if pkg.tag_name().name() != "package" {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF root element is not <package>",
            opf_path,
        );
        return;
    }
    if let Some(prefix) = pkg.attribute("prefix") {
        check_reserved_prefixes(prefix, opf_path, report);
    }

    // --- version ---
    let version = pkg.attribute("version").unwrap_or("");
    if version.is_empty() {
        report.push_at(
            OPF_001,
            Severity::Error,
            "<package> is missing the required 'version' attribute",
            opf_path,
        );
    } else if !(version.starts_with("2.") || version.starts_with("3.")) {
        report.push_at(
            OPF_001,
            Severity::Error,
            format!("Unrecognized EPUB version '{version}'"),
            opf_path,
        );
    }
    let is_epub3 = version.starts_with("3.");
    let is_epub2 = version.starts_with("2.");

    // Schema validation against our own (permissive) package-document RNG.
    // Additive: a structurally non-conformant package is reported as RSC-005.
    if !crate::rng::validate_node(&crate::rng::package_grammar(), pkg) {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF does not conform to the EPUB package-document schema",
            opf_path,
        );
    }

    // Schematron rules our own RNG can't express (id uniqueness,
    // unique-identifier resolution, dcterms:modified cardinality, @refines
    // targets). Same additive pattern, reported as RSC-005.
    for message in crate::schematron::run(&crate::schematron::package_schema(), &doc) {
        report.push_at(RSC_005, Severity::Error, message, opf_path);
    }

    // --- required metadata ---
    // The package's actual identifier text (the dc:identifier named by
    // unique-identifier), used later for the NCX dtb:uid cross-check.
    let mut package_identifier_text: Option<String> = None;
    // Package-level fixed-layout default (individual spine itemrefs can
    // override this via their own 'properties'), used for the viewport/
    // viewBox checks below.
    let mut package_fixed_layout = false;
    // media:active-class / media:playback-active-class: the CSS class a
    // reading system applies to the active/playing media-overlay element,
    // used for the CSS-029/030 cross-referencing pass below.
    let mut media_active_class: Option<String> = None;
    let mut media_playback_active_class: Option<String> = None;
    // This rendition's own dc:type text, and whether a print-source for
    // pagination is identified (dc:source + a meta[property=source-of]
    // refining it to "pagination") - both used by the EDUPUB checks below.
    let mut opf_dc_type: Option<String> = None;
    let mut has_pagination_source = false;
    let metadata = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata");
    if let Some(md) = metadata {
        let elem_text = |n: roxmltree::Node| -> String {
            n.descendants()
                .filter(|t| t.is_text())
                .filter_map(|t| t.text())
                .collect::<String>()
                .trim()
                .to_string()
        };
        let meta_property_text = |property: &str| -> Option<String> {
            md.children()
                .find(|n| {
                    n.is_element()
                        && n.tag_name().name() == "meta"
                        && n.attribute("property") == Some(property)
                })
                .map(elem_text)
        };
        // rendition:spread's "portrait" value is deprecated as a global
        // value (a warning, so hand-coded here rather than via
        // Schematron - crate::schematron::run's caller below maps every
        // finding to RSC-005/Error uniformly, which doesn't fit a
        // deprecation warning with its own dedicated code).
        if meta_property_text("rendition:spread").as_deref() == Some("portrait") {
            report.push_at(
                OPF_086,
                Severity::Warning,
                "the \"portrait\" value of the \"rendition:spread\" property is deprecated",
                opf_path,
            );
        }
        media_active_class = meta_property_text("media:active-class");
        media_playback_active_class = meta_property_text("media:playback-active-class");

        // media:duration values must be valid SMIL3 clock values - reuses
        // the same clock-value grammar the Media Overlays checks already
        // use for clipBegin/clipEnd (src/smil.rs).
        for n in md.children().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("property") == Some("media:duration")
        }) {
            let text = elem_text(n);
            if crate::smil::parse_clock_value(&text).is_none() {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!("media:duration value '{text}' must be a valid SMIL3 clock value"),
                    opf_path,
                );
            }
        }
        // rendition:viewport is deprecated - every occurrence is flagged
        // (not deduplicated), and its value must still parse under the
        // same "key=value,key=value" grammar the fixed-layout viewport
        // checks already use (src/layout.rs).
        for n in md.children().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("property") == Some("rendition:viewport")
        }) {
            report.push_at(
                OPF_086,
                Severity::Warning,
                "the \"rendition:viewport\" property is deprecated",
                opf_path,
            );
            let text = elem_text(n);
            let syntax_ok = text
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .all(|piece| match piece.split_once('=') {
                    Some((key, value)) if !key.trim().is_empty() && !value.trim().is_empty() => {
                        let key = key.trim();
                        let value = value.trim();
                        !matches!(key, "width" | "height")
                            || crate::layout::is_valid_viewport_value(key, value)
                    }
                    _ => false,
                });
            if !syntax_ok {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!("The value of the \"rendition:viewport\" property must be of the form 'width=w,height=h' ('{text}')"),
                    opf_path,
                );
            }
        }
        opf_dc_type = md
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "type")
            .map(elem_text);
        has_pagination_source = md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "source")
            .filter_map(|n| n.attribute("id"))
            .any(|source_id| {
                md.children().any(|n| {
                    n.is_element()
                        && n.tag_name().name() == "meta"
                        && n.attribute("property") == Some("source-of")
                        && n.attribute("refines").map(|r| r.trim_start_matches('#'))
                            == Some(source_id)
                        && elem_text(n) == "pagination"
                })
            });

        package_fixed_layout = md
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attribute("property") == Some("rendition:layout")
            })
            .any(|n| {
                let text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                text.trim() == "pre-paginated"
            });
        let has = |local: &str| {
            md.children()
                .any(|n| n.is_element() && n.tag_name().name() == local)
        };
        if !has("title") {
            report.push_at(
                RSC_005,
                Severity::Error,
                "Required metadata dc:title is missing",
                opf_path,
            );
        }
        if !has("language") {
            report.push_at(
                RSC_005,
                Severity::Error,
                "Required metadata dc:language is missing",
                opf_path,
            );
        }
        let identifiers: Vec<_> = md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
            .collect();
        if identifiers.is_empty() {
            report.push_at(
                RSC_005,
                Severity::Error,
                "Required metadata dc:identifier is missing",
                opf_path,
            );
        }
        if let Some(uid) = pkg.attribute("unique-identifier").map(str::trim) {
            let matching = identifiers
                .iter()
                .find(|n| n.attribute("id").map(str::trim) == Some(uid));
            match matching {
                Some(n) => {
                    package_identifier_text = Some(
                        n.descendants()
                            .filter(|t| t.is_text())
                            .filter_map(|t| t.text())
                            .collect::<String>(),
                    );
                }
                None => {
                    report.push_at(
                        OPF_030,
                        Severity::Error,
                        format!(
                            "package unique-identifier '{uid}' does not match any dc:identifier id"
                        ),
                        opf_path,
                    );
                }
            }
        }

        // --- Media Overlays duration sum (MED-016) ---
        // Package-metadata-only: sum every `refines`-scoped media:duration
        // value and compare against the single un-refined total, 1s
        // tolerance. Silently skipped (no finding) if the total is absent
        // or any part fails to parse, to avoid false positives on
        // partial/malformed data.
        let duration_metas: Vec<_> = md
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attribute("property") == Some("media:duration")
            })
            .collect();
        let total = duration_metas
            .iter()
            .find(|n| n.attribute("refines").is_none())
            .and_then(|n| n.text())
            .and_then(crate::smil::parse_clock_value);
        let parts: Option<Vec<f64>> = duration_metas
            .iter()
            .filter(|n| n.attribute("refines").is_some())
            .map(|n| n.text().and_then(crate::smil::parse_clock_value))
            .collect();
        if let (Some(total), Some(parts)) = (total, parts) {
            if !parts.is_empty() {
                let sum: f64 = parts.iter().sum();
                if (total - sum).abs() > 1.0 {
                    report.push_at(
                        MED_016,
                        Severity::Warning,
                        "media:duration total does not match the sum of overlay durations",
                        opf_path,
                    );
                }
            }
        }
    } else {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF is missing the <metadata> element",
            opf_path,
        );
    }

    let base_dir = parent_dir(opf_path);

    // NFC-normalized index of container entry names -> original name (for
    // existence checks and for reading members back regardless of Unicode form).
    let name_index: HashMap<String, String> =
        ocf.names.iter().map(|n| (nfc(n), n.clone())).collect();

    // --- manifest ---
    // id -> (resolved-path, media-type)
    let mut items: HashMap<String, (String, String)> = HashMap::new();
    // content-doc resolved-path -> declared media-overlay manifest id (raw,
    // resolved to an overlay path once the full manifest is known below).
    let mut media_overlay_attrs: Vec<(String, String)> = Vec::new();
    // manifest id -> its declared 'fallback' manifest id, for spine
    // core-media-type fallback-chain resolution.
    let mut fallback_map: HashMap<String, String> = HashMap::new();
    let mut nav_present = false;
    let mut nav_path: Option<String> = None;
    // Data Navigation Document(s) (properties="data-nav"): (resolved path,
    // media-type), for the Region-Based Navigation checks below.
    let mut data_nav_items: Vec<(String, String)> = Vec::new();
    let manifest = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "manifest");
    if let Some(mn) = manifest {
        let mut seen = HashSet::new();
        for item in mn
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "item")
        {
            let (id, href, mt) = (
                item.attribute("id"),
                item.attribute("href"),
                item.attribute("media-type"),
            );
            let (id, href, mt) = match (id, href, mt) {
                (Some(i), Some(h), Some(m)) => (i.trim(), h, m),
                _ => {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        format!("manifest <item> is missing id/href/media-type (id={id:?})"),
                        opf_path,
                    );
                    continue;
                }
            };
            if !seen.insert(id.to_string()) {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!("duplicate manifest item id '{id}'"),
                    opf_path,
                );
            }
            let resolved = resolve(&base_dir, href);
            if item
                .attribute("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "nav"))
            {
                nav_present = true;
                nav_path = Some(resolved.clone());
            }
            if item
                .attribute("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "data-nav"))
            {
                data_nav_items.push((resolved.clone(), mt.to_string()));
            }
            if !is_external(href) && !name_index.contains_key(&nfc(&resolved)) {
                report.push(
                    RSC_001,
                    Severity::Error,
                    format!("manifest item '{id}' references a missing resource '{href}'"),
                );
            }
            if let Some(mo) = item.attribute("media-overlay") {
                media_overlay_attrs.push((nfc(&resolved), mo.trim().to_string()));
            }
            if let Some(fb) = item.attribute("fallback") {
                fallback_map.insert(id.to_string(), fb.trim().to_string());
            }
            items.insert(id.to_string(), (resolved, mt.to_string()));
        }
    } else {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF is missing the <manifest> element",
            opf_path,
        );
    }

    // content-doc resolved-path -> its declared overlay's resolved-path
    // (once the id it names is resolvable). Used below to cross-reference
    // against what each overlay's <text src> actually references.
    let content_doc_overlay: HashMap<String, String> = media_overlay_attrs
        .into_iter()
        .filter_map(|(doc_path, overlay_id)| {
            items
                .get(&overlay_id)
                .map(|(overlay_path, _)| (doc_path, nfc(overlay_path)))
        })
        .collect();

    // --- spine ---
    // content-doc resolved-path (NFC) -> reading-order position, for the
    // nav toc's spine-order check (NAV-011).
    let mut spine_order: HashMap<String, usize> = HashMap::new();
    // content-doc resolved-path -> whether it's fixed-layout, for the
    // region-based nav's NAV-009 target cross-check below.
    let mut fixed_layout_docs: HashMap<String, bool> = HashMap::new();
    let spine = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "spine");
    if let Some(sp) = spine {
        let refs: Vec<_> = sp
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
            .collect();
        // linear defaults to "yes" when absent; only an explicit "no"
        // (whitespace-trimmed) marks an itemref non-linear. A spine that's
        // empty, or where every itemref is explicitly non-linear, has no
        // linear resources at all.
        if refs
            .iter()
            .all(|ir| ir.attribute("linear").map(str::trim) == Some("no"))
        {
            report.push_at(
                OPF_033,
                Severity::Error,
                "<spine> contains no linear resources",
                opf_path,
            );
        }
        let mut spine_seen: HashSet<&str> = HashSet::new();
        for (position, ir) in refs.into_iter().enumerate() {
            match ir.attribute("idref").map(str::trim) {
                None => report.push_at(
                    RSC_005,
                    Severity::Error,
                    "spine <itemref> is missing 'idref'",
                    opf_path,
                ),
                Some(idref) => {
                    if !spine_seen.insert(idref) {
                        report.push_at(
                            OPF_034,
                            Severity::Error,
                            format!("spine references manifest item id '{idref}' more than once"),
                            opf_path,
                        );
                    }
                    match items.get(idref) {
                        None => report.push_at(
                            OPF_049,
                            Severity::Error,
                            format!("spine itemref idref '{idref}' was not found in the manifest"),
                            opf_path,
                        ),
                        Some((path, mt)) => {
                            spine_order.entry(nfc(path)).or_insert(position);
                            // Core content-document media types valid in the
                            // spine without a fallback; otherwise walk the
                            // 'fallback' chain (bounded, in case of a cycle)
                            // looking for one that resolves to a core type.
                            let is_core =
                                |mt: &str| mt == "application/xhtml+xml" || mt == "image/svg+xml";
                            let mut covered = is_core(mt);
                            let mut cur = idref;
                            let mut hops = 0;
                            while !covered && hops < 10 {
                                let Some(next) = fallback_map.get(cur) else {
                                    break;
                                };
                                let Some((_, next_mt)) = items.get(next.as_str()) else {
                                    break;
                                };
                                covered = is_core(next_mt);
                                cur = next.as_str();
                                hops += 1;
                            }
                            if !covered {
                                report.push_at(
                                    OPF_043,
                                    Severity::Warning,
                                    format!("spine item idref '{idref}' has non-content media-type '{mt}' with no verified fallback"),
                                    opf_path,
                                );
                            }

                            // --- Fixed-layout viewport/viewBox checks ---
                            let props = ir.attribute("properties").unwrap_or("");
                            check_itemref_rendition_conflicts(props, opf_path, report);
                            let is_fixed_layout = if props
                                .split_whitespace()
                                .any(|p| p == "rendition:layout-reflowable")
                            {
                                false
                            } else if props
                                .split_whitespace()
                                .any(|p| p == "rendition:layout-pre-paginated")
                            {
                                true
                            } else {
                                package_fixed_layout
                            };
                            fixed_layout_docs.insert(nfc(path), is_fixed_layout);
                            if let Some(orig) = name_index.get(&nfc(path)).cloned() {
                                if let Some(b) = ocf.read(&orig) {
                                    let t = String::from_utf8_lossy(&b).into_owned();
                                    if let Ok(d) = parse_xml(&t) {
                                        if mt == "application/xhtml+xml" {
                                            if is_fixed_layout {
                                                crate::layout::check_xhtml_viewport(
                                                    &d, path, report,
                                                );
                                            } else {
                                                crate::layout::check_reflowable_viewport(
                                                    &d, path, report,
                                                );
                                            }
                                        } else if mt == "image/svg+xml" && is_fixed_layout {
                                            crate::layout::check_svg_viewbox(&d, path, report);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Table of contents (NCX): required in EPUB 2, and when present the
        // 'toc' attribute must point to an NCX manifest item.
        const NCX: &str = "application/x-dtbncx+xml";
        match sp.attribute("toc").map(str::trim) {
            None => {
                if is_epub2 {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        "EPUB 2 <spine> is missing the required 'toc' (NCX) attribute",
                        opf_path,
                    );
                }
            }
            Some(toc) => match items.get(toc) {
                None => report.push_at(
                    OPF_049,
                    Severity::Error,
                    format!("spine 'toc' idref '{toc}' was not found in the manifest"),
                    opf_path,
                ),
                Some((ncx_path, mt)) => {
                    if mt != NCX {
                        report.push_at(
                            OPF_050,
                            Severity::Error,
                            format!("spine 'toc' references '{toc}' with media-type '{mt}'; an NCX ({NCX}) is expected"),
                            opf_path,
                        );
                    } else if let Some(uid_text) = &package_identifier_text {
                        if let Some(orig) = name_index.get(&nfc(ncx_path)).cloned() {
                            if let Some(b) = ocf.read(&orig) {
                                let ncx_text = String::from_utf8_lossy(&b).into_owned();
                                crate::ncx::check(&ncx_text, ncx_path, uid_text, report);
                            }
                        }
                    }
                }
            },
        }
    } else {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF is missing the <spine> element",
            opf_path,
        );
    }

    // --- Data Navigation Document (EPUB Region-Based Navigation) ---
    if data_nav_items.len() > 1 {
        report.push_at(
            RSC_005,
            Severity::Error,
            "the manifest must not include more than one Data Navigation Document",
            opf_path,
        );
    }
    let data_nav_path: Option<String> = data_nav_items.first().map(|(path, _)| nfc(path));
    if let Some((path, mt)) = data_nav_items.first() {
        if mt != "application/xhtml+xml" {
            report.push_at(
                OPF_012,
                Severity::Error,
                "the Data Navigation Document must be an XHTML content document",
                opf_path,
            );
        }
        if spine_order.contains_key(&nfc(path)) {
            report.push_at(
                OPF_077,
                Severity::Warning,
                "the Data Navigation Document must not be referenced from the spine",
                opf_path,
            );
        }
    }

    // --- EPUB 3 navigation document ---
    // epubcheck enforces this via its package Schematron and reports RSC-005.
    if is_epub3 && !nav_present {
        report.push_at(
            RSC_005,
            Severity::Error,
            "EPUB 3 requires a navigation document (a manifest item with properties=\"nav\")",
            opf_path,
        );
    }

    // --- broken internal references + content-model from content documents ---
    let content_docs: Vec<String> = items
        .values()
        .filter(|(_, mt)| mt == "application/xhtml+xml")
        .map(|(path, _)| path.clone())
        .collect();
    // Which content docs are XHTML (as opposed to e.g. SVG) - the
    // CSS-029/030 cross-referencing pass below only has CSS-collection
    // support for XHTML docs (SVG's own <style>/xml-stylesheet forms are a
    // deliberately deferred, separate extension), so it must not treat an
    // SVG doc's absence from `doc_class_names` as "no CSS found."
    let xhtml_doc_paths: HashSet<String> = content_docs.iter().cloned().collect();
    let xhtml_grammar = crate::rng::xhtml_grammar();
    // content-doc resolved-path -> CSS class names used in its own
    // associated stylesheets (inline <style> + linked <link
    // rel="stylesheet">), for the CSS-029/030 cross-referencing pass below.
    let mut doc_class_names: HashMap<String, HashSet<String>> = HashMap::new();
    // Whether the (required) toc nav has an epub:type="page-list" nav -
    // for the EDUPUB pagination-source cross-check after this loop.
    let mut has_page_list_nav = false;
    for path in content_docs {
        let Some(orig) = name_index.get(&nfc(&path)).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        // BOM-aware decode: a UTF-16-encoded content document read as
        // plain UTF-8 turns into byte-level garbage that fails to parse
        // as XML at all, silently skipping every check below - not just
        // HTM-058 (same fix `css::decode_bytes` already got for
        // stylesheets, reused here rather than duplicated).
        let t = crate::css::decode_bytes(&b);
        crate::htm::check_raw(&b, &t, &path, is_epub3, report);
        let Ok(d) = parse_xml(&t) else {
            continue;
        };
        crate::htm::check_dom(&d, &path, is_epub3, report);

        if let Some(prefix) = d
            .root_element()
            .attribute(("http://www.idpf.org/2007/ops", "prefix"))
        {
            check_reserved_prefixes(prefix, &path, report);
        }

        // EDUPUB: microdata attributes aren't allowed in an edupub content
        // document (applies to every content doc uniformly, nav docs
        // included - no fixture suggests otherwise).
        if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
            crate::edupub::check_content_doc(&d, &path, report);
        }

        let nfc_path = nfc(&path);
        if nav_path.as_deref() == Some(path.as_str()) {
            has_page_list_nav = d.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "nav"
                    && n.attribute(("http://www.idpf.org/2007/ops", "type")) == Some("page-list")
            });
        } else if data_nav_path.as_deref() == Some(nfc_path.as_str()) {
            // Region-Based Navigation: validate the Data Navigation
            // Document's own nav elements and, for the region-based one,
            // its content model + fixed-layout target cross-check.
            if let Some(region_nav) = crate::regionnav::check_data_nav_doc(&d, &path, report) {
                crate::regionnav::check_content_model(region_nav, &path, report);
                let dir_here = parent_dir(&path);
                for href in crate::regionnav::collect_targets(region_nav) {
                    if is_external(&href) {
                        continue;
                    }
                    let target = nfc(&resolve(&dir_here, &href));
                    if fixed_layout_docs.get(&target) == Some(&false) {
                        report.push_at(
                            NAV_009,
                            Severity::Error,
                            format!(
                                "region-based nav target '{href}' is not a fixed-layout document"
                            ),
                            path.clone(),
                        );
                    }
                }
            }
        } else {
            // Region-based navigation belongs only in the Data Navigation
            // Document - anywhere else it's misplaced (HTM-052).
            crate::regionnav::check_misplaced(&d, &path, report);
        }

        // Schema validation against our own XHTML content-document RNG.
        // Additive: a non-conformant content document is reported as RSC-005.
        if !crate::rng::validate_node(&xhtml_grammar, d.root_element()) {
            report.push_at(
                RSC_005,
                Severity::Error,
                "content document does not conform to the EPUB XHTML content-model schema",
                path.clone(),
            );
        }

        // <title> present but empty.
        if let Some(title) = d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "title")
        {
            // `Node::text()` returns content for comment nodes too, not
            // just text nodes - filter to real text first, or a title
            // containing only a comment (e.g. `<title><!--x--></title>`)
            // would be mistaken for having real content.
            let text: String = title
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            if text.trim().is_empty() {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "\"title\" must not be empty",
                    path.clone(),
                );
            }
        }

        // Both an http-equiv Content-Type meta and a charset meta declared.
        let has_http_equiv_content_type = d.descendants().any(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("http-equiv")
                    .is_some_and(|v| v.eq_ignore_ascii_case("content-type"))
        });
        let has_charset_meta = d.descendants().any(|n| {
            n.is_element() && n.tag_name().name() == "meta" && n.attribute("charset").is_some()
        });
        if has_http_equiv_content_type && has_charset_meta {
            report.push_at(
                RSC_005,
                Severity::Error,
                "must not contain both a meta element in encoding declaration state (http-equiv='content-type') and a meta element with the charset attribute",
                path.clone(),
            );
        }

        // epub:switch is deprecated - a separate, additive signal alongside
        // whatever structural case/default sequencing schemas/xhtml.rng
        // already enforces on it. Namespace-checked: SVG has its own,
        // unrelated native <switch> element (conditional rendering), which
        // a local-name-only match would misidentify as epub:switch.
        const EPUB_NS: &str = "http://www.idpf.org/2007/ops";
        for _ in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "switch"
                && n.tag_name().namespace() == Some(EPUB_NS)
        }) {
            report.push_at(
                RSC_017,
                Severity::Warning,
                "The \"epub:switch\" element is deprecated",
                path.clone(),
            );
        }

        let dir = parent_dir(&path);

        // --- Navigation document checks (NAV-010/011) ---
        if nav_path.as_deref() == Some(path.as_str()) {
            // NAV-010: external links inside the required toc/page-list/
            // landmarks nav elements aren't allowed (links to remote
            // resources are fine in other, custom nav types).
            for nav_el in d
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "nav")
            {
                let nav_type = nav_el.attribute((EPUB_NS, "type"));
                if !matches!(
                    nav_type,
                    Some("toc") | Some("page-list") | Some("landmarks")
                ) {
                    continue;
                }
                for a in nav_el
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "a")
                {
                    if let Some(href) = a.attribute("href") {
                        if is_external(href) {
                            report.push_at(
                                NAV_010,
                                Severity::Error,
                                format!("external link '{href}' in a toc/page-list/landmarks nav"),
                                path.clone(),
                            );
                        }
                    }
                }
            }

            // NAV-011: the toc nav's links, in nav order, should match
            // reading order - spine order first, then (for links into the
            // same document) DOM order, with a fragment-less link ("the
            // whole document") sorting before any fragment into it. Scored
            // as adjacent-pair inversions, not "any disorder = 1 finding"
            // (confirmed against the real corpus: a single spine-order
            // mistake reports once, two fragment-order mistakes report
            // twice).
            if let Some(toc_nav) = d.descendants().find(|n| {
                n.is_element()
                    && n.tag_name().name() == "nav"
                    && n.attribute((EPUB_NS, "type")) == Some("toc")
            }) {
                let mut id_order_cache: HashMap<String, HashMap<String, usize>> = HashMap::new();
                // (spine_idx, dom_idx): dom_idx is 0 for a fragment-less
                // link ("the whole document") and real-fragment-index + 1
                // otherwise, so it always sorts before any real fragment
                // into the same document without needing a separate flag.
                let mut keys: Vec<(usize, usize)> = Vec::new();
                for a in toc_nav
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "a")
                {
                    let Some(href) = a.attribute("href") else {
                        continue;
                    };
                    if is_external(href) {
                        continue;
                    }
                    let (path_part, frag) = match href.split_once('#') {
                        Some((p, f)) => (p, Some(f)),
                        None => (href, None),
                    };
                    let resolved_nfc = nfc(&resolve(&dir, path_part));
                    let Some(&spine_idx) = spine_order.get(&resolved_nfc) else {
                        continue;
                    };
                    let dom_idx = match frag {
                        None => 0,
                        Some(f) => {
                            if !id_order_cache.contains_key(&resolved_nfc) {
                                let order = name_index
                                    .get(&resolved_nfc)
                                    .and_then(|orig| ocf.read(orig))
                                    .and_then(|b| {
                                        let t = String::from_utf8_lossy(&b).into_owned();
                                        parse_xml(&t).ok().map(|d2| dom_id_order(&d2))
                                    })
                                    .unwrap_or_default();
                                id_order_cache.insert(resolved_nfc.clone(), order);
                            }
                            // Missing ids are already caught elsewhere as
                            // broken references; skip this link here
                            // rather than letting it break the comparison.
                            match id_order_cache[&resolved_nfc].get(f) {
                                Some(&idx) => idx + 1,
                                None => continue,
                            }
                        }
                    };
                    keys.push((spine_idx, dom_idx));
                }
                for w in keys.windows(2) {
                    if w[0] > w[1] {
                        report.push_at(
                            NAV_011,
                            Severity::Warning,
                            "toc nav link order does not match reading order",
                            path.clone(),
                        );
                    }
                }
            }
        }

        for node in d.descendants().filter(|n| n.is_element()) {
            // <base href> sets a base URI for resolving *other* relative
            // references; it isn't itself a reference to an existing
            // resource (and may legitimately point at "./" or elsewhere).
            if node.tag_name().name() == "base" {
                continue;
            }
            for attr in ["src", "href"] {
                if let Some(v) = node.attribute(attr) {
                    if is_external(v) {
                        continue;
                    }
                    let resolved = resolve(&dir, v);
                    if !name_index.contains_key(&nfc(&resolved)) {
                        report.push_at(
                            RSC_001,
                            Severity::Error,
                            format!("references a missing resource '{v}'"),
                            path.clone(),
                        );
                    }
                }
            }
            // Embedded CSS: inline <style> resolves relative to this
            // content document's own location, not to any separate file.
            if node.tag_name().name() == "style" {
                let css_text: String = node
                    .descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect();
                crate::css::check(&css_text, &path, &dir, &name_index, None, report);
                let sheet = styloria::Parser::parse_stylesheet(&css_text);
                doc_class_names
                    .entry(path.clone())
                    .or_default()
                    .extend(crate::css::selector_class_names(&sheet));
            }
            // A linked stylesheet also counts as this document's own CSS
            // (for the CSS-029/030 media-overlay class cross-reference
            // below) - its own findings are already reported separately
            // via the manifest text/css loop further down.
            if node.tag_name().name() == "link"
                && node.attribute("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
            {
                if let Some(href) = node.attribute("href") {
                    if !is_external(href) {
                        let resolved = resolve(&dir, href);
                        if let Some(orig) = name_index.get(&nfc(&resolved)).cloned() {
                            if let Some(b) = ocf.read(&orig) {
                                let css_text = crate::css::decode_bytes(&b);
                                let sheet = styloria::Parser::parse_stylesheet(&css_text);
                                doc_class_names
                                    .entry(path.clone())
                                    .or_default()
                                    .extend(crate::css::selector_class_names(&sheet));
                            }
                        }
                    }
                }
            }
        }
    }

    // --- EDUPUB pagination source / page-list cross-check (NAV-003/OPF-066) ---
    if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
        crate::edupub::check_page_list(has_pagination_source, has_page_list_nav, opf_path, report);
    }

    // --- Media-overlay active-class CSS cross-referencing (CSS-029/030) ---
    const WELL_KNOWN_ACTIVE_CLASS: &str = "-epub-media-overlay-active";
    const WELL_KNOWN_PLAYBACK_CLASS: &str = "-epub-media-overlay-playing";

    // CSS-029 (usage): a well-known class name is used as a CSS selector
    // somewhere, but its corresponding property isn't declared at all.
    for (well_known, declared) in [
        (WELL_KNOWN_ACTIVE_CLASS, media_active_class.is_some()),
        (
            WELL_KNOWN_PLAYBACK_CLASS,
            media_playback_active_class.is_some(),
        ),
    ] {
        if declared {
            continue;
        }
        for (doc_path, classes) in &doc_class_names {
            if classes.contains(well_known) {
                report.push_at(
                    CSS_029,
                    Severity::Info,
                    format!("well-known media-overlay class '{well_known}' is used but not declared in the package metadata"),
                    doc_path.clone(),
                );
            }
        }
    }

    // CSS-030: a declared property has no matching CSS selector in the
    // content document its media overlay actually applies to.
    let empty_classes: HashSet<String> = HashSet::new();
    for doc_path in content_doc_overlay
        .keys()
        .filter(|p| xhtml_doc_paths.contains(p.as_str()))
    {
        let classes = doc_class_names.get(doc_path).unwrap_or(&empty_classes);
        for (property_name, declared_class) in [
            ("media:active-class", &media_active_class),
            ("media:playback-active-class", &media_playback_active_class),
        ] {
            if let Some(name) = declared_class {
                if !classes.contains(name.as_str()) {
                    report.push_at(
                        CSS_030,
                        Severity::Error,
                        format!("{property_name} '{name}' has no matching CSS selector in this content document"),
                        doc_path.clone(),
                    );
                }
            }
        }
    }

    // --- CSS resources declared in the manifest ---
    let css_items: Vec<String> = items
        .values()
        .filter(|(_, mt)| mt == "text/css")
        .map(|(path, _)| path.clone())
        .collect();
    for path in css_items {
        let Some(orig) = name_index.get(&nfc(&path)).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        let css_text = crate::css::decode_bytes(&b);
        let dir = parent_dir(&path);
        crate::css::check(&css_text, &path, &dir, &name_index, Some(&b), report);
    }

    // --- Media Overlays (SMIL) ---
    // resolved-path -> media-type, for the audio Core Media Type check.
    let media_type_index: HashMap<String, String> = items
        .values()
        .map(|(path, mt)| (nfc(path), mt.clone()))
        .collect();
    let smil_items: Vec<String> = items
        .values()
        .filter(|(_, mt)| mt == "application/smil+xml")
        .map(|(path, _)| path.clone())
        .collect();
    // content-doc resolved-path -> set of distinct overlay resolved-paths
    // that reference it via <text src>, for the cross-referencing pass below.
    let mut referenced_by: HashMap<String, HashSet<String>> = HashMap::new();
    for path in smil_items {
        let Some(orig) = name_index.get(&nfc(&path)).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        let smil_text = String::from_utf8_lossy(&b).into_owned();
        let dir = parent_dir(&path);
        let overlay_path = nfc(&path);
        let targets = crate::smil::check(
            &smil_text,
            &path,
            &dir,
            &name_index,
            &media_type_index,
            report,
        );

        // MED-015: this overlay's <text> targets, in SMIL sequence order,
        // should appear in the same relative order as the ids they name in
        // the referenced content document's own DOM. Grouped by content
        // doc (an overlay typically covers one), order preserved within
        // each group; only checked once a doc has 2+ referenced ids (a
        // single id is trivially "in order").
        let mut doc_groups: HashMap<String, Vec<String>> = HashMap::new();
        for (content_doc_path, frag) in &targets {
            doc_groups
                .entry(content_doc_path.clone())
                .or_default()
                .push(frag.clone());
        }
        for (content_doc_path, frags) in &doc_groups {
            if frags.len() < 2 {
                continue;
            }
            let Some(orig) = name_index.get(content_doc_path).cloned() else {
                continue;
            };
            let Some(b) = ocf.read(&orig) else { continue };
            let t = String::from_utf8_lossy(&b).into_owned();
            let Ok(d) = parse_xml(&t) else { continue };
            let id_order = dom_id_order(&d);
            // Ids the SMIL references but the doc doesn't have are already
            // separately caught as broken references elsewhere - skip them
            // here rather than letting a missing id break the comparison.
            let indices: Vec<usize> = frags
                .iter()
                .filter_map(|f| id_order.get(f).copied())
                .collect();
            let in_order = indices.windows(2).all(|w| w[0] <= w[1]);
            if !in_order && indices.len() >= 2 {
                report.push_at(
                    MED_015,
                    Severity::Info,
                    "media overlay <text> order does not match the content document's DOM order",
                    path.clone(),
                );
            }
        }

        for (content_doc_path, _frag) in targets {
            referenced_by
                .entry(content_doc_path)
                .or_default()
                .insert(overlay_path.clone());
        }
    }

    let all_docs: HashSet<&String> = content_doc_overlay
        .keys()
        .chain(referenced_by.keys())
        .collect();
    for content_doc_path in all_docs {
        let declared = content_doc_overlay.get(content_doc_path);
        let actual = referenced_by.get(content_doc_path);
        match actual.map(|s| s.len()).unwrap_or(0) {
            0 => {
                if declared.is_some() {
                    report.push_at(
                        MED_013,
                        Severity::Error,
                        "content document declares a media-overlay attribute but is not referenced from that overlay",
                        content_doc_path.clone(),
                    );
                }
            }
            1 => {
                let actual_overlay = actual.unwrap().iter().next().unwrap();
                match declared {
                    None => report.push_at(
                        MED_010,
                        Severity::Error,
                        "content document is referenced from a media overlay but has no media-overlay attribute",
                        content_doc_path.clone(),
                    ),
                    Some(d) if d != actual_overlay => report.push_at(
                        MED_012,
                        Severity::Error,
                        "content document references the wrong media overlay",
                        content_doc_path.clone(),
                    ),
                    Some(_) => {}
                }
            }
            _ => {
                report.push_at(
                    MED_011,
                    Severity::Error,
                    "content document is declared/referenced in more than one media overlay",
                    content_doc_path.clone(),
                );
            }
        }
    }

    check_font_obfuscation(ocf, &items, &name_index, report);
}

/// Recognized font Core Media Types, assembled from every real media-type
/// string used across the corpus's font-related fixtures (not guessed):
/// the modern, preferred IANA-registered types plus the non-preferred but
/// still-valid legacy aliases, and SVG (which reuses the existing SVG core
/// type for SVG fonts).
const FONT_CORE_MEDIA_TYPES: [&str; 10] = [
    "font/otf",
    "font/ttf",
    "font/woff",
    "font/woff2",
    "application/font-sfnt",
    "application/font-woff",
    "application/x-font-ttf",
    "application/x-font-woff",
    "application/vnd.ms-opentype",
    "image/svg+xml",
];
const OBFUSCATION_ALGORITHM: &str = "http://www.idpf.org/2008/embedding";

/// A resource obfuscated with the IDPF font-obfuscation algorithm must
/// declare a font Core Media Type in the manifest. `ocf::check_encryption`
/// (which runs before the OPF is even parsed) already reports every
/// encrypted resource as RSC-004; this is additive, and needs the
/// manifest's id -> (path, media-type) map, so it can only run here.
fn check_font_obfuscation(
    ocf: &mut Ocf,
    items: &HashMap<String, (String, String)>,
    name_index: &HashMap<String, String>,
    report: &mut Report,
) {
    const ENC: &str = "META-INF/encryption.xml";
    let Some(bytes) = ocf.read(ENC) else { return };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = parse_xml(&text) else { return };

    for enc_data in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "EncryptedData")
    {
        let algorithm = enc_data
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "EncryptionMethod")
            .and_then(|n| n.attribute("Algorithm"));
        if algorithm != Some(OBFUSCATION_ALGORITHM) {
            continue;
        }
        let Some(uri) = enc_data
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "CipherReference")
            .and_then(|n| n.attribute("URI"))
        else {
            continue;
        };
        // CipherReference URI is relative to the OCF container root, not
        // the OPF's own directory (confirmed via the real fixtures: the
        // OPF lives at "EPUB/package.opf" but the cipher reference reads
        // "EPUB/obfuscated-font.otf", the full container-relative path).
        let resolved = nfc(&resolve("", uri));
        if !name_index.contains_key(&resolved) {
            continue; // a missing resource is already reported elsewhere (RSC-001/004)
        }
        let media_type = items
            .values()
            .find(|(path, _)| nfc(path) == resolved)
            .map(|(_, mt)| mt.as_str());
        if !media_type.is_some_and(|mt| FONT_CORE_MEDIA_TYPES.contains(&mt)) {
            report.push_at(
                PKG_026,
                Severity::Error,
                format!("obfuscated resource '{uri}' is not a font Core Media Type"),
                ENC,
            );
        }
    }
}
