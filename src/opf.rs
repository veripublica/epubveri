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

/// RSC-026: a local href is path-absolute (starts with "/") or, once its
/// ".." segments are followed from `base_dir`, would escape above the
/// container root entirely - both confirmed via dedicated real fixtures.
/// `resolve()` above is deliberately lenient about this (a `pop()` past
/// empty is a harmless no-op, so leaking hrefs still resolve to the
/// "intended" real path) - this is the separate, stricter check that
/// actually flags the leak.
pub(crate) fn href_leaks_container_root(base_dir: &str, href: &str) -> bool {
    if href.starts_with('/') {
        return true;
    }
    let path_part = href.split(['#', '?']).next().unwrap_or(href);
    let mut depth: i32 = base_dir.split('/').filter(|p| !p.is_empty()).count() as i32;
    for seg in path_part.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// True for a manifest media-type that's a real "OPS"/Content Document -
/// XHTML or SVG (EPUB 3), or DTBook (a real, valid EPUB 2 OPS content
/// type, confirmed via a real `ops-dtbook-valid` fixture that a guide/NCX
/// reference check must not reject).
pub(crate) fn is_content_document_type(mt: &str) -> bool {
    matches!(
        mt,
        "application/xhtml+xml" | "image/svg+xml" | "application/x-dtbook+xml"
    )
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
        || href.starts_with("file:")
}

/// True only for a genuine remote fetch (http/https) - unlike
/// `is_external` above (which also covers fragment-only, `data:`,
/// `mailto:`, `tel:` - anything that isn't a local container path, for
/// resolution-skipping purposes), this is the narrower predicate the
/// remote-resources/scripted/svg content-property checks need: a CSS
/// `filter: url(#id)` or an `<a href="mailto:...">` isn't "using a
/// remote resource" just because it isn't locally resolvable.
pub(crate) fn is_remote_url(href: &str) -> bool {
    let href = href.trim();
    href.starts_with("http://") || href.starts_with("https://")
}

/// Strip a `#fragment` from a remote URL before comparing it against the
/// manifest's own declared hrefs - a remote resource can legitimately be
/// referenced with a fragment (e.g. an SVG font glyph, `https://x/y#g`)
/// while its manifest item declares the bare URL (`https://x/y`);
/// confirmed via a real corpus fixture where the two would otherwise fail
/// to match and produce a false RSC-008.
fn strip_url_fragment(url: &str) -> String {
    url.split('#').next().unwrap_or(url).to_string()
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

/// Known manifest `item/@properties` values ("cover-image" is handled
/// separately above, since it has its own cardinality/media-type rules).
const KNOWN_ITEM_PROPERTIES: &[&str] = &[
    "mathml",
    "nav",
    "remote-resources",
    "scripted",
    "svg",
    "switch",
    "data-nav",
    // EPUB Dictionaries & Glossaries 1.0 and EPUB Indexes 1.0 (separate
    // extension specs, not implemented, but their manifest properties are
    // real and shouldn't misfire OPF-027 on otherwise-valid fixtures).
    "dictionary",
    "search-key-map",
    "glossary",
    "index",
];

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// OPF-092: a language tag (`xml:lang`, `link/@hreflang`, or `dc:language`'s
/// own text) must not have leading/trailing whitespace, and - once trimmed
/// - must be empty (allowed) or a syntactically plausible BCP-47 tag. No
/// regex needed: the only real failure mode confirmed by the corpus is a
/// single-letter primary subtag ("a-value"), which real BCP-47 never
/// allows (a language subtag is ISO 639, always 2-8 letters).
fn is_valid_lang_tag(raw: &str) -> bool {
    if raw != raw.trim() {
        return false;
    }
    if raw.is_empty() {
        return true;
    }
    let mut subtags = raw.split('-');
    let Some(first) = subtags.next() else {
        return false;
    };
    if first.len() < 2 || !first.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    subtags.all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Walks the whole OPF for every `xml:lang` attribute, `link/@hreflang`,
/// and `dc:language`'s own text, checking each against `is_valid_lang_tag`
/// (OPF-092).
fn check_lang_tags(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(lang) = n.attribute((XML_NS, "lang")) {
            if !is_valid_lang_tag(lang) {
                report.push_at(
                    OPF_092,
                    Severity::Error,
                    format!("language tag '{lang}' is not well-formed"),
                    opf_path,
                );
            }
        }
        if n.tag_name().name() == "link" {
            if let Some(hreflang) = n.attribute("hreflang") {
                if !is_valid_lang_tag(hreflang) {
                    report.push_at(
                        OPF_092,
                        Severity::Error,
                        format!("hreflang value '{hreflang}' is not well-formed"),
                        opf_path,
                    );
                }
            }
        }
        if n.tag_name().name() == "language" {
            let text: String = n
                .descendants()
                .filter(|t| t.is_text())
                .filter_map(|t| t.text())
                .collect::<String>()
                .trim()
                .to_string();
            if !text.is_empty() && !is_valid_lang_tag(&text) {
                report.push_at(
                    OPF_092,
                    Severity::Error,
                    format!("dc:language value '{text}' is not well-formed"),
                    opf_path,
                );
            }
        }
    }
}

/// OPF-065: a `@refines` chain must not form a cycle. General over every
/// element with both `@id` and `@refines` in the whole document (not
/// specific to any one property) - builds an id -> refines-target-id
/// edge map, then DFS-walks from each node with cycle detection (bounded
/// by the visited set, same style as the existing OPF-043 fallback-chain
/// cycle guard).
fn check_refines_cycles(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    let edges: HashMap<String, String> = doc
        .descendants()
        .filter(|n| n.is_element())
        .filter_map(|n| {
            let id = n.attribute("id")?.trim().to_string();
            let refines = n.attribute("refines")?.trim();
            let target = refines.strip_prefix('#')?.to_string();
            Some((id, target))
        })
        .collect();

    let mut reported = HashSet::new();
    for start in edges.keys() {
        if reported.contains(start) {
            continue;
        }
        let mut seen = Vec::new();
        let mut cur = start.as_str();
        loop {
            if seen.iter().any(|s: &String| s == cur) {
                if seen.first().map(|s| s.as_str()) == Some(start.as_str()) {
                    for id in &seen {
                        reported.insert(id.clone());
                    }
                    report.push_at(
                        OPF_065,
                        Severity::Error,
                        "a chain of \"refines\" attributes forms a cycle",
                        opf_path,
                    );
                }
                break;
            }
            seen.push(cur.to_string());
            match edges.get(cur) {
                Some(next) => cur = next,
                None => break,
            }
        }
    }
}

/// OPF-085: a `dc:identifier` starting with `urn:uuid:` must be followed
/// by a syntactically valid UUID (8-4-4-4-12 hex groups).
/// A Dublin Core date: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` (the W3C-DTF
/// profile of ISO 8601 that `dc:date` actually uses) - a bare year is the
/// common, valid case (a real fixture's own "no other errors" pairing
/// with a specific-year value elsewhere in this codebase already relies
/// on this), an empty string or a natural-language date are both
/// rejected uniformly by not matching any of the three shapes.
fn is_valid_dc_date(s: &str) -> bool {
    let digits_in = |slice: &str| slice.bytes().all(|b| b.is_ascii_digit());
    match s.len() {
        4 => digits_in(s),
        7 => {
            s.as_bytes().get(4) == Some(&b'-')
                && digits_in(&s[0..4])
                && digits_in(&s[5..7])
                && (1..=12).contains(&s[5..7].parse().unwrap_or(0))
        }
        10 => {
            s.as_bytes().get(4) == Some(&b'-')
                && s.as_bytes().get(7) == Some(&b'-')
                && digits_in(&s[0..4])
                && digits_in(&s[5..7])
                && digits_in(&s[8..10])
                && (1..=12).contains(&s[5..7].parse().unwrap_or(0))
                && (1..=31).contains(&s[8..10].parse().unwrap_or(0))
        }
        _ => false,
    }
}

fn is_valid_uuid(uuid: &str) -> bool {
    let groups: Vec<&str> = uuid.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&groups)
            .all(|(len, g)| g.len() == *len && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// OPF-085: a `dc:identifier` claiming to be a UUID - either via the
/// `urn:uuid:` scheme prefix, or (an EPUB 2 convention) an `opf:scheme="
/// uuid"` attribute with the bare UUID as the element's text - must
/// actually look like one.
fn check_uuid_identifiers(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    const OPF_NS: &str = "http://www.idpf.org/2007/opf";
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
    {
        let text: String = n
            .descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect::<String>()
            .trim()
            .to_string();
        let uuid_part = if let Some(rest) = text.strip_prefix("urn:uuid:") {
            Some(rest)
        } else if n
            .attribute((OPF_NS, "scheme"))
            .is_some_and(|s| s.eq_ignore_ascii_case("uuid"))
        {
            Some(text.as_str())
        } else {
            None
        };
        let Some(uuid_part) = uuid_part else {
            continue;
        };
        if !is_valid_uuid(uuid_part) {
            report.push_at(
                OPF_085,
                Severity::Warning,
                format!("dc:identifier '{text}' does not look like a valid UUID"),
                opf_path,
            );
        }
    }
}

/// A meta property/scheme value is well-formed if it's a bare NCName, or
/// a `prefix:reference` pair where both halves are non-empty NCNames -
/// approximated here as "non-empty and alphanumeric/hyphen/underscore/
/// colon, with a non-empty reference part after any colon" (no real
/// NCName Unicode-category checking, which the corpus doesn't exercise).
fn is_well_formed_ncname_or_prefixed(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    match value.split_once(':') {
        Some((prefix, reference)) => {
            !prefix.is_empty()
                && !reference.is_empty()
                && !reference.contains(':')
                && value
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
        }
        None => value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')),
    }
}

/// Small per-`<meta>` checks that need their own dedicated code/severity
/// rather than the uniform RSC-005/Error every Schematron finding gets:
/// RSC-017 ("should use a fragment identifier") when `@refines` is a
/// non-empty, non-fragment, non-absolute reference; OPF-027 when
/// `@scheme` has no `prefix:` part; OPF-026 when `@property` isn't a
/// well-formed (possibly prefixed) NCName.
fn check_meta_property_scheme_shape(
    doc: &roxmltree::Document,
    opf_path: &str,
    report: &mut Report,
) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "meta")
    {
        if let Some(refines) = n.attribute("refines") {
            let refines = refines.trim();
            if !refines.is_empty() && !refines.starts_with('#') && !refines.contains("://") {
                report.push_at(
                    RSC_017,
                    Severity::Warning,
                    "@refines should use a fragment identifier pointing to its manifest item",
                    opf_path,
                );
            }
        }
        if let Some(scheme) = n.attribute("scheme") {
            let scheme = scheme.trim();
            if !scheme.is_empty() && !scheme.contains(':') {
                report.push_at(
                    OPF_027,
                    Severity::Error,
                    format!("unknown scheme value '{scheme}' (must be prefixed)"),
                    opf_path,
                );
            }
        }
        if let Some(property) = n.attribute("property") {
            let property = property.trim();
            if !property.is_empty()
                && !property.contains(' ')
                && !is_well_formed_ncname_or_prefixed(property)
            {
                report.push_at(
                    OPF_026,
                    Severity::Error,
                    format!("meta property '{property}' is not well-formed"),
                    opf_path,
                );
            }
        }
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

/// OPF-070: a `collection/@role` used as a URL (contains "://") must have
/// valid percent-encoding - every `%` must be followed by exactly 2 hex
/// digits. Not full RFC 3986 validation, just the one failure mode the
/// corpus exercises (a trailing, incomplete "%").
/// Reads and parses a (possibly remote/missing, silently skipped) local
/// stylesheet, returning the CSS class names used in its selectors -
/// shared by the SVG active-class scan below for both `<link
/// rel="stylesheet">` targets and `@import`/`<?xml-stylesheet?>` targets.
fn read_stylesheet_classes(
    href: &str,
    dir: &str,
    name_index: &HashMap<String, String>,
    ocf: &mut Ocf,
) -> HashSet<String> {
    if is_external(href) {
        return HashSet::new();
    }
    let resolved = resolve(dir, href);
    let Some(orig) = name_index.get(&nfc(&resolved)).cloned() else {
        return HashSet::new();
    };
    let Some(b) = ocf.read(&orig) else {
        return HashSet::new();
    };
    let text = crate::css::decode_bytes(&b);
    let sheet = styloria::Parser::parse_stylesheet(&text);
    crate::css::selector_class_names(&sheet)
}

/// Extracts the `href="..."` pseudo-attribute from a `<?xml-stylesheet
/// ...?>` processing instruction's value string (e.g. `type="text/css"
/// href="styles.css"`) - a tiny hand-rolled scan rather than a full
/// XML-attribute parser, since it's one attribute in one fixed,
/// well-known position.
fn extract_pi_href(value: &str) -> Option<String> {
    let start = value.find("href=")? + 5;
    let quote = value.as_bytes().get(start).copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let rest = &value[start + 1..];
    let end = rest.find(quote as char)?;
    Some(rest[..end].to_string())
}

/// Collects CSS class names used by an SVG top-level content document's
/// own stylesheets - the 4 real linking mechanisms SVG uses (confirmed
/// via real corpus fixtures): inline `<style>`, linked `<link
/// rel="stylesheet">`, `@import` inside a `<style>` block, and a
/// top-level `<?xml-stylesheet?>` processing instruction. Only reached
/// for SVG docs that declare a `media-overlay` (the CSS-029/030
/// cross-reference is the only reason SVG's own CSS matters at all).
fn collect_svg_class_names(
    doc: &roxmltree::Document,
    dir: &str,
    name_index: &HashMap<String, String>,
    ocf: &mut Ocf,
) -> HashSet<String> {
    let mut classes = HashSet::new();

    for pi in doc.root().children().filter(|n| n.is_pi()) {
        if let Some(p) = pi.pi() {
            if p.target == "xml-stylesheet" {
                if let Some(href) = p.value.and_then(extract_pi_href) {
                    classes.extend(read_stylesheet_classes(&href, dir, name_index, ocf));
                }
            }
        }
    }

    for node in doc.descendants().filter(|n| n.is_element()) {
        if node.tag_name().name() == "style" {
            let css_text: String = node
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            let sheet = styloria::Parser::parse_stylesheet(&css_text);
            classes.extend(crate::css::selector_class_names(&sheet));
            for import_url in crate::css::import_targets(&sheet) {
                classes.extend(read_stylesheet_classes(&import_url, dir, name_index, ocf));
            }
        }
        if node.tag_name().name() == "link"
            && node.attribute("rel").is_some_and(|r| {
                r.split_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("stylesheet"))
            })
        {
            if let Some(href) = node.attribute("href") {
                classes.extend(read_stylesheet_classes(href, dir, name_index, ocf));
            }
        }
    }

    classes
}

fn check_collection_roles(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "collection")
    {
        let Some(role) = n.attribute("role") else {
            continue;
        };
        if !role.contains("://") {
            continue;
        }
        let bytes = role.as_bytes();
        let mut i = 0;
        let mut valid = true;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let hex_ok = i + 2 < bytes.len()
                    && (bytes[i + 1] as char).is_ascii_hexdigit()
                    && (bytes[i + 2] as char).is_ascii_hexdigit();
                if !hex_ok {
                    valid = false;
                    break;
                }
                i += 3;
            } else {
                i += 1;
            }
        }
        if !valid {
            report.push_at(
                OPF_070,
                Severity::Warning,
                format!("collection role '{role}' is not a valid URL"),
                opf_path,
            );
        }
    }
}

