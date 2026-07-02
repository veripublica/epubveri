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
    let metadata = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata");
    if let Some(md) = metadata {
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
            if !identifiers
                .iter()
                .any(|n| n.attribute("id").map(str::trim) == Some(uid))
            {
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
    let mut nav_present = false;
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
            if item
                .attribute("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "nav"))
            {
                nav_present = true;
            }
            let resolved = resolve(&base_dir, href);
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
    let spine = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "spine");
    if let Some(sp) = spine {
        let refs: Vec<_> = sp
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
            .collect();
        if refs.is_empty() {
            report.push_at(
                OPF_033,
                Severity::Error,
                "<spine> contains no linear resources",
                opf_path,
            );
        }
        let mut spine_seen: HashSet<&str> = HashSet::new();
        for ir in refs {
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
                        Some((_, mt)) => {
                            // Core content-document media types valid in the spine
                            // without a fallback. (We do not yet trace fallback
                            // chains, so this only flags the no-fallback common case.)
                            let is_core = mt == "application/xhtml+xml" || mt == "image/svg+xml";
                            if !is_core {
                                report.push_at(
                                    OPF_043,
                                    Severity::Warning,
                                    format!("spine item idref '{idref}' has non-content media-type '{mt}' with no verified fallback"),
                                    opf_path,
                                );
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
                Some((_, mt)) => {
                    if mt != NCX {
                        report.push_at(
                            OPF_050,
                            Severity::Error,
                            format!("spine 'toc' references '{toc}' with media-type '{mt}'; an NCX ({NCX}) is expected"),
                            opf_path,
                        );
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
        let t = String::from_utf8_lossy(&b).into_owned();
        let Ok(d) = parse_xml(&t) else {
            continue;
        };

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

        let dir = parent_dir(&path);
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
                crate::css::check(&css_text, &path, &dir, &name_index, report);
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
        crate::css::check(&css_text, &path, &dir, &name_index, report);
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
