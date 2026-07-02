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
    let metadata = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata");
    if let Some(md) = metadata {
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
    let xhtml_grammar = crate::rng::xhtml_grammar();
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
}