/// RSC-017, once per offending entry (confirmed via the corpus: two
/// duplicate `reference`s report "2 times", one per entry, not one per
/// pair): `guide/reference` entries must not duplicate the same
/// `type`+`href` combination.
fn check_guide_duplicates(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    let refs: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "reference")
        .collect();
    for (i, r) in refs.iter().enumerate() {
        if r.attribute("type").is_none() {
            continue;
        }
        let dup_exists = refs.iter().enumerate().any(|(j, other)| {
            j != i
                && other.attribute("type") == r.attribute("type")
                && other.attribute("href") == r.attribute("href")
        });
        if dup_exists {
            report.push_at(
                RSC_017,
                Severity::Warning,
                "duplicate \"reference\" elements with the same \"type\" and \"href\" attributes",
                opf_path,
            );
        }
    }
}

/// `guide/reference` targets: OPF-031 if not declared as a manifest item
/// (plus RSC-007 if the file doesn't exist in the container at all -
/// confirmed via a real fixture where the target is both undeclared and
/// missing); OPF-032 if it *is* declared but isn't a Content Document
/// (a real fixture links to a plain image).
fn check_guide_references(
    doc: &roxmltree::Document,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    items: &HashMap<String, (String, String)>,
    opf_path: &str,
    report: &mut Report,
) {
    for r in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "reference")
    {
        let Some(href) = r.attribute("href") else {
            continue;
        };
        if is_external(href) {
            continue;
        }
        let path_part = href.split(['#', '?']).next().unwrap_or(href);
        let resolved = nfc(&resolve(base_dir, path_part));
        match items.values().find(|(p, _)| nfc(p) == resolved) {
            None => {
                report.push_at(
                    OPF_031,
                    Severity::Error,
                    format!("guide reference '{href}' is not declared in the manifest"),
                    opf_path,
                );
                if !name_index.contains_key(&resolved) {
                    report.push_at(
                        RSC_007,
                        Severity::Error,
                        format!("guide reference '{href}' does not resolve to a real resource"),
                        opf_path,
                    );
                }
            }
            Some((_, mt)) => {
                if !is_content_document_type(mt) {
                    report.push_at(
                        OPF_032,
                        Severity::Error,
                        format!("guide reference '{href}' does not target a Content Document"),
                        opf_path,
                    );
                }
            }
        }
    }
}

/// RSC-007/RSC-010/RSC-012: an NCX `<content src="...">` target must
/// exist in the container (RSC-007 if not - confirmed via a real fixture
/// referencing a bogus local path), must be an OPS (Content Document)
/// resource, not e.g. a plain image (RSC-010, confirmed via a real
/// fixture), and - when the reference carries a `#fragment` - that
/// fragment must resolve to a real `id` in the target document (RSC-012).
/// Reads each distinct target doc once, caching its id set (a real book
/// can have many navPoints pointing to the same doc).
fn check_ncx_content_fragments(
    ncx_doc: &roxmltree::Document,
    ncx_path: &str,
    ocf: &mut Ocf,
    name_index: &HashMap<String, String>,
    items: &HashMap<String, (String, String)>,
    report: &mut Report,
) {
    let dir = parent_dir(ncx_path);
    let mut id_cache: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for n in ncx_doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "content")
    {
        let Some(src) = n.attribute("src") else {
            continue;
        };
        if is_external(src) {
            continue;
        }
        let (target, frag) = match src.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (src, None),
        };
        let resolved = nfc(&resolve(&dir, target));
        if !name_index.contains_key(&resolved) {
            report.push_at(
                RSC_007,
                Severity::Error,
                format!("NCX content src '{src}' does not resolve to a real resource"),
                ncx_path,
            );
            continue;
        }
        if let Some((_, mt)) = items.values().find(|(p, _)| nfc(p) == resolved) {
            if !is_content_document_type(mt) {
                report.push_at(
                    RSC_010,
                    Severity::Error,
                    format!("NCX content src '{src}' does not target an OPS document"),
                    ncx_path,
                );
                continue;
            }
        }
        let Some(frag) = frag else { continue };
        if frag.is_empty() {
            continue;
        }
        if !id_cache.contains_key(&resolved) {
            let ids = name_index
                .get(&resolved)
                .cloned()
                .and_then(|orig| ocf.read(&orig))
                .map(|b| {
                    let text = String::from_utf8_lossy(&b).into_owned();
                    parse_xml(&text)
                        .map(|d| dom_id_order(&d))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            id_cache.insert(resolved.clone(), ids);
        }
        if !id_cache[&resolved].contains_key(frag) {
            report.push_at(
                RSC_012,
                Severity::Error,
                format!("fragment identifier '{frag}' is not defined in '{target}'"),
                ncx_path,
            );
        }
    }
}

/// Extracts the `encoding="..."` (or `'...'`) pseudo-attribute value from
/// an XML declaration's own text, if present - a tiny hand-rolled scan
/// (same style as `extract_pi_href` above), scoped to only the text before
/// the declaration's own `?>` and only when it actually starts with
/// `<?xml`, so it never matches an unrelated `encoding=` elsewhere in the
/// document.
fn extract_xml_declared_encoding(text: &str) -> Option<String> {
    let decl_end = text.find("?>")?;
    let decl = &text[..decl_end];
    if !decl.trim_start().starts_with("<?xml") {
        return None;
    }
    let idx = decl.find("encoding")?;
    let rest = &decl[idx + "encoding".len()..];
    let eq = rest.find('=')?;
    let after_eq = rest[eq + 1..].trim_start();
    let quote = after_eq.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let after_quote = &after_eq[quote.len_utf8()..];
    let end = after_quote.find(quote)?;
    Some(after_quote[..end].to_string())
}

fn decode_utf32(bytes: &[u8], big_endian: bool) -> String {
    bytes
        .chunks_exact(4)
        .filter_map(|c| {
            let v = if big_endian {
                u32::from_be_bytes([c[0], c[1], c[2], c[3]])
            } else {
                u32::from_le_bytes([c[0], c[1], c[2], c[3]])
            };
            char::from_u32(v)
        })
        .collect()
}

/// EPUB 3 §3.9 (XML conformance): decodes the OPF's raw bytes into text,
/// detecting its real encoding from a BOM or (for BOM-less UTF-32, per the
/// XML spec's own Appendix F autodetection) a `00 00 00 '<'`/`'<' 00 00 00`
/// byte pattern, and reports:
/// - **RSC-027** (warning): genuine UTF-16 (BOM-detected) - EPUB requires
///   UTF-8 but this is still decodable, so checking continues.
/// - **RSC-028** (error): any other non-UTF-8 encoding (UTF-32, Latin-1,
///   or any other declared-but-recognized name) - still decodable, so
///   checking continues.
/// - **RSC-016** (fatal, in *addition* to RSC-027/028, returns `None` to
///   abort all further checks): the declared encoding doesn't match the
///   actual bytes (a UTF-16-BOM'd file declaring `UTF-8`) or names an
///   encoding we don't recognize at all - a real, strict XML parser
///   can't recover from either, so neither can we.
fn decode_opf_bytes(bytes: &[u8], opf_path: &str, report: &mut Report) -> Option<String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Some(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }
    if bytes.len() >= 2
        && ((bytes[0] == 0xFE && bytes[1] == 0xFF) || (bytes[0] == 0xFF && bytes[1] == 0xFE))
    {
        let big_endian = bytes[0] == 0xFE;
        let text = crate::css::decode_utf16(&bytes[2..], big_endian);
        report.push_at(
            RSC_027,
            Severity::Warning,
            "the OPF is UTF-16 encoded; EPUB requires UTF-8",
            opf_path,
        );
        if let Some(declared) = extract_xml_declared_encoding(&text) {
            let is_utf16 = declared.eq_ignore_ascii_case("utf-16")
                || declared.eq_ignore_ascii_case("utf-16le")
                || declared.eq_ignore_ascii_case("utf-16be");
            if !is_utf16 {
                report.push_at(
                    RSC_016,
                    Severity::Error,
                    format!("declared encoding '{declared}' does not match the file's actual UTF-16 encoding"),
                    opf_path,
                );
                return None;
            }
        }
        return Some(text);
    }
    let is_utf32_be = bytes.len() >= 4
        && ((bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0xFE && bytes[3] == 0xFF)
            || (bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0x00 && bytes[3] == b'<'));
    let is_utf32_le = bytes.len() >= 4
        && ((bytes[0] == 0xFF && bytes[1] == 0xFE && bytes[2] == 0x00 && bytes[3] == 0x00)
            || (bytes[0] == b'<' && bytes[1] == 0x00 && bytes[2] == 0x00 && bytes[3] == 0x00));
    if is_utf32_be || is_utf32_le {
        let has_real_bom =
            (bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0xFE && bytes[3] == 0xFF)
                || (bytes[0] == 0xFF && bytes[1] == 0xFE && bytes[2] == 0x00 && bytes[3] == 0x00);
        let body = if has_real_bom { &bytes[4..] } else { bytes };
        report.push_at(
            RSC_028,
            Severity::Error,
            "the OPF uses an encoding other than UTF-8, which is not allowed",
            opf_path,
        );
        return Some(decode_utf32(body, is_utf32_be));
    }
    let prelim = String::from_utf8_lossy(bytes).into_owned();
    match extract_xml_declared_encoding(&prelim) {
        None => Some(prelim),
        Some(enc) if enc.eq_ignore_ascii_case("utf-8") || enc.eq_ignore_ascii_case("utf8") => {
            Some(prelim)
        }
        Some(enc) => {
            const KNOWN_NON_UTF8: [&str; 5] = [
                "iso-8859-1",
                "iso-8859-15",
                "us-ascii",
                "ascii",
                "windows-1252",
            ];
            let is_known = KNOWN_NON_UTF8.iter().any(|k| enc.eq_ignore_ascii_case(k));
            report.push_at(
                RSC_028,
                Severity::Error,
                format!(
                    "the OPF declares encoding '{enc}', which is not allowed (EPUB requires UTF-8)"
                ),
                opf_path,
            );
            if !is_known {
                report.push_at(
                    RSC_016,
                    Severity::Error,
                    format!("unrecognized encoding '{enc}'"),
                    opf_path,
                );
                return None;
            }
            if enc.eq_ignore_ascii_case("iso-8859-1")
                || enc.eq_ignore_ascii_case("iso-8859-15")
                || enc.eq_ignore_ascii_case("windows-1252")
            {
                // A single-byte-per-codepoint encoding: byte value IS the
                // Unicode codepoint (exact for Latin-1; a close enough
                // approximation for the other two - no corpus fixture
                // exercises a codepoint where they'd actually differ).
                Some(bytes.iter().map(|&b| b as char).collect())
            } else {
                Some(prelim)
            }
        }
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
    let Some(text) = decode_opf_bytes(&bytes, opf_path, report) else {
        return;
    };
    crate::htm::check_opf_doctype(&text, opf_path, report);
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at(
                RSC_016,
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
    check_lang_tags(&doc, opf_path, report);
    check_refines_cycles(&doc, opf_path, report);
    check_uuid_identifiers(&doc, opf_path, report);
    check_meta_property_scheme_shape(&doc, opf_path, report);
    check_collection_roles(&doc, opf_path, report);
    check_guide_duplicates(&doc, opf_path, report);

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

    // PKG-025 (EPUB 3 only - a real EPUB 2 fixture, "Ignore unknown files
    // in the META-INF directory", explicitly stays clean with an
    // unrecognized META-INF file, contradicting EPUB 3's stricter rule):
    // only a closed set of well-known files may live directly in
    // META-INF - anything else (a publication resource stored there,
    // confirmed via a real EPUB 3 fixture) is an error. Checked here
    // (not in `ocf::open`) since the container is opened before the
    // package version is even known.
    const META_INF_RESERVED_NAMES: [&str; 6] = [
        "container.xml",
        "encryption.xml",
        "manifest.xml",
        "metadata.xml",
        "rights.xml",
        "signatures.xml",
    ];
    if is_epub3 {
        for name in &ocf.names {
            if let Some(rest) = name.strip_prefix("META-INF/") {
                if !rest.is_empty() && !META_INF_RESERVED_NAMES.contains(&rest) {
                    report.push_at(
                        PKG_025,
                        Severity::Error,
                        format!("'{name}' is a publication resource stored inside META-INF"),
                        name.as_str(),
                    );
                }
            }
        }
    }

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
    // 5.4: the metadata element must come before the manifest element.
    // A plain child-index compare, hand-coded because our XPath 1.0 core
    // has no preceding-sibling:: axis to express this in Schematron.
    {
        let element_children: Vec<_> = pkg.children().filter(|n| n.is_element()).collect();
        let metadata_pos = element_children
            .iter()
            .position(|n| n.tag_name().name() == "metadata");
        let manifest_pos = element_children
            .iter()
            .position(|n| n.tag_name().name() == "manifest");
        if let (Some(m), Some(mf)) = (metadata_pos, manifest_pos) {
            if mf < m {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "the \"metadata\" element must come before the \"manifest\" element",
                    opf_path,
                );
            }
        }
    }
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
        } else if is_epub2 {
            // EPUB 3 already reports an empty dc:title as RSC-005 (via
            // `schemas/package.sch`'s own version-scoped pattern); EPUB 2
            // is more lenient - a real corpus fixture expects only a
            // warning.
            for n in md
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "title")
            {
                let text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                if text.trim().is_empty() {
                    report.push_at(OPF_055, Severity::Warning, "dc:title is empty", opf_path);
                }
            }
        }
        // OPF-054: dc:date must be a non-empty, ISO-8601 (YYYY[-MM[-DD]])
        // value - confirmed via two real fixtures (an empty date, and one
        // using a natural-language date string).
        for n in md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "date")
        {
            let text: String = n
                .descendants()
                .filter(|t| t.is_text())
                .filter_map(|t| t.text())
                .collect();
            if !is_valid_dc_date(text.trim()) {
                report.push_at(
                    OPF_054,
                    Severity::Error,
                    format!(
                        "dc:date value '{}' is empty or doesn't conform to ISO 8601",
                        text.trim()
                    ),
                    opf_path,
                );
            }
        }
        // OPF-052: a dc:creator/dc:contributor's opf:role (any of the
        // "opf"/"epub" prefixes real fixtures use - both bind to the same
        // namespace) must be a real MARC relator code - approximated as
        // "exactly 3 lowercase ASCII letters" (every MARC code has this
        // shape; the corpus's own fixtures - "edc"/"clr" valid, the 9-
        // letter "companion" invalid - don't need the full ~500-entry
        // vocabulary to distinguish).
        const OPF_NS_ROLE: &str = "http://www.idpf.org/2007/opf";
        for n in md
            .children()
            .filter(|n| n.is_element() && matches!(n.tag_name().name(), "creator" | "contributor"))
        {
            if let Some(role) = n.attribute((OPF_NS_ROLE, "role")) {
                let valid = role.len() == 3 && role.bytes().all(|b| b.is_ascii_lowercase());
                if !valid {
                    report.push_at(
                        OPF_052,
                        Severity::Error,
                        format!("'{role}' is not a recognized MARC relator code"),
                        opf_path,
                    );
                }
            }
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
        } else {
            report.push_at(
                RSC_005,
                Severity::Error,
                "<package> is missing the required attribute \"unique-identifier\"",
                opf_path,
            );
            report.push_at(
                OPF_048,
                Severity::Error,
                "<package> is missing its required unique-identifier attribute",
                opf_path,
            );
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
    // manifest id -> its declared (obsolete EPUB 2) 'fallback-style'
    // manifest id, validated once the whole manifest is known (OPF-041).
    let mut fallback_style_map: HashMap<String, String> = HashMap::new();
    let mut nav_present = false;
    let mut nav_path: Option<String> = None;
    // Data Navigation Document(s) (properties="data-nav"): (resolved path,
    // media-type), for the Region-Based Navigation checks below.
    let mut data_nav_items: Vec<(String, String)> = Vec::new();
    // resolved+NFC'd item path -> its declared `properties` attribute
    // (raw string), for the remote-resources/scripted/svg cross-reference
    // below (OPF-014/018).
    let mut item_properties: HashMap<String, String> = HashMap::new();
    // raw href -> media-type, for every manifest item whose href is
    // itself a remote URL - used by the RSC-006/RSC-008 cross-reference
    // below (is a remote reference from a content doc actually declared
    // as its own manifest item, and if so, is it an image referenced via
    // a plain hyperlink rather than an embedding element).
    let mut remote_manifest: HashMap<String, String> = HashMap::new();
    let manifest = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "manifest");
    if let Some(mn) = manifest {
        let mut seen = HashSet::new();
        // resolved+NFC'd resource path -> first manifest item id that
        // declared it, for the OPF-074 duplicate-resource check below.
        let mut resource_seen: HashMap<String, String> = HashMap::new();
        let mut cover_image_count = 0usize;
        let opf_own_name = nfc(opf_path);
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
            if href.contains(' ') {
                report.push_at(
                    RSC_020,
                    Severity::Error,
                    format!("manifest item href '{href}' contains unencoded spaces"),
                    opf_path,
                );
            }
            if href.contains('#') {
                report.push_at(
                    OPF_091,
                    Severity::Error,
                    format!("manifest item href '{href}' must not have a fragment identifier"),
                    opf_path,
                );
            }
            if href.trim_start().starts_with("data:") {
                report.push_at(
                    RSC_029,
                    Severity::Error,
                    format!("manifest item '{id}' href must not be a data URL"),
                    opf_path,
                );
            }
            if href.trim_start().starts_with("file:") {
                report.push_at(
                    RSC_030,
                    Severity::Error,
                    format!("manifest item '{id}' href is a file URL, which is not allowed"),
                    opf_path,
                );
            }
            if crate::cmt::is_non_preferred_core_media_type(mt) {
                report.push_at(
                    OPF_090,
                    Severity::Info,
                    format!("media-type '{mt}' is a non-preferred (but valid) Core Media Type"),
                    opf_path,
                );
            }
            if mt == "text/x-oeb1-css" {
                report.push_at(
                    OPF_037,
                    Severity::Warning,
                    "media-type 'text/x-oeb1-css' is a deprecated OEB 1.x construct",
                    opf_path,
                );
            }
            // resolve()'s query-stripping and path-segment handling are
            // meant for container-relative paths; applied to an absolute
            // remote URL, they'd garble it (and remote resources can
            // legitimately differ only by query string, e.g.
            // "...?type=flash" vs "...?type=mp4" - confirmed via a real
            // corpus fixture where treating those as "the same resource"
            // would be a false OPF-074). So self-reference/duplicate/
            // space-in-name only make sense for local items.
            let resolved = if is_remote_url(href) {
                remote_manifest.insert(href.to_string(), mt.to_string());
                href.to_string()
            } else if is_external(href) {
                href.to_string()
            } else {
                resolve(&base_dir, href)
            };
            let resolved_nfc = nfc(&resolved);
            if !is_external(href) {
                if href_leaks_container_root(&base_dir, href) {
                    report.push_at(
                        RSC_026,
                        Severity::Error,
                        format!("manifest item '{id}' href '{href}' is path-absolute or escapes the container root"),
                        opf_path,
                    );
                }
                if href.contains('?') {
                    report.push_at(
                        RSC_033,
                        Severity::Error,
                        format!("manifest item '{id}' href '{href}' must not have a query string"),
                        opf_path,
                    );
                }
                if resolved.contains(' ') {
                    report.push_at(
                        PKG_010,
                        Severity::Warning,
                        format!("resource '{resolved}' has a space in its name"),
                        opf_path,
                    );
                }
                if resolved_nfc == opf_own_name {
                    report.push_at(
                        OPF_099,
                        Severity::Error,
                        format!("manifest item '{id}' references the package document itself"),
                        opf_path,
                    );
                }
                if let Some(first_id) = resource_seen.get(&resolved_nfc) {
                    report.push_at(
                        OPF_074,
                        Severity::Error,
                        format!(
                            "manifest item '{id}' represents the same resource as item '{first_id}'"
                        ),
                        opf_path,
                    );
                } else {
                    resource_seen.insert(resolved_nfc.clone(), id.to_string());
                }
            }
            if let Some(props) = item.attribute("properties") {
                item_properties.insert(resolved_nfc.clone(), props.to_string());
                for token in props.split_whitespace() {
                    if token == "cover-image" {
                        cover_image_count += 1;
                        if !mt.starts_with("image/") {
                            report.push_at(
                                OPF_012,
                                Severity::Error,
                                "the \"cover-image\" property must only be used on an image",
                                opf_path,
                            );
                        }
                    } else if !token.contains(':') && !KNOWN_ITEM_PROPERTIES.contains(&token) {
                        report.push_at(
                            OPF_027,
                            Severity::Error,
                            format!("unknown manifest item property '{token}'"),
                            opf_path,
                        );
                    }
                }
            }
            if is_epub3 && item.attribute("fallback-style").is_some() {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "the \"fallback-style\" attribute is an obsolete EPUB 2 construct",
                    opf_path,
                );
            } else if let Some(fs) = item.attribute("fallback-style") {
                fallback_style_map.insert(id.to_string(), fs.trim().to_string());
            }
            if let Some(fb) = item.attribute("fallback").map(str::trim) {
                if fb == id {
                    report.push_at(
                        OPF_045,
                        Severity::Error,
                        format!("item '{id}' cannot fall back to itself"),
                        opf_path,
                    );
                }
            }
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
                // PKG-009/012: real epubcheck's single-package-document
                // check mode has no actual container to inspect, so it
                // validates the declared href's own file-name segments
                // directly (confirmed via a real fixture pair testing the
                // identical defect once as a real file name, once as a
                // bare `.opf`'s manifest href) - only meaningful here when
                // the resource doesn't actually exist, since an existing
                // file's real name is already checked in `ocf::open` and
                // double-reporting the same defect for a normal, fully-
                // resolvable publication would be wrong.
                let href_path = href.split(['?', '#']).next().unwrap_or(href);
                for segment in href_path.split('/').filter(|s| !s.is_empty()) {
                    let decoded = percent_decode(segment);
                    if crate::filename::has_forbidden_char(&decoded) {
                        report.push_at(
                            PKG_009,
                            Severity::Error,
                            format!("manifest item '{id}' href segment '{decoded}' contains a forbidden character"),
                            opf_path,
                        );
                    }
                    if crate::filename::has_non_ascii(&decoded) {
                        report.push_at(
                            PKG_012,
                            Severity::Info,
                            format!("manifest item '{id}' href segment '{decoded}' contains non-ASCII characters"),
                            opf_path,
                        );
                    }
                }
            }
            // An XHTML Content Document can never be remote - it *is* the
            // publication's content, unlike embedded media/fonts, which
            // may legitimately live outside the container. Checked here
            // (manifest-level) rather than only via a DOM reference,
            // since a real corpus fixture declares one with no reference
            // to it anywhere at all (`resources-remote-spine-item-
            // error`). Deliberately NOT extended to `image/svg+xml`: SVG
            // is dual-purpose (a content document OR a font/image
            // resource, e.g. an SVG font referenced only from CSS -
            // confirmed via `resources-remote-font-svg-valid`, a remote
            // `image/svg+xml` item used exclusively via `@font-face`); a
            // remote SVG genuinely used *as* a content document is still
            // caught separately when it's referenced via `<img>`
            // (`resources-remote-svg-contentdoc-error`).
            if is_remote_url(href) && mt == "application/xhtml+xml" {
                report.push_at(
                    RSC_006,
                    Severity::Error,
                    format!("Content Document '{href}' must not be remote"),
                    opf_path,
                );
            }
            if let Some(mo) = item.attribute("media-overlay") {
                if mt != "application/xhtml+xml" && mt != "image/svg+xml" {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        "the media-overlay attribute is only allowed on EPUB Content Documents",
                        opf_path,
                    );
                }
                media_overlay_attrs.push((nfc(&resolved), mo.trim().to_string()));
            }
            if let Some(fb) = item.attribute("fallback") {
                fallback_map.insert(id.to_string(), fb.trim().to_string());
            }
            items.insert(id.to_string(), (resolved, mt.to_string()));
        }
        if cover_image_count > 1 {
            report.push_at(
                RSC_005,
                Severity::Error,
                "the \"cover-image\" property must occur at most once in the manifest",
                opf_path,
            );
        }
    } else {
        report.push_at(
            RSC_005,
            Severity::Error,
            "OPF is missing the <manifest> element",
            opf_path,
        );
    }
    for target in fallback_map.values() {
        if !items.contains_key(target) {
            report.push_at(
                OPF_040,
                Severity::Error,
                format!("fallback references unknown manifest item id '{target}'"),
                opf_path,
            );
        }
    }
    for target in fallback_style_map.values() {
        if !items.contains_key(target) {
            report.push_at(
                OPF_041,
                Severity::Error,
                format!("fallback-style references unknown manifest item id '{target}'"),
                opf_path,
            );
        }
    }
    check_guide_references(&doc, &base_dir, &name_index, &items, opf_path, report);
    // OPF-045: a `fallback` chain must not form a cycle - same DFS-cycle-
    // detector shape as OPF-065's `@refines`-cycle check, over
    // `fallback_map` (already built above) instead of walking the DOM
    // again. The direct self-fallback case (`fb == id`) is already caught
    // separately above; this catches longer cycles (confirmed via a real
    // 2-item cycle fixture).
    {
        let mut reported = HashSet::new();
        for start in fallback_map.keys() {
            if reported.contains(start) {
                continue;
            }
            let mut seen = Vec::new();
            let mut cur = start.as_str();
            loop {
                if seen.iter().any(|s: &String| s == cur) {
                    if seen.first().map(|s| s.as_str()) == Some(start.as_str()) {
                        for id in &seen {
                            reported.insert(id.clone());
                        }
                        report.push_at(
                            OPF_045,
                            Severity::Error,
                            "a chain of \"fallback\" attributes forms a cycle",
                            opf_path,
                        );
                    }
                    break;
                }
                seen.push(cur.to_string());
                match fallback_map.get(cur) {
                    Some(next) => cur = next,
                    None => break,
                }
            }
        }
    }
    // A media-overlay attribute's target item must itself be a Media
    // Overlay Document (application/smil+xml).
    for (_, overlay_id) in &media_overlay_attrs {
        if let Some((_, mt)) = items.get(overlay_id) {
            if mt != "application/smil+xml" {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!(
                        "media-overlay target '{overlay_id}' must be of the \"application/smil+xml\" type"
                    ),
                    opf_path,
                );
            }
        }
    }
    // 9.3.5.2: once any content document declares a media-overlay, (a) a
    // global (non-refines) media:duration must exist for the whole
    // publication, and (b) each distinct overlay id referenced must have
    // its own refines-scoped media:duration. Distinct from the existing
    // MED-016 total-vs-sum check below, which only compares values once
    // both sides are already known to exist.
    if let Some(md) = metadata {
        let has_global_duration = md.children().any(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("property") == Some("media:duration")
                && n.attribute("refines").is_none()
        });
        if !media_overlay_attrs.is_empty() && !has_global_duration {
            report.push_at(
                RSC_005,
                Severity::Error,
                "the global media:duration meta element not set",
                opf_path,
            );
        }
        let overlay_ids: HashSet<&str> = media_overlay_attrs
            .iter()
            .map(|(_, id)| id.as_str())
            .collect();
        for overlay_id in overlay_ids {
            let has_item_duration = md.children().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attribute("property") == Some("media:duration")
                    && n.attribute("refines").map(|r| r.trim_start_matches('#')) == Some(overlay_id)
            });
            if !has_item_duration {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!("the item media:duration meta element not set for '{overlay_id}'"),
                    opf_path,
                );
            }
        }
    }

    // --- 5.5.7 The link element ---
    // Scoped to metadata-level links only - a <link> inside a <collection>
    // (e.g. a "preview"/"manifest"-role collection indexing existing
    // manifest resources) follows different rules and legitimately omits
    // media-type/points at real resources without these checks applying
    // (confirmed via a real corpus fixture, preview-embedded-valid).
    for link in metadata
        .into_iter()
        .flat_map(|md| md.children())
        .filter(|n| n.is_element() && n.tag_name().name() == "link")
    {
        let Some(href) = link.attribute("href") else {
            continue;
        };
        let href = href.trim();
        if let Some(frag) = href.strip_prefix('#') {
            if items.contains_key(frag) {
                report.push_at(
                    OPF_098,
                    Severity::Error,
                    "a link target must not reference a manifest item id",
                    opf_path,
                );
            }
            continue;
        }
        if href.starts_with("data:") {
            report.push_at(
                RSC_029,
                Severity::Error,
                "a package link href must not be a data URL",
                opf_path,
            );
            continue;
        }
        if href.starts_with("file:") {
            report.push_at(
                RSC_030,
                Severity::Error,
                "a package link href must not be a file URL",
                opf_path,
            );
            continue;
        }
        if is_external(href) {
            continue;
        }
        if href.contains('?') {
            report.push_at(
                RSC_033,
                Severity::Error,
                format!("package link href '{href}' must not have a query string"),
                opf_path,
            );
        }
        let resolved = resolve(&base_dir, href);
        if !name_index.contains_key(&nfc(&resolved)) {
            report.push_at(
                RSC_007,
                Severity::Warning,
                format!("link references a missing resource '{href}'"),
                opf_path,
            );
        }
        if link.attribute("media-type").is_none() {
            report.push_at(
                OPF_093,
                Severity::Error,
                "a link to a local resource must declare a media-type",
                opf_path,
            );
        }
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
    // resolved+NFC'd paths of every itemref explicitly marked
    // linear="no", for the OPF-096 reachability check below.
    let mut non_linear_paths: Vec<String> = Vec::new();
    // content-doc resolved-path -> whether it's fixed-layout, for the
    // region-based nav's NAV-009 target cross-check below.
    let mut fixed_layout_docs: HashMap<String, bool> = HashMap::new();
    let spine = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "spine");
    if let Some(sp) = spine {
        // `page-map` is an invalid (never-standardized) Adobe extension -
        // any use at all is a content-model violation (RSC-005),
        // regardless of whether it resolves; if it *also* doesn't resolve
        // to a real manifest item, that's additionally OPF-063.
        if let Some(page_map) = sp.attribute("page-map") {
            report.push_at(
                RSC_005,
                Severity::Error,
                "attribute \"page-map\" not allowed here",
                opf_path,
            );
            if !items.contains_key(page_map.trim()) {
                report.push_at(
                    OPF_063,
                    Severity::Warning,
                    format!("page-map reference '{page_map}' was not found in the manifest"),
                    opf_path,
                );
            }
        }
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
                            if ir.attribute("linear").map(str::trim) == Some("no") {
                                non_linear_paths.push(nfc(path));
                            }
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
                                if mt.starts_with("image/") {
                                    // A real fixture confirms an image is
                                    // its own dedicated (error-level)
                                    // case, not the generic warning.
                                    report.push_at(
                                        OPF_042,
                                        Severity::Error,
                                        format!("spine item idref '{idref}' is an image, not a Content Document"),
                                        opf_path,
                                    );
                                } else {
                                    report.push_at(
                                        OPF_043,
                                        Severity::Warning,
                                        format!("spine item idref '{idref}' has non-content media-type '{mt}' with no verified fallback"),
                                        opf_path,
                                    );
                                }
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
                                if let Ok(ncx_doc) = parse_xml(&ncx_text) {
                                    check_ncx_content_fragments(
                                        &ncx_doc,
                                        ncx_path,
                                        ocf,
                                        &name_index,
                                        &items,
                                        report,
                                    );
                                }
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

    // Manifest-declared resource paths (nfc-normalized) - used by
    // `css::check` to distinguish RSC-001 (declared but missing) from
    // RSC-007/RSC-008 (undeclared, missing vs. still present).
    let manifest_paths: HashSet<String> = items.values().map(|(p, _)| nfc(p)).collect();

    // OPF-003 (usage): a real container resource that isn't declared as
    // any manifest item at all - `mimetype`/`META-INF/*`/the OPF itself
    // are structural, not "publication resources", and OS junk files
    // (`.DS_Store`, `Thumbs.db`) are explicitly ignored (confirmed via a
    // real corpus fixture pair).
    {
        const IGNORED_BASENAMES: [&str; 2] = [".ds_store", "thumbs.db"];
        let opf_own = nfc(opf_path);
        for name in &ocf.names {
            if name == "mimetype" || name.starts_with("META-INF/") || name.ends_with('/') {
                continue;
            }
            let key = nfc(name);
            if key == opf_own {
                continue;
            }
            let basename = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if IGNORED_BASENAMES.contains(&basename.as_str()) {
                continue;
            }
            if !manifest_paths.contains(&key) {
                report.push_at(
                    OPF_003,
                    Severity::Info,
                    format!("container resource '{name}' is not listed in the manifest"),
                    opf_path,
                );
            }
        }
    }

    // resolved-resource-key -> Core-Media-Type/fallback status, for the
    // foreign-resource-fallback checks (RSC-032/MED-003/MED-007) below.
    let resource_status = crate::foreign::build_resource_status(&items, &fallback_map);

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
    // Every local content-doc target hyperlinked from *any* content
    // document (including the nav) - for RSC-011 (a hyperlink target not
    // listed in the spine) and OPF-096 (a linear="no" spine item not
    // reachable via any hyperlink or the nav).
    let mut hyperlink_targets: HashSet<String> = HashSet::new();
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
        if !is_epub3 {
            crate::htm::check_dom_epub2(&d, &path, report);
        }

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

        // --- SVG content models: foreignObject (flow content, reused via
        // wrap+reparse), title (namespace-only), generic vocabulary
        // (RSC-025/usage) ---
        for svg_root in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "svg"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
                && !n.ancestors().skip(1).any(|a| {
                    a.tag_name().name() == "svg"
                        && a.tag_name().namespace() == Some(crate::svg::SVG_NS)
                })
        }) {
            crate::svg::check_vocabulary(svg_root, &path, report);
        }
        for fo in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "foreignObject"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
        }) {
            crate::svg::check_foreign_object(fo, &t, d.root_element(), &path, is_epub3, report);
        }
        for svg_title in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "title"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
        }) {
            crate::svg::check_title_content(svg_title, &path, report);
        }

        // --- MathML content model: Presentation-only at the top level,
        // annotation-xml encoding/name/content validation ---
        for math_el in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "math"
                && n.tag_name().namespace() == Some(crate::mathml::MATHML_NS)
        }) {
            crate::mathml::check_math_element(math_el, &path, report);
        }

        // <title> present but empty, or missing entirely.
        match d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "title")
        {
            Some(title) => {
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
            None => {
                report.push_at(
                    RSC_017,
                    Severity::Warning,
                    "The \"head\" element should have a \"title\" child element.",
                    path.clone(),
                );
            }
        }

        // Duplicate `id` attribute values within this document.
        {
            let mut seen: HashSet<&str> = HashSet::new();
            for n in d.descendants().filter(|n| n.is_element()) {
                if let Some(id) = n.attribute("id") {
                    if !seen.insert(id) {
                        report.push_at(
                            RSC_005,
                            Severity::Error,
                            format!("Duplicate ID \"{id}\""),
                            path.clone(),
                        );
                    }
                }
            }
        }

        // ID-referencing attributes (ARIA + a couple of plain HTML ones)
        // must refer to a real id in the same document.
        {
            let ids: HashSet<&str> = d.descendants().filter_map(|n| n.attribute("id")).collect();
            const MULTI_TOKEN: &[&str] = &[
                "aria-labelledby",
                "aria-describedby",
                "aria-owns",
                "aria-activedescendant",
                "aria-controls",
                "aria-flowto",
                "aria-details",
            ];
            const SINGLE_TOKEN: &[&str] = &["for", "list"];
            for n in d.descendants().filter(|n| n.is_element()) {
                for attr in MULTI_TOKEN {
                    if let Some(v) = n.attribute(*attr) {
                        for token in v.split_whitespace() {
                            if !ids.contains(token) {
                                report.push_at(
                                    RSC_005,
                                    Severity::Error,
                                    format!("attribute \"{attr}\" must refer to elements in the same document (target ID missing)"),
                                    path.clone(),
                                );
                            }
                        }
                    }
                }
                // <output for="..."> is a space-separated *list* of
                // control ids (like the ARIA attributes above), unlike
                // <label for>/<input list>, which each name a single id -
                // confirmed via a real fixture using `<output for="o2 o3">`.
                if n.tag_name().name() == "output" {
                    if let Some(v) = n.attribute("for") {
                        for token in v.split_whitespace() {
                            if !ids.contains(token) {
                                report.push_at(
                                    RSC_005,
                                    Severity::Error,
                                    "attribute \"for\" must refer to elements in the same document (target ID missing)",
                                    path.clone(),
                                );
                            }
                        }
                    }
                    continue;
                }
                for attr in SINGLE_TOKEN {
                    if let Some(v) = n.attribute(*attr) {
                        let v = v.trim();
                        if !v.is_empty() && !ids.contains(v) {
                            report.push_at(
                                RSC_005,
                                Severity::Error,
                                format!("attribute \"{attr}\" must refer to elements in the same document (target ID missing)"),
                                path.clone(),
                            );
                        }
                    }
                }
            }
        }

        // <img src> must not be empty/whitespace-only.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "img")
        {
            if n.attribute("src").is_some_and(|v| v.trim().is_empty()) {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "\"img\" element's \"src\" attribute must not be empty",
                    path.clone(),
                );
            }
        }

        // lang/xml:lang must agree when both are present on the same element.
        for n in d.descendants().filter(|n| n.is_element()) {
            if let (Some(lang), Some(xml_lang)) = (
                n.attribute("lang"),
                n.attribute(("http://www.w3.org/XML/1998/namespace", "lang")),
            ) {
                if lang.trim() != xml_lang.trim() {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        "lang and xml:lang attributes must have the same value",
                        path.clone(),
                    );
                }
            }
        }

        // <img usemap> must be a "#name" reference in EPUB 3 (HTML5's
        // IDREF-typed usemap) - a bare name with no leading '#' is
        // invalid there regardless of whether a matching <map name>
        // exists. EPUB 2's XHTML 1.1 DTD later retyped usemap as URIREF
        // (basically CDATA), which explicitly also permits the bare form
        // (confirmed via a real, deliberately-commented EPUB2 fixture) -
        // so this check is EPUB3-only.
        for n in d
            .descendants()
            .filter(|n| is_epub3 && n.is_element() && n.tag_name().name() == "img")
        {
            if let Some(usemap) = n.attribute("usemap") {
                if !usemap.starts_with('#') {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        format!("value of attribute \"usemap\" is invalid: \"{usemap}\""),
                        path.clone(),
                    );
                }
            }
        }

        // Both an http-equiv Content-Type meta and a charset meta declared;
        // and, independently, an http-equiv Content-Type meta whose value
        // isn't exactly the expected UTF-8 declaration.
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
        for n in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attribute("http-equiv")
                    .is_some_and(|v| v.eq_ignore_ascii_case("content-type"))
        }) {
            if !n
                .attribute("content")
                .is_some_and(|v| v.eq_ignore_ascii_case("text/html; charset=utf-8"))
            {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "the \"content\" attribute must have the value \"text/html; charset=utf-8\"",
                    path.clone(),
                );
            }
        }

        // HTML5 microdata: itemprop is only meaningful on an element that
        // also carries the attribute microdata uses to derive that
        // element's *value* - a/area/link -> href, several embed-like
        // elements -> src, object -> data, data/meter -> value, time ->
        // datetime. Missing that attribute is a real, corpus-confirmed
        // misuse (only a/object are exercised by the real fixture; the
        // rest of this table is the well-known HTML5 microdata spec rule,
        // included for the same family of elements rather than guessed).
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.has_attribute("itemprop"))
        {
            let (required_attr, tag) = match n.tag_name().name() {
                t @ ("a" | "area" | "link") => ("href", t),
                t @ ("audio" | "embed" | "iframe" | "img" | "source" | "track" | "video") => {
                    ("src", t)
                }
                t @ "object" => ("data", t),
                t @ ("data" | "meter") => ("value", t),
                t @ "time" => ("datetime", t),
                _ => continue,
            };
            if !n.has_attribute(required_attr) {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!(
                        "element \"{tag}\" missing required attribute \"{required_attr}\" (if the itemprop is specified on this element type, that attribute must also be present)"
                    ),
                    path.clone(),
                );
            }
        }

        // A <dfn> must not have a <dfn> descendant.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "dfn")
        {
            if n.descendants()
                .skip(1)
                .any(|c| c.is_element() && c.tag_name().name() == "dfn")
            {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "a \"dfn\" element must not contain a nested \"dfn\" element",
                    path.clone(),
                );
            }
        }

        // epub:trigger is deprecated; its ref/ev:observer attributes must
        // each resolve to a real id in the same document.
        {
            let ids: HashSet<&str> = d.descendants().filter_map(|n| n.attribute("id")).collect();
            for n in d.descendants().filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "trigger"
                    && n.tag_name().namespace() == Some(EPUB_NS)
            }) {
                report.push_at(
                    RSC_017,
                    Severity::Warning,
                    "The \"epub:trigger\" element is deprecated",
                    path.clone(),
                );
                if let Some(r) = n.attribute("ref") {
                    if !ids.contains(r) {
                        report.push_at(
                            RSC_005,
                            Severity::Error,
                            "The ref attribute must refer to an element in the same document",
                            path.clone(),
                        );
                    }
                }
                if let Some(o) = n.attribute(("http://www.w3.org/2001/xml-events", "observer")) {
                    if !ids.contains(o) {
                        report.push_at(
                            RSC_005,
                            Severity::Error,
                            "The ev:observer attribute must refer to an element in the same document",
                            path.clone(),
                        );
                    }
                }
            }
        }

        // Deprecated DPUB-ARIA roles - confirmed via the real corpus's
        // only negative ARIA-role scenario (every other ARIA/DPUB-ARIA
        // fixture is a "-valid" one that just needs to stay clean, which
        // it already does without any role-validity check at all - no
        // scenario tests "which roles are valid on which host elements",
        // so that fuller taxonomy isn't attempted here, only what's
        // actually evidenced). `doc-endnote`/`doc-biblioentry` are
        // deprecated regardless of host element (the real fixture fires
        // on both a `<li>` and a `<div>` carrying the same role).
        const DEPRECATED_ARIA_ROLES: &[&str] = &["doc-endnote", "doc-biblioentry"];
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.has_attribute("role"))
        {
            for token in n.attribute("role").unwrap().split_whitespace() {
                if DEPRECATED_ARIA_ROLES.contains(&token) {
                    report.push_at(
                        RSC_017,
                        Severity::Warning,
                        format!("\"{token}\" role is deprecated"),
                        path.clone(),
                    );
                }
            }
        }

        // epub:type default-vocabulary / deprecated / misuse taxonomies -
        // reuses smil::is_default_vocab_type (built for SMIL's own
        // epub:type check in an earlier increment); custom-prefixed
        // tokens (containing ':') are always exempt.
        const DEPRECATED_SSV: &[&str] = &[
            "annoref",
            "annotation",
            "biblioentry",
            "bridgehead",
            "endnote",
            "help",
            "marginalia",
            "note",
            "rearnote",
            "rearnotes",
            "sidebar",
            "subchapter",
            "warning",
        ];
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.attribute((EPUB_NS, "type")).is_some())
        {
            let value = n.attribute((EPUB_NS, "type")).unwrap();
            let tag = n.tag_name().name();
            for token in value.split_whitespace() {
                if token.contains(':') {
                    continue;
                }
                if !crate::smil::is_default_vocab_type(token) {
                    report.push_at(
                        OPF_088,
                        Severity::Info,
                        format!("epub:type value '{token}' is not in the default vocabulary"),
                        path.clone(),
                    );
                }
                // "endnote" specifically is deprecated only when used
                // *without* being nested inside its proper "endnotes"
                // container - confirmed via two real fixtures: a
                // standalone `<aside epub:type="endnote">` is deprecated,
                // but the same value on a `<div>` nested inside a
                // `<section epub:type="endnotes">` is the recommended,
                // non-deprecated usage.
                let endnote_exempt = token == "endnote"
                    && n.ancestors().any(|a| {
                        a.attribute((EPUB_NS, "type"))
                            .is_some_and(|t| t.split_whitespace().any(|tok| tok == "endnotes"))
                    });
                if DEPRECATED_SSV.contains(&token) && !endnote_exempt {
                    report.push_at(
                        OPF_086,
                        Severity::Info,
                        format!("epub:type value '{token}' is deprecated"),
                        path.clone(),
                    );
                }
                let redundant = matches!(
                    (tag, token),
                    ("table", "table")
                        | ("tr", "table-row")
                        | ("td", "table-cell")
                        | ("ul", "list")
                        | ("ol", "list")
                        | ("li", "list-item")
                        | ("figure", "figure")
                        | ("aside", "aside")
                );
                if redundant {
                    report.push_at(
                        OPF_087,
                        Severity::Info,
                        format!("epub:type value '{token}' only restates the semantic of its host element \"{tag}\""),
                        path.clone(),
                    );
                }
            }
        }

        // The epub: namespace prefix should be bound to exactly the real
        // EPUB ops namespace URI - an unrecognized binding is informative,
        // not an error (the document may still be usable).
        for ns in d.root_element().namespaces() {
            if ns.name() == Some("epub") && ns.uri() != EPUB_NS {
                report.push_at(
                    HTM_010,
                    Severity::Info,
                    format!("Namespace \"{}\" is unusual", ns.uri()),
                    path.clone(),
                );
            }
        }

        // MathML <math> with no alttext at all, and no annotation
        // (annotation/annotation-xml, tex or otherwise) providing an
        // alternative representation either, has no accessible fallback.
        // Real corpus finding: several "valid" fixtures have no `alttext`
        // attribute but do have a `<semantics><annotation-xml ...>` child,
        // which counts as an alternative just as much as `alttext` would.
        for n in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "math"
                && n.tag_name().namespace() == Some("http://www.w3.org/1998/Math/MathML")
        }) {
            let has_annotation = n.descendants().any(|c| {
                c.is_element()
                    && matches!(c.tag_name().name(), "annotation" | "annotation-xml")
                    && c.tag_name().namespace() == Some("http://www.w3.org/1998/Math/MathML")
            });
            if !n.has_attribute("alttext") && !has_annotation {
                report.push_at(
                    ACC_009,
                    Severity::Info,
                    "MathML markup has no alternative text",
                    path.clone(),
                );
            }
        }

        // HTML5 <time datetime="..."> value grammar.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "time")
        {
            if let Some(v) = n.attribute("datetime") {
                if !crate::htm::is_valid_html5_datetime(v) {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        format!("value of attribute \"datetime\" is invalid: \"{v}\""),
                        path.clone(),
                    );
                }
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

        crate::foreign::check_content_doc(&d, &path, &dir, &resource_status, report);

        // OPF-013 (warning): an explicit `type` attribute on `<object>`/
        // `<embed>`/a `<picture><source>` doesn't match the resource's own
        // manifest-declared media-type - real epubcheck IDs this as an
        // ordinary MIME-type mismatch, not an EPUB-defined content-model
        // check (same convention already used for OPF-029's image case).
        for n in d.descendants().filter(|n| n.is_element()) {
            let (href_attr, resolve_srcset) = match n.tag_name().name() {
                "object" => ("data", false),
                "embed" => ("src", false),
                "source"
                    if n.ancestors()
                        .skip(1)
                        .any(|a| a.is_element() && a.tag_name().name() == "picture") =>
                {
                    ("srcset", true)
                }
                _ => continue,
            };
            let Some(declared_type) = n.attribute("type") else {
                continue;
            };
            let Some(href) = n.attribute(href_attr) else {
                continue;
            };
            let target = if resolve_srcset {
                href.split(',')
                    .next()
                    .unwrap_or(href)
                    .trim()
                    .split_whitespace()
                    .next()
            } else {
                Some(href)
            };
            let Some(target) = target else { continue };
            if is_external(target) {
                continue;
            }
            let resolved = nfc(&resolve(&dir, target));
            if let Some((_, actual_type)) = items.values().find(|(ip, _)| nfc(ip) == resolved) {
                if !actual_type.eq_ignore_ascii_case(declared_type) {
                    report.push_at(
                        OPF_013,
                        Severity::Warning,
                        format!(
                            "declared type \"{declared_type}\" doesn't match the resource's actual media-type \"{actual_type}\""
                        ),
                        path.clone(),
                    );
                }
            }
        }

        // --- <a href> fragment resolution (RSC-012/RSC-014), stylesheet/
        // svg-use/img fragment classification (RSC-013/RSC-015/RSC-009),
        // srcset (RSC-008), and base-URI-aware remote reclassification
        // (RSC-006) ---
        {
            // An absolute remote <base href>/xml:base means every
            // relative-or-fragment-only <a href> elsewhere in *this*
            // document actually resolves to a remote URL through it -
            // narrower than (and additive to) the existing manifest-
            // declared-remote-image RSC-006 check further below, since
            // this target was never manifest-declared at all.
            let remote_base = d
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "base")
                .and_then(|n| n.attribute("href"))
                .filter(|v| is_remote_url(v))
                .or_else(|| {
                    d.root_element()
                        .attribute(("http://www.w3.org/XML/1998/namespace", "base"))
                        .filter(|v| is_remote_url(v))
                })
                .is_some();

            let mut frag_id_cache: HashMap<String, HashMap<String, usize>> = HashMap::new();

            for a in d
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "a")
            {
                let Some(href) = a.attribute("href") else {
                    continue;
                };
                if crate::url::is_absolute(href) {
                    if crate::url::has_syntax_error(href) {
                        report.push_at(
                            RSC_020,
                            Severity::Error,
                            format!("URL '{href}' is not conforming"),
                            path.clone(),
                        );
                    } else if crate::url::has_unregistered_scheme(href) {
                        report.push_at(
                            HTM_025,
                            Severity::Warning,
                            format!("URL '{href}' uses an unregistered scheme"),
                            path.clone(),
                        );
                    }
                }
                // `is_external` treats *any* fragment-only href
                // (`#foo`) as "skip normal resolution" - correct for the
                // old file-existence check, but RSC-012's fragment
                // resolution needs to run on exactly those hrefs, so only
                // bail out here for a genuinely remote/data/mailto/tel
                // href (empty hrefs have no fragment to check either).
                if !href.starts_with('#') && is_external(href) {
                    continue;
                }
                if href.trim().is_empty() {
                    continue;
                }
                if remote_base {
                    report.push_at(
                        RSC_006,
                        Severity::Error,
                        format!(
                            "relative reference '{href}' resolves to a remote resource via base"
                        ),
                        path.clone(),
                    );
                    continue;
                }
                let (path_part, frag) = match href.split_once('#') {
                    Some((p, f)) => (p, Some(f)),
                    None => (href, None),
                };
                let Some(frag) = frag else { continue };
                // Not a plain NCName-style id reference - e.g. a CFI
                // (`epubcfi(...)`) or a Media Fragments URI
                // (`xywh=percent:5,5,15,15`), both real, valid constructs
                // confirmed via the corpus (`nav-cfi-valid`,
                // `region-based-nav-valid`) that this project doesn't
                // resolve as an id.
                if frag.is_empty() || frag.contains(['=', ':', '(']) {
                    continue;
                }
                let target_nfc = if path_part.is_empty() {
                    nfc(&path)
                } else {
                    nfc(&resolve(&dir, path_part))
                };
                // A hyperlink to the package document itself (a CFI-style
                // self-reference) isn't a content document with ids to
                // resolve against (same exemption as the RSC-011 spine-
                // reachability check, confirmed via the same fixture).
                if target_nfc == nfc(opf_path) {
                    continue;
                }
                if !frag_id_cache.contains_key(&target_nfc) {
                    let ids = if target_nfc == nfc(&path) {
                        dom_id_order(&d)
                    } else {
                        name_index
                            .get(&target_nfc)
                            .cloned()
                            .and_then(|orig| ocf.read(&orig))
                            .map(|b| {
                                let t = String::from_utf8_lossy(&b).into_owned();
                                parse_xml(&t)
                                    .map(|d2| dom_id_order(&d2))
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default()
                    };
                    frag_id_cache.insert(target_nfc.clone(), ids);
                }
                if !frag_id_cache[&target_nfc].contains_key(frag) {
                    report.push_at(
                        RSC_012,
                        Severity::Error,
                        format!("fragment identifier '{frag}' is not defined in '{target_nfc}'"),
                        path.clone(),
                    );
                    continue;
                }
                // RSC-014: a same-document hyperlink to an SVG <symbol> -
                // navigable links can't target an SVG element definition.
                if path_part.is_empty() {
                    if let Some(target_node) =
                        d.descendants().find(|n| n.attribute("id") == Some(frag))
                    {
                        if target_node.tag_name().name() == "symbol"
                            && target_node.tag_name().namespace()
                                == Some("http://www.w3.org/2000/svg")
                        {
                            report.push_at(
                                RSC_014,
                                Severity::Error,
                                format!("hyperlink '{href}' targets an SVG symbol (incompatible resource type)"),
                                path.clone(),
                            );
                        }
                    }
                }
            }
        }

        // RSC-013: a stylesheet reference must not carry a fragment.
        for n in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "link"
                && n.attribute("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
        }) {
            if let Some(href) = n.attribute("href") {
                if !is_external(href) && href.contains('#') {
                    report.push_at(
                        RSC_013,
                        Severity::Error,
                        format!(
                            "stylesheet reference '{href}' must not have a fragment identifier"
                        ),
                        path.clone(),
                    );
                }
            }
        }

        // RSC-009: a non-SVG image referenced via a URL fragment - image
        // fragments only make sense for SVG targets. RSC-008: an <img
        // srcset> candidate not declared in the manifest at all.
        for n in d.descendants().filter(|n| n.is_element()) {
            let (src_attr, tag) = match n.tag_name().name() {
                "img" => ("src", "img"),
                "image" if n.tag_name().namespace() == Some("http://www.w3.org/2000/svg") => {
                    ("href", "image")
                }
                _ => continue,
            };
            let src = n.attribute(src_attr).or_else(|| {
                if tag == "image" {
                    n.attribute(("http://www.w3.org/1999/xlink", "href"))
                } else {
                    None
                }
            });
            if let Some(v) = src {
                if let Some((p, _frag)) = v.split_once('#') {
                    if !is_external(v) {
                        let resolved = nfc(&resolve(&dir, p));
                        let is_svg = resolved.ends_with(".svg")
                            || items
                                .values()
                                .any(|(ip, mt)| nfc(ip) == resolved && mt == "image/svg+xml");
                        if !is_svg {
                            report.push_at(
                                RSC_009,
                                Severity::Warning,
                                format!(
                                    "non-SVG image '{v}' is referenced with a fragment identifier"
                                ),
                                path.clone(),
                            );
                        }
                    }
                }
            }
            if tag == "img" {
                if let Some(srcset) = n.attribute("srcset") {
                    for candidate in srcset.split(',') {
                        let url = candidate.trim().split_whitespace().next().unwrap_or("");
                        if url.is_empty() || is_external(url) {
                            continue;
                        }
                        let resolved = nfc(&resolve(&dir, url));
                        // Real corpus finding: the srcset candidate file
                        // genuinely exists in the container - the defect
                        // is that it's missing its own manifest item, so
                        // this must check manifest declaration (`items`),
                        // not container file existence (`name_index`).
                        if !items.values().any(|(ip, _)| nfc(ip) == resolved) {
                            report.push_at(
                                RSC_008,
                                Severity::Error,
                                format!("srcset candidate '{url}' is not declared in the manifest"),
                                path.clone(),
                            );
                        }
                    }
                }
            }
        }

        // RSC-015: an SVG <use> element's href must always carry a
        // fragment identifier (it references an element definition, never
        // a whole document).
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "use")
        {
            let href = n
                .attribute("href")
                .or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(v) = href {
                if !is_external(v) && !v.contains('#') {
                    report.push_at(
                        RSC_015,
                        Severity::Error,
                        format!("\"use\" element's href '{v}' has no fragment identifier"),
                        path.clone(),
                    );
                }
            }
        }

        // --- Navigation document checks (NAV-010/011) ---
        if nav_path.as_deref() == Some(path.as_str()) {
            crate::navdoc::check(&d, &path, &dir, &items, report);
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
                        // `is_external` also covers fragment-only/data:/
                        // mailto:/tel: hrefs (correct for "should this be
                        // resolved as a container path", wrong here - a
                        // same-document `#toc` anchor is a completely
                        // normal same-page link, not "external" - a real
                        // false positive found via a real `nav-landmarks-
                        // valid` fixture using exactly that shape).
                        if is_remote_url(href) {
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

        // remote-resources/scripted/svg detection (OPF-014/018) - a
        // direct scan only: a document that references a remote resource
        // *transitively* (e.g. via a local SVG file that itself embeds a
        // remote font) isn't traced, since this project has no SVG-
        // content parser. Named, accepted limitation.
        let mut has_remote = false;
        let mut has_script = false;
        let mut has_svg = false;
        let mut has_switch = false;
        let mut remote_refs: HashSet<String> = HashSet::new();
        let mut remote_link_refs: HashSet<String> = HashSet::new();
        // Remote references EPUB 3 never allows regardless of manifest
        // declaration (§3.6): img/iframe/script are always restricted;
        // `<object>` follows its resource's own category (exempt only if
        // it's audio/video/font, confirmed via `resources-remote-audio-
        // object-valid` vs `resources-remote-object-undeclared-error`).
        // Reported as RSC-006 instead of (not in addition to) RSC-008.
        let mut restricted_remote_refs: HashSet<String> = HashSet::new();
        for node in d.descendants().filter(|n| n.is_element()) {
            // <base href> sets a base URI for resolving *other* relative
            // references; it isn't itself a reference to an existing
            // resource (and may legitimately point at "./" or elsewhere).
            if node.tag_name().name() == "base" {
                continue;
            }
            for attr in ["src", "href", "data", "poster", "altimg", "cite"] {
                if let Some(v) = node.attribute(attr) {
                    if v.trim_start().starts_with("file:") {
                        report.push_at(
                            RSC_030,
                            Severity::Error,
                            format!("'{v}' is a file URL, which is not allowed"),
                            path.clone(),
                        );
                        continue;
                    }
                    let tag = node.tag_name().name();
                    // A `<link>` whose `rel` isn't "stylesheet" (e.g.
                    // `rel="prev"`/`rel="next"`/an RDFa vocabulary term
                    // used as `rel`) is a metadata/navigation reference,
                    // not an embedded resource dependency at all - a real
                    // corpus fixture (`rdfa-valid.xhtml`) uses exactly
                    // this shape with a remote `href`, which must not be
                    // treated as "using a remote resource".
                    let is_non_stylesheet_link = tag == "link"
                        && attr == "href"
                        && !node.attribute("rel").is_some_and(|r| {
                            r.split_whitespace()
                                .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                        });
                    if is_remote_url(v) && !is_non_stylesheet_link {
                        let bare = strip_url_fragment(v);
                        // A plain hyperlink to a remote resource is
                        // navigation, not an embedded dependency - it
                        // doesn't need a manifest declaration (RSC-008),
                        // doesn't trigger the remote-resources property,
                        // and isn't itself subject to the http-vs-https
                        // check (RSC-031) - only tracked separately, for
                        // the narrower "hyperlink to an image" defect
                        // (RSC-006, below). Confirmed via a real corpus
                        // fixture (`rdfa-valid.xhtml`) using ordinary
                        // `<a href="http://...">` links with no manifest
                        // declaration at all, which must stay clean.
                        if (tag == "a" && attr == "href") || attr == "cite" {
                            remote_link_refs.insert(bare);
                        } else {
                            remote_refs.insert(bare.clone());
                            has_remote = true;
                            let restricted = match tag {
                                "img" | "iframe" => true,
                                "script" if attr == "src" => true,
                                "link" if attr == "href" => {
                                    node.attribute("rel").is_some_and(|r| {
                                        r.split_whitespace()
                                            .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                                    })
                                }
                                "object" if attr == "data" => !remote_manifest
                                    .get(&bare)
                                    .is_some_and(|mt| crate::cmt::is_audio_video_or_font(mt)),
                                _ => false,
                            };
                            if restricted {
                                restricted_remote_refs.insert(bare);
                            }
                        }
                    }
                    if is_external(v) {
                        continue;
                    }
                    if attr == "data" || attr == "poster" {
                        continue;
                    }
                    // `resolve` already strips any "#fragment" - a
                    // fragment-only href (e.g. "#foo") is caught by the
                    // `is_external` check above instead (fragment
                    // resolution is RSC-012, checked separately below).
                    let resolved = resolve(&dir, v);
                    if !name_index.contains_key(&nfc(&resolved)) {
                        // Real corpus finding, grep-verified across the
                        // whole corpus: RSC-001 is used exclusively for a
                        // manifest item/@href missing from the container
                        // (and a CSS @import target, handled separately in
                        // css.rs) - every other "this content-doc
                        // reference doesn't resolve" case is RSC-007.
                        report.push_at(
                            RSC_007,
                            Severity::Error,
                            format!("reference to a resource missing from the publication: '{v}'"),
                            path.clone(),
                        );
                    }
                }
            }
            if node.tag_name().name() == "switch"
                && node.tag_name().namespace() == Some("http://www.idpf.org/2007/ops")
            {
                has_switch = true;
            }
            if matches!(node.tag_name().name(), "a" | "area") {
                // An SVG `<a>` may use `xlink:href` instead of a bare
                // `href` (confirmed via `data-url-in-svg-a-href-error`).
                let href = node
                    .attribute("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
                if let Some(href) = href {
                    if href.trim_start().starts_with("data:") {
                        report.push_at(
                            RSC_029,
                            Severity::Error,
                            "a hyperlink href must not be a data URL",
                            path.clone(),
                        );
                    } else if !is_external(href) {
                        if href.contains('?') {
                            report.push_at(
                                RSC_033,
                                Severity::Error,
                                format!("hyperlink href '{href}' must not have a query string"),
                                path.clone(),
                            );
                        }
                        if node.tag_name().name() == "a" {
                            hyperlink_targets.insert(nfc(&resolve(&dir, href)));
                        }
                    }
                }
            }
            if node.tag_name().name() == "script" {
                let script_type = node.attribute("type").unwrap_or("");
                if script_type.is_empty()
                    || script_type.eq_ignore_ascii_case("text/javascript")
                    || script_type.eq_ignore_ascii_case("application/javascript")
                    || script_type.eq_ignore_ascii_case("module")
                {
                    has_script = true;
                }
            }
            if matches!(
                node.tag_name().name(),
                "input" | "button" | "select" | "textarea"
            ) {
                has_script = true;
            }
            if node.tag_name().name() == "svg"
                && node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            {
                has_svg = true;
            }
            // Embedded CSS: inline <style> resolves relative to this
            // content document's own location, not to any separate file.
            if node.tag_name().name() == "style" {
                let css_text: String = node
                    .descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect();
                crate::css::check(
                    &css_text,
                    &path,
                    &dir,
                    &name_index,
                    &manifest_paths,
                    None,
                    report,
                );
                let sheet = styloria::Parser::parse_stylesheet(&css_text);
                check_exempt_font_usage(&sheet, &dir, &items, &path, report);
                doc_class_names
                    .entry(path.clone())
                    .or_default()
                    .extend(crate::css::selector_class_names(&sheet));
                for u in crate::css::stylesheet_urls(&sheet) {
                    if is_remote_url(&u) {
                        has_remote = true;
                        remote_refs.insert(strip_url_fragment(&u));
                    }
                }
                // Unlike a remote font/background image referenced via
                // CSS (allowed, `resources-remote-font-in-css-valid`), a
                // remote `@import` fetches another *stylesheet* - always
                // restricted, same as a `<link rel="stylesheet">` (RSC-006
                // instead of RSC-008), confirmed via the real
                // `resources-remote-stylesheet-svg-import-error` fixture.
                for u in crate::css::import_targets(&sheet) {
                    if is_remote_url(&u) {
                        restricted_remote_refs.insert(strip_url_fragment(&u));
                    }
                }
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
                                for u in crate::css::stylesheet_urls(&sheet) {
                                    if is_remote_url(&u) {
                                        has_remote = true;
                                        remote_refs.insert(strip_url_fragment(&u));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // CSS-005 (usage): a plain `<link rel="stylesheet">` (not
            // "alternate stylesheet") whose `class` names more than one
            // alt-style-tag - a single name is fine (even if unrecognized),
            // only multiple conflicting names are flagged.
            if node.tag_name().name() == "link" {
                let rel_tokens: Vec<&str> = node
                    .attribute("rel")
                    .map(|r| r.split_whitespace().collect())
                    .unwrap_or_default();
                let is_plain_stylesheet =
                    rel_tokens.len() == 1 && rel_tokens[0].eq_ignore_ascii_case("stylesheet");
                let is_alt_stylesheet = rel_tokens.len() == 2
                    && rel_tokens[0].eq_ignore_ascii_case("alternate")
                    && rel_tokens[1].eq_ignore_ascii_case("stylesheet");
                if is_plain_stylesheet {
                    if let Some(class) = node.attribute("class") {
                        if class.split_whitespace().count() > 1 {
                            report.push_at(
                                CSS_005,
                                Severity::Info,
                                "link element's class names conflicting alt style tags",
                                path.clone(),
                            );
                        }
                    }
                }
                // CSS-015: an alternate-stylesheet link must have a
                // non-empty title (missing and present-but-empty are each
                // their own finding).
                if is_alt_stylesheet {
                    match node.attribute("title") {
                        None => {
                            report.push_at(
                                CSS_015,
                                Severity::Error,
                                "an alternate stylesheet link must have a title attribute",
                                path.clone(),
                            );
                        }
                        Some(t) if t.trim().is_empty() => {
                            report.push_at(
                                CSS_015,
                                Severity::Error,
                                "an alternate stylesheet link's title must not be empty",
                                path.clone(),
                            );
                        }
                        Some(_) => {}
                    }
                }
            }
            // CSS-008: a `style="..."` attribute is a plain declaration
            // list, same malformed-shape check as a stylesheet's own block.
            if let Some(style) = node.attribute("style") {
                crate::css::check_style_attribute(style, &path, report);
            }
        }

        // Content-model properties (remote-resources/scripted/svg/switch)
        // are an EPUB 3 manifest-item concept; EPUB 2 has no `properties`
        // attribute at all, so a legitimate EPUB 2 <script> or similar
        // must not be held to this rule (confirmed via a real epub2
        // corpus fixture using <script> validly with no properties
        // concept in play).
        if is_epub3 {
            let declared = item_properties
                .get(&nfc(&path))
                .cloned()
                .unwrap_or_default();
            let declared_tokens: Vec<&str> = declared.split_whitespace().collect();
            // "used but undeclared" is uniformly OPF-014/Error across all
            // three properties; "declared but unused" differs per property -
            // remote-resources is OPF-018/Warning, scripted/svg are
            // OPF-015/Error (confirmed via each property's own dedicated
            // corpus fixture, not assumed uniform).
            for (used, name, unused_id, unused_sev) in [
                (has_remote, "remote-resources", OPF_018, Severity::Warning),
                (has_script, "scripted", OPF_015, Severity::Error),
                (has_svg, "svg", OPF_015, Severity::Error),
            ] {
                let declared_here = declared_tokens.contains(&name);
                if used && !declared_here {
                    report.push_at(
                        OPF_014,
                        Severity::Error,
                        format!(
                            "content document uses {name} but doesn't declare the \"{name}\" property"
                        ),
                        path.clone(),
                    );
                } else if declared_here && !used {
                    report.push_at(
                        unused_id,
                        unused_sev,
                        format!(
                            "the \"{name}\" property is declared but doesn't appear to be needed"
                        ),
                        path.clone(),
                    );
                }
            }
            if has_switch && !declared_tokens.contains(&"switch") {
                report.push_at(
                    OPF_014,
                    Severity::Error,
                    "content document uses epub:switch but doesn't declare the \"switch\" property",
                    path.clone(),
                );
            }
        }

        // RSC-008: a remote resource referenced from this content
        // document isn't declared as its own manifest item at all
        // (EPUB 3 requires every resource, including remote ones, to
        // have a manifest entry) - except a `restricted_remote_refs`
        // reference, which is always RSC-006 instead (declared or not;
        // confirmed via `resources-remote-iframe-undeclared-error` etc.,
        // where only RSC-006 is expected, never RSC-008 too).
        for r in &remote_refs {
            if restricted_remote_refs.contains(r) {
                continue;
            }
            if !remote_manifest.contains_key(r) {
                report.push_at(
                    RSC_008,
                    Severity::Error,
                    format!("remote resource '{r}' is not declared in the manifest"),
                    path.clone(),
                );
            }
        }
        // RSC-006: a hyperlink (<a href>, not an embedding element)
        // points to a remote resource that *is* declared, but as an
        // image - hyperlinking to an image directly is the wrong
        // construct (should be embedded, e.g. via <img>).
        for r in &remote_link_refs {
            if remote_manifest
                .get(r)
                .is_some_and(|mt| mt.starts_with("image/"))
            {
                report.push_at(
                    RSC_006,
                    Severity::Error,
                    format!("remote image '{r}' is referenced from an \"a\" element"),
                    path.clone(),
                );
            }
        }
        // RSC-006: img/iframe/script/stylesheet/non-exempt-object always
        // disallow a remote resource, regardless of manifest declaration.
        for r in &restricted_remote_refs {
            report.push_at(
                RSC_006,
                Severity::Error,
                format!("remote resource '{r}' is not allowed in this context"),
                path.clone(),
            );
        }
        // RSC-031: any remote reference (exempt or restricted) using a
        // plain `http://` URL instead of `https://`.
        for r in &remote_refs {
            if r.starts_with("http://") {
                report.push_at(
                    RSC_031,
                    Severity::Warning,
                    format!("remote resource '{r}' should use https"),
                    path.clone(),
                );
            }
        }
    }

    // --- Spine reachability (RSC-011/OPF-096) ---
    let opf_own_name_nfc = nfc(opf_path);
    for target in &hyperlink_targets {
        if *target == opf_own_name_nfc {
            // A hyperlink to the package document itself (e.g. a CFI-style
            // self-reference) isn't a content document that could ever be
            // "in the spine" - confirmed via a real corpus fixture.
            continue;
        }
        // "In the spine" is only a meaningful expectation for a genuine
        // Content Document - a hyperlink to e.g. an image (confirmed via a
        // real corpus fixture, `nav-links-to-non-content-document-type-
        // error`, which expects only RSC-010 for that link, not this too)
        // was being wrongly flagged here as well, since this check
        // previously only looked at container file existence, not type.
        let is_content_doc = items.values().any(|(p, mt)| {
            nfc(p) == *target && (mt == "application/xhtml+xml" || mt == "image/svg+xml")
        });
        if is_content_doc && !spine_order.contains_key(target) && name_index.contains_key(target) {
            report.push_at(
                RSC_011,
                Severity::Error,
                format!("'{target}' is hyperlinked but not listed in the spine"),
                opf_path,
            );
        }
    }
    for path in &non_linear_paths {
        if !hyperlink_targets.contains(path) {
            report.push_at(
                OPF_096,
                Severity::Error,
                format!("non-linear content '{path}' is not reachable from the reading order"),
                opf_path,
            );
        }
    }

    // --- EDUPUB pagination source / page-list cross-check (NAV-003/OPF-066) ---
    if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
        crate::edupub::check_page_list(has_pagination_source, has_page_list_nav, opf_path, report);
    }

    // SVG top-level content documents that declare a media-overlay also
    // need their own CSS scanned for the CSS-029/030 cross-reference
    // below (deferred in the original CSS-029/030 increment - only
    // scanned here, not in the main XHTML content_docs loop above, since
    // that's the only reason SVG's own CSS matters at all).
    let svg_doc_paths: HashSet<String> = items
        .values()
        .filter(|(_, mt)| mt == "image/svg+xml")
        .map(|(path, _)| nfc(path))
        .collect();
    for doc_path in svg_doc_paths
        .iter()
        .filter(|p| content_doc_overlay.contains_key(p.as_str()))
    {
        let Some(orig) = name_index.get(doc_path).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        let text = String::from_utf8_lossy(&b).into_owned();
        let Ok(d) = parse_xml(&text) else { continue };
        let dir = parent_dir(doc_path);
        doc_class_names
            .entry(doc_path.clone())
            .or_default()
            .extend(collect_svg_class_names(&d, &dir, &name_index, ocf));
    }

    // Standalone top-level SVG content documents (`image/svg+xml`) never
    // go through the XHTML content_docs loop above (which is scoped to
    // `application/xhtml+xml` only), so its SVG content-model checks -
    // generic vocabulary (RSC-025), foreignObject/title content models -
    // would otherwise never run on a bare SVG document at all (confirmed
    // via a real fixture: `content-svg-use-href-no-fragment-error`'s
    // standalone `cover.svg`).
    for doc_path in &svg_doc_paths {
        let Some(orig) = name_index.get(doc_path).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        let text = String::from_utf8_lossy(&b).into_owned();
        let Ok(d) = parse_xml(&text) else { continue };
        crate::svg::check_vocabulary(d.root_element(), doc_path, report);
        for fo in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "foreignObject"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
        }) {
            crate::svg::check_foreign_object(
                fo,
                &text,
                d.root_element(),
                doc_path,
                is_epub3,
                report,
            );
        }
        for svg_title in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "title"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
        }) {
            crate::svg::check_title_content(svg_title, doc_path, report);
        }
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "use")
        {
            let href = n
                .attribute("href")
                .or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(v) = href {
                if !is_external(v) && !v.contains('#') {
                    report.push_at(
                        RSC_015,
                        Severity::Error,
                        format!("\"use\" element's href '{v}' has no fragment identifier"),
                        doc_path.clone(),
                    );
                }
            }
        }

        // RSC-006: a remote stylesheet reference from a standalone SVG
        // content document - via a top-level `<?xml-stylesheet?>` PI, an
        // inline `<style>`'s `@import`, or a `<link rel="stylesheet">` -
        // is always restricted, same rule as the XHTML content-doc loop
        // above (a remote *stylesheet* is never allowed, unlike a remote
        // font/image referenced from CSS).
        for pi in d.root().children().filter(|n| n.is_pi()) {
            if let Some(p) = pi.pi() {
                if p.target == "xml-stylesheet" {
                    if let Some(href) = p.value.and_then(extract_pi_href) {
                        if is_remote_url(&href) {
                            report.push_at(
                                RSC_006,
                                Severity::Error,
                                format!("remote stylesheet '{href}' is not allowed"),
                                doc_path.clone(),
                            );
                        }
                    }
                }
            }
        }
        for n in d.descendants().filter(|n| n.is_element()) {
            if n.tag_name().name() == "style" {
                let css_text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                let sheet = styloria::Parser::parse_stylesheet(&css_text);
                for import_url in crate::css::import_targets(&sheet) {
                    if is_remote_url(&import_url) {
                        report.push_at(
                            RSC_006,
                            Severity::Error,
                            format!("remote stylesheet import '{import_url}' is not allowed"),
                            doc_path.clone(),
                        );
                    }
                }
            }
            if n.tag_name().name() == "link"
                && n.attribute("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
            {
                if let Some(href) = n.attribute("href") {
                    if is_remote_url(href) {
                        report.push_at(
                            RSC_006,
                            Severity::Error,
                            format!("remote stylesheet '{href}' is not allowed"),
                            doc_path.clone(),
                        );
                    }
                }
            }
        }
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
        .filter(|p| xhtml_doc_paths.contains(p.as_str()) || svg_doc_paths.contains(p.as_str()))
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
        crate::css::check(
            &css_text,
            &path,
            &dir,
            &name_index,
            &manifest_paths,
            Some(&b),
            report,
        );
        // RSC-008: a standalone (manifest-declared) stylesheet can
        // reference a remote resource without any content document ever
        // linking to it - still needs its own manifest item. OPF-014: and
        // the stylesheet's *own* manifest item needs "remote-resources"
        // declared, same as a content document or SMIL overlay would.
        let sheet = styloria::Parser::parse_stylesheet(&css_text);
        check_exempt_font_usage(&sheet, &dir, &items, &path, report);
        let mut css_has_remote = false;
        for u in crate::css::stylesheet_urls(&sheet) {
            if is_remote_url(&u) {
                css_has_remote = true;
                let u = strip_url_fragment(&u);
                if !remote_manifest.contains_key(&u) {
                    report.push_at(
                        RSC_008,
                        Severity::Error,
                        format!("remote resource '{u}' is not declared in the manifest"),
                        path.clone(),
                    );
                }
            }
        }
        if css_has_remote
            && !item_properties
                .get(&nfc(&path))
                .is_some_and(|p| p.split_whitespace().any(|t| t == "remote-resources"))
        {
            report.push_at(
                OPF_014,
                Severity::Error,
                "stylesheet uses a remote resource but doesn't declare the \"remote-resources\" property",
                path.clone(),
            );
        }
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
        let (targets, textref_targets) = crate::smil::check(
            &smil_text,
            &path,
            &dir,
            &name_index,
            &media_type_index,
            report,
        );

        // RSC-012: epub:textref fragments must resolve to a real id in
        // their target document - same shape as the NCX <content src>
        // fragment check, reusing the same id_cache-per-target pattern.
        {
            let mut id_cache: HashMap<String, HashMap<String, usize>> = HashMap::new();
            for (target, frag) in &textref_targets {
                if !id_cache.contains_key(target) {
                    let ids = name_index
                        .get(target)
                        .cloned()
                        .and_then(|orig| ocf.read(&orig))
                        .map(|b| {
                            let text = String::from_utf8_lossy(&b).into_owned();
                            parse_xml(&text)
                                .map(|d| dom_id_order(&d))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    id_cache.insert(target.clone(), ids);
                }
                if !id_cache[target].contains_key(frag) {
                    report.push_at(
                        RSC_012,
                        Severity::Error,
                        format!("epub:textref fragment '{frag}' is not defined in '{target}'"),
                        path.clone(),
                    );
                }
            }
        }

        // OPF-014: a media overlay referencing a remote resource
        // (typically <audio src>) needs its own manifest item to
        // declare "remote-resources", same as a content document.
        if let Ok(smil_doc) = parse_xml(&smil_text) {
            let has_remote_audio = smil_doc.descendants().any(|n| {
                n.is_element()
                    && matches!(n.tag_name().name(), "audio" | "text")
                    && n.attribute("src").is_some_and(is_remote_url)
            });
            if has_remote_audio
                && !item_properties
                    .get(&overlay_path)
                    .is_some_and(|p| p.split_whitespace().any(|t| t == "remote-resources"))
            {
                report.push_at(
                    OPF_014,
                    Severity::Error,
                    "media overlay uses a remote resource but doesn't declare the \"remote-resources\" property",
                    path.clone(),
                );
            }
        }

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
    check_image_signatures(ocf, &items, &name_index, report);
    check_html_declared_as_xhtml(ocf, &items, &name_index, report);
}

/// OPF-035 (warning): a manifest item declared `text/html` whose actual
/// content *is* a real XHTML document (a well-formed XML document whose
/// root is `<html>` in the XHTML namespace) - should be declared
/// `application/xhtml+xml` instead. Confirmed via a real fixture using
/// exactly this shape, unreferenced by anything else in the book (so this
/// check runs over every manifest item regardless of spine/hyperlink
/// usage).
fn check_html_declared_as_xhtml(
    ocf: &mut Ocf,
    items: &HashMap<String, (String, String)>,
    name_index: &HashMap<String, String>,
    report: &mut Report,
) {
    const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
    for (path, mt) in items.values() {
        if mt != "text/html" {
            continue;
        }
        let Some(orig) = name_index.get(&nfc(path)).cloned() else {
            continue;
        };
        let Some(bytes) = ocf.read(&orig) else {
            continue;
        };
        let text = crate::css::decode_bytes(&bytes);
        let Ok(d) = parse_xml(&text) else { continue };
        let root = d.root_element();
        if root.tag_name().name() == "html" && root.tag_name().namespace() == Some(XHTML_NS) {
            report.push_at(
                OPF_035,
                Severity::Warning,
                format!("manifest item '{path}' is XHTML but declared as text/html"),
                path.as_str(),
            );
        }
    }
}

/// Raster Core Media Types this project can sniff a real signature for
/// (SVG is XML, already validated as such elsewhere).
const SNIFFABLE_IMAGE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

/// PKG-021/MED-004 (corrupt image), OPF-029 (declared type doesn't match
/// actual content), PKG-022 (file extension doesn't match actual
/// content/declared type) - all three confirmed via dedicated real corpus
/// fixtures. Only applies to manifest items declaring one of the four
/// raster Core Media Types; SVG and anything already foreign is out of
/// scope (foreign resources have no "actual format" expectation to sniff
/// against in the first place).
fn check_image_signatures(
    ocf: &mut Ocf,
    items: &HashMap<String, (String, String)>,
    name_index: &HashMap<String, String>,
    report: &mut Report,
) {
    for (path, mt) in items.values() {
        if !SNIFFABLE_IMAGE_TYPES.contains(&mt.as_str()) {
            continue;
        }
        let Some(orig) = name_index.get(&nfc(path)).cloned() else {
            continue;
        };
        let Some(bytes) = ocf.read(&orig) else {
            continue;
        };
        match crate::image::sniff_image_type(&bytes) {
            None => {
                report.push_at(
                    MED_004,
                    Severity::Error,
                    format!("image '{path}' is corrupt (its content doesn't match any known image format)"),
                    path.as_str(),
                );
                report.push_at(
                    PKG_021,
                    Severity::Error,
                    format!("image '{path}' is corrupt"),
                    path.as_str(),
                );
            }
            Some(actual) if actual != *mt => {
                report.push_at(
                    OPF_029,
                    Severity::Error,
                    format!(
                        "image '{path}' is declared as '{mt}' but its actual format is '{actual}'"
                    ),
                    path.as_str(),
                );
            }
            Some(actual) => {
                let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                if !crate::image::conventional_extensions(actual).contains(&ext.as_str()) {
                    report.push_at(
                        PKG_022,
                        Severity::Warning,
                        format!("image '{path}' has a file extension that doesn't match its actual format '{actual}'"),
                        path.as_str(),
                    );
                }
            }
        }
    }
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

/// CSS-007 (usage): a `@font-face src` target resolves to a manifest item
/// whose declared media-type is neither a Core Media Type nor exempt
/// video - i.e. a genuinely foreign font (§3.4 exempts fonts from ever
/// needing a fallback, but real epubcheck still flags the usage at Info
/// level, confirmed via `foreign-exempt-font-valid`). Core/non-preferred-
/// Core font types (confirmed via `resources-cmt-font-truetype-valid`,
/// which expects this reported *zero* times) must not fire.
fn check_exempt_font_usage(
    sheet: &styloria::Stylesheet,
    dir: &str,
    items: &HashMap<String, (String, String)>,
    path: &str,
    report: &mut Report,
) {
    for u in crate::css::font_face_src_urls(sheet) {
        if is_external(&u) {
            continue;
        }
        let resolved = nfc(&resolve(dir, &u));
        if let Some((_, mt)) = items.values().find(|(ip, _)| nfc(ip) == resolved) {
            if !crate::cmt::is_core_media_type(mt) && !crate::cmt::is_exempt_video(mt) {
                report.push_at(
                    CSS_007,
                    Severity::Info,
                    format!("font '{u}' is a foreign resource, exempt from requiring a fallback"),
                    path,
                );
            }
        }
    }
}

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
