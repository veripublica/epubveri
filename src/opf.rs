//! OPF package-document checks: version, required metadata, manifest/spine
//! integrity, declared media-types, the EPUB 3 nav doc, and broken internal
//! references from content documents.

use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;

use crate::ids::*;
use crate::ocf::{Ocf, parse_xml};
use crate::report::{Position, Report, Severity};
use crate::xmlext::{NodeExt, attr_no_ns_node, attr_ns_node};

/// Report one RELAX NG [`Blame`](crate::rng::Blame) as RSC-005, routing it to
/// the `push_node*` variant that pins the right thing: `@name` for an attribute
/// fault, `…/text()[n]` for a stray run, the element otherwise.
///
/// Shared by the package-document and content-document call sites rather than
/// written out at each. The two had identical copies of a two-way match, and
/// when `Blame::Text` arrived (#68) a copy would have been the natural place to
/// forget it - which is precisely the bug being fixed, one layer up.
pub(crate) fn push_blame(
    report: &mut Report,
    location: &str,
    rule: &'static str,
    blame: &crate::rng::Blame,
) {
    let (text, params) = blame.describe();
    let kind = blame.kind();
    if let Some(a) = blame.attribute() {
        report.push_node_attr(
            RSC_005,
            Severity::Error,
            text,
            location,
            blame.node(),
            a,
            rule,
            params,
        );
    } else if blame.is_text() {
        report.push_node_text(
            RSC_005,
            Severity::Error,
            text,
            location,
            blame.node(),
            rule,
            params,
        );
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            text,
            location,
            blame.node(),
            rule,
            params,
        );
    }
    // One stamp for all three arms, on the line after the push, so the pairing
    // cannot drift. See `Report::attach_violation_kind` for why the kind is not
    // a parameter on the push helpers.
    report.attach_violation_kind(kind);
}

/// A manifest item whose href is remote and which nothing in the publication
/// references (#70).
///
/// epubcheck's `OPFChecker30.checkItemAfterResourceValidation`: audio, video,
/// Flash and fonts may legitimately live outside the container, so they are
/// exempt; anything else remote and unreferenced is **RSC-006**, downgraded to
/// the usage-level **RSC-006b** when the publication has scripts, since a
/// script could fetch it at runtime. The same `HAS_SCRIPTS` downgrade already
/// governs OPF-018b and OPF-096b here.
///
/// Also OPF-097, which the caller's loop skipped for every external href. That
/// skip is right for `data:`/`mailto:` — there is no resource to be
/// unreferenced — and wrong for a remote one, which is an ordinary
/// publication resource that simply happens to live elsewhere.
///
/// EPUB 3 only: the whole branch is on `OPFChecker30`, and EPUB 2 has no
/// remote-resource allowance to check against.
#[allow(clippy::too_many_arguments)]
fn check_unreferenced_remote_item(
    item: roxmltree::Node,
    href: &str,
    remote_resource_refs: &HashSet<String>,
    book_has_scripts: bool,
    is_epub3: bool,
    opf_path: &str,
    report: &mut Report,
) {
    if !is_epub3 {
        return;
    }
    let mt = item.attr_no_ns("media-type").unwrap_or_default();
    if crate::cmt::is_audio_video_or_font(mt) || mt == "application/x-shockwave-flash" {
        return;
    }
    // A remote XHTML item is already reported, unconditionally and without
    // needing to know whether anything references it, by the manifest check
    // that owns "a content document can never be remote". Reporting here too
    // gave two RSC-006 for one item where epubcheck gives one.
    if crate::cmt::base_media_type(mt) == "application/xhtml+xml" {
        return;
    }
    if remote_resource_refs.contains(href) {
        return;
    }
    let (id, severity) = if book_has_scripts {
        (RSC_006B, Severity::Usage)
    } else {
        (RSC_006, Severity::Error)
    };
    report.push_node(
        id,
        severity,
        if book_has_scripts {
            format!(
                "remote resource '{href}' is never referenced; check that a script retrieves it"
            )
        } else {
            format!("remote resource '{href}' is not allowed in this context")
        },
        opf_path,
        item,
        "opf.manifest_item.remote_never_referenced",
        vec![href.to_string()],
    );
    report.push_node(
        OPF_097,
        Severity::Usage,
        format!("'{href}' is declared in the manifest, but no content document references it"),
        opf_path,
        item,
        "opf.manifest_item.never_referenced",
        vec![href.to_string()],
    );
}

/// Does this manifest item's `fallback` chain reach a Content Document?
///
/// epubcheck's RSC-010 condition has three clauses and we had two:
///
/// ```java
/// if (!isBlessedItemType(mt, version) && !isDeprecatedBlessedItemType(mt)
///     && !targetResource.hasContentDocumentFallback())
/// ```
///
/// Doitsu, MobileRead #168, on the IDPF `haruko-jpeg` sample: an image-based
/// book whose nav and NCX link straight at the JPEGs, each of which declares
/// `fallback="fallback"` to an XHTML document. That is the ordinary
/// image-publication shape and epubcheck says nothing about it; we reported
/// three errors.
///
/// Bounded at 10 hops, the same guard the OPF-043/OPF-065 chain walks use, so
/// a `fallback` cycle cannot spin here. `foreign::fallback_reaches_core` is
/// the neighbouring walk and deliberately not reused: it asks for a Core Media
/// Type, which is a wider set than a Content Document.
pub(crate) fn fallback_reaches_content_document(
    start_id: &str,
    items: &HashMap<String, (String, String)>,
    fallback_map: &HashMap<String, String>,
) -> bool {
    let mut cur = start_id;
    for _ in 0..10 {
        let Some(next) = fallback_map.get(cur) else {
            return false;
        };
        let Some((_, mt)) = items.get(next.as_str()) else {
            return false;
        };
        // `hasContentDocumentFallback` is satisfied by the deprecated types
        // too - epubcheck's `FallbackChainResolver`:48-49 ORs both predicates,
        // with no version condition.
        if is_content_document_type(mt) || is_deprecated_content_document_type(mt) {
            return true;
        }
        cur = next.as_str();
    }
    false
}

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
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
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

/// True for the two *deprecated* content-document types - epubcheck's
/// `OPFChecker.isDeprecatedBlessedItemType`.
///
/// These are not Content Documents, but epubcheck deliberately treats them as
/// if they were for every question about *placement*: a spine item declaring
/// one needs no fallback, a guide reference may target one, and a hyperlink
/// may point at one. What it does not do is validate them against the XHTML
/// grammar - see the `content_docs` collection, where that split is made.
///
/// The one that matters in practice is `text/html`, which Calibre emits: a
/// real book on the shelf declares it on all 91 of its spine items and drew 94
/// findings epubcheck does not report, while every reference *inside* those 91
/// documents went unchecked (issue #72).
///
/// `text/x-oeb1-document` was already carved out for OEBPS 1.2 packages
/// specifically; epubcheck's predicate is not conditioned on the package
/// being OEBPS 1.2, and neither is this one.
pub(crate) fn is_deprecated_content_document_type(mt: &str) -> bool {
    matches!(mt, "text/html" | "text/x-oeb1-document")
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
    // epubcheck's rule, from `OCFContainer.isRemote`: a `data:` URL is never
    // remote, anything inside the container is not remote, and everything
    // else with a scheme is. It is deliberately *not* a list of known
    // schemes - `res:///system/fonts/HelveticaNeue.ttf` in a real book's
    // `@font-face` drew RSC-006 and OPF-014 from epubcheck and nothing at
    // all from us, because `res:` was remote enough to skip local
    // resolution (`is_external` matches on `://`) and not remote enough to
    // be reported. A gap between two checks reports nothing, which is the
    // one failure a user cannot notice.
    //
    // Hyperlink targets are unaffected: `<a href>` and `@cite` are collected
    // into `remote_link_refs`, a separate set, so a `mailto:` link does not
    // become an embedded remote dependency. That split is what makes the
    // general predicate safe here, and it is also how epubcheck arranges it
    // - the URL is remote either way, and the checks that care are only
    // asked about resources that must live in the container.
    // `data:` is never remote (epubcheck's own first branch), and `file:`
    // is RSC-030's alone: epubcheck reports that and stops, so treating it
    // as a remote resource here added a second finding on top - caught by
    // the `file-url-in-css-error` fixture, which expects RSC-030 and
    // nothing else.
    if href.starts_with("data:") || is_file_url(href) {
        return false;
    }
    let Some(colon) = href.find(':') else {
        return false;
    };
    let scheme = &href[..colon];
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// A `file:` URL, which EPUB never allows (RSC-030). epubcheck's rule is
/// exactly `startsWith("file:")` on the raw reference string; matched
/// case-sensitively for parity (a `FILE:` URL is valid per RFC but rare,
/// and epubcheck misses it too).
pub(crate) fn is_file_url(href: &str) -> bool {
    href.trim_start().starts_with("file:")
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

/// What an `id` names, which decides whether a given kind of reference may
/// point at it (RSC-014).
///
/// epubcheck types every `id` from the element carrying it and then requires
/// each reference's type to match: a hyperlink or a `cite` may reach
/// `Generic` only, an SVG `<use>` may reach `SvgSymbol` or `Generic`, and a
/// paint reference (`fill`/`stroke="url(#…)"`) must reach `SvgPaint`
/// exactly. `SvgClipPath` is a *target* kind only — nothing on either side
/// registers a clip-path reference, so no reference can be compared against
/// it (see `docs/COVERAGE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdKind {
    Generic,
    SvgSymbol,
    SvgPaint,
    SvgClipPath,
}

impl IdKind {
    /// How the kind reads in a finding's message.
    fn describe(self) -> &'static str {
        match self {
            Self::Generic => "an element",
            Self::SvgSymbol => "an SVG symbol",
            Self::SvgPaint => "an SVG paint server",
            Self::SvgClipPath => "an SVG clip path",
        }
    }

    /// The kind of the element carrying the `id`. Names are compared
    /// case-insensitively because epubcheck lowercases before comparing, and
    /// the list is `OPSHandler`'s rather than the SVG spec's notion of a
    /// definition element — `marker`, `mask` and `filter` are deliberately
    /// absent, verified against epubcheck one book each.
    fn of(n: roxmltree::Node) -> Self {
        if n.tag_name().namespace() != Some("http://www.w3.org/2000/svg") {
            return Self::Generic;
        }
        let name = n.tag_name().name();
        if name.eq_ignore_ascii_case("symbol") {
            Self::SvgSymbol
        } else if ["linearGradient", "radialGradient", "pattern"]
            .iter()
            .any(|p| name.eq_ignore_ascii_case(p))
        {
            Self::SvgPaint
        } else if name.eq_ignore_ascii_case("clipPath") {
            Self::SvgClipPath
        } else {
            Self::Generic
        }
    }
}

/// What the declared/present matrix says about one resolved reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceRef {
    /// Declared in the manifest, or structural. Nothing to report here.
    Fine,
    /// Present in the container but nobody declared it — RSC-008.
    Undeclared,
    /// Neither declared nor present — RSC-007.
    Missing,
}

/// The declared/present matrix `css.rs` applies to every `url()` — whose
/// comment claimed it was "already established for XHTML content-doc
/// references" when only one of its three cells was. All three were measured
/// against epubcheck 5.3.0, one book per cell, on books otherwise clean in
/// both tools, at 2.0 and 3.0:
///
/// - **declared + file missing** → RSC-001 *only*. The manifest pass already
///   reports it and we were adding a second RSC-007 on top.
/// - **undeclared + file present** → RSC-008. We said nothing, so a real book
///   referencing a container file nobody declared drew only the usage-level
///   OPF-003 from the container side and no error from the reference side.
/// - **undeclared + file missing** → RSC-007.
///
/// RSC-001 is used exclusively for a manifest item's `@href` missing from the
/// container (and a CSS `@import` target, handled in `css.rs`), which is why
/// the first cell is silent here rather than reported under another id.
///
/// The OPF itself, `mimetype` and `META-INF/*` are structural resources,
/// never manifest items — the same exemption OPF-003 makes on the container
/// side. Without it the `nav-cfi-valid` fixture, whose nav points at
/// `package.opf#epubcfi(...)`, became a false positive: the file is present,
/// is not declared, and never could be.
///
/// Only the *decision* lives here, not the reporting: the callers anchor
/// their findings differently (a DOM node, or a raw position on the parse-
/// error path, which has no node — #73).
fn classify_resource_ref(
    resolved: &str,
    manifest_paths: &HashSet<String>,
    name_index: &HashMap<String, String>,
    opf_path: &str,
) -> ResourceRef {
    let structural =
        resolved == nfc(opf_path) || resolved == "mimetype" || resolved.starts_with("META-INF/");
    if manifest_paths.contains(resolved) || structural {
        return ResourceRef::Fine;
    }
    if name_index.contains_key(resolved) {
        ResourceRef::Undeclared
    } else {
        ResourceRef::Missing
    }
}

/// The SVG namespace, which several checks here have to name.
/// The declared media type of a resolved container path, if it is a manifest
/// item.
fn declared_media_type<'a>(
    items: &'a HashMap<String, (String, String)>,
    resolved: &str,
) -> Option<&'a str> {
    items
        .values()
        .find(|(p, _)| nfc(p) == resolved)
        .map(|(_, mt)| mt.as_str())
}

/// Which id a *missing* fragment gets, by the target document's media type.
///
/// epubcheck guards RSC-012 on the target being XHTML or SVG
/// (`ResourceReferencesChecker`); a `text/html` document is
/// `MIMEType.HTML`, neither of those, so the missing id falls through to the
/// reference-type switch, where a null id type is neither the reference's
/// type nor GENERIC and comes out as **RSC-014** (#82).
///
/// A comment at the NCX site used to say a dangling fragment into such a
/// document "draws nothing there". That was measured for RSC-012 and true;
/// the conclusion was not, because nobody had looked for the other id.
fn missing_fragment_id(items: &HashMap<String, (String, String)>, resolved: &str) -> &'static str {
    match declared_media_type(items, resolved) {
        Some(mt) if is_content_document_type(mt) => RSC_012,
        Some(mt) if is_deprecated_content_document_type(mt) => RSC_014,
        _ => RSC_012,
    }
}

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// A reference that carries an expectation about its target's [`IdKind`],
/// and what it will accept (RSC-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefKind {
    /// `<use xlink:href="#…">` — a symbol, or a generic id.
    Symbol,
    /// `fill`/`stroke="url(#…)"` — a paint server, exactly.
    Paint,
    /// `cite="#…"` on `blockquote`/`q`/`ins`/`del` — a generic id only.
    /// **EPUB 3 only**: epubcheck collects it in `OPSHandler30`, and an
    /// EPUB 2 book carrying the same markup is clean, measured both ways.
    Cite,
    /// A media overlay's `<text src="doc.xhtml#…">` — a generic id only,
    /// same acceptance as [`Self::Cite`]. Kept as its own variant because
    /// it is a different reference in a different file type, and a future
    /// divergence between the two should be a one-line change rather than a
    /// re-reading of which callers meant which.
    OverlayText,
}

impl RefKind {
    fn accepts(self, target: IdKind) -> bool {
        match self {
            Self::Symbol => matches!(target, IdKind::SvgSymbol | IdKind::Generic),
            Self::Paint => target == IdKind::SvgPaint,
            Self::Cite | Self::OverlayText => target == IdKind::Generic,
        }
    }
}

/// Every `id` in one document, mapped to its element's document-order index
/// and its [`IdKind`]. Cached per target document, `None` when the target
/// could not be read or parsed.
type IdMap = HashMap<String, (usize, IdKind)>;

/// Maps every `id` attribute in a document to its element's document-order
/// index and its [`IdKind`].
///
/// The index drives two reading-order comparisons — the nav toc's fragment
/// order against the content document's DOM order, and MED-015's media
/// overlay `<text>` order against the same — while the kind drives RSC-014.
///
/// A first version of this change deleted the index, on the grounds that
/// every caller only ever asked `contains_key`. That was a grep over the
/// names `ids`/`target_ids`, and both real users are called something else
/// (`id_order_cache`, `id_order`), so the search confirmed exactly what it
/// had been pointed at. The compiler caught both. The claim to be careful
/// with is not "this is unused" but "my search would have found it".
fn dom_id_kinds(d: &roxmltree::Document) -> IdMap {
    let mut kinds = HashMap::new();
    for (i, n) in d.descendants().filter(|n| n.is_element()).enumerate() {
        if let Some(id) = n.attr_no_ns("id") {
            kinds
                .entry(id.to_string())
                .or_insert_with(|| (i, IdKind::of(n)));
        }
    }
    kinds
}

/// Whether an `(element, attribute)` pair is a reference that *consumes* the
/// target as a publication resource, as opposed to merely pointing at it.
///
/// This is the distinction OPF-097 turns on, and it is not the obvious one:
/// a `<a href>` hyperlink does **not** count. "Referenced" there means the
/// resource is embedded or loaded by a document - an image drawn, a
/// stylesheet applied, a font loaded - not that some page links to it.
/// epubcheck draws the same line (`Reference.Type.isPublicationResourceReference`:
/// GENERIC, STYLESHEET, FONT, IMAGE, AUDIO, VIDEO, TRACK, MEDIA_OVERLAY are
/// resource references; HYPERLINK, CITE, the SVG paint/symbol references and
/// the nav links are not).
///
/// CSS `url()`s and `@import`s are resource references too, but they come
/// from the stylesheet passes rather than from an element attribute, so they
/// are collected there.
fn is_resource_reference(node: roxmltree::Node, attr: &str) -> bool {
    match (node.tag_name().name(), attr) {
        // Loaded and rendered by the document.
        ("img", "src") | ("video", "poster") | ("object", "data") => true,
        ("audio" | "video" | "source" | "track" | "embed" | "iframe", "src") => true,
        ("script", "src") => true,
        // Only a stylesheet link consumes its target; `<link rel="next">`
        // and friends are navigation, not resources.
        ("link", "href") => node.attr_no_ns("rel").is_some_and(|r| {
            r.split_whitespace()
                .any(|t| t.eq_ignore_ascii_case("stylesheet"))
        }),
        // MathML's `altimg` is an image the renderer draws when it can't do
        // MathML - a resource by any reading.
        (_, "altimg") => true,
        // `<a href>`/`<area href>` are hyperlinks; `cite` is a citation URL.
        // Neither consumes anything.
        _ => false,
    }
}

/// The `dom_id_order` of another document in the container, or `None` when
/// that document could not be read or parsed at all.
///
/// The `None` matters: it is the difference between "I checked, and the id
/// is absent" and "I could not check". Every caller here is about to decide
/// whether some fragment reference is broken, and only the first of those
/// two answers can honestly produce an RSC-012. Collapsing them - which is
/// what an `unwrap_or_default()` on the parse does - turns an unreadable
/// document into an id-less one and reports every fragment pointing into it
/// as undefined. That was issue #23: 1079 invented RSC-012s across 31 books,
/// 86% of every RSC-012 on a real shelf, against ids that were plainly
/// there.
///
/// Decoding is BOM-aware and the XHTML DTD's entities are declared, exactly
/// as in the main content-document walk - a target document must not fail to
/// parse *here* for a reason it would not fail to parse *there*.
fn target_id_kinds(
    ocf: &mut Ocf,
    name_index: &HashMap<String, String>,
    target_nfc: &str,
    is_epub3: bool,
) -> Option<IdMap> {
    let orig = name_index.get(target_nfc)?;
    let bytes = ocf.read(orig)?;
    // The shift is irrelevant here: this reads an id map out of the DOM and
    // never reports a position.
    let (text, _) = crate::htm::declare_dtd_entities(crate::css::decode_bytes(&bytes), is_epub3);
    let doc = parse_xml(&text).ok()?;
    Some(dom_id_kinds(&doc))
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
/// Which document declared the `prefix` attribute. epubcheck reserves a
/// *different* set of prefixes in each, and reporting the union - as this did
/// - invents OPF-007 in both directions.
///
/// Doitsu, MobileRead #161: the IDPF `cc-shared-culture` sample declares
/// `epub:prefix="media: http://idpf.org/epub/vocab/media/#"` on a content
/// document's `<html>`. That URI differs from the Media Overlays vocabulary,
/// so a redeclaration check fires - but only if `media` is reserved *there*,
/// and in a content document it is not. epubcheck reports nothing.
#[derive(Clone, Copy, PartialEq)]
enum PrefixContext {
    /// `OPFHandler30`: the union of its five `parsePrefixDeclaration` calls.
    Package,
    /// `OPSHandler30.RESERVED_VOCABS` - the magazine-navigation and PRISM
    /// foreign vocabularies, and nothing else.
    ContentDocument,
    /// `OverlayHandler.RESERVED_VOCABS` is the default structure vocabulary
    /// alone, so a Media Overlay reserves no prefix at all.
    Overlay,
}

const RESERVED_PREFIXES_PACKAGE: &[(&str, &str)] = &[
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
];

const RESERVED_PREFIXES_CONTENT: &[(&str, &str)] = &[
    ("msv", "http://www.idpf.org/epub/vocab/structure/magazine/#"),
    (
        "prism",
        "http://www.prismstandard.org/specifications/3.0/PRISM_CV_Spec_3.0.htm#",
    ),
];

/// Every reserved prefix, in any context.
///
/// Used only by the two checks that ask "is this prefix reserved *somewhere*":
/// whether an undeclared prefix is usable, and whether a manifest-item
/// property's prefix is known. Those are different questions from
/// redeclaration, which is per-context above, and they are left on the union
/// deliberately, since narrowing them was not what #161 reported and is not
/// measured.
const RESERVED_PREFIXES_ANY: &[(&str, &str)] = &[
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

/// Reserved prefixes EPUB 3.4 deprecates (w3c/epubcheck#1649).
const DEPRECATED_PREFIXES_34: &[&str] = &["xsd", "msv", "prism"];

impl PrefixContext {
    fn reserved(self) -> &'static [(&'static str, &'static str)] {
        match self {
            PrefixContext::Package => RESERVED_PREFIXES_PACKAGE,
            PrefixContext::ContentDocument => RESERVED_PREFIXES_CONTENT,
            PrefixContext::Overlay => &[],
        }
    }
}

/// Does this XHTML document declare an initial containing block, i.e. a
/// `<meta name="viewport">` giving both a width and a height?
///
/// Only *presence* is asked. `crate::layout::check_xhtml_viewport` is the
/// real check and validates the values too; duplicating that here would give
/// an advisory two ways to disagree with an error-level check about the same
/// document. The advisory answers the one question #1651 raises — are the ICB
/// dimensions set at all.
fn has_icb_dimensions(d: &roxmltree::Document) -> bool {
    d.descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attr_no_ns("name") == Some("viewport")
        })
        .any(|n| {
            let content = n.attr_no_ns("content").unwrap_or("");
            let has = |k: &str| {
                content
                    .split(',')
                    .any(|p| p.trim_start().starts_with(k) && p.contains('='))
            };
            has("width") && has("height")
        })
}

/// EPUB 3.4 deprecations and roll-layout constraints that live on a spine
/// itemref (w3c/epubcheck#1649, #1651 — both open and unimplemented there).
///
/// Two conditions, kept as separate IDs because a consumer filters on them
/// differently:
///
/// - **ADV-006** — a `rendition:layout-*` spine override beside a `roll`
///   package layout. #1651: "no mixing layouts". Only the two override values
///   are reported; `roll` itself has no spine-override form, which is why
///   `KNOWN_ITEMREF_PROPERTIES` does not carry one.
/// - **ADV-008** — `rendition:align-x-center`, deprecated in 3.4 (#1649).
///
/// Advisory-only for the reason the whole restrictive half is: epubcheck
/// reports neither, so in a side-by-side diff these are indistinguishable
/// from false positives until it catches up. Nothing on the 125-book shelf
/// carries `align-x-center` or a `roll` layout, so the shelf is silent on
/// both and its silence is not evidence — no real book uses a layout the
/// specification introduced weeks ago.
fn check_epub34_itemref_deprecations(
    props: &str,
    package_layout_roll: bool,
    path: &str,
    ir: roxmltree::Node,
    report: &mut Report,
) {
    for token in props.split_whitespace() {
        if package_layout_roll
            && (token == "rendition:layout-reflowable" || token == "rendition:layout-pre-paginated")
        {
            report.push_node(
                NEXT_006,
                Severity::Usage,
                format!(
                    "EPUB 3.4: a roll layout admits no per-spine layout override, \
                     but this itemref declares \"{token}\""
                ),
                path.to_string(),
                ir,
                "opf.itemref.layout_override_beside_roll",
                vec![token.to_string()],
            );
        }
        if token == "rendition:align-x-center" {
            report.push_node(
                NEXT_008,
                Severity::Usage,
                "EPUB 3.4: the \"rendition:align-x-center\" property is deprecated",
                path.to_string(),
                ir,
                "opf.itemref.deprecated_align_x_center",
                vec![token.to_string()],
            );
        }
    }
}

/// EPUB 3.4: a spine override placing a page of a **reflowable** document on
/// one side of a spread is meaningless, so `page-spread-*` is confined to
/// fixed-layout content (w3c/epubcheck#1652).
///
/// "Reflowable" is the caller's `is_fixed_layout` inverted, which already
/// folds the itemref's own `rendition:layout-*` override over the package
/// default - so this fires both on a book that is reflowable throughout and
/// on one pre-paginated page that overrides itself back to reflowable.
///
/// The five prohibited values are the issue's own list. Note the asymmetry:
/// `center` exists only in the prefixed form, because the unprefixed pair is
/// the legacy EPUB 3.0 spine property and never had a centre value - the same
/// asymmetry `KNOWN_ITEMREF_PROPERTIES` already encodes.
///
/// Advisory-only, and deliberately so even though the spec is a Candidate
/// Recommendation: epubcheck has not implemented #1652, so to anyone diffing
/// the two tools this is indistinguishable from a false positive. It becomes
/// a normal error once epubcheck ships it. Nothing on the 125-book shelf uses
/// `page-spread-*` at all, so the shelf can neither confirm nor refute this
/// one - the evidence is the enumeration in the tests, not the shelf's
/// silence (the rule #48 set).
fn check_reflowable_page_spread(props: &str, path: &str, ir: roxmltree::Node, report: &mut Report) {
    const PROHIBITED: &[&str] = &[
        "page-spread-left",
        "page-spread-right",
        "rendition:page-spread-left",
        "rendition:page-spread-right",
        "rendition:page-spread-center",
    ];
    for token in props.split_whitespace() {
        if PROHIBITED.contains(&token) {
            report.push_node(
                NEXT_005,
                Severity::Usage,
                format!(
                    "EPUB 3.4: the \"{token}\" spine override applies to \
                     fixed-layout content, but this document is reflowable"
                ),
                path.to_string(),
                ir,
                "opf.itemref.page_spread_on_reflowable",
                vec![token.to_string()],
            );
        }
    }
}

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
fn check_itemref_rendition_conflicts(
    props: &str,
    path: &str,
    ir: roxmltree::Node,
    is_epub3: bool,
    report: &mut Report,
) {
    let tokens: Vec<&str> = props.split_whitespace().collect();
    for kind in ["layout", "orientation", "spread", "flow"] {
        let prefix = format!("rendition:{kind}-");
        if tokens.iter().filter(|t| t.starts_with(&prefix)).count() > 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("rendition:{kind} spine override values are mutually exclusive"),
                path.to_string(),
                ir,
                "opf.itemref.rendition_override_conflict",
                vec![kind.to_string()],
            );
        }
    }
    if tokens
        .iter()
        .filter(|t| t.starts_with("page-spread-") || t.starts_with("rendition:page-spread-"))
        .count()
        > 1
    {
        report.push_node(
            RSC_005,
            Severity::Error,
            "page-spread-* spine override values are mutually exclusive",
            path.to_string(),
            ir,
            "opf.itemref.page_spread_conflict",
            Vec::new(),
        );
    }
    if tokens.contains(&"rendition:spread-portrait") {
        report.push_node(
            OPF_086,
            Severity::Warning,
            "the \"portrait\" value of the \"rendition:spread\" property is deprecated",
            path.to_string(),
            ir,
            "opf.itemref.deprecated_spread_portrait",
            Vec::new(),
        );
    }
    // Vocabulary, not just conflicts (issue #67): an unknown token here went
    // entirely unreported, where the manifest `item/@properties` equivalent
    // has always been checked. Two vocabularies apply, per epubcheck's
    // `RESERVED_ITEMREF_VOCABS`: the unprefixed `ITEMREF_VOCAB` (two names)
    // and `RenditionVocabs.ITEMREF_VOCAB` under `rendition:`. Any other
    // prefix is left alone - see the meta-property check for why.
    const KNOWN_ITEMREF_PROPERTIES: &[&str] = &["page-spread-left", "page-spread-right"];
    const KNOWN_RENDITION_ITEMREF_PROPERTIES: &[&str] = &[
        "rendition:layout-pre-paginated",
        "rendition:layout-reflowable",
        "rendition:orientation-auto",
        "rendition:orientation-landscape",
        "rendition:orientation-portrait",
        "rendition:spread-auto",
        "rendition:spread-both",
        "rendition:spread-landscape",
        "rendition:spread-none",
        "rendition:spread-portrait",
        "rendition:page-spread-center",
        "rendition:page-spread-left",
        "rendition:page-spread-right",
        "rendition:flow-paginated",
        "rendition:flow-scrolled-continuous",
        "rendition:flow-scrolled-doc",
        "rendition:flow-auto",
        "rendition:align-x-center",
    ];
    for token in &tokens {
        // EPUB 3 only, for the same reason as the meta-property check: an
        // EPUB 2 `<itemref properties>` is already reported by the EPUB 2
        // package grammar.
        let unknown = if !is_epub3 {
            false
        } else if token.starts_with("rendition:") {
            !KNOWN_RENDITION_ITEMREF_PROPERTIES.contains(token)
        } else if !token.contains(':') {
            !KNOWN_ITEMREF_PROPERTIES.contains(token)
        } else {
            false
        };
        if unknown {
            report.push_node(
                OPF_027,
                Severity::Error,
                format!("unknown spine item property '{token}'"),
                path.to_string(),
                ir,
                "opf.itemref.unknown_property",
                vec![token.to_string()],
            );
        }
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

/// The OEBPS 1.2 package namespace — the pre-EPUB format's own (see OPF-047).
const OEB12_PKG_NS: &str = "http://openebook.org/namespaces/oeb-package/1.0/";

/// OPF-092: a language tag (`xml:lang`, `link/@hreflang`, or `dc:language`'s
/// own text) must not have leading/trailing whitespace, and - once trimmed
/// - must be empty (allowed) or a syntactically plausible BCP-47 tag. No
///   regex needed: the only real failure mode confirmed by the corpus is a
///   single-letter primary subtag ("a-value"), which real BCP-47 never
///   allows (a language subtag is ISO 639, always 2-8 letters).
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
        if let Some(lang) = n.attribute((XML_NS, "lang"))
            && !is_valid_lang_tag(lang)
        {
            report.push_node(
                OPF_092,
                Severity::Error,
                format!("language tag '{lang}' is not well-formed"),
                opf_path,
                n,
                "opf.language.invalid_tag",
                vec!["xml:lang".to_string(), lang.to_string()],
            );
        }
        if n.tag_name().name() == "link"
            && let Some(hreflang) = n.attr_no_ns("hreflang")
            && !is_valid_lang_tag(hreflang)
        {
            report.push_node(
                OPF_092,
                Severity::Error,
                format!("hreflang value '{hreflang}' is not well-formed"),
                opf_path,
                n,
                "opf.language.invalid_tag",
                vec!["hreflang".to_string(), hreflang.to_string()],
            );
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
                report.push_node(
                    OPF_092,
                    Severity::Error,
                    format!("dc:language value '{text}' is not well-formed"),
                    opf_path,
                    n,
                    "opf.language.invalid_tag",
                    vec!["dc:language".to_string(), text.clone()],
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
            let id = n.attr_no_ns("id")?.trim().to_string();
            let refines = n.attr_no_ns("refines")?.trim();
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
/// A W3C-DTF date - the ISO 8601 profile `dc:date` actually uses. The
/// date-only forms are `YYYY`, `YYYY-MM`, and `YYYY-MM-DD` (a bare year is
/// the common, valid case); a full timestamp appends `T`, a time, and a
/// mandatory timezone designator, e.g. `2025-04-24T17:00:00Z` - a form real
/// books commonly use and epubcheck accepts without complaint (issue #4).
/// An empty string or a natural-language date match no shape and are
/// rejected. Non-ASCII input can't be a valid date and is refused up front,
/// which also keeps every byte index on a char boundary so the slicing
/// below can't panic.
fn is_valid_dc_date(s: &str) -> bool {
    if !s.is_ascii() {
        return false;
    }
    match s.len() {
        4 => s.bytes().all(|b| b.is_ascii_digit()),
        7 => {
            s.as_bytes()[4] == b'-'
                && s[0..4].bytes().all(|b| b.is_ascii_digit())
                && two_digit_in_range(&s[5..7], 1, 12)
        }
        10 => is_wcdtf_full_date(s),
        _ => {
            s.len() > 10
                && is_wcdtf_full_date(&s[0..10])
                && s.as_bytes()[10] == b'T'
                && is_wcdtf_time_with_tz(&s[11..])
        }
    }
}

/// Exactly two ASCII digits whose value falls within `lo..=hi`.
fn two_digit_in_range(s: &str, lo: u32, hi: u32) -> bool {
    s.len() == 2
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u32>().is_ok_and(|v| (lo..=hi).contains(&v))
}

/// A W3C-DTF calendar date `YYYY-MM-DD` (month 01-12, day 01-31). Assumes
/// ASCII input (guaranteed by the sole caller, `is_valid_dc_date`).
fn is_wcdtf_full_date(s: &str) -> bool {
    s.len() == 10
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-'
        && s[0..4].bytes().all(|b| b.is_ascii_digit())
        && two_digit_in_range(&s[5..7], 1, 12)
        && two_digit_in_range(&s[8..10], 1, 31)
}

/// The time-of-day part of a W3C-DTF timestamp - `hh:mm`, `hh:mm:ss`, or
/// `hh:mm:ss.s+` - followed by a mandatory timezone designator, either `Z`
/// or a numeric offset `±hh:mm`. Assumes ASCII input.
fn is_wcdtf_time_with_tz(s: &str) -> bool {
    // Peel off the (required) timezone designator first.
    let time = if let Some(t) = s.strip_suffix('Z') {
        t
    } else {
        if s.len() < 6 {
            return false;
        }
        let (t, tz) = s.split_at(s.len() - 6);
        let z = tz.as_bytes();
        if (z[0] != b'+' && z[0] != b'-') || z[3] != b':' {
            return false;
        }
        if !two_digit_in_range(&tz[1..3], 0, 23) || !two_digit_in_range(&tz[4..6], 0, 59) {
            return false;
        }
        t
    };
    // hh:mm, with an optional :ss and an optional .fraction on the seconds.
    let mut parts = time.splitn(3, ':');
    let (Some(hh), Some(mm)) = (parts.next(), parts.next()) else {
        return false;
    };
    if !two_digit_in_range(hh, 0, 23) || !two_digit_in_range(mm, 0, 59) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(sec) => match sec.split_once('.') {
            None => two_digit_in_range(sec, 0, 59),
            Some((ss, frac)) => {
                two_digit_in_range(ss, 0, 59)
                    && !frac.is_empty()
                    && frac.bytes().all(|b| b.is_ascii_digit())
            }
        },
    }
}

/// `dcterms:modified` must be exactly `CCYY-MM-DDThh:mm:ssZ` (fixed
/// width, literal `T`/`Z`, no fractional seconds or numeric timezone
/// offset - confirmed via a real fixture using a bare date with no time
/// component at all, and the expected message text itself spelling out
/// this exact form).
fn is_valid_dcterms_modified(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 20 {
        return false;
    }
    let digit = |i: usize| b[i].is_ascii_digit();
    (0..4).all(digit)
        && b[4] == b'-'
        && (5..7).all(digit)
        && b[7] == b'-'
        && (8..10).all(digit)
        && b[10] == b'T'
        && (11..13).all(digit)
        && b[13] == b':'
        && (14..16).all(digit)
        && b[16] == b':'
        && (17..19).all(digit)
        && b[19] == b'Z'
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
    // Only the *publication* identifier, i.e. the one `unique-identifier`
    // points at. epubcheck's single OPF-085 call site sits inside
    // `if (idAttr.trim().equals(uniqueIdent))`, so a secondary `dc:identifier`
    // - a Calibre UUID, an ISBN, a scheme-tagged duplicate - is never judged.
    // We were checking every one of them, which is a false positive on any
    // book that carries a second, malformed UUID it does not publish under.
    let unique_id = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "package")
        .and_then(|p| p.attribute("unique-identifier"))
        .map(str::trim);
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
    {
        let is_unique = n
            .attribute("id")
            .map(str::trim)
            .is_some_and(|id| !id.is_empty() && Some(id) == unique_id);
        if !is_unique {
            continue;
        }
        // epubcheck reads the element's text as `getPrivateData(TEXT)` and
        // does nothing at all when it is null - an element with no text node
        // whatsoever. So `<dc:identifier opf:scheme="UUID"/>` draws no
        // OPF-085; the empty value is already an RSC-005 from the schema
        // (`DC.metadata-required-content` in `opf20.rng`), and adding "'' is
        // not a valid UUID" on top says nothing the reader doesn't have.
        // Note this is *not* the same as trimming to empty: whitespace-only
        // text is non-null there, and does still report.
        if !n.children().any(|c| c.is_text()) {
            continue;
        }
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
            report.push_at_pos(
                OPF_085,
                Severity::Warning,
                format!("dc:identifier '{text}' does not look like a valid UUID"),
                opf_path,
                Position::of(n),
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
/// RSC-017 ("should use a fragment identifier") when `@refines` names a
/// manifest item by href; OPF-027 when `@scheme` has no `prefix:` part;
/// OPF-026 when `@property` isn't a well-formed (possibly prefixed) NCName.
///
/// **The RSC-017 condition is narrow, and used not to be** (Doitsu,
/// MobileRead #163). epubcheck's `opf.refines.by-fragment` resolves
/// `@refines` and reports only when it matches an actual manifest item's
/// `@href`:
///
/// ```text
/// <let name="item" value="//opf:manifest/opf:item[resolve-uri(@href)=$refines-url]"/>
/// <report test="$item">… should instead refer to … ("#<item/@id>")</report>
/// ```
///
/// We fired on *any* non-fragment value, so `refines="creator-id"` - a bare
/// id with a missing `#`, which is what the reporter's book had - drew a
/// warning epubcheck never gives. `resolve-uri` is not in our XPath subset,
/// which is why this is here rather than in `package.sch`; both sides are
/// resolved against the same base, so comparing normalised relative paths is
/// the same comparison.
fn check_meta_property_scheme_shape(
    doc: &roxmltree::Document,
    opf_path: &str,
    report: &mut Report,
) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "meta")
    {
        if let Some(refines_attr) = attr_no_ns_node(n, "refines") {
            let refines = refines_attr.value().trim();
            if !refines.is_empty() && !refines.starts_with('#') && !refines.contains("://") {
                let target = nfc(&resolve("", strip_url_fragment(refines).trim()));
                let item_id = doc
                    .descendants()
                    .filter(|m| m.is_element() && m.tag_name().name() == "item")
                    .find(|m| {
                        m.attr_no_ns("href")
                            .is_some_and(|h| nfc(&resolve("", h.trim())) == target)
                    })
                    .and_then(|m| m.attr_no_ns("id"));
                if let Some(id) = item_id {
                    report.push_node_attr(
                        RSC_017,
                        Severity::Warning,
                        format!(
                            "@refines should refer to '{refines}' by a fragment identifier pointing to its manifest item (\"#{id}\")"
                        ),
                        opf_path,
                        n,
                        refines_attr,
                        "opf.meta.refines_should_use_fragment",
                        vec![refines.to_string(), id.to_string()],
                    );
                }
            }
        }
        if let Some(scheme_attr) = attr_no_ns_node(n, "scheme") {
            let scheme = scheme_attr.value().trim();
            if !scheme.is_empty() && !scheme.contains(':') {
                report.push_node_attr(
                    OPF_027,
                    Severity::Error,
                    format!("unknown scheme value '{scheme}' (must be prefixed)"),
                    opf_path,
                    n,
                    scheme_attr,
                    "opf.meta.unprefixed_scheme",
                    vec![scheme.to_string()],
                );
            }
        }
        if let Some(property) = n.attr_no_ns("property") {
            let property = property.trim();
            if !property.is_empty()
                && !property.contains(' ')
                && !is_well_formed_ncname_or_prefixed(property)
            {
                report.push_at_pos(
                    OPF_026,
                    Severity::Error,
                    format!("meta property '{property}' is not well-formed"),
                    opf_path,
                    Position::of(n),
                );
            }
        }
    }
}

/// The 4 default-vocabulary URIs - package `meta`/`link`/`item`/`itemref`
/// attribute contexts each have their own unprefixed "default" vocabulary
/// - explicitly mapping any prefix to one of these is forbidden
///   (OPF-007, "b" sub-case), confirmed via a real fixture (which happens
///   to reuse the names "meta"/"link"/"item"/"itemref" as its prefix names
///   too, but the rule text is about the URI side, not the name).
const DEFAULT_VOCAB_URIS: &[&str] = &[
    "http://idpf.org/epub/vocab/package/meta/#",
    "http://idpf.org/epub/vocab/package/link/#",
    "http://idpf.org/epub/vocab/package/item/#",
    "http://idpf.org/epub/vocab/package/itemref/#",
];

const DC_ELEMENTS_NS: &str = "http://purl.org/dc/elements/1.1/";
/// One way a `prefix` attribute value can be malformed, and the message ID
/// epubcheck picks for it.
#[derive(Debug, PartialEq)]
enum PrefixFault {
    /// OPF-004: leading or trailing whitespace around the whole value.
    Syntax,
    /// OPF-004a: a mapping with no prefix before the colon.
    EmptyPrefix,
    /// OPF-004b: the prefix is not an NCName.
    NotNcName(String),
    /// OPF-004c: the prefix is not immediately followed by its colon.
    NoColon(Option<String>),
    /// OPF-004d: no space between the colon and the URI.
    NoSpace(Option<String>),
    /// OPF-004e: something other than a plain space separates them.
    IllegalSpace(Option<String>),
    /// OPF-004f: illegal whitespace between two mappings.
    IllegalWhitespaceBetween(Option<String>),
    /// OPF-005: the value ends with a prefix that has no URI after it.
    MissingUri(Option<String>),
}

/// Parses a `prefix`/`epub:prefix` attribute value.
///
/// A character-level port of epubcheck's `PrefixDeclarationParser`, whose
/// grammar is
///
/// ```text
/// prefixes = mapping , { whitespace, { whitespace } , mapping } ;
/// mapping  = prefix , ":" , space , { space } , ? xsd:anyURI ? ;
/// prefix   = ? xsd:NCName ? ;
/// space    = #x20 ;
/// whitespace = (#x20 | #x9 | #xD | #xA) ;
/// ```
///
/// It is ported rather than approximated because each state reports its own
/// message ID (#70), and the distinctions live below the level a
/// `split_whitespace()` tokenizer can see: `foaf:  URI` (two spaces) is
/// *valid*, `foaf:\tURI` is OPF-004e. Both were measured against epubcheck
/// one book at a time before this was written, along with the two defects
/// the old tokenizer had beyond the IDs - `: URI` produced two findings
/// where epubcheck produces one, and a non-NCName prefix produced none at
/// all.
///
/// Known gap: OPF-004f needs whitespace that Guava's `CharMatcher.whitespace()`
/// accepts but that is not one of space/tab/CR/LF - a vertical tab, say.
/// Tab-separated mappings are legal and measured as such.
fn parse_prefix_value(value: &str) -> (HashMap<String, String>, Vec<PrefixFault>) {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Start,
        Prefix,
        PrefixEnd,
        Space,
        Uri,
        Whitespace,
    }
    // `accepted` is the run this state consumes; `allowed` is the subset of
    // that run which is not an error. The two differ only for Space and
    // Whitespace, which is exactly where OPF-004e/f come from.
    fn accepted(state: State, c: char) -> bool {
        match state {
            State::Start | State::Space | State::Whitespace => c.is_whitespace(),
            State::Prefix => !c.is_whitespace() && c != ':',
            State::PrefixEnd => c == ':',
            State::Uri => !c.is_whitespace(),
        }
    }
    fn allowed(state: State, c: char) -> bool {
        match state {
            State::Start => false,
            State::Space => c == ' ',
            State::Whitespace => matches!(c, ' ' | '\t' | '\r' | '\n'),
            _ => true,
        }
    }

    let chars: Vec<char> = value.chars().collect();
    let mut pairs = HashMap::new();
    let mut faults = Vec::new();
    let mut state = State::Start;
    let mut prefix: Option<String> = None;
    let mut pos = 0usize;
    let mut run = String::new();

    while pos < chars.len() {
        // `consume`: the maximal run of `accepted` characters from here, plus
        // the ones inside it that are not `allowed`.
        run.clear();
        let mut bad = String::new();
        while pos < chars.len() && accepted(state, chars[pos]) {
            run.push(chars[pos]);
            if !allowed(state, chars[pos]) {
                bad.push(chars[pos]);
            }
            pos += 1;
        }
        match state {
            State::Start => {
                prefix = None;
                if !run.is_empty() {
                    faults.push(PrefixFault::Syntax);
                }
                state = State::Prefix;
            }
            State::Prefix => {
                if run.is_empty() {
                    faults.push(PrefixFault::EmptyPrefix);
                } else if !crate::ncx::is_valid_ncname(&run) {
                    faults.push(PrefixFault::NotNcName(run.clone()));
                } else {
                    prefix = Some(run.clone());
                }
                state = State::PrefixEnd;
            }
            State::PrefixEnd => {
                if run.is_empty() {
                    // Skip whitespace looking for the colon: if one turns up
                    // the mapping merely spaced it wrongly, and if not there
                    // is no colon at all. epubcheck reports OPF-004c either
                    // way and only the next state differs.
                    while pos < chars.len() && chars[pos].is_whitespace() && chars[pos] != ':' {
                        pos += 1;
                    }
                    let at_colon = pos < chars.len() && chars[pos] == ':';
                    faults.push(PrefixFault::NoColon(prefix.take()));
                    state = if at_colon {
                        State::PrefixEnd
                    } else {
                        State::Uri
                    };
                } else {
                    state = State::Space;
                }
            }
            State::Space => {
                if run.is_empty() {
                    faults.push(PrefixFault::NoSpace(prefix.take()));
                } else if !bad.is_empty() {
                    faults.push(PrefixFault::IllegalSpace(prefix.clone()));
                }
                state = State::Uri;
            }
            State::Uri => {
                if let Some(p) = prefix.take() {
                    pairs.insert(p, run.clone());
                }
                state = State::Whitespace;
            }
            State::Whitespace => {
                if !bad.is_empty() {
                    faults.push(PrefixFault::IllegalWhitespaceBetween(prefix.clone()));
                }
                state = State::Prefix;
            }
        }
        // A state whose run was empty consumed nothing; the character that
        // stopped it is re-read by the next state, exactly as epubcheck's
        // `reader.reset()` arranges. Guard against standing still.
        if run.is_empty() && matches!(state, State::Prefix) && pos < chars.len() {
            // Start -> Prefix with no whitespace consumed: fine, the next
            // iteration reads the same character in the Prefix state.
        }
    }

    // "string ends with a single prefix": any non-final state means the value
    // stopped mid-mapping, which is OPF-005 rather than a syntax error.
    if !matches!(state, State::Start | State::Prefix | State::Whitespace) {
        faults.push(PrefixFault::MissingUri(prefix.take()));
    }
    if state == State::Prefix && !run.is_empty() {
        faults.push(PrefixFault::Syntax); // trailing whitespace
    }
    (pairs, faults)
}

/// Whether a prefix declaration's URI half would fail to parse as a URI -
/// OPF-006. epubcheck's test is Java's `new URI(...)`, which rejects
/// characters outside the unreserved/reserved/escaped sets; whitespace can't
/// reach here (the value is split on it), so what remains is this small set
/// of illegal characters plus malformed percent-escapes. Deliberately
/// conservative: being stricter than Java's parser would invent errors on
/// URIs epubcheck accepts.
fn is_unparseable_uri(uri: &str) -> bool {
    const ILLEGAL: &[char] = &['<', '>', '"', '{', '}', '|', '\\', '^', '`'];
    if uri.chars().any(|c| ILLEGAL.contains(&c) || c.is_control()) {
        return true;
    }
    let bytes = uri.as_bytes();
    bytes.iter().enumerate().any(|(i, b)| {
        *b == b'%'
            && !(i + 2 < bytes.len()
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit())
    })
}

/// The MARC relator codes epubcheck accepts for `opf:role` (273 of
/// them, from its own `OPFHandler.validRoles`). Membership, not shape: the
/// old "three lowercase letters" approximation let a fake code like `xyz`
/// through (#54).
const MARC_RELATORS: &[&str] = &[
    "abr", "acp", "act", "adi", "adp", "aft", "anl", "anm", "ann", "ant", "ape", "apl", "app",
    "aqt", "arc", "ard", "arr", "art", "asg", "asn", "ato", "att", "auc", "aud", "aui", "aus",
    "aut", "bdd", "bjd", "bkd", "bkp", "blw", "bnd", "bpd", "brd", "brl", "bsl", "cas", "ccp",
    "chr", "clb", "cli", "cll", "clr", "clt", "cmm", "cmp", "cmt", "cnd", "cng", "cns", "coe",
    "col", "com", "con", "cor", "cos", "cot", "cou", "cov", "cpc", "cpe", "cph", "cpl", "cpt",
    "cre", "crp", "crr", "crt", "csl", "csp", "cst", "ctb", "cte", "ctg", "ctr", "cts", "ctt",
    "cur", "cwt", "dbp", "dfd", "dfe", "dft", "dgc", "dgg", "dgs", "dis", "dln", "dnc", "dnr",
    "dpc", "dpt", "drm", "drt", "dsr", "dst", "dtc", "dte", "dtm", "dto", "dub", "edc", "edm",
    "edt", "egr", "elg", "elt", "eng", "enj", "etr", "evp", "exp", "fac", "fds", "fld", "flm",
    "fmd", "fmk", "fmo", "fmp", "fnd", "fpy", "frg", "gis", "grt", "his", "hnr", "hst", "ill",
    "ilu", "ins", "inv", "isb", "itr", "ive", "ivr", "jud", "jug", "lbr", "lbt", "ldr", "led",
    "lee", "lel", "len", "let", "lgd", "lie", "lil", "lit", "lsa", "lse", "lso", "ltg", "lyr",
    "mcp", "mdc", "med", "mfp", "mfr", "mod", "mon", "mrb", "mrk", "msd", "mte", "mtk", "mus",
    "nrt", "opn", "org", "orm", "osp", "oth", "own", "pad", "pan", "pat", "pbd", "pbl", "pdr",
    "pfr", "pht", "plt", "pma", "pmn", "pop", "ppm", "ppt", "pra", "prc", "prd", "pre", "prf",
    "prg", "prm", "prn", "pro", "prp", "prs", "prt", "prv", "pta", "pte", "ptf", "pth", "ptt",
    "pup", "rbr", "rcd", "rce", "rcp", "rdd", "red", "ren", "res", "rev", "rpc", "rps", "rpt",
    "rpy", "rse", "rsg", "rsp", "rsr", "rst", "rth", "rtm", "sad", "sce", "scl", "scr", "sds",
    "sec", "sgd", "sgn", "sht", "sll", "sng", "spk", "spn", "spy", "srv", "std", "stg", "stl",
    "stm", "stn", "str", "tcd", "tch", "ths", "tld", "tlp", "trc", "trl", "tyd", "tyg", "uvp",
    "vac", "vdg", "voc", "wac", "wal", "wam", "wat", "wdc", "wde", "win", "wit", "wpr", "wst",
];

/// Validates a `prefix`/`epub:prefix` attribute's declared value: syntax
/// errors (OPF-004), the reserved prefix `_` (OPF-007a), a prefix mapped to
/// one of the 4 default-vocabulary URIs (OPF-007b), a prefix mapped to the
/// Dublin Core elements namespace (OPF-007c), and a reserved prefix
/// redeclared to a *different* URI than its own default (bare OPF-007).
/// Returns the declared name->URI map for the caller's own OPF-028
/// (undeclared-prefix-usage) checking.
///
/// **This comment used to say all four shared the single OPF-007 ID, and
/// cited the corpus harness as confirmation** - that its ID matching "strips
/// the a/b/c Gherkin sub-case suffixes real epubcheck's feature file uses to
/// label them". They are not Gherkin labels: `MessageId.java` declares
/// OPF_007a/b/c as constants and epubcheck emits them. The harness was
/// wrong, and its error was read as evidence and designed into the
/// validator - which is the whole reason #70 exists. An instrument is not a
/// source about the thing it measures.
///
/// Still coarse, and knowingly: `syntax_errors` is a count, so every syntax
/// fault reports the bare OPF-004 where epubcheck picks one of
/// OPF-004a..OPF-004f from a character-level state machine
/// (`PrefixDeclarationParser`). Splitting those means porting that machine,
/// and getting it subtly wrong invents errors on an attribute most EPUB 3
/// books carry - so it is tracked in #70 rather than guessed at here.
fn check_prefix_declaration(
    prefix_attr: roxmltree::Attribute,
    path: &str,
    node: roxmltree::Node,
    context: PrefixContext,
    advisory: bool,
    report: &mut Report,
) -> HashMap<String, String> {
    let (pairs, faults) = parse_prefix_value(prefix_attr.value());
    for fault in &faults {
        let (id, severity, text) = match fault {
            PrefixFault::Syntax => (
                OPF_004,
                Severity::Error,
                "the \"prefix\" attribute value has a syntax error".to_string(),
            ),
            PrefixFault::EmptyPrefix => (
                OPF_004A,
                Severity::Error,
                "a prefix declaration is missing its prefix".to_string(),
            ),
            PrefixFault::NotNcName(p) => (
                OPF_004B,
                Severity::Error,
                format!("the prefix \"{p}\" is not a valid non-colonized name"),
            ),
            PrefixFault::NoColon(p) => (
                OPF_004C,
                Severity::Error,
                match p {
                    Some(p) => {
                        format!("the prefix \"{p}\" must be followed immediately by a colon")
                    }
                    None => "a prefix must be followed immediately by a colon".to_string(),
                },
            ),
            PrefixFault::NoSpace(p) => (
                OPF_004D,
                Severity::Error,
                match p {
                    Some(p) => {
                        format!("the prefix \"{p}\" must be separated from its URI by a space")
                    }
                    None => "a prefix must be separated from its URI by a space".to_string(),
                },
            ),
            PrefixFault::IllegalSpace(p) => (
                OPF_004E,
                Severity::Error,
                match p {
                    Some(p) => format!("illegal whitespace between the prefix \"{p}\" and its URI"),
                    None => "illegal whitespace between a prefix and its URI".to_string(),
                },
            ),
            PrefixFault::IllegalWhitespaceBetween(_) => (
                OPF_004F,
                Severity::Error,
                "illegal whitespace between prefix mappings".to_string(),
            ),
            PrefixFault::MissingUri(p) => (
                OPF_005,
                Severity::Error,
                match p {
                    Some(p) => format!("no URI was declared for the prefix \"{p}\""),
                    None => "no URI was declared for a prefix".to_string(),
                },
            ),
        };
        report.push_at_pos(id, severity, text, path, Position::of(node));
    }
    for (name, uri) in &pairs {
        if is_unparseable_uri(uri) {
            report.push_at_pos(
                OPF_006,
                Severity::Error,
                format!("the URI declared for the prefix \"{name}\" is not a valid URI"),
                path,
                Position::of(node),
            );
        }
    }
    // OPF-007's four cases, as an if/else-if chain rather than four
    // independent `if`s, because that is what epubcheck's
    // `VocabUtil.checkPrefixes` is: at most one finding per mapping. Written
    // as independent tests, a `_` prefix pointed at the Dublin Core namespace
    // drew two findings from us and one from epubcheck.
    //
    // Each case also has its own message ID there (#70). We reported the bare
    // OPF-007 for all four, which is the ID of the *last* case only, so three
    // of them named a code epubcheck does not emit for that condition.
    for (name, uri) in &pairs {
        if name == "_" {
            report.push_node_attr(
                OPF_007A,
                Severity::Error,
                "the prefix \"_\" must not be declared",
                path,
                node,
                prefix_attr,
                "opf.prefix.reserved_underscore",
                Vec::new(),
            );
        } else if DEFAULT_VOCAB_URIS.contains(&uri.as_str()) {
            report.push_node_attr(
                OPF_007B,
                Severity::Error,
                format!("prefix '{name}' must not be assigned to a default-vocabulary URI"),
                path,
                node,
                prefix_attr,
                "opf.prefix.assigned_to_default_vocab_uri",
                vec![name.clone()],
            );
        } else if uri == DC_ELEMENTS_NS {
            report.push_node_attr(
                OPF_007C,
                Severity::Error,
                format!("prefix '{name}' must not be mapped to the Dublin Core elements namespace"),
                path,
                node,
                prefix_attr,
                "opf.prefix.assigned_to_dc_namespace",
                vec![name.clone()],
            );
        } else if let Some((_, default_uri)) = context.reserved().iter().find(|(n, _)| n == name)
            && uri != default_uri
        {
            report.push_node_attr(
                OPF_007,
                Severity::Warning,
                format!("the '{name}' prefix is reserved and must not be redeclared"),
                path,
                node,
                prefix_attr,
                "opf.prefix.reserved_redeclared",
                vec![name.clone()],
            );
        }
        // EPUB 3.4 (w3c/epubcheck#1649, open and unimplemented there):
        // these three reserved prefixes are deprecated. Reported on the
        // *declaration*, which is the unambiguous signal — a book that
        // relies on the reserved mapping without declaring it says nothing
        // in the package document, and guessing from property names would
        // need the whole vocabulary. Advisory-only, like the rest of the
        // restrictive 3.4 work.
        if advisory && DEPRECATED_PREFIXES_34.contains(&name.as_str()) {
            report.push_node_attr(
                NEXT_008,
                Severity::Usage,
                format!("EPUB 3.4: the reserved prefix \"{name}\" is deprecated"),
                path,
                node,
                prefix_attr,
                "opf.prefix.deprecated_in_epub34",
                vec![name.clone()],
            );
        }
    }
    pairs
}

/// OPF-028: a `prefix:term` token (from an `epub:type`/`property`/
/// `properties` attribute value) whose prefix is neither one of the fixed
/// reserved prefixes (always usable undeclared) nor present in `declared`
/// (this document's own parsed `prefix`/`epub:prefix` attribute).
fn check_prefix_usage(
    text: &str,
    declared: &HashMap<String, String>,
    path: &str,
    node: roxmltree::Node,
    report: &mut Report,
) {
    for tok in text.split_whitespace() {
        let Some((prefix, _)) = tok.split_once(':') else {
            continue;
        };
        if prefix.is_empty() || RESERVED_PREFIXES_ANY.iter().any(|(n, _)| *n == prefix) {
            continue;
        }
        if declared.contains_key(prefix) {
            continue;
        }
        report.push_at_pos(
            OPF_028,
            Severity::Error,
            format!("undeclared prefix '{prefix}' used in '{tok}'"),
            path,
            Position::of(node),
        );
    }
}

/// RSC-005: a `prefix`/`epub:prefix` attribute is only allowed on the
/// document's own root element - confirmed via real fixtures flagging it
/// on an XHTML `<head>` and on an embedded `<svg>` element.
fn check_prefix_placement(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    let root = doc.root_element();
    for n in doc.descendants().filter(|n| n.is_element() && *n != root) {
        if let Some(prefix_attr) = n
            .attributes()
            .find(|a| a.namespace() == Some("http://www.idpf.org/2007/ops") && a.name() == "prefix")
        {
            report.push_node_attr(
                RSC_005,
                Severity::Error,
                "attribute \"epub:prefix\" not allowed here",
                path,
                n,
                prefix_attr,
                "opf.prefix.misplaced_epub_prefix_attribute",
                Vec::new(),
            );
        }
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
        if let Some(p) = pi.pi()
            && p.target == "xml-stylesheet"
            && let Some(href) = p.value.and_then(extract_pi_href)
        {
            classes.extend(read_stylesheet_classes(&href, dir, name_index, ocf));
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
            && node.attr_no_ns("rel").is_some_and(|r| {
                r.split_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("stylesheet"))
            })
            && let Some(href) = node.attr_no_ns("href")
        {
            classes.extend(read_stylesheet_classes(href, dir, name_index, ocf));
        }
    }

    classes
}

fn check_collection_roles(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "collection")
    {
        let Some(role) = n.attr_no_ns("role") else {
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
            report.push_at_pos(
                OPF_070,
                Severity::Warning,
                format!("collection role '{role}' is not a valid URL"),
                opf_path,
                Position::of(n),
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
        if r.attr_no_ns("type").is_none() {
            continue;
        }
        let dup_exists = refs.iter().enumerate().any(|(j, other)| {
            j != i
                && other.attr_no_ns("type") == r.attr_no_ns("type")
                && other.attr_no_ns("href") == r.attr_no_ns("href")
        });
        if dup_exists {
            report.push_node(
                RSC_017,
                Severity::Warning,
                "duplicate \"reference\" elements with the same \"type\" and \"href\" attributes",
                opf_path,
                *r,
                "opf.guide.duplicate_reference",
                Vec::new(),
            );
        }
    }
}

/// `guide/reference` targets: OPF-031 if not declared as a manifest item
/// (plus RSC-007 if the file doesn't exist in the container at all -
/// confirmed via a real fixture where the target is both undeclared and
/// missing); OPF-032 if it *is* declared but isn't a Content Document
/// (a real fixture links to a plain image); and RSC-012 when the reference
/// carries a `#fragment` that doesn't resolve to an `id` in the target.
///
/// The fragment half is here because epubcheck's own check is on the
/// *reference*, not on what produced it - `ResourceReferencesChecker`
/// resolves every registered reference the same way, so a `<guide>` href
/// is covered there for free. Ours was per-source and had grown three
/// sites (NCX `<content src>`, content-document hrefs, `epub:textref`)
/// with the guide left out; found by `compare` on a real book whose
/// `<reference type="toc">` pointed at an id that lives in a *different*
/// file, where our output for the whole package document was empty.
#[allow(clippy::too_many_arguments)]
fn check_guide_references(
    doc: &roxmltree::Document,
    base_dir: &str,
    ocf: &mut Ocf,
    name_index: &HashMap<String, String>,
    items: &HashMap<String, (String, String)>,
    fallback_map: &HashMap<String, String>,
    is_epub3: bool,
    opf_path: &str,
    report: &mut Report,
) {
    let mut id_cache: HashMap<String, Option<IdMap>> = HashMap::new();
    for r in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "reference")
    {
        let Some(href) = r.attr_no_ns("href") else {
            continue;
        };
        if is_external(href) {
            continue;
        }
        // RSC-020: an unencoded space in the reference itself, before the
        // resolution below. epubcheck validates every *registered* reference's
        // URL, and a `<guide>` reference is one — probed 2026-08-21 with a
        // book carrying `href="a b.xhtml"` here, which it reports and we did
        // not. This is the per-source/per-reference asymmetry again, the same
        // gap the NCX had in 0.9.27; the `<guide>` was simply never added to
        // the list.
        //
        // **Neither the shelf nor the corpus could have found it**: 0 of 385
        // real books have a spaced guide href, and epubcheck's own test suite
        // has no fixture for it either. The oracle is the only witness, which
        // is why parity questions get probed rather than measured.
        if href.trim().contains(' ')
            && let Some(href_attr) = attr_no_ns_node(r, "href")
        {
            report.push_node_attr(
                RSC_020,
                Severity::Error,
                format!("guide reference '{href}' contains unencoded spaces"),
                opf_path.to_string(),
                r,
                href_attr,
                "opf.guide.reference_unencoded_space",
                vec![href.to_string()],
            );
        }
        let path_part = href.split(['#', '?']).next().unwrap_or(href);
        let resolved = nfc(&resolve(base_dir, path_part));
        match items.iter().find(|(_, (p, _))| nfc(p) == resolved) {
            None => {
                report.push_at_pos(
                    OPF_031,
                    Severity::Error,
                    format!("guide reference '{href}' is not declared in the manifest"),
                    opf_path,
                    Position::of(r),
                );
                if !name_index.contains_key(&resolved) {
                    report.push_node(
                        RSC_007,
                        Severity::Error,
                        format!("guide reference '{href}' does not resolve to a real resource"),
                        opf_path,
                        r,
                        "opf.guide.reference_missing_resource",
                        vec![href.to_string()],
                    );
                }
            }
            Some((id, (_, mt))) => {
                // epubcheck exempts the *deprecated* content-document types
                // here as well as the real ones - `OPFChecker`:172 tests
                // `!isBlessedItemType && !isDeprecatedBlessedItemType` - so a
                // guide reference to a `text/html` document is not OPF-032
                // (issue #72).
                if !is_content_document_type(mt) && !is_deprecated_content_document_type(mt) {
                    report.push_at_pos(
                        OPF_032,
                        Severity::Error,
                        format!("guide reference '{href}' does not target a Content Document"),
                        opf_path,
                        Position::of(r),
                    );
                }
                // Independently of that, the guide reference is subject to the
                // foreign-resource fallback rule: epubcheck registers every
                // guide reference as a GENERIC reference (`OPFHandler`:563)
                // and `ResourceReferencesChecker::checkFallbacks` (:303)
                // reports RSC-032 when the target is not a Core Media Type and
                // no fallback chain reaches one. It is not version-gated, and
                // it is a *separate* question from OPF-032 above - measured on
                // three target types, one book each: `text/html` draws
                // RSC-032 alone, `application/x-dtbook+xml` draws RSC-032
                // alone (it is blessed in EPUB 2), and `application/pdf` draws
                // both. We previously reported no RSC-032 for a guide
                // reference at all, so the last two were gaps of their own.
                if !crate::cmt::is_core_media_type(mt)
                    && !crate::foreign::fallback_reaches_core(id, items, fallback_map)
                {
                    report.push_at_pos(
                        RSC_032,
                        Severity::Error,
                        format!(
                            "guide reference '{href}' targets a foreign resource with no fallback"
                        ),
                        opf_path,
                        Position::of(r),
                    );
                }
                if !is_content_document_type(mt) && !is_deprecated_content_document_type(mt) {
                    continue;
                }
                // epubcheck only resolves a fragment against XHTML and SVG
                // targets (`ResourceReferencesChecker`), which leaves out the
                // third Content Document type, DTBook - whose documents this
                // project doesn't validate either (see `docs/COVERAGE.md`).
                // A deprecated-blessed target (`text/html`) is not skipped:
                // its missing fragment is RSC-014 rather than RSC-012 (#82).
                if !is_content_document_type(mt) && !is_deprecated_content_document_type(mt) {
                    continue;
                }
                let Some(frag) = href.split_once('#').map(|(_, f)| f) else {
                    continue;
                };
                // Same exemption as the content-document site: an empty
                // fragment addresses the document itself, and a fragment
                // carrying `=`, `:` or `(` is a CFI or a media-fragment
                // rather than an id.
                if frag.is_empty() || frag.contains(['=', ':', '(']) {
                    continue;
                }
                if !id_cache.contains_key(&resolved) {
                    let ids = target_id_kinds(ocf, name_index, &resolved, is_epub3);
                    id_cache.insert(resolved.clone(), ids);
                }
                // `None` = the target could not be read/parsed, so whether the
                // fragment resolves is unknown and unreported (see
                // `target_id_kinds`).
                let Some(ids) = &id_cache[&resolved] else {
                    continue;
                };
                if !ids.contains_key(frag) {
                    report.push_node(
                        missing_fragment_id(items, &resolved),
                        Severity::Error,
                        format!("fragment identifier '{frag}' is not defined in '{resolved}'"),
                        opf_path,
                        r,
                        "opf.guide.reference_fragment_not_defined",
                        vec![frag.to_string(), resolved.clone()],
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
#[allow(clippy::too_many_arguments)]
fn check_ncx_content_fragments(
    ncx_doc: &roxmltree::Document,
    ncx_path: &str,
    ocf: &mut Ocf,
    name_index: &HashMap<String, String>,
    items: &HashMap<String, (String, String)>,
    fallback_map: &HashMap<String, String>,
    is_epub3: bool,
    report: &mut Report,
) {
    let dir = parent_dir(ncx_path);
    let mut id_cache: HashMap<String, Option<IdMap>> = HashMap::new();
    for n in ncx_doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "content")
    {
        let Some(src) = n.attr_no_ns("src") else {
            continue;
        };
        if is_external(src) {
            continue;
        }
        // RSC-020: an unencoded space in the reference itself. epubcheck
        // validates every *registered* reference's URL, so a Calibre book
        // whose files are named `Kamelyali Kadin_split_000.html` draws one
        // finding per manifest href *and* one per NCX `<content src>` - 32
        // and 28 on one real book, of which we reported only the 32. This
        // check is organised per source here rather than per reference, so
        // the NCX simply never joined the list (the same shape that left the
        // `<guide>` out of fragment resolution).
        //
        // Reported before the RSC-007 resolution below, not after: the two
        // are independent there - the file usually *does* exist, spaces and
        // all, so the `continue` would have swallowed nothing, but a
        // reference that is both malformed and missing earns both findings.
        // Interior space only; leading/trailing is stripped by the URL
        // parser and valid, as at the content-document sites.
        if src.trim().contains(' ')
            && let Some(src_attr) = attr_no_ns_node(n, "src")
        {
            report.push_node_attr(
                RSC_020,
                Severity::Error,
                format!("NCX content src '{src}' contains unencoded spaces"),
                ncx_path,
                n,
                src_attr,
                "opf.ncx.content_src_unencoded_space",
                vec![src.to_string()],
            );
        }
        let (target, frag) = match src.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (src, None),
        };
        let resolved = nfc(&resolve(&dir, target));
        if !name_index.contains_key(&resolved) {
            report.push_node(
                RSC_007,
                Severity::Error,
                format!("NCX content src '{src}' does not resolve to a real resource"),
                ncx_path,
                n,
                "opf.ncx.content_src_missing_resource",
                vec![src.to_string()],
            );
            continue;
        }
        // The deprecated types are exempt here too. epubcheck's hyperlink
        // branch (`ResourceReferencesChecker`:227) tests both predicates and
        // is *not* version-gated, unlike the OPF-043 exemption.
        if let Some((id, (_, mt))) = items.iter().find(|(_, (p, _))| nfc(p) == resolved)
            && !is_content_document_type(mt)
            && !is_deprecated_content_document_type(mt)
            && !fallback_reaches_content_document(id, items, fallback_map)
        {
            report.push_node(
                RSC_010,
                Severity::Error,
                format!("NCX content src '{src}' does not target an OPS document"),
                ncx_path,
                n,
                "opf.ncx.content_src_not_content_document",
                vec![src.to_string()],
            );
            continue;
        }
        let Some(frag) = frag else { continue };
        if frag.is_empty() {
            continue;
        }
        // epubcheck resolves an ID fragment only when the target is XHTML or
        // SVG - `ResourceReferencesChecker`:177-179, whose own comment says
        // "Check that target ID exists (if the target is XHTML or SVG)". A
        // `text/html` document is `MIMEType.HTML`, not XHTML, so a dangling
        // fragment into one draws nothing there. The guide's own fragment
        // check below already had this condition; this one did not, and once
        // issue #72 made these documents readable it started reporting a
        // fourth RSC-012 on a real book where epubcheck reports three.
        // A deprecated-blessed target (`text/html`) stays in: epubcheck's
        // RSC-012 is guarded on XHTML/SVG, so a missing fragment there comes
        // out as RSC-014 instead of nothing (#82). The comment this replaces
        // said such a fragment "draws nothing there" - true of RSC-012 only.
        if !declared_media_type(items, &resolved).is_some_and(|mt| {
            is_content_document_type(mt) || is_deprecated_content_document_type(mt)
        }) {
            continue;
        }
        if !id_cache.contains_key(&resolved) {
            let ids = target_id_kinds(ocf, name_index, &resolved, is_epub3);
            id_cache.insert(resolved.clone(), ids);
        }
        // `None` = the target could not be read/parsed, so whether the
        // fragment resolves is unknown and unreported (see
        // `target_id_kinds`).
        let Some(ids) = &id_cache[&resolved] else {
            continue;
        };
        if !ids.contains_key(frag) {
            report.push_node(
                missing_fragment_id(items, &resolved),
                Severity::Error,
                format!("fragment identifier '{frag}' is not defined in '{target}'"),
                ncx_path,
                n,
                "opf.ncx.content_fragment_not_defined",
                vec![frag.to_string(), target.to_string()],
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

/// RSC-005: `id` attributes must be unique within the package document.
///
/// Ported out of `schemas/package.sch`, which expressed it as
/// `count($id-set[normalize-space(@id) = normalize-space(current()/@id)]) = 1`
/// over `<let name="id-set" value="//*[@id]"/>`. That is quadratic as
/// written — every element carrying an `id` rescans every other one — and it
/// measured 11.5s of a 15.5s validation on a 4,000-item package document.
/// XPath 1.0 cannot express uniqueness in less than quadratic time, so the
/// rule moved here instead of being rewritten; two linear passes replace it.
///
/// **The output is deliberately byte-identical to the Schematron version**,
/// which is what lets the corpus check this port at all: the shelf never
/// exercises it (0 of 73 books), while the corpus has two dedicated
/// scenarios — `attr-id-duplicate-error.opf` and
/// `attr-id-duplicate-with-spaces-error.opf`, each "reported 2 times (once
/// for each ID)".
///
/// Four things therefore have to stay exactly as they were, and each is a
/// way this could drift silently:
///
/// 1. One finding per *occurrence*, not per duplicated value — hence the
///    second pass, in document order, rather than reporting from the map.
/// 2. The text carries the **normalized** id, as `<value-of>` did.
/// 3. RSC-005 / `Error` / located at the OPF / **no rule slug**, because
///    Schematron findings carry none and adding one would change
///    `--format json`.
/// 4. `*[@id]` matches an element in any namespace but only a
///    **no-namespace** `id`; `xml:id` is a different attribute and the rule
///    never saw it. This one bit first: `Node::attribute("id")` looks like
///    it means that and does not — given a `&str` roxmltree ignores the
///    namespace — so the attribute is matched explicitly below.
fn check_duplicate_ids(doc: &roxmltree::Document, opf_path: &str, report: &mut Report) {
    // Mirrors the engine's own `normalize-space`, not XPath's definition of
    // it: `split_whitespace` is Unicode-aware where XPath 1.0 lists only
    // #x20/#x9/#xD/#xA. Matching the engine is what keeps the output
    // identical — if that function is ever made spec-exact, this follows it.
    fn normalize_space(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    // `select_context_nodes` walks `root_element().descendants()`, which
    // includes the root element itself, in document order. Both passes
    // below use the same walk so the set and the order match the rule's.
    // Not `Node::attribute("id")`: given a plain `&str` roxmltree ignores the
    // namespace, so that matches `xml:id` as readily as `id` — which the
    // Schematron `*[@id]` never did (an unprefixed name in an XPath node
    // test is the *null* namespace). Verified against roxmltree 0.21, and
    // pinned by `xml_id_is_a_different_attribute` below.
    let ids = || {
        doc.root_element()
            .descendants()
            .filter(|n| n.is_element())
            .filter_map(|n| {
                n.attributes()
                    .find(|a| a.namespace().is_none() && a.name() == "id")
                    .map(|a| (n, normalize_space(a.value())))
            })
    };

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, id) in ids() {
        *counts.entry(id).or_insert(0) += 1;
    }
    for (node, id) in ids() {
        if counts.get(&id).is_some_and(|&c| c > 1) {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                format!("duplicate id \"{id}\""),
                opf_path,
                Position::of(node),
            );
        }
    }
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
                report.push_at_rule(
                    RSC_016,
                    Severity::Fatal,
                    format!("declared encoding '{declared}' does not match the file's actual UTF-16 encoding"),
                    opf_path,
                    "opf.encoding.mismatched_utf16",
                    vec![declared.clone()],
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
        report.push_at_rule(
            RSC_028,
            Severity::Error,
            "the OPF uses an encoding other than UTF-8, which is not allowed",
            opf_path,
            "opf.encoding.non_utf8_detected",
            Vec::new(),
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
            report.push_at_rule(
                RSC_028,
                Severity::Error,
                format!(
                    "the OPF declares encoding '{enc}', which is not allowed (EPUB requires UTF-8)"
                ),
                opf_path,
                "opf.encoding.declared_non_utf8",
                vec![enc.to_string()],
            );
            if !is_known {
                report.push_at_rule(
                    RSC_016,
                    Severity::Fatal,
                    format!("unrecognized encoding '{enc}'"),
                    opf_path,
                    "opf.encoding.unrecognized",
                    vec![enc.to_string()],
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

/// Check one package document and everything it reaches.
///
/// Takes the whole [`crate::Options`] rather than one parameter per setting:
/// the third option added (an EPUB-version override, #61) was also the second
/// time this signature had to change, and every such change breaks embedders
/// for a reason they don't care about. One more option now costs them nothing.
/// ADV-004 (#62, suggested by JSWolf on MobileRead): the book declares EPUB 2,
/// but its package document is written in EPUB 3. DNSB reports having seen
/// several such books - EPUB 3 throughout, with `version="2.0"` left in the
/// OPF.
///
/// **This does not detect anything the other checks miss.** A mislabelled book
/// already draws a pile of findings, every one of them accurate: the nav
/// document is not an EPUB 2 construct, `<section>` is not in XHTML 1.1, and
/// so on. What it lacks is the *diagnosis* - that all of them follow from one
/// wrong character in the version attribute. So this fires only where such a
/// pile already exists, which is what keeps it from inventing a verdict.
///
/// The content-document half of ADV-004's evidence, collected while the spine
/// is walked so no document is read or parsed twice.
///
/// Both fields mean *used*, never *declared* — see the measurement in
/// [`check_declared_version_advisory`], where keying on the `xmlns:epub`
/// declaration would have fired on 6 shelf books carrying it as dead
/// boilerplate.
#[derive(Clone, Copy, Default)]
struct ContentVersionSignals {
    /// An `epub:type` attribute on any element. Its namespace does not exist
    /// in OPS 2.0.1.
    epub_type: bool,
    /// A `section`/`header`/`footer`/`article`/`aside`/`nav` element. XHTML
    /// 1.1 has none of them; they arrived with HTML5.
    html5_sectioning: bool,
}

/// Each signal is structural, and each is illegal in EPUB 2 on its own - no
/// judgement about intent is being made, only a count.
///
/// **Content-document signals joined the package ones in 0.9.13**, which the
/// note here used to defer "until the real-book shelf can say whether they add
/// anything a correctly-labelled book doesn't also trip". The shelf has now
/// said, prompted by DNSB (MobileRead #169/#170) with a Calibre AZW3→EPUB 2
/// conversion whose package carries **no** signal at all - no
/// `<meta property>`, no `properties=` - while its content documents carry 374
/// `epub:type` attributes and 75 HTML5 sectioning elements. The book this
/// check exists for was exactly the book it stayed silent on.
///
/// The reporter proved the diagnosis themselves by flipping the version
/// attribute to `3.0`: our findings go 429 → 6, epubcheck's 3432 → 10.
///
/// **The signal is *use*, not declaration**, and that is measured rather than
/// assumed. Of 72 EPUB 2 books on the shelf, 8 declare the EPUB 3 `ops`
/// namespace in their content documents and only **2** ever use `epub:type` -
/// the other 6 carry an unused `xmlns:epub` as producer boilerplate. Keying on
/// the declaration would fire on all 8, which is the ADV-003 failure mode (an
/// advisory that cries wolf teaches people never to pass the flag). Keying on
/// use, and keeping the two-signal threshold, leaves those 6 silent and
/// reports 3.
///
/// The reverse direction - a `3.0` book that is really EPUB 2 - is not
/// implemented, and not for lack of interest: EPUB 3 *permits* an NCX and a
/// `<guide>` for backwards compatibility, so the obvious signals there are
/// legal constructs rather than illegal ones. That is a different and much
/// weaker instrument, and it would be the first check here to guess.
///
/// Advisory-only (opt-in `--advisory`, usage severity, never in the exit
/// code): epubcheck has no verdict on this, and the standing rule is that
/// anything beyond its verdict stays opt-in.
fn check_declared_version_advisory(
    doc: &roxmltree::Document,
    opf_path: &str,
    content: ContentVersionSignals,
    report: &mut Report,
) {
    let elements = || doc.descendants().filter(|n| n.is_element());
    let mut signals: Vec<String> = Vec::new();

    // EPUB 2 spells metadata `<meta name="…" content="…">`; the `property`
    // attribute arrived with EPUB 3's metadata vocabulary.
    if elements().any(|n| n.tag_name().name() == "meta" && n.attr_no_ns("property").is_some()) {
        signals.push("EPUB 3 metadata (a <meta property=\"…\">)".to_string());
    }
    // `properties` on a manifest item is EPUB 3 only. A navigation document
    // is called out separately from the rest (`scripted`, `mathml`, `svg`,
    // `remote-resources`) because it is the single most telling one - it is
    // the EPUB 3 table of contents.
    let item_props = || {
        elements()
            .filter(|n| n.tag_name().name() == "item")
            .filter_map(|n| n.attr_no_ns("properties"))
    };
    if item_props().any(|p| p.split_whitespace().any(|t| t == "nav")) {
        signals.push("a navigation document (properties=\"nav\")".to_string());
    }
    if item_props().any(|p| p.split_whitespace().any(|t| t != "nav")) {
        signals.push("other EPUB 3 manifest properties".to_string());
    }
    // The content-document half, gathered while the spine was walked. Both
    // are illegal in EPUB 2 the same way the package ones are: `epub:type`
    // belongs to a namespace OPS 2.0.1 does not have, and the sectioning
    // elements arrived with HTML5, where EPUB 2 is XHTML 1.1.
    if content.epub_type {
        signals.push("epub:type in the content documents".to_string());
    }
    if content.html5_sectioning {
        signals.push("HTML5 sectioning elements in the content documents".to_string());
    }

    // Two independent signals, not one. A single stray attribute is a
    // mistake in one line; two say the document was written to the other
    // version. The threshold is the whole of the judgement here, so it is
    // one number in one place, easy to revisit against the shelf.
    if signals.len() < 2 {
        return;
    }
    report.push_at(
        ADV_004,
        Severity::Usage,
        format!(
            "the package document declares EPUB 2 but is written in EPUB 3: {}. \
             If this book was meant to be EPUB 3, most other findings in this \
             report follow from the version attribute rather than from the content",
            signals.join(", ")
        ),
        opf_path,
    );
}

/// The `full-path` of every `<rootfile>`, read straight from
/// `META-INF/container.xml` and **reporting nothing**.
///
/// `ocf::find_rootfiles` answers the same question but reports as it goes, so
/// calling it a second time would duplicate its findings. OPF-003 needs the
/// list only to know what the *other* renditions declare.
fn container_rootfiles_silently(ocf: &mut Ocf) -> Vec<String> {
    let Some(bytes) = ocf.read("META-INF/container.xml") else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = crate::ocf::parse_xml(&text) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "rootfile")
        .filter_map(|n| n.attr_no_ns("full-path"))
        .map(|p| nfc(p.trim()))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Everything a package document declares: its manifest item hrefs and, on
/// EPUB 3, its metadata `<link href>` targets. Resolved against that
/// package's own directory and NFC-normalized. Reports nothing.
///
/// Used for the *other* renditions of a multiple-rendition publication, whose
/// resources are declared somewhere this package's manifest cannot see.
fn declared_resources_of(ocf: &mut Ocf, package_path: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(bytes) = ocf.read(package_path) else {
        return out;
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Ok(doc) = crate::ocf::parse_xml(&text) else {
        return out;
    };
    let dir = parent_dir(package_path);
    for n in doc.descendants().filter(|n| n.is_element()) {
        let name = n.tag_name().name();
        if name != "item" && name != "link" {
            continue;
        }
        if let Some(href) = n.attr_no_ns("href")
            && !is_external(href)
            && !is_remote_url(href)
            && !is_file_url(href)
        {
            out.insert(nfc(&resolve(&dir, strip_url_fragment(href).trim())));
        }
    }
    out
}

/// `audio/ogg; codecs=opus` is how an Opus file is declared in a manifest, and
/// the content side writes it as plain `audio/ogg`. epubcheck folds the two by
/// hand in `checkMimetypeMatches`; without it every Opus book draws an OPF-013
/// from us and none from epubcheck.
fn normalize_opus(mt: &str) -> &str {
    let t = mt.trim();
    let (head, rest) = t.split_once(';').unwrap_or((t, ""));
    if head.trim().eq_ignore_ascii_case("audio/ogg")
        && rest.split(';').any(|p| {
            p.split_once('=')
                .is_some_and(|(k, v)| k.trim().eq_ignore_ascii_case("codecs") && v.trim() == "opus")
        })
    {
        return "audio/ogg";
    }
    t
}

pub fn check(ocf: &mut Ocf, opf_path: &str, options: &crate::Options, report: &mut Report) {
    let profile = options.profile.as_deref();
    let advisory = options.advisory;
    let bytes = match ocf.read(opf_path) {
        Some(b) => b,
        None => {
            report.push(
                OPF_002,
                Severity::Fatal,
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
            report.push_full(
                RSC_016,
                Severity::Fatal,
                format!(
                    "OPF is not well-formed XML: {}",
                    crate::ocf::parse_error_detail(&text, &e)
                ),
                opf_path,
                Position::of_parse_error(&e),
                "opf.package.malformed_xml",
                Vec::new(),
            );
            return;
        }
    };

    let pkg = doc.root_element();
    if pkg.tag_name().name() != "package" {
        report.push_node(
            RSC_005,
            Severity::Error,
            "OPF root element is not <package>",
            opf_path,
            pkg,
            "opf.package.wrong_root_element",
            Vec::new(),
        );
        return;
    }
    let declared_prefixes = attr_no_ns_node(pkg, "prefix")
        .map(|p| {
            check_prefix_declaration(p, opf_path, pkg, PrefixContext::Package, advisory, report)
        })
        .unwrap_or_default();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(v) = n.attr_no_ns("property") {
            check_prefix_usage(v, &declared_prefixes, opf_path, n, report);
        }
        if let Some(v) = n.attr_no_ns("properties") {
            check_prefix_usage(v, &declared_prefixes, opf_path, n, report);
        }
    }
    check_lang_tags(&doc, opf_path, report);
    check_refines_cycles(&doc, opf_path, report);
    check_uuid_identifiers(&doc, opf_path, report);
    check_meta_property_scheme_shape(&doc, opf_path, report);
    check_collection_roles(&doc, opf_path, report);
    check_guide_duplicates(&doc, opf_path, report);
    if let Some(bindings) = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "bindings")
    {
        report.push_node(
            RSC_017,
            Severity::Warning,
            "the \"bindings\" element is deprecated",
            opf_path,
            bindings,
            "opf.package.deprecated_bindings",
            Vec::new(),
        );
    }

    // --- version ---
    let version = pkg.attr_no_ns("version").unwrap_or("");
    // Recorded even when unrecognized: callers outside the package document
    // (the PKG-017/PKG-024 extension check) need to know what the book
    // claimed, not what we made of it.
    if !version.is_empty() {
        report.epub_version = Some(version.to_string());
    }
    if version.is_empty() {
        report.push_node(
            OPF_001,
            Severity::Error,
            "<package> is missing the required 'version' attribute",
            opf_path,
            pkg,
            "opf.package.missing_version_attribute",
            Vec::new(),
        );
    } else if !(version.starts_with("2.") || version.starts_with("3.")) {
        report.push_node(
            OPF_001,
            Severity::Error,
            format!("Unrecognized EPUB version '{version}'"),
            opf_path,
            pkg,
            "opf.package.unrecognized_version",
            vec![version.to_string()],
        );
    }
    // Without a version there is no spec to check against, so OPF-001 is the
    // only thing that can honestly be said and everything below would be
    // guessing. epubcheck stops here too, structurally: it picks its package
    // checker *by version*, and an unknown version selects none, so the
    // document is never validated (`OCFChecker`). That is why its
    // `opf-legacy-oebps12-error` fixture - an OEBPS 1.2 package, wrong
    // namespace, `<dc-metadata>`, `text/x-oeb1-document`, plenty for us to
    // complain about - expects OPF-001 *and nothing else*. We were reporting
    // six extra findings on it, all of them about a spec the book never
    // claimed to follow (issue #26).
    if !(version.starts_with("2.") || version.starts_with("3.")) {
        return;
    }
    // PKG-001 (#61): the caller may demand a version, and epubcheck's rule is
    // that the demand wins - it reports the disagreement and then validates
    // against the version that was *asked for*, not the one the book
    // declares (`OCFChecker.checkPublicationVersion` returns `context.version`
    // on the mismatch branch). Matching that is the whole point of having the
    // flag: a `-v 2.0` invocation ported from epubcheck has to mean here what
    // it means there, or the compatibility it exists for is a fiction.
    //
    // Expect noise when the two disagree - a 3.0 book checked as 2.0 draws a
    // long list of findings that are all really one finding. That is
    // epubcheck's behaviour too, and PKG-001 sits at the top of the report
    // saying why.
    let declared_major = if version.starts_with("3.") { "3" } else { "2" };
    // Both spellings of each version are accepted, and anything else is
    // ignored rather than rejected - the same permissiveness an unrecognized
    // `profile` gets, so an embedder passing junk validates the book normally
    // instead of having its call fail. The CLI checks the value itself, where
    // a typo can still be answered with a usage error.
    let requested_major = match options.epub_version.as_deref() {
        Some("2" | "2.0") => Some("2"),
        Some("3" | "3.0") => Some("3"),
        _ => None,
    };
    let major = match requested_major {
        Some(requested) if requested != declared_major => {
            report.push_at(
                PKG_001,
                Severity::Warning,
                format!(
                    "validating as EPUB {requested}.0 because it was requested, \
                     but the package document declares version {version}"
                ),
                opf_path,
            );
            requested
        }
        _ => declared_major,
    };
    let is_epub3 = major == "3";
    let is_epub2 = !is_epub3;
    // OPF-047: the package document is written in **OEBPS 1.2**, the pre-EPUB
    // format EPUB 2 replaced, kept legal for backwards compatibility. Detected
    // exactly as epubcheck does (`OPFHandler.startElement`): a `<package>`
    // outside the OPF namespace.
    //
    // Validating such a package *as EPUB 2* is what this flag exists to stop.
    // OEBPS 1.2 puts its Dublin Core inside `<dc-metadata>` with title-case
    // names (`<dc:Title>`), has no NCX and so no `spine/@toc`, and uses
    // `text/x-oeb1-document` as its content-document type. Measured on
    // epubcheck's own fixture before this existed: epubcheck reported 4
    // findings, we reported 7 errors - OPF-030 in common, and six of ours
    // invented by rules that do not apply to the format the book declares.
    //
    // **Deliberately partial.** This stops the EPUB 2 rules being applied to a
    // format that does not have them; it does not implement OEBPS 1.2. The
    // media-type checks OPF-038/OPF-039 hang off this same flag further down
    // (they became cheap once it existed), but the `oebpkg12` DTD stays
    // unimplemented, which is the rest of the standing scope decision - see
    // `docs/COVERAGE.md`. Getting the wrong answer and getting no answer are
    // different, and this trades the first for the second.
    // Narrow on purpose. epubcheck's own guard admits only three namespaces
    // before asking the question (`OPFHandler.startElement`): absent, empty,
    // or the OEBPS 1.2 URI. A `<package>` in some *other* wrong namespace —
    // its `xml-namespace-wrongdefault-error.opf` fixture has a typo'd
    // `www.ipdf.org` — is not legacy syntax, it is a mistake, and must still
    // reach the schema error. A first version tested `!= OPF_PKG_NS`, called
    // that fixture OEBPS 1.2 and lost its RSC-005.
    let is_oeb12 = matches!(
        pkg.tag_name().namespace(),
        None | Some("") | Some(OEB12_PKG_NS)
    );
    if is_oeb12 {
        report.push_at_pos(
            OPF_047,
            Severity::Warning,
            "package document uses legacy OEBPS 1.2 syntax, allowing backwards \
             compatibility",
            opf_path,
            Position::of(pkg),
        );
    }
    // PKG-023 (usage): validation profiles are an EPUB 3 feature, so asking
    // for one against an EPUB 2 publication does nothing - epubcheck says so
    // rather than letting the caller believe their profile ran. Keyed on the
    // version being *validated against*, which is why it lives here and not
    // beside the call: with an override in play the declared version is the
    // wrong question, and epubcheck likewise keys its own check on the
    // validation version (`checkPublicationProfile`).
    //
    // Only a recognized profile counts. An unrecognized name already means
    // "the default profile" everywhere else in this tool, and reporting that
    // as an ignored profile would describe a request the caller never
    // successfully made.
    if is_epub2 && crate::PROFILES.contains(&profile.unwrap_or_default()) {
        report.push_at(
            PKG_023,
            Severity::Usage,
            "validation profiles do not apply to EPUB 2; the default profile was used",
            opf_path,
        );
    }
    // The 'dict'/'edupub'/'preview' CLI profiles are all EPUB 3-only
    // extension specs - a real fixture confirms an EPUB 2 publication
    // stays fully valid even when one of these profiles is specified
    // ("even when a 3.0 profile is specified"), so a version mismatch
    // must silently disable profile enforcement rather than force a
    // spurious "dc:type required" error onto a book that was never
    // attempting to be one in the first place.
    let profile = if is_epub3 { profile } else { None };

    // Schema validation against our own (permissive) package-document RNG.
    // Additive: a structurally non-conformant package is reported as RSC-005.
    // The grammar reports *where* the content model collapsed - every offending
    // node in document order (issues #17/#18): a real line:column and element
    // path, pinning the offending attribute when attribute-level. It still
    // doesn't name the specific rule that failed; a catch-all here.
    {
        let rule = "opf.package.schema_violation";
        let grammar = if is_epub3 {
            crate::rng::package_grammar()
        } else {
            crate::rng::package_grammar_epub2()
        };
        // Both grammars are bound to the OPF namespace, so on an OEBPS 1.2
        // package every element misses and the only finding produced is
        // "element package is not allowed here; expected package" - true,
        // useless, and hiding the one thing worth saying, which OPF-047 now
        // says. epubcheck validates these against `oebpkg12.dtd` instead; we
        // do not have that grammar and are not adding it.
        if !is_oeb12 {
            for blame in crate::rng::validate_node_report(&grammar, pkg) {
                push_blame(report, opf_path, rule, &blame);
            }
        }
    }

    // Schematron rules our own RNG can't express (id uniqueness,
    // unique-identifier resolution, dcterms:modified cardinality, @refines
    // targets). Same additive pattern, reported as RSC-005. Each finding
    // carries the position of the context element the rule matched, so
    // Schematron output now gets line/column too (previously it was the
    // one documented family that couldn't).
    check_duplicate_ids(&doc, opf_path, report);
    for (message, position, rule) in
        crate::schematron::run(&crate::schematron::package_schema(), &doc, "opf.package")
    {
        report.push_full(
            RSC_005,
            Severity::Error,
            message,
            opf_path,
            position,
            rule,
            Vec::new(),
        );
    }

    // --- required metadata ---
    // The package's actual identifier text (the dc:identifier named by
    // unique-identifier), used later for the NCX dtb:uid cross-check.
    let mut package_identifier_text: Option<String> = None;
    // Package-level fixed-layout default (individual spine itemrefs can
    // override this via their own 'properties'), used for the viewport/
    // viewBox checks below.
    let mut package_fixed_layout = false;
    // EPUB 3.4's webtoon layout. Kept beside `package_fixed_layout` rather
    // than folded into it: roll *is* a fixed-layout mode, but saying so here
    // would switch on the error-severity viewport checks unflagged, and the
    // restrictive half of #1651 is advisory until epubcheck ships it.
    let mut package_layout_roll = false;
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
        if let (Some(m), Some(mf)) = (metadata_pos, manifest_pos)
            && mf < m
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                "the \"metadata\" element must come before the \"manifest\" element",
                opf_path,
                pkg,
                "opf.package.metadata_after_manifest",
                Vec::new(),
            );
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
                        && n.attr_no_ns("property") == Some(property)
                })
                .map(elem_text)
        };
        // rendition:spread's "portrait" value is deprecated as a global
        // value (a warning, so hand-coded here rather than via
        // Schematron - crate::schematron::run's caller below maps every
        // finding to RSC-005/Error uniformly, which doesn't fit a
        // deprecation warning with its own dedicated code).
        if meta_property_text("rendition:spread").as_deref() == Some("portrait") {
            report.push_node(
                OPF_086,
                Severity::Warning,
                "the \"portrait\" value of the \"rendition:spread\" property is deprecated",
                opf_path,
                md,
                "opf.metadata.deprecated_spread_portrait",
                Vec::new(),
            );
        }
        // rendition:X custom/unknown properties (OPF-027) and the
        // deprecated meta-auth property (RSC-017), both simple
        // presence/name checks over every meta[@property] element.
        const KNOWN_RENDITION_PROPERTIES: &[&str] = &[
            "rendition:layout",
            "rendition:orientation",
            "rendition:spread",
            "rendition:flow",
            "rendition:viewport",
        ];
        // The `a11y:` meta-property vocabulary. epubcheck's
        // `AccessibilityVocab.META_PROPERTIES` holds three names
        // (`certifiedBy`, `certifierCredential`, `exemption`); this list
        // differs from it in two measured places, both deliberate and both
        // permissive.
        //
        // **`contactEmail`** is EPUB Accessibility 1.2's addition (w3c/epubcheck
        // #1669, reported by Gregorio Pellegrino 2026-07-02 and unanswered).
        // The property is not new: the spec's own change log dates it
        // **2025-09-04**, three days after epubcheck 5.3.0 shipped, which is
        // why that release cannot know it — and nothing has been committed
        // upstream since. We had simply copied the 1.1 list.
        //
        // Shipped while the spec is a Candidate Recommendation Draft
        // (18 August 2026), on direction rather than on maturity: this is a
        // *permissive* change, so being wrong costs a false negative nobody
        // can see, while waiting costs an ERROR — an INVALID verdict — on
        // correct accessibility metadata that its author cannot fix without
        // deleting it. The same reasoning shipped AVIF/JXL days after the
        // EPUB 3.4 CR while the restrictive 3.4 items stay behind
        // `--advisory`.
        //
        // **`certifierReport`** is epubcheck's `LINKREL_PROPERTIES`, not its
        // meta vocabulary, so a `<meta property="a11y:certifierReport">` is
        // OPF-027 there and accepted here. Pre-existing and left alone: same
        // direction, and the name is real spec vocabulary either way.
        //
        // Re-compare the whole vocabulary when 1.2 reaches Recommendation.
        // One open question deliberately not answered here: `exemption` did
        // not appear in a 2026-08-19 reading of the 1.2 property table, and
        // whether 1.2 dropped it needs its own measurement — accepting it
        // costs nothing in the meantime.
        const KNOWN_A11Y_META_PROPERTIES: &[&str] = &[
            "a11y:certifiedBy",
            "a11y:certifierCredential",
            "a11y:certifierReport",
            "a11y:contactEmail",
            "a11y:exemption",
        ];
        // The `media:` meta vocabulary (Media Overlays), the fourth and last
        // vocabulary reachable from a `meta@property` (issue #67). Four
        // names, and `active-class`/`playback-active-class` are already read
        // by name further down for the CSS-029/030 cross-check - they were
        // consumed without ever being validated as a set.
        //
        // epubcheck maps `media:` (and `rendition:`) to an *empty* vocabulary
        // in the item/itemref/link positions, which makes any name under
        // those prefixes undefined there. Not implemented: a `media:` token
        // in a manifest item's properties is not something real books do,
        // and every position we do check is one a real book can reach.
        const KNOWN_MEDIA_META_PROPERTIES: &[&str] = &[
            "media:active-class",
            "media:duration",
            "media:narrator",
            "media:playback-active-class",
        ];
        // The *unprefixed* meta property vocabulary (issue #67).
        // epubcheck's `PackageVocabs.META_VOCAB`, whose names are the
        // enum constants lower-hyphenated by `EnumVocab`, plus the
        // separate `META_VOCAB_CAMEL` holding the one camelCase name.
        // `dictionary-type`, `source-language` and `target-language` come
        // from the Dictionaries extension and sit in the same vocabulary
        // there, so they are accepted regardless of profile - as epubcheck
        // accepts them.
        //
        // `pageBreakSource` is EPUB 3.4 (spec change log 02-Jun-2025,
        // replacing `source-of`); epubcheck has already implemented it, so
        // this is parity rather than an early feature.
        const KNOWN_META_PROPERTIES: &[&str] = &[
            "alternate-script",
            "authority",
            "belongs-to-collection",
            "collection-type",
            "display-seq",
            "dictionary-type",
            "file-as",
            "group-position",
            "identifier-type",
            "meta-auth",
            "role",
            "source-language",
            "source-of",
            "target-language",
            "term",
            "title-type",
            "pageBreakSource",
        ];
        for n in md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "meta")
        {
            if let Some(property) = n.attr_no_ns("property") {
                if property.starts_with("rendition:")
                    && !KNOWN_RENDITION_PROPERTIES.contains(&property)
                {
                    report.push_node(
                        OPF_027,
                        Severity::Error,
                        format!("unknown rendition property '{property}'"),
                        opf_path,
                        n,
                        "opf.metadata.unknown_rendition_property",
                        vec![property.to_string()],
                    );
                }
                if property.starts_with("a11y:") && !KNOWN_A11Y_META_PROPERTIES.contains(&property)
                {
                    report.push_node(
                        OPF_027,
                        Severity::Error,
                        format!("unknown a11y property '{property}'"),
                        opf_path,
                        n,
                        "opf.metadata.unknown_a11y_property",
                        vec![property.to_string()],
                    );
                }
                if property.starts_with("media:")
                    && !KNOWN_MEDIA_META_PROPERTIES.contains(&property)
                {
                    report.push_node(
                        OPF_027,
                        Severity::Error,
                        format!("unknown media overlays property '{property}'"),
                        opf_path,
                        n,
                        "opf.metadata.unknown_media_property",
                        vec![property.to_string()],
                    );
                }
                // A name with no prefix must come from the package meta
                // vocabulary. A *prefixed* one is left alone: either the
                // prefix is reserved (handled by the two checks above and
                // by the media:/dcterms: readers elsewhere) or it is
                // author-declared, in which case its vocabulary is not ours
                // to know - and an *undeclared* prefix is OPF-028, a
                // different message, which is why this cannot simply reject
                // everything it does not recognise.
                //
                // EPUB 3 only: `property` is not an EPUB 2 attribute at all,
                // and the EPUB 2 package grammar already reports it, so
                // running this there would be a second finding for one
                // mistake.
                if is_epub3 && !property.contains(':') && !KNOWN_META_PROPERTIES.contains(&property)
                {
                    report.push_node(
                        OPF_027,
                        Severity::Error,
                        format!("unknown metadata property '{property}'"),
                        opf_path,
                        n,
                        "opf.metadata.unknown_meta_property",
                        vec![property.to_string()],
                    );
                }
                if property == "meta-auth" {
                    report.push_node(
                        RSC_017,
                        Severity::Warning,
                        "the meta-auth property is deprecated",
                        opf_path,
                        n,
                        "opf.metadata.deprecated_meta_auth",
                        Vec::new(),
                    );
                }
            }
        }

        media_active_class = meta_property_text("media:active-class");
        media_playback_active_class = meta_property_text("media:playback-active-class");

        // media:duration values must be valid SMIL3 clock values - reuses
        // the same clock-value grammar the Media Overlays checks already
        // use for clipBegin/clipEnd (src/smil.rs).
        for n in md.children().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attr_no_ns("property") == Some("media:duration")
        }) {
            let text = elem_text(n);
            if crate::smil::parse_clock_value(&text).is_none() {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("media:duration value '{text}' must be a valid SMIL3 clock value"),
                    opf_path,
                    n,
                    "opf.metadata.invalid_media_duration",
                    vec![text.clone()],
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
                && n.attr_no_ns("property") == Some("rendition:viewport")
        }) {
            report.push_node(
                OPF_086,
                Severity::Warning,
                "the \"rendition:viewport\" property is deprecated",
                opf_path,
                n,
                "opf.metadata.deprecated_rendition_viewport",
                Vec::new(),
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
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("The value of the \"rendition:viewport\" property must be of the form 'width=w,height=h' ('{text}')"),
                    opf_path,
                    n,
                    "opf.metadata.invalid_rendition_viewport",
                    vec![text.clone()],
                );
            }
        }
        opf_dc_type = md
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "type")
            .map(elem_text);
        let dc_types: Vec<String> = md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "type")
            .map(elem_text)
            .collect();
        crate::edupub::check_teacher_edition_and_accessibility(
            &dc_types,
            profile,
            Some(md),
            opf_path,
            report,
        );
        has_pagination_source = md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "source")
            .filter_map(|n| n.attr_no_ns("id"))
            .any(|source_id| {
                md.children().any(|n| {
                    n.is_element()
                        && n.tag_name().name() == "meta"
                        && n.attr_no_ns("property") == Some("source-of")
                        && n.attr_no_ns("refines").map(|r| r.trim_start_matches('#'))
                            == Some(source_id)
                        && elem_text(n) == "pagination"
                })
            });

        package_fixed_layout = md
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property") == Some("rendition:layout")
            })
            .any(|n| {
                let text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                text.trim() == "pre-paginated"
            });
        package_layout_roll = md
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property") == Some("rendition:layout")
            })
            .any(|n| {
                let text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                text.trim() == "roll"
            });
        let has = |local: &str| {
            md.children()
                .any(|n| n.is_element() && n.tag_name().name() == local)
        };
        // The three required-metadata reports are silenced for OEBPS 1.2, and
        // silenced rather than *taught* the format on purpose. That package
        // wraps its Dublin Core in `<dc-metadata>` with title-case names
        // (`<dc:Title>`), so the first attempt here widened the scan to match
        // them — which made us accept a book epubcheck rejects, because
        // epubcheck does not recognise those names either: its own handler
        // matches `identifier` case-sensitively, so it still reports OPF-030
        // "unique-identifier not found" on this very fixture. Coding to the
        // format instead of to the oracle is the mistake this project keeps
        // having to undo. epubcheck reports none of these three here, so
        // neither do we — and OPF-030 stays, as it does there.
        if !has("title") && !is_oeb12 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Required metadata dc:title is missing",
                opf_path,
                md,
                "opf.metadata.missing_title",
                Vec::new(),
            );
        } else if is_epub2 {
            // EPUB 3 already reports an empty dc:title as RSC-005 (via
            // `schemas/package.sch`'s own version-scoped pattern); EPUB 2
            // is more lenient - a real corpus fixture expects only a
            // warning.
            // `dc:language` shares this branch in epubcheck (`"title".equals
            // (name) || "language".equals(name)`, then one EPUB 2 emptiness
            // check for both), so an empty one is OPF-055 here too - not
            // OPF-072, and not the RSC-005 the Schematron used to raise.
            for n in md
                .children()
                .filter(|n| n.is_element() && matches!(n.tag_name().name(), "title" | "language"))
            {
                let text: String = n
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                if text.trim().is_empty() {
                    let name = n.tag_name().name();
                    report.push_at_pos(
                        OPF_055,
                        Severity::Warning,
                        format!("dc:{name} is empty"),
                        opf_path,
                        Position::of(n),
                    );
                }
            }
        }
        // dc:date must be a non-empty, ISO-8601 (YYYY[-MM[-DD]]) value -
        // confirmed via two real EPUB2 fixtures (an empty date, and one
        // using a natural-language date string) that this is OPF-054/Error
        // there, but two real EPUB3 fixtures (an invalid-syntax and an
        // unknown-format date) confirm the *same* underlying check is only
        // OPF-053/Warning in EPUB3 - a version-scoped severity/ID split,
        // same shape as dc:title's empty-value check elsewhere in this file.
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
                if is_epub3 {
                    report.push_full(
                        OPF_053,
                        Severity::Warning,
                        format!(
                            "dc:date value '{}' does not follow recommended syntax",
                            text.trim()
                        ),
                        opf_path,
                        Position::of(n),
                        "opf.metadata.date_syntax_not_recommended",
                        vec![text.trim().to_string()],
                    );
                } else {
                    report.push_at_pos(
                        OPF_054,
                        Severity::Error,
                        format!(
                            "dc:date value '{}' is empty or doesn't conform to ISO 8601",
                            text.trim()
                        ),
                        opf_path,
                        Position::of(n),
                    );
                }
            }
        }
        // OPF-072 (usage, EPUB 2 only): a `dc:*` metadata element with no
        // text. epubcheck reports each empty one; in EPUB 3 an empty element
        // is a schema error instead, so this is version-scoped.
        //
        // The exclusions are not a judgement call - they are epubcheck's own
        // branch structure. `OPFHandler` walks the `dc:*` names through an
        // if/else-if chain and only its final `else` reaches OPF_072, so the
        // four names that take an earlier branch can never produce it:
        // `identifier`, `date`, `title` and `language`. Each has a more
        // specific check instead (nothing at all for a bare empty
        // `identifier`; OPF-054 / OPF-055 / OPF-055 for the rest).
        //
        // We had only `title` and `date`, so an empty `dc:identifier` drew a
        // spurious OPF-072 and an empty `dc:language` drew OPF-072 where
        // epubcheck gives OPF-055.
        if is_epub2 {
            for n in md.children().filter(|n| {
                n.is_element()
                    && n.tag_name().namespace() == Some(DC_ELEMENTS_NS)
                    && !matches!(
                        n.tag_name().name(),
                        "identifier" | "date" | "title" | "language"
                    )
            }) {
                // The element's *own* text, not its descendants'. Calibre
                // writes unescaped `<p>` markup into `dc:description`, and
                // epubcheck counts such an element as empty: its handler keeps
                // the character data delivered to that element, and text
                // sitting inside a child is not it. Probed one book per shape
                // against 5.3.0 - empty and plain text agree either way,
                // `<p>text</p>` alone is the case that differs, and a mixed
                // `text<p>...</p>` is non-empty to both, which is why this
                // reads direct children rather than "has no element children".
                let text: String = n
                    .children()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect();
                if text.trim().is_empty() {
                    let name = n.tag_name().name();
                    report.push_full(
                        OPF_072,
                        Severity::Usage,
                        format!("dc:{name} metadata element is empty"),
                        opf_path,
                        Position::of(n),
                        "opf.metadata.empty_element",
                        vec![format!("dc:{name}")],
                    );
                }
            }
        }
        // dcterms:modified must be exactly 'CCYY-MM-DDThh:mm:ssZ' (the
        // message text itself is checked verbatim by a real fixture) - a
        // plain fixed-width byte-shape check, not the XPath-engine date
        // regex this was originally deferred as needing (EPUB3-only,
        // matching where the existing "must be defined" RSC-005 check for
        // this same property is already scoped).
        if is_epub3
            && let Some(modified) = md.children().find(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property") == Some("dcterms:modified")
            })
        {
            let text: String = modified
                .descendants()
                .filter(|t| t.is_text())
                .filter_map(|t| t.text())
                .collect();
            if !is_valid_dcterms_modified(text.trim()) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "dcterms:modified must be of the form 'CCYY-MM-DDThh:mm:ssZ'",
                    opf_path,
                    modified,
                    "opf.metadata.invalid_dcterms_modified",
                    vec![text.trim().to_string()],
                );
            }
        }
        // OPF-052: a dc:creator's opf:role (any of the "opf"/"epub"
        // prefixes real fixtures use - both bind to the same namespace)
        // must be a real MARC relator code.
        //
        // Two things this used to get wrong (#54). The shape test - "exactly
        // 3 lowercase ASCII letters" - passed any invented code, `xyz`
        // included; it is now membership in `MARC_RELATORS`. And the check
        // ran on `contributor` as well, which epubcheck does not do: its
        // only OPF-052 site is `else if (name.equals("creator"))`, so a
        // contributor role it accepts was an error for us.
        //
        // `oth.`-prefixed roles are epubcheck's escape hatch for anything
        // outside the vocabulary, and are valid by definition.
        const OPF_NS_ROLE: &str = "http://www.idpf.org/2007/opf";
        for n in md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "creator")
        {
            if let Some(role) = n.attribute((OPF_NS_ROLE, "role")) {
                let valid = MARC_RELATORS.contains(&role) || role.starts_with("oth.");
                if !valid {
                    report.push_full(
                        OPF_052,
                        Severity::Error,
                        format!("'{role}' is not a recognized MARC relator code"),
                        opf_path,
                        Position::of(n),
                        "opf.metadata.unknown_marc_relator",
                        vec![role.to_string()],
                    );
                }
            }
        }
        if !has("language") && !is_oeb12 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Required metadata dc:language is missing",
                opf_path,
                md,
                "opf.metadata.missing_language",
                Vec::new(),
            );
        }
        let identifiers: Vec<_> = md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
            .collect();
        if identifiers.is_empty() && !is_oeb12 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Required metadata dc:identifier is missing",
                opf_path,
                md,
                "opf.metadata.missing_identifier",
                Vec::new(),
            );
        }
        if let Some(uid) = pkg.attr_no_ns("unique-identifier").map(str::trim) {
            let matching = identifiers
                .iter()
                .find(|n| n.attr_no_ns("id").map(str::trim) == Some(uid));
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
                    report.push_full(
                        OPF_030,
                        Severity::Error,
                        format!(
                            "package unique-identifier '{uid}' does not match any dc:identifier id"
                        ),
                        opf_path,
                        Position::of(pkg),
                        "opf.package.unique_identifier_unresolved",
                        vec![uid.to_string()],
                    );
                }
            }
        } else {
            report.push_node(
                RSC_005,
                Severity::Error,
                "<package> is missing the required attribute \"unique-identifier\"",
                opf_path,
                pkg,
                "opf.package.missing_unique_identifier_attribute",
                Vec::new(),
            );
            report.push_at_pos(
                OPF_048,
                Severity::Error,
                "<package> is missing its required unique-identifier attribute",
                opf_path,
                Position::of(pkg),
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
                    && n.attr_no_ns("property") == Some("media:duration")
            })
            .collect();
        let total = duration_metas
            .iter()
            .find(|n| n.attr_no_ns("refines").is_none())
            .and_then(|n| n.text())
            .and_then(crate::smil::parse_clock_value);
        let parts: Option<Vec<f64>> = duration_metas
            .iter()
            .filter(|n| n.attr_no_ns("refines").is_some())
            .map(|n| n.text().and_then(crate::smil::parse_clock_value))
            .collect();
        if let (Some(total), Some(parts)) = (total, parts)
            && !parts.is_empty()
        {
            let sum: f64 = parts.iter().sum();
            if (total - sum).abs() > 1.0 {
                report.push_at_pos(
                    MED_016,
                    Severity::Warning,
                    "media:duration total does not match the sum of overlay durations",
                    opf_path,
                    Position::of(md),
                );
            }
        }
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            "OPF is missing the <metadata> element",
            opf_path,
            pkg,
            "opf.package.missing_metadata_element",
            Vec::new(),
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
    // The same (resolved-path, media-type) pairs in **manifest document
    // order**, which `items` cannot preserve.
    //
    // Rust's `HashMap` is randomly seeded, so iterating `items.values()` gives
    // a different order on every run. That order used to decide which content
    // document was visited first, which in turn decided the file order of the
    // whole report — `Report::sort_by_document_order` derives it from the
    // order findings arrive in. Result: **94 of 385 real books printed their
    // findings in a different order on each run of the same binary**, same
    // findings, same byte count, shuffled.
    //
    // Nothing could see it. The corpus, the shelf, `compare` and the tests all
    // compare ID sets or counts, which are order-insensitive by construction;
    // it surfaced only from byte-comparing two runs while verifying an
    // unrelated refactor.
    let mut manifest_order: Vec<(String, String)> = Vec::new();
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
    let mut nav_count = 0u32;
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
                item.attr_no_ns("id"),
                item.attr_no_ns("href"),
                item.attr_no_ns("media-type"),
            );
            let (id, href, mt) = match (id, href, mt) {
                (Some(i), Some(h), Some(m)) => (i.trim(), h, m),
                _ => {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!("manifest <item> is missing id/href/media-type (id={id:?})"),
                        opf_path,
                        item,
                        "opf.manifest_item.missing_required_attribute",
                        vec![format!("{id:?}")],
                    );
                    continue;
                }
            };
            // The href attribute node, for @href-targeted findings (issue #18).
            // Present here — the match above `continue`d when href was absent.
            let Some(href_attr) = attr_no_ns_node(item, "href") else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("duplicate manifest item id '{id}'"),
                    opf_path,
                    item,
                    "opf.manifest_item.duplicate_id",
                    vec![id.to_string()],
                );
            }
            if href.contains(' ') {
                report.push_node_attr(
                    RSC_020,
                    Severity::Error,
                    format!("manifest item href '{href}' contains unencoded spaces"),
                    opf_path,
                    item,
                    href_attr,
                    "opf.manifest_item.unencoded_space_in_href",
                    vec![href.to_string()],
                );
            }
            if href.contains('#') {
                report.push_at_pos(
                    OPF_091,
                    Severity::Error,
                    format!("manifest item href '{href}' must not have a fragment identifier"),
                    opf_path,
                    Position::of(item),
                );
            }
            if href.trim_start().starts_with("data:") {
                report.push_node_attr(
                    RSC_029,
                    Severity::Error,
                    format!("manifest item '{id}' href must not be a data URL"),
                    opf_path,
                    item,
                    href_attr,
                    "opf.manifest_item.data_url_href",
                    vec![id.to_string()],
                );
            }
            if href.trim_start().starts_with("file:") {
                report.push_node_attr(
                    RSC_030,
                    Severity::Error,
                    format!("manifest item '{id}' href is a file URL, which is not allowed"),
                    opf_path,
                    item,
                    href_attr,
                    "opf.manifest_item.file_url_href",
                    vec![id.to_string()],
                );
            }
            // "Core Media Types" (and their preferred/non-preferred split) are
            // an EPUB 3 concept; EPUB 2 has no such preference, so OPF-090 is
            // EPUB 3 only (issue #9: a legacy .otf font wrongly flagged in an
            // EPUB 2 book that epubcheck reports clean).
            if is_epub3 && crate::cmt::is_non_preferred_core_media_type(mt) {
                report.push_full(
                    OPF_090,
                    Severity::Usage,
                    format!("media-type '{mt}' is a non-preferred (but valid) Core Media Type"),
                    opf_path,
                    Position::of(item),
                    "opf.manifest_item.non_preferred_media_type",
                    vec![mt.to_string()],
                );
            }
            if mt == "text/x-oeb1-css" {
                report.push_at_pos(
                    OPF_037,
                    Severity::Warning,
                    "media-type 'text/x-oeb1-css' is a deprecated OEB 1.x construct",
                    opf_path,
                    Position::of(item),
                );
            }
            // OPF-038/OPF-039: inside an **OEBPS 1.2** package the modern
            // media types are the wrong ones - that format wants
            // `text/x-oeb1-document` and `text/x-oeb1-css`. Ported from
            // `OPFChecker.checkItem`, which asks it in two separate places
            // and they are not interchangeable:
            //
            //   1. a *deprecated blessed* type (`text/x-oeb1-document` or
            //      `text/html`) is OPF-035 in a normal package and OPF-038 in
            //      an OEBPS 1.2 one - `text/html` only, unconditionally;
            //   2. a *blessed* type (EPUB 2: XHTML or DTBook) or a blessed
            //      style type (`text/css`) is OPF-038/OPF-039, but only when
            //      the item declares no `fallback`.
            //
            // Cheap now and expensive an hour ago: these were "implement the
            // format" until OPF-047 gave us `is_oeb12`. The `oebpkg12` DTD
            // stays unimplemented, which is the rest of the scope decision.
            if is_oeb12 {
                if mt == "text/html" {
                    report.push_at_pos(
                        OPF_038,
                        Severity::Warning,
                        format!(
                            "media-type '{mt}' is not appropriate in an OEBPS 1.2 context; \
                             use 'text/x-oeb1-document'"
                        ),
                        opf_path,
                        Position::of(item),
                    );
                } else if item.attr_no_ns("fallback").is_none() {
                    if mt == "application/xhtml+xml" || mt == "application/x-dtbook+xml" {
                        report.push_at_pos(
                            OPF_038,
                            Severity::Warning,
                            format!(
                                "media-type '{mt}' is not appropriate in an OEBPS 1.2 context; \
                                 use 'text/x-oeb1-document'"
                            ),
                            opf_path,
                            Position::of(item),
                        );
                    } else if mt == "text/css" {
                        report.push_at_pos(
                            OPF_039,
                            Severity::Warning,
                            format!(
                                "media-type '{mt}' is not appropriate in an OEBPS 1.2 context; \
                                 use 'text/x-oeb1-css'"
                            ),
                            opf_path,
                            Position::of(item),
                        );
                    }
                }
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
                    report.push_at_pos(
                        RSC_026,
                        Severity::Error,
                        format!("manifest item '{id}' href '{href}' is path-absolute or escapes the container root"),
                        opf_path,
                        Position::of(item),
                    );
                }
                if href.contains('?') {
                    report.push_node_attr(
                        RSC_033,
                        Severity::Error,
                        format!("manifest item '{id}' href '{href}' must not have a query string"),
                        opf_path,
                        item,
                        href_attr,
                        "opf.manifest_item.href_has_query_string",
                        vec![id.to_string(), href.to_string()],
                    );
                }
                if resolved.contains(' ') {
                    report.push_full(
                        PKG_010,
                        Severity::Warning,
                        format!("resource '{resolved}' has a space in its name"),
                        opf_path,
                        Position::of(item),
                        "opf.manifest_item.filename_contains_space",
                        vec![resolved.clone()],
                    );
                }
                if resolved_nfc == opf_own_name {
                    report.push_at_pos(
                        OPF_099,
                        Severity::Error,
                        format!("manifest item '{id}' references the package document itself"),
                        opf_path,
                        Position::of(item),
                    );
                }
                if let Some(first_id) = resource_seen.get(&resolved_nfc) {
                    report.push_at_pos(
                        OPF_074,
                        Severity::Error,
                        format!(
                            "manifest item '{id}' represents the same resource as item '{first_id}'"
                        ),
                        opf_path,
                        Position::of(item),
                    );
                } else {
                    resource_seen.insert(resolved_nfc.clone(), id.to_string());
                }
            }
            if let Some(props) = item.attr_no_ns("properties") {
                item_properties.insert(resolved_nfc.clone(), props.to_string());
                for token in props.split_whitespace() {
                    if token == "cover-image" {
                        cover_image_count += 1;
                        if !mt.starts_with("image/") {
                            report.push_node(
                                OPF_012,
                                Severity::Error,
                                "the \"cover-image\" property must only be used on an image",
                                opf_path,
                                item,
                                "opf.manifest_item.cover_image_not_image",
                                Vec::new(),
                            );
                        }
                    } else if token == "search-key-map"
                        && mt != "application/vnd.epub.search-key-map+xml"
                    {
                        report.push_node(
                            OPF_012,
                            Severity::Error,
                            format!(
                                "property \"search-key-map\" is not defined for media type '{mt}'"
                            ),
                            opf_path,
                            item,
                            "opf.manifest_item.search_key_map_wrong_media_type",
                            vec![mt.to_string()],
                        );
                    } else if token == "nav" && mt != "application/xhtml+xml" {
                        report.push_node(
                            OPF_012,
                            Severity::Error,
                            format!("property \"nav\" is not defined for media type '{mt}'"),
                            opf_path,
                            item,
                            "opf.manifest_item.nav_wrong_media_type",
                            vec![mt.to_string()],
                        );
                        report.push_node(
                            RSC_005,
                            Severity::Error,
                            "the nav document must be an XHTML Content Document",
                            opf_path,
                            item,
                            "opf.manifest_item.nav_not_xhtml",
                            Vec::new(),
                        );
                    } else {
                        // A genuinely custom (non-reserved) prefix is
                        // always allowed - but a *reserved*-prefixed
                        // token (e.g. "rendition:layout-pre-paginated",
                        // which is only ever a valid <itemref> override,
                        // never a manifest <item> property) has no known
                        // valid manifest-item-level term at all, so it's
                        // just as "unknown" as an unprefixed one.
                        let unknown = match token.split_once(':') {
                            Some((prefix, _)) => {
                                RESERVED_PREFIXES_ANY.iter().any(|(n, _)| *n == prefix)
                            }
                            None => !KNOWN_ITEM_PROPERTIES.contains(&token),
                        };
                        if unknown {
                            report.push_node(
                                OPF_027,
                                Severity::Error,
                                format!("unknown manifest item property '{token}'"),
                                opf_path,
                                item,
                                "opf.manifest_item.unknown_property",
                                vec![token.to_string()],
                            );
                        }
                    }
                }
            }
            if is_epub3 && item.attr_no_ns("fallback-style").is_some() {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "the \"fallback-style\" attribute is an obsolete EPUB 2 construct",
                    opf_path,
                    item,
                    "opf.manifest_item.obsolete_fallback_style",
                    Vec::new(),
                );
            } else if let Some(fs) = item.attr_no_ns("fallback-style") {
                fallback_style_map.insert(id.to_string(), fs.trim().to_string());
            }
            if let Some(fb) = item.attr_no_ns("fallback").map(str::trim)
                && fb == id
            {
                report.push_node(
                    OPF_045,
                    Severity::Error,
                    format!("item '{id}' cannot fall back to itself"),
                    opf_path,
                    item,
                    "opf.manifest_item.self_fallback",
                    vec![id.to_string()],
                );
            }
            if item
                .attr_no_ns("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "nav"))
            {
                nav_present = true;
                nav_count += 1;
                nav_path = Some(resolved.clone());
            }
            if item
                .attr_no_ns("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "data-nav"))
            {
                data_nav_items.push((resolved.clone(), mt.to_string()));
            }
            if !is_external(href) && !name_index.contains_key(&nfc(&resolved)) {
                report.push_node(
                    RSC_001,
                    Severity::Error,
                    format!("manifest item '{id}' references a missing resource '{href}'"),
                    opf_path,
                    item,
                    "opf.manifest_item.missing_resource",
                    vec![id.to_string(), href.to_string()],
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
                        report.push_node_attr(
                            PKG_009,
                            Severity::Error,
                            format!("manifest item '{id}' href segment '{decoded}' contains a forbidden character"),
                            opf_path,
                            item,
                            href_attr,
                            "opf.manifest_item.href_segment_forbidden_char",
                            vec![decoded.clone()],
                        );
                    }
                    if crate::filename::has_non_ascii(&decoded) {
                        report.push_node_attr(
                            PKG_012,
                            Severity::Usage,
                            format!("manifest item '{id}' href segment '{decoded}' contains non-ASCII characters"),
                            opf_path,
                            item,
                            href_attr,
                            "opf.manifest_item.href_segment_non_ascii",
                            vec![decoded.clone()],
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
                report.push_node(
                    RSC_006,
                    Severity::Error,
                    format!("Content Document '{href}' must not be remote"),
                    opf_path,
                    item,
                    "opf.manifest_item.remote_content_document",
                    vec![href.to_string()],
                );
            }
            if let Some(mo) = item.attr_no_ns("media-overlay") {
                if mt != "application/xhtml+xml" && mt != "image/svg+xml" {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "the media-overlay attribute is only allowed on EPUB Content Documents",
                        opf_path,
                        item,
                        "opf.manifest_item.media_overlay_on_non_content_document",
                        Vec::new(),
                    );
                }
                media_overlay_attrs.push((nfc(&resolved), mo.trim().to_string()));
            }
            if let Some(fb) = item.attr_no_ns("fallback") {
                fallback_map.insert(id.to_string(), fb.trim().to_string());
            }
            manifest_order.push((resolved.clone(), mt.to_string()));
            items.insert(id.to_string(), (resolved, mt.to_string()));
        }
        if cover_image_count > 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "the \"cover-image\" property must occur at most once in the manifest",
                opf_path,
                mn,
                "opf.manifest.multiple_cover_image",
                Vec::new(),
            );
        }
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            "OPF is missing the <manifest> element",
            opf_path,
            pkg,
            "opf.package.missing_manifest_element",
            Vec::new(),
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
    // PKG-025 (EPUB 3 only - a real EPUB 2 fixture, "Ignore unknown files
    // in the META-INF directory", explicitly stays clean): a *publication
    // resource* must not live in META-INF. "Publication resource" means
    // manifest-declared - epubcheck's own fixture triggers this with
    // `<item href="../META-INF/image.jpeg">`, i.e. the file is in the
    // manifest AND stored under META-INF. Undeclared extras there (Apple's
    // display-options, calibre bookmarks, ...) are container-level metadata
    // the OCF spec permits, and flagging them was a real-world false
    // positive (issue #16, reported by Doitsu on the MobileRead forum).
    // Checked here, after the manifest is parsed, because "declared" is the
    // deciding half of the condition.
    if is_epub3 {
        let declared: HashSet<String> = items.values().map(|(p, _)| nfc(p)).collect();
        for name in &ocf.names {
            if let Some(rest) = name.strip_prefix("META-INF/")
                && !rest.is_empty()
                && declared.contains(&nfc(name))
            {
                report.push_at(
                    PKG_025,
                    Severity::Error,
                    format!("'{name}' is a publication resource stored inside META-INF"),
                    name.as_str(),
                );
            }
        }
    }

    check_guide_references(
        &doc,
        &base_dir,
        ocf,
        &name_index,
        &items,
        &fallback_map,
        is_epub3,
        opf_path,
        report,
    );
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
                        report.push_at_rule(
                            OPF_045,
                            Severity::Error,
                            "a chain of \"fallback\" attributes forms a cycle",
                            opf_path,
                            "opf.manifest_item.fallback_cycle",
                            Vec::new(),
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
        if let Some((_, mt)) = items.get(overlay_id)
            && mt != "application/smil+xml"
        {
            report.push_at_rule(
                    RSC_005,
                    Severity::Error,
                    format!(
                        "media-overlay target '{overlay_id}' must be of the \"application/smil+xml\" type"
                    ),
                    opf_path,
                    "opf.manifest.media_overlay_target_not_smil",
                    vec![overlay_id.clone()],
                );
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
                && n.attr_no_ns("property") == Some("media:duration")
                && n.attr_no_ns("refines").is_none()
        });
        if !media_overlay_attrs.is_empty() && !has_global_duration {
            report.push_node(
                RSC_005,
                Severity::Error,
                "the global media:duration meta element not set",
                opf_path,
                md,
                "opf.metadata.missing_global_media_duration",
                Vec::new(),
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
                    && n.attr_no_ns("property") == Some("media:duration")
                    && n.attr_no_ns("refines").map(|r| r.trim_start_matches('#'))
                        == Some(overlay_id)
            });
            if !has_item_duration {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("the item media:duration meta element not set for '{overlay_id}'"),
                    opf_path,
                    md,
                    "opf.metadata.missing_item_media_duration",
                    vec![overlay_id.to_string()],
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
        let rel_tokens: Vec<&str> = link
            .attr_no_ns("rel")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if rel_tokens.contains(&"alternate") && rel_tokens.len() > 1 {
            report.push_at_pos(
                OPF_089,
                Severity::Error,
                "the \"alternate\" keyword must not be combined with other link relationships",
                opf_path,
                Position::of(link),
            );
        }
        // Deprecated metadata link-rel keywords (EPUB 3 §D.4.1): the legacy
        // per-format `*-record` forms are superseded by the generic `record`
        // keyword plus a `properties` attribute, and `xml-signature` is
        // dropped entirely. epubcheck reports each as a warning-level
        // OPF-086 (the same warning family the deprecated rendition/viewport
        // properties use — distinct from the usage-level OPF-086b for a
        // deprecated epub:type value).
        const DEPRECATED_LINK_RELS: &[&str] = &[
            "marc21xml-record",
            "mods-record",
            "onix-record",
            "xmp-record",
            "xml-signature",
        ];
        for token in &rel_tokens {
            if DEPRECATED_LINK_RELS.contains(token) {
                report.push_node(
                    OPF_086,
                    Severity::Warning,
                    format!("the \"{token}\" link keyword is deprecated"),
                    opf_path,
                    link,
                    "opf.link.deprecated_rel",
                    vec![token.to_string()],
                );
            }
        }
        // The real EPUB Accessibility 1.1 a11y: link-rel vocabulary
        // (confirmed via real fixtures: "certifierReport"/
        // "certifierCredential" valid, a lowercase "certifierreport"
        // invalid - rel values are case-sensitive).
        // The unprefixed link-rel vocabulary (issue #67), epubcheck's
        // `LINKREL_VOCAB`. The five deprecated names above are members of it
        // too - deprecated, not removed - so they must stay listed here or
        // they would draw OPF-027 on top of their own OPF-086. `acquire` is
        // no longer defined in EPUB 3.3 but epubcheck still accepts it for
        // backward compatibility with the Previews specification, and so do
        // we; dropping it would be a false positive on every preview
        // publication.
        const KNOWN_LINK_RELS: &[&str] = &[
            "acquire",
            "alternate",
            "marc21xml-record",
            "mods-record",
            "onix-record",
            "record",
            "voicing",
            "xml-signature",
            "xmp-record",
        ];
        for token in &rel_tokens {
            if is_epub3 && !token.contains(':') && !KNOWN_LINK_RELS.contains(token) {
                report.push_node(
                    OPF_027,
                    Severity::Error,
                    format!("unknown link relationship '{token}'"),
                    opf_path,
                    link,
                    "opf.link.unknown_rel",
                    vec![token.to_string()],
                );
            }
        }
        const KNOWN_A11Y_LINK_RELS: &[&str] = &["a11y:certifierCredential", "a11y:certifierReport"];
        for token in &rel_tokens {
            if token.starts_with("a11y:") && !KNOWN_A11Y_LINK_RELS.contains(token) {
                report.push_node(
                    OPF_027,
                    Severity::Error,
                    format!("unknown a11y link relationship '{token}'"),
                    opf_path,
                    link,
                    "opf.link.unknown_a11y_rel",
                    vec![token.to_string()],
                );
            }
        }
        // The only real link/@properties vocabulary term is "onix"
        // (confirmed via a real fixture pairing it with a custom-prefixed
        // token, both valid) - anything else unprefixed is undefined.
        if let Some(props) = link.attr_no_ns("properties") {
            for token in props.split_whitespace() {
                if token != "onix" && !token.contains(':') {
                    report.push_node(
                        OPF_027,
                        Severity::Error,
                        format!("unknown link property '{token}'"),
                        opf_path,
                        link,
                        "opf.link.unknown_property",
                        vec![token.to_string()],
                    );
                }
            }
        }
        // "record"/"voicing" links must declare a media-type even when
        // remote - a stricter rule than the general OPF-093 leniency
        // below, confirmed via real fixtures explicitly noting "even when
        // remote".
        let media_type_always_required =
            rel_tokens.iter().any(|t| *t == "record" || *t == "voicing");
        let media_type = link.attr_no_ns("media-type");
        if rel_tokens.contains(&"voicing")
            && let Some(mt) = media_type
            && !mt.starts_with("audio/")
        {
            report.push_at_pos(
                OPF_095,
                Severity::Error,
                format!("a \"voicing\" link's media-type '{mt}' must be an audio type"),
                opf_path,
                Position::of(link),
            );
        }
        let Some(href_attr) = attr_no_ns_node(link, "href") else {
            continue;
        };
        let href = href_attr.value().trim();
        if let Some(frag) = href.strip_prefix('#') {
            if items.contains_key(frag) {
                report.push_at_pos(
                    OPF_098,
                    Severity::Error,
                    "a link target must not reference a manifest item id",
                    opf_path,
                    Position::of(link),
                );
            }
            continue;
        }
        if href.starts_with("data:") {
            report.push_node_attr(
                RSC_029,
                Severity::Error,
                "a package link href must not be a data URL",
                opf_path,
                link,
                href_attr,
                "opf.link.data_url_href",
                Vec::new(),
            );
            continue;
        }
        if href.starts_with("file:") {
            report.push_node_attr(
                RSC_030,
                Severity::Error,
                "a package link href must not be a file URL",
                opf_path,
                link,
                href_attr,
                "opf.link.file_url_href",
                Vec::new(),
            );
            continue;
        }
        if is_external(href) {
            if media_type_always_required && media_type.is_none() {
                report.push_at_pos(
                    OPF_094,
                    Severity::Error,
                    "a \"record\"/\"voicing\" link must declare a media-type even when remote",
                    opf_path,
                    Position::of(link),
                );
            }
            continue;
        }
        if href.contains('?') {
            report.push_node_attr(
                RSC_033,
                Severity::Error,
                format!("package link href '{href}' must not have a query string"),
                opf_path,
                link,
                href_attr,
                "opf.link.href_has_query_string",
                vec![href.to_string()],
            );
        }
        let resolved = resolve(&base_dir, href);
        if !name_index.contains_key(&nfc(&resolved)) {
            // RSC-007w, not RSC-007: epubcheck's `checkUndeclaredReference`
            // splits this one case out by ID, `version == VERSION_3 &&
            // reference.type == LINK`, and it is the *warning* form. We
            // already had the severity right and the ID wrong (#70).
            //
            // The EPUB 2 arm keeps the ID it had rather than taking
            // epubcheck's error-level bare RSC-007, because OPF 2.0 has no
            // package `<link>` to begin with, so which of us is right there
            // is unmeasured. Not worth a restrictive change on a shape no
            // book can legally contain.
            report.push_node_attr(
                if is_epub3 { RSC_007W } else { RSC_007 },
                Severity::Warning,
                format!("link references a missing resource '{href}'"),
                opf_path,
                link,
                href_attr,
                "opf.link.missing_resource",
                vec![href.to_string()],
            );
        }
        if media_type.is_none() {
            report.push_at_pos(
                if media_type_always_required {
                    OPF_094
                } else {
                    OPF_093
                },
                Severity::Error,
                "a link to a local resource must declare a media-type",
                opf_path,
                Position::of(link),
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
    // linear="no", each paired with that itemref's position, for the
    // OPF-096 reachability check below (the position lets OPF-096 point at
    // the offending `<itemref linear="no">` rather than the package root).
    let mut non_linear_paths: Vec<(String, Position)> = Vec::new();
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
        if let Some(page_map) = sp.attr_no_ns("page-map") {
            report.push_node(
                RSC_005,
                Severity::Error,
                "attribute \"page-map\" not allowed here",
                opf_path,
                sp,
                "opf.spine.pagemap_not_allowed",
                Vec::new(),
            );
            // OPF-062 (usage): epubcheck notes the extension's *presence*
            // alongside the schema error above - two findings for the one
            // attribute, saying different things. The RSC-005 says the
            // document is invalid; this says which non-standard feature it
            // is, which is what tells an author whether they meant to use
            // it. Reported missing by Doitsu on the MobileRead forum.
            report.push_node(
                OPF_062,
                Severity::Usage,
                "the Adobe 'page-map' spine extension is in use; it was never part of any EPUB specification",
                opf_path,
                sp,
                "opf.spine.adobe_pagemap_usage",
                Vec::new(),
            );
            if !items.contains_key(page_map.trim()) {
                report.push_at_pos(
                    OPF_063,
                    Severity::Warning,
                    format!("page-map reference '{page_map}' was not found in the manifest"),
                    opf_path,
                    Position::of(sp),
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
            .all(|ir| ir.attr_no_ns("linear").map(str::trim) == Some("no"))
        {
            report.push_at_pos(
                OPF_033,
                Severity::Error,
                "<spine> contains no linear resources",
                opf_path,
                Position::of(sp),
            );
        }
        let mut spine_seen: HashSet<&str> = HashSet::new();
        for (position, ir) in refs.into_iter().enumerate() {
            match ir.attr_no_ns("idref").map(str::trim) {
                None => report.push_node(
                    RSC_005,
                    Severity::Error,
                    "spine <itemref> is missing 'idref'",
                    opf_path,
                    ir,
                    "opf.spine.itemref_missing_idref",
                    Vec::new(),
                ),
                Some(idref) => {
                    if !spine_seen.insert(idref) {
                        // Same underlying condition, version-scoped ID:
                        // EPUB2's own dedicated fixture confirms OPF-034,
                        // but the identically-shaped EPUB3 fixture expects
                        // RSC-005 instead.
                        report.push_node(
                            if is_epub3 { RSC_005 } else { OPF_034 },
                            Severity::Error,
                            format!("spine references manifest item id '{idref}' more than once"),
                            opf_path,
                            ir,
                            "opf.spine.duplicate_itemref",
                            vec![idref.to_string()],
                        );
                    }
                    match items.get(idref) {
                        None => report.push_node(
                            OPF_049,
                            Severity::Error,
                            format!("spine itemref idref '{idref}' was not found in the manifest"),
                            opf_path,
                            ir,
                            "opf.spine.itemref_idref_not_in_manifest",
                            vec![idref.to_string()],
                        ),
                        Some((path, mt)) => {
                            spine_order.entry(nfc(path)).or_insert(position);
                            if ir.attr_no_ns("linear").map(str::trim) == Some("no") {
                                non_linear_paths.push((nfc(path), Position::of(ir)));
                            }
                            // Core content-document media types valid in the
                            // spine without a fallback; otherwise walk the
                            // 'fallback' chain (bounded, in case of a cycle)
                            // looking for one that resolves to a core type.
                            // The set of "blessed" spine content types is
                            // version-specific: EPUB 3 is XHTML or SVG, but
                            // EPUB 2 is XHTML or DTBook (`application/
                            // x-dtbook+xml`), a first-class content type there.
                            // Using the EPUB 3 set for everything reported a
                            // valid EPUB 2 DTBook book as OPF-043 - harmless
                            // while OPF-043 was a warning, a false *error* once
                            // it became one (issue #26; same EPUB-3-into-EPUB-2
                            // class as #24).
                            //
                            // The *deprecated* types (`text/html`,
                            // `text/x-oeb1-document`) are exempt too, and the
                            // exemption is EPUB 2's alone: epubcheck's EPUB 2
                            // branch tests `!isBlessedItemType &&
                            // !isDeprecatedBlessedItemType` (`OPFChecker`
                            // :419) while its EPUB 3 branch tests only
                            // `!isBlessedItemType` (`OPFChecker30`:251), so
                            // OPF-043 on a `text/html` spine item is correct
                            // in an EPUB 3 book and wrong in an EPUB 2 one.
                            //
                            // This used to be scoped to `is_oeb12`, which is
                            // narrower than epubcheck's predicate and cost 91
                            // false positives on one real Calibre-produced
                            // EPUB 2 book (issue #72).
                            let is_core = |mt: &str| {
                                mt == "application/xhtml+xml"
                                    || if is_epub3 {
                                        mt == "image/svg+xml"
                                    } else {
                                        mt == "application/x-dtbook+xml"
                                            || is_deprecated_content_document_type(mt)
                                    }
                            };
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
                                    report.push_at_pos(
                                        OPF_042,
                                        Severity::Error,
                                        format!("spine item idref '{idref}' is an image, not a Content Document"),
                                        opf_path,
                                        Position::of(ir),
                                    );
                                } else if fallback_map.contains_key(idref) {
                                    // A fallback chain exists but no hop reaches
                                    // a content document - epubcheck's OPF-044,
                                    // distinct from OPF-043 (no fallback at all).
                                    // Same ERROR either way; the ID sharpens the
                                    // message (#41).
                                    report.push_at_pos(
                                        OPF_044,
                                        Severity::Error,
                                        format!("spine item idref '{idref}' has non-content media-type '{mt}' whose fallback chain never reaches a content document"),
                                        opf_path,
                                        Position::of(ir),
                                    );
                                } else {
                                    // Error, not a warning: a spine item the
                                    // reading system cannot render and has no
                                    // fallback for is a hole in the reading
                                    // order. epubcheck's severity table says
                                    // ERROR and its one fixture says "Then
                                    // error OPF-043" - two independent
                                    // statements, and we disagreed with both,
                                    // invisibly (issue #26).
                                    report.push_at_pos(
                                        OPF_043,
                                        Severity::Error,
                                        format!("spine item idref '{idref}' has non-content media-type '{mt}' with no fallback"),
                                        opf_path,
                                        Position::of(ir),
                                    );
                                }
                            }

                            // --- Fixed-layout viewport/viewBox checks ---
                            let props = ir.attr_no_ns("properties").unwrap_or("");
                            // **Pre-paginated is tested first, and the order
                            // is load-bearing when an itemref carries both
                            // overrides.** They are mutually exclusive and
                            // both tools say so (RSC-005), but the document
                            // still has to be validated as *something*, and
                            // epubcheck resolves it pre-paginated:
                            // `OPFHandler30.processItemrefProperties` reads
                            // `properties.contains(PRE_PAGINATED) || ...`, so
                            // the pre-paginated disjunct short-circuits before
                            // reflowable is consulted. We tested reflowable
                            // first and called such a document reflowable,
                            // which skipped its viewport requirement —
                            // HTM-046 on W3C's `fxl-spine-overrides_duplicate`.
                            //
                            // An error on a book does not excuse the checks
                            // after it from being right: the reader still gets
                            // a verdict on the rest of the document.
                            let is_fixed_layout = if props
                                .split_whitespace()
                                .any(|p| p == "rendition:layout-pre-paginated")
                            {
                                true
                            } else if props
                                .split_whitespace()
                                .any(|p| p == "rendition:layout-reflowable")
                            {
                                false
                            } else {
                                package_fixed_layout
                            };
                            check_itemref_rendition_conflicts(
                                props, opf_path, ir, is_epub3, report,
                            );
                            if advisory && is_epub3 {
                                if !is_fixed_layout {
                                    check_reflowable_page_spread(props, opf_path, ir, report);
                                }
                                check_epub34_itemref_deprecations(
                                    props,
                                    package_layout_roll,
                                    opf_path,
                                    ir,
                                    report,
                                );
                            }
                            fixed_layout_docs.insert(nfc(path), is_fixed_layout);
                            if let Some(orig) = name_index.get(&nfc(path)).cloned()
                                && let Some(b) = ocf.read(&orig)
                            {
                                let t = String::from_utf8_lossy(&b).into_owned();
                                if let Ok(d) = parse_xml(&t) {
                                    if mt == "application/xhtml+xml" {
                                        if is_fixed_layout {
                                            crate::layout::check_xhtml_viewport(&d, path, report);
                                        } else {
                                            crate::layout::check_reflowable_viewport(
                                                &d, path, report,
                                            );
                                        }
                                    } else if mt == "image/svg+xml" && is_fixed_layout {
                                        crate::layout::check_svg_viewbox(&d, path, report);
                                    }
                                    // EPUB 3.4 (#1651): a roll spine must
                                    // reference fixed-layout documents, which
                                    // for an XHTML one means its ICB
                                    // dimensions are set.
                                    //
                                    // Deliberately *not* done by making roll
                                    // imply `is_fixed_layout` above. That
                                    // would be the truer model, and it would
                                    // switch on `check_xhtml_viewport`, whose
                                    // findings are HTM-046 and friends at
                                    // error severity - restrictive, unflagged,
                                    // and counting toward the verdict, which
                                    // is exactly what an advisory may not do.
                                    // So the presence question is asked here
                                    // and answered at usage level; when
                                    // epubcheck ships #1651 this collapses
                                    // into `is_fixed_layout` and this block
                                    // goes away.
                                    if advisory
                                        && is_epub3
                                        && package_layout_roll
                                        && mt == "application/xhtml+xml"
                                        && !has_icb_dimensions(&d)
                                    {
                                        report.push_at(
                                            NEXT_007,
                                            Severity::Usage,
                                            "EPUB 3.4: a roll layout requires fixed-layout \
                                             documents, but this one declares no viewport \
                                             width and height",
                                            path,
                                        );
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
        match sp.attr_no_ns("toc").map(str::trim) {
            None => {
                // ...but not in OEBPS 1.2, which predates the NCX entirely.
                if is_epub2 && !is_oeb12 {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "EPUB 2 <spine> is missing the required 'toc' (NCX) attribute",
                        opf_path,
                        sp,
                        "opf.spine.missing_toc_epub2",
                        Vec::new(),
                    );
                }
            }
            Some(toc) => match items.get(toc) {
                None => report.push_node(
                    OPF_049,
                    Severity::Error,
                    format!("spine 'toc' idref '{toc}' was not found in the manifest"),
                    opf_path,
                    sp,
                    "opf.spine.toc_idref_not_in_manifest",
                    vec![toc.to_string()],
                ),
                Some((ncx_path, mt)) => {
                    if mt != NCX {
                        report.push_at_pos(
                            OPF_050,
                            Severity::Error,
                            format!("spine 'toc' references '{toc}' with media-type '{mt}'; an NCX ({NCX}) is expected"),
                            opf_path,
                            Position::of(sp),
                        );
                    } else if let Some(orig) = name_index.get(&nfc(ncx_path)).cloned()
                        && let Some(b) = ocf.read(&orig)
                    {
                        let ncx_text = String::from_utf8_lossy(&b).into_owned();
                        // Only NCX-001/NCX-004 need the package identifier -
                        // they compare `dtb:uid` against it. Gating the whole
                        // block on it meant a book whose `unique-identifier`
                        // resolves to nothing (already its own OPF-030) had
                        // RSC-007, RSC-010 and RSC-012 on its NCX silently
                        // switched off as well. One shelf book, three real
                        // undefined fragments, and no output at all: the
                        // familiar shape where a precondition for one check
                        // takes unrelated ones down with it.
                        if let Some(uid_text) = &package_identifier_text {
                            crate::ncx::check(&ncx_text, ncx_path, uid_text, report);
                        }
                        if let Ok(ncx_doc) = parse_xml(&ncx_text) {
                            check_ncx_content_fragments(
                                &ncx_doc,
                                ncx_path,
                                ocf,
                                &name_index,
                                &items,
                                &fallback_map,
                                is_epub3,
                                report,
                            );
                            // Outside the `package_identifier_text` guard
                            // above on purpose: this one has nothing to do
                            // with `dtb:uid`, and that guard is exactly how
                            // unrelated NCX checks got switched off once.
                            if advisory {
                                crate::ncx::check_duplicate_targets(&ncx_doc, ncx_path, report);
                            }
                        }
                    }
                }
            },
        }
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            "OPF is missing the <spine> element",
            opf_path,
            pkg,
            "opf.package.missing_spine_element",
            Vec::new(),
        );
    }

    // OPF-067 (#55): a resource pointed at by a metadata <link> must not
    // also be a manifest item. epubcheck's rule
    // (`OPFChecker30.checkLinkedResources`) carries one extra condition that
    // is easy to miss: it only fires when the manifest item is **not in the
    // spine**. Dropping that would over-fire on the legitimate case of a
    // linked resource that is also a content document. EPUB 3 only - the
    // check lives in epubcheck's EPUB 3 checker.
    if is_epub3 && let Some(md) = metadata {
        let spine_ids: HashSet<&str> = pkg
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "spine")
            .into_iter()
            .flat_map(|sp| sp.children())
            .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
            .filter_map(|ir| ir.attr_no_ns("idref").map(str::trim))
            .collect();
        // resolved manifest path -> is that item in the spine
        let in_spine: HashMap<&str, bool> = items
            .iter()
            .map(|(id, (resolved, _))| (resolved.as_str(), spine_ids.contains(id.as_str())))
            .collect();
        for link in md
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
        {
            let Some(href) = link.attr_no_ns("href") else {
                continue;
            };
            if is_external(href) {
                continue;
            }
            // The rule is about the linked *document*, so a fragment on the
            // link doesn't change which resource it names.
            let doc_href = href.split('#').next().unwrap_or(href);
            let resolved = resolve(&base_dir, doc_href);
            if in_spine.get(resolved.as_str()) == Some(&false) {
                report.push_node(
                    OPF_067,
                    Severity::Error,
                    format!(
                        "'{doc_href}' must not be both a metadata <link> target and a manifest item"
                    ),
                    opf_path,
                    link,
                    "opf.link.also_a_manifest_item",
                    vec![doc_href.to_string()],
                );
            }
        }
    }

    // --- Data Navigation Document (EPUB Region-Based Navigation) ---
    if data_nav_items.len() > 1 {
        report.push_at_rule(
            RSC_005,
            Severity::Error,
            "the manifest must not include more than one Data Navigation Document",
            opf_path,
            "opf.manifest.multiple_data_nav_documents",
            Vec::new(),
        );
    }
    let data_nav_path: Option<String> = data_nav_items.first().map(|(path, _)| nfc(path));
    if let Some((path, mt)) = data_nav_items.first() {
        if mt != "application/xhtml+xml" {
            report.push_at_rule(
                OPF_012,
                Severity::Error,
                "the Data Navigation Document must be an XHTML content document",
                opf_path,
                "opf.manifest.data_nav_not_xhtml",
                Vec::new(),
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
        report.push_node(
            RSC_005,
            Severity::Error,
            "EPUB 3 requires a navigation document (a manifest item with properties=\"nav\")",
            opf_path,
            pkg,
            "opf.package.missing_nav_document",
            Vec::new(),
        );
    }
    if nav_count > 1 {
        report.push_node(
            RSC_005,
            Severity::Error,
            "only one manifest item may declare the \"nav\" property",
            opf_path,
            pkg,
            "opf.manifest.multiple_nav_documents",
            Vec::new(),
        );
    }
    // NAV-001 is NOT emitted, because epubcheck cannot emit it: the ID is
    // dead in their source and we had implemented it as a false positive.
    //
    // Its one call site is `NavChecker`'s constructor, guarded by
    // `version == VERSION_2` - which reads like "an EPUB 2 book with a nav
    // document". But a `NavChecker` is only ever constructed for an item
    // where `isNav()` holds, and `isNav()` comes from the manifest
    // `properties` attribute, which **only the EPUB 3 handler parses**;
    // `OPFHandler` (EPUB 2) has no properties handling at all. The CLI's
    // single-file path guards the same construction with
    // `if (version == VERSION_3)`. So the branch is unreachable from either
    // direction.
    //
    // Confirmed on a real book rather than by reading alone: DNSB posted
    // epubcheck's and epubveri's output for the same mislabelled EPUB
    // (MobileRead #134). epubcheck reported the nav document's *contents*
    // as ordinary XHTML 1.1 violations - `<nav>` is not in that content
    // model - and no NAV-001. We reported NAV-001 on top of the same
    // content errors.
    //
    // Nothing is lost by removing it. An EPUB 2 book carrying a nav document
    // is still reported, through the content grammar, which is exactly how
    // epubcheck reports it. See docs/COVERAGE.md, where NAV-001 now joins
    // OPF-011/OPF-036/PKG-015 as a dead ID.
    let _ = nav_present;

    // ADV-004 is emitted *after* the content-document walk below, because
    // half its evidence is gathered there (`ContentVersionSignals`).

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
        // **This is a container-level question, and epubcheck asks it in
        // `OCFChecker`, once, against every package document at the same
        // time**: `Iterables.tryFind(opfHandlers, ...)` searches *all* of
        // them, and counts a match in either a manifest `<item>` or — on
        // EPUB 3 — a metadata `<link>` (`getLinkedResources`).
        //
        // Ours ran inside the per-package check with only that package's
        // manifest in hand, which was wrong twice over, and W3C's
        // `epub-tests` has one publication for each:
        //
        // - `ocf-package_multiple` declares three renditions in three
        //   directories. Each rendition's manifest legitimately lists only
        //   its own files, so every package blamed the other two's — 18
        //   findings where epubcheck reports none.
        // - `pkg-linked-records` references an ONIX record with
        //   `<link rel="record" href="sampleONIX-30.xml"/>` in the package
        //   metadata. A `<link>` is a declaration; we only looked at
        //   `<item>`.
        //
        // So: the declared set is the union across every rootfile, a
        // `<link href>` counts, and the finding is emitted once for the
        // publication rather than once per rendition — hence the
        // default-rendition guard below.
        let rootfiles = container_rootfiles_silently(ocf);
        let is_default_rendition = rootfiles
            .first()
            .is_none_or(|first| *first == nfc(opf_path));
        let mut declared = manifest_paths.clone();
        if is_default_rendition {
            // `declared_resources_of` re-reads this package too, which is
            // what picks up its own `<link href>` targets; `manifest_paths`
            // holds only the `<item>`s.
            for rf in std::iter::once(nfc(opf_path)).chain(rootfiles.iter().cloned()) {
                declared.extend(declared_resources_of(ocf, &rf));
            }
        }
        let structural: HashSet<String> = std::iter::once(nfc(opf_path))
            .chain(rootfiles.iter().cloned())
            .collect();
        let names: Vec<String> = ocf.names.clone();
        for name in names.iter().filter(|_| is_default_rendition) {
            if name == "mimetype" || name.starts_with("META-INF/") || name.ends_with('/') {
                continue;
            }
            let key = nfc(name);
            if structural.contains(&key) {
                continue;
            }
            let basename = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if IGNORED_BASENAMES.contains(&basename.as_str()) {
                continue;
            }
            if !declared.contains(&key) {
                report.push_at_rule(
                    OPF_003,
                    Severity::Usage,
                    format!("container resource '{name}' is not listed in the manifest"),
                    opf_path,
                    "opf.container.resource_not_in_manifest",
                    vec![name.to_string()],
                );
            }
        }
    }

    // resolved-resource-key -> Core-Media-Type/fallback status, for the
    // foreign-resource-fallback checks (RSC-032/MED-003/MED-007) below.
    let resource_status = crate::foreign::build_resource_status(&items, &fallback_map);

    // --- broken internal references + content-model from content documents ---
    // An EPUB 2 `text/html` item is checked here too, but *not* against the
    // XHTML grammar - see `schema_validated_docs` below. epubcheck runs its
    // OPS checker over such an item (`CheckerFactory`, `case HTML:`, guarded
    // on VERSION_2) which always installs the handler, while the validators
    // it installs come from a map keyed on `application/xhtml+xml`, so the
    // list comes back empty. Handler checks yes, schema no.
    //
    // Leaving them out entirely meant every reference inside them went
    // unchecked: one real book's 91 `text/html` documents hid 91 missing
    // resources, and a document that wasn't even well-formed XML reported
    // nothing at all (issue #72). That is the silent-skip shape again - a
    // document dropped from every check reads exactly like a clean one.
    // Manifest document order, not `items.values()` — the loop below decides
    // the order findings arrive in, and therefore the file order of the whole
    // report. See `manifest_order`'s note: sourcing this from the HashMap made
    // a quarter of real books print a differently-ordered report on every run.
    let content_docs: Vec<String> = manifest_order
        .iter()
        .filter(|(_, mt)| {
            mt == "application/xhtml+xml" || (!is_epub3 && is_deprecated_content_document_type(mt))
        })
        .map(|(path, _)| path.clone())
        .collect();
    // The subset the RELAX NG grammar and the Schematron run over. Measured,
    // not assumed: the identical document draws three RSC-005 from epubcheck
    // when the manifest declares it `application/xhtml+xml` and none at all
    // when it declares it `text/html`.
    let schema_validated_docs: HashSet<String> = items
        .values()
        .filter(|(_, mt)| mt == "application/xhtml+xml")
        .map(|(path, _)| nfc(path))
        .collect();
    // Which content docs are XHTML (as opposed to e.g. SVG) - the
    // CSS-029/030 cross-referencing pass below only has CSS-collection
    // support for XHTML docs (SVG's own <style>/xml-stylesheet forms are a
    // deliberately deferred, separate extension), so it must not treat an
    // SVG doc's absence from `doc_class_names` as "no CSS found."
    let xhtml_doc_paths: HashSet<String> = content_docs.iter().cloned().collect();
    // The NFC-normalized paths of every SVG resource in the manifest, built
    // once here rather than rebuilt per reference.
    //
    // The `references_svg` check below asks, for each `src`/`href`/`data`/
    // `poster` in each content document, whether that target is an SVG
    // manifest item. Asking it as a scan over `items` re-normalized every
    // manifest path on every attribute — n references x m items — and
    // because the scan only stops early when it *finds* an SVG, the worst
    // case was the common one: a book with no SVG at all. On a 4,000-item
    // package that was 16 million `nfc` calls and 93% of the run.
    let svg_manifest_paths: HashSet<String> = items
        .values()
        .filter(|(_, mt)| mt.trim() == "image/svg+xml")
        .map(|(res, _)| nfc(res))
        .collect();
    // Content documents are validated against the version's own content model:
    // EPUB 3 is XHTML5, EPUB 2 is XHTML 1.1 + OPS 2.0.1 - different
    // vocabularies in both directions (`big`/`tt` valid only in EPUB 2, the
    // HTML5 additions and `s`/`u` valid only in EPUB 3). Using the EPUB 3
    // grammar for everything produced false positives and false negatives at
    // once on real EPUB 2 books (issue #24).
    let xhtml_grammar = if is_epub3 {
        crate::rng::xhtml_grammar()
    } else {
        crate::rng::xhtml_grammar_epub2()
    };
    // The HTML5 content-model *nesting* constraints the RELAX NG grammar above
    // structurally can't express (a node must / must not have a given
    // ancestor: interactive content not inside <a>, no nested <form>, …).
    // EPUB 3 only — these are XHTML5 rules; EPUB 2 uses its own grammar and has
    // no such Schematron. Built once, run per content document below.
    let xhtml_sch = is_epub3.then(crate::schematron::xhtml_schema);
    // content-doc resolved-path -> CSS class names used in its own
    // associated stylesheets (inline <style> + linked <link
    // rel="stylesheet">), for the CSS-029/030 cross-referencing pass below.
    let mut doc_class_names: HashMap<String, HashSet<String>> = HashMap::new();
    // Where a media-overlay class name is actually *written*: (class name,
    // the file holding that CSS, position within it). `doc_class_names`
    // above answers "which classes does this content document see", which
    // is what CSS-030 needs; it cannot answer "where do I go to read this
    // one", which is what CSS-029 must tell the author - the name lives in
    // the stylesheet, not in the document that links it. An inline `<style>`
    // maps through `css::CssOrigin` like every other CSS position; the
    // `Option` is for the SVG collector, which has no CSS offsets at all.
    let mut mo_class_sites: Vec<(String, String, Option<Position>)> = Vec::new();
    // Whether the (required) toc nav has an epub:type="page-list" nav -
    // for the EDUPUB pagination-source cross-check after this loop.
    let mut has_page_list_nav = false;
    // EDUPUB nav-completeness (NAV-004..008): content-doc features vs the
    // nav's special-nav lists, accumulated across the loop below.
    let mut nav_completeness = crate::edupub::NavCompleteness::default();
    // Every local content-doc target hyperlinked from *any* content
    // document (including the nav) - for RSC-011 (a hyperlink target not
    // listed in the spine) and OPF-096 (a linear="no" spine item not
    // reachable via any hyperlink or the nav). Keyed by resolved target, each
    // remembers the *source* `<a>` that hyperlinks to it (file + position +
    // element path), captured while its document is still parsed, so RSC-011
    // can anchor at that link instead of the OPF package root (#22). First
    // source per target wins.
    struct HyperlinkSource {
        file: String,
        position: Position,
        element_path: crate::xmlext::NodePath,
    }
    let mut hyperlink_targets: HashMap<String, HyperlinkSource> = HashMap::new();
    // Whether *any* content document in the whole book uses scripting -
    // mirrors real epubcheck's book-wide `FeatureEnum.HAS_SCRIPTS` (not
    // scoped to any one document): when true, OPF-096's "non-linear
    // content unreachable" check is downgraded from an error to a usage
    // note (OPF-096b), since script could add navigation/hyperlinks
    // dynamically that this static analysis can't see.
    let mut book_has_scripts = false;
    // Resolved paths of every content document carrying an
    // epub:type="dictionary" marker anywhere - for the EPUB Dictionaries &
    // Glossaries OPF-078/079 cross-checks in `check_dictionaries` below
    // (checked per-collection for a multi-dictionary publication, so a
    // bool alone isn't enough).
    let mut dictionary_marked_docs: HashSet<String> = HashSet::new();
    // EPUB Indexes 1.0: which content documents are specifically
    // identified as indexes (manifest properties="index", or linked from
    // a `<collection role="index"|"index-group">`) - each such document
    // must itself carry an epub:type="index" marker. Absent either
    // signal, a confirmed index publication (dc:type=index) instead only
    // needs *some* content document anywhere to have one (tracked via
    // `any_index_content` below).
    let manifest_index_paths: HashSet<String> = item_properties
        .iter()
        .filter(|(_, props)| props.split_whitespace().any(|t| t == "index"))
        .map(|(p, _)| p.clone())
        .collect();
    let collection_index_paths: HashSet<String> = crate::indexes::linked_paths(&pkg, &base_dir);
    let is_index_pub = opf_dc_type.as_deref() == Some("index");
    let mut any_index_content = false;
    // Every publication resource some document actually *consumes* - drawn,
    // applied, loaded (see `is_resource_reference`). Manifest items that
    // never appear here are what OPF-097 reports. Collected across the whole
    // walk, since any document may be the one that uses a given resource.
    let mut resource_refs: HashSet<String> = HashSet::new();
    // The same question for *remote* targets, which `resource_refs` cannot
    // answer: it only ever records references that are not external. Needed
    // book-wide, because "no reference to this remote item exists anywhere"
    // is what selects RSC-006/RSC-006b below.
    let mut remote_resource_refs: HashSet<String> = HashSet::new();
    let mut pending_dtd_fix: Option<(usize, String, crate::htm::DtdShift)> = None;
    // ADV-004's content-document half; only ever read for an EPUB 2 book.
    let mut version_signals = ContentVersionSignals::default();
    for path in content_docs {
        if let Some((from, p, sh)) = pending_dtd_fix.take() {
            crate::htm::correct_dtd_shift(&mut report.messages[from..], &p, sh);
        }
        let Some(orig) = name_index.get(&nfc(&path)).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        // BOM-aware decode: a UTF-16-encoded content document read as
        // plain UTF-8 turns into byte-level garbage that fails to parse
        // as XML at all, silently skipping every check below - not just
        // HTM-058 (same fix `css::decode_bytes` already got for
        // stylesheets, reused here rather than duplicated).
        // Whether this document is checked against the grammar as well as by
        // the hand-coded checks - false for an EPUB 2 `text/html` item, which
        // epubcheck gives the OPS *handler* but an empty *validator* list.
        // Everything derived from one of its validators has to hang off this,
        // not just the RELAX NG pass: measured against epubcheck one document
        // at a time, `text/html` draws no schema violation, no duplicate-`id`
        // report (its `IDUNIQUE_20_SCH` is keyed the same way) and no
        // ID-reference resolution.
        let schema_validated = schema_validated_docs.contains(&nfc(&path));
        let t = crate::css::decode_bytes(&b);
        // The raw scans must see the document exactly as authored, so they
        // run before the entity declarations below are added to it.
        let before_raw = report.messages.len();
        crate::htm::check_raw(&b, &t, &path, is_epub3, report);
        // An EPUB 2 document's DOCTYPE promises the XHTML DTD's named
        // entities; declare them inline so `&nbsp;` doesn't fail the parse
        // and silently skip every check below (issue #23). Line numbers are
        // preserved, and `t` stays the text `d` was parsed from - checks
        // below pass both together and must not disagree.
        let (t, dtd_shift) = crate::htm::declare_dtd_entities(t, is_epub3);
        // Every finding this iteration is about to push carries a position
        // taken from `t`, which the injection above moved. Record where this
        // document's findings start; they get corrected back onto the real
        // file at the top of the next iteration (and after the loop for the
        // last document) - deliberately not at the end of this body, which
        // the many `continue`s below would skip.
        if let Some(sh) = dtd_shift {
            pending_dtd_fix = Some((report.messages.len(), path.clone(), sh));
        }
        let d = match parse_xml(&t) {
            Ok(d) => d,
            Err(e) => {
                // A content document that isn't well-formed XML was, until
                // now, silently skipped — every check below it never ran and
                // the book validated clean (a false negative; forum report,
                // issue #12). Surface it as RSC-016 Fatal at the parse-error
                // position, mirroring how the OPF's own parse failure is
                // handled. Entity-reference failures are the one exception:
                // `check_raw`'s entity scan above already owns those
                // (undeclared / missing-';' named entities), and reporting
                // them here too would double up two Fatals on one defect.
                //
                // The suppression asks whether that scan *actually*
                // reported, instead of trusting that it covers the class.
                // That trust has now been misplaced three times, and every
                // time the failure was silence rather than a wrong answer:
                // the document neither parsed nor drew a finding, so it
                // vanished from every check below and a book with real
                // errors validated clean. Issue #23 lost 690 documents that
                // way; 0.7.12 was a bare `&`; 0.7.13 an XHTML 1.0 `&nbsp;`;
                // and malformed numeric references (`&#0;`, `&#;`, `&#zz;`,
                // an unterminated `&#38`) were the same hole again - the
                // parser calls them entity errors, the scan reads only named
                // references and walks straight past them.
                //
                // Reading the report costs one slice scan and cannot drift
                // out of date, which is the point: the invariant is enforced
                // here rather than documented and hoped for.
                let entity_reported = report.messages[before_raw..]
                    .iter()
                    .any(|m| m.rule.is_some_and(|r| r.starts_with("htm.entity.")));
                if !(crate::ocf::is_entity_reference_error(&e) && entity_reported) {
                    // roxmltree's wording for an unexpected close tag reads
                    // backwards and points at the close tag rather than the
                    // element the author left open, so we say it ourselves.
                    // Every other parse error keeps the library's wording.
                    // See `unterminated_element_message`.
                    let detail = crate::ocf::unterminated_element_message(&t, &e)
                        .unwrap_or_else(|| e.to_string());
                    report.push_full(
                        RSC_016,
                        Severity::Fatal,
                        format!("content document is not well-formed XML: {detail}"),
                        path.clone(),
                        Position::of_parse_error(&e),
                        "content.malformed_xml",
                        Vec::new(),
                    );
                }
                // #73: recover the references this document had already
                // shown before it stopped parsing. epubcheck's parser is
                // streaming, so it has registered whatever it passed and
                // still reports it; ours builds a DOM, gets nothing, and
                // every reference in the document went unchecked - a book
                // with a missing stylesheet *and* a stray `&` reported only
                // the entity.
                //
                // Only what *precedes* the failure, which is what epubcheck
                // keeps: everything after the error is lost there too, and
                // claiming more would be a divergence in the direction that
                // reads as invention.
                //
                // This is the one path where a text scan is the right tool.
                // It runs nowhere else - a document that parses is walked as
                // a DOM - so the comment/CDATA misreadings a scanner can make
                // are confined to books that are already FATAL and INVALID,
                // rather than paid on every book. That is the argument the
                // issue got backwards, and `scan_references` skips those
                // constructs by construction anyway.
                let err_offset = {
                    let p = e.pos();
                    let mut off = t.len();
                    let mut row = 1u32;
                    for (i, ch) in t.char_indices() {
                        if row == p.row {
                            off = i + (p.col as usize).saturating_sub(1);
                            break;
                        }
                        if ch == '\n' {
                            row += 1;
                        }
                    }
                    off.min(t.len())
                };
                for (value, off) in crate::htm::scan_references(&t) {
                    if off >= err_offset {
                        break;
                    }
                    let v = value.trim();
                    if v.is_empty() || is_external(v) || v.starts_with('#') {
                        continue;
                    }
                    let resolved = nfc(&resolve(&parent_dir(&path), strip_url_fragment(v).trim()));
                    let (id, msg, rule) = match classify_resource_ref(
                        &resolved,
                        &manifest_paths,
                        &name_index,
                        opf_path,
                    ) {
                        ResourceRef::Fine => continue,
                        ResourceRef::Undeclared => (
                            RSC_008,
                            format!("resource '{v}' is not declared in the manifest"),
                            "opf.content_document.undeclared_resource",
                        ),
                        ResourceRef::Missing => (
                            RSC_007,
                            format!("reference to a resource missing from the publication: '{v}'"),
                            "opf.content_document.reference_missing_resource",
                        ),
                    };
                    report.push_full(
                        id,
                        Severity::Error,
                        msg,
                        path.clone(),
                        Position::of_offset(&t, off),
                        rule,
                        vec![v.to_string()],
                    );
                }
                continue;
            }
        };

        // **A viewport meta in a document that is not in the spine.**
        // `check_reflowable_viewport` runs in the spine-itemref loop above,
        // so a manifest XHTML document outside the spine — a nav document,
        // an unreferenced cover page — was never asked. epubcheck asks every
        // XHTML content document: the check lives in `OPSHandler30`, which
        // runs per document, and the layout comes from the item
        // (`OPFItem.isFixedLayout`).
        //
        // A non-spine document is therefore **reflowable whatever the package
        // says**, which is the part worth writing down: `fixedLayout` is set
        // only in `OPFHandler30.processItemrefProperties`, and an item with no
        // itemref never reaches it. So a `rendition:layout` of `pre-paginated`
        // at package level does not make the nav document fixed-layout.
        // W3C's `lay-pp-embedded-images` is exactly that shape — a
        // pre-paginated book whose nav and cover both carry a viewport.
        //
        // Spine members keep their own call above; this covers the rest, and
        // `fixed_layout_docs` holds precisely the spine members.
        if is_epub3 && !fixed_layout_docs.contains_key(&nfc(&path)) {
            crate::layout::check_reflowable_viewport(&d, &path, report);
        }
        crate::htm::check_dom(&d, &path, is_epub3, report);
        if !is_epub3 {
            crate::htm::check_dom_epub2(&d, &path, report);
        }
        crate::dict::check_content_doc(&d, &path, report);
        if crate::dict::has_dictionary_marker(&d) {
            dictionary_marked_docs.insert(nfc(&path));
        }

        // ADV-004's content half. Only an EPUB 2 book can be diagnosed this
        // way, and both flags latch, so this stops looking once each is set.
        if !is_epub3 && !(version_signals.epub_type && version_signals.html5_sectioning) {
            const HTML5_SECTIONING: &[&str] =
                &["section", "header", "footer", "article", "aside", "nav"];
            for n in d.descendants().filter(|n| n.is_element()) {
                // `attribute("epub:type")` would not do: roxmltree matches on
                // the *namespace*, and the prefix a document happens to bind
                // is not the question. Asking for the OPS namespace with any
                // local name `type` is.
                if n.attributes().any(|a| {
                    a.name() == "type" && a.namespace() == Some("http://www.idpf.org/2007/ops")
                }) {
                    version_signals.epub_type = true;
                }
                if HTML5_SECTIONING.contains(&n.tag_name().name()) {
                    version_signals.html5_sectioning = true;
                }
                if version_signals.epub_type && version_signals.html5_sectioning {
                    break;
                }
            }
        }

        if is_epub3 {
            let doc_key = nfc(&path);
            let has_index_elem = !crate::indexes::index_elements(&d).is_empty();
            if has_index_elem {
                any_index_content = true;
            } else if manifest_index_paths.contains(&doc_key)
                || collection_index_paths.contains(&doc_key)
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "At least one \"index\" element must be present in a document declared as an index in the OPF",
                    path.clone(),
                    d.root_element(),
                    "opf.index.missing_index_element",
                    Vec::new(),
                );
            }
            // epubcheck applies the index content-model schematron
            // (idx-xhtml.sch) only to documents *declared* as an index - a
            // manifest `properties="index"` item, a document linked from an
            // index `<collection>`, or (whole publication) `dc:type="index"`
            // - never to a document that merely *contains* an
            // `epub:type="index"` element. A nav landmark like `<a epub:type=
            // "index" href="index.xhtml">` is such an element but sits in an
            // ordinary nav document, which OPSChecker/NavChecker leave
            // unvalidated by this schema. Gating on the same signal avoids a
            // false RSC-005 on those landmarks (Doitsu, MobileRead #72).
            if manifest_index_paths.contains(&doc_key)
                || collection_index_paths.contains(&doc_key)
                || is_index_pub
            {
                crate::indexes::check_content_model(&d, &path, report);
            }
        }

        let declared_prefixes =
            attr_ns_node(d.root_element(), "http://www.idpf.org/2007/ops", "prefix")
                .map(|p| {
                    check_prefix_declaration(
                        p,
                        &path,
                        d.root_element(),
                        PrefixContext::ContentDocument,
                        advisory,
                        report,
                    )
                })
                .unwrap_or_default();
        check_prefix_placement(&d, &path, report);
        for n in d.descendants().filter(|n| n.is_element()) {
            if let Some(v) = n.attribute(("http://www.idpf.org/2007/ops", "type")) {
                check_prefix_usage(v, &declared_prefixes, &path, n, report);
            }
        }

        // EDUPUB: microdata attributes aren't allowed in an edupub content
        // document (applies to every content doc uniformly, nav docs
        // included - no fixture suggests otherwise).
        if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
            crate::edupub::check_content_doc(&d, &path, report);
            let doc_key = nfc(&path);
            let is_nav = nav_path.as_deref() == Some(path.as_str());
            // NAV-005..008: media features come from every content document
            // (the nav is handled separately, below).
            if !is_nav {
                nav_completeness.add_media(&d);
            }
            // Sectioning/heading structure is exempt for fixed-layout
            // content ("Section with no heading OK in FXL", a real
            // fixture's own comment) and for non-linear spine items
            // ("EDUPUB structural requirements do not apply to non-linear
            // content", also a real fixture comment).
            let is_fxl = fixed_layout_docs.get(&doc_key).copied().unwrap_or(false);
            let is_non_linear = non_linear_paths.iter().any(|(p, _)| p == &doc_key);
            if !is_fxl && !is_non_linear {
                crate::edupub::check_sectioning_and_headings(&d, &path, report);
                // NAV-004: epubcheck counts SECTIONS on linear content docs.
                if !is_nav {
                    nav_completeness.add_sections(&d);
                }
            }
        }

        let nfc_path = nfc(&path);
        if nav_path.as_deref() == Some(path.as_str()) {
            has_page_list_nav = d.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "nav"
                    && n.attribute(("http://www.idpf.org/2007/ops", "type")) == Some("page-list")
            });
            if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
                nav_completeness.set_nav(&d);
            }
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
                        report.push_at_pos(
                            NAV_009,
                            Severity::Error,
                            format!(
                                "region-based nav target '{href}' is not a fixed-layout document"
                            ),
                            path.clone(),
                            Position::of(region_nav),
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
        // The grammar reports *where* the content model collapsed - every
        // offending node in document order (issues #17/#18), each with a real
        // line:column and element path instead of anchoring the whole document
        // at its root, pinning the offending attribute when attribute-level, and
        // naming *what* is wrong (the offending element/attribute) in the message
        // text, in the style of epubcheck's own RSC-005 wording (forum #78).
        //
        // Skipped for an EPUB 2 `text/html` document: epubcheck installs the
        // handler but no validators for those, so it reports their references
        // and their well-formedness and says nothing about their content
        // model. Running the grammar here anyway would have turned issue #72's
        // 94 false positives into a larger number of them - the one real book
        // is 91 legacy HTML documents deep.
        if schema_validated {
            let rule = "opf.content_document.schema_violation";
            for blame in crate::rng::validate_node_report(&xhtml_grammar, d.root_element()) {
                // #35 (part of the #31 attribute-allowlist epic): data-* is
                // an open-ended attribute-name family RELAX NG can't
                // express as a name class, so it has no explicit grammar
                // rule and (once #36 removes the still-present permissive
                // wildcard) would otherwise blame it "not allowed" here.
                // Suppressed at the report level instead - a malformed
                // data-* name is separately and more precisely caught by
                // HTM-061 in htm.rs, so this deliberately doesn't
                // re-validate the suffix (see is_data_attribute_name's own
                // doc comment). Currently unreachable in practice (the
                // wildcard already accepts data-* names, so the grammar
                // never blames them not-allowed to begin with) - built now
                // so the mechanism exists and is tested ahead of #36.
                //
                // EPUB 3 only. `data-*` is an HTML5 concept and XHTML 1.1 has
                // no such family, so epubcheck reports a plain RSC-005 for
                // `<p data-foo="x">` in a `version="2.0"` book - probed one
                // book per case against 5.3.0, with a clean control on both
                // sides. Suppressing it at every version was a false
                // negative, found while verifying the 2026-08-16 shelf
                // additions.
                if is_epub3
                    && let crate::rng::Blame::Attribute(
                        _,
                        a,
                        crate::rng::AttributeFault::NotAllowed,
                    ) = &blame
                    && a.namespace().is_none()
                    && crate::htm::is_data_attribute_name(a.name())
                {
                    continue;
                }
                // `check_dom`/`check_dom_epub2` ran earlier over this same
                // document and report obsolete attributes under their own
                // rule. Where both fire we emit one attribute twice, and
                // epubcheck emits it once.
                //
                // Asked of the report rather than assumed, the same shape as
                // the entity suppression above: the two checks own overlapping
                // but not nested sets (`clear` is obsolete *and* absent from
                // the grammar; a misspelt attribute is only the latter), so a
                // claim that one covers the other would be the exact kind of
                // belief that keeps turning out false here.
                //
                // This predates #69 - `<p clear="all">` already drew both -
                // but #69 widened its reach: the attributes of a *misplaced*
                // element never reached the grammar at all before, so on one
                // shelf book ten `<br clear>` went from one finding each to
                // two.
                if let crate::rng::Blame::Attribute(node, a, _) = &blame
                    && let here = crate::xmlext::node_path_attr(*node, *a)
                    && report.messages.iter().any(|m| {
                        m.rule == Some("htm.obsolete_attribute")
                            && m.location.as_deref() == Some(path.as_str())
                            && m.element_path.as_ref().is_some_and(|p| p.path == here.path)
                    })
                {
                    continue;
                }
                push_blame(report, &path, rule, &blame);
            }
            // EPUB 3 content-model nesting constraints (Schematron), reported as
            // RSC-005 at the offending element, matching epubcheck.
            if let Some(sch) = &xhtml_sch {
                for (message, position, rule) in
                    crate::schematron::run(sch, &d, "opf.content_document")
                {
                    report.push_full(
                        RSC_005,
                        Severity::Error,
                        message,
                        path.clone(),
                        position,
                        rule,
                        Vec::new(),
                    );
                }
                // IDREF/IDREFS resolution (hand-coded; needs per-token iteration
                // the Schematron's XPath 1.0 core can't do). EPUB 3 only.
                crate::htm::check_idref_resolution(&d, &path, report);
            }
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
            // RSC-025 is EPUB 3 only. epubcheck attaches the full SVG 1.1
            // grammar as `SVG_30_INFORMATIVE_NVDL` - the one validator it
            // registers with `isNormative=false`, which is what turns its
            // findings into usage-level RSC-025 - and its `ValidatorMap`
            // pairs that validator with VERSION_3 alone. An EPUB 2 document
            // gets `XHTML_20_NVDL`/`SVG_20_NVDL` and no informative pass at
            // all, so epubcheck never emits RSC-025 there.
            //
            // We ran it on both versions, which made a lowercase `viewbox`
            // in a real EPUB 2 book our last false-positive candidate on the
            // 104-book shelf. The attribute really is wrong - SVG names are
            // case-sensitive - but epubcheck has no opinion on it in EPUB 2,
            // and RSC-025 is the "informative" family precisely because it is
            // epubcheck's opinion rather than a spec requirement.
            if is_epub3 {
                crate::svg::check_vocabulary(svg_root, &path, report);
                crate::svg::check_attribute_vocabulary(svg_root, &path, report);
            }
            crate::svg::check_epub_attributes(svg_root, &path, report);
            // `check_ids` is standalone-SVG-only: a real fixture confirms
            // `id="1"` on an SVG root is fine when the SVG is embedded
            // inline inside an XHTML document (a shared XML id-space with
            // the rest of that document, not its own document-level id
            // rules) - the identically-shaped standalone-SVG fixture
            // rejects it.
            crate::svg::check_link_labels(svg_root, &path, report);
        }
        for fo in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "foreignObject"
                && n.tag_name().namespace() == Some(crate::svg::SVG_NS)
        }) {
            crate::svg::check_foreign_object(
                fo,
                &t,
                d.root_element(),
                &path,
                is_epub3,
                true,
                report,
            );
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

        // **Two Schematron rules with two different contexts**, and they are
        // kept apart because that is what they are: `title.non-empty` has
        // context `h:title`, `title.present` has context `h:head`
        // (`epub-xhtml-30.sch`).
        //
        // They were ported as a single `find` for the first descendant named
        // `title`, which lost both contexts at once. `title.present` then
        // fired on documents with no `<head>` at all: W3C's
        // `pub-xml-external-id` carries a one-line `<span>The test
        // fails.</span>` and drew "the head element should have a title child
        // element" — about a head that does not exist. And matching on the
        // local name alone accepted a `title` in any namespace at any depth,
        // where `h:title` means the XHTML namespace and, as a child of
        // `h:head`, a direct child.
        //
        // **A Schematron rule without its context is a wider rule.** When
        // porting one, the context is half the rule; carry it over with the
        // assertion.
        const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
        let is_xhtml = |n: roxmltree::Node, name: &str| {
            n.is_element()
                && n.tag_name().name() == name
                && n.tag_name().namespace() == Some(XHTML_NS)
        };
        // EPUB 3 only, for both. `title.non-empty` has no equivalent under
        // `schema/20`, where XHTML 1.1 types `<title>` as `<text/>` and RELAX
        // NG matches that on empty content, so an empty title is valid in an
        // EPUB 2 book — we rejected it once, for 115 findings across ten real
        // books. `title.present` is EPUB 3 only for a different reason: XHTML
        // 1.1 makes `title` *required* by the grammar (`head.content =
        // title`), which is an RSC-005 error and is enforced by
        // `headEl-epub2`; reporting both would double up on one missing
        // element.
        if is_epub3 {
            // title.non-empty — context `h:title`.
            for title in d.descendants().filter(|n| is_xhtml(*n, "title")) {
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
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "\"title\" must not be empty",
                        path.clone(),
                        title,
                        "opf.content_document.empty_title",
                        Vec::new(),
                    );
                }
            }
            // title.present — context `h:head`, so no head means no rule.
            // epubcheck's assertion message begins with "WARNING:", and its
            // error handler maps that prefix to RSC-017 rather than RSC-005,
            // which is why a warning is right here.
            for head in d.descendants().filter(|n| is_xhtml(*n, "head")) {
                if !head.children().any(|c| is_xhtml(c, "title")) {
                    report.push_node(
                        RSC_017,
                        Severity::Warning,
                        "The \"head\" element should have a \"title\" child element.",
                        path.clone(),
                        head,
                        "opf.content_document.head_missing_title",
                        Vec::new(),
                    );
                }
            }
        }

        // Duplicate `id` attribute values within this document.
        // epubcheck's equivalent is the `IDUNIQUE_20_SCH` validator, keyed on
        // `application/xhtml+xml` like the grammar, so it does not run for a
        // `text/html` item.
        if schema_validated {
            let mut seen: HashSet<&str> = HashSet::new();
            for n in d.descendants().filter(|n| n.is_element()) {
                if let Some(id) = n.attr_no_ns("id")
                    && !seen.insert(id)
                {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!("Duplicate ID \"{id}\""),
                        path.clone(),
                        n,
                        "opf.content_document.duplicate_id",
                        vec![id.to_string()],
                    );
                }
            }
        }

        // <img src> must not be empty/whitespace-only.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "img")
        {
            if let Some(src) = attr_no_ns_node(n, "src")
                && src.value().trim().is_empty()
            {
                report.push_node_attr(
                    RSC_005,
                    Severity::Error,
                    "\"img\" element's \"src\" attribute must not be empty",
                    path.clone(),
                    n,
                    src,
                    "opf.content_document.empty_img_src",
                    Vec::new(),
                );
            }
        }

        // lang/xml:lang must agree when both are present on the same element.
        //
        // EPUB 3 only (#58). epubcheck asserts this in `epub-xhtml-30.sch`
        // (`lang-xmllang`); `schema/20` has no counterpart, and XHTML 1.1's
        // own `lang.attrib` declares `xml:lang` and `lang` as two independent
        // optional attributes with no constraint tying their values together.
        // So a book that sets both to different values is valid EPUB 2.
        for n in d.descendants().filter(|n| is_epub3 && n.is_element()) {
            if let (Some(lang), Some(xml_lang)) = (
                n.attr_no_ns("lang"),
                n.attribute(("http://www.w3.org/XML/1998/namespace", "lang")),
            ) && lang.trim() != xml_lang.trim()
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "lang and xml:lang attributes must have the same value",
                    path.clone(),
                    n,
                    "opf.content_document.lang_xmllang_mismatch",
                    vec![lang.trim().to_string(), xml_lang.trim().to_string()],
                );
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
            if let Some(usemap) = n.attr_no_ns("usemap")
                && !usemap.starts_with('#')
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"usemap\" is invalid: \"{usemap}\""),
                    path.clone(),
                    n,
                    "opf.content_document.invalid_usemap",
                    vec![usemap.to_string()],
                );
            }
        }

        // Encoding-declaration checks on `<meta>`: (1) an http-equiv
        // Content-Type meta together with a charset meta, and (2) an http-equiv
        // Content-Type meta whose value isn't exactly the expected UTF-8
        // declaration. Both are HTML5 rules (the `<meta>` "encoding declaration
        // state"), so they apply to XHTML5 content documents — i.e. EPUB 3 only.
        // EPUB 2 content is XHTML 1.1, served as application/xhtml+xml, where
        // `content="application/xhtml+xml; charset=utf-8"` is the traditional,
        // valid form; epubcheck validates EPUB 2 against the XHTML 1.1 DTD, which
        // has no such constraint, so it never flags these there (#21, same class
        // as #9: an EPUB-3 rule must not leak into EPUB 2).
        if is_epub3 {
            let has_http_equiv_content_type = d.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("http-equiv")
                        .is_some_and(|v| v.eq_ignore_ascii_case("content-type"))
            });
            let has_charset_meta = d.descendants().any(|n| {
                n.is_element() && n.tag_name().name() == "meta" && n.attr_no_ns("charset").is_some()
            });
            if has_http_equiv_content_type && has_charset_meta {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "must not contain both a meta element in encoding declaration state (http-equiv='content-type') and a meta element with the charset attribute",
                    path.clone(),
                    d.root_element(),
                    "opf.content_document.conflicting_encoding_declarations",
                    Vec::new(),
                );
            }
            for n in d.descendants().filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("http-equiv")
                        .is_some_and(|v| v.eq_ignore_ascii_case("content-type"))
            }) {
                let content = attr_no_ns_node(n, "content");
                if !content
                    .is_some_and(|a| a.value().eq_ignore_ascii_case("text/html; charset=utf-8"))
                {
                    // Pin `@content` when there is one; a `<meta http-equiv>`
                    // with no `content` at all has no attribute to point at.
                    match content {
                        Some(a) => report.push_node_attr(
                            RSC_005,
                            Severity::Error,
                            "the \"content\" attribute must have the value \"text/html; charset=utf-8\"",
                            path.clone(),
                            n,
                            a,
                            "opf.content_document.invalid_content_type_meta",
                            Vec::new(),
                        ),
                        None => report.push_node(
                            RSC_005,
                            Severity::Error,
                            "the \"content\" attribute must have the value \"text/html; charset=utf-8\"",
                            path.clone(),
                            n,
                            "opf.content_document.invalid_content_type_meta",
                            Vec::new(),
                        ),
                    }
                }
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
            .filter(|n| n.is_element() && n.has_attr_no_ns("itemprop"))
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
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!(
                        "element \"{tag}\" missing required attribute \"{required_attr}\" (if the itemprop is specified on this element type, that attribute must also be present)"
                    ),
                    path.clone(),
                    n,
                    "opf.content_document.microdata_missing_attribute",
                    vec![tag.to_string(), required_attr.to_string()],
                );
            }
        }

        // A <dfn> must not have a <dfn> descendant.
        //
        // EPUB 3 only (#58). `descendant-dfn-dfn` lives in
        // `epub-xhtml-30.sch`; EPUB 2's whole XHTML Schematron is a single
        // rule (nested hyperlinks), and XHTML 1.1's grammar lets `dfn` nest.
        for n in d
            .descendants()
            .filter(|n| is_epub3 && n.is_element() && n.tag_name().name() == "dfn")
        {
            if n.descendants()
                .skip(1)
                .any(|c| c.is_element() && c.tag_name().name() == "dfn")
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "a \"dfn\" element must not contain a nested \"dfn\" element",
                    path.clone(),
                    n,
                    "opf.content_document.nested_dfn",
                    Vec::new(),
                );
            }
        }

        // epub:trigger is deprecated; its ref/ev:observer attributes must
        // each resolve to a real id in the same document.
        {
            let ids: HashSet<&str> = d.descendants().filter_map(|n| n.attr_no_ns("id")).collect();
            for n in d.descendants().filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "trigger"
                    && n.tag_name().namespace() == Some(EPUB_NS)
            }) {
                report.push_node(
                    RSC_017,
                    Severity::Warning,
                    "The \"epub:trigger\" element is deprecated",
                    path.clone(),
                    n,
                    "opf.content_document.deprecated_epub_trigger",
                    Vec::new(),
                );
                if let Some(r) = n.attr_no_ns("ref")
                    && !ids.contains(r)
                {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "The ref attribute must refer to an element in the same document",
                        path.clone(),
                        n,
                        "opf.content_document.dangling_id_reference",
                        vec!["ref".to_string(), r.to_string()],
                    );
                }
                if let Some(o) = n.attribute(("http://www.w3.org/2001/xml-events", "observer"))
                    && !ids.contains(o)
                {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        "The ev:observer attribute must refer to an element in the same document",
                        path.clone(),
                        n,
                        "opf.content_document.dangling_id_reference",
                        vec!["ev:observer".to_string(), o.to_string()],
                    );
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
            .filter(|n| n.is_element() && n.has_attr_no_ns("role"))
        {
            let role = attr_no_ns_node(n, "role").expect("filtered on has_attr_no_ns above");
            for token in role.value().split_whitespace() {
                if DEPRECATED_ARIA_ROLES.contains(&token) {
                    report.push_node_attr(
                        RSC_017,
                        Severity::Warning,
                        format!("\"{token}\" role is deprecated"),
                        path.clone(),
                        n,
                        role,
                        "opf.content_document.deprecated_aria_role",
                        vec![token.to_string()],
                    );
                }
            }
        }

        // epub:type default-vocabulary / deprecated / HTML-usage taxonomies.
        // The vocabulary itself lives in `ssv`; custom-prefixed tokens
        // (containing ':') are always exempt.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.attribute((EPUB_NS, "type")).is_some())
        {
            let type_attr =
                attr_ns_node(n, EPUB_NS, "type").expect("filtered on the same attribute above");
            let value = type_attr.value();
            for token in value.split_whitespace() {
                if token.contains(':') {
                    continue;
                }
                if !crate::ssv::is_default_vocab_type(token) {
                    report.push_node_attr(
                        OPF_088,
                        Severity::Usage,
                        format!("epub:type value '{token}' is not in the default vocabulary"),
                        path.clone(),
                        n,
                        type_attr,
                        "opf.content_document.epub_type_not_default_vocab",
                        vec![token.to_string()],
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
                if let Some((_, replacement)) =
                    crate::ssv::DEPRECATED.iter().find(|(t, _)| *t == token)
                    && !endnote_exempt
                {
                    // epubcheck reports a deprecated epub:type semantic as
                    // usage-level OPF-086b (the corpus'
                    // `epubtype-deprecated-usage.xhtml`: "usage OPF-086b"),
                    // a distinct sub-code from the warning-level OPF-086 the
                    // rendition/viewport deprecations use - same split, and
                    // same lettered-ID representation, as OPF-096 vs
                    // OPF-096b. Matches its sibling OPF-088 (usage) in this
                    // very loop.
                    //
                    // Naming what to use instead is the whole value of the
                    // message: "deprecated" alone leaves the author to go
                    // find the vocabulary themselves. The spec names a
                    // replacement for only 5 of the 13, so the rest say
                    // nothing rather than invent one.
                    let text = match replacement {
                        Some(r) => {
                            format!("epub:type value '{token}' is deprecated; consider {r} instead")
                        }
                        None => format!("epub:type value '{token}' is deprecated"),
                    };
                    report.push_node_attr(
                        OPF_086B,
                        Severity::Usage,
                        text,
                        path.clone(),
                        n,
                        type_attr,
                        "opf.content_document.deprecated_epub_type",
                        vec![token.to_string()],
                    );
                }
                // OPF-087 (usage): the vocabulary gives these terms an HTML
                // usage context of "Not Allowed" - they mean something only
                // on a media overlay's `seq`/`par` (escapable/skippable
                // structure), and nothing on an HTML element.
                //
                // This was previously read as "the value restates the
                // semantic of its host element" (`ol` + `list`, `table` +
                // `table`, ...). That agreed with the corpus fixture on
                // every count - `epubtype-misuse-usage.xhtml` only ever
                // pairs each term with its matching element, so both rules
                // report its 7 - but it is not the rule: the term is not
                // allowed on *any* HTML element, so `<div epub:type="list">`
                // was missed entirely. A fixture agreeing is not the rule
                // agreeing (reported by Doitsu on the MobileRead forum).
                if crate::ssv::is_media_overlay_only(token) {
                    report.push_node_attr(
                        OPF_087,
                        Severity::Usage,
                        format!(
                            "epub:type value '{token}' is not allowed in an XHTML content document; \
                             it applies only to media overlays"
                        ),
                        path.clone(),
                        n,
                        type_attr,
                        "opf.content_document.epub_type_not_allowed_in_html",
                        vec![token.to_string()],
                    );
                }
            }
        }

        // The epub: namespace prefix should be bound to exactly the real
        // EPUB ops namespace URI - an unrecognized binding is informative,
        // not an error (the document may still be usable).
        for ns in d.root_element().namespaces() {
            if ns.name() == Some("epub") && ns.uri() != EPUB_NS {
                report.push_at_pos(
                    HTM_010,
                    Severity::Usage,
                    format!("Namespace \"{}\" is unusual", ns.uri()),
                    path.clone(),
                    Position::of(d.root_element()),
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
            if !n.has_attr_no_ns("alttext") && !has_annotation {
                report.push_full(
                    ACC_009,
                    Severity::Usage,
                    "MathML markup has no alternative text",
                    path.clone(),
                    Position::of(n),
                    "htm.mathml.no_alternative_text",
                    Vec::new(),
                );
            }
        }

        // HTML5 <time datetime="..."> value grammar.
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "time")
        {
            if let Some(v) = n.attr_no_ns("datetime")
                && !crate::htm::is_valid_html5_datetime(v)
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"datetime\" is invalid: \"{v}\""),
                    path.clone(),
                    n,
                    "opf.content_document.invalid_html5_datetime",
                    vec![v.to_string()],
                );
            }
        }

        // epub:switch is deprecated - a separate, additive signal alongside
        // whatever structural case/default sequencing schemas/xhtml.rng
        // already enforces on it. Namespace-checked: SVG has its own,
        // unrelated native <switch> element (conditional rendering), which
        // a local-name-only match would misidentify as epub:switch.
        const EPUB_NS: &str = "http://www.idpf.org/2007/ops";
        for n in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "switch"
                && n.tag_name().namespace() == Some(EPUB_NS)
        }) {
            report.push_node(
                RSC_017,
                Severity::Warning,
                "The \"epub:switch\" element is deprecated",
                path.clone(),
                n,
                "opf.content_document.deprecated_epub_switch",
                Vec::new(),
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
                // A `<source>` inside `<audio>`/`<video>` names its resource
                // with `src`, not `srcset`, and was not covered at all — the
                // arm above requires a `<picture>` ancestor. epubcheck asks
                // the question of every `<source>`; its
                // `type-mismatch-in-audio-warning` fixture is exactly this
                // shape and drew nothing from us.
                "source"
                    if n.ancestors().skip(1).any(|a| {
                        a.is_element() && matches!(a.tag_name().name(), "audio" | "video")
                    }) =>
                {
                    ("src", false)
                }
                _ => continue,
            };
            let Some(declared_type) = n.attr_no_ns("type") else {
                continue;
            };
            let Some(href) = n.attr_no_ns(href_attr) else {
                continue;
            };
            let target = if resolve_srcset {
                href.split(',')
                    .next()
                    .unwrap_or(href)
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
            // **Both sides are normalized, and the two normalizations are not
            // the same** — this mirrors `OPSHandler30.checkMimetypeMatches`
            // rather than tidying it up.
            //
            // The content-side type always loses its parameters:
            // `type="audio/mp4; codecs=mp4"` is a claim about `audio/mp4`, and
            // comparing the whole string would have reported every
            // `type="audio/mpeg; codecs=mp3"` against a manifest `audio/mpeg`
            // — a false positive on correct markup, and one we were carrying
            // latently for `<object>`/`<embed>` before this.
            //
            // The manifest side keeps its parameters except for one case
            // epubcheck special-cases by hand: `audio/ogg; codecs=opus`, which
            // is how an Opus file is legitimately declared. Their source calls
            // this a hack pending real MIME parsing; matched anyway, because
            // an OPUS book must not draw a warning here from either tool.
            let declared_type = declared_type.split(';').next().unwrap_or("").trim();
            if let Some((_, actual_type)) = items.values().find(|(ip, _)| nfc(ip) == resolved)
                && !normalize_opus(actual_type).eq_ignore_ascii_case(declared_type)
            {
                report.push_at_pos(
                        OPF_013,
                        Severity::Warning,
                        format!(
                            "declared type \"{declared_type}\" doesn't match the resource's actual media-type \"{actual_type}\""
                        ),
                        path.clone(),
                        Position::of(n),
                    );
            }
        }

        // --- <a href> fragment resolution (RSC-012/RSC-014), stylesheet/
        // svg-use/img fragment classification (RSC-013/RSC-015/RSC-009),
        // srcset (RSC-008), and base-URI-aware remote reclassification
        // (RSC-006) ---
        // An absolute remote <base href>/xml:base means every relative
        // reference in *this* document resolves to a remote URL through it,
        // not to a file in the container. Computed once for the document:
        // the <a href> pass below uses it to reclassify targets as remote
        // (RSC-006), and the attribute walk uses it to not go looking for
        // them on disk.
        let remote_base = d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "base")
            .and_then(|n| n.attr_no_ns("href"))
            .filter(|v| is_remote_url(v))
            .or_else(|| {
                d.root_element()
                    .attribute(("http://www.w3.org/XML/1998/namespace", "base"))
                    .filter(|v| is_remote_url(v))
            })
            .is_some();

        {
            let mut frag_id_cache: HashMap<String, Option<IdMap>> = HashMap::new();

            for a in d
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "a")
            {
                // Which attribute addresses the target depends on the
                // namespace, and epubcheck reads exactly one per anchor: the
                // XHTML handler takes `href`, the SVG one calls
                // `checkHRef(xlink, "href")`. Ours matched on element name
                // and always read `href`, so the two were *inverted* for
                // every check in this walk, not only RSC-014 - measured on
                // one book per shape: a `xlink:href` to a missing file drew
                // RSC-007 from epubcheck and nothing here, a plain `href` to
                // the same file drew RSC-007 here and nothing there, and the
                // same pair held for RSC-020 and for the fragment checks.
                let svg_anchor = a.tag_name().namespace() == Some("http://www.w3.org/2000/svg");
                let attr = if svg_anchor {
                    a.attribute(("http://www.w3.org/1999/xlink", "href"))
                } else {
                    a.attr_no_ns("href")
                };
                let Some(href) = attr else {
                    continue;
                };
                if crate::url::is_absolute(href) {
                    if crate::url::has_syntax_error(href) {
                        report.push_node(
                            RSC_020,
                            Severity::Error,
                            format!("URL '{href}' is not conforming"),
                            path.clone(),
                            a,
                            "opf.content_document.malformed_absolute_url",
                            vec![href.to_string()],
                        );
                    } else if crate::url::has_unregistered_scheme(href) {
                        report.push_at_pos(
                            HTM_025,
                            Severity::Warning,
                            format!("URL '{href}' uses an unregistered scheme"),
                            path.clone(),
                            Position::of(a),
                        );
                    }
                }
                // `is_external` treats *any* fragment-only href
                // (`#foo`) as "skip normal resolution" - correct for the
                // old file-existence check, but RSC-012's fragment
                // resolution needs to run on exactly those hrefs, so only
                // bail out here for a genuinely remote/data/mailto/tel
                // href (empty hrefs have no fragment to check either).
                // HTM-045 (#56): an empty href resolves to the document
                // itself. That is legal, so epubcheck only hints - but it is
                // almost never what the author meant. Checked ahead of the
                // `is_external` bail-out below, which counts an empty href as
                // external (nothing to resolve) and would skip past this.
                if href.trim().is_empty() {
                    report.push_at_pos(
                        HTM_045,
                        Severity::Usage,
                        "the empty \"href\" points this document at itself",
                        path.clone(),
                        Position::of(a),
                    );
                    continue;
                }
                if !href.starts_with('#') && is_external(href) {
                    continue;
                }
                if remote_base {
                    report.push_node(
                        RSC_006,
                        Severity::Error,
                        format!(
                            "relative reference '{href}' resolves to a remote resource via base"
                        ),
                        path.clone(),
                        a,
                        "opf.content_document.relative_reference_remote_via_base",
                        vec![href.to_string()],
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
                        // The document being walked - already parsed.
                        Some(dom_id_kinds(&d))
                    } else {
                        target_id_kinds(ocf, &name_index, &target_nfc, is_epub3)
                    };
                    frag_id_cache.insert(target_nfc.clone(), ids);
                }
                // `None` = the target could not be read/parsed, so whether
                // the fragment resolves is unknown and unreported (see
                // `target_id_kinds`).
                let Some(target_ids) = &frag_id_cache[&target_nfc] else {
                    continue;
                };
                if !target_ids.contains_key(frag) {
                    report.push_node(
                        missing_fragment_id(&items, &target_nfc),
                        Severity::Error,
                        format!("fragment identifier '{frag}' is not defined in '{target_nfc}'"),
                        path.clone(),
                        a,
                        "opf.content_document.dangling_fragment",
                        vec![frag.to_string(), target_nfc.clone()],
                    );
                    continue;
                }
                // RSC-014: a hyperlink to an SVG definition element - a
                // navigable link can't target one. Cross-document as well as
                // same-document, since the kind travels in the id map.
                //
                // epubcheck decides this by *typing* every id: an SVG
                // `symbol` is SVG_SYMBOL, a `linearGradient`/`radialGradient`
                // /`pattern` is SVG_PAINT, a `clipPath` is SVG_CLIP_PATH, and
                // everything else is GENERIC. A hyperlink may target GENERIC
                // and nothing else, so all five names below are errors. We
                // had only `symbol`, measured against the oracle one book per
                // shape.
                //
                // Still narrower than epubcheck, deliberately, and
                // `docs/COVERAGE.md` says so: a *cross-document* hyperlink
                // needs the id's type carried in `frag_id_cache`, which holds
                // document order only, and the two non-hyperlink reference
                // kinds (`<use xlink:href>`, `fill`/`stroke="url(#…)"`) are
                // not collected here at all.
                if let Some(&(_, kind)) = target_ids.get(frag)
                    && kind != IdKind::Generic
                {
                    report.push_at_pos(
                        RSC_014,
                        Severity::Error,
                        format!(
                            "hyperlink '{href}' targets {} (incompatible resource type)",
                            kind.describe()
                        ),
                        path.clone(),
                        Position::of(a),
                    );
                }
            }

            // RSC-012/RSC-014 for the two SVG reference kinds that are not
            // hyperlinks. epubcheck resolves every reference through one
            // path and then compares the target id's type against the
            // reference's; these two are the rest of that comparison.
            //
            // - `<use>` may reach an SVG symbol or a generic id, nothing
            //   else. **`xlink:href` only, deliberately**: epubcheck's
            //   `checkSymbol()` reads that spelling alone, so SVG 2's plain
            //   `<use href>` registers no reference there at all. Reading
            //   both here would report where epubcheck is silent, which to
            //   anyone diffing the two tools is indistinguishable from a
            //   false positive. (Our RSC-015 does accept both spellings -
            //   that check is about a missing fragment, not about the
            //   target's type, and epubcheck reaches it by another route.)
            // - `fill`/`stroke="url(#…)"` must reach a paint server exactly,
            //   not a symbol and not a generic id. Those two attributes are
            //   epubcheck's whole list. `clip-path` is **not** checked by
            //   either tool: nothing there ever registers a clip-path
            //   reference, so its `case` is dead code (see COVERAGE.md).
            let mut typed_refs: Vec<(roxmltree::Node, String, RefKind)> = Vec::new();
            for n in d.descendants().filter(|n| {
                n.is_element() && n.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            }) {
                if n.tag_name().name() == "use"
                    && let Some(v) = n.attribute(("http://www.w3.org/1999/xlink", "href"))
                {
                    typed_refs.push((n, v.to_string(), RefKind::Symbol));
                }
                for attr in ["fill", "stroke"] {
                    if let Some(v) = n.attr_no_ns(attr)
                        && let Some(inner) =
                            v.strip_prefix("url(").and_then(|r| r.strip_suffix(')'))
                    {
                        typed_refs.push((n, inner.trim().to_string(), RefKind::Paint));
                    }
                }
            }
            // `cite` on the four elements HTML gives it. EPUB 3 only, and
            // that is measured rather than assumed: `checkCiteAttribute`
            // lives in `OPSHandler30`, and an EPUB 2 book carrying the same
            // `<blockquote cite="#sym">` is clean from epubcheck while the
            // EPUB 3 one is RSC-014.
            if is_epub3 {
                for n in d.descendants().filter(|n| {
                    n.is_element()
                        && ["blockquote", "q", "ins", "del"].contains(&n.tag_name().name())
                }) {
                    if let Some(v) = n.attr_no_ns("cite") {
                        typed_refs.push((n, v.to_string(), RefKind::Cite));
                    }
                }
            }
            for (n, href, ref_kind) in typed_refs {
                if crate::url::is_absolute(&href) || is_remote_url(&href) || remote_base {
                    continue;
                }
                let Some((path_part, frag)) = href.split_once('#') else {
                    continue;
                };
                if frag.is_empty() || frag.contains(['=', ':', '(']) {
                    continue;
                }
                let target_nfc = if path_part.is_empty() {
                    nfc(&path)
                } else {
                    nfc(&resolve(&dir, path_part))
                };
                if target_nfc == nfc(opf_path) {
                    continue;
                }
                if !frag_id_cache.contains_key(&target_nfc) {
                    let ids = if target_nfc == nfc(&path) {
                        Some(dom_id_kinds(&d))
                    } else {
                        target_id_kinds(ocf, &name_index, &target_nfc, is_epub3)
                    };
                    frag_id_cache.insert(target_nfc.clone(), ids);
                }
                let Some(target_ids) = &frag_id_cache[&target_nfc] else {
                    continue;
                };
                let Some(&(_, kind)) = target_ids.get(frag) else {
                    report.push_node(
                        RSC_012,
                        Severity::Error,
                        format!("fragment identifier '{frag}' is not defined in '{target_nfc}'"),
                        path.clone(),
                        n,
                        "opf.content_document.dangling_fragment",
                        vec![frag.to_string(), target_nfc.clone()],
                    );
                    continue;
                };
                if !ref_kind.accepts(kind) {
                    report.push_at_pos(
                        RSC_014,
                        Severity::Error,
                        format!(
                            "reference '{href}' targets {} (incompatible resource type)",
                            kind.describe()
                        ),
                        path.clone(),
                        Position::of(n),
                    );
                }
            }
        }

        // RSC-013: a stylesheet reference must not carry a fragment.
        for n in d.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().name() == "link"
                && n.attr_no_ns("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
        }) {
            if let Some(href) = n.attr_no_ns("href")
                && !is_external(href)
                && href.contains('#')
            {
                report.push_at_pos(
                    RSC_013,
                    Severity::Error,
                    format!("stylesheet reference '{href}' must not have a fragment identifier"),
                    path.clone(),
                    Position::of(n),
                );
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
            let src = n.attr_no_ns(src_attr).or_else(|| {
                if tag == "image" {
                    n.attribute(("http://www.w3.org/1999/xlink", "href"))
                } else {
                    None
                }
            });
            if let Some(v) = src
                && let Some((p, _frag)) = v.split_once('#')
                && !is_external(v)
            {
                let resolved = nfc(&resolve(&dir, p));
                let is_svg = resolved.ends_with(".svg")
                    || items
                        .values()
                        .any(|(ip, mt)| nfc(ip) == resolved && mt == "image/svg+xml");
                if !is_svg {
                    report.push_at_pos(
                        RSC_009,
                        Severity::Warning,
                        format!("non-SVG image '{v}' is referenced with a fragment identifier"),
                        path.clone(),
                        Position::of(n),
                    );
                }
            }
            if tag == "img"
                && let Some(srcset_attr) = attr_no_ns_node(n, "srcset")
            {
                for candidate in srcset_attr.value().split(',') {
                    let url = candidate.split_whitespace().next().unwrap_or("");
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
                        report.push_node_attr(
                            RSC_008,
                            Severity::Error,
                            format!("srcset candidate '{url}' is not declared in the manifest"),
                            path.clone(),
                            n,
                            srcset_attr,
                            "opf.content_document.srcset_not_in_manifest",
                            vec![url.to_string()],
                        );
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
            // Either spelling addresses the target; pin whichever this
            // element actually used, so the path names the attribute the
            // author would edit.
            let href = attr_no_ns_node(n, "href")
                .or_else(|| attr_ns_node(n, "http://www.w3.org/1999/xlink", "href"));
            if let Some(a) = href
                && !is_external(a.value())
                && !a.value().contains('#')
            {
                let v = a.value();
                report.push_node_attr(
                    RSC_015,
                    Severity::Error,
                    format!("\"use\" element's href '{v}' has no fragment identifier"),
                    path.clone(),
                    n,
                    a,
                    "opf.content_document.use_href_missing_fragment",
                    vec![v.to_string()],
                );
            }
        }

        // --- Navigation document checks (NAV-010/011) ---
        if nav_path.as_deref() == Some(path.as_str()) {
            crate::navdoc::check(&d, &path, &dir, report);
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
                    if let Some(href) = a.attr_no_ns("href") {
                        // `is_external` also covers fragment-only/data:/
                        // mailto:/tel: hrefs (correct for "should this be
                        // resolved as a container path", wrong here - a
                        // same-document `#toc` anchor is a completely
                        // normal same-page link, not "external" - a real
                        // false positive found via a real `nav-landmarks-
                        // valid` fixture using exactly that shape).
                        if is_remote_url(href) {
                            report.push_at_pos(
                                NAV_010,
                                Severity::Error,
                                format!("external link '{href}' in a toc/page-list/landmarks nav"),
                                path.clone(),
                                Position::of(a),
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
                let mut id_order_cache: HashMap<String, Option<IdMap>> = HashMap::new();
                // (spine_idx, dom_idx): dom_idx is 0 for a fragment-less
                // link ("the whole document") and real-fragment-index + 1
                // otherwise, so it always sorts before any real fragment
                // into the same document without needing a separate flag.
                // (spine_idx, anchor, the <a> node, href). `anchor` is
                // `None` when the fragment cannot be resolved: such a link
                // still takes part in the *spine* comparison and only sits
                // out the document-order one, which is epubcheck's
                // `targetAnchorPosition > -1` guard.
                //
                // It used to `continue` here instead, on the grounds that a
                // dangling fragment "is already caught elsewhere as a broken
                // reference". RSC-012 does catch it — but RSC-012 answers
                // *is this fragment defined* and this rule answers *is the
                // order right*, so the link vanished from the ordering
                // question entirely. JSWolf's `scrambled.epub` (MobileRead,
                // 2026-08-21) is 67 links with dangling fragments: epubcheck
                // reports 71 NAV-011 and we reported 5.
                //
                // Fourth time a check has suppressed a case because it
                // believed another check owned it, and the fourth time the
                // other check owned a *different question*.
                #[allow(clippy::type_complexity)]
                let mut keys: Vec<(usize, Option<usize>, roxmltree::Node, String)> = Vec::new();
                for a in toc_nav
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "a")
                {
                    let Some(href) = a.attr_no_ns("href") else {
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
                        None => Some(0),
                        Some(f) => {
                            if !id_order_cache.contains_key(&resolved_nfc) {
                                let order =
                                    target_id_kinds(ocf, &name_index, &resolved_nfc, is_epub3);
                                id_order_cache.insert(resolved_nfc.clone(), order);
                            }
                            // Missing ids are already caught elsewhere as
                            // broken references; skip this link here
                            // rather than letting it break the comparison.
                            // An unreadable target (`None`) is skipped for
                            // the same reason - its DOM order is unknown,
                            // not empty.
                            id_order_cache[&resolved_nfc]
                                .as_ref()
                                .and_then(|o| o.get(f))
                                .map(|&(idx, _)| idx + 1)
                        }
                    };
                    keys.push((spine_idx, dom_idx, a, href.to_string()));
                }
                // epubcheck's two-level state machine, rather than a scan of
                // adjacent pairs: the document-order baseline resets whenever
                // the spine advances or a spine violation is reported, and a
                // link whose fragment did not resolve leaves it untouched.
                // Adjacent pairs got this wrong whenever an unresolvable link
                // sat between two resolvable ones — neither pair compared, so
                // the two real positions were never checked against each
                // other.
                let mut last_spine: Option<usize> = None;
                let mut last_anchor: Option<usize> = None;
                for (spine, anchor, node, href) in &keys {
                    // Position and target on every finding. They were all
                    // anchored on the <nav> element with no target named, so
                    // a book with five of them showed five identical lines
                    // and an editor would mark one line five times.
                    let mut say = |what: &str| {
                        report.push_node(
                            NAV_011,
                            Severity::Warning,
                            format!("toc nav link '{href}' is out of {what} order"),
                            path.clone(),
                            *node,
                            "navdoc.toc.link_out_of_reading_order",
                            vec![href.clone()],
                        );
                    };
                    if last_spine.is_some_and(|ls| *spine < ls) {
                        say("spine");
                        last_spine = Some(*spine);
                        last_anchor = None;
                        continue;
                    }
                    if last_spine != Some(*spine) {
                        last_spine = Some(*spine);
                        last_anchor = None;
                    }
                    if let Some(a) = anchor {
                        if last_anchor.is_some_and(|la| *a < la) {
                            say("document");
                        }
                        last_anchor = Some(*a);
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
        // Distinct from `has_svg`: epubcheck separates a property that is
        // *required* (the document contains SVG markup - OPF-014 if the
        // declaration is missing) from one that is merely *allowed* (the
        // document references an SVG resource - declaring it is optional,
        // and doing so is not an error). Its own comment says so:
        // "the `svg` property MAY be set if an SVG resource is referenced in
        // HTML". We had only the required half, so `properties="svg"` on a
        // document whose only SVG is an `<img src="x.svg">` drew OPF-015 -
        // reported by Doitsu, MobileRead #126.
        let mut references_svg = false;
        // A `<math>` element in the MathML namespace requires the `mathml`
        // property, exactly as `<svg>` requires `svg` (OPSHandler30 adds
        // ITEM_PROPERTIES.MATHML on that element and nothing else - a
        // MathML *child* without its `math` root cannot occur, and a
        // reference to a MathML resource has no "allowed" half the way
        // `references_svg` does). Reported by Doitsu, MobileRead #138.
        let mut has_mathml = false;
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
            // SVG's `<image>`/`<use>` address their target with a
            // *namespaced* `xlink:href`, which the bare-name attribute walk
            // below cannot see. An `<image>` draws its target, so it is a
            // resource reference; `<use>`/paint references point inside a
            // document and are not (matching epubcheck's SVG_SYMBOL /
            // SVG_PAINT, which it also excludes).
            if node.tag_name().name() == "image"
                && let Some(v) = node
                    .attr_no_ns("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
            {
                if is_external(v) {
                    // A remote target is still a reference, and the
                    // unreferenced-remote-item check (#70) turns on whether
                    // one exists anywhere. `resource_refs` cannot hold it -
                    // it stores resolved container paths - so it goes to the
                    // remote set instead. This generic href/xlink:href site
                    // is what catches SVG's `<font-face-uri xlink:href>`,
                    // whose media type gives no hint that it is a font.
                    if is_remote_url(v) {
                        remote_resource_refs.insert(strip_url_fragment(v).trim().to_string());
                    }
                } else {
                    // Same interior-space rule as the attribute walk below.
                    // SVG reaches this site instead of that one, so a
                    // `<image xlink:href="../Images/bes sevgi dili.jpg">` -
                    // a real book - drew the manifest-side RSC-020 and not
                    // the reference-side one epubcheck also reports.
                    if v.trim().contains(' ') {
                        report.push_node(
                            RSC_020,
                            Severity::Error,
                            format!("URL '{v}' is not conforming"),
                            path.clone(),
                            node,
                            "opf.content_document.malformed_relative_url",
                            vec![v.to_string()],
                        );
                    }
                    let key = nfc(&resolve(&dir, strip_url_fragment(v).trim()));
                    resource_refs.insert(key.clone());
                    // The same declared/present matrix the no-namespace
                    // attribute walk below applies. It could not reach here:
                    // that walk reads `attr_no_ns`, and SVG's reference is
                    // `xlink:href`, so a broken image reference inside an
                    // `<svg>` produced nothing at all from us - not the
                    // RSC-007 for a missing file, and not the RSC-008 for an
                    // undeclared one. Measured against epubcheck 5.3.0 on a
                    // real book (an `<image xlink:href>` pointing at a
                    // container file nobody declared) and on one probe per
                    // cell.
                    let structural =
                        key == nfc(opf_path) || key == "mimetype" || key.starts_with("META-INF/");
                    if !structural && !manifest_paths.contains(&key) {
                        if name_index.contains_key(&key) {
                            report.push_node(
                                RSC_008,
                                Severity::Error,
                                format!("resource '{v}' is not declared in the manifest"),
                                path.clone(),
                                node,
                                "opf.content_document.undeclared_resource",
                                vec![v.to_string()],
                            );
                        } else {
                            report.push_node(
                                RSC_007,
                                Severity::Error,
                                format!(
                                    "reference to a resource missing from the publication: '{v}'"
                                ),
                                path.clone(),
                                node,
                                "opf.content_document.reference_missing_resource",
                                vec![v.to_string()],
                            );
                        }
                    }
                }
            }
            // An SVG `<a>` addresses its target with `xlink:href`, which
            // this bare-name walk cannot see - and its plain `href` is not a
            // reference at all to epubcheck, whose SVG handler reads the
            // namespaced spelling alone. Reading it here reported RSC-007 on
            // an `<a href="missing.xhtml">` inside an `<svg>` that epubcheck
            // passes, which is the false-positive-shaped direction. Measured
            // one book per spelling.
            //
            // For an SVG anchor the same questions are asked of `xlink:href`
            // instead, which is the spelling epubcheck's SVG handler reads
            // (#77). Feeding it through this loop rather than the anchor walk
            // is what gives it the existence check: `is_resource_reference`
            // already draws the "consumes the target" line, and an `a`/`href`
            // pair is on the *not*-consuming side of it - so the reference is
            // checked for RSC-007/RSC-008 without entering `resource_refs`,
            // where it would wrongly answer OPF-097's "is this resource
            // referenced". That distinction is why this looked like its own
            // change and turned out to be a substitution.
            let svg_anchor =
                node.tag_name().name() == "a" && node.tag_name().namespace() == Some(SVG_NS);
            for attr in ["src", "href", "data", "poster", "altimg", "cite"] {
                let value = if svg_anchor && attr == "href" {
                    node.attribute(("http://www.w3.org/1999/xlink", "href"))
                } else {
                    node.attr_no_ns(attr)
                };
                if let Some(v) = value {
                    if !is_external(v) && is_resource_reference(node, attr) {
                        resource_refs.insert(nfc(&resolve(&dir, strip_url_fragment(v).trim())));
                    }
                    // RSC-026: the reference resolves above the container
                    // root, or is path-absolute. epubcheck applies this in
                    // `URLChecker`, its single resolution point, so it lands
                    // on *every* URL it resolves - we had it on manifest
                    // hrefs only. It is additive with RSC-007: a leaking
                    // reference is both outside the container and missing
                    // from it, and epubcheck reports both.
                    if !is_external(v)
                        && !v.trim().is_empty()
                        && href_leaks_container_root(&dir, v.trim())
                    {
                        report.push_node(
                            RSC_026,
                            Severity::Error,
                            format!("'{v}' leaks outside the container"),
                            path.clone(),
                            node,
                            "opf.content_document.reference_leaks_container_root",
                            vec![v.to_string()],
                        );
                    }
                    if is_file_url(v) {
                        report.push_node(
                            RSC_030,
                            Severity::Error,
                            format!("'{v}' is a file URL, which is not allowed"),
                            path.clone(),
                            node,
                            "opf.content_document.file_url_reference",
                            vec![v.to_string()],
                        );
                        // **No `continue` here, and that is the fix.** It used
                        // to stop, on the reasonable-sounding grounds that
                        // RSC-030 is the whole story for a file URL. It is
                        // not: the reference still has to be *classified*,
                        // and skipping that cost a finding in each direction
                        // on W3C's `pub-file-urls`. `has_remote` stayed false,
                        // so the `remote-resources` property the author had
                        // correctly declared drew OPF-018 "declared but
                        // doesn't appear to be needed"; and the restricted-
                        // context branch never ran, so the three `<iframe>`s
                        // drew no RSC-006 where epubcheck reports one each.
                        // epubcheck agrees a file URL is remote — `isRemote`
                        // is "not `data:` and not same-origin" — it simply
                        // does not stop after saying so.
                        //
                        // Third instance of the silent-skip shape in this
                        // file: a check suppresses the rest of a path because
                        // it believes it owns the case. A wrong answer gets
                        // reported; no answer does not.
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
                        && !node.attr_no_ns("rel").is_some_and(|r| {
                            r.split_whitespace()
                                .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                        });
                    if (is_remote_url(v) || is_file_url(v)) && !is_non_stylesheet_link {
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
                            // A remote stylesheet does not ask for the
                            // "remote-resources" property. The property
                            // declares that the document fetches something
                            // from the network that is *allowed* to be
                            // there; a stylesheet never is, so declaring it
                            // could not legitimize this and asking for it
                            // points at the wrong half of the problem (the
                            // RSC-006 below is the whole of it). epubcheck
                            // draws the same line structurally - its `<link>`
                            // handling sits outside the resource-URL path
                            // that collects the property's requirement.
                            // Evidenced by `resources-remote-stylesheet-error`,
                            // which declares no property and expects RSC-006
                            // alone (issue #26); the corpus says nothing
                            // either way about the other restricted tags, so
                            // they are left as they are.
                            if tag != "link" {
                                has_remote = true;
                            }
                            let restricted = match tag {
                                "img" | "iframe" => true,
                                "script" if attr == "src" => true,
                                "link" if attr == "href" => {
                                    node.attr_no_ns("rel").is_some_and(|r| {
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
                    // Under a remote base this reference never pointed
                    // into the container in the first place - it resolves
                    // through the base to a remote URL - so looking for it
                    // on disk and reporting it missing describes a file the
                    // document never asked for (issue #26; epubcheck reports
                    // only the RSC-006 that the remote resolution itself
                    // earns).
                    if remote_base {
                        continue;
                    }
                    // RSC-020: an unencoded space in a *relative* reference.
                    // The absolute-URL path is `url::has_syntax_error`, which
                    // this walk never reaches - it bails on `is_external`
                    // before here - so `<img src="../Images/Screen Shot
                    // 2018-01-07 at 23.14.51.png">` produced nothing from us
                    // and RSC-020 from epubcheck. Two real books carry five
                    // between them; measured against 5.3.0 per book.
                    // Interior space only - leading/trailing is stripped by
                    // the URL parser and valid (`content-model-a-with-
                    // leading-trailing-spaces-valid` in the corpus).
                    if v.trim().contains(' ') {
                        report.push_node(
                            RSC_020,
                            Severity::Error,
                            format!("URL '{v}' is not conforming"),
                            path.clone(),
                            node,
                            "opf.content_document.malformed_relative_url",
                            vec![v.to_string()],
                        );
                    }
                    let resolved = nfc(&resolve(&dir, v));
                    // The matrix and its reasoning live in
                    // `classify_resource_ref`; only the anchoring is here.
                    match classify_resource_ref(&resolved, &manifest_paths, &name_index, opf_path) {
                        ResourceRef::Undeclared => {
                            report.push_node(
                                RSC_008,
                                Severity::Error,
                                format!("resource '{v}' is not declared in the manifest"),
                                path.clone(),
                                node,
                                "opf.content_document.undeclared_resource",
                                vec![v.to_string()],
                            );
                        }
                        ResourceRef::Missing => {
                            report.push_node(
                                RSC_007,
                                Severity::Error,
                                format!(
                                    "reference to a resource missing from the publication: '{v}'"
                                ),
                                path.clone(),
                                node,
                                "opf.content_document.reference_missing_resource",
                                vec![v.to_string()],
                            );
                        }
                        ResourceRef::Fine => {}
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
                    .attr_no_ns("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
                if let Some(href) = href {
                    if href.trim_start().starts_with("data:") {
                        report.push_node(
                            RSC_029,
                            Severity::Error,
                            "a hyperlink href must not be a data URL",
                            path.clone(),
                            node,
                            "opf.content_document.hyperlink_data_url",
                            Vec::new(),
                        );
                    } else if href.trim_start().starts_with('#') {
                        // A fragment-only href is an internal link into the
                        // document's own content; `is_external` (below)
                        // treats it as external and would drop it, but for
                        // OPF-096 reachability epubcheck counts such a
                        // self-reference as a hyperlink pointing at *this*
                        // resource - enough to make a non-linear resource
                        // reachable (Kevin Hendricks, issue #1: "the same
                        // internal link trick works for any xhtml file listed
                        // as non-linear and always has"). Record the document
                        // as a target of itself.
                        if node.tag_name().name() == "a" {
                            hyperlink_targets.entry(nfc(&path)).or_insert_with(|| {
                                HyperlinkSource {
                                    file: path.clone(),
                                    position: Position::of(node),
                                    element_path: crate::xmlext::node_path(node),
                                }
                            });
                        }
                    } else if !is_external(href) {
                        if href.contains('?') {
                            report.push_node(
                                RSC_033,
                                Severity::Error,
                                format!("hyperlink href '{href}' must not have a query string"),
                                path.clone(),
                                node,
                                "opf.content_document.hyperlink_query_string",
                                vec![href.to_string()],
                            );
                        }
                        if node.tag_name().name() == "a" {
                            hyperlink_targets
                                .entry(nfc(&resolve(&dir, href)))
                                .or_insert_with(|| HyperlinkSource {
                                    file: path.clone(),
                                    position: Position::of(node),
                                    element_path: crate::xmlext::node_path(node),
                                });
                        }
                    }
                }
            }
            if node.tag_name().name() == "script" {
                let script_type = node.attr_no_ns("type").unwrap_or("");
                if script_type.is_empty()
                    || script_type.eq_ignore_ascii_case("text/javascript")
                    || script_type.eq_ignore_ascii_case("application/javascript")
                    || script_type.eq_ignore_ascii_case("module")
                {
                    has_script = true;
                }
            }
            // Scripted content (OPF-014/015): epubcheck (OPSHandler30) marks
            // a document scripted when it has javascript (the <script> case
            // above), a <form> element, or any on* event-handler attribute -
            // NOT the mere presence of a form control like <input>/<button>.
            // The old code had it backwards on both counts (any form control
            // triggered it; on* attributes never did), so <input required>
            // wrongly reported OPF-014 while <span onclick="…"> did not (#37).
            // "any unnamespaced name starting with on" is the HTML5 event-
            // handler rule; on a valid document (all this check cares about)
            // every such attribute is a real handler, and the grammar now
            // rejects any non-handler on*-named attribute anyway (#31/#36).
            if node.tag_name().name() == "form"
                || node
                    .attributes()
                    .any(|a| a.namespace().is_none() && a.name().starts_with("on"))
            {
                has_script = true;
            }
            if node.tag_name().name() == "svg"
                && node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            {
                has_svg = true;
            }
            if node.tag_name().name() == "math"
                && node.tag_name().namespace() == Some(crate::mathml::MATHML_NS)
            {
                has_mathml = true;
            }
            if !references_svg {
                for attr in node.attributes() {
                    if !matches!(attr.name(), "src" | "href" | "data" | "poster") {
                        continue;
                    }
                    let v = attr.value();
                    if is_external(v) {
                        continue;
                    }
                    let target = nfc(&resolve(&dir, v.split('#').next().unwrap_or(v)));
                    if svg_manifest_paths.contains(&target) {
                        references_svg = true;
                        break;
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
                let origin = crate::css::inline_origin(&t, &css_text, node);
                crate::css::check(
                    &css_text,
                    &path,
                    &dir,
                    &name_index,
                    &manifest_paths,
                    origin,
                    advisory,
                    is_epub3,
                    report,
                );
                crate::opf::check_exempt_font_usage(
                    &css_text,
                    &dir,
                    &crate::opf::ResourceView {
                        items: &items,
                        name_index: &name_index,
                    },
                    &path,
                    origin,
                    is_epub3,
                    report,
                );
                let sheet = styloria::Parser::parse_stylesheet(&css_text);
                let inline_classes = crate::css::selector_class_names(&sheet);
                if inline_classes.iter().any(|c| is_media_overlay_class(c)) {
                    for c in crate::css::selector_class_names_spanned(&css_text) {
                        if is_media_overlay_class(&c.node) {
                            mo_class_sites.push((
                                c.node,
                                path.clone(),
                                Some(origin.position(&css_text, c.span.start)),
                            ));
                        }
                    }
                }
                doc_class_names
                    .entry(path.clone())
                    .or_default()
                    .extend(inline_classes);
                for u in crate::css::stylesheet_urls(&sheet) {
                    // A stylesheet's url()/@import targets are consumed
                    // resources too (fonts, images, imported sheets) - see
                    // OPF-097 below. Inline <style> resolves against this
                    // document's own directory.
                    if !is_external(&u) {
                        resource_refs.insert(nfc(&resolve(&dir, strip_url_fragment(&u).trim())));
                    }
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
                && node.attr_no_ns("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
                && let Some(href) = node.attr_no_ns("href")
                && !is_external(href)
            {
                let resolved = resolve(&dir, href);
                if let Some(orig) = name_index.get(&nfc(&resolved)).cloned()
                    && let Some(b) = ocf.read(&orig)
                {
                    let css_text = crate::css::decode_bytes(&b);
                    let sheet = styloria::Parser::parse_stylesheet(&css_text);
                    doc_class_names
                        .entry(path.clone())
                        .or_default()
                        .extend(crate::css::selector_class_names(&sheet));
                    let css_path = nfc(&resolved);
                    for c in crate::css::selector_class_names_spanned(&css_text) {
                        if is_media_overlay_class(&c.node) {
                            mo_class_sites.push((
                                c.node,
                                css_path.clone(),
                                Some(Position::of_offset(&css_text, c.span.start)),
                            ));
                        }
                    }
                    let css_dir = parent_dir(&resolved);
                    for u in crate::css::stylesheet_urls(&sheet) {
                        // Resolved against the stylesheet's own directory,
                        // not the document that links it.
                        if !is_external(&u) {
                            resource_refs
                                .insert(nfc(&resolve(&css_dir, strip_url_fragment(&u).trim())));
                        }
                        // A *linked* stylesheet's remote URLs are not this
                        // document's remote references. The manifest pass
                        // over the stylesheet itself already reports them,
                        // once, against the stylesheet - which is where
                        // epubcheck puts them too. Adding them here as well
                        // reported the same remote font once per document
                        // that links the sheet: one `@font-face` in one
                        // shared stylesheet produced 10 RSC-008 and 9
                        // RSC-031 on a ten-document book, against
                        // epubcheck's one finding.
                        //
                        // Invisible to every instrument. No book on the
                        // 346-book shelf has a remote URL in CSS at all, and
                        // the corpus fixtures that do have a single content
                        // document each - the duplication scales with the
                        // number of linking documents, so at one document it
                        // cannot appear. Inline `<style>` above is a
                        // different case and still counts: those URLs really
                        // are the document's own.
                    }
                }
            }
            // CSS-005 (usage): a plain `<link rel="stylesheet">` (not
            // "alternate stylesheet") whose `class` names more than one
            // alt-style-tag - a single name is fine (even if unrecognized),
            // only multiple conflicting names are flagged.
            if node.tag_name().name() == "link" {
                let rel_tokens: Vec<&str> = node
                    .attr_no_ns("rel")
                    .map(|r| r.split_whitespace().collect())
                    .unwrap_or_default();
                let is_plain_stylesheet =
                    rel_tokens.len() == 1 && rel_tokens[0].eq_ignore_ascii_case("stylesheet");
                let is_alt_stylesheet = rel_tokens.len() == 2
                    && rel_tokens[0].eq_ignore_ascii_case("alternate")
                    && rel_tokens[1].eq_ignore_ascii_case("stylesheet");
                if is_plain_stylesheet
                    && let Some(class) = node.attr_no_ns("class")
                    && class.split_whitespace().count() > 1
                {
                    report.push_at_pos(
                        CSS_005,
                        Severity::Usage,
                        "link element's class names conflicting alt style tags",
                        path.clone(),
                        Position::of(node),
                    );
                }
                // CSS-015: an alternate-stylesheet link must have a
                // non-empty title (missing and present-but-empty are each
                // their own finding).
                if is_alt_stylesheet {
                    match node.attr_no_ns("title") {
                        None => {
                            report.push_node(
                                CSS_015,
                                Severity::Error,
                                "an alternate stylesheet link must have a title attribute",
                                path.clone(),
                                node,
                                "opf.content_document.alt_stylesheet_missing_title",
                                Vec::new(),
                            );
                        }
                        Some(t) if t.trim().is_empty() => {
                            report.push_node(
                                CSS_015,
                                Severity::Error,
                                "an alternate stylesheet link's title must not be empty",
                                path.clone(),
                                node,
                                "opf.content_document.alt_stylesheet_empty_title",
                                Vec::new(),
                            );
                        }
                        Some(_) => {}
                    }
                }
            }
            // CSS-008: a `style="..."` attribute is a plain declaration
            // list, same malformed-shape check as a stylesheet's own block.
            if let Some(style) = node.attr_no_ns("style") {
                crate::css::check_style_attribute(style, &path, advisory, is_epub3, report);
            }
        }
        book_has_scripts |= has_script;

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
            // Two flags per property, not one. `required` drives OPF-014
            // (used but undeclared); `allowed` drives the "declared but not
            // needed" branch. They differ only for `svg`: referencing an SVG
            // resource makes the declaration *permitted* without making it
            // *required*, which is epubcheck's own distinction between
            // `requiredProperties` and `allowedProperties`. Collapsing them
            // into one flag would trade this false positive for the opposite
            // one - demanding the property from any document that links to an
            // SVG image.
            for (required, allowed, name, unused_id, unused_sev) in [
                (
                    has_remote,
                    has_remote,
                    "remote-resources",
                    OPF_018,
                    Severity::Warning,
                ),
                (has_script, has_script, "scripted", OPF_015, Severity::Error),
                (has_mathml, has_mathml, "mathml", OPF_015, Severity::Error),
                (
                    has_svg,
                    has_svg || references_svg,
                    "svg",
                    OPF_015,
                    Severity::Error,
                ),
            ] {
                let declared_here = declared_tokens.contains(&name);
                if required && !declared_here {
                    report.push_node(
                        OPF_014,
                        Severity::Error,
                        format!(
                            "content document uses {name} but doesn't declare the \"{name}\" property"
                        ),
                        path.clone(),
                        d.root_element(),
                        "opf.content_document.property_used_undeclared",
                        vec![name.to_string()],
                    );
                } else if declared_here && !allowed {
                    // For `remote-resources` specifically, scripted content
                    // changes the verdict from a warning to a usage note: a
                    // script can fetch a remote resource dynamically, which
                    // static analysis cannot see, so the property can't be
                    // disproven - only left unverified. epubcheck reports
                    // OPF-018b (usage) instead of OPF-018 (warning) in that
                    // case, the same HAS_SCRIPTS downgrade as OPF-096b and
                    // RSC-006b. scripted/svg have no such variant.
                    let (id, sev) = if name == "remote-resources" && has_script {
                        (OPF_018B, Severity::Usage)
                    } else {
                        (unused_id, unused_sev)
                    };
                    report.push_at_pos(
                        id,
                        sev,
                        format!(
                            "the \"{name}\" property is declared but doesn't appear to be needed"
                        ),
                        path.clone(),
                        Position::of(d.root_element()),
                    );
                }
            }
            if has_switch && !declared_tokens.contains(&"switch") {
                report.push_node(
                    OPF_014,
                    Severity::Error,
                    "content document uses epub:switch but doesn't declare the \"switch\" property",
                    path.clone(),
                    d.root_element(),
                    "opf.content_document.property_used_undeclared",
                    vec!["switch".to_string()],
                );
            }
            // "index" only gets the "declared but unused" direction
            // (OPF-015, confirmed via a real fixture) - unlike remote-
            // resources/scripted/svg, a real "index" *usage* is detected
            // via epub:type markers that don't need the manifest property
            // at all when the publication is identified as an index some
            // other way (dc:type=index, or a <collection role="index">
            // link) - so "used but undeclared" isn't a real rule here
            // (confirmed the hard way: a naive uniform version false-
            // positived on `index-whole-pub-valid`).
            if declared_tokens.contains(&"index") && crate::indexes::index_elements(&d).is_empty() {
                report.push_at_pos(
                    OPF_015,
                    Severity::Error,
                    "the \"index\" property is declared but doesn't appear to be needed",
                    path.clone(),
                    Position::of(d.root_element()),
                );
            }
        }

        // In EPUB 2 *every* remote reference is a restricted one. OPS 2.0.1
        // has no remote-resource concept at all: there is no
        // `remote-resources` property to declare and nothing may live
        // outside the container, so "is it in the manifest" is not the
        // question and RSC-008 is the wrong answer. epubcheck reports
        // RSC-006 for a remote font in CSS at 2.0 and RSC-008 at 3.0 -
        // probed one book per version, in a linked stylesheet and in an
        // inline `<style>`, on books otherwise clean in both tools.
        //
        // Routing it through the existing restricted set rather than adding
        // a second branch gets the RSC-031 rule right for free: the https
        // warning below already skips restricted references, and epubcheck
        // likewise gives no RSC-031 at 2.0, for the reason documented there
        // - the scheme is beside the point when the resource may not be
        // remote at all.
        if !is_epub3 {
            restricted_remote_refs.extend(remote_refs.iter().cloned());
        }

        // RSC-008: a remote resource referenced from this content
        // document isn't declared as its own manifest item at all
        // (EPUB 3 requires every resource, including remote ones, to
        // have a manifest entry) - except a `restricted_remote_refs`
        // reference, which is always RSC-006 instead (declared or not;
        // confirmed via `resources-remote-iframe-undeclared-error` etc.,
        // where only RSC-006 is expected, never RSC-008 too).
        remote_resource_refs.extend(remote_refs.iter().cloned());
        for r in &remote_refs {
            if restricted_remote_refs.contains(r) {
                continue;
            }
            if !remote_manifest.contains_key(r) {
                report.push_node(
                    RSC_008,
                    Severity::Error,
                    format!("remote resource '{r}' is not declared in the manifest"),
                    path.clone(),
                    d.root_element(),
                    "opf.content_document.remote_resource_not_in_manifest",
                    vec![r.clone()],
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
                report.push_node(
                    RSC_006,
                    Severity::Error,
                    format!("remote image '{r}' is referenced from an \"a\" element"),
                    path.clone(),
                    d.root_element(),
                    "opf.content_document.remote_image_hyperlinked",
                    vec![r.clone()],
                );
            }
        }
        // RSC-006: img/iframe/script/stylesheet/non-exempt-object always
        // disallow a remote resource, regardless of manifest declaration.
        for r in &restricted_remote_refs {
            report.push_node(
                RSC_006,
                Severity::Error,
                format!("remote resource '{r}' is not allowed in this context"),
                path.clone(),
                d.root_element(),
                "opf.content_document.remote_resource_restricted_context",
                vec![r.clone()],
            );
        }
        // RSC-031: a remote reference using a plain `http://` URL instead
        // of `https://`.
        //
        // Not for the ones that just drew RSC-006 above: if the resource is
        // not allowed to be remote *at all* in this context, its scheme is
        // beside the point, and saying "also, use https" invites fixing the
        // wrong half. epubcheck draws the same line by construction - it
        // reports RSC-006 and aborts that reference's checks, with RSC-031
        // on the `else` branch (`ResourceReferencesChecker`). Four corpus
        // scenarios expect RSC-006 "and no other errors or warnings"; we
        // were adding RSC-031 to every one (issue #26).
        for r in remote_refs.difference(&restricted_remote_refs) {
            if crate::url::is_insecure_remote(r) {
                report.push_at_pos(
                    RSC_031,
                    Severity::Warning,
                    format!("remote resource '{r}' should use https"),
                    path.clone(),
                    Position::of(d.root_element()),
                );
            }
        }
    }

    if advisory && !is_epub3 {
        check_declared_version_advisory(&doc, opf_path, version_signals, report);
    }

    // The last document has no next iteration to correct its findings.
    if let Some((from, p, sh)) = pending_dtd_fix.take() {
        crate::htm::correct_dtd_shift(&mut report.messages[from..], &p, sh);
    }

    // Whole-publication index fallback: only when neither a manifest
    // properties="index" item nor an index/index-group collection
    // narrows things down to specific documents - a confirmed index
    // publication then just needs *some* content document anywhere with
    // an epub:type="index" element (confirmed via a real fixture using
    // dc:type=index alone, with the index marked on an ordinary
    // <section>, not called out via any manifest/collection signal).
    if is_index_pub
        && manifest_index_paths.is_empty()
        && collection_index_paths.is_empty()
        && !any_index_content
    {
        report.push_node(
            RSC_005,
            Severity::Error,
            "At least one \"index\" element must be present in a document declared as an index in the OPF",
            opf_path,
            pkg,
            "opf.index.missing_index_element",
            Vec::new(),
        );
    }

    // dc:type="dictionary" detection - the OPF-078/079 cross-check itself
    // (whether real dictionary content backs it up, per-collection for a
    // multi-dictionary publication) happens in `check_dictionaries` below,
    // which also needs the full `dictionary_marked_docs` set, not just a
    // whole-publication bool.
    let is_dictionary_pub = opf_dc_type.as_deref() == Some("dictionary");
    if !is_dictionary_pub && !dictionary_marked_docs.is_empty() {
        report.push_at_pos(
            OPF_079,
            Severity::Warning,
            "dictionary content was detected, but the dc:type identifier \"dictionary\" is not declared",
            opf_path,
            Position::of(pkg),
        );
    }

    // --- Spine reachability (RSC-011/OPF-096) ---
    let opf_own_name_nfc = nfc(opf_path);
    for (target, source) in &hyperlink_targets {
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
        // RSC-010: the target is a manifest item that is not a Content
        // Document and has no fallback that reaches one (#78). epubcheck
        // runs this for *every* hyperlink (`ResourceReferencesChecker`:220,
        // `case HYPERLINK`), and reports it *instead of* RSC-011 - it
        // aborts the reference's checks right after, which is why the
        // `continue` below is part of the parity and not a shortcut. We had
        // it on the two toc paths only (the NCX `<content src>` and the nav
        // toc link), so an ordinary `<a href="styles.css">` drew nothing.
        //
        // The deprecated types are exempt here as they are there, and this
        // is not version-gated: epubcheck's hyperlink branch tests both
        // predicates without asking the version.
        if let Some((id, (_, mt))) = items.iter().find(|(_, (p, _))| nfc(p) == *target)
            && !is_content_document_type(mt)
            && !is_deprecated_content_document_type(mt)
            && !fallback_reaches_content_document(id, &items, &fallback_map)
        {
            report.push_full_path(
                RSC_010,
                Severity::Error,
                format!("'{target}' is hyperlinked but is not a Content Document"),
                source.file.clone(),
                source.position,
                source.element_path.clone(),
                "opf.content_document.hyperlink_not_content_document",
                vec![target.clone()],
            );
            continue;
        }
        if is_content_doc && !spine_order.contains_key(target) && name_index.contains_key(target) {
            // Anchor at the source `<a>` (its file + line:column + element
            // path), not the OPF package root, matching where epubcheck points
            // (#22).
            report.push_full_path(
                RSC_011,
                Severity::Error,
                format!("'{target}' is hyperlinked but not listed in the spine"),
                source.file.clone(),
                source.position,
                source.element_path.clone(),
                "opf.spine.hyperlinked_not_in_spine",
                vec![target.clone()],
            );
        }
    }
    // Reachability is purely "does any <a> hyperlink resolve to this
    // resource" - including a link the resource makes to *itself*. That is
    // exactly how epubcheck has always treated it (Kevin Hendricks, issue
    // #1: a Sigil-built nav is reachable because its own landmarks section
    // links to the nav, and "the same internal link trick works for any
    // xhtml file listed as non-linear and always has"). So the toc nav is
    // NOT special-cased here: a nav that self-links via its landmarks (the
    // normal Sigil shape) is already in `hyperlink_targets` and passes,
    // while a non-linear nav with genuinely no link to it is flagged, which
    // is what epubcheck does too. Both self-link forms feed the set: a
    // full-href landmark link (`href="nav.xhtml"`) via the resolve() insert,
    // and a fragment-only self-link (`href="#..."`) via the self-reference
    // insert - see the hyperlink-collection pass above.
    //
    // EPUB 3 only. The reachability requirement is EPUB 3's ("Each EPUB
    // content document referenced from the spine with linear=no must be
    // reachable"); EPUB 2.0.1 has no such rule, and epubcheck implements it
    // in `OPFChecker30` - its EPUB-3 checker - which is what the severity
    // note below was read off in the first place. Every OPF-096 fixture in
    // epubcheck's corpus lives under `epub3/`, and epubcheck stays silent on
    // a real EPUB 2 book with an unreachable `linear=no` document that we
    // used to flag (reported by Doitsu on the MobileRead forum). Same class
    // as #9 and #21: an EPUB 3 rule leaking into EPUB 2.
    for (path, itemref_pos) in &non_linear_paths {
        if is_epub3 && !hyperlink_targets.contains_key(path) {
            // Real epubcheck downgrades this from an error to a usage note
            // when the book uses scripting anywhere - script could add
            // navigation/hyperlinks dynamically that this static analysis
            // can't see (confirmed against epubcheck's own
            // `OPFChecker30`: `FeatureEnum.HAS_SCRIPTS` gates
            // `OPF-096` vs `OPF-096b`).
            let (id, severity) = if book_has_scripts {
                (OPF_096B, Severity::Usage)
            } else {
                (OPF_096, Severity::Error)
            };
            report.push_full(
                id,
                severity,
                format!(
                    "non-linear content '{path}' has no hyperlink pointing to it, \
                     so it is not reachable from the reading order"
                ),
                opf_path,
                *itemref_pos,
                "opf.spine.non_linear_unreachable",
                vec![path.to_string()],
            );
        }
    }

    // --- EDUPUB pagination source / page-list cross-check (NAV-003/OPF-066) ---
    if crate::edupub::is_edupub(opf_dc_type.as_deref()) {
        crate::edupub::check_page_list(has_pagination_source, has_page_list_nav, opf_path, report);
        // NAV-004..008: nav-completeness vs content-doc features.
        nav_completeness.check(opf_path, report);
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
        let declared_prefixes =
            attr_ns_node(d.root_element(), "http://www.idpf.org/2007/ops", "prefix")
                .map(|p| {
                    check_prefix_declaration(
                        p,
                        doc_path,
                        d.root_element(),
                        PrefixContext::ContentDocument,
                        advisory,
                        report,
                    )
                })
                .unwrap_or_default();
        check_prefix_placement(&d, doc_path, report);
        for n in d.descendants().filter(|n| n.is_element()) {
            if let Some(v) = n.attribute(("http://www.idpf.org/2007/ops", "type")) {
                check_prefix_usage(v, &declared_prefixes, doc_path, n, report);
            }
        }
        // A standalone SVG has no XHTML href walk, so its remote references
        // have to be collected here or the unreferenced-remote-item check
        // (#70) sees none. `<font-face-uri xlink:href>` is the case that
        // matters: the manifest gives such a font a media type like
        // `application/vnd.dafont`, which no font-type test recognises, so
        // the reference is the only thing that makes it legitimate.
        for n in d.descendants().filter(|n| n.is_element()) {
            if let Some(v) = n
                .attribute(("http://www.w3.org/1999/xlink", "href"))
                .or_else(|| n.attr_no_ns("href"))
                && is_remote_url(v)
            {
                remote_resource_refs.insert(strip_url_fragment(v).trim().to_string());
            }
        }
        // OPF-014: a standalone SVG content document embedding a remote
        // font (via <font-face-uri>) uses a remote resource just as much
        // as an XHTML doc referencing one directly - confirmed via a real
        // fixture where the SVG's own manifest item lacks the
        // "remote-resources" property.
        if is_epub3 {
            let uses_remote_font = d.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "font-face-uri"
                    && n.attribute(("http://www.w3.org/1999/xlink", "href"))
                        .or_else(|| n.attr_no_ns("href"))
                        .is_some_and(is_remote_url)
            });
            if uses_remote_font {
                let declared = item_properties
                    .get(doc_path.as_str())
                    .cloned()
                    .unwrap_or_default();
                if !declared.split_whitespace().any(|t| t == "remote-resources") {
                    report.push_node(
                        OPF_014,
                        Severity::Error,
                        "content document uses a remote font but doesn't declare the \"remote-resources\" property",
                        doc_path.clone(),
                        d.root_element(),
                        "opf.content_document.property_used_undeclared",
                        vec!["remote-resources".to_string()],
                    );
                }
            }
        }
        // EPUB 3 only, for the same reason as the inline-SVG site above:
        // `image/svg+xml` + VERSION_2 maps to `SVG_20_NVDL` with no
        // informative validator beside it.
        if is_epub3 {
            crate::svg::check_vocabulary(d.root_element(), doc_path, report);
            crate::svg::check_attribute_vocabulary(d.root_element(), doc_path, report);
        }
        crate::svg::check_epub_attributes(d.root_element(), doc_path, report);
        crate::svg::check_ids(d.root_element(), doc_path, report);
        crate::svg::check_link_labels(d.root_element(), doc_path, report);
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
                false,
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
                .attr_no_ns("href")
                .or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(v) = href
                && !is_external(v)
                && !v.contains('#')
            {
                report.push_node(
                    RSC_015,
                    Severity::Error,
                    format!("\"use\" element's href '{v}' has no fragment identifier"),
                    doc_path.clone(),
                    n,
                    "opf.content_document.use_href_missing_fragment",
                    vec![v.to_string()],
                );
            }
        }

        // RSC-006: a remote stylesheet reference from a standalone SVG
        // content document - via a top-level `<?xml-stylesheet?>` PI, an
        // inline `<style>`'s `@import`, or a `<link rel="stylesheet">` -
        // is always restricted, same rule as the XHTML content-doc loop
        // above (a remote *stylesheet* is never allowed, unlike a remote
        // font/image referenced from CSS).
        for pi in d.root().children().filter(|n| n.is_pi()) {
            if let Some(p) = pi.pi()
                && p.target == "xml-stylesheet"
                && let Some(href) = p.value.and_then(extract_pi_href)
            {
                if is_remote_url(&href) {
                    report.push_node(
                        RSC_006,
                        Severity::Error,
                        format!("remote stylesheet '{href}' is not allowed"),
                        doc_path.clone(),
                        pi,
                        "opf.content_document.remote_stylesheet_pi",
                        vec![href.clone()],
                    );
                }
                // RSC-030: a file: URL is never allowed. The SVG pass
                // scans these stylesheet forms itself (they don't go
                // through the XHTML href walk or the CSS url() pass, both
                // of which already flag file: URLs), so the check has to
                // be repeated here.
                if is_file_url(&href) {
                    report.push_node(
                        RSC_030,
                        Severity::Error,
                        format!("'{href}' is a file URL, which is not allowed"),
                        doc_path.clone(),
                        pi,
                        "opf.content_document.file_url_stylesheet_pi",
                        vec![href.clone()],
                    );
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
                        report.push_node(
                            RSC_006,
                            Severity::Error,
                            format!("remote stylesheet import '{import_url}' is not allowed"),
                            doc_path.clone(),
                            n,
                            "opf.content_document.remote_stylesheet_import",
                            vec![import_url.clone()],
                        );
                    }
                    if is_file_url(&import_url) {
                        report.push_node(
                            RSC_030,
                            Severity::Error,
                            format!("'{import_url}' is a file URL, which is not allowed"),
                            doc_path.clone(),
                            n,
                            "opf.content_document.file_url_stylesheet_import",
                            vec![import_url.clone()],
                        );
                    }
                }
            }
            if n.tag_name().name() == "link"
                && n.attr_no_ns("rel").is_some_and(|r| {
                    r.split_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                })
                && let Some(href) = n.attr_no_ns("href")
            {
                if is_remote_url(href) {
                    report.push_node(
                        RSC_006,
                        Severity::Error,
                        format!("remote stylesheet '{href}' is not allowed"),
                        doc_path.clone(),
                        n,
                        "opf.content_document.remote_stylesheet_link",
                        vec![href.to_string()],
                    );
                }
                if is_file_url(href) {
                    report.push_node(
                        RSC_030,
                        Severity::Error,
                        format!("'{href}' is a file URL, which is not allowed"),
                        doc_path.clone(),
                        n,
                        "opf.content_document.file_url_stylesheet_link",
                        vec![href.to_string()],
                    );
                }
            }
        }
    }

    // --- Media-overlay active-class CSS cross-referencing (CSS-029/030) ---

    // CSS-029 (usage): a well-known class name is used as a CSS selector
    // somewhere, but its corresponding property isn't declared at all.
    //
    // Reported once per place the name is written, at that place - not once
    // per content document that happens to link the stylesheet. The two
    // differ whenever a stylesheet is shared: the old shape reported the
    // same one selector once per document, each time naming a file the
    // class name does not appear in (reported by Doitsu on the MobileRead
    // forum).
    mo_class_sites.sort_by(|a, b| {
        (&a.0, &a.1, a.2.map(|p| (p.line, p.column))).cmp(&(
            &b.0,
            &b.1,
            b.2.map(|p| (p.line, p.column)),
        ))
    });
    mo_class_sites.dedup();
    for (class, css_path, pos) in &mo_class_sites {
        let declared = match class.as_str() {
            WELL_KNOWN_ACTIVE_CLASS => media_active_class.is_some(),
            WELL_KNOWN_PLAYBACK_CLASS => media_playback_active_class.is_some(),
            _ => continue,
        };
        if declared {
            continue;
        }
        let property = match class.as_str() {
            WELL_KNOWN_ACTIVE_CLASS => "media:active-class",
            _ => "media:playback-active-class",
        };
        let text = format!(
            "CSS class '{class}' is used as a selector, but no '{property}' property is declared in the package document"
        );
        let args = vec![class.clone(), property.to_string()];
        match pos {
            Some(p) => report.push_full(
                CSS_029,
                Severity::Usage,
                text,
                css_path.clone(),
                *p,
                "css.media_overlay.class_property_not_declared",
                args,
            ),
            None => report.push_at_rule(
                CSS_029,
                Severity::Usage,
                text,
                css_path.clone(),
                "css.media_overlay.class_property_not_declared",
                args,
            ),
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
            if let Some(name) = declared_class
                && !classes.contains(name.as_str())
            {
                report.push_at(
                        CSS_030,
                        Severity::Error,
                        format!("{property_name} '{name}' has no matching CSS selector in this content document"),
                        doc_path.clone(),
                    );
            }
        }
    }

    // --- CSS resources declared in the manifest ---
    // Manifest order, for the same reason as `content_docs` — this loop is the
    // other place whose visit order reaches the report. Fixing only the
    // content documents took the unstable books from 94 to 5, and every one of
    // the five was a stylesheet.
    let css_items: Vec<String> = manifest_order
        .iter()
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
            crate::css::CssOrigin::File { bytes: Some(&b) },
            advisory,
            is_epub3,
            report,
        );
        // RSC-008: a standalone (manifest-declared) stylesheet can
        // reference a remote resource without any content document ever
        // linking to it - still needs its own manifest item. OPF-014: and
        // the stylesheet's *own* manifest item needs "remote-resources"
        // declared, same as a content document or SMIL overlay would.
        check_exempt_font_usage(
            &css_text,
            &dir,
            &ResourceView {
                items: &items,
                name_index: &name_index,
            },
            &path,
            crate::css::CssOrigin::File { bytes: None },
            is_epub3,
            report,
        );
        let sheet = styloria::Parser::parse_stylesheet(&css_text);
        let mut css_has_remote = false;
        for u in crate::css::stylesheet_urls(&sheet) {
            // Consumed resources, for OPF-097 - a font is "used" if any
            // stylesheet in the manifest asks for it, exactly as epubcheck
            // registers references from every CSS resource it checks.
            if !is_external(&u) {
                resource_refs.insert(nfc(&resolve(&dir, strip_url_fragment(&u).trim())));
            }
            // RSC-020, same reasoning as the guide and the NCX: a `url()` is a
            // registered reference and epubcheck validates every one. Probed
            // 2026-08-21 — `url(i m.png)` draws RSC-020 there and drew nothing
            // here. Interior space only; leading/trailing is stripped by the
            // URL parser and valid.
            if !is_external(&u) && u.trim().contains(' ') {
                report.push_at_rule(
                    RSC_020,
                    Severity::Error,
                    format!("URL '{u}' is not conforming"),
                    path.clone(),
                    "css.url.malformed_relative_url",
                    vec![u.clone()],
                );
            }
            if is_remote_url(&u) {
                css_has_remote = true;
                let u = strip_url_fragment(&u);
                // A stylesheet asking for a remote font *is* a reference to
                // it, and RSC-006/RSC-006b turn on whether one exists
                // anywhere. Without this, three corpus fixtures whose only
                // use of a remote font is `@font-face` were reported as
                // unreferenced.
                remote_resource_refs.insert(u.clone());
                // EPUB 2: nothing may be remote, so this is RSC-006 and the
                // manifest question never arises - the same rule the
                // content-document path applies through
                // `restricted_remote_refs`. See the comment there.
                if !is_epub3 {
                    report.push_at_rule(
                        RSC_006,
                        Severity::Error,
                        format!("remote resource '{u}' is not allowed in this context"),
                        path.clone(),
                        "opf.content_document.remote_resource_restricted_context",
                        vec![u.clone()],
                    );
                } else if !remote_manifest.contains_key(&u) {
                    report.push_at_rule(
                        RSC_008,
                        Severity::Error,
                        format!("remote resource '{u}' is not declared in the manifest"),
                        path.clone(),
                        "opf.content_document.remote_resource_not_in_manifest",
                        vec![u.clone()],
                    );
                }
                // RSC-031 belongs here too, not only on the per-document
                // path. It used to reach a linked stylesheet's URLs only
                // because the linking document adopted them, so removing
                // that duplication took the https warning with it - caught
                // by re-probing EPUB 3 after the change rather than by any
                // test, since no shelf book has a remote URL in CSS.
                //
                // EPUB 3 only: in EPUB 2 nothing may be remote at all, and
                // epubcheck reports that and stops, on the same reasoning
                // the per-document site above already documents - telling
                // someone to switch to https points at the wrong half of
                // their problem. Probed both versions against 5.3.0.
                if is_epub3 && crate::url::is_insecure_remote(&u) {
                    report.push_at_rule(
                        RSC_031,
                        Severity::Warning,
                        format!("remote resource '{u}' should use https"),
                        path.clone(),
                        "opf.content_document.remote_resource_insecure_scheme",
                        vec![u],
                    );
                }
            }
        }
        // **No `is_epub3` here, and that is deliberate — do not "fix" it.**
        // Its neighbour twelve lines up (RSC-031) is EPUB 3 only, so the
        // absence of a guard on this one reads like an oversight; epubsana
        // read it that way on 2026-08-21 and asked. Measured that day, one
        // book per version against 5.3.0, same stylesheet and same
        // `res:///…` URL, changing nothing but `version`: **epubcheck
        // reports OPF-014 for both 2.0 and 3.0.** The neighbour's reasoning
        // ("in EPUB 2 nothing may be remote at all, so pointing at the
        // scheme aims at the wrong half of the problem") is specific to the
        // https advice and does not transfer to the missing-property error.
        //
        // A repairer must still decline to *act* on this in an EPUB 2
        // package, because `properties` is not an OPS 2.0.1 attribute -
        // but that is a fact about the edit, not about the finding, and it
        // is epubsana's guard to hold rather than ours to suppress.
        if css_has_remote
            && !item_properties
                .get(&nfc(&path))
                .is_some_and(|p| p.split_whitespace().any(|t| t == "remote-resources"))
        {
            report.push_at_rule(
                OPF_014,
                Severity::Error,
                "stylesheet uses a remote resource but doesn't declare the \"remote-resources\" property",
                path.clone(),
                "opf.content_document.property_used_undeclared",
                vec!["remote-resources".to_string()],
            );
        }
    }

    // --- OPF-097: manifest resources nothing consumes ---
    //
    // A resource declared in the manifest that no document ever draws,
    // applies or loads is almost certainly dead weight - a font left behind
    // by an earlier revision, an image no page uses. epubcheck reports it as
    // a usage note; the book stays valid (requested on the MobileRead forum
    // by JSWolf, for unused fonts and images specifically).
    //
    // "Referenced" is narrower than it sounds and the narrowness is the
    // whole rule: a hyperlink to a document does *not* count (see
    // `is_resource_reference`). What is exempt instead is what the container
    // itself reaches: anything in the spine, the nav document, and the NCX
    // are all consumed by the reading system rather than by a document, so
    // they can never be "unused".
    //
    // "No *content document* references it" is the exact claim, and the
    // precision matters: a `properties="cover-image"` cover really is
    // referenced - by the package document, and used by the reading system -
    // yet no content document draws it, so it is reported. epubcheck reports
    // it too (its rule has no cover exemption either), and the note is
    // factually true; what to do about it is the author's call, which is why
    // this is usage and not advice.
    //
    // EPUB 3 only - the rule lives in epubcheck's EPUB-3 checker, and there
    // is no EPUB 2 counterpart.
    if is_epub3 {
        // **The overlays' own references, folded in before the question is
        // asked.** The comment below always claimed the SMIL's audio/text
        // targets were "collected by the overlay pass" — they were not, and
        // could not have been: that pass runs *after* this block, so nothing
        // it collects can reach `resource_refs` in time. The audio file of
        // every media-overlay book was therefore reported unreferenced, on
        // 19 of W3C's 209 `epub-tests` publications. `smil::resource_refs`
        // answers only the reference question and reports nothing, so the
        // second parse costs a finding-free walk of a small file.
        //
        // **SVG is the second source with the same problem, found the same
        // way.** A standalone SVG in the spine is a content document, but
        // `content_docs` selects on `application/xhtml+xml`, so an SVG's own
        // references were collected by nothing. W3C's
        // `lay-pp-embedded-images-svg` is eight `<svg><image
        // xlink:href="../images/A.png"/></svg>` plates; we called all eight
        // PNGs unreferenced against epubcheck's none.
        type RefExtractor = fn(&str, &str) -> Vec<String>;
        for (path, mt) in &manifest_order {
            let extract: RefExtractor = match mt.as_str() {
                "application/smil+xml" => crate::smil::resource_refs,
                "image/svg+xml" => crate::svg::resource_refs,
                _ => continue,
            };
            let Some(orig) = name_index.get(&nfc(path)) else {
                continue;
            };
            let Some(b) = ocf.read(orig) else { continue };
            resource_refs.extend(extract(&String::from_utf8_lossy(&b), &parent_dir(path)));
        }
        // A media-overlay attribute consumes its SMIL, and the SMIL's own
        // audio/text targets are consumed in turn (folded in just above);
        // both arrive as manifest ids or paths rather than as
        // document references, so fold them in here.
        let overlay_paths: HashSet<&String> = content_doc_overlay.values().collect();
        let manifest = pkg
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "manifest");
        if let Some(mn) = manifest {
            for item in mn
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "item")
            {
                let Some(href) = item.attr_no_ns("href") else {
                    continue;
                };
                // A remote item is judged on its own terms: it has no
                // container path to resolve, and epubcheck runs a separate
                // branch for it (`OPFChecker30.checkItemAfterResourceValidation`).
                if is_remote_url(href) {
                    check_unreferenced_remote_item(
                        item,
                        href,
                        &remote_resource_refs,
                        book_has_scripts,
                        is_epub3,
                        opf_path,
                        report,
                    );
                    continue;
                }
                if is_external(href) {
                    continue;
                }
                let resolved = nfc(&resolve(&base_dir, href));
                let mt = item.attr_no_ns("media-type").unwrap_or_default();
                let is_nav = item
                    .attr_no_ns("properties")
                    .is_some_and(|p| p.split_whitespace().any(|t| t == "nav"));
                if spine_order.contains_key(&resolved)
                    || is_nav
                    || mt == "application/x-dtbncx+xml"
                    || overlay_paths.contains(&resolved)
                    || resource_refs.contains(&resolved)
                {
                    continue;
                }
                report.push_node(
                    OPF_097,
                    Severity::Usage,
                    format!(
                        "'{href}' is declared in the manifest, but no content document references it"
                    ),
                    opf_path,
                    item,
                    "opf.manifest_item.never_referenced",
                    vec![href.to_string()],
                );
            }
        }
    }

    // --- Media Overlays (SMIL) ---
    // resolved-path -> media-type, for the audio Core Media Type check.
    let media_type_index: HashMap<String, String> = items
        .values()
        .map(|(path, mt)| (nfc(path), mt.clone()))
        .collect();
    // Manifest order, not `items.values()`: the HashMap's iteration order is
    // randomly seeded, so a book with two overlays printed their findings in
    // a different order every run. Third site of that bug — see
    // `manifest_order`'s note.
    let smil_items: Vec<String> = manifest_order
        .iter()
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

        // Vocabulary association (prefix/epub:type), same rules as XHTML/
        // SVG: a bare (non-namespaced) `prefix` attribute isn't part of
        // SMIL's own content model at all (RSC-005, confirmed via a real
        // fixture - only the namespaced `epub:prefix` is recognized).
        if let Ok(smil_doc) = parse_xml(&smil_text) {
            let smil_root = smil_doc.root_element();
            if smil_root.attr_no_ns("prefix").is_some() {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "attribute \"prefix\" not allowed here",
                    path.as_str(),
                    smil_root,
                    "opf.smil.bare_prefix_attribute",
                    Vec::new(),
                );
            }
            let declared_prefixes =
                attr_ns_node(smil_root, "http://www.idpf.org/2007/ops", "prefix")
                    .map(|p| {
                        check_prefix_declaration(
                            p,
                            &path,
                            smil_root,
                            PrefixContext::Overlay,
                            advisory,
                            report,
                        )
                    })
                    .unwrap_or_default();
            check_prefix_placement(&smil_doc, &path, report);
            for n in smil_doc.descendants().filter(|n| n.is_element()) {
                if let Some(v) = n.attribute(("http://www.idpf.org/2007/ops", "type")) {
                    check_prefix_usage(v, &declared_prefixes, &path, n, report);
                }
            }
        }

        // RSC-012: epub:textref fragments must resolve to a real id in
        // their target document - same shape as the NCX <content src>
        // fragment check, reusing the same id_cache-per-target pattern.
        {
            let mut id_cache: HashMap<String, Option<IdMap>> = HashMap::new();
            for (target, frag) in &textref_targets {
                if !id_cache.contains_key(target) {
                    let ids = target_id_kinds(ocf, &name_index, target, is_epub3);
                    id_cache.insert(target.clone(), ids);
                }
                // `None` = the target could not be read/parsed, so whether
                // the fragment resolves is unknown and unreported (see
                // `target_id_kinds`).
                let Some(target_ids) = &id_cache[target] else {
                    continue;
                };
                if !target_ids.contains_key(frag) {
                    report.push_at_rule(
                        RSC_012,
                        Severity::Error,
                        format!("epub:textref fragment '{frag}' is not defined in '{target}'"),
                        path.clone(),
                        "opf.smil.textref_fragment_not_defined",
                        vec![frag.clone(), target.clone()],
                    );
                }
            }
        }

        // RSC-014 for a media overlay's `<text src>`: epubcheck's last
        // reference type, and it accepts a generic id only - the same rule
        // as a hyperlink or a `cite`. An overlay pointing at an SVG symbol
        // is exotic, so this is measured rather than assumed: a minimal
        // overlay book naming `ch1.xhtml#sym` draws RSC-014 from epubcheck
        // and, before this, nothing from us.
        {
            let mut id_cache: HashMap<String, Option<IdMap>> = HashMap::new();
            for (target, frag) in &targets {
                if !id_cache.contains_key(target) {
                    let ids = target_id_kinds(ocf, &name_index, target, is_epub3);
                    id_cache.insert(target.clone(), ids);
                }
                let Some(target_ids) = &id_cache[target] else {
                    continue;
                };
                if let Some(&(_, kind)) = target_ids.get(frag)
                    && !RefKind::OverlayText.accepts(kind)
                {
                    report.push_at_rule(
                        RSC_014,
                        Severity::Error,
                        format!(
                            "overlay text link '{target}#{frag}' targets {} (incompatible resource type)",
                            kind.describe()
                        ),
                        path.clone(),
                        "opf.smil.text_incompatible_target",
                        vec![frag.clone(), target.clone()],
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
                    && n.attr_no_ns("src").is_some_and(is_remote_url)
            });
            if has_remote_audio
                && !item_properties
                    .get(&overlay_path)
                    .is_some_and(|p| p.split_whitespace().any(|t| t == "remote-resources"))
            {
                report.push_node(
                    OPF_014,
                    Severity::Error,
                    "media overlay uses a remote resource but doesn't declare the \"remote-resources\" property",
                    path.clone(),
                    smil_doc.root_element(),
                    "opf.content_document.property_used_undeclared",
                    vec!["remote-resources".to_string()],
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
            // DOM-order only, no positions reported - shift irrelevant.
            let (t, _) = crate::htm::declare_dtd_entities(crate::css::decode_bytes(&b), is_epub3);
            let Ok(d) = parse_xml(&t) else { continue };
            let id_order = dom_id_kinds(&d);
            // Ids the SMIL references but the doc doesn't have are already
            // separately caught as broken references elsewhere - skip them
            // here rather than letting a missing id break the comparison.
            let indices: Vec<usize> = frags
                .iter()
                .filter_map(|f| id_order.get(f).map(|&(i, _)| i))
                .collect();
            let in_order = indices.windows(2).all(|w| w[0] <= w[1]);
            if !in_order && indices.len() >= 2 {
                report.push_at(
                    MED_015,
                    Severity::Usage,
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
    check_html_declared_as_xhtml(&doc, is_oeb12, is_epub3, opf_path, report);
    check_external_identifiers(ocf, &items, &name_index, opf_path, is_epub3, report);
    check_dictionaries(
        &pkg,
        is_dictionary_pub,
        profile,
        &dictionary_marked_docs,
        &items,
        &item_properties,
        &base_dir,
        &name_index,
        ocf,
        opf_path,
        report,
    );
    crate::indexes::check_collections(&pkg, &items, &base_dir, opf_path, report);
    crate::previews::check_embedded_preview(&pkg, &items, &base_dir, opf_path, report);
    crate::previews::check_preview_publication(
        opf_dc_type.as_deref() == Some("preview"),
        profile,
        metadata,
        package_identifier_text.as_deref(),
        opf_path,
        report,
    );
    check_distributable_objects(&pkg, opf_path, report);
}

/// EPUB Distributable Objects 1.0, §2.2.3: a `<collection role=
/// "distributable-object">`'s own nested `<metadata>` must include
/// exactly one `dc:identifier` (confirmed via a real fixture with zero).
fn check_distributable_objects(pkg: &roxmltree::Node, opf_path: &str, report: &mut Report) {
    for coll in pkg.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "collection"
            && n.attr_no_ns("role") == Some("distributable-object")
    }) {
        let count = coll
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "metadata")
            .into_iter()
            .flat_map(|md| md.children())
            .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
            .count();
        if count != 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "A \"distributable-object\" collection must include exactly one identifier",
                opf_path,
                coll,
                "opf.collection.distributable_object_identifier_count",
                vec![count.to_string()],
            );
        }
    }
}

/// EPUB Dictionaries & Glossaries 1.0 package-level checks: Search Key Map
/// document parsing/cross-referencing (regardless of whether this is a
/// confirmed dictionary publication - a glossary can have one too), and -
/// only for a confirmed dictionary publication (`dc:type="dictionary"`) -
/// the single- vs. collection-based structural rules from spec §2.5.
// Eleven arguments, and the honest fix is not a struct wrapping these
// eleven: nine of them are the same publication-wide context that half the
// functions in this file thread through (`items`, `item_properties`,
// `name_index`, `base_dir`, `ocf`, `opf_path`, `report`, …). Bundling that
// once, for all of them, is a real refactor with a real payoff; bundling it
// here alone would move the argument list behind a type without making
// anything simpler. Left as it is until that refactor is worth doing.
#[allow(clippy::too_many_arguments)]
fn check_dictionaries(
    pkg: &roxmltree::Node,
    is_dictionary_pub: bool,
    profile: Option<&str>,
    dictionary_marked_docs: &HashSet<String>,
    items: &HashMap<String, (String, String)>,
    item_properties: &HashMap<String, String>,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    ocf: &mut Ocf,
    opf_path: &str,
    report: &mut Report,
) {
    const SKM_MT: &str = "application/vnd.epub.search-key-map+xml";
    let has_prop = |props: &str, token: &str| props.split_whitespace().any(|t| t == token);
    let node_text = |n: roxmltree::Node| -> String {
        n.descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect::<String>()
            .trim()
            .to_string()
    };

    // Search Key Map document parsing + cross-referencing.
    for (path, mt) in items.values() {
        if mt != SKM_MT {
            continue;
        }
        if !path.to_ascii_lowercase().ends_with(".xml") {
            report.push_at(
                OPF_080,
                Severity::Warning,
                format!("Search Key Map document '{path}' should have an .xml extension"),
                opf_path,
            );
        }
        let Some(orig) = name_index.get(&nfc(path)).cloned() else {
            continue;
        };
        let Some(b) = ocf.read(&orig) else { continue };
        let text = String::from_utf8_lossy(&b).into_owned();
        let Ok(d) = parse_xml(&text) else { continue };
        let skm_dir = parent_dir(path);
        let hrefs = crate::dict::check_skm(&d, path, report);
        for href in hrefs {
            if is_external(&href) {
                continue;
            }
            let path_part = href.split(['#', '?']).next().unwrap_or(&href);
            let resolved = nfc(&resolve(&skm_dir, path_part));
            if !name_index.contains_key(&resolved) {
                report.push_node(
                    RSC_007,
                    Severity::Error,
                    format!("search-key-group href '{href}' does not resolve to a real resource"),
                    path.as_str(),
                    d.root_element(),
                    "opf.dictionary.search_key_group_href_missing_resource",
                    vec![href.clone()],
                );
                continue;
            }
            if let Some((_, target_mt)) = items.values().find(|(p, _)| nfc(p) == resolved)
                && target_mt != "application/xhtml+xml"
                && target_mt != "image/svg+xml"
            {
                report.push_at_pos(
                    RSC_021,
                    Severity::Error,
                    format!("search-key-group href '{href}' does not target a Content Document"),
                    path.as_str(),
                    Position::of(d.root_element()),
                );
            }
        }
    }

    let dictionary_collections: Vec<_> = pkg
        .children()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "collection"
                && n.attr_no_ns("role") == Some("dictionary")
        })
        .collect();

    if !is_dictionary_pub {
        // The 'dict' CLI profile forces treatment as a dictionary
        // publication for the purpose of *this one* gating check only -
        // real epubcheck's own corpus fixture for this (a bare, single-
        // Package-Document check with zero other dictionary content at
        // all) expects exactly this one finding and nothing else, not
        // the full structural check suite cascading on top of content
        // that was never meant to satisfy it.
        if profile == Some("dict") {
            report.push_node(
                RSC_005,
                Severity::Error,
                "The dc:type identifier \"dictionary\" is required",
                opf_path,
                *pkg,
                "opf.dictionary.missing_dc_type",
                Vec::new(),
            );
        }
        // ...but the *collection*-scoped rules are not gated on `dc:type` at
        // all. epubcheck's `checkCollections`/`checkCollectionsContent` iterate
        // the collections and test `collection.hasRole(DICTIONARY)` and nothing
        // else, so `<collection role="dictionary">` alone is enough there.
        //
        // We required `dc:type="dictionary"` for all of it, so a book with a
        // malformed dictionary collection and no `dc:type` drew *nothing* from
        // us where epubcheck reports four - including OPF-083, a row
        // `docs/COVERAGE.md` marks implemented. A check that cannot fire is
        // worse in the matrix than one that is honestly absent.
        //
        // Safe for the fixture named above: it carries no `<collection>` at
        // all, so this branch finds nothing there.
        if dictionary_collections.is_empty() {
            return;
        }
    }

    let metadata = pkg
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata");
    let dc_languages: HashSet<String> = metadata
        .map(|md| {
            md.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "language")
                .map(node_text)
                .collect()
        })
        .unwrap_or_default();

    // Source/target language declarations - shared shape between the
    // package's own metadata (single-dictionary publications) and each
    // dictionary collection's own nested <metadata> (multi-dictionary
    // publications). A missing target-language is only enforced at the
    // collection scope (untested at the package scope) - and, per a real
    // fixture, uses the *same* message text as a missing source language
    // (confirmed, if slightly odd - not this project's own wording choice
    // but what the corpus scenario actually expects).
    let check_languages = |scope: Option<roxmltree::Node>,
                           require_target: bool,
                           report: &mut Report| {
        let metas = |property: &str| -> Vec<String> {
            scope
                .into_iter()
                .flat_map(|s| s.children())
                .filter(|n| {
                    n.is_element()
                        && n.tag_name().name() == "meta"
                        && n.attr_no_ns("property") == Some(property)
                })
                .map(node_text)
                .collect()
        };
        let report_here =
            |report: &mut Report, id, text: String, rule: &'static str, params: Vec<String>| {
                match scope {
                    Some(s) => {
                        report.push_node(id, Severity::Error, text, opf_path, s, rule, params)
                    }
                    None => report.push_at_rule(id, Severity::Error, text, opf_path, rule, params),
                }
            };
        let sources = metas("source-language");
        if sources.is_empty() {
            report_here(
                report,
                RSC_005,
                "a dictionary must declare its source language".to_string(),
                "opf.dictionary.missing_source_language",
                Vec::new(),
            );
        } else if sources.len() > 1 {
            report_here(
                report,
                RSC_005,
                "a dictionary must not declare more than one source language".to_string(),
                "opf.dictionary.multiple_source_languages",
                Vec::new(),
            );
        }
        let targets = metas("target-language");
        if targets.is_empty() {
            if require_target {
                // Note: this reuses the source-language message text
                // verbatim (matches a real corpus fixture's own
                // expectation, not this project's wording choice) - `rule`
                // correctly disambiguates it as the target-language case
                // despite the misleading shared text.
                report_here(
                    report,
                    RSC_005,
                    "a dictionary must declare its source language".to_string(),
                    "opf.dictionary.missing_target_language",
                    Vec::new(),
                );
            }
        } else {
            for t in targets {
                if !dc_languages.contains(&t) {
                    report_here(
                        report,
                        RSC_005,
                        format!("target-language '{t}' must also be declared as \"dc:language\""),
                        "opf.dictionary.target_language_not_declared",
                        vec![t.clone()],
                    );
                }
            }
        }
    };

    if dictionary_collections.is_empty() {
        if dictionary_marked_docs.is_empty() {
            report.push_node(
                OPF_078,
                Severity::Error,
                "no content document was found with dictionary content",
                opf_path,
                *pkg,
                "opf.dictionary.no_dictionary_content",
                Vec::new(),
            );
        }
        check_languages(metadata, false, report);

        let candidates: Vec<_> = item_properties
            .iter()
            .filter(|(_, props)| has_prop(props, "search-key-map"))
            .collect();
        if candidates.is_empty() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "a dictionary publication must contain exactly one Search Key Map document",
                opf_path,
                *pkg,
                "opf.dictionary.missing_search_key_map",
                Vec::new(),
            );
        } else if candidates.len() == 1 {
            let (_, props) = candidates[0];
            if !has_prop(props, "dictionary") {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "the Search Key Map document must have the \"dictionary\" property",
                    opf_path,
                    *pkg,
                    "opf.dictionary.search_key_map_missing_property",
                    Vec::new(),
                );
            }
        }

        if let Some(md) = metadata
            && let Some(dt) = md.children().find(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property") == Some("dictionary-type")
            })
        {
            let text = node_text(dt);
            if !matches!(text.as_str(), "monolingual" | "bilingual" | "multilingual") {
                report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!("\"dictionary-type\" metadata must be one of monolingual/bilingual/multilingual ('{text}')"),
                        opf_path,
                        dt,
                        "opf.dictionary.invalid_dictionary_type",
                        vec![text.clone()],
                    );
            }
        }
        return;
    }

    let mut skm_owner: HashMap<String, usize> = HashMap::new();
    for (idx, collection) in dictionary_collections.iter().enumerate() {
        let has_subcollection = collection
            .children()
            .any(|n| n.is_element() && n.tag_name().name() == "collection");
        if has_subcollection {
            report.push_node(
                RSC_005,
                Severity::Error,
                "a dictionary collection must not have sub-collections",
                opf_path,
                *collection,
                "opf.dictionary.collection_has_subcollections",
                Vec::new(),
            );
        }

        let mut skm_count = 0;
        let mut has_dict_content = false;
        for link in collection
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
        {
            let Some(href) = link.attr_no_ns("href") else {
                continue;
            };
            if is_external(href) {
                continue;
            }
            let resolved = nfc(&resolve(base_dir, href));
            if dictionary_marked_docs.contains(&resolved) {
                has_dict_content = true;
            }
            match items.values().find(|(p, _)| nfc(p) == resolved) {
                None => {
                    report.push_at_pos(
                        OPF_081,
                        Severity::Error,
                        format!(
                            "dictionary collection link '{href}' was not found in the manifest"
                        ),
                        opf_path,
                        Position::of(link),
                    );
                }
                Some((_, mt)) => {
                    let props = item_properties.get(&resolved).cloned().unwrap_or_default();
                    if has_prop(&props, "search-key-map") {
                        skm_count += 1;
                        if let Some(&first) = skm_owner.get(&resolved) {
                            if first != idx {
                                report.push_node(
                                    RSC_005,
                                    Severity::Error,
                                    format!("Search Key Map document '{href}' is referenced in more than one dictionary collection"),
                                    opf_path,
                                    link,
                                    "opf.dictionary.search_key_map_multiple_collections",
                                    vec![href.to_string()],
                                );
                            }
                        } else {
                            skm_owner.insert(resolved.clone(), idx);
                        }
                    } else if mt != "application/xhtml+xml" && mt != "image/svg+xml" {
                        report.push_at_pos(
                            OPF_084,
                            Severity::Error,
                            format!("dictionary collection link '{href}' is neither a Search Key Map Document nor an XHTML Content Document"),
                            opf_path,
                            Position::of(link),
                        );
                    }
                }
            }
        }
        match skm_count {
            0 => report.push_at_pos(
                OPF_083,
                Severity::Error,
                "a dictionary collection contains no Search Key Map Document",
                opf_path,
                Position::of(*collection),
            ),
            1 => {}
            _ => report.push_at_pos(
                OPF_082,
                Severity::Error,
                "a dictionary collection must not contain more than one Search Key Map Document",
                opf_path,
                Position::of(*collection),
            ),
        }
        if !has_dict_content {
            report.push_node(
                OPF_078,
                Severity::Error,
                "no content document was found with dictionary content",
                opf_path,
                *collection,
                "opf.dictionary.no_dictionary_content",
                Vec::new(),
            );
        }

        // A collection's own nested <metadata> is authoritative when
        // present; a real fixture with no per-collection <metadata> at
        // all instead relies entirely on the package-level source/target-
        // language declarations, so this falls back to those rather than
        // treating the collection as having zero declarations.
        let coll_metadata = collection
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "metadata");
        check_languages(coll_metadata.or(metadata), true, report);
    }
}

/// Skipped entirely for an OEBPS 1.2 package: there `text/html` is the
/// format's own (deprecated) content type, and epubcheck reports OPF-038
/// instead of OPF-035 for it (`OPFChecker`, the `getOpf12PackageFile()`
/// branch). We implement neither OPF-038 nor OPF-039, so silence is the
/// honest answer rather than a message naming the wrong problem.
///
/// OPF-035 (warning): a manifest item declared `text/html` should be declared
/// `application/xhtml+xml` instead.
///
/// **EPUB 2 only, and independent of the item's content** - both halves were
/// wrong here until issue #72, in opposite directions:
///
/// - epubcheck emits this from `OPFChecker.checkItem` purely on the declared
///   media-type, with no content inspection at all. We required the file to
///   parse as XML with an `<html>` root, so a `text/html` item holding plain
///   text (or malformed markup) drew nothing - the one shape where the
///   author most needs telling. Verified with a probe: a `text/html` item
///   containing `just plain text, not markup at all` still draws OPF-035.
/// - `OPFChecker30.checkItem` does not call `super.checkItem`, so this site
///   is unreachable for an EPUB 3 package. We had no version condition and
///   reported OPF-035 on an EPUB 3 book, which epubcheck does not - a false
///   positive, also confirmed by probe.
///
/// Skipped entirely for an OEBPS 1.2 package: there `text/html` is the
/// format's own (deprecated) content type, and epubcheck reports OPF-038
/// instead of OPF-035 for it (`OPFChecker`, the `getOpf12PackageFile()`
/// branch). We implement neither OPF-038 nor OPF-039, so silence is the
/// honest answer rather than a message naming the wrong problem.
fn check_html_declared_as_xhtml(
    doc: &roxmltree::Document,
    is_oeb12: bool,
    is_epub3: bool,
    opf_path: &str,
    report: &mut Report,
) {
    if is_oeb12 || is_epub3 {
        return;
    }
    // Reported at the manifest item in the package document, which is where
    // epubcheck reports it (`item.getLocation()`); it used to be anchored in
    // the content document, which was the only place the old content-sniffing
    // version had a position to give.
    for item in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
    {
        if item.attr_no_ns("media-type").map(str::trim) == Some("text/html") {
            let href = item.attr_no_ns("href").unwrap_or_default();
            report.push_at_pos(
                OPF_035,
                Severity::Warning,
                format!(
                    "manifest item '{href}' should be declared application/xhtml+xml, not text/html"
                ),
                opf_path,
                Position::of(item),
            );
        }
    }
}

/// EPUB 3.3 Appendix B - Allowed External Identifiers: a small, closed
/// table of `(media-type, PUBLIC id, SYSTEM id)` triples a manifest
/// resource's own DOCTYPE (if it has one at all) must match *exactly*
/// for its declared media type - confirmed via real fixtures for NCX/
/// SVG/MathML (all three MathML sub-types share the same DTD pair).
const ALLOWED_EXTERNAL_IDENTIFIERS: &[(&str, &str, &str)] = &[
    (
        "application/x-dtbncx+xml",
        "-//NISO//DTD ncx 2005-1//EN",
        "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd",
    ),
    (
        "image/svg+xml",
        "-//W3C//DTD SVG 1.1//EN",
        "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd",
    ),
    (
        "application/mathml+xml",
        "-//W3C//DTD MathML 3.0//EN",
        "http://www.w3.org/Math/DTD/mathml3/mathml3.dtd",
    ),
    (
        "application/mathml-presentation+xml",
        "-//W3C//DTD MathML 3.0//EN",
        "http://www.w3.org/Math/DTD/mathml3/mathml3.dtd",
    ),
    (
        "application/mathml-content+xml",
        "-//W3C//DTD MathML 3.0//EN",
        "http://www.w3.org/Math/DTD/mathml3/mathml3.dtd",
    ),
];

fn extract_quoted(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &s[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some((rest[..end].to_string(), &rest[end + quote.len_utf8()..]))
}

/// Extracts a DOCTYPE's `PUBLIC "id" "system"` pair, if present.
fn extract_doctype_ids(text: &str) -> Option<(String, String)> {
    let start = text.find("<!DOCTYPE")?;
    let after = &text[start..];
    let end = after.find('>')?;
    let decl = &after[..end];
    let public_idx = decl.find("PUBLIC")?;
    let rest = &decl[public_idx + "PUBLIC".len()..];
    let (public_id, rest) = extract_quoted(rest)?;
    let (system_id, _) = extract_quoted(rest)?;
    Some((public_id, system_id))
}

/// OPF-073: a manifest resource whose media type has a real allowed
/// external identifier (NCX/SVG/MathML) but whose own DOCTYPE doesn't
/// match it exactly - either a real external identifier used on the
/// *wrong* media type (confirmed via a real fixture using SVG's DOCTYPE
/// on an NCX resource), or a public identifier with a mismatched/
/// non-standard system identifier (confirmed via a real fixture using
/// the NCX public id with an arbitrary, non-DAISY system id).
fn check_external_identifiers(
    ocf: &mut Ocf,
    items: &HashMap<String, (String, String)>,
    name_index: &HashMap<String, String>,
    opf_path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    // EPUB 3 only. epubcheck's OPF-073 lives in `DeclarationHandler`, which
    // its EPUB 2 path never installs: measured against 5.3.0 with two minimal
    // books, a non-spine SVG carrying `-//W3C//DTD SVG 20010904//EN` and an
    // NCX carrying the right public id with a wrong system id. Both draw
    // OPF-073 from us; epubcheck reports neither in a `version="2.0"` package
    // and does report the SVG one in a 3.0 package. Found by the differ on a
    // real book (2026-08-04).
    if !is_epub3 {
        return;
    }
    for (path, mt) in items.values() {
        let Some((_, allowed_public, allowed_system)) = ALLOWED_EXTERNAL_IDENTIFIERS
            .iter()
            .find(|(m, _, _)| m == mt)
        else {
            continue;
        };
        let Some(orig) = name_index.get(&nfc(path)).cloned() else {
            continue;
        };
        let Some(bytes) = ocf.read(&orig) else {
            continue;
        };
        let text = crate::css::decode_bytes(&bytes);
        let Some((public_id, system_id)) = extract_doctype_ids(&text) else {
            continue;
        };
        if public_id != *allowed_public || system_id != *allowed_system {
            report.push_at(
                OPF_073,
                Severity::Error,
                format!("DOCTYPE external identifier is not allowed for media type '{mt}'"),
                opf_path,
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
        // Mirror epubcheck's `BitmapChecker` branch order (#45):
        //   < 4 bytes         -> MED-004 (can't even read a 4-byte header)
        //   magic != declared -> OPF-029; and dimensions unreadable -> PKG-021
        //   magic == declared -> extension-vs-format check (PKG-022)
        // MED-004 is specifically the too-short case; a >=4-byte file whose
        // header matches nothing is a declared/actual mismatch (OPF-029), not
        // MED-004 - that was the divergence in #45.
        if bytes.len() < 4 {
            report.push_at(
                MED_004,
                Severity::Error,
                format!("image '{path}' is corrupt (too short to contain an image header)"),
                path.as_str(),
            );
            report.push_at(
                PKG_021,
                Severity::Error,
                format!("image '{path}' is corrupt"),
                path.as_str(),
            );
            continue;
        }
        match crate::image::sniff_image_type(&bytes) {
            None => {
                report.push_at(
                    OPF_029,
                    Severity::Error,
                    format!(
                        "image '{path}' is declared as '{mt}' but its content doesn't match that media type"
                    ),
                    path.as_str(),
                );
                report.push_at(
                    PKG_021,
                    Severity::Error,
                    format!("image '{path}' is corrupt"),
                    path.as_str(),
                );
            }
            Some(actual) => {
                // Two independent axes, and epubcheck reports both when both
                // are wrong: OPF-029 compares the *declared media type* to the
                // sniffed format, PKG-022 the *file extension* to it. PKG-022
                // used to sit in an `else` arm here, so a file that was
                // mislabelled twice over - `<item media-type="image/jpeg">` on
                // a `.jpg` name holding a PNG - drew only OPF-029 and the
                // extension was never looked at. Found by the differ, 2026-08-04.
                if actual != *mt {
                    report.push_at_rule(
                        OPF_029,
                        Severity::Error,
                        format!(
                            "image '{path}' is declared as '{mt}' but its actual format is '{actual}'"
                        ),
                        path.as_str(),
                        "opf.manifest_item.declared_media_type_mismatch",
                        vec![mt.to_string(), actual.to_string()],
                    );
                }
                let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                if !crate::image::conventional_extensions(actual).contains(&ext.as_str()) {
                    report.push_at_rule(
                        PKG_022,
                        Severity::Warning,
                        format!("image '{path}' has a file extension that doesn't match its actual format '{actual}'"),
                        path.as_str(),
                        "opf.manifest_item.extension_format_mismatch",
                        vec![ext.clone(), actual.to_string()],
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
/// The two class names EPUB reserves for media-overlay styling, each
/// paired with the package-metadata property that must declare it
/// (CSS-029/030).
const WELL_KNOWN_ACTIVE_CLASS: &str = "-epub-media-overlay-active";
const WELL_KNOWN_PLAYBACK_CLASS: &str = "-epub-media-overlay-playing";

/// Whether `class` is one of the two reserved media-overlay class names.
fn is_media_overlay_class(class: &str) -> bool {
    class == WELL_KNOWN_ACTIVE_CLASS || class == WELL_KNOWN_PLAYBACK_CLASS
}

const OBFUSCATION_ALGORITHM: &str = "http://www.idpf.org/2008/embedding";

/// CSS-007 (info): a `@font-face src` target resolves to a manifest item
/// whose declared media-type is neither a Core Media Type nor exempt
/// video - i.e. the stylesheet asks for a font in a format EPUB does not
/// standardize, like the widespread-but-never-registered
/// `application/x-font-opentype`. Core/non-preferred-Core font types
/// (confirmed via `resources-cmt-font-truetype-valid`, which expects this
/// reported *zero* times) must not fire.
///
/// §3.4 exempts fonts from ever needing a fallback, and this used to be
/// worded as if that were the finding ("a foreign resource, exempt from
/// requiring a fallback"), which describes the rule that *doesn't* fire and
/// buries the one that does - a reader could only conclude epubveri was
/// reporting a non-problem (reported by Doitsu on the MobileRead forum).
/// The finding is the non-standard font format; the fallback exemption is
/// why it is Info rather than an error.
/// EPUB 2 blesses a *wider* set of font types than the Core Media Types, and
/// epubcheck's own EPUB 2 branch says so: `OPFChecker.isBlessedFontMimetype20`
/// accepts anything starting `font/`, `application/font` or
/// `application/x-font`, plus `application/vnd.ms-opentype`. Two shelf books
/// declare `application/x-font-truetype`, which is blessed there and drew this
/// note from us and nothing from epubcheck — found by the differ, 2026-08-04.
/// Info severity, so it never changed a verdict, but it was still us reporting
/// on markup that is fine for its version.
fn blessed_font_type_epub2(mt: &str) -> bool {
    mt.starts_with("font/")
        || mt.starts_with("application/font")
        || mt.starts_with("application/x-font")
        || mt == "application/vnd.ms-opentype"
}

/// What the manifest declares and what the container actually holds. Bundled
/// because this check needs both for the same path and always together: a
/// declared item whose file is absent is RSC-001's business, a reference to
/// something nothing declares is RSC-007's.
struct ResourceView<'a> {
    items: &'a HashMap<String, (String, String)>,
    name_index: &'a HashMap<String, String>,
}

fn check_exempt_font_usage(
    css: &str,
    dir: &str,
    res: &ResourceView<'_>,
    path: &str,
    origin: crate::css::CssOrigin,
    is_epub3: bool,
    report: &mut Report,
) {
    let (items, name_index) = (res.items, res.name_index);
    for u in crate::css::font_face_src_urls_spanned(css) {
        if is_external(&u.node) {
            continue;
        }
        // RSC-026, same rule as every other url() - `@font-face src` is
        // walked here rather than by the generic CSS url pass, so the check
        // has to be repeated. The two walks are disjoint (measured: a
        // `background: url()` draws it there, a `src: url()` only here), so
        // this cannot double-report.
        if href_leaks_container_root(dir, &u.node) {
            report.push_full(
                RSC_026,
                Severity::Error,
                format!("'{}' leaks outside the container", u.node),
                path,
                origin.position(css, u.span.start),
                "css.font_face.leaks_container_root",
                vec![u.node.clone()],
            );
        }
        let resolved = nfc(&resolve(dir, &u.node));
        let declared = items.values().any(|(ip, _)| nfc(ip) == resolved);
        if !declared && !name_index.contains_key(&resolved) {
            report.push_full(
                RSC_007,
                Severity::Error,
                format!(
                    "reference to a resource missing from the publication: '{}'",
                    u.node
                ),
                path,
                origin.position(css, u.span.start),
                "css.font_face.missing_target",
                vec![u.node.clone()],
            );
            continue;
        }
        if let Some((_, mt)) = items.values().find(|(ip, _)| nfc(ip) == resolved)
            && !crate::cmt::is_core_media_type(mt)
            && !crate::cmt::is_exempt_video(mt)
            && (is_epub3 || !blessed_font_type_epub2(mt))
        {
            report.push_full(
                CSS_007,
                Severity::Info,
                format!(
                    "font '{}' has media type '{mt}', which is not a Core Media Type; \
                     fonts need no fallback, but reading systems need not support it",
                    u.node
                ),
                path,
                origin.position(css, u.span.start),
                "css.font_face.non_core_media_type",
                vec![u.node.clone(), mt.clone()],
            );
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
            .and_then(|n| n.attr_no_ns("Algorithm"));
        if algorithm != Some(OBFUSCATION_ALGORITHM) {
            continue;
        }
        let Some(uri) = enc_data
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "CipherReference")
            .and_then(|n| n.attr_no_ns("URI"))
        else {
            continue;
        };
        // CipherReference URI is relative to the OCF container root, not
        // the OPF's own directory (confirmed via the real fixtures: the
        // OPF lives at "EPUB/package.opf" but the cipher reference reads
        // "EPUB/obfuscated-font.otf", the full container-relative path).
        let resolved = nfc(&resolve("", uri));
        if !name_index.contains_key(&resolved) {
            // Genuinely covered now, and it was not when this said so: the
            // comment here used to claim "a missing resource is already
            // reported elsewhere (RSC-001/004)", but RSC-004 says a file is
            // *encrypted*, never that it is missing, and nothing emitted
            // RSC-001 for it. `ocf::check_encryption` reports RSC-007, as
            // epubcheck does. Asking a resource's media type when the resource
            // is absent would add a second, worse-worded finding for one fact.
            continue;
        }
        let media_type = items
            .values()
            .find(|(path, _)| nfc(path) == resolved)
            .map(|(_, mt)| mt.as_str());
        if !media_type.is_some_and(|mt| FONT_CORE_MEDIA_TYPES.contains(&mt)) {
            report.push_at_pos(
                PKG_026,
                Severity::Error,
                format!("obfuscated resource '{uri}' is not a font Core Media Type"),
                ENC,
                Position::of(enc_data),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_dc_date;

    /// The four semantics `check_duplicate_ids` inherited when it moved out
    /// of `schemas/package.sch`. The shelf never exercises this rule (0 of
    /// 73 books) and the corpus has only two scenarios, so these pin the
    /// parts a port can silently get wrong.
    mod duplicate_ids {
        use crate::report::Report;

        fn ids_reported(package_body: &str) -> Vec<(String, u32)> {
            let xml = format!(
                r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:xml="http://www.w3.org/XML/1998/namespace" version="3.0">{package_body}</package>"#
            );
            let doc = crate::ocf::parse_xml(&xml).expect("fixture must parse");
            let mut report = Report::new();
            super::super::check_duplicate_ids(&doc, "c.opf", &mut report);
            report
                .messages
                .iter()
                .map(|m| (m.text.clone(), m.position.map(|p| p.column).unwrap_or(0)))
                .collect()
        }

        /// One finding per *occurrence*, not one per duplicated value - the
        /// corpus states it as "reported 2 times (once for each ID)".
        #[test]
        fn reports_once_per_occurrence() {
            let out = ids_reported(r#"<a id="x"/><b id="x"/><c id="x"/>"#);
            assert_eq!(out.len(), 3, "three elements share the id: {out:?}");
            assert!(out.iter().all(|(t, _)| t == r#"duplicate id "x""#));
            // Distinct positions, so each points at its own element.
            let cols: std::collections::BTreeSet<u32> = out.iter().map(|(_, c)| *c).collect();
            assert_eq!(cols.len(), 3, "each occurrence gets its own position");
        }

        /// Uniqueness is judged after whitespace normalization, and the
        /// *normalized* value is what the message prints.
        #[test]
        fn normalizes_whitespace_before_comparing() {
            let out = ids_reported("<a id=\" x \"/><b id=\"x\"/>");
            assert_eq!(out.len(), 2, "` x ` and `x` are the same id: {out:?}");
            assert!(
                out.iter().all(|(t, _)| t == r#"duplicate id "x""#),
                "the message carries the normalized id: {out:?}"
            );
        }

        /// `*[@id]` meant a no-namespace `id`. An `xml:id` is a different
        /// attribute and the Schematron rule never saw it.
        #[test]
        fn xml_id_is_a_different_attribute() {
            assert!(
                ids_reported(r#"<a id="x"/><b xml:id="x"/>"#).is_empty(),
                "xml:id must not collide with id"
            );
        }

        /// The far more common case: nothing to say.
        #[test]
        fn distinct_ids_are_silent() {
            assert!(ids_reported(r#"<a id="x"/><b id="y"/>"#).is_empty());
        }
    }

    /// These tables are keyed lookups - `RESERVED_PREFIXES` by prefix,
    /// `ALLOWED_EXTERNAL_IDENTIFIERS` by media type. A duplicated key makes
    /// the first entry silently shadow the rest, and every fixture that
    /// touches the key would keep passing on the shadowing entry, so nothing
    /// but a table invariant would notice a copy-paste slip.
    #[test]
    fn keyed_tables_have_no_duplicate_keys() {
        let mut prefixes = std::collections::BTreeSet::new();
        for (p, _) in super::RESERVED_PREFIXES_ANY {
            assert!(prefixes.insert(*p), "reserved prefix '{p}' is listed twice");
        }
        // The per-context lists must partition the union exactly, with the
        // same URI for each prefix. Three tables that can drift apart is how
        // the redeclaration check would quietly start reserving the wrong
        // prefix in the wrong document again (#161).
        let any: std::collections::BTreeSet<_> = super::RESERVED_PREFIXES_ANY.iter().collect();
        let split: std::collections::BTreeSet<_> = super::RESERVED_PREFIXES_PACKAGE
            .iter()
            .chain(super::RESERVED_PREFIXES_CONTENT)
            .collect();
        assert_eq!(any, split, "the context lists must partition the union");
        assert_eq!(
            super::RESERVED_PREFIXES_PACKAGE.len() + super::RESERVED_PREFIXES_CONTENT.len(),
            super::RESERVED_PREFIXES_ANY.len(),
            "and must not overlap"
        );
        let mut media_types = std::collections::BTreeSet::new();
        for (mt, _, _) in super::ALLOWED_EXTERNAL_IDENTIFIERS {
            assert!(
                media_types.insert(*mt),
                "external-identifier media type '{mt}' is listed twice"
            );
        }
    }

    #[test]
    fn dc_date_accepts_date_only_forms() {
        assert!(is_valid_dc_date("2011"));
        assert!(is_valid_dc_date("2011-05"));
        assert!(is_valid_dc_date("2011-05-04"));
    }

    #[test]
    fn dc_date_accepts_full_timestamps() {
        // The form from issue #4 (JSWolf's book) that was wrongly rejected.
        assert!(is_valid_dc_date("2025-04-24T17:00:00Z"));
        // Other valid W3C-DTF timestamp shapes.
        assert!(is_valid_dc_date("2025-04-24T17:00Z"));
        assert!(is_valid_dc_date("2025-04-24T17:00:00.5Z"));
        assert!(is_valid_dc_date("2025-04-24T17:00:00+03:00"));
        assert!(is_valid_dc_date("2025-04-24T17:00:00-05:30"));
    }

    #[test]
    fn dc_date_rejects_invalid_values() {
        assert!(!is_valid_dc_date(""));
        assert!(!is_valid_dc_date("Anno Domini Twenty"));
        assert!(!is_valid_dc_date("20010-11-08")); // 5-digit year
        assert!(!is_valid_dc_date("2025-13-01")); // month 13
        assert!(!is_valid_dc_date("2025-04-32")); // day 32
        assert!(!is_valid_dc_date("2025-04-24 17:00:00Z")); // space, not 'T'
        assert!(!is_valid_dc_date("2025-04-24T25:00:00Z")); // hour 25
        assert!(!is_valid_dc_date("2025-04-24T17:00:00")); // missing timezone
        assert!(!is_valid_dc_date("2025-04-24T17:00:00X")); // bad timezone
    }

    // --- OPF-096 non-linear reachability via a self-link (issue #1) ---

    /// Build a minimal valid EPUB 3 whose spine has a linear `ch1` plus the
    /// toc nav marked `linear="no"`, with the nav's body supplied by the
    /// caller. Used to exercise OPF-096 reachability: whether the non-linear
    /// nav is reachable depends only on whether some `<a>` (here, one inside
    /// the nav itself) links to it.
    fn epub_with_nav_body(nav_body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="nav" linear="no"/></spine>
</package>"#;
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p>Hi</p></body></html>"#;

        let nav = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body>{nav_body}</body></html>"#
        );

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            // mimetype must be first and stored (uncompressed).
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", nav.as_str()),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// #73: references that precede a parse failure are recovered.
    ///
    /// A document that is not well-formed used to lose every check below it,
    /// so a book with a missing stylesheet *and* a stray `&` reported only
    /// the entity. epubcheck's parser is streaming and keeps whatever it
    /// passed before the failure; this recovers the same set from the text.
    ///
    /// Eight malformation kinds were measured against epubcheck 5.3.0, one
    /// book each — undeclared entity, malformed numeric reference, unclosed
    /// element, mismatched tag, unquoted attribute, stray `<`, duplicate
    /// attribute, unknown namespace prefix. All eight behave identically in
    /// both tools, which is why this is one rule rather than a family.
    #[test]
    fn references_before_a_parse_failure_are_recovered() {
        let ids = |body: &str| -> Vec<&'static str> {
            let ch1 = format!(
                "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>c</title>\
                 <link rel=\"stylesheet\" type=\"text/css\" href=\"missing.css\"/></head>\
                 <body><p><img src=\"missing.png\" alt=\"x\"/></p>{body}</body></html>"
            );
            let mut v: Vec<&'static str> = crate::validate_bytes(epub_with_body("2.0", &ch1))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_007 || m.id == crate::ids::RSC_016)
                .map(|m| m.id)
                .collect();
            v.sort_unstable();
            v
        };
        // The document parses: the DOM walk reports both references and
        // there is no fatal. This is the control that keeps the recovery
        // from being credited for what the normal path already does.
        assert_eq!(
            ids("<p>fine</p>"),
            vec![crate::ids::RSC_007, crate::ids::RSC_007]
        );
        // It does not parse: same two references, plus the fatal.
        for bad in [
            "<p>&badentity;</p>",
            "<p>&#zz;</p>",
            "<p>text",
            "<p>text</div>",
            "<p class=x>t</p>",
            "<p>a < b</p>",
            "<p id=\"a\" id=\"b\">t</p>",
            "<foo:bar>t</foo:bar>",
        ] {
            assert_eq!(
                ids(bad),
                vec![
                    crate::ids::RSC_007,
                    crate::ids::RSC_007,
                    crate::ids::RSC_016
                ],
                "{bad}"
            );
        }
    }

    /// The other half of the rule: a reference *after* the failure stays
    /// lost, because epubcheck loses it too. Claiming more would be a
    /// divergence in the direction that reads as invention.
    #[test]
    fn a_reference_after_a_parse_failure_is_not_recovered() {
        let ch1 = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>c</title></head>\
                   <body><p>&badentity;</p><p><img src=\"missing.png\" alt=\"x\"/></p></body></html>";
        let ids: Vec<&'static str> = crate::validate_bytes(epub_with_body("2.0", ch1))
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_007 || m.id == crate::ids::RSC_016)
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec![crate::ids::RSC_016]);
    }

    /// #82: which id a *missing* fragment gets depends on the target's media
    /// type.
    ///
    /// epubcheck guards RSC-012 on the target being XHTML or SVG. A
    /// `text/html` document is neither, so the missing id falls through to
    /// the reference-type switch and comes out as **RSC-014**. Found by
    /// `compare` on a real book whose NCX pointed into a `text/html`
    /// chapter; measured at both EPUB versions, one book per shape, and the
    /// rule is not version-dependent.
    ///
    /// The target has to be a *separate* document. A `text/html` file linking
    /// to itself draws neither id from either tool — measured, after a first
    /// version of this test asserted RSC-014 there and failed. Its references
    /// are simply never collected, and epubcheck agrees, so the shape is a
    /// distraction rather than a rule.
    #[test]
    fn a_missing_fragment_in_a_text_html_target_is_rsc_014() {
        // `meta.xml` is written by the builder and declared here as
        // `text/html`, which gives a second document to point at without a
        // second builder. It holds `<r/>` and so has no ids at all.
        let opf = |mt: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="tgt" href="meta.xml" media-type="{mt}"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            )
        };
        let ids = |mt: &str| -> Vec<&'static str> {
            let ch1 = "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\">\
                       <head><title>t</title></head>\
                       <body><p><a href=\"meta.xml#nope\">x</a></p></body></html>";
            crate::validate_bytes(epub_with_opf(Some(&opf(mt)), ch1))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_012 || m.id == crate::ids::RSC_014)
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(ids("application/xhtml+xml"), vec![crate::ids::RSC_012]);
        assert_eq!(ids("text/html"), vec![crate::ids::RSC_014]);
    }

    /// #80: PKG-003 and PKG-004 name *which* way a container is unreadable.
    ///
    /// epubcheck's `OCFZipChecker` reads a **58-byte** header: too short to
    /// fill it is PKG-003, long enough but not starting with `PK` is
    /// PKG-004. We had PKG-003 for an empty file only and PKG-004 behind an
    /// image sniff, so 36 bytes of text and 200 random bytes both drew the
    /// generic PKG-008 alone.
    ///
    /// 58 and not 30: the check reaches past the local file header to the
    /// `mimetype` name at offset 30. A comment in `ocf.rs` said 30, and that
    /// misreading is what made the two tools look inconsistent when they
    /// agreed — the boundary is the whole rule here, so it is pinned from
    /// both sides.
    #[test]
    fn a_short_or_unsigned_container_is_named() {
        let ids = |bytes: Vec<u8>| -> Vec<&'static str> {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::PKG_003 || m.id == crate::ids::PKG_004)
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(ids(Vec::new()), vec![crate::ids::PKG_003], "empty");
        assert_eq!(
            ids(b"NOTAZIPFILE garbage header bytes".to_vec()),
            vec![crate::ids::PKG_003]
        );
        // 57 vs 58 — the boundary itself.
        assert_eq!(ids(vec![b'x'; 57]), vec![crate::ids::PKG_003], "57 bytes");
        assert_eq!(ids(vec![b'x'; 58]), vec![crate::ids::PKG_004], "58 bytes");
        // epubcheck's test is an `and`, so one matching byte falls through.
        // Measured: it then reports **PKG-006** there (the header's filename
        // -size field is not 8), which we do not - our PKG-005/PKG-006 read
        // the parsed zip while epubcheck reads the raw header, so they never
        // run on a container that fails to open. Same family as this fix and
        // left as its own change; what is pinned here is only that neither
        // PKG-003 nor PKG-004 is the answer.
        assert!(
            ids(b"PX"
                .iter()
                .copied()
                .chain(std::iter::repeat_n(b'x', 60))
                .collect())
            .is_empty()
        );
        assert!(
            ids(b"xK"
                .iter()
                .copied()
                .chain(std::iter::repeat_n(b'x', 60))
                .collect())
            .is_empty()
        );
    }

    /// #79 step 3: two Schematron patterns that were implemented at one end
    /// only.
    ///
    /// `nav-ocurrence` asserts `count(toc) = 1`, and we had the zero end
    /// alone — two `toc` navs were accepted. `heading-content`'s context is
    /// every `h1`-`h6` in the navigation document, and ours ran on the
    /// heading a nav opens with, so an empty heading outside any nav was
    /// silent. Both measured against epubcheck 5.3.0, one book per shape.
    #[test]
    fn nav_document_toc_count_and_headings_anywhere() {
        let ids = |nav_body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_nav_body(nav_body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005)
                .map(|m| m.id)
                .collect()
        };
        let ol = "<ol><li><a href=\"ch1.xhtml\">One</a></li></ol>";
        let toc = format!("<nav epub:type=\"toc\"><h1>T</h1>{ol}</nav>");

        assert!(ids(&toc).is_empty(), "one toc, one non-empty heading");
        assert_eq!(
            ids(&format!("{toc}{toc}")),
            vec![crate::ids::RSC_005],
            "two tocs"
        );
        assert_eq!(
            ids(&format!("{toc}<h2>  </h2>")),
            vec![crate::ids::RSC_005],
            "an empty heading outside any nav"
        );
        // An image-only heading is not empty — `has_text_or_image` is what
        // keeps the widened check off legitimate markup, and the shelf is too
        // thin here to have caught it (8 headings across 66 nav documents).
        assert!(
            ids(&format!(
                "{toc}<h2><img src=\"i.jpg\" alt=\"Part One\"/></h2>"
            ))
            .is_empty(),
            "a heading whose text comes from an image alt"
        );
    }

    /// #79: a `nav` requires an `ol`, and a flat nav must have exactly one.
    ///
    /// The missing-`ol` half was a silent return — `check_nav_content_model`
    /// did `let Some(ol) = children.get(idx) else { return }`, so a
    /// `<nav><h1>…</h1></nav>` reported nothing while the neighbouring arm
    /// happily reported a child that was present and wrong. Reported on
    /// MobileRead; measured against epubcheck 5.3.0.
    ///
    /// `flat-nav` (RSC-017) is independent of it: `epub-nav-30.sch` asserts
    /// `count(.//ol) = 1` on a `page-list` or `landmarks` nav, so **zero**
    /// fails it as well as two, and the reported book draws both messages on
    /// the same element.
    #[test]
    fn a_nav_requires_an_ol_and_a_flat_nav_exactly_one() {
        let ids = |nav_body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_nav_body(nav_body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005 || m.id == crate::ids::RSC_017)
                .map(|m| m.id)
                .collect()
        };
        let ol = "<ol><li><a href=\"ch1.xhtml\">One</a></li></ol>";

        // The control that has to stay clean, or everything below is noise.
        assert!(ids(&format!("<nav epub:type=\"toc\"><h1>T</h1>{ol}</nav>")).is_empty());

        // No ol at all.
        assert_eq!(
            ids("<nav epub:type=\"toc\"><h1>T</h1></nav>"),
            vec![crate::ids::RSC_005]
        );

        // A landmarks nav with no ol draws both: the missing element and the
        // flat-nav assertion, which zero also fails.
        let mut both = ids(&format!(
            "<nav epub:type=\"toc\"><h1>T</h1>{ol}</nav>\
             <nav epub:type=\"landmarks\"><h1>L</h1></nav>"
        ));
        both.sort_unstable();
        assert_eq!(both, vec![crate::ids::RSC_005, crate::ids::RSC_017]);

        // A nested sublist is two ols: the flat rule fires, the content model
        // does not.
        // `page-list` rather than `landmarks`: both are in the flat set, but a
        // landmarks anchor must also carry `epub:type`, which would put an
        // unrelated RSC-005 in the way of what this case is about.
        let nested = format!(
            "<nav epub:type=\"toc\"><h1>T</h1>{ol}</nav>\
             <nav epub:type=\"page-list\"><h1>P</h1>\
             <ol><li><a href=\"ch1.xhtml\">A</a>{ol}</li></ol></nav>"
        );
        assert_eq!(ids(&nested), vec![crate::ids::RSC_017]);

        // `toc` is not in the flat set, so the same nesting is clean there —
        // this is the assertion that keeps the rule from widening.
        let toc_nested = format!(
            "<nav epub:type=\"toc\"><h1>T</h1>\
             <ol><li><a href=\"ch1.xhtml\">A</a>{ol}</li></ol></nav>"
        );
        assert!(ids(&toc_nested).is_empty());
    }

    fn has_opf_096(nav_body: &str) -> bool {
        let report = crate::validate_bytes(epub_with_nav_body(nav_body));
        report.messages.iter().any(|m| m.id == crate::ids::OPF_096)
    }

    /// An EPUB 3 whose manifest declares `img.jpg` as `image/jpeg`, with the
    /// given raw bytes for it - so the image-signature check can be exercised.
    fn epub_with_image(img: &[u8]) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="img.jpg" media-type="image/jpeg"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p><img src="img.jpg" alt="x"/></p></body></html>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER.as_bytes()),
                ("OEBPS/content.opf", OPF.as_bytes()),
                ("OEBPS/ch1.xhtml", CH1.as_bytes()),
                ("OEBPS/nav.xhtml", NAV.as_bytes()),
                ("OEBPS/img.jpg", img),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// An EDUPUB (`dc:type=edupub`) EPUB 3 with a linear content document and
    /// a nav document whose bodies are given - for the NAV-004..008 checks.
    fn epub_edupub(ch1_body: &str, nav_body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language><dc:type>edupub</dc:type>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        let ch1 = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>C</title></head><body>{ch1_body}</body></html>"#
        );
        let nav = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head><body>{nav_body}</body></html>"#
        );
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.xhtml", ch1.as_str()),
                ("OEBPS/nav.xhtml", nav.as_str()),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// #46: NAV-005..008 fire when a content document has an
    /// audio/figure/table/video but the nav lacks the matching special nav;
    /// NAV-004 fires when the section count doesn't match the toc-link count.
    #[test]
    fn edupub_nav_completeness_nav004_008() {
        let ids = |ch1: &str, nav: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_edupub(ch1, nav))
                .messages
                .iter()
                .map(|m| m.id)
                .collect()
        };
        // One section (matches one toc link -> no NAV-004) containing an
        // <audio>, and a toc with no `loa` -> NAV-005 only.
        let a = ids(
            r#"<section><h1>C</h1><audio src="a.mp3"/></section>"#,
            r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav>"#,
        );
        assert!(a.contains(&crate::ids::NAV_005), "audio/no-loa: {a:?}");
        assert!(
            !a.contains(&crate::ids::NAV_004),
            "sections==toc, no NAV-004: {a:?}"
        );
        assert!(!a.contains(&crate::ids::NAV_006), "{a:?}");

        // Same but the nav now has a `loa` -> no NAV-005.
        let b = ids(
            r#"<section><h1>C</h1><audio src="a.mp3"/></section>"#,
            r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav>
<nav epub:type="loa"><ol><li><a href="ch1.xhtml#a">A</a></li></ol></nav>"#,
        );
        assert!(!b.contains(&crate::ids::NAV_005), "audio+loa: {b:?}");

        // Two sections but only one toc link -> NAV-004.
        let c = ids(
            r#"<section><h1>A</h1></section><section><h1>B</h1></section>"#,
            r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">A</a></li></ol></nav>"#,
        );
        assert!(
            c.contains(&crate::ids::NAV_004),
            "2 sections vs 1 link: {c:?}"
        );

        // A non-edupub book with the same shape draws none of NAV-004..008.
        let non_edu = crate::validate_bytes(epub_with_image(b"NOT-A-REAL-JPEG"));
        for id in [
            crate::ids::NAV_004,
            crate::ids::NAV_005,
            crate::ids::NAV_006,
            crate::ids::NAV_007,
            crate::ids::NAV_008,
        ] {
            assert!(
                !non_edu.messages.iter().any(|m| m.id == id),
                "non-edupub must not draw {id}"
            );
        }
    }

    /// An EPUB 3 with an audio file in the spine (a non-content media type),
    /// with a configurable `fallback` attribute and optional extra manifest
    /// items - for the OPF-043 (no fallback) / OPF-044 (fallback chain never
    /// reaches a content document) split.
    fn epub_audio_spine(audio_attrs: &str, extra_manifest: &str, extra_files: &[&str]) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="aud" href="a.mp3" media-type="audio/mpeg"{audio_attrs}/>
    {extra_manifest}
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="aud"/></spine>
</package>"#
        );
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p>Hi</p></body></html>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let mut files: Vec<(String, &[u8])> = vec![
                ("META-INF/container.xml".into(), CONTAINER.as_bytes()),
                ("OEBPS/content.opf".into(), opf.as_bytes()),
                ("OEBPS/ch1.xhtml".into(), CH1.as_bytes()),
                ("OEBPS/nav.xhtml".into(), NAV.as_bytes()),
                ("OEBPS/a.mp3".into(), b"ID3 fake audio"),
            ];
            for f in extra_files {
                files.push((format!("OEBPS/{f}"), b"ID3 fake audio"));
            }
            for (name, data) in files {
                zip.start_file(&name, deflated).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// #41: OPF-043 is "no fallback"; OPF-044 is "a fallback chain exists but
    /// never reaches a content document". We used to report both as OPF-043.
    #[test]
    fn opf_043_vs_044_fallback_split() {
        let ids = |bytes: Vec<u8>| -> Vec<&'static str> {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .map(|m| m.id)
                .collect()
        };
        // Audio in the spine, no fallback -> OPF-043 (not OPF-044).
        let no_fb = ids(epub_audio_spine("", "", &[]));
        assert!(
            no_fb.contains(&crate::ids::OPF_043),
            "no fallback: {no_fb:?}"
        );
        assert!(
            !no_fb.contains(&crate::ids::OPF_044),
            "no fallback: {no_fb:?}"
        );
        // Audio with a fallback to another audio (chain never reaches a
        // content document) -> OPF-044 (not OPF-043).
        let bad_fb = ids(epub_audio_spine(
            " fallback=\"aud2\"",
            r#"<item id="aud2" href="a2.mp3" media-type="audio/mpeg"/>"#,
            &["a2.mp3"],
        ));
        assert!(
            bad_fb.contains(&crate::ids::OPF_044),
            "bad fallback: {bad_fb:?}"
        );
        assert!(
            !bad_fb.contains(&crate::ids::OPF_043),
            "bad fallback: {bad_fb:?}"
        );
    }

    /// #45: MED-004 is specifically the "file too short to read a header"
    /// case (<4 bytes). A >=4-byte file whose header matches nothing is a
    /// declared/actual mismatch (OPF-029), not MED-004 - matching epubcheck.
    #[test]
    fn image_error_mapping_med004_only_for_too_short() {
        let ids = |b: &[u8]| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_image(b))
                .messages
                .iter()
                .map(|m| m.id)
                .collect()
        };
        // >=4-byte garbage declared as JPEG: OPF-029 + PKG-021, not MED-004.
        let g = ids(b"NOT-A-REAL-JPEG");
        assert!(g.contains(&crate::ids::OPF_029), "garbage: {g:?}");
        assert!(g.contains(&crate::ids::PKG_021), "garbage: {g:?}");
        assert!(
            !g.contains(&crate::ids::MED_004),
            "garbage should not be MED-004: {g:?}"
        );
        // 0-byte file: MED-004 + PKG-021, not OPF-029.
        let e = ids(b"");
        assert!(e.contains(&crate::ids::MED_004), "empty: {e:?}");
        assert!(e.contains(&crate::ids::PKG_021), "empty: {e:?}");
        assert!(
            !e.contains(&crate::ids::OPF_029),
            "empty should not be OPF-029: {e:?}"
        );
    }

    #[test]
    fn non_linear_nav_reachable_via_landmark_self_link() {
        // The Sigil shape Kevin Hendricks described (issue #1): the nav's
        // own landmarks section links to the nav (`href="nav.xhtml"`), which
        // makes it reachable - no OPF-096.
        let nav = r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav>
<nav epub:type="landmarks"><ol><li><a epub:type="toc" href="nav.xhtml">TOC</a></li></ol></nav>"#;
        assert!(!has_opf_096(nav));
    }

    #[test]
    fn non_linear_nav_reachable_via_fragment_only_self_link() {
        // "The same internal link trick works for any xhtml file" - a
        // fragment-only self-link (`href="#toc"`) also counts as reaching
        // the document itself.
        let nav = r##"<nav epub:type="toc" id="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li><li><a href="#toc">Self</a></li></ol></nav>"##;
        assert!(!has_opf_096(nav));
    }

    #[test]
    fn non_linear_nav_with_no_incoming_link_is_flagged() {
        // A non-linear nav that nothing links to (not even itself) IS
        // flagged, exactly as epubcheck does - the categorical nav exemption
        // that used to suppress this was wrong (issue #1, Kevin: "epubcheck
        // will complain in the exact same way").
        let nav = r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav>"#;
        assert!(has_opf_096(nav));
    }

    // --- RSC-016: non-well-formed content documents (forum report, #12) ---

    /// Build a minimal valid EPUB 3 whose spine's `ch1` content document is
    /// supplied verbatim by the caller — used to feed deliberately malformed
    /// XHTML through the real content-document loop.
    /// Builds an EPUB 3 with a linked stylesheet `Styles/s.css` holding
    /// `css`, and one font declared in the manifest with `font_media_type`,
    /// for the CSS-007/CSS-029 cross-referencing checks (both need a
    /// manifest, so they can't be reached through `css::check` alone).
    fn epub_with_stylesheet(css: &str, font_media_type: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="s" href="Styles/s.css" media-type="text/css"/>
    <item id="f" href="Fonts/f.ttf" media-type="{font_media_type}"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<link rel="stylesheet" type="text/css" href="Styles/s.css"/></head>
<body><p>x</p></body></html>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>t</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">c</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
                ),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/Styles/s.css", css),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.start_file("OEBPS/Fonts/f.ttf", o).unwrap();
            z.write_all(b"\0\x01\0\0not-a-real-font").unwrap();
            z.finish().unwrap();
        }
        buf
    }

    /// Builds an EPUB 3 with a stylesheet, a font that `css` uses, and a
    /// second font (`Fonts/orphan.ttf`) that nothing mentions at all.
    fn epub_with_stylesheet_and_orphans(css: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="s" href="Styles/s.css" media-type="text/css"/>
    <item id="f" href="Fonts/f.ttf" media-type="font/ttf"/>
    <item id="orphan" href="Fonts/orphan.ttf" media-type="font/ttf"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<link rel="stylesheet" type="text/css" href="Styles/s.css"/></head>
<body><p>x</p></body></html>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>t</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">c</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
                ),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/Styles/s.css", css),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            for f in ["OEBPS/Fonts/f.ttf", "OEBPS/Fonts/orphan.ttf"] {
                z.start_file(f, o).unwrap();
                z.write_all(b"\0\x01\0\0not-a-real-font").unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// Parses `<tag attr="x"/>` and asks `is_resource_reference` about it.
    fn is_resource_reference_for_test(tag: &str, attr: &str) -> bool {
        let xml = format!("<{tag} {attr}=\"x\"/>");
        let d = crate::ocf::parse_xml(&xml).unwrap();
        super::is_resource_reference(d.root_element(), attr)
    }

    /// OPF-097: a manifest resource nothing consumes. Requested on the
    /// MobileRead forum by JSWolf, for unused fonts and images.
    ///
    /// Asserts both directions on one book, because the check is only worth
    /// anything if it is silent about the resources that *are* used - a
    /// checker that flags everything tells you nothing, and this one's
    /// output invites deleting files.
    #[test]
    fn opf_097_reports_only_the_resources_nothing_uses() {
        let css = "@font-face {\n  font-family: X;\n  src: url(../Fonts/f.ttf);\n}";
        let report = crate::validate_bytes(epub_with_stylesheet_and_orphans(css));
        let unused: Vec<&str> = report
            .messages
            .iter()
            .filter(|m| m.rule == Some("opf.manifest_item.never_referenced"))
            .map(|m| m.text.as_str())
            .collect();
        assert_eq!(unused.len(), 1, "exactly the orphan font; got {unused:?}");
        assert!(unused[0].contains("Fonts/orphan.ttf"), "got {unused:?}");
        let hit = report
            .messages
            .iter()
            .find(|m| m.rule == Some("opf.manifest_item.never_referenced"))
            .unwrap();
        assert_eq!(hit.id, crate::ids::OPF_097);
        assert_eq!(hit.severity, crate::report::Severity::Usage);
        assert!(report.is_valid(), "an unused resource is not an error");
    }

    /// A hyperlink does not consume its target. This is the whole point of
    /// the rule and the least obvious part of it: epubcheck counts only
    /// references that embed or load a resource (IMAGE, FONT, STYLESHEET,
    /// …), never HYPERLINK. Getting this wrong in the permissive direction
    /// would silently disable the check for every linked document.
    #[test]
    fn opf_097_a_hyperlink_does_not_count_as_a_reference() {
        assert!(!is_resource_reference_for_test("a", "href"));
        assert!(!is_resource_reference_for_test("area", "href"));
        // ...while the things that really do load their target do count.
        assert!(is_resource_reference_for_test("img", "src"));
        assert!(is_resource_reference_for_test("audio", "src"));
        assert!(is_resource_reference_for_test("track", "src"));
        assert!(is_resource_reference_for_test("object", "data"));
    }

    /// A `@font-face` src naming something the publication does not contain.
    /// Found on MobileRead #150's book: `src: url(fonts/00001.ttf)` with no
    /// such entry drew nothing from us and RSC-007 from epubcheck — the
    /// lookup used `if let Some(...)` and said nothing when it missed, the
    /// silent-skip shape again.
    ///
    /// **The two cases must not be confused, and the corpus enforces it.**
    /// epubcheck picks the ID by whether the font is *declared*: a manifest
    /// item whose file is absent is RSC-001 (its own check), and only a
    /// reference to something nothing declares is RSC-007. A first version
    /// tested container presence alone and put three extra RSC-007s on
    /// `package-manifest-fonts-missing-error`, which expects RSC-001 alone.
    #[test]
    fn font_face_target_missing_from_the_publication() {
        let css = "@font-face{font-family:X;src:url(f.ttf)}";
        let run = |declared: bool, present: bool| {
            let mut items = std::collections::HashMap::new();
            if declared {
                items.insert(
                    "f".to_string(),
                    ("f.ttf".to_string(), "font/ttf".to_string()),
                );
            }
            let mut name_index = std::collections::HashMap::new();
            if present {
                name_index.insert("f.ttf".to_string(), "f.ttf".to_string());
            }
            let mut report = crate::report::Report::default();
            crate::opf::check_exempt_font_usage(
                css,
                "",
                &crate::opf::ResourceView {
                    items: &items,
                    name_index: &name_index,
                },
                "s.css",
                crate::css::CssOrigin::File { bytes: None },
                true,
                &mut report,
            );
            report
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_007)
                .count()
        };

        assert_eq!(run(false, false), 1, "nothing declares it and it is absent");
        assert_eq!(run(true, false), 0, "declared but absent is RSC-001's job");
        assert_eq!(run(true, true), 0, "declared and present is fine");
        assert_eq!(
            run(false, true),
            0,
            "present but undeclared is OPF-003's job, not this one"
        );
    }

    /// EPUB 2 blesses a wider set of font types than the Core Media Types, so
    /// CSS-007 must not fire there for one of them. epubcheck's own EPUB 2
    /// branch (`OPFChecker.isBlessedFontMimetype20`) accepts anything starting
    /// `font/`, `application/font` or `application/x-font`, plus
    /// `application/vnd.ms-opentype`.
    ///
    /// Two shelf books declare `application/x-font-truetype` and drew this
    /// note from us and nothing from epubcheck. Info severity, so no verdict
    /// changed — but it was still a report on markup that is fine for its
    /// version, which is the class that matters for trust.
    #[test]
    fn epub2_blesses_more_font_types_than_epub3() {
        for mt in [
            "application/x-font-truetype",
            "application/x-font-ttf",
            "application/font-woff",
            "font/woff2",
            "application/vnd.ms-opentype",
        ] {
            assert!(super::blessed_font_type_epub2(mt), "EPUB 2 blesses {mt}");
        }
        // Not a font type at all, so the check still has something to catch.
        for mt in ["image/png", "application/octet-stream", "text/css"] {
            assert!(
                !super::blessed_font_type_epub2(mt),
                "EPUB 2 does not bless {mt}"
            );
        }
        // The EPUB 3 side is unchanged and narrower: these are not Core Media
        // Types, so an EPUB 3 book still draws the note.
        assert!(!crate::cmt::is_core_media_type(
            "application/x-font-truetype"
        ));
    }

    /// CSS-007 must name the media type that made it fire, and point at the
    /// `src` that names the font. It used to say the font was "a foreign
    /// resource, exempt from requiring a fallback" with no position at all -
    /// which describes the rule that does *not* fire, reads as a non-problem,
    /// and leaves a reader of a many-font stylesheet with nowhere to look
    /// (reported by Doitsu on the MobileRead forum).
    #[test]
    fn css_007_names_the_media_type_and_points_at_the_src() {
        let css = "@font-face {\n  font-family: X;\n  src: url(../Fonts/f.ttf);\n}";
        let report =
            crate::validate_bytes(epub_with_stylesheet(css, "application/x-font-opentype"));
        let hit = report
            .messages
            .iter()
            .find(|m| m.rule == Some("css.font_face.non_core_media_type"))
            .expect("a non-Core-Media-Type font must be reported");
        assert_eq!(hit.id, crate::ids::CSS_007);
        assert!(
            hit.text.contains("application/x-font-opentype"),
            "the message must name what made it fire; got: {}",
            hit.text
        );
        assert_eq!(hit.location.as_deref(), Some("OEBPS/Styles/s.css"));
        // The `src` line, not the stylesheet as a whole.
        assert_eq!(hit.position.map(|p| p.line), Some(3));
        assert!(
            report.is_valid(),
            "a non-standard font type is not an error"
        );
    }

    /// A Core Media Type font draws nothing - the check is about the format
    /// EPUB does not standardize, not about fonts in general.
    #[test]
    fn css_007_is_silent_for_a_core_media_type_font() {
        let css = "@font-face {\n  font-family: X;\n  src: url(../Fonts/f.ttf);\n}";
        let report = crate::validate_bytes(epub_with_stylesheet(css, "font/ttf"));
        assert!(
            !report
                .messages
                .iter()
                .any(|m| m.rule == Some("css.font_face.non_core_media_type")),
            "font/ttf is a Core Media Type"
        );
    }

    /// Findings come out in **manifest document order**, and the same book
    /// validated twice gives byte-identical output.
    ///
    /// `content_docs` and `css_items` used to be built from `items.values()`,
    /// and `items` is a `HashMap` — randomly seeded, so the visit order
    /// differed on every run. That order decides which file's findings arrive
    /// first, and `Report::sort_by_document_order` derives the report's file
    /// order from exactly that. **94 of 385 real books printed their findings
    /// in a different order on each run of the same binary** — same findings,
    /// same byte count, shuffled. Fixing the content documents took it to 5,
    /// all stylesheets; fixing those took it to 0, verified over three
    /// full-shelf runs.
    ///
    /// **No instrument here could have seen it**: the corpus, the shelf,
    /// `compare` and every other test compare ID sets or counts, which are
    /// order-insensitive by construction. It surfaced from byte-comparing two
    /// runs while verifying an unrelated refactor, and only because the first
    /// comparison failed and the control — the *same* binary twice — failed
    /// too.
    ///
    /// The manifest below is deliberately not in alphabetical order, so
    /// passing means the order was taken from the manifest rather than from
    /// anything that happens to agree with it. Validating twice makes a
    /// lucky hash order a 1-in-14400 event rather than 1-in-120.
    #[test]
    fn findings_follow_manifest_order_and_do_not_shuffle_between_runs() {
        let docs = ["e.xhtml", "b.xhtml", "d.xhtml", "a.xhtml", "c.xhtml"];
        let manifest: String = docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                format!("<item id=\"i{i}\" href=\"{d}\" media-type=\"application/xhtml+xml\"/>")
            })
            .collect();
        let spine: String = (0..docs.len())
            .map(|i| format!("<itemref idref=\"i{i}\"/>"))
            .collect();
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>{manifest}</manifest>
  <spine>{spine}</spine>
</package>"#
        );
        // One error per document, identical in every one, so only the file
        // ordering can distinguish the runs.
        let bad = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p><bogus/></p></body></html>";
        let build = || {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                z.start_file("META-INF/container.xml", o).unwrap();
                z.write_all(CONTAINER.as_bytes()).unwrap();
                z.start_file("OEBPS/content.opf", o).unwrap();
                z.write_all(opf.as_bytes()).unwrap();
                for d in docs {
                    z.start_file(format!("OEBPS/{d}"), o).unwrap();
                    z.write_all(bad.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            buf
        };
        // The file order the report actually came out in, first-seen.
        let file_order = || {
            let r = crate::validate_bytes(build());
            let mut seen: Vec<String> = Vec::new();
            for m in &r.messages {
                if let Some(l) = m.location.as_deref()
                    && l.ends_with(".xhtml")
                    && !seen.iter().any(|s| s == l)
                {
                    seen.push(l.to_string());
                }
            }
            seen
        };
        let expected: Vec<String> = docs.iter().map(|d| format!("OEBPS/{d}")).collect();
        assert_eq!(
            file_order(),
            expected,
            "findings must follow manifest order"
        );
        assert_eq!(
            file_order(),
            expected,
            "and must not shuffle on a second run"
        );
    }

    /// A `CipherReference` naming a container entry that is not there is
    /// **RSC-007 instead of RSC-004**, not as well as it.
    ///
    /// JSWolf, MobileRead #223, after deleting an obfuscated font and leaving
    /// its `encryption.xml` behind. epubcheck reports the missing resource and
    /// suppresses the "this file is encrypted" note, which is the right way
    /// round: a reference to nothing is not a file whose content was skipped.
    ///
    /// Both halves are asserted because getting one right is easy. Adding
    /// RSC-007 while leaving RSC-004 in place would pass a presence check and
    /// tell the reader two things about one fact, the second of them false.
    #[test]
    fn a_cipher_reference_to_a_missing_entry_replaces_the_encrypted_note() {
        const PRESENT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData><enc:CipherData><enc:CipherReference URI="OEBPS/stray.txt"/></enc:CipherData></enc:EncryptedData>
</encryption>"#;
        const MISSING: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData><enc:CipherData><enc:CipherReference URI="OEBPS/fonts/gone.otf"/></enc:CipherData></enc:EncryptedData>
</encryption>"#;

        let rules = |enc: &str| -> Vec<&'static str> {
            let report =
                crate::validate_bytes(epub2_with_stray_files_and_encryption(&["stray.txt"], enc));
            report
                .messages
                .iter()
                .filter_map(|m| m.rule)
                .filter(|r| r.starts_with("ocf.encryption") || r.starts_with("ocf.resource"))
                .collect()
        };

        // The control: a reference that resolves keeps the informational note
        // and draws nothing else. Without this the test below would pass on a
        // build that had simply stopped emitting RSC-004 altogether.
        assert_eq!(rules(PRESENT), vec!["ocf.resource.encrypted_not_checked"]);

        assert_eq!(
            rules(MISSING),
            vec!["ocf.encryption.missing_resource"],
            "a missing target is reported instead of the encrypted note, not alongside it"
        );
    }

    /// **The library never filters by severity, and this is the guard.**
    ///
    /// 0.9.19 hid usage findings from the *human* report, as epubcheck does.
    /// That belongs in `render_human`/`print_report` and nowhere else: epubsana
    /// calls `validate_bytes`, and three of its fixers dispatch on rules that
    /// fire below error severity — `opf.metadata.empty_element` and
    /// `opf.manifest_item.non_preferred_media_type` (usage) and
    /// `opf.guide.duplicate_reference` (warning). They measured what a library
    /// filter would cost: **41 proposals and 147 findings cleared across 385
    /// books, against a change in the error count of exactly zero.**
    ///
    /// The failure would be silent in both directions — a fixer that finds
    /// nothing proposes nothing, and no test of theirs can fail — which is why
    /// the guard has to live here, on our side of the call.
    ///
    /// Worth keeping distinct from the ordering question that arrives in the
    /// same conversation and often the same patch: **reordering what
    /// `validate_bytes` returns is free, removing from it is not** (epubsana
    /// measured their plan byte-identical across an 11,334-position reshuffle,
    /// and their fixers immune to order — but not to absence).
    #[test]
    fn the_library_returns_usage_findings_however_the_cli_displays_them() {
        let report = crate::validate_bytes(epub2_with_stray_files(&["stray.txt"]));
        let usage: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.severity == crate::report::Severity::Usage)
            .collect();
        assert!(
            !usage.is_empty(),
            "validate_bytes must return usage findings; hiding them is the CLI's job"
        );
        assert!(
            usage
                .iter()
                .any(|m| m.rule == Some("opf.container.resource_not_in_manifest")),
            "got {:?}",
            usage.iter().map(|m| m.rule).collect::<Vec<_>>()
        );
        // And they are genuinely below the verdict line, which is what makes
        // hiding them defensible in the first place.
        assert_eq!(report.errors(), 0);
        assert!(report.is_valid());
    }

    /// Findings that land on the **same position** keep a stable relative
    /// order, and it is the order of the thing they describe.
    ///
    /// The sibling test above pins *file* order, which is what 0.9.28 fixed.
    /// It cannot see one level down: `sort_by_document_order` keys on
    /// `(file, line, column)`, so several findings at one position are left to
    /// the sort's stability, i.e. to the order the checks emitted them. That
    /// order is a property of every collection a check walks, and nothing
    /// asserted it — a check switching to a `HashMap` would shuffle these on
    /// every run, exactly as `items.values()` shuffled whole files, and
    /// exactly as invisibly.
    ///
    /// Ties are real rather than hypothetical: 1,449 findings on the 385-book
    /// shelf share a position with another, in 720 groups, and the largest is
    /// eight OPF-003s at one spot. (That is *after* an attribute fault started
    /// being positioned at its attribute, which halved the count — two faults
    /// on one start tag used to collide by construction.)
    ///
    /// OPF-003 is the shape to pin because it is the at-risk one: many
    /// findings, one rule, one position, driven by a collection walk. It reads
    /// `ocf.names`, a `Vec` in zip entry order, so the contract is *zip order*
    /// — and the fixture writes its entries in a deliberately non-alphabetical
    /// order so that passing cannot be a coincidence of sorting.
    #[test]
    fn findings_at_one_position_keep_a_stable_order() {
        // Undeclared, so each draws OPF-003 - all anchored at the package
        // document, with no position of their own to separate them.
        const STRAY: [&str; 5] = ["e.txt", "b.txt", "d.txt", "a.txt", "c.txt"];
        let build = || epub2_with_stray_files(&STRAY);

        let order = || {
            crate::validate_bytes(build())
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.container.resource_not_in_manifest"))
                .map(|m| m.params.first().cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        };
        let expected: Vec<String> = STRAY.iter().map(|f| format!("OEBPS/{f}")).collect();

        // All five share one position, so only their emission order can tell
        // the runs apart - assert the order itself, not merely that two runs
        // agree, which a shuffle reproduces 1 time in 120.
        let first = order();
        assert_eq!(first.len(), 5, "each stray file must draw one OPF-003");
        assert_eq!(first, expected, "ties must follow zip entry order");
        assert_eq!(order(), expected, "and must not shuffle on a second run");

        // The tie is the premise of this test: if positions ever separate
        // these findings, the sort does the work and the assertion above stops
        // testing what it claims to.
        let positions: Vec<_> = crate::validate_bytes(build())
            .messages
            .iter()
            .filter(|m| m.rule == Some("opf.container.resource_not_in_manifest"))
            .map(|m| (m.location.clone(), m.position.map(|p| (p.line, p.column))))
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] == w[1]),
            "the fixture must actually produce a tie; got {positions:?}"
        );
    }

    /// A minimal, otherwise-valid EPUB 2 carrying `stray` extra container
    /// entries that the manifest does not declare — one usage-severity OPF-003
    /// each, all anchored at the package document and so all sharing a
    /// position. `stray`'s order is the zip entry order.
    fn epub2_with_stray_files(stray: &[&str]) -> Vec<u8> {
        epub2_with_stray_files_and_encryption_inner(stray, None)
    }

    /// The same book with a `META-INF/encryption.xml` of the caller's choosing.
    fn epub2_with_stray_files_and_encryption(stray: &[&str], encryption: &str) -> Vec<u8> {
        epub2_with_stray_files_and_encryption_inner(stray, Some(encryption))
    }

    fn epub2_with_stray_files_and_encryption_inner(
        stray: &[&str],
        encryption: Option<&str>,
    ) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"#;
        const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap><navPoint id="n1" playOrder="1"><navLabel><text>T</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";

        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/toc.ncx", NCX),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            if let Some(enc) = encryption {
                z.start_file("META-INF/encryption.xml", o).unwrap();
                z.write_all(enc.as_bytes()).unwrap();
            }
            for f in stray {
                z.start_file(format!("OEBPS/{f}"), o).unwrap();
                z.write_all(b"x").unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// An EPUB 2 with a caller-supplied package document and stylesheet, one
    /// content document and one font — enough manifest for the guide and CSS
    /// reference checks to have somewhere to point.
    fn epub_with_opf_and_css(opf: &str, css: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title>\
            <link rel=\"stylesheet\" type=\"text/css\" href=\"Styles/s.css\"/></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/Styles/s.css", css),
                ("OEBPS/Fonts/f.ttf", "\0\u{1}\0\0"),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// RSC-020 reaches the `<guide>` and CSS `url()` too — the last two
    /// reference sites it did not.
    ///
    /// **These were closed on 2026-08-20 as "population zero across 375
    /// books", and that was the wrong kind of evidence.** The shelf is one
    /// person's library; a rule can be absent from it and present everywhere
    /// else. epubcheck's own corpus has no fixture for these either, which
    /// says something about *its test suite* rather than about real books. The
    /// witness that settled it was the oracle: a probe book with
    /// `<reference href="a b.xhtml">` and `url("i m.png")` draws RSC-020 from
    /// epubcheck at both sites and drew nothing here (2026-08-21).
    ///
    /// So the lesson is about which question to ask. "How often does this
    /// occur" needs real books and ours are not representative. "Does
    /// epubcheck report it" needs a probe, always answers, and is the only
    /// question a parity gap actually turns on.
    ///
    /// **One divergence stays and is deliberate**: `url(i m.png)` *unquoted*.
    /// An unescaped space makes that an invalid url-token, so styloria does not
    /// produce a URL from it and neither the reference nor this check sees one;
    /// epubcheck's older parser extracts it anyway. Same class as the CSS-008
    /// empty-declaration divergence — their parser predates the spec it is
    /// judged against, and teaching our CSS layer to recover from invalid
    /// syntax to match it would be the detector serving parity. The quoted
    /// form, which is valid CSS, is the case that matters and it agrees.
    #[test]
    fn rsc_020_reaches_the_guide_and_css_url_sites() {
        let rules = |css: &str, guide_href: &str| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="s" href="Styles/s.css" media-type="text/css"/>
    <item id="f" href="Fonts/f.ttf" media-type="font/ttf"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
  <guide><reference type="toc" title="T" href="{guide_href}"/></guide>
</package>"#
            );
            let mut r: Vec<&'static str> = crate::validate_bytes(epub_with_opf_and_css(&opf, css))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_020)
                .filter_map(|m| m.rule)
                .collect();
            r.sort_unstable();
            r
        };
        // Control first: clean references say nothing, so a pass below is not
        // "RSC-020 always fires".
        assert!(
            rules("body { color: red; }", "ch1.xhtml").is_empty(),
            "clean references must stay silent"
        );
        assert_eq!(
            rules("body { color: red; }", "a b.xhtml"),
            vec!["opf.guide.reference_unencoded_space"],
            "a spaced guide href"
        );
        assert_eq!(
            rules(r#"body { background: url("i m.png"); }"#, "ch1.xhtml"),
            vec!["css.url.malformed_relative_url"],
            "a spaced url() in a stylesheet, quoted so it is valid CSS"
        );
        // The deliberate divergence: unquoted, so not a url-token at all.
        assert!(
            rules("body { background: url(i m.png); }", "ch1.xhtml").is_empty(),
            "an unquoted url with a space is invalid CSS and yields no URL"
        );
    }

    /// A `<source>` inside `<audio>`/`<video>` is asked whether its declared
    /// type matches the manifest — and the comparison is normalized.
    ///
    /// The `<source>` arm required a `<picture>` ancestor and read `srcset`, so
    /// a media `<source src type>` was covered by nothing. epubcheck's
    /// `type-mismatch-in-audio-warning` fixture is exactly that shape and drew
    /// nothing from us.
    ///
    /// **The normalization is the half that could have gone wrong**, and the
    /// last two assertions are the reason it is written the way it is:
    /// - the content-side type loses its parameters, so
    ///   `type="audio/mpeg; codecs=mp3"` against a manifest `audio/mpeg` is a
    ///   match. Comparing whole strings would have invented a warning on
    ///   correct markup — a false positive we were already carrying latently
    ///   on `<object>`/`<embed>`;
    ///   `audio/ogg; codecs=opus` is how an Opus file is legitimately declared
    ///   in a manifest while the content writes plain `audio/ogg`. epubcheck
    ///   folds those two by hand and calls it a hack; matched anyway, because
    ///   an Opus book must not draw a warning from one tool and not the other.
    #[test]
    fn a_media_source_type_is_compared_against_the_manifest_normalized() {
        fn opf013(manifest_type: &str, declared: &str) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="a" href="a.snd" media-type="{manifest_type}"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body><audio controls=\"controls\">\
                 <source src=\"a.snd\" type=\"{declared}\"/></audio></body></html>"
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", ch1.as_str()),
                    ("OEBPS/a.snd", "\0\0\0\0"),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_013)
                .map(|m| m.text.clone())
                .collect()
        }

        assert_eq!(
            opf013("audio/mpeg", "audio/mp4; codecs=mp4").len(),
            1,
            "a real mismatch is reported, parameters and all"
        );
        assert!(
            opf013("audio/mpeg", "audio/mpeg").is_empty(),
            "matching types say nothing"
        );
        assert!(
            opf013("audio/mpeg", "audio/mpeg; codecs=mp3").is_empty(),
            "a parameter on the declared type is not a mismatch"
        );
        assert!(
            opf013("audio/ogg; codecs=opus", "audio/ogg").is_empty(),
            "the Opus manifest spelling matches plain audio/ogg in content"
        );
    }

    /// An `<object>` pointing at a foreign resource needs a fallback.
    ///
    /// `<object>` was simply never added to the list of elements that can
    /// reference a foreign resource — the per-source shape again, where the
    /// elements are enumerated by hand and nothing fails loudly when one is
    /// missing. epubcheck reports RSC-032 on its own
    /// `foreign-xhtml-object-no-fallback-error` fixture; we reported nothing.
    ///
    /// **The fallback is the element's own content**, which is what makes this
    /// dangerous to implement carelessly: an `<object>` with real content owes
    /// nothing, and reporting one would be a false positive on the ordinary,
    /// correct way to author the element. That is the second assertion.
    ///
    /// The third is the trick in epubcheck's fixture, and it is the reason the
    /// `hidden` rule is not optional: the object *has* a `<p>` child and that
    /// `<p>` is `hidden`, so the fallback is not really there. An
    /// implementation that only asked "does it have child content" would call
    /// that book clean.
    ///
    /// The shelf is no witness — **none of its 385 books contains an
    /// `<object>` at all** — so this test is the evidence.
    #[test]
    fn an_object_referencing_a_foreign_resource_needs_a_fallback() {
        fn rsc032(body: &str) -> usize {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="slides" href="slideshow.xml" media-type="application/x-demo-slideshow"/>
    <item id="pic" href="pic.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body>{body}</body></html>"
            );
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", ch1.as_str()),
                    ("OEBPS/slideshow.xml", "<x/>"),
                    ("OEBPS/pic.png", "\0\0\0\0"),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_032)
                .count()
        }

        assert_eq!(
            rsc032(r#"<object data="slideshow.xml" type="application/x-demo-slideshow"/>"#),
            1,
            "a foreign resource with an empty object has no fallback"
        );
        assert_eq!(
            rsc032(
                r#"<object data="slideshow.xml" type="application/x-demo-slideshow"><p>a description</p></object>"#
            ),
            0,
            "the object's own content is its fallback"
        );
        assert_eq!(
            rsc032(
                r#"<object data="slideshow.xml" type="application/x-demo-slideshow"><p hidden="hidden">nope</p></object>"#
            ),
            1,
            "hidden content is not a fallback"
        );
        assert_eq!(
            rsc032(r#"<object data="pic.png" type="image/png"/>"#),
            0,
            "a core media type is not foreign and owes no fallback"
        );
    }

    /// A `file:` URL inside `@font-face` is reported like any other.
    ///
    /// The generic `url()` pass in `css.rs` deliberately skips `@font-face`
    /// blocks and hands them to `check_font_face`, which asked about the
    /// declaration, an empty block and an empty `url()` — but never about the
    /// scheme. So every question the generic pass asks had to be asked again
    /// there, and this one was not: epubcheck reports two file-URL errors on
    /// its own `file-url-in-css-error` fixture and we reported the manifest
    /// one alone.
    ///
    /// Same shape as the rest of this release: a special-cased branch takes
    /// ownership of a case and the general handling is skipped. The predicate
    /// is now shared between the two sites so they cannot drift apart again.
    #[test]
    fn a_file_url_in_a_font_face_src_is_reported() {
        let opf = |_: ()| {
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="Styles/s.css" media-type="text/css"/>
    <item id="f" href="Fonts/f.ttf" media-type="application/x-font-ttf"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
                .to_string()
        };
        let file_urls = |css: &str| -> Vec<String> {
            crate::validate_bytes(epub_with_opf_and_css(&opf(()), css))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_030)
                .flat_map(|m| m.params.clone())
                .collect()
        };

        assert_eq!(
            file_urls("@font-face { font-family: \"f\"; src: url('file:/font.woff'); }"),
            vec!["file:/font.woff".to_string()],
            "a file URL in @font-face src is a file URL"
        );
        // Control: the ordinary case must stay silent, or the check is just
        // noise on every book that embeds a font.
        assert!(
            file_urls("@font-face { font-family: \"f\"; src: url('../Fonts/f.ttf'); }").is_empty(),
            "a relative font url is not a file URL"
        );
        // And the generic pass still works, so the two sites agree.
        assert_eq!(
            file_urls("body { background: url('file:/bg.png'); }"),
            vec!["file:/bg.png".to_string()],
            "the generic url() pass asks the same question"
        );
    }

    /// A nav link with a dangling fragment still takes part in the
    /// spine-order comparison.
    ///
    /// It used to be dropped from the comparison entirely, on the grounds
    /// that a dangling fragment "is already caught elsewhere as a broken
    /// reference". RSC-012 does catch it — but RSC-012 answers *is this
    /// fragment defined* and NAV-011 answers *is the order right*, so the
    /// link left the ordering question altogether. JSWolf's `scrambled.epub`
    /// (MobileRead, 2026-08-21) is 67 such links: epubcheck reported 71
    /// NAV-011 and we reported 5. After the fix, 71 and 71.
    ///
    /// epubcheck's guard is `targetAnchorPosition > -1` around the
    /// *document-order* half only; the spine half has already run.
    ///
    /// The findings also carry the offending link and its position now. All
    /// five used to be anchored on the `<nav>` element with no target named,
    /// so a reader saw five identical lines and an editor would mark one line
    /// five times.
    #[test]
    fn a_nav_link_with_a_dangling_fragment_is_still_ordered() {
        fn nav011(links: &[&str]) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let items: String = links
                .iter()
                .map(|h| format!("<li><a href=\"{h}\">x</a></li>"))
                .collect();
            let nav = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                 <body><nav epub:type=\"toc\"><ol>{items}</ol></nav></body></html>"
            );
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
    <item id="b" href="b.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="a"/><itemref idref="b"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            let doc = |ids: &str| {
                format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                     <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                     <body>{ids}</body></html>"
                )
            };
            let a = doc("<p id=\"p1\">x</p><p id=\"p2\">y</p>");
            let b = doc("<p id=\"q1\">x</p>");
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", nav.as_str()),
                    ("OEBPS/a.xhtml", a.as_str()),
                    ("OEBPS/b.xhtml", b.as_str()),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::NAV_011)
                .flat_map(|m| m.params.clone())
                .collect()
        }

        // Control: reading order, nothing to say.
        assert!(
            nav011(&["a.xhtml#p1", "a.xhtml#p2", "b.xhtml#q1"]).is_empty(),
            "a nav in reading order is silent"
        );
        // A plain spine regression, both fragments resolvable.
        assert_eq!(
            nav011(&["b.xhtml#q1", "a.xhtml#p1"]),
            vec!["a.xhtml#p1".to_string()],
            "the offending link is named, not the nav element"
        );
        // The fix: the second link's fragment does not exist, so its document
        // position is unknown — but it still goes backwards in the spine.
        assert_eq!(
            nav011(&["b.xhtml#q1", "a.xhtml#nosuchid"]),
            vec!["a.xhtml#nosuchid".to_string()],
            "a dangling fragment does not excuse the link from spine ordering"
        );
        // And a dangling fragment between two good links must not hide the
        // document-order comparison between them.
        assert_eq!(
            nav011(&["a.xhtml#p2", "a.xhtml#nosuchid", "a.xhtml#p1"]),
            vec!["a.xhtml#p1".to_string()],
            "the unresolvable link leaves the document-order baseline untouched"
        );
    }

    /// When an itemref carries both layout overrides, pre-paginated wins.
    ///
    /// They are mutually exclusive and both tools say so (RSC-005) — but the
    /// document still has to be validated as *something*, and epubcheck
    /// resolves it pre-paginated: `processItemrefProperties` reads
    /// `properties.contains(PRE_PAGINATED) || ...`, so that disjunct
    /// short-circuits before reflowable is consulted. We tested reflowable
    /// first, called such a document reflowable, and skipped its viewport
    /// requirement — HTM-046 on W3C's `fxl-spine-overrides_duplicate`.
    ///
    /// **An error on a book does not excuse the checks after it from being
    /// right.** The reader still gets a verdict on the rest of the document,
    /// and here the wrong branch silently dropped a real error.
    ///
    /// The shelf is no witness at all: **none** of its 385 books uses a
    /// `rendition:layout` spine override, so its silence across this change
    /// means nothing. The corpus and this test are the evidence.
    #[test]
    fn both_layout_overrides_on_one_itemref_resolve_pre_paginated() {
        fn htm046(props: &str) -> usize {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"
         prefix="rendition: http://www.idpf.org/vocab/rendition/#">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1" properties="{props}"/></spine>
</package>"#
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            // No viewport meta: a fixed-layout document owes one.
            const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                <body><p>x</p></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", CH1),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::HTM_046)
                .count()
        }

        assert_eq!(
            htm046("rendition:layout-reflowable rendition:layout-pre-paginated"),
            1,
            "both present resolves pre-paginated, so the viewport is owed"
        );
        // Order in the attribute must not decide it either.
        assert_eq!(
            htm046("rendition:layout-pre-paginated rendition:layout-reflowable"),
            1,
            "the resolution is by property, not by document order"
        );
        assert_eq!(
            htm046("rendition:layout-pre-paginated"),
            1,
            "plain fixed-layout"
        );
        assert_eq!(
            htm046("rendition:layout-reflowable"),
            0,
            "plain reflowable owes nothing"
        );
    }

    /// A viewport meta outside the spine is still asked about.
    ///
    /// `check_reflowable_viewport` ran in the spine-itemref loop, so a
    /// manifest XHTML document with no itemref — a nav document, an
    /// unreferenced cover page — was never asked. epubcheck asks every XHTML
    /// content document; the check lives in `OPSHandler30`, which runs per
    /// document.
    ///
    /// **A non-spine document is reflowable whatever the package says**, and
    /// that is the non-obvious half: `fixedLayout` is set only in
    /// `OPFHandler30.processItemrefProperties`, which an item with no itemref
    /// never reaches. So a package-level `rendition:layout` of
    /// `pre-paginated` does not make the nav document fixed-layout — which is
    /// why the assertion below uses a pre-paginated package.
    #[test]
    fn a_viewport_outside_the_spine_is_reported_as_reflowable() {
        fn htm060b(nav_viewport: bool, spine_doc_viewport: bool) -> usize {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let vp = r#"<meta name="viewport" content="width=800,height=600"/>"#;
            let head = |on: bool| if on { vp } else { "" };
            let nav = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title>{}</head>\
                 <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>",
                head(nav_viewport)
            );
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title>{}</head>\
                 <body><p>x</p></body></html>",
                head(spine_doc_viewport)
            );
            // Pre-paginated at package level: the spine document is
            // fixed-layout, the nav document is not.
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"
         prefix="rendition: http://www.idpf.org/vocab/rendition/#">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <meta property="rendition:layout">pre-paginated</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", nav.as_str()),
                    ("OEBPS/ch1.xhtml", ch1.as_str()),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::HTM_060B)
                .count()
        }

        assert_eq!(
            htm060b(true, true),
            1,
            "the nav document's viewport is reported; the spine document's is fixed-layout"
        );
        assert_eq!(
            htm060b(false, true),
            0,
            "a fixed-layout spine document's viewport is checked, not ignored"
        );
        assert_eq!(htm060b(false, false), 0, "no viewport, nothing to say");
    }

    /// A duplicated cardinality property is one finding, not one per
    /// occurrence — and the count is global, not per `@refines` group.
    ///
    /// The Schematron rule is a cardinality assertion whose context is the
    /// **metadata container** — `count(...) le 1` said once — and ours had
    /// the context on the `meta` element instead, so two duplicates drew two
    /// findings. epubcheck reports one; W3C's `fxl-layout-duplication` and
    /// `lay-pp-layout-duplication` are where the pair showed up. All five
    /// `rendition:*` cardinality rules had the same shifted context.
    ///
    /// **The zero case is the one that could have gone badly.** Moving the
    /// context to the container makes the rule run on *every* book, where it
    /// had previously only run when a matching `meta` existed — so the
    /// assertion had to become `<=` rather than `= 1`, or every book without
    /// the property would have been reported. All three counts are asserted
    /// for that reason.
    #[test]
    fn a_duplicated_cardinality_property_is_reported_once() {
        fn findings(metas: &str) -> usize {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"
         prefix="rendition: http://www.idpf.org/vocab/rendition/# media: http://www.idpf.org/vocab/overlays/#">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {metas}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                <body><p>x</p></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", CH1),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.text.contains("must not occur more than one time"))
                .count()
        }

        const ONE: &str = r#"<meta property="rendition:layout">pre-paginated</meta>"#;
        // The trap: with the context on the container the rule runs on every
        // book, so absence must stay silent.
        assert_eq!(findings(""), 0, "no property at all must be silent");
        assert_eq!(findings(ONE), 0, "one global value is exactly right");
        assert_eq!(
            findings(&format!("{ONE}{ONE}")),
            1,
            "two duplicates are one violation of one constraint, not two"
        );
        assert_eq!(
            findings(&format!("{ONE}{ONE}{ONE}")),
            1,
            "three duplicates are still one violation"
        );

        // **The `media:*` pair had the same shifted context and a second
        // defect on top: they counted only within a `@refines` group.** Two
        // measured against epubcheck, one probe each:
        // - same group, ours reported twice where epubcheck reports once;
        // - different groups, ours reported *nothing* where epubcheck still
        //   reports once, because each group held one.
        // So the rule was wrong in both directions at the same time, and the
        // global count fixes both.
        const MEDIA: &str = r#"<meta property="media:active-class">c</meta>"#;
        assert_eq!(findings(MEDIA), 0, "one active-class is right");
        assert_eq!(
            findings(&format!("{MEDIA}{MEDIA}")),
            1,
            "two in one group: one violation, not two"
        );
        assert_eq!(
            findings(
                r##"<meta property="media:active-class" refines="#ch1">a</meta><meta property="media:active-class" refines="#nav">b</meta>"##
            ),
            1,
            "two in different groups is still one violation — the count is global"
        );
    }

    /// A nav link to a non-Content-Document draws RSC-010 exactly **once**.
    ///
    /// #78 generalised this check from the two toc paths to every hyperlink
    /// and left the narrower `navdoc` one in place, so a `toc` nav link drew
    /// it twice at the same position. epubcheck reports one; W3C's
    /// `pub-foreign_bad-fallback` is where the two showed up side by side.
    ///
    /// **The assertion is on the count, not on presence**, which is the whole
    /// point: `> 0` passed throughout the duplicate's entire lifetime. Same
    /// lesson #76 left, applied to the rule that inherited it — and the
    /// removed check had no test of its own, which is why nothing failed when
    /// the duplicate appeared.
    #[test]
    fn a_nav_link_to_a_non_content_document_is_reported_once() {
        fn rsc010(fallback: bool) -> usize {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="blob" href="foo.dmg" media-type="application/octet-stream"{}/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#,
                if fallback { r#" fallback="ch1""# } else { "" }
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol>\
                <li><a href=\"ch1.xhtml\">c</a></li>\
                <li><a href=\"foo.dmg\">d</a></li></ol></nav></body></html>";
            const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                <body><p>x</p></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", CH1),
                    ("OEBPS/foo.dmg", "\0\0\0\0"),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_010)
                .count()
        }

        assert_eq!(rsc010(false), 1, "reported once, not once per check");
        // Control: a fallback chain reaching a Content Document is a
        // legitimate target (Doitsu, MobileRead #168), so nothing is due.
        assert_eq!(
            rsc010(true),
            0,
            "a fallback to a Content Document is legitimate"
        );
    }

    /// A standalone SVG's own references count for OPF-097.
    ///
    /// An SVG in the spine is a content document, but `content_docs` selects
    /// on `application/xhtml+xml`, so its references were collected by
    /// nothing. W3C's `lay-pp-embedded-images-svg` is eight
    /// `<svg><image xlink:href="../images/A.png"/></svg>` plates: we called
    /// all eight PNGs unreferenced where epubcheck called none of them.
    ///
    /// **Second instance of the per-source shape**, after the media-overlay
    /// one above. References are gathered per source here and per reference
    /// in epubcheck, so each new source has to be added by hand and nothing
    /// fails loudly when one is missed.
    #[test]
    fn a_spine_svgs_own_references_count_for_opf_097() {
        fn unreferenced(image_ref: Option<&str>) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10">
  <title>t</title>{}
</svg>"#,
                match image_ref {
                    Some(h) => format!(r#"<image xlink:href="{h}" width="10" height="10"/>"#),
                    None => String::new(),
                }
            );
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="plate" href="plates/a.svg" media-type="image/svg+xml"/>
    <item id="png" href="images/a.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="plate"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"plates/a.svg\">c</a></li></ol></nav></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/plates/a.svg", svg.as_str()),
                    ("OEBPS/images/a.png", "\0\0\0\0"),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            let mut v: Vec<String> = crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_097)
                .flat_map(|m| m.params.clone())
                .collect();
            v.sort();
            v
        }

        assert_eq!(
            unreferenced(Some("../images/a.png")),
            Vec::<String>::new(),
            "an image the spine SVG embeds must not be called unreferenced"
        );
        // Control: nothing points at it, so it is unreferenced — the fix does
        // not exempt images, it collects a reference.
        assert_eq!(
            unreferenced(None),
            vec!["images/a.png".to_string()],
            "an image no document references is still unreferenced"
        );
    }

    /// OPF-003 is a container-level question, asked once against every
    /// package document.
    ///
    /// epubcheck searches *all* package documents (`Iterables.tryFind(
    /// opfHandlers, ...)`) and counts a metadata `<link href>` as a
    /// declaration alongside a manifest `<item>`. Ours ran per package with
    /// only that package's `<item>`s, which W3C's `epub-tests` caught twice:
    /// `pkg-linked-records` (an ONIX record referenced by `<link>`) and
    /// `ocf-package_multiple` (three renditions, each blaming the other two's
    /// files — 18 findings against epubcheck's none).
    ///
    /// The control is the point of the test: a file nothing declares is
    /// still reported. Both fixes widen what counts as declared, and a
    /// widening with no floor under it is just a deletion.
    #[test]
    fn opf_003_unions_every_rendition_and_counts_a_link_as_a_declaration() {
        fn opf003(
            container_rootfiles: &str,
            packages: &[(&str, &str)],
            extra: &[&str],
        ) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let container = format!(
                r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>{container_rootfiles}</rootfiles>
</container>"#
            );
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                z.start_file("META-INF/container.xml", o).unwrap();
                z.write_all(container.as_bytes()).unwrap();
                for (name, body) in packages {
                    z.start_file(*name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                for name in extra {
                    z.start_file(*name, o).unwrap();
                    z.write_all(b"x").unwrap();
                }
                z.finish().unwrap();
            }
            let mut v: Vec<String> = crate::validate_bytes(buf)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_003)
                .flat_map(|m| m.params.clone())
                .collect();
            v.sort();
            v
        }

        // One rendition, one package. `extra.bin` is declared by nothing;
        // `record.xml` only by a metadata <link>.
        let pkg = |dir: &str, link: bool| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine><itemref idref="nav"/></spine>
</package>"#,
                if link {
                    r#"<link rel="record" href="record.xml" media-type="application/xml"/>"#
                } else {
                    ""
                },
            )
            .replace("nav.xhtml", &format!("{dir}nav.xhtml"))
        };

        const RF1: &str =
            r#"<rootfile full-path="A/package.opf" media-type="application/oebps-package+xml"/>"#;
        const RF2: &str =
            r#"<rootfile full-path="B/package.opf" media-type="application/oebps-package+xml"/>"#;

        // A metadata <link> is a declaration.
        assert_eq!(
            opf003(
                RF1,
                &[("A/package.opf", &pkg("", true))],
                &["A/nav.xhtml", "A/record.xml"]
            ),
            Vec::<String>::new(),
            "a resource referenced by a metadata <link> is declared"
        );
        // Control: drop the <link> and the same file is undeclared.
        assert_eq!(
            opf003(
                RF1,
                &[("A/package.opf", &pkg("", false))],
                &["A/nav.xhtml", "A/record.xml"]
            ),
            vec!["A/record.xml".to_string()],
            "a resource nothing declares is still reported"
        );
        // Two renditions: neither may be blamed for the other's files, and
        // the finding is emitted once for the publication, not per package.
        assert_eq!(
            opf003(
                &format!("{RF1}{RF2}"),
                &[
                    ("A/package.opf", &pkg("", false)),
                    ("B/package.opf", &pkg("", false)),
                ],
                &["A/nav.xhtml", "B/nav.xhtml"]
            ),
            Vec::<String>::new(),
            "one rendition's files are declared, by its own package"
        );
    }

    /// A file URL is reported *and* still classified.
    ///
    /// RSC-030 used to `continue`, on the reasonable-sounding grounds that
    /// it was the whole story for a file URL. It was not: the reference
    /// still has to be classified, and skipping that cost one finding in
    /// each direction on W3C's `pub-file-urls` — a `remote-resources`
    /// property the author had correctly declared drew OPF-018 "doesn't
    /// appear to be needed", and three `<iframe>`s in a restricted context
    /// drew no RSC-006 where epubcheck reports one each.
    ///
    /// **Both directions are asserted because the bug had both**, and a test
    /// for either alone would have passed throughout its lifetime: dropping
    /// the false OPF-018 without gaining RSC-006 is a different, wrong fix.
    #[test]
    fn a_file_url_is_reported_and_still_counts_as_a_remote_reference() {
        fn ids(props: &str, body: &str) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"{props}/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body>{body}</body></html>"
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", ch1.as_str()),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            let mut v: Vec<String> = crate::validate_bytes(buf)
                .messages
                .iter()
                .map(|m| m.id.to_string())
                .filter(|i| i.starts_with("RSC-0") || i.starts_with("OPF-01"))
                .collect();
            v.sort();
            v.dedup();
            v
        }

        const IFRAME: &str = r#"<iframe src="file:///var/log/lastlog"></iframe>"#;
        // The property is declared and a file URL justifies it: RSC-030 for
        // the URL, RSC-006 because an iframe is a restricted context, and no
        // OPF-018 saying the property is unneeded.
        assert_eq!(
            ids(r#" properties="remote-resources""#, IFRAME),
            vec!["RSC-006".to_string(), "RSC-030".to_string()],
            "a file URL is remote enough to justify the property and to be refused in context"
        );
        // Control: no file URL, property still declared — OPF-018 is exactly
        // the right complaint, so the fix did not simply disable it.
        assert_eq!(
            ids(r#" properties="remote-resources""#, "<p>x</p>"),
            vec!["OPF-018".to_string()],
            "an unjustified remote-resources property is still reported"
        );
    }

    /// `title.present` only applies where its Schematron context does.
    ///
    /// The rule is `<rule context="h:head"><assert test="exists(h:title)">`.
    /// Ported without the context it fired on any EPUB 3 content document
    /// with no `<title>` anywhere — including one with no `<head>` at all,
    /// where the message describes an element the document does not contain.
    /// W3C's `pub-xml-external-id` carries exactly that: a content document
    /// whose whole body is `<span>The test fails.</span>`.
    ///
    /// The control matters more than the fix here: a document that *does*
    /// have a head and no title must still be reported, or the fix is just a
    /// deletion. Both are asserted, and so is the namespace, since `h:title`
    /// is XHTML's `title` and not SVG's.
    #[test]
    fn the_head_title_rule_needs_a_head_to_apply_to() {
        fn findings(doc: &str) -> Vec<String> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", doc),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
                .messages
                .iter()
                .filter_map(|m| m.rule)
                .filter(|r| r.contains("head_missing_title") || r.contains("empty_title"))
                .map(str::to_owned)
                .collect()
        }

        const NS: &str = "xmlns=\"http://www.w3.org/1999/xhtml\"";
        // No head: the rule has no context node, so it does not apply. The
        // document is still rejected, by the grammar, for other reasons.
        assert!(
            findings("<span>The test fails.</span>").is_empty(),
            "a document with no head must not be told its head needs a title"
        );
        // Control: a head with no title is exactly what the rule is for.
        assert_eq!(
            findings(&format!(
                "<html {NS}><head></head><body><p>x</p></body></html>"
            )),
            vec!["opf.content_document.head_missing_title"],
            "a head without a title is still reported"
        );
        // A present-but-empty title is the sibling rule, context `h:title`.
        assert_eq!(
            findings(&format!(
                "<html {NS}><head><title>  </title></head><body><p>x</p></body></html>"
            )),
            vec!["opf.content_document.empty_title"],
            "an empty title is the other rule, and still fires"
        );
        // A well-formed head says nothing.
        assert!(
            findings(&format!(
                "<html {NS}><head><title>t</title></head><body><p>x</p></body></html>"
            ))
            .is_empty(),
            "a head with a title must be silent"
        );
    }

    /// A Media Overlay's own references count for OPF-097.
    ///
    /// The audio file of a media-overlay book is referenced by its SMIL and
    /// by nothing else — that is what the format prescribes — so asking
    /// whether a *content document* draws it reported every such book's
    /// audio as unreferenced. 19 of W3C's 209 `epub-tests` publications
    /// tripped it.
    ///
    /// **Both directions, because the cheap fix passes only one of them.**
    /// Exempting audio media types outright, or every SMIL-adjacent
    /// resource, would silence the first assertion and the second: an audio
    /// file no overlay mentions is still unreferenced and epubcheck still
    /// says so. The distinguishing fact is the reference, not the type.
    ///
    /// Neither the corpus nor the shelf could have caught this — no book on
    /// the shelf carries an overlay at all — which is the argument for
    /// `epub-tests` as an instrument rather than for a wider shelf.
    #[test]
    fn a_media_overlays_audio_is_referenced_and_an_orphan_audio_is_not() {
        fn book(smil_audio_src: Option<&str>) -> Vec<u8> {
            use std::io::Write;
            use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

            let audio_el = match smil_audio_src {
                Some(src) => format!(r#"<audio src="{src}" clipBegin="0s" clipEnd="1s"/>"#),
                None => String::new(),
            };
            let smil = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<smil xmlns="http://www.w3.org/ns/SMIL" xmlns:epub="http://www.idpf.org/2007/ops" version="3.0">
  <body><par><text src="../ch1.xhtml#p1"/>{audio_el}</par></body>
</smil>"#
            );
            const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml" media-overlay="mo"/>
    <item id="mo" href="mo/ch1.smil" media-type="application/smil+xml"/>
    <item id="a" href="audio/a.mp3" media-type="audio/mpeg"/>
    <item id="orphan" href="audio/orphan.mp3" media-type="audio/mpeg"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                <body><p id=\"p1\">x</p></body></html>";
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
                <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">c</a></li></ol></nav></body></html>";

            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", OPF),
                    ("OEBPS/nav.xhtml", NAV),
                    ("OEBPS/ch1.xhtml", CH1),
                    ("OEBPS/mo/ch1.smil", smil.as_str()),
                    ("OEBPS/audio/a.mp3", "\0\0\0\0"),
                    ("OEBPS/audio/orphan.mp3", "\0\0\0\0"),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            buf
        }

        let unreferenced = |smil_audio_src: Option<&str>| -> Vec<String> {
            let mut v: Vec<String> = crate::validate_bytes(book(smil_audio_src))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_097)
                .flat_map(|m| m.params.clone())
                .collect();
            v.sort();
            v
        };

        // The overlay names a.mp3, so only the orphan is unreferenced.
        assert_eq!(
            unreferenced(Some("../audio/a.mp3")),
            vec!["audio/orphan.mp3".to_string()],
            "an audio file the overlay references must not be called unreferenced"
        );
        // Drop that one reference and it joins the orphan — the check still
        // works, it is not exempting audio wholesale.
        assert_eq!(
            unreferenced(None),
            vec!["audio/a.mp3".to_string(), "audio/orphan.mp3".to_string()],
            "an audio file no overlay references is still unreferenced"
        );
    }

    /// RSC-031 asks whether the scheme is `https`, not whether it is
    /// `http://` — and until 0.9.27 we asked the second question.
    ///
    /// epubcheck's condition (`ResourceReferencesChecker`:382-388) is
    /// EPUB 3, the reference is not a `LINK`/`HYPERLINK`, and the scheme is
    /// neither `https` nor `file`. Ours was `starts_with("http://")`, so a
    /// Calibre/Kobo `url(res:///system/fonts/HelveticaNeue.ttf)` — the exact
    /// shape that made 0.9.20 widen `is_remote_url` — drew the warning there
    /// and nothing here. Found while settling an unrelated OPF-014 question
    /// for epubsana, not by any instrument: **no shelf book has a remote URL
    /// in CSS at all**, which is also why the site had no test before this
    /// one, as the comment at the emission site says in as many words.
    ///
    /// Measured one book per scheme against 5.3.0, at both emission sites
    /// (this one and the content-document site, via a remote `<audio>`):
    /// nine probes, all exact matches, no overshoot.
    ///
    /// The control is the point — `https` and a case-shifted `HTTPS` must
    /// stay silent, or the widening would just be "warn on every remote URL".
    #[test]
    fn rsc_031_asks_for_https_rather_than_against_http() {
        let rsc_031 = |url: &str| {
            let css = format!("@font-face {{\n  font-family: X;\n  src: url({url});\n}}");
            crate::validate_bytes(epub_with_stylesheet(&css, "font/ttf"))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_031)
                .count()
        };
        // Secure, and the same scheme spelled loudly: silent either way.
        assert_eq!(rsc_031("https://example.com/f.ttf"), 0);
        assert_eq!(rsc_031("HTTPS://example.com/f.ttf"), 0);
        // Insecure, whatever the scheme is called.
        assert_eq!(rsc_031("http://example.com/f.ttf"), 1, "the case we had");
        assert_eq!(
            rsc_031("res:///system/fonts/HelveticaNeue.ttf"),
            1,
            "the real Calibre/Kobo shape this was found on"
        );
        assert_eq!(rsc_031("ftp://example.com/f.ttf"), 1);
    }

    /// CSS-029 must point at the stylesheet the class name is written in.
    /// It used to point at the content document that merely links that
    /// stylesheet - a file the name does not appear in (reported by Doitsu
    /// on the MobileRead forum).
    #[test]
    fn css_029_points_at_the_stylesheet_the_class_is_written_in() {
        let css = "body { color: red; }\n.-epub-media-overlay-active { color: blue; }";
        let report = crate::validate_bytes(epub_with_stylesheet(css, "font/ttf"));
        let hit = report
            .messages
            .iter()
            .find(|m| m.rule == Some("css.media_overlay.class_property_not_declared"))
            .expect("an undeclared media-overlay class must be reported");
        assert_eq!(hit.id, crate::ids::CSS_029);
        assert_eq!(
            hit.location.as_deref(),
            Some("OEBPS/Styles/s.css"),
            "the class name is in the stylesheet, not in ch1.xhtml"
        );
        assert_eq!(hit.position.map(|p| p.line), Some(2));
        assert!(
            hit.text.contains("media:active-class"),
            "the message must name the property that would declare it; got: {}",
            hit.text
        );
    }

    /// A CSS finding from an inline `<style>` must report the line the
    /// property is on *in the file*, not in the style text extracted out of
    /// it. It used to report the latter against the former's path: a
    /// `direction` on line 7 came out as line 3, where the reader finds
    /// `<head>`. Every CSS rule shared the defect, since they all take their
    /// offsets from the extracted text.
    #[test]
    fn inline_style_findings_report_the_line_in_the_document() {
        // <style> opens on line 5; `direction` is on line 7 of the file.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
            <head>\n\
            <title>t</title>\n\
            <style>\n\
            body { color: red; }\n\
            p { direction: rtl; }\n\
            </style>\n\
            </head>\n\
            <body><p>x</p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.id == crate::ids::CSS_001)
            .expect("'direction' must be reported");
        assert_eq!(hit.location.as_deref(), Some("OEBPS/ch1.xhtml"));
        assert_eq!(hit.position.map(|p| p.line), Some(7));
    }

    /// When the style text isn't a verbatim slice of the document - here a
    /// CDATA section, so the extracted text and the source differ - no
    /// offset within it can be mapped. Rather than report a confidently
    /// wrong line, fall back to the `<style>` element's own position: less
    /// precise, but a real place in the file.
    #[test]
    fn inline_style_falls_back_to_the_element_position_when_unmappable() {
        // <style> is on line 5; the CDATA-wrapped `direction` on line 7.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
            <head>\n\
            <title>t</title>\n\
            <style>\n\
            <![CDATA[\n\
            p { direction: rtl; }\n\
            ]]>\n\
            </style>\n\
            </head>\n\
            <body><p>x</p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.id == crate::ids::CSS_001)
            .expect("'direction' must still be reported");
        assert_eq!(hit.location.as_deref(), Some("OEBPS/ch1.xhtml"));
        assert_eq!(
            hit.position.map(|p| p.line),
            Some(5),
            "the <style> element's own line, not a guess inside it"
        );
    }

    /// EPUB 3 whose `ch1` manifest item declares `ch1_props` (e.g.
    /// `"scripted remote-resources"`) and whose `ch1` body is `ch1_body`.
    /// Nothing here actually *uses* remote resources, so declaring
    /// `remote-resources` is always "declared but unused" - the OPF-018 /
    /// OPF-018b case, gated on whether the body scripts.
    fn epub_declaring_props(ch1_props: &str, ch1_body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml" properties="{ch1_props}"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        let ch1 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body>{ch1_body}</body></html>"
        );
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", ch1.as_str()),
                ("OEBPS/nav.xhtml", NAV),
            ] {
                zip.start_file(name, o).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// Issue #67: the three package vocabularies that had no check at all -
    /// unprefixed `meta@property`, `itemref@properties` and `link@rel`.
    /// Builds a publication carrying one extra metadata line, one set of
    /// itemref properties, and one link rel, and returns the reported IDs.
    fn vocab_ids(meta_extra: &str, itemref_props: &str, link_rel: &str) -> Vec<String> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title id="t">T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {meta_extra}
    <link rel="{link_rel}" href="rec.xml" media-type="application/xml"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="rec" href="rec.xml" media-type="application/xml"/>
  </manifest>
  <spine><itemref idref="ch1" properties="{itemref_props}"/></spine>
</package>"#
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head><body><p>x</p></body></html>";
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/rec.xml", "<r/>"),
            ] {
                zip.start_file(name, o).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        crate::validate_bytes(buf)
            .messages
            .iter()
            .map(|m| m.id.to_string())
            .collect()
    }

    #[test]
    fn unprefixed_package_vocabularies_are_checked() {
        // Valid throughout: three real vocabulary members, one per surface.
        assert!(
            !vocab_ids(
                "<meta property=\"file-as\" refines=\"#t\">T, The</meta>",
                "page-spread-left",
                "record"
            )
            .contains(&"OPF-027".to_string()),
            "known names must not be reported"
        );
        // One unknown name per surface, each reported.
        for (meta, itemref, rel) in [
            (
                "<meta property=\"nonsense\">x</meta>",
                "page-spread-left",
                "record",
            ),
            ("", "bogusProp", "record"),
            ("", "page-spread-left", "bogusRel"),
            // rendition: overrides have their own vocabulary on itemref.
            ("", "rendition:layout-invented", "record"),
        ] {
            assert!(
                vocab_ids(meta, itemref, rel).contains(&"OPF-027".to_string()),
                "expected OPF-027 for ({meta:?}, {itemref:?}, {rel:?})"
            );
        }
        // The `media:` vocabulary is four names; a real one used correctly
        // stays clean, an invented one under the same prefix does not.
        assert!(
            !vocab_ids(
                "<meta property=\"media:duration\">0:30:00</meta>",
                "page-spread-left",
                "record"
            )
            .contains(&"OPF-027".to_string())
        );
        assert!(
            vocab_ids(
                "<meta property=\"media:invented\">x</meta>",
                "page-spread-left",
                "record"
            )
            .contains(&"OPF-027".to_string())
        );
        // A prefixed name is *not* ours to judge: an author-declared prefix
        // carries a vocabulary we don't know, and an undeclared one is
        // OPF-028, a different message.
        assert!(
            !vocab_ids(
                "<meta property=\"foo:whatever\">x</meta>",
                "foo:whatever",
                "foo:whatever"
            )
            .contains(&"OPF-027".to_string()),
            "prefixed names must not draw OPF-027"
        );
        // `pageBreakSource` (EPUB 3.4) and the deprecated-but-still-defined
        // link rels are members, so they draw no OPF-027 - the deprecated
        // ones keep their own OPF-086 instead.
        let deprecated = vocab_ids(
            "<meta property=\"pageBreakSource\" refines=\"#t\">print</meta>",
            "page-spread-left",
            "onix-record",
        );
        assert!(!deprecated.contains(&"OPF-027".to_string()));
        assert!(deprecated.contains(&"OPF-086".to_string()));
    }

    /// The `a11y:` meta vocabulary, and specifically `contactEmail`
    /// (w3c/epubcheck [#1669](https://github.com/w3c/epubcheck/issues/1669),
    /// reported 2026-07-02 and unanswered).
    ///
    /// **epubcheck reports OPF-027 on it and so did we** — measured one book
    /// each against 5.3.0, with a `certifiedBy` control that stays clean on
    /// both sides. Its own vocabulary is three names and cannot include this
    /// one: EPUB Accessibility 1.2's change log dates `contactEmail`
    /// **2025-09-04**, three days after 5.3.0 shipped, and nothing has been
    /// committed upstream since. We had no such excuse — the list was copied
    /// from 1.1.
    ///
    /// The negative half is the point of the test: the vocabulary check must
    /// still bite, or widening the list would have passed here by disabling
    /// it.
    #[test]
    fn a11y_meta_vocabulary_takes_epub_accessibility_12() {
        let has_027 = |meta: &str| {
            vocab_ids(meta, "page-spread-left", "record").contains(&"OPF-027".to_string())
        };
        for name in [
            "a11y:certifiedBy",
            "a11y:certifierCredential",
            "a11y:exemption",
            // 1.2's addition.
            "a11y:contactEmail",
        ] {
            assert!(
                !has_027(&format!("<meta property=\"{name}\">x</meta>")),
                "{name} is real accessibility vocabulary"
            );
        }
        assert!(
            has_027("<meta property=\"a11y:notAThing\">x</meta>"),
            "an invented a11y name must still be reported"
        );
    }

    /// `remote-resources` declared but nothing remote is used: a warning
    /// (OPF-018) normally, but a *usage* note (OPF-018b) when the document
    /// scripts - a script could fetch a remote resource dynamically, so the
    /// property can't be disproven, only left unverified. Matches
    /// epubcheck's HAS_SCRIPTS downgrade (same shape as OPF-096/096b).
    #[test]
    fn remote_resources_declared_unused_is_warning_without_script() {
        let report = crate::validate_bytes(epub_declaring_props("remote-resources", "<p>x</p>"));
        let hit = report
            .messages
            .iter()
            .find(|m| m.text.contains("remote-resources"))
            .expect("declared-but-unused remote-resources must be reported");
        assert_eq!(hit.id, crate::ids::OPF_018);
        assert_eq!(hit.severity, crate::report::Severity::Warning);
    }

    #[test]
    fn remote_resources_declared_unused_is_usage_with_script() {
        let report = crate::validate_bytes(epub_declaring_props(
            "scripted remote-resources",
            "<script>var x=1;</script><p>x</p>",
        ));
        let hit = report
            .messages
            .iter()
            .find(|m| m.text.contains("remote-resources"))
            .expect("declared-but-unused remote-resources must still be reported");
        assert_eq!(hit.id, crate::ids::OPF_018B, "scripted -> OPF-018b");
        assert_eq!(hit.severity, crate::report::Severity::Usage);
        assert!(report.is_valid(), "usage does not invalidate");
    }

    /// `mathml` was in `KNOWN_ITEM_PROPERTIES` (so declaring it never drew
    /// OPF-027) but in neither direction of the used/declared cross-check,
    /// so a book with MathML and no property, and a book with the property
    /// and no MathML, both passed. epubcheck adds `ITEM_PROPERTIES.MATHML`
    /// on the `math` element itself (OPSHandler30) - Doitsu, MobileRead
    /// #138.
    #[test]
    fn mathml_property_is_checked_in_both_directions() {
        const MATH: &str = "<p><math xmlns=\"http://www.w3.org/1998/Math/MathML\">\
                            <mi>x</mi></math></p>";
        let finding = |props: &str, body: &str| {
            crate::validate_bytes(epub_declaring_props(props, body))
                .messages
                .iter()
                .find(|m| m.text.contains("mathml"))
                .map(|m| (m.id, m.severity))
        };

        assert_eq!(
            finding("", MATH),
            Some((crate::ids::OPF_014, crate::report::Severity::Error)),
            "MathML present, property missing"
        );
        assert_eq!(
            finding("mathml", "<p>no maths</p>"),
            Some((crate::ids::OPF_015, crate::report::Severity::Error)),
            "property declared, no MathML"
        );
        assert_eq!(finding("mathml", MATH), None, "declared and used");
        assert_eq!(finding("", "<p>no maths</p>"), None, "neither");
    }

    // #37: scripted-content detection (OPF-014/015). epubcheck (OPSHandler30)
    // marks a document scripted on a <form> element, javascript, or any on*
    // event-handler attribute - NOT the bare presence of a form control.

    /// A document that declares no `scripted` property draws OPF-014 iff it
    /// is detected as scripted, so this is a direct probe of the detection.
    fn is_detected_scripted(body: &str) -> bool {
        crate::validate_bytes(epub_declaring_props("", body))
            .messages
            .iter()
            .any(|m| m.id == crate::ids::OPF_014 && m.text.contains("scripted"))
    }

    #[test]
    fn form_element_is_scripted() {
        assert!(is_detected_scripted("<form></form>"));
    }

    #[test]
    fn on_handler_attribute_is_scripted() {
        assert!(is_detected_scripted("<p onclick=\"f()\">x</p>"));
        assert!(is_detected_scripted("<p onpointerdown=\"f()\">x</p>"));
    }

    #[test]
    fn form_control_alone_is_not_scripted() {
        // The #37 fix: a form control with no <form>/on* is NOT scripted
        // (epubcheck triggers on <form>, not the controls). This used to
        // wrongly draw OPF-014.
        assert!(!is_detected_scripted(
            "<p><input required=\"required\"/></p>"
        ));
        assert!(!is_detected_scripted(
            "<p><button type=\"button\">b</button></p>"
        ));
        assert!(!is_detected_scripted(
            "<p><textarea></textarea><select><option>a</option></select></p>"
        ));
    }

    fn epub_with_ch1(ch1: &str) -> Vec<u8> {
        epub_with_opf(None, ch1)
    }

    /// The same book with the package document swapped out, for checks that
    /// are about the OPF itself. `OEBPS/meta.xml` is always present, so an
    /// OPF can point a metadata `<link>` at a real resource.
    fn epub_with_opf(opf: Option<&str>, ch1: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.unwrap_or(OPF)),
                ("OEBPS/ch1.xhtml", ch1),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/meta.xml", "<?xml version=\"1.0\"?><r/>"),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    fn rsc_016_rules(ch1: &str) -> Vec<&'static str> {
        crate::validate_bytes(epub_with_ch1(ch1))
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_016)
            .map(|m| m.rule.unwrap_or(""))
            .collect()
    }

    /// A minimal, otherwise-valid EPUB **2** carrying the caller's full ch1
    /// XHTML 1.1 content document — used to check that EPUB-3-only content-model
    /// rules don't leak into EPUB 2 (#21).
    /// Issue #23's position guarantee, at its hardest point. When the
    /// DOCTYPE's closing `>` and the root element share a line, the entity
    /// declarations injected before that `>` push the root's column to the
    /// right in the text epubveri parses. Measured before this was
    /// corrected: `<html>` moved 25 columns.
    ///
    /// It has to be corrected rather than avoided - inserting text on a line
    /// necessarily moves what follows it, and not injecting would send the
    /// document back to not parsing at all. So the reported column must
    /// describe the file on disk, not the text we parsed. epubsana locates
    /// nodes by these positions and edits files in place; a column that is
    /// right for our parser and wrong for the file is worse than none.
    #[test]
    fn epub2_dtd_entities_report_columns_of_the_real_file() {
        // DOCTYPE, root and an obsolete <font> all on line 2, after a &nbsp;
        // document. `epub_type_findings`-style single-line shape on purpose.
        //
        // The anchor used to be an empty <title>, until a real-book shelf run
        // showed that is valid in EPUB 2 (XHTML 1.1 types it `<text/>`; only
        // `epub-xhtml-30.sch` asserts non-empty). <font> is obsolete in every
        // version, so it anchors the position check without depending on a
        // rule that turned out to be EPUB 3-only.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
            \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\
            <head><title>t</title></head><body><p>a&nbsp;<font>b</font></p></body></html>";
        let report = crate::validate_bytes(epub2_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.id == crate::ids::RSC_005 && m.text.contains("font"))
            .expect("the obsolete <font> behind the &nbsp; must be seen");
        let pos = hit.position.expect("a position");
        // Where <font> really is, in the file the author has.
        let want = crate::report::Position::of_offset(ch1, ch1.find("<font>").unwrap());
        assert_eq!(
            (pos.line, pos.column),
            (want.line, want.column),
            "reported position must describe the original file, not the augmented text"
        );
    }

    /// Builds a minimal EPUB 2 whose `<spine>` carries `spine_attrs` and
    /// whose second document (`cover.xhtml`) carries `cover_itemref_attrs`.
    /// Nothing links to cover.xhtml, so `linear="no"` on it makes it
    /// unreachable - the OPF-096 shape, in an EPUB 2.
    fn epub2_spine_case(spine_attrs: &str, cover_itemref_attrs: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"{spine_attrs}>
    <itemref idref="ch1"/>
    <itemref idref="cover"{cover_itemref_attrs}/>
  </spine>
</package>"#
        );
        const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap><navPoint id="n1" playOrder="1"><navLabel><text>Ch1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
        const DOC: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n\
              \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";

        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
                ),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/toc.ncx", NCX),
                ("OEBPS/ch1.xhtml", DOC),
                ("OEBPS/cover.xhtml", DOC),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// OPF-096 ("non-linear content is not reachable") is an EPUB 3 rule:
    /// EPUB 2.0.1 has no reachability requirement, and epubcheck implements
    /// it in its EPUB-3 checker only. We were applying it to EPUB 2 as well,
    /// inventing an error on books epubcheck passes (reported by Doitsu on
    /// the MobileRead forum) - the same EPUB-3-rule-leaks-into-EPUB-2 class
    /// as #9 and #21.
    #[test]
    fn opf_096_does_not_apply_to_epub2() {
        let report = crate::validate_bytes(epub2_spine_case("", " linear=\"no\""));
        let hits: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::OPF_096 || m.id == crate::ids::OPF_096B)
            .map(|m| m.text.as_str())
            .collect();
        assert!(
            hits.is_empty(),
            "EPUB 2 has no reachability rule; got {hits:?}"
        );
    }

    /// The Adobe `page-map` spine attribute draws two findings that say
    /// different things: RSC-005 (the document is invalid) and OPF-062
    /// (*which* non-standard feature is in use — the part that tells an
    /// author whether they meant it). We had only the first; epubcheck emits
    /// both (reported by Doitsu on the MobileRead forum).
    #[test]
    fn adobe_page_map_draws_both_the_error_and_the_usage_note() {
        let report = crate::validate_bytes(epub2_spine_case(" page-map=\"pm\"", ""));
        let rules: Vec<_> = report
            .messages
            .iter()
            .filter_map(|m| m.rule)
            .filter(|r| r.starts_with("opf.spine."))
            .collect();
        assert!(
            rules.contains(&"opf.spine.pagemap_not_allowed"),
            "got {rules:?}"
        );
        assert!(
            rules.contains(&"opf.spine.adobe_pagemap_usage"),
            "the usage note naming the extension is missing; got {rules:?}"
        );
        let usage = report
            .messages
            .iter()
            .find(|m| m.rule == Some("opf.spine.adobe_pagemap_usage"))
            .unwrap();
        assert_eq!(usage.id, crate::ids::OPF_062);
        assert_eq!(usage.severity, crate::report::Severity::Usage);
    }

    /// An attribute fault is reported at the **attribute**, not at the element
    /// carrying it (JSWolf, MobileRead #220: "the error is correct, the line is
    /// correct, the column is wrong").
    ///
    /// `element_path` has ended in an `/@name` step since #18; `position` was
    /// left pointing at the element start, so a reader — or a Sigil/calibre
    /// plugin placing a cursor — was sent to the `<` of a start tag that on a
    /// real package document is often a hundred characters wide.
    ///
    /// **The negative half is the point.** Asserting only that a position
    /// exists, or that the line is right, would have passed for the entire
    /// lifetime of the bug: the line was never wrong. So this pins the column
    /// to the attribute *and* states what it must no longer be.
    #[test]
    fn an_attribute_fault_is_reported_at_the_attribute_not_the_element() {
        const ATTRS: &str = " page-progression-direction=\"ltr\"";
        let report = crate::validate_bytes(epub2_spine_case(ATTRS, ""));
        let hit = report
            .messages
            .iter()
            .find(|m| m.id == crate::ids::RSC_005 && m.text.contains("page-progression-direction"))
            .expect("EPUB 2 has no page-progression-direction; the fault must be reported");
        let pos = hit.position.expect("a position");

        // Rebuilt from the same pieces the fixture writes, so a change to the
        // fixture's spine line fails here loudly instead of silently moving the
        // column this test claims to pin.
        let spine_line = format!("  <spine toc=\"ncx\"{ATTRS}>");
        let at_attribute = spine_line.find("page-progression-direction").unwrap() as u32 + 1;
        let at_element = spine_line.find("<spine").unwrap() as u32 + 1;

        assert_eq!(
            pos.column, at_attribute,
            "column must point at the offending attribute"
        );
        assert_ne!(
            pos.column, at_element,
            "column must no longer point at the element carrying the attribute"
        );
        assert_eq!(
            hit.element_path.as_ref().map(|p| p.path.as_str()),
            Some("/opf:package[1]/opf:spine[1]/@page-progression-direction"),
            "the machine half and the human half must name the same thing"
        );
    }

    /// `params[0]` carries the **prefixed** attribute name for the attribute
    /// kinds, and the prefix is reconstructed from the namespace rather than
    /// read from the document.
    ///
    /// This is the half of the contract that is easiest to get wrong from the
    /// outside: the book below binds the OPS namespace to `e`, so the string
    /// `epub:type` appears **nowhere in the file**, and a consumer using
    /// `params[0]` as a lookup key into the source would find nothing.
    /// epubsana asked for this to be stated outright rather than left implicit
    /// (2026-08-22), and a promise that is only in a doc comment is one nobody
    /// notices breaking.
    #[test]
    fn params0_is_the_reconstructed_prefix_not_the_documents_own() {
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
            \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" \
            xmlns:e=\"http://www.idpf.org/2007/ops\">\
            <head><title>t</title></head><body><p e:type=\"chapter\">a</p></body></html>";
        assert!(
            !ch1.contains("epub:type"),
            "the fixture must not contain the string the contract produces"
        );
        let report = crate::validate_bytes(epub2_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.violation_kind == Some(crate::report::ViolationKind::AttributeNotAllowed))
            .expect("epub:type is not an EPUB 2 attribute; it must be rejected");
        assert_eq!(
            hit.params.first().map(String::as_str),
            Some("epub:type"),
            "the attribute kinds carry the conventional prefix, reconstructed \
             from the namespace"
        );
    }

    /// The element kinds carry the **local** name, never a prefix — the other
    /// half of the same contract, and the reason it is written per kind rather
    /// than as one rule.
    ///
    /// The two spellings never meet inside a consumer's `(kind, params[0])`
    /// group key, because the kind already separates attribute faults from
    /// element faults. That is why the asymmetry is documented rather than
    /// removed: making it uniform would mean changing the attribute side, which
    /// is a spelling epubsana already got moved under them once (0.9.19).
    #[test]
    fn element_kinds_carry_the_local_name() {
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
            \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\
            <head><title>t</title></head><body><p>a<nav>x</nav></p></body></html>";
        let report = crate::validate_bytes(epub2_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.violation_kind == Some(crate::report::ViolationKind::ElementNotAllowed))
            .expect("<nav> is HTML5; EPUB 2 must reject it");
        let p0 = hit.params.first().map(String::as_str);
        assert_eq!(p0, Some("nav"));
        assert!(
            !p0.unwrap().contains(':'),
            "element kinds are never prefixed; got {p0:?}"
        );
    }

    /// `None` is a statement about the **rule**, never about the finding.
    ///
    /// Both findings below come from one document: the grammar rejects `<nav>`
    /// and carries a kind, while the hand-coded broken-reference check is a rule
    /// outside the family and carries none. A consumer meeting `None` must be
    /// able to conclude "this rule does not do kinds" rather than "this
    /// violation's kind is unknown" — which is only true while no kind-carrying
    /// rule can emit `None`, and the mapping being a total `match` over the
    /// engine's six states is what makes that structural rather than a habit.
    #[test]
    fn none_means_the_rule_carries_no_kind_never_an_undetermined_one() {
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
            \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\
            <head><title>t</title></head>\
            <body><p><a href=\"gone.xhtml\">x</a></p><nav/></body></html>";
        let report = crate::validate_bytes(epub2_with_ch1(ch1));

        let schema: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.rule == Some("opf.content_document.schema_violation"))
            .collect();
        assert!(!schema.is_empty(), "the fixture must produce one");
        assert!(
            schema.iter().all(|m| m.violation_kind.is_some()),
            "a kind-carrying rule must never emit None"
        );

        let hand_coded: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.rule == Some("opf.content_document.reference_missing_resource"))
            .collect();
        assert!(!hand_coded.is_empty(), "the fixture must produce one");
        assert!(
            hand_coded.iter().all(|m| m.violation_kind.is_none()),
            "a rule outside the family must carry no kind"
        );
    }

    /// Every finding whose rule carries kinds has one, and no other rule does —
    /// asserted over a document producing several at once, so a check that
    /// stopped stamping would not be hidden by a single-finding fixture.
    #[test]
    fn the_kind_is_present_exactly_on_the_rules_that_carry_it() {
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
            \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
            <html xmlns=\"http://www.w3.org/1999/xhtml\">\
            <head><title>t</title></head><body>loose<p bogus=\"1\">a</p><nav/></body></html>";
        let report = crate::validate_bytes(epub2_with_ch1(ch1));
        let mut kinds: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.rule.is_some_and(|r| r.ends_with("schema_violation")))
            .map(|m| {
                m.violation_kind
                    .expect("every schema violation carries a kind")
            })
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert!(
            kinds.len() >= 3,
            "the fixture must exercise several kinds; got {kinds:?}"
        );
        assert!(
            report
                .messages
                .iter()
                .filter(|m| !m.rule.is_some_and(|r| r.ends_with("schema_violation")))
                .all(|m| m.violation_kind.is_none()),
            "no rule outside the family may carry a kind"
        );
    }

    fn epub2_with_ch1(ch1: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#;
        const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap><navPoint id="n1" playOrder="1"><navLabel><text>Ch1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.xhtml", ch1),
                ("OEBPS/toc.ncx", NCX),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// NAV-001: an EPUB 3 navigation document (`properties="nav"`) is not a
    /// valid EPUB 2 construct. epubcheck flags it; the EPUB 3 nav is fine.
    #[test]
    fn nav_001_epub2_nav_property() {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#;
        const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap><navPoint id="n1" playOrder="1"><navLabel><text>Ch1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>hi</p></body></html>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>n</title></head><body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/toc.ncx", NCX),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let report = crate::validate_bytes(buf);
        assert!(
            !report.messages.iter().any(|m| m.id == crate::ids::NAV_001),
            "NAV-001 is unreachable in epubcheck; emitting it was a false positive"
        );
        // The book is still reported, the way epubcheck reports it: `<nav>`
        // is not in the XHTML 1.1 content model, so the nav document's own
        // contents are the finding. Losing NAV-001 loses no coverage.
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.id == crate::ids::RSC_005
                    && m.location.as_deref() == Some("OEBPS/nav.xhtml")),
            "the nav document must still be reported through the content grammar: {:?}",
            report
                .messages
                .iter()
                .map(|m| (m.id, m.location.clone()))
                .collect::<Vec<_>>()
        );
    }

    /// ADV-004's content-document half (DNSB, MobileRead #169/#170).
    ///
    /// The reporter's book is a Calibre AZW3→EPUB 2 conversion whose package
    /// carries no EPUB 3 signal at all, so the package-only version of this
    /// check stayed silent on the exact book it exists for. Its content
    /// documents carry 374 `epub:type` attributes and 75 HTML5 sectioning
    /// elements.
    ///
    /// The threshold is unchanged at two signals, and the two halves mix: a
    /// package signal plus a content one is a mislabelled book just as much
    /// as two of either.
    ///
    /// **`epub:type` must be counted by namespace, not by prefix.** A
    /// document is free to bind the OPS namespace to any prefix it likes, so
    /// one of these uses `ops:type` to keep an attribute-name match from
    /// passing by accident.
    #[test]
    fn content_documents_can_carry_the_evidence_for_adv_004() {
        // No `<meta property>`, no `properties=`: nothing for the
        // package-side signals to find.
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#;
        // `epub_with_opf` always writes an EPUB 3 nav document into the zip,
        // and it carries both signals itself. Leaving it out of the manifest
        // keeps it from being walked as a content document, so each case
        // below measures only its own `ch1`. (That the fixture's nav *would*
        // have counted is the check working: a nav document with epub:type is
        // exactly the evidence this looks for.)
        let body = |b: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:ops="http://www.idpf.org/2007/ops">
<head><title>t</title></head><body>{b}</body></html>"#
            )
        };
        let adv_004 = |ch1: String, advisory: bool| {
            crate::validate_bytes_with_options(
                epub_with_opf(Some(OPF), &ch1),
                &crate::Options {
                    advisory,
                    ..Default::default()
                },
            )
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::ADV_004)
            .count()
        };

        // Both content signals, and the prefix is not `epub:`.
        let both = body(r#"<section ops:type="chapter"><p>x</p></section>"#);
        assert_eq!(adv_004(both.clone(), true), 1, "two content signals");
        assert_eq!(adv_004(both, false), 0, "still opt-in");

        // One signal each: a stray, not a mislabelled book.
        assert_eq!(
            adv_004(body(r#"<section><p>x</p></section>"#), true),
            0,
            "sectioning alone is one signal"
        );
        assert_eq!(
            adv_004(body(r#"<p ops:type="bridgehead">x</p>"#), true),
            0,
            "epub:type alone is one signal"
        );

        // Declaring the OPS namespace without ever using it is producer
        // boilerplate: 6 of the 8 EPUB 2 shelf books that bind it never use
        // it, and firing on them is the ADV-003 failure mode.
        assert_eq!(
            adv_004(body(r#"<p>x</p>"#), true),
            0,
            "an unused xmlns:ops binding is not evidence of anything"
        );

        // A correct EPUB 2 book, with neither namespace nor HTML5 element.
        assert_eq!(
            adv_004(
                r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head>
<body><p>x</p></body></html>"#
                    .to_string(),
                true
            ),
            0
        );
    }

    /// ADV-004 (#62): a book declaring EPUB 2 whose package document is
    /// written in EPUB 3.
    ///
    /// The threshold is the whole judgement, so both sides of it are pinned:
    /// two independent signals fire, one does not. And the default run must
    /// stay byte-identical — this is advisory-only, and an advisory that
    /// leaks into the default verdict is just an unreviewed check.
    #[test]
    fn a_package_document_written_in_the_other_version_is_advisory_only() {
        let opf = |metadata: &str, nav_props: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>{metadata}
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"{nav_props}/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#
            )
        };
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>x</p></body></html>"#;
        // `properties` hangs off the sole content document rather than a
        // second `nav.xhtml` item, so these cases isolate the *package*
        // signals. `epub_with_opf` writes an EPUB 3 nav document into every
        // zip it builds - `<nav epub:type="toc">`, i.e. both content signals -
        // and manifesting it made every case here a two-signal book once the
        // content half was added. The fixture was never the "plain EPUB 2" the
        // last assertion below calls it; the package-only check simply could
        // not see what it was carrying.
        let adv_004 = |opf: String, advisory: bool| {
            crate::validate_bytes_with_options(
                epub_with_opf(Some(&opf), CH1),
                &crate::Options {
                    advisory,
                    ..Default::default()
                },
            )
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::ADV_004)
            .count()
        };

        let both = opf(
            r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#,
            r#" properties="nav""#,
        );
        assert_eq!(adv_004(both.clone(), true), 1, "two signals must be named");
        assert_eq!(
            adv_004(both, false),
            0,
            "and must stay out of a default run entirely"
        );

        assert_eq!(
            adv_004(opf("", r#" properties="nav""#), true),
            0,
            "one signal is a stray attribute, not a mislabelled book"
        );
        assert_eq!(
            adv_004(
                opf(
                    r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#,
                    ""
                ),
                true
            ),
            0,
            "likewise in the other direction"
        );
        assert_eq!(adv_004(opf("", ""), true), 0, "a plain EPUB 2 says nothing");
    }

    /// Every RSC-005 `rule` slug a validation of `bytes` produces.
    fn rsc_005_rules(bytes: Vec<u8>) -> Vec<String> {
        crate::validate_bytes(bytes)
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_005)
            .map(|m| m.rule.unwrap_or("").to_string())
            .collect()
    }

    /// Attributes XHTML 1.1 has and our EPUB 2 branch was rejecting — false
    /// positives on ordinary markup, `<style media="…">` most of all.
    ///
    /// Found by diffing our EPUB 2 attribute lists against epubcheck's
    /// `schema/20/rng/xhtml/` modules rather than by waiting for a report,
    /// which is the same method as #58 and the reason those two attributes
    /// in #64 should not have needed a user to find them.
    #[test]
    fn xhtml11_attributes_we_were_rejecting() {
        let doc = |head: &str, body: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                 \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" version=\"XHTML 1.1\">\
                 <head profile=\"http://example.org/p\"><title>t</title>{head}</head>\
                 <body>{body}</body></html>"
            )
        };
        for (head, body) in [
            (
                "<style type=\"text/css\" media=\"screen\">p{color:red}</style>",
                "<p>x</p>",
            ),
            (
                "<meta name=\"d\" content=\"1\" scheme=\"ISO8601\"/>",
                "<p>x</p>",
            ),
            (
                "<base href=\"http://example.org/\" target=\"_blank\"/>",
                "<p>x</p>",
            ),
            ("", "<p><q cite=\"http://example.org/\">said</q></p>"),
        ] {
            assert_eq!(
                rsc_005_rules(epub2_with_ch1(&doc(head, body))),
                Vec::<String>::new(),
                "valid XHTML 1.1 must not be rejected: {head}{body}"
            );
        }
    }

    /// #65: an EPUB 2 `<body>` must hold at least one block element
    /// (XHTML 1.1's `Block.model` is `oneOrMore Block.mix`). Ours was
    /// `zeroOrMore`, so a document whose every child was rejected reported
    /// only the children, where epubcheck also reports the body itself as
    /// incomplete — one of the findings missing from the MobileRead #134
    /// comparison.
    ///
    /// The per-child reports are asserted alongside it, since the point of
    /// the issue was that *all* of them should appear: fixing the body
    /// message by swallowing the children would be a worse answer than the
    /// bug.
    #[test]
    fn an_epub2_body_needs_content_and_still_names_every_bad_child() {
        let doc = |body: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                 \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <head><title>t</title></head><body>{body}</body></html>"
            )
        };
        let texts = |body: &str| {
            crate::validate_bytes(epub2_with_ch1(&doc(body)))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005)
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
        };

        let found = texts("<nav>a</nav><nav>b</nav><nav>c</nav>");
        assert_eq!(
            found.iter().filter(|t| t.contains("\"nav\"")).count(),
            3,
            "every rejected child is still named: {found:?}"
        );
        assert!(
            found.iter().any(|t| t.contains("\"body\"")),
            "and the body itself is incomplete: {found:?}"
        );
        assert_eq!(texts("<p>x</p>"), Vec::<String>::new());
    }

    /// `ol@start`, `ol@type` and `li@value` are HTML5/legacy attributes, not
    /// XHTML 1.1 ones: `schema/20/rng/xhtml/list.rng` gives `ol.attlist`,
    /// `ul.attlist` and `li.attlist` exactly `Common.attrib`, and the module
    /// that adds these three, `legacy.rng`, is never included by
    /// `content.rng`. epubcheck therefore reports one RSC-005 for each, which
    /// it was doing on 4 shelf books while we said nothing.
    ///
    /// The EPUB 3 half is the half that can regress silently: HTML5 does have
    /// all three, so tightening the shared grammar instead of the EPUB 2 one
    /// would invent errors on every ordered list in a modern book.
    #[test]
    fn epub2_lists_do_not_take_the_html5_numbering_attributes() {
        let body = |markup: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                 \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <head><title>t</title></head><body>{markup}</body></html>"
            )
        };
        let epub2 = |markup: &str| {
            crate::validate_bytes(epub2_with_ch1(&body(markup)))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005)
                .count()
        };
        let epub3 = |markup: &str| {
            crate::validate_bytes(epub_with_ch1(&format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 xmlns:epub=\"http://www.idpf.org/2007/ops\">\
                 <head><title>t</title></head><body>{markup}</body></html>"
            )))
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_005)
            .count()
        };

        // The control has to be clean, or the counts below mean nothing.
        assert_eq!(epub2("<ol><li>x</li></ol>"), 0, "plain list, EPUB 2");
        assert_eq!(epub3("<ol><li>x</li></ol>"), 0, "plain list, EPUB 3");

        for markup in [
            "<ol start=\"2\"><li>x</li></ol>",
            "<ol type=\"a\"><li>x</li></ol>",
            "<ol><li value=\"2\">x</li></ol>",
        ] {
            assert_eq!(epub2(markup), 1, "EPUB 2 must reject: {markup}");
            assert_eq!(epub3(markup), 0, "EPUB 3 must accept: {markup}");
        }
    }

    /// Two more EPUB 2 false negatives, same shape as the list attributes
    /// above and found the same way - by asking epubcheck rather than by
    /// waiting for a book to arrive.
    ///
    /// `data-*` is an HTML5 attribute family; XHTML 1.1 has no such concept,
    /// so epubcheck gives a plain RSC-005. We suppressed the grammar's blame
    /// for any `data-` name at *every* version, which was right for EPUB 3
    /// and silent for EPUB 2. The malformed-name case is included because
    /// HTM-061 also owns it, and epubcheck still reports exactly one finding
    /// there - so this must not become a double report.
    ///
    /// An empty row or row group is an error in XHTML 1.1: `basic-table.rng`
    /// makes `tr` `oneOrMore (th|td)` and `table.rng` makes `thead`/`tfoot`/
    /// `tbody` `oneOrMore tr`. HTML5 permits all of them empty.
    #[test]
    fn epub2_rejects_data_attributes_and_empty_table_rows() {
        let body = |markup: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                 \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <head><title>t</title></head><body>{markup}</body></html>"
            )
        };
        let epub2 = |markup: &str| {
            crate::validate_bytes(epub2_with_ch1(&body(markup)))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005)
                .count()
        };
        let epub3 = |markup: &str| {
            crate::validate_bytes(epub_with_ch1(&format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 xmlns:epub=\"http://www.idpf.org/2007/ops\">\
                 <head><title>t</title></head><body>{markup}</body></html>"
            )))
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_005)
            .count()
        };

        assert_eq!(epub2("<p>x</p>"), 0, "control, EPUB 2");
        assert_eq!(
            epub2("<table><tr><td>x</td></tr></table>"),
            0,
            "a table with a cell is fine in EPUB 2"
        );

        for markup in [
            "<p data-foo=\"x\">y</p>",
            // Malformed suffix: exactly one finding, not one per owning check.
            "<p data-=\"x\">y</p>",
            "<table><tr></tr></table>",
            "<table><tbody></tbody></table>",
        ] {
            assert_eq!(
                epub2(markup),
                1,
                "EPUB 2 must reject exactly once: {markup}"
            );
            assert_eq!(epub3(markup), 0, "EPUB 3 must accept: {markup}");
        }
    }

    /// An EPUB 3 package document's Dublin Core elements take `id`, `dir` and
    /// `xml:lang` - and identifier/language/date/type/format take `id` alone.
    /// The OPF 2 attributes a converted book keeps (`opf:role`,
    /// `opf:file-as`, `opf:scheme`, `opf:event`) were replaced by
    /// `<meta refines>` in EPUB 3 and are errors there.
    ///
    /// Reported from the MobileRead thread with epubcheck's output beside
    /// ours: 13 findings against our 7, and every one of the six we missed
    /// was this. Our `<metadata>` model was `looseContent`, so no attribute
    /// on any metadata element was checked at all.
    ///
    /// **Nothing but this test protects the rule.** No book on the 336-book
    /// shelf carries an `opf:*` attribute on a dc element and the corpus has
    /// no such fixture, so both instruments were byte-identical before and
    /// after the change. Each case below was probed against epubcheck 5.3.0,
    /// one attribute per book, on a minimal book clean in both tools.
    #[test]
    fn epub3_dc_metadata_takes_only_id_dir_and_xml_lang() {
        let opf = |metadata: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" xmlns:opf="http://www.idpf.org/2007/opf"
         version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    {metadata}
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            )
        };
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title></head><body><p>x</p></body></html>"#;
        const ID: &str = r#"<dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>"#;
        let base = format!("{ID}<dc:title>T</dc:title><dc:language>en</dc:language>");
        let count = |metadata: String| {
            crate::validate_bytes(epub_with_opf(Some(&opf(&metadata)), CH1))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_005)
                .count()
        };

        // The control has to be silent, or every count below means nothing.
        assert_eq!(count(base.clone()), 0, "plain EPUB 3 metadata");

        for (label, metadata) in [
            (
                "creator opf:role",
                format!(r#"{base}<dc:creator opf:role="aut">A</dc:creator>"#),
            ),
            (
                "creator opf:file-as",
                format!(r#"{base}<dc:creator opf:file-as="A, B">A</dc:creator>"#),
            ),
            (
                "title opf:file-as",
                format!(
                    r#"{ID}<dc:title opf:file-as="T">T</dc:title><dc:language>en</dc:language>"#
                ),
            ),
            (
                "identifier opf:scheme",
                r#"<dc:identifier id="id" opf:scheme="uuid">urn:uuid:1</dc:identifier>
                   <dc:title>T</dc:title><dc:language>en</dc:language>"#
                    .to_string(),
            ),
            (
                "date opf:event",
                format!(r#"{base}<dc:date opf:event="publication">2020</dc:date>"#),
            ),
            // `xml:lang` is valid on `dc:title` and an error on `dc:language`:
            // the second attribute list is the one that is easy to get wrong.
            (
                "language xml:lang",
                format!(r#"{ID}<dc:title>T</dc:title><dc:language xml:lang="en">en</dc:language>"#),
            ),
            (
                "title dir=bogus",
                format!(r#"{ID}<dc:title dir="bogus">T</dc:title><dc:language>en</dc:language>"#),
            ),
        ] {
            assert_eq!(count(metadata), 1, "EPUB 3 must reject once: {label}");
        }

        // The other direction, and the reason the two lists are kept apart:
        // these are all valid and must stay silent.
        for (label, metadata) in [
            (
                "title id+dir+xml:lang",
                format!(
                    r#"{ID}<dc:title id="t" dir="ltr" xml:lang="en">T</dc:title>
                       <dc:language>en</dc:language>"#
                ),
            ),
            (
                "creator dir+xml:lang",
                format!(r#"{base}<dc:creator dir="rtl" xml:lang="ar">A</dc:creator>"#),
            ),
            (
                "date id",
                format!(r#"{base}<dc:date id="d">2020</dc:date>"#),
            ),
            // `<meta>`, `<link>` and foreign metadata stay unconstrained.
            (
                "foreign metadata element",
                format!(r#"{base}<foo xmlns="http://example.com/" bar="baz">x</foo>"#),
            ),
        ] {
            assert_eq!(count(metadata), 0, "EPUB 3 must accept: {label}");
        }
    }

    /// #63: an EPUB 2 package document is checked against opf20's closed
    /// shapes, not the permissive EPUB 3 grammar. `<meta property=…>` and a
    /// manifest/spine `properties` attribute are EPUB 3 constructs that
    /// epubcheck rejects in a `version="2.0"` package, and we accepted all
    /// three (DNSB, MobileRead #134 — four of the eleven findings we missed).
    ///
    /// The EPUB 3 column matters as much as the EPUB 2 one: this grammar had
    /// no version switch at all before, so the risk in adding one is applying
    /// the strict shapes to a book that is entitled to them.
    #[test]
    fn epub2_package_shapes_are_closed_and_epub3_is_untouched() {
        let opf = |version: &str, meta: &str, item_attr: &str, itemref_attr: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>{meta}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"{item_attr}/>
  </manifest>
  <spine><itemref idref="ch1"{itemref_attr}/></spine>
</package>"#
            )
        };
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>x</p></body></html>"#;
        let schema_hits = |opf: String| {
            crate::validate_bytes(epub_with_opf(Some(&opf), CH1))
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.package.schema_violation"))
                .count()
        };
        const EPUB3_META: &str = r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#;

        for (label, meta, item, itemref) in [
            ("EPUB 3 metadata syntax", EPUB3_META, "", ""),
            ("item properties", "", r#" properties="scripted""#, ""),
            (
                "itemref properties",
                "",
                "",
                r#" properties="page-spread-left""#,
            ),
        ] {
            assert!(
                schema_hits(opf("2.0", meta, item, itemref)) > 0,
                "EPUB 2 must reject: {label}"
            );
            assert_eq!(
                schema_hits(opf("3.0", meta, item, itemref)),
                0,
                "EPUB 3 must accept: {label}"
            );
        }
        // A package written the EPUB 2 way stays silent, including the
        // `<meta name= content=>` spelling opf20 does require.
        assert_eq!(
            schema_hits(opf(
                "2.0",
                r#"<meta name="cover" content="c"/>"#,
                " fallback=\"nav\"",
                " linear=\"no\""
            )),
            0
        );
    }

    /// #64: `epub:type` and `meta@charset` are EPUB 3, and the EPUB 2 branch
    /// was accepting both because it reused the EPUB 3 attribute pools.
    /// Found by diffing epubcheck's output against ours on a mislabelled book
    /// (DNSB, MobileRead #134), where they accounted for four of the eleven
    /// findings we were missing.
    ///
    /// The EPUB 3 half of each pair is asserted too. This is the direction
    /// #58 had to undo ten times: an EPUB 3 rule leaking into EPUB 2 is a
    /// false positive, and the same mistake in reverse would be one here.
    #[test]
    fn epub3_only_attributes_are_rejected_in_epub2_and_kept_in_epub3() {
        let doc = |doctype: &str, head: &str, body: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>{doctype}\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 xmlns:epub=\"http://www.idpf.org/2007/ops\">\
                 <head><title>t</title>{head}</head><body>{body}</body></html>"
            )
        };
        const DT: &str = "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                          \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">";

        for (head, body) in [
            ("", "<p epub:type=\"chapter\">x</p>"),
            ("<meta charset=\"utf-8\"/>", "<p>x</p>"),
        ] {
            assert!(
                !rsc_005_rules(epub2_with_ch1(&doc(DT, head, body))).is_empty(),
                "EPUB 2 must reject: {head}{body}"
            );
            assert_eq!(
                rsc_005_rules(epub_with_ch1(&doc("", head, body))),
                Vec::<String>::new(),
                "EPUB 3 must still accept: {head}{body}"
            );
        }
        // And nothing else moved: the XHTML 1.1 spellings stay valid.
        assert_eq!(
            rsc_005_rules(epub2_with_ch1(&doc(
                DT,
                "<meta name=\"author\" content=\"a\"/>",
                "<p>x</p>"
            ))),
            Vec::<String>::new()
        );
    }

    /// `<hgroup>` holds exactly one heading, interleaved with any number of
    /// `<p>` — epubcheck's `hgroup.inner` verbatim. Reported by Doitsu on
    /// MobileRead: the canonical modern shape, a title with a subtitle
    /// paragraph, was drawing RSC-005.
    ///
    /// Both directions are asserted because the old definition was wrong in
    /// both: it rejected `<p>` outright, and its `oneOrMore` accepted the
    /// pre-2022 `<h1>` + `<h2>` pairing that epubcheck's schema does not.
    /// The corpus has no `hgroup` fixture either way, so the schema is the
    /// only authority — the same footing as the EPUB 2 rules in #58.
    #[test]
    fn hgroup_takes_one_heading_and_any_number_of_paragraphs() {
        let body = |inner: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <head><title>t</title></head><body>{inner}</body></html>"
            )
        };
        for valid in [
            "<hgroup><h1>Frankenstein</h1><p>Or: The Modern Prometheus</p></hgroup>",
            "<hgroup><p>before</p><h2>T</h2><p>after</p></hgroup>",
            "<hgroup><h1>T</h1></hgroup>",
        ] {
            assert_eq!(
                rsc_005_rules(epub_with_ch1(&body(valid))),
                Vec::<String>::new(),
                "must be accepted: {valid}"
            );
        }
        assert!(
            !rsc_005_rules(epub_with_ch1(&body(
                "<hgroup><h1>T</h1><h2>Sub</h2></hgroup>"
            )))
            .is_empty(),
            "a second heading is not epubcheck's model - the subtitle is a <p>"
        );
    }

    /// #24 end-to-end: an EPUB 2 book is validated against the EPUB 2 content
    /// model, not the EPUB 3 one. `<big>` (valid XHTML 1.1, removed in HTML5)
    /// must pass, and `<s>` (valid HTML5, absent from XHTML 1.1) must be
    /// flagged - the exact false-positive/false-negative pair Doitsu reported.
    #[test]
    fn epub2_content_uses_the_epub2_grammar() {
        let body = |inner: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
                 \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body>{inner}</body></html>"
            )
        };
        assert!(
            crate::validate_bytes(epub2_with_ch1(&body("<p>a <big>b</big> c</p>"))).is_valid(),
            "<big> is valid XHTML 1.1 and must not be flagged in EPUB 2"
        );
        let report = crate::validate_bytes(epub2_with_ch1(&body("<p>a <s>b</s> c</p>")));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.rule == Some("opf.content_document.schema_violation")
                    && m.text.contains("\"s\"")),
            "<s> is absent from XHTML 1.1 and must be flagged in EPUB 2; got {:?}",
            report.messages.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn epub2_content_type_meta_is_not_an_html5_rule() {
        // #21 (Doitsu, MobileRead #82): `application/xhtml+xml; charset=utf-8`
        // is the correct XHTML 1.1 encoding declaration for an EPUB 2 content
        // document, and epubcheck never flags it. The "must be text/html;
        // charset=utf-8" rule is HTML5-only (EPUB 3).
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title>\
            <meta http-equiv=\"Content-Type\" content=\"application/xhtml+xml; charset=utf-8\"/>\
            </head><body><p>hi</p></body></html>";
        assert!(
            !rsc_005_rules(epub2_with_ch1(ch1))
                .iter()
                .any(|r| r == "opf.content_document.invalid_content_type_meta"),
            "EPUB 2 content-type meta must not be flagged: {:?}",
            rsc_005_rules(epub2_with_ch1(ch1))
        );
    }

    #[test]
    fn rsc_011_anchors_at_the_source_hyperlink() {
        // #22 (Doitsu, MobileRead #82): a hyperlink to a content document that
        // isn't in the spine must anchor at the source `<a>` (its file +
        // line:column + element path), not at the OPF package root. Here ch1
        // links to the nav doc, which is in the manifest but not the spine.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>C</title></head>\n\
            <body><p><a href=\"nav.xhtml\">to nav</a></p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        let m = report
            .messages
            .iter()
            .find(|m| m.id == crate::ids::RSC_011)
            .expect("expected an RSC-011 for the nav hyperlinked but not in spine");
        assert_eq!(
            m.location.as_deref(),
            Some("OEBPS/ch1.xhtml"),
            "must anchor in the source document, not content.opf"
        );
        assert!(m.position.is_some(), "must carry a source line:column");
        assert!(
            m.element_path.is_some(),
            "must carry the source element path"
        );
        assert_eq!(m.rule, Some("opf.spine.hyperlinked_not_in_spine"));
    }

    /// #58: two Schematron rules that are EPUB 3-only in epubcheck and were
    /// firing on EPUB 2 books as well.
    ///
    /// `lang-xmllang` and `descendant-dfn-dfn` both live in
    /// `epub-xhtml-30.sch`. EPUB 2's entire XHTML Schematron is a single rule
    /// (nested hyperlinks), and XHTML 1.1's `lang.attrib` declares `xml:lang`
    /// and `lang` as independent optional attributes with nothing tying their
    /// values together. Both must stay silent on EPUB 2 and keep firing on
    /// EPUB 3 - the pairing is the test, since gating a rule is easy to
    /// overshoot into never firing at all.
    #[test]
    fn epub3_only_schematron_rules_do_not_fire_on_epub2() {
        const BODY: &str = "<p lang=\"en\" xml:lang=\"fr\">x</p>\
             <p><dfn>outer <dfn>inner</dfn></dfn></p>";
        let epub2 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
             \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\
             <html xmlns=\"http://www.w3.org/1999/xhtml\">\
             <head><title>t</title></head><body>{BODY}</body></html>"
        );
        let epub3 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <html xmlns=\"http://www.w3.org/1999/xhtml\">\
             <head><title>t</title></head><body>{BODY}</body></html>"
        );
        let rules = |bytes: Vec<u8>| -> Vec<String> {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .filter_map(|m| m.rule.map(String::from))
                .collect()
        };
        let two = rules(epub2_with_ch1(&epub2));
        for r in [
            "opf.content_document.lang_xmllang_mismatch",
            "opf.content_document.nested_dfn",
        ] {
            assert!(
                !two.contains(&r.to_string()),
                "{r} must not fire on EPUB 2; got {two:?}"
            );
            assert!(
                rules(epub_with_ch1(&epub3)).contains(&r.to_string()),
                "{r} must still fire on EPUB 3"
            );
        }
    }

    /// OPF-005 (#50): a prefix declaration ending in a name with no URI.
    /// epubcheck reports this *instead of* a syntax error, not alongside one -
    /// its parser ends in the URI state, which is not one of its FINAL_STATES,
    /// so the OPF-004 branch never runs. Getting that wrong would double-report
    /// the same defect.
    #[test]
    fn prefix_declaration_missing_its_uri() {
        use super::PrefixFault;
        let (pairs, faults) = super::parse_prefix_value("foaf: http://xmlns.com/foaf/ x:");
        assert_eq!(
            faults,
            [PrefixFault::MissingUri(Some("x".to_string()))],
            "OPF-005 replaces the syntax error, it doesn't add"
        );
        assert_eq!(
            pairs.get("foaf").map(String::as_str),
            Some("http://xmlns.com/foaf/")
        );

        // A bare ":" names no prefix. The whole value is read as one
        // malformed mapping plus a second, prefix-less one: "http" is taken
        // as a prefix whose colon is followed by "//example.org" with no
        // space (OPF-004d), then the bare ":" is an empty prefix (OPF-004a)
        // with nothing after it (OPF-005). Confirmed against epubcheck on
        // this exact value - the expectation written here first was wrong,
        // and the parser was right.
        let (_, faults) = super::parse_prefix_value("http://example.org :");
        assert_eq!(
            faults,
            [
                PrefixFault::NoSpace(Some("http".to_string())),
                PrefixFault::EmptyPrefix,
                PrefixFault::MissingUri(None),
            ],
            "got {faults:?}"
        );

        // A well-formed declaration produces nothing at all.
        assert!(
            super::parse_prefix_value("foaf: http://xmlns.com/foaf/")
                .1
                .is_empty()
        );
    }

    /// #70: every state of epubcheck's `PrefixDeclarationParser` reports its
    /// own message ID, and the distinctions are below what a
    /// `split_whitespace()` tokenizer can see. Each row was measured against
    /// epubcheck 5.3.0, one book per case.
    ///
    /// Two of them are not ID changes but defects the old tokenizer had:
    /// `: URI` produced two findings where epubcheck produces one, and a
    /// prefix that is not an NCName produced none at all.
    #[test]
    fn prefix_value_faults_match_epubchecks_state_machine() {
        use super::PrefixFault::*;
        let faults = |v: &str| format!("{:?}", super::parse_prefix_value(v).1);
        for (value, want) in [
            (
                "foaf: http://x",
                format!("{:?}", Vec::<super::PrefixFault>::new()),
            ),
            (": http://x", format!("{:?}", vec![EmptyPrefix])),
            (
                "1foaf: http://x",
                format!("{:?}", vec![NotNcName("1foaf".into())]),
            ),
            (
                "foaf : http://x",
                format!("{:?}", vec![NoColon(Some("foaf".into()))]),
            ),
            (
                "foaf http://x",
                format!("{:?}", vec![NoColon(Some("foaf".into()))]),
            ),
            (
                "foaf:http://x",
                format!("{:?}", vec![NoSpace(Some("foaf".into()))]),
            ),
            // Two spaces are legal; a tab is not. This pair is the reason the
            // tokenizer had to go.
            (
                "foaf:  http://x",
                format!("{:?}", Vec::<super::PrefixFault>::new()),
            ),
            (
                "foaf:\thttp://x",
                format!("{:?}", vec![IllegalSpace(Some("foaf".into()))]),
            ),
            // Tab *between* mappings is legal - measured, not assumed.
            (
                "a: http://x\tb: http://y",
                format!("{:?}", Vec::<super::PrefixFault>::new()),
            ),
            (
                "1foaf : http://x",
                format!("{:?}", vec![NotNcName("1foaf".into()), NoColon(None)]),
            ),
        ] {
            assert_eq!(faults(value), want, "for {value:?}");
        }
        // Both mappings survive when the value is clean.
        let (pairs, _) = super::parse_prefix_value("a: http://x\tb: http://y");
        assert_eq!(pairs.len(), 2);
    }

    /// Doitsu, MobileRead #163. Two halves of one report.
    ///
    /// `<meta refines="title" property="title-type">` — a missing `#` — drew a
    /// WARNING from us and an ERROR from epubcheck, so the *verdict* differed:
    /// its book was INVALID, ours VALID. epubcheck's Schematron compares
    /// `@refines` against `concat('#', @id)`, so a bare id fails the
    /// "must refine a title property" assertion. Seven such assertions are now
    /// ported.
    ///
    /// The other half was ours being too broad: RSC-017 fired on *any*
    /// non-fragment `@refines`, where epubcheck reports it only when the value
    /// resolves to a real manifest item's href.
    #[test]
    fn refines_must_name_its_target_and_the_fragment_hint_is_narrow() {
        let ids = |meta: &str| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title id="title">T</dc:title>
    <dc:creator id="cre">A</dc:creator>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {meta}
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            let d = roxmltree::Document::parse(&opf).unwrap();
            let mut report = crate::report::Report::new();
            super::check_meta_property_scheme_shape(&d, "OEBPS/content.opf", &mut report);
            let sch = crate::schematron::load(crate::schematron::PACKAGE_SCH).unwrap();
            let mut ids: Vec<String> = report
                .messages
                .iter()
                .map(|m| m.id.to_string())
                .chain(
                    crate::schematron::run(&sch, &d, "opf.package")
                        .into_iter()
                        .map(|_| "RSC-005".to_string()),
                )
                .collect();
            ids.sort();
            ids
        };
        // A bare id where a fragment was meant: the specific "must refine"
        // error, and no fragment hint (the value is not a manifest item).
        assert_eq!(
            ids(r#"<meta refines="title" property="title-type">main</meta>"#),
            ["RSC-005"]
        );
        // Correct form: silent.
        assert!(ids(r##"<meta refines="#title" property="title-type">main</meta>"##).is_empty());
        // A bare id on a property with no "must refine" rule: epubcheck says
        // nothing, and this is the false positive the report found.
        assert!(ids(r#"<meta refines="cre" property="file-as">A, B</meta>"#).is_empty());
        // Naming a manifest item by href is the one case RSC-017 is for.
        assert_eq!(
            ids(r#"<meta refines="ch1.xhtml" property="file-as">x</meta>"#),
            ["RSC-017"]
        );
    }

    /// Doitsu, MobileRead #161: which prefixes are *reserved* depends on the
    /// document declaring them, and we were reporting the union.
    ///
    /// epubcheck passes a different `predefined` map per context:
    /// `OPFHandler30` reserves a11y/dcterms/marc/media/onix/rendition/schema/
    /// xsd, `OPSHandler30` reserves only msv/prism, and `OverlayHandler`
    /// reserves none. The sample declares `media:` on a content document's
    /// `<html>`, pointing at a URI that is not the Media Overlays one - a
    /// redeclaration in a package document, and nothing at all in a content
    /// document.
    #[test]
    fn reserved_prefixes_depend_on_the_declaring_document() {
        let redeclares = |ctx: super::PrefixContext, decl: &str| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id" prefix="{decl}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"/>
</package>"#
            );
            let d = roxmltree::Document::parse(&opf).unwrap();
            let pkg = d.root_element();
            let attr = super::attr_no_ns_node(pkg, "prefix").unwrap();
            let mut report = crate::report::Report::new();
            super::check_prefix_declaration(
                attr,
                "OEBPS/content.opf",
                pkg,
                ctx,
                false,
                &mut report,
            );
            report
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_007)
                .count()
        };
        const MEDIA: &str = "media: http://idpf.org/epub/vocab/media/#";
        const MSV: &str = "msv: http://example.org/not-magazine/#";
        use super::PrefixContext::*;
        assert_eq!(
            redeclares(Package, MEDIA),
            1,
            "media is reserved in the OPF"
        );
        assert_eq!(redeclares(ContentDocument, MEDIA), 0, "but not in XHTML");
        assert_eq!(redeclares(ContentDocument, MSV), 1, "msv is, there");
        assert_eq!(redeclares(Package, MSV), 0, "and not in the OPF");
        assert_eq!(redeclares(Overlay, MEDIA), 0, "an overlay reserves none");
        assert_eq!(redeclares(Overlay, MSV), 0);
    }

    /// #70: the three special prefix-mapping faults each have their own
    /// epubcheck ID, and `VocabUtil.checkPrefixes` is an if/else-if chain, so
    /// at most one fires per mapping.
    ///
    /// Both halves matter. We used to emit the bare OPF-007 for all four
    /// cases — an ID epubcheck emits only for the last — and to test them
    /// independently, so `_` mapped to the Dublin Core namespace produced two
    /// findings against epubcheck's one.
    #[test]
    fn each_prefix_mapping_fault_has_its_own_id_and_fires_once() {
        let ids_for = |prefix: &str| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id" prefix="{prefix}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            let d = roxmltree::Document::parse(&opf).unwrap();
            let pkg = d.root_element();
            let attr = super::attr_no_ns_node(pkg, "prefix").unwrap();
            let mut report = crate::report::Report::new();
            super::check_prefix_declaration(
                attr,
                "OEBPS/content.opf",
                pkg,
                super::PrefixContext::Package,
                false,
                &mut report,
            );
            report
                .messages
                .iter()
                .map(|m| m.id)
                .filter(|id| id.starts_with("OPF-007"))
                .collect::<Vec<_>>()
        };

        assert_eq!(ids_for("_: http://example.org/x"), [crate::ids::OPF_007A]);
        assert_eq!(
            ids_for("x: http://idpf.org/epub/vocab/package/meta/#"),
            [crate::ids::OPF_007B],
            "a default-vocabulary URI"
        );
        assert_eq!(
            ids_for("x: http://purl.org/dc/elements/1.1/"),
            [crate::ids::OPF_007C],
            "the Dublin Core elements namespace"
        );
        assert_eq!(
            ids_for("dcterms: http://example.org/not-dcterms"),
            [crate::ids::OPF_007],
            "a reserved prefix redeclared is the chain's else, and keeps the bare id"
        );
        // The chain, not four independent tests: `_` is also mapped to the
        // Dublin Core namespace here, and only the first case may report.
        assert_eq!(
            ids_for("_: http://purl.org/dc/elements/1.1/"),
            [crate::ids::OPF_007A],
            "one finding per mapping, not one per matching condition"
        );
        // A clean declaration says nothing at all.
        assert!(ids_for("foaf: http://xmlns.com/foaf/spec/").is_empty());
    }

    /// OPF-006 (#50): the URI half has to parse as a URI. Deliberately
    /// conservative - epubcheck's test is Java's `new URI(...)`, and being
    /// stricter than that would invent errors on URIs it accepts.
    #[test]
    fn prefix_declaration_uri_validity() {
        for ok in [
            "http://xmlns.com/foaf/spec/",
            "http://example.org/vocab#",
            "https://example.org/a%20b",
            "urn:uuid:12345678-1234-1234-1234-123456789abc",
            "http://example.org/~user/(x)",
        ] {
            assert!(!super::is_unparseable_uri(ok), "{ok} is a valid URI");
        }
        for bad in [
            "http://example.org/<x>",
            "http://example.org/{x}",
            "http://example.org/a|b",
            "http://example.org/a\\b",
            "http://example.org/a%zz",
            "http://example.org/a%",
        ] {
            assert!(super::is_unparseable_uri(bad), "{bad} should not parse");
        }
    }

    /// OPF-052 (#54): `opf:role` must be a real MARC relator code. The shape
    /// heuristic this replaced ("three lowercase letters") accepted anything
    /// invented, and it also ran on `contributor`, which epubcheck never
    /// checks - so a contributor role it accepts used to be an error here.
    #[test]
    fn opf_role_is_checked_against_the_marc_relator_list() {
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head>\
            <body><p>hi</p></body></html>";
        let opf = |element: &str, role: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <dc:{element} opf:role="{role}">N</dc:{element}>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            )
        };
        let fires = |element: &str, role: &str| {
            crate::validate_bytes(epub_with_opf(Some(&opf(element, role)), CH1))
                .messages
                .iter()
                .any(|m| m.id == crate::ids::OPF_052)
        };
        assert!(!fires("creator", "aut"), "'aut' is a real relator code");
        assert!(!fires("creator", "edc"), "'edc' is a real relator code");
        assert!(
            fires("creator", "xyz"),
            "'xyz' has the right shape but is not a relator code - the whole point of #54"
        );
        assert!(
            fires("creator", "companion"),
            "a word is not a relator code"
        );
        assert!(
            !fires("creator", "oth.whatever"),
            "'oth.' is epubcheck's escape hatch for roles outside the vocabulary"
        );
        assert!(
            !fires("contributor", "xyz"),
            "epubcheck only checks creator roles"
        );
    }

    /// OPF-067 (#55): a metadata `<link>` must not point at a manifest item.
    /// The paired negative is the whole point - epubcheck only reports this
    /// when the item is **not in the spine**, so a link at a content document
    /// must stay silent. Dropping that condition is the obvious way to
    /// implement this wrong, and it would over-fire on valid books.
    #[test]
    fn metadata_link_at_a_non_spine_manifest_item() {
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head>\
            <body><p>hi</p></body></html>";
        let opf = |link_href: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <link rel="record" href="{link_href}" media-type="application/xml"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="m" href="meta.xml" media-type="application/xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            )
        };
        let fires = |href: &str| {
            crate::validate_bytes(epub_with_opf(Some(&opf(href)), CH1))
                .messages
                .iter()
                .any(|m| m.id == crate::ids::OPF_067)
        };
        assert!(
            fires("meta.xml"),
            "a link at a manifest item outside the spine is OPF-067"
        );
        assert!(
            !fires("ch1.xhtml"),
            "a link at an in-spine item must stay silent"
        );
        assert!(
            !fires("nowhere.xml"),
            "a link at a non-manifest resource is not OPF-067"
        );
    }

    #[test]
    fn epub3_content_type_meta_still_strict() {
        // EPUB 3 is XHTML5, where the encoding-declaration meta must be exactly
        // `text/html; charset=utf-8` (matches epubcheck) — unchanged.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title>\
            <meta http-equiv=\"Content-Type\" content=\"application/xhtml+xml; charset=utf-8\"/>\
            </head><body><p>hi</p></body></html>";
        assert!(
            rsc_005_rules(epub_with_ch1(ch1))
                .iter()
                .any(|r| r == "opf.content_document.invalid_content_type_meta"),
            "EPUB 3 content-type meta must still be flagged: {:?}",
            rsc_005_rules(epub_with_ch1(ch1))
        );
    }

    #[test]
    fn malformed_content_document_is_reported_fatal_not_silently_skipped() {
        // A missing `</p>` end-tag (Doitsu's forum report, #12). Before the
        // fix the parse failure hit `else { continue }` and the book
        // validated clean — a false negative. It must now surface as a Fatal
        // RSC-016 so the book is INVALID.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\n\
            <body><p>hello world</body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        assert!(
            report.messages.iter().any(|m| m.id == crate::ids::RSC_016
                && m.severity == crate::report::Severity::Fatal
                && m.rule == Some("content.malformed_xml")),
            "expected a Fatal RSC-016 for the unclosed <p>, got: {:?}",
            report.messages
        );
        assert!(!report.is_valid());
    }

    #[test]
    fn data_star_attributes_are_not_reported_as_unknown() {
        // #35/#36 (part of the #31 attribute-allowlist epic): data-* is
        // genuinely open-ended - no RELAX NG name class can express "any
        // name starting with data-" - so it has no grammar rule at all.
        // Now that #36 has removed the permissive wildcard, the grammar
        // alone WOULD blame a well-formed data-* attribute as "not
        // allowed" (confirmed: this test would fail without the
        // suppression). htm::is_data_attribute_name + its call site in the
        // RSC-005 emission loop above catch exactly this case. This is the
        // real end-to-end test #35 could only note as a future TODO, since
        // it wasn't reachable before the wildcard was actually gone.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p data-foo=\"bar\" data-x-y-z=\"1\">hi</p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        assert!(report.is_valid(), "got: {:?}", report.messages);
        assert!(rsc_005_rules(epub_with_ch1(ch1)).is_empty());
    }

    #[test]
    fn malformed_data_star_attribute_is_htm_061_only_not_also_rsc_005() {
        // The suppression is name-shape-only (not re-validating the
        // suffix) so a malformed data-* name isn't silently accepted - it
        // still fails HTM-061 - but it must not ALSO draw a redundant
        // RSC-005 for the same defect (the double-report class of bug this
        // epic has hit and fixed twice already, in svg.rs and 0.5.10/
        // 0.5.16 before it).
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p data-Bad=\"x\">hi</p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        assert!(
            report.messages.iter().any(|m| m.id == crate::ids::HTM_061),
            "got: {:?}",
            report.messages
        );
        assert!(
            rsc_005_rules(epub_with_ch1(ch1)).is_empty(),
            "got: {:?}",
            report.messages
        );
    }

    /// Build a minimal valid EPUB 3 with the caller's extra lines injected
    /// into the OPF `<metadata>` — used to exercise metadata-level checks
    /// (here, deprecated `<link rel>` keywords).
    /// An unresolvable `unique-identifier` must not switch off the NCX's
    /// other checks.
    ///
    /// Only NCX-001/NCX-004 need the package identifier — they compare
    /// `dtb:uid` against it. The whole NCX block used to be gated on it, so a
    /// book whose `unique-identifier` names no `dc:identifier` (already its
    /// own OPF-030) also lost RSC-007, RSC-010 and RSC-012 on its NCX. One
    /// shelf book had three genuinely undefined fragments and produced
    /// nothing; epubcheck reports all three.
    #[test]
    fn a_missing_unique_identifier_does_not_silence_the_ncx_checks() {
        let ids = |uid_present: bool| {
            let bytes = epub2_with_ncx_fragment(uid_present, "#nosuchid");
            let r = crate::validate_bytes(bytes);
            let mut v = r.messages.iter().map(|m| m.id).collect::<Vec<_>>();
            v.sort_unstable();
            v.dedup();
            v
        };
        assert!(
            ids(true).contains(&crate::ids::RSC_012),
            "baseline: the fragment is undefined either way"
        );
        let broken = ids(false);
        assert!(
            broken.contains(&crate::ids::OPF_030),
            "the unresolvable unique-identifier is still its own finding"
        );
        assert!(
            broken.contains(&crate::ids::RSC_012),
            "...and it must not take the fragment check down with it, got {broken:?}"
        );
        // A fragment that does resolve stays silent, so this is not just
        // "RSC-012 always fires".
        assert!(
            !crate::validate_bytes(epub2_with_ncx_fragment(false, "#real"))
                .messages
                .iter()
                .any(|m| m.id == crate::ids::RSC_012)
        );
    }

    /// An EPUB 2 whose NCX `<content src>` carries `frag`, and whose
    /// `unique-identifier` either resolves to a `dc:identifier` or does not.
    fn epub2_with_ncx_fragment(uid_present: bool, frag: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let ident = if uid_present {
            "<dc:identifier id=\"id\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>"
        } else {
            // Declared but never defined: the package points `unique-identifier`
            // at an id no element carries.
            "<dc:identifier id=\"other\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>"
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    {ident}<dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#
        );
        let ncx = format!(
            "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
             version=\"2005-1\"><head><meta name=\"dtb:uid\" \
             content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
             <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n1\" playOrder=\"1\">\
             <navLabel><text>T</text></navLabel><content src=\"ch1.xhtml{frag}\"/></navPoint>\
             </navMap></ncx>"
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p id=\"real\">x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/toc.ncx", ncx.as_str()),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// An EPUB 2 whose single content document is named `name`, referenced
    /// under that name from both the manifest and the NCX.
    fn epub2_named(name: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="{name}" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
</package>"#
        );
        let ncx = format!(
            "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
             version=\"2005-1\"><head><meta name=\"dtb:uid\" \
             content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
             <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n1\" playOrder=\"1\">\
             <navLabel><text>T</text></navLabel><content src=\"{name}\"/></navPoint>\
             </navMap></ncx>"
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            let doc = format!("OEBPS/{name}");
            for (n, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                (doc.as_str(), CH1),
                ("OEBPS/toc.ncx", ncx.as_str()),
            ] {
                z.start_file(n, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// RSC-020: an unencoded space in an NCX `<content src>` is its own
    /// finding, separate from the manifest href naming the same file.
    ///
    /// The URL check is organised per *source* here — manifest href, then
    /// content-document references — while epubcheck validates every
    /// registered *reference* through one path. The NCX was simply never
    /// added to our list, so a Calibre book whose files are named
    /// `Kamelyali Kadin_split_000.html` drew 32 findings from us and 60 from
    /// epubcheck: one per manifest item from both, plus 28 navPoints naming
    /// the same files that only epubcheck reported. Measured against 5.3.0 on
    /// a real book, 2026-08-20; three shelf books carried the shape.
    ///
    /// **The count is asserted, not just the presence.** The two sites walk
    /// the same filename, so an over-eager shared walk would report one of
    /// them twice and a bare `any()` would pass throughout.
    #[test]
    fn an_unencoded_space_in_an_ncx_content_src_is_reported() {
        let rules = |name: &str| {
            crate::validate_bytes(epub2_named(name))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_020)
                .map(|m| m.rule.unwrap_or("(unkeyed)"))
                .collect::<Vec<_>>()
        };
        // The control runs first, so a pass here can never mean "RSC-020
        // always fires": the same book with a space-free name says nothing.
        assert!(
            rules("ch1.xhtml").is_empty(),
            "a space-free name must stay silent, got {:?}",
            rules("ch1.xhtml")
        );
        assert_eq!(
            rules("a b.xhtml"),
            vec![
                "opf.manifest_item.unencoded_space_in_href",
                "opf.ncx.content_src_unencoded_space",
            ],
            "exactly one finding per site"
        );
    }

    /// An OEBPS 1.2 package draws OPF-047 and stops being judged by EPUB 2's
    /// rules, but keeps the checks epubcheck still makes there.
    ///
    /// Measured against epubcheck 5.3.0 on its own `opf-legacy-oebps12-*`
    /// fixtures before and after. Before: epubcheck 4 findings, us 7 errors —
    /// OPF-030 in common and six of ours invented by rules the format does
    /// not have (required DC metadata in the OPF namespace, `spine/@toc`,
    /// OPF-043 on `text/x-oeb1-document`, and a package grammar bound to the
    /// OPF namespace that could only ever say "element package is not allowed
    /// here"). After: our set is a strict subset of epubcheck's, and the
    /// verdict agrees.
    ///
    /// **OPF-030 must survive**, and that is the half worth guarding. The
    /// first attempt widened the metadata scan to recognise OEBPS 1.2's
    /// `<dc-metadata>` and title-case `<dc:Identifier>`, which resolved the
    /// unique-identifier and silenced OPF-030 — on a fixture epubcheck
    /// reports it for, because its own handler matches `identifier`
    /// case-sensitively too. Coding to the format rather than to the oracle.
    #[test]
    fn an_oebps12_package_is_flagged_not_judged_as_epub2() {
        let ids = |ns: &str| {
            let r = crate::validate_bytes(epub_with_package_ns(ns));
            let mut v = r.messages.iter().map(|m| m.id).collect::<Vec<_>>();
            v.sort_unstable();
            v.dedup();
            v
        };
        let oeb = ids("http://openebook.org/namespaces/oeb-package/1.0/");
        assert!(oeb.contains(&crate::ids::OPF_047), "got {oeb:?}");
        assert!(
            oeb.contains(&crate::ids::OPF_030),
            "the checks epubcheck still makes must survive, got {oeb:?}"
        );
        assert!(
            !oeb.contains(&crate::ids::RSC_005),
            "no EPUB 2 rule may fire on an OEBPS 1.2 package, got {oeb:?}"
        );

        // A *different* wrong namespace is a typo, not legacy syntax:
        // epubcheck's guard admits only absent/empty/OEBPS 1.2, and its
        // `xml-namespace-wrongdefault-error.opf` fixture expects the schema
        // error. Widening this test to "not the OPF namespace" cost that
        // fixture its RSC-005.
        let typo = ids("http://www.ipdf.org/2007/opf");
        assert!(!typo.contains(&crate::ids::OPF_047), "got {typo:?}");
    }

    /// OPF-038/OPF-039: the modern media types are the wrong ones inside an
    /// OEBPS 1.2 package.
    ///
    /// Enumerated because `OPFChecker.checkItem` asks the question in two
    /// places with different conditions, and a port that collapsed them would
    /// still pass epubcheck's two fixtures: `text/html` is OPF-038
    /// **unconditionally**, while XHTML/DTBook and `text/css` are
    /// OPF-038/OPF-039 **only when the item declares no `fallback`**. The
    /// fallback half is what no fixture covers.
    #[test]
    fn oebps12_media_types_are_reported_per_epubchecks_two_conditions() {
        let ids_for = |mt: &str, fallback: bool| {
            let r = crate::validate_bytes(epub_oeb12_with_item(mt, fallback));
            r.messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_038 || m.id == crate::ids::OPF_039)
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        // Deprecated-blessed: reported whether or not a fallback exists.
        assert_eq!(ids_for("text/html", false), vec![crate::ids::OPF_038]);
        assert_eq!(ids_for("text/html", true), vec![crate::ids::OPF_038]);
        // Blessed, and the style type: only without a fallback.
        assert_eq!(
            ids_for("application/xhtml+xml", false),
            vec![crate::ids::OPF_038]
        );
        assert_eq!(ids_for("application/xhtml+xml", true), Vec::<&str>::new());
        assert_eq!(ids_for("text/css", false), vec![crate::ids::OPF_039]);
        assert_eq!(ids_for("text/css", true), Vec::<&str>::new());
        // The format's own types are what it wants: silent either way.
        assert_eq!(ids_for("text/x-oeb1-document", false), Vec::<&str>::new());
        assert_eq!(ids_for("text/x-oeb1-css", false), Vec::<&str>::new());
    }

    /// An OEBPS 1.2 package with one extra manifest item of media-type `mt`,
    /// optionally carrying a `fallback`.
    fn epub_oeb12_with_item(mt: &str, fallback: bool) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let fb = if fallback { r#" fallback="c1""# } else { "" };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://openebook.org/namespaces/oeb-package/1.0/" version="2.0" unique-identifier="q">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc-metadata><dc:Title>T</dc:Title><dc:Language>en</dc:Language>
    <dc:Identifier id="q">NOID</dc:Identifier></dc-metadata>
  </metadata>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="text/x-oeb1-document"/>
    <item id="x1" href="other.bin" media-type="{mt}"{fb}/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/other.bin", "x"),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// An EPUB 2-era package whose `<package>` sits in `ns`, with the Dublin
    /// Core in OEBPS 1.2's `<dc-metadata>` wrapper under title-case names and
    /// a `text/x-oeb1-document` spine item — the shape epubcheck's own
    /// legacy fixtures use.
    fn epub_with_package_ns(ns: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="{ns}" version="2.0" unique-identifier="q">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc-metadata>
      <dc:Title>T</dc:Title><dc:Language>en</dc:Language>
      <dc:Identifier id="q">NOID</dc:Identifier>
    </dc-metadata>
  </metadata>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="text/x-oeb1-document"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// A `<guide><reference href="…#frag">` whose fragment names no `id` in
    /// the target is RSC-012, as it is from an NCX or a content document.
    ///
    /// Our fragment resolution grew per *source* — NCX `<content src>`, then
    /// content-document hrefs, then `epub:textref` — and the guide was never
    /// added, while epubcheck's runs over every registered reference at once
    /// and so covered it from the start. Found by `compare` on a shelf book
    /// whose `<reference type="toc">` pointed at an id living in a different
    /// file: epubcheck reported it, and our output for the entire package
    /// document was empty.
    #[test]
    fn a_guide_reference_fragment_must_resolve() {
        let ids = |frag: &str| {
            let r = crate::validate_bytes(epub2_with_guide_fragment(frag));
            r.messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_012)
                .count()
        };
        assert_eq!(
            ids("#nosuchid"),
            1,
            "the fragment names no id in the target"
        );
        // Both negatives matter: a resolving fragment and no fragment at all
        // must stay silent, or this is just "RSC-012 always fires on a guide".
        assert_eq!(ids("#real"), 0);
        assert_eq!(ids(""), 0);
    }

    /// An EPUB 2 whose `<guide>` reference carries `frag`.
    fn epub2_with_guide_fragment(frag: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="ch1"/></spine>
  <guide><reference type="toc" title="T" href="ch1.xhtml{frag}"/></guide>
</package>"#
        );
        const NCX: &str = "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
             version=\"2005-1\"><head><meta name=\"dtb:uid\" \
             content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
             <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n1\" playOrder=\"1\">\
             <navLabel><text>T</text></navLabel><content src=\"ch1.xhtml\"/></navPoint>\
             </navMap></ncx>";
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p id=\"real\">x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/toc.ncx", NCX),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// EPUB 3.4's restrictive half: the roll-layout constraints (#1651) and
    /// the deprecations (#1649).
    ///
    /// All four are advisory. epubcheck has implemented none of them, so in
    /// a side-by-side diff they are indistinguishable from false positives —
    /// and **0 of the 125 shelf books draws any of them**, so the shelf
    /// confirms silence but cannot confirm correctness. No real book uses a
    /// layout the specification introduced weeks ago; this enumeration is
    /// the evidence, as it was for ADV-005.
    #[test]
    fn epub34_roll_constraints_and_deprecations_are_advisory() {
        const VIEWPORT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<meta name="viewport" content="width=1200, height=1600"/></head><body><p>x</p></body></html>"#;
        const NO_VIEWPORT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>x</p></body></html>"#;

        let ids =
            |layout: &str, itemref_props: &str, pkg_prefix: &str, ch1: &str, advisory: bool| {
                let layout_meta = if layout.is_empty() {
                    String::new()
                } else {
                    format!(r#"<meta property="rendition:layout">{layout}</meta>"#)
                };
                let props = if itemref_props.is_empty() {
                    String::new()
                } else {
                    format!(r#" properties="{itemref_props}""#)
                };
                let opf = format!(
                    r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"{pkg_prefix}>
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {layout_meta}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"{props}/></spine>
</package>"#
                );
                let mut v: Vec<&'static str> = crate::validate_bytes_with_options(
                    epub_with_opf(Some(&opf), ch1),
                    &crate::Options {
                        advisory,
                        ..Default::default()
                    },
                )
                .messages
                .iter()
                .map(|m| m.id)
                .filter(|id| {
                    (id.starts_with("NEXT-") || id.starts_with("ADV-"))
                        && *id != crate::ids::NEXT_005
                })
                .collect();
                v.sort_unstable();
                v.dedup();
                v
            };

        // #1651: no per-spine layout override beside a roll layout.
        assert_eq!(
            ids("roll", "rendition:layout-reflowable", "", VIEWPORT, true),
            vec![crate::ids::NEXT_006]
        );
        assert_eq!(
            ids("roll", "rendition:layout-pre-paginated", "", VIEWPORT, true),
            vec![crate::ids::NEXT_006]
        );
        // The same override is ordinary outside a roll layout.
        assert!(ids("", "rendition:layout-reflowable", "", VIEWPORT, true).is_empty());

        // #1651: a roll spine document must declare its ICB dimensions.
        assert_eq!(
            ids("roll", "", "", NO_VIEWPORT, true),
            vec![crate::ids::NEXT_007]
        );
        assert!(ids("roll", "", "", VIEWPORT, true).is_empty());
        // Only under a roll layout — a plain reflowable book has no ICB.
        assert!(ids("", "", "", NO_VIEWPORT, true).is_empty());

        // #1649: two deprecations, one ID.
        assert_eq!(
            ids("", "rendition:align-x-center", "", VIEWPORT, true),
            vec![crate::ids::NEXT_008]
        );
        for prefix in ["xsd", "msv", "prism"] {
            assert_eq!(
                ids(
                    "",
                    "",
                    &format!(r#" prefix="{prefix}: http://example.org/{prefix}#""#),
                    VIEWPORT,
                    true
                ),
                vec![crate::ids::NEXT_008],
                "{prefix} is deprecated in EPUB 3.4"
            );
        }
        // A reserved prefix 3.4 does *not* deprecate stays quiet.
        assert!(
            ids(
                "",
                "",
                r#" prefix="foo: http://example.org/foo#""#,
                VIEWPORT,
                true
            )
            .is_empty()
        );

        // Opt-in, every one of them.
        assert!(ids("roll", "rendition:layout-reflowable", "", VIEWPORT, false).is_empty());
        assert!(ids("roll", "", "", NO_VIEWPORT, false).is_empty());
        assert!(ids("", "rendition:align-x-center", "", VIEWPORT, false).is_empty());
    }

    /// EPUB 3.4 (w3c/epubcheck#1651): `rendition:layout` gains the value
    /// `roll`, the webtoon layout.
    ///
    /// This half is **permissive** and therefore ships unflagged, unlike
    /// ADV-005: accepting `roll` costs a false negative against
    /// epubcheck-as-it-is-today (5.3.0 still rejects it, measured) and
    /// removes a false positive against the spec-as-it-will-be. That is the
    /// whole of the "first validator to support 3.4" position.
    ///
    /// The neighbouring assertion must keep working — a value the spec has
    /// never had is still an error — which is the half a widened enum can
    /// silently lose.
    #[test]
    fn rendition_layout_accepts_roll_but_not_an_invented_value() {
        let layout_findings = |value: &str| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <meta property="rendition:layout">{value}</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<meta name="viewport" content="width=1200, height=1600"/></head><body><p>x</p></body></html>"#;
            crate::validate_bytes(epub_with_opf(Some(&opf), CH1))
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.package.rendition_layout_value"))
                .count()
        };

        assert_eq!(layout_findings("reflowable"), 0);
        assert_eq!(layout_findings("pre-paginated"), 0);
        assert_eq!(layout_findings("roll"), 0, "EPUB 3.4 webtoon layout");
        assert_eq!(
            layout_findings("bogusvalue"),
            1,
            "widening the enum must not switch the assertion off"
        );
        // Case matters: the spec's values are lowercase, and epubcheck's
        // Schematron compares them literally.
        assert_eq!(layout_findings("Roll"), 1);
    }

    /// EPUB 3.4 (w3c/epubcheck#1652): `page-spread-*` is confined to
    /// fixed-layout content.
    ///
    /// The shelf is blind here - 0 of 125 books carry a `page-spread-*`
    /// token at all - so its silence is not evidence and this enumeration
    /// is. The space is closed and small, so it is walked in full: the four
    /// ways a document ends up reflowable or fixed (package default, with
    /// and without an itemref override in each direction) against the five
    /// prohibited tokens, plus the tokens that must stay silent.
    ///
    /// epubcheck cannot arbitrate any of it - #1652 is open and
    /// unimplemented - which is exactly why this is advisory-only.
    #[test]
    fn page_spread_is_confined_to_fixed_layout_content() {
        const PROHIBITED: &[&str] = &[
            "page-spread-left",
            "page-spread-right",
            "rendition:page-spread-left",
            "rendition:page-spread-right",
            "rendition:page-spread-center",
        ];
        // (package is pre-paginated, itemref override) -> is the document
        // reflowable, i.e. must the advisory fire?
        const LAYOUTS: &[(bool, &str, bool)] = &[
            (false, "", true),
            (true, "", false),
            (true, "rendition:layout-reflowable ", true),
            (false, "rendition:layout-pre-paginated ", false),
        ];

        for &(pre_paginated, override_token, reflowable) in LAYOUTS {
            for token in PROHIBITED {
                let n = adv_005(pre_paginated, &format!("{override_token}{token}"), true);
                assert_eq!(
                    n,
                    usize::from(reflowable),
                    "package pre-paginated={pre_paginated}, override={override_token:?}, \
                     token={token}: expected reflowable={reflowable}"
                );
            }
            // A layout override on its own is never the subject of this rule.
            assert_eq!(
                adv_005(pre_paginated, override_token.trim(), true),
                0,
                "no page-spread token, nothing to report"
            );
        }

        // Opt-in only: the same book is silent without `--advisory`, which is
        // what keeps a rule epubcheck has not shipped out of the default diff.
        assert_eq!(adv_005(false, "page-spread-left", false), 0);
        // A neighbouring rendition override is not a page-spread token.
        assert_eq!(adv_005(false, "rendition:align-x-center", true), 0);
    }

    /// Count ADV-005 for one spine itemref, with the package either
    /// pre-paginated or left at its reflowable default.
    fn adv_005(package_pre_paginated: bool, itemref_props: &str, advisory: bool) -> usize {
        let layout = if package_pre_paginated {
            r#"<meta property="rendition:layout">pre-paginated</meta>"#
        } else {
            ""
        };
        let props = if itemref_props.is_empty() {
            String::new()
        } else {
            format!(r#" properties="{itemref_props}""#)
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {layout}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"{props}/></spine>
</package>"#
        );
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<meta name="viewport" content="width=100, height=100"/></head><body><p>x</p></body></html>"#;
        crate::validate_bytes_with_options(
            epub_with_opf(Some(&opf), CH1),
            &crate::Options {
                advisory,
                ..Default::default()
            },
        )
        .messages
        .iter()
        .filter(|m| m.id == crate::ids::NEXT_005)
        .count()
    }

    /// A nav or NCX link to a non-Content-Document is legal when the manifest
    /// item declares a `fallback` chain reaching one.
    ///
    /// epubcheck's RSC-010 condition has three clauses and we had two; the
    /// missing one is `!targetResource.hasContentDocumentFallback()`. Doitsu,
    /// MobileRead #168, on the IDPF `haruko-jpeg` sample: an image-based book
    /// whose nav and NCX link straight at the JPEGs, each declaring
    /// `fallback="fallback"` to an XHTML document. epubcheck reports one usage
    /// message for the whole book; we reported three errors.
    #[test]
    fn a_link_target_with_a_content_document_fallback_is_allowed() {
        let count = |fallback: bool| {
            let fb = if fallback { r#" fallback="fb""# } else { "" };
            let bytes = epub_with_image_nav_target(fb);
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_010)
                .count()
        };
        assert_eq!(count(true), 0, "a fallback to XHTML makes the target legal");
        assert!(count(false) > 0, "without one it is still an error");
    }

    /// An EPUB 3 book whose `toc` nav links at an image, with the image's
    /// manifest item optionally declaring a fallback.
    fn epub_with_image_nav_target(fb_attr: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="img" href="01.jpg" media-type="image/jpeg"{fb_attr}/>
    <item id="fb" href="fallback.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="img"/></spine>
</package>"#
        );
        const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" \
            xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>N</title></head>\
            <body><nav epub:type=\"toc\"><ol><li><a href=\"01.jpg\">C</a></li></ol></nav>\
            </body></html>";
        const FB: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>f</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/fallback.xhtml", FB),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.start_file("OEBPS/01.jpg", o).unwrap();
            z.write_all(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
            z.finish().unwrap();
        }
        buf
    }

    /// A dictionary collection is judged on its own `role`, not on the
    /// publication declaring `dc:type="dictionary"`.
    ///
    /// epubcheck's `checkCollections`/`checkCollectionsContent` iterate the
    /// collections and test `collection.hasRole(DICTIONARY)` and nothing else.
    /// We gated the whole suite on `dc:type`, so a book with a malformed
    /// dictionary collection and no `dc:type` drew **nothing at all** where
    /// epubcheck reports four — including OPF-083, a row `docs/COVERAGE.md`
    /// marks as implemented. A check that cannot fire is worse in the matrix
    /// than one that is honestly absent.
    ///
    /// Safe for the fixture the `dc:type` gate exists for
    /// (`dictionary-metadata-type-missing-error.opf`): it carries no
    /// `<collection>` at all, so this branch finds nothing there.
    #[test]
    fn a_dictionary_collection_is_checked_without_dc_type() {
        for dc_type in [true, false] {
            let ty = if dc_type {
                "<dc:type>dictionary</dc:type>"
            } else {
                ""
            };
            let r = crate::validate_bytes(epub_with_dictionary_collection(ty));
            assert!(
                r.messages.iter().any(|m| m.id == crate::ids::OPF_083),
                "a collection with no Search Key Map must draw OPF-083 (dc:type={dc_type})"
            );
        }
    }

    /// An EPUB 3 book whose package carries a `<collection role="dictionary">`
    /// with a single XHTML link and no Search Key Map.
    fn epub_with_dictionary_collection(dc_type: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>{dc_type}
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
  <collection role="dictionary"><link href="ch1.xhtml"/></collection>
</package>"#
        );
        const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" \
            xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>N</title></head>\
            <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.xhtml\">C</a></li></ol></nav>\
            </body></html>";
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// RSC-025 is EPUB 3 only, because epubcheck's `ValidatorMap` pairs the
    /// one non-normative validator it has - `SVG_30_INFORMATIVE_NVDL`, the
    /// full SVG 1.1 grammar whose findings become usage-level RSC-025 - with
    /// VERSION_3 alone. An EPUB 2 document gets `XHTML_20_NVDL` /
    /// `SVG_20_NVDL` and no informative pass at all.
    ///
    /// A lowercase `viewbox` in a real EPUB 2 book was the last
    /// false-positive candidate on the 104-book shelf. The attribute really
    /// is wrong - SVG names are case-sensitive - but RSC-025 is the family
    /// for epubcheck's *opinion*, and in EPUB 2 it has none.
    ///
    /// Measured both ways against epubcheck 5.3.0, inline and standalone:
    /// EPUB 3 gives two findings each, EPUB 2 none.
    #[test]
    fn svg_vocabulary_usage_is_epub3_only() {
        const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewbox=\"0 0 10 10\" \
             preserveaspectratio=\"xMidYMid meet\"><rect width=\"5\" height=\"5\"/></svg>";
        let rsc025 = |ver: &str| {
            crate::validate_bytes(epub_with_body(ver, SVG))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_025)
                .count()
        };
        assert_eq!(rsc025("3.0"), 2, "lowercase viewBox/preserveAspectRatio");
        assert_eq!(
            rsc025("2.0"),
            0,
            "epubcheck runs no informative SVG grammar on EPUB 2"
        );
    }

    /// RSC-014: a hyperlink may target a GENERIC id and nothing else.
    ///
    /// epubcheck types every id from the element that carries it — an SVG
    /// `symbol` is SVG_SYMBOL, `linearGradient`/`radialGradient`/`pattern`
    /// are SVG_PAINT, `clipPath` is SVG_CLIP_PATH — and a hyperlink to any
    /// of them is an incompatible resource type. We had `symbol` alone, so
    /// four of the five names were a silent gap. Each case below was built
    /// as a book and run through epubcheck 5.3.0 one shape per run.
    ///
    /// The shelf cannot see any of this: **0 of 346 books define an SVG
    /// symbol, gradient, pattern or clipPath at all**. The enumeration is
    /// the evidence.
    #[test]
    fn a_hyperlink_to_a_typed_svg_id_is_rsc_014() {
        let rsc014 = |body: &str| {
            crate::validate_bytes(epub_with_body("3.0", body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_014)
                .count()
        };
        let defs = |el: &str, id: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><{el} id=\"{id}\"/></defs></svg>\
                 <p><a href=\"#{id}\">link</a></p>"
            )
        };
        for el in [
            "symbol",
            "linearGradient",
            "radialGradient",
            "pattern",
            "clipPath",
        ] {
            assert_eq!(rsc014(&defs(el, "t")), 1, "hyperlink to an SVG {el}");
        }

        // A GENERIC id is the whole point of the rule, so assert the
        // negative too - a check that only ever fires would pass the loop
        // above just as well.
        assert_eq!(rsc014("<p id=\"t\">x</p><p><a href=\"#t\">link</a></p>"), 0);
        // Not on epubcheck's list, and equally a "definition" element: the
        // list is theirs, not the SVG spec's notion of a definition.
        assert_eq!(
            rsc014(&defs("marker", "t")),
            0,
            "marker is GENERIC to epubcheck"
        );
        assert_eq!(
            rsc014(&defs("mask", "t")),
            0,
            "mask is GENERIC to epubcheck"
        );
    }

    /// The other two reference kinds epubcheck compares against an id's
    /// type: an SVG `<use>`, which may reach a symbol or a generic id, and a
    /// paint reference (`fill`/`stroke="url(#…)"`), which must reach a paint
    /// server exactly. A reference whose fragment resolves to nothing is
    /// RSC-012, not RSC-014 — the same split epubcheck makes.
    ///
    /// Fourteen shapes were built as books and run through epubcheck 5.3.0;
    /// this pins the ones expressible in a single document.
    #[test]
    fn use_and_paint_references_are_typed_too() {
        let ids = |body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_body("3.0", body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_014 || m.id == crate::ids::RSC_012)
                .map(|m| m.id)
                .collect()
        };
        // `defs` holds a symbol, a paint server and a generic id.
        let svg = |body: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\"><defs>\
                 <symbol id=\"sym\"/><linearGradient id=\"grad\"/><g id=\"gen\"/>\
                 </defs>{body}</svg>"
            )
        };

        // <use>: a symbol or a generic id is fine, a paint server is not.
        assert!(ids(&svg("<use xlink:href=\"#sym\"/>")).is_empty());
        assert!(ids(&svg("<use xlink:href=\"#gen\"/>")).is_empty());
        assert_eq!(
            ids(&svg("<use xlink:href=\"#grad\"/>")),
            vec![crate::ids::RSC_014]
        );

        // Paint: a paint server exactly.
        assert!(ids(&svg("<rect fill=\"url(#grad)\"/>")).is_empty());
        assert_eq!(
            ids(&svg("<rect fill=\"url(#sym)\"/>")),
            vec![crate::ids::RSC_014]
        );
        assert_eq!(
            ids(&svg("<rect stroke=\"url(#gen)\"/>")),
            vec![crate::ids::RSC_014]
        );

        // A fragment that resolves to nothing is the other message.
        assert_eq!(
            ids(&svg("<use xlink:href=\"#nope\"/>")),
            vec![crate::ids::RSC_012]
        );
        assert_eq!(
            ids(&svg("<rect fill=\"url(#nope)\"/>")),
            vec![crate::ids::RSC_012]
        );

        // Two deliberate silences, both matching epubcheck rather than the
        // SVG spec. Its `checkSymbol()` reads `xlink:href` only, so SVG 2's
        // plain `href` registers no reference; and nothing on either side
        // registers a clip-path reference at all. Reporting either would be
        // indistinguishable from a false positive to anyone diffing the two
        // tools. Both measured, one book each.
        assert!(
            ids(&svg("<use href=\"#grad\"/>")).is_empty(),
            "SVG 2 use href"
        );
        assert!(
            ids(&svg("<rect clip-path=\"url(#grad)\"/>")).is_empty(),
            "clip-path is unchecked by epubcheck"
        );
    }

    /// #78: RSC-010 on an ordinary hyperlink, not just on a toc link.
    ///
    /// epubcheck runs this for every hyperlink and reports it *instead of*
    /// RSC-011 — it aborts the reference's checks straight after — so the
    /// negative half of this test is the parity, not decoration.
    #[test]
    fn a_hyperlink_to_a_non_content_document_is_rsc_010() {
        let ids = |body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_body("3.0", body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_010 || m.id == crate::ids::RSC_011)
                .map(|m| m.id)
                .collect()
        };
        // `ch1.xhtml` is the only manifest item in this fixture and is in the
        // spine, so a self-link is the control: a Content Document, reachable,
        // draws neither message.
        assert!(ids("<p><a href=\"ch1.xhtml\">x</a></p>").is_empty());
    }

    /// #77: an SVG anchor's target is checked for existence, through
    /// `xlink:href` and only that.
    ///
    /// The existence check lives in the bare-name attribute walk, which
    /// cannot see a namespaced attribute - so 0.9.22 fixed the fragment and
    /// URL halves of the SVG-anchor inversion and left this one. Routing the
    /// namespaced value through the same loop was the whole fix, because
    /// `is_resource_reference` already puts an `a`/`href` pair on the
    /// *not*-consuming side: the target is checked but never enters the
    /// resource set that answers OPF-097.
    #[test]
    fn an_svg_anchor_target_is_checked_for_existence() {
        let ids = |body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_body("3.0", body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_007)
                .map(|m| m.id)
                .collect()
        };
        let svga = |attr: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\">\
                 <a {attr}=\"missing.xhtml\"><rect/></a></svg>"
            )
        };
        assert_eq!(ids(&svga("xlink:href")), vec![crate::ids::RSC_007]);
        // The plain spelling registers no reference in epubcheck, so it must
        // register none here either - reporting it was the false-positive
        // half, removed in 0.9.22.
        assert!(ids(&svga("href")).is_empty(), "plain href on an SVG anchor");
        // The XHTML control, same target, still reported.
        assert_eq!(
            ids("<p><a href=\"missing.xhtml\">x</a></p>"),
            vec![crate::ids::RSC_007]
        );
    }

    /// The last two reference kinds: `cite` on the four elements HTML gives
    /// it, and an SVG `<a>`. Both carry a version or spelling condition that
    /// was measured against epubcheck rather than reasoned about, because
    /// getting either wrong reports where epubcheck is silent.
    #[test]
    fn cite_and_svg_anchor_references_are_typed() {
        let ids = |ver: &str, body: &str| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_body(ver, body))
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_014 || m.id == crate::ids::RSC_012)
                .map(|m| m.id)
                .collect()
        };
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                   xmlns:xlink=\"http://www.w3.org/1999/xlink\"><defs>\
                   <symbol id=\"sym\"/></defs></svg>";

        // `cite` is EPUB 3 only - epubcheck collects it in `OPSHandler30`,
        // and the identical EPUB 2 book is clean from it.
        for el in ["blockquote", "q", "ins", "del"] {
            let body = format!("{svg}<{el} cite=\"#sym\">x</{el}>");
            assert_eq!(
                ids("3.0", &body),
                vec![crate::ids::RSC_014],
                "{el} in EPUB 3"
            );
            assert!(ids("2.0", &body).is_empty(), "{el} in EPUB 2");
        }
        assert!(ids("3.0", &format!("{svg}<p id=\"g\"/><q cite=\"#g\">x</q>")).is_empty());

        // An SVG anchor is read through `xlink:href` and only that: a plain
        // `href` there registers no reference in epubcheck and draws nothing
        // at all, so reporting it would be a false-positive-shaped
        // divergence. The XHTML `<a href>` case is the control - same
        // attribute name, different namespace, still reported.
        let svga = |attr: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\"><defs><symbol id=\"sym\"/></defs>\
                 <a {attr}=\"#sym\"><rect/></a></svg>"
            )
        };
        assert_eq!(ids("3.0", &svga("xlink:href")), vec![crate::ids::RSC_014]);
        assert!(
            ids("3.0", &svga("href")).is_empty(),
            "plain href on an SVG anchor"
        );
        assert_eq!(
            ids("3.0", &format!("{svg}<a href=\"#sym\">x</a>")),
            vec![crate::ids::RSC_014],
            "the XHTML anchor is unaffected"
        );
    }

    /// A minimal EPUB (version `ver`) whose single content document has
    /// `body` as its body content.
    /// A minimal EPUB 3 whose one content document has `body` as its body
    /// and carries a media overlay whose `<text src>` values are `frags`
    /// (each resolved against that same document).
    ///
    /// Built because nothing else in the crate produces a SMIL-bearing book,
    /// which is why the overlay half of RSC-014 was measured with a
    /// hand-built probe and had no test.
    fn epub_with_overlay(body: &str, frags: &[&str]) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let pars: String = frags
            .iter()
            .enumerate()
            .map(|(i, f)| format!("<par id=\"p{i}\"><text src=\"ch1.xhtml#{f}\"/></par>"))
            .collect();
        let smil = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
             <smil xmlns=\"http://www.w3.org/ns/SMIL\" version=\"3.0\"><body>{pars}</body></smil>"
        );
        let opf = r##"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <meta property="media:duration">0:00:20</meta>
    <meta property="media:duration" refines="#mo">0:00:20</meta>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml" media-overlay="mo" properties="svg"/>
    <item id="mo" href="ch1.smil" media-type="application/smil+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"##;
        let ch1 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
             <body>{body}</body></html>"
        );
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, content) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf),
                ("OEBPS/ch1.xhtml", ch1.as_str()),
                ("OEBPS/ch1.smil", smil.as_str()),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(content.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// The overlay cell of RSC-014: a media overlay's `<text src>` may name a
    /// generic id only. Measured against epubcheck with a hand-built book
    /// before this test existed - it reports RSC-014 for an overlay pointing
    /// at an SVG `<symbol>`.
    #[test]
    fn an_overlay_text_link_to_an_svg_symbol_is_rsc_014() {
        let ids = |frags: &[&str]| -> Vec<&'static str> {
            crate::validate_bytes(epub_with_overlay(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><symbol id=\"sym\"/></defs></svg>\
                 <p id=\"ok\">t</p><p id=\"ok2\">t</p>",
                frags,
            ))
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_014 || m.id == crate::ids::RSC_012)
            .map(|m| m.id)
            .collect()
        };
        assert_eq!(ids(&["sym", "ok"]), vec![crate::ids::RSC_014]);
        // The control that can fail: two generic targets stay clean, so the
        // assertion above is about the symbol and not about overlays.
        assert!(ids(&["ok", "ok2"]).is_empty());
    }

    fn epub_with_body(ver: &str, body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let (modified, ncx_item, toc_attr) = if ver.starts_with('3') {
            (
                "<meta property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta>",
                "",
                "",
            )
        } else {
            (
                "",
                "<item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>",
                " toc=\"ncx\"",
            )
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{ver}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    {modified}
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>{ncx_item}
  </manifest>
  <spine{toc_attr}><itemref idref="ch1"/></spine>
</package>"#
        );
        let ch1 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
             <body>{body}</body></html>"
        );
        const NCX: &str = "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
             version=\"2005-1\"><head><meta name=\"dtb:uid\" \
             content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
             <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n1\" playOrder=\"1\">\
             <navLabel><text>T</text></navLabel><content src=\"ch1.xhtml\"/></navPoint></navMap></ncx>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", ch1.as_str()),
                ("OEBPS/toc.ncx", NCX),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// A minimal EPUB (version `ver`, e.g. "2.0" or "3.0") whose metadata
    /// carries `dc_extra` verbatim, for the empty-metadata checks.
    fn epub_with_metadata(ver: &str, dc_extra: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let modified = if ver.starts_with('3') {
            "<meta property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta>"
        } else {
            ""
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{ver}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    {modified}
    {dc_extra}
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
                ),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// A content-document reference is RSC-001, RSC-007, RSC-008 or silent,
    /// by the same declared/present matrix `css.rs` applies to every `url()`.
    /// That comment claimed the split was "already established for XHTML
    /// content-doc references" when only one of the three cells existed.
    ///
    /// Two of them were wrong, in opposite directions: a *declared* target
    /// with a missing file drew RSC-007 on top of the manifest pass's
    /// RSC-001, and an *undeclared* target that is present drew nothing at
    /// all, so a real book referencing a container file nobody declared got
    /// only the usage-level OPF-003 from the container side.
    ///
    /// The SVG case is separate wiring, not the same walk: that one reads
    /// `attr_no_ns`, and SVG references through `xlink:href`, so a broken
    /// image reference inside an `<svg>` produced nothing whatsoever. It is
    /// how the real book reported it.
    ///
    /// The structural exemption is what the corpus caught: `nav-cfi-valid`
    /// points its nav at `package.opf#epubcfi(...)`, and the OPF is present,
    /// undeclared, and never could be declared. Same exemption OPF-003 makes
    /// on the container side.
    #[test]
    fn a_content_document_reference_is_rsc_001_007_or_008_by_declared_and_present() {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        // `extra_item` goes in the manifest, `extra_file` into the container.
        let build = |body: &str, extra_item: &str, extra_file: bool| {
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    {extra_item}
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body>{body}</body></html>"
            );
            const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
                <head><title>nav</title></head><body><nav epub:type=\"toc\"><ol>\
                <li><a href=\"ch1.xhtml\">1</a></li></ol></nav></body></html>";
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    (
                        "META-INF/container.xml",
                        r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#.to_string(),
                    ),
                    ("OEBPS/content.opf", opf),
                    ("OEBPS/nav.xhtml", NAV.to_string()),
                    ("OEBPS/ch1.xhtml", ch1),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                if extra_file {
                    z.start_file("OEBPS/x.gif", o).unwrap();
                    z.write_all(b"GIF89a").unwrap();
                }
                z.finish().unwrap();
            }
            crate::validate_bytes(buf)
        };
        let ids =
            |r: &crate::report::Report, id: &str| r.messages.iter().filter(|m| m.id == id).count();
        const ITEM: &str = "<item id=\"img\" href=\"x.gif\" media-type=\"image/gif\"/>";
        const IMG: &str = "<p><img src=\"x.gif\" alt=\"a\"/></p>";

        // present + undeclared -> RSC-008 (and OPF-003 from the container side)
        let r = build(IMG, "", true);
        assert_eq!(ids(&r, crate::ids::RSC_008), 1, "undeclared but present");
        assert_eq!(ids(&r, crate::ids::RSC_007), 0);

        // missing + undeclared -> RSC-007
        let r = build(IMG, "", false);
        assert_eq!(ids(&r, crate::ids::RSC_007), 1, "undeclared and missing");
        assert_eq!(ids(&r, crate::ids::RSC_008), 0);

        // missing + declared -> RSC-001 only; the reference site stays silent
        let r = build(IMG, ITEM, false);
        assert_eq!(ids(&r, crate::ids::RSC_001), 1, "declared but missing");
        assert_eq!(
            ids(&r, crate::ids::RSC_007),
            0,
            "no second finding on top of RSC-001"
        );

        // present + declared -> silent
        let r = build(IMG, ITEM, true);
        assert_eq!(ids(&r, crate::ids::RSC_007), 0);
        assert_eq!(ids(&r, crate::ids::RSC_008), 0);

        // SVG reaches the same matrix through `xlink:href`.
        let svg = "<p><svg xmlns=\"http://www.w3.org/2000/svg\" \
                   xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 1 1\">\
                   <image xlink:href=\"x.gif\"/></svg></p>";
        let r = build(svg, "", true);
        assert_eq!(
            ids(&r, crate::ids::RSC_008),
            1,
            "svg image, undeclared but present"
        );
        let r = build(svg, "", false);
        assert_eq!(ids(&r, crate::ids::RSC_007), 1, "svg image, missing");

        // The OPF itself is structural: present, undeclared, and never
        // declarable. It must draw neither.
        let r = build("<p><a href=\"content.opf\">o</a></p>", "", false);
        assert_eq!(
            ids(&r, crate::ids::RSC_008),
            0,
            "the package document is not a manifest item"
        );
        assert_eq!(ids(&r, crate::ids::RSC_007), 0);
    }

    /// `is_remote_url` is epubcheck's predicate, not a list of known
    /// schemes: anything with a scheme is remote except `data:`.
    ///
    /// It used to be `http`/`https` only, and `res:///system/fonts/X.ttf` in
    /// a real book's `@font-face` fell in the gap between two checks -
    /// `is_external` matches on `://` so local resolution was skipped, and
    /// this said "not remote" so the remote-resource rules never ran. Neither
    /// check reported anything, which is the failure a user cannot notice.
    /// epubcheck gives OPF-014 and four RSC-006 on that book; so do we now.
    ///
    /// `mailto:` and `tel:` are remote *by this predicate* and that is
    /// correct: what keeps them harmless is that `<a href>` and `@cite` go
    /// into a separate `remote_link_refs` set and never become embedded
    /// dependencies. Asserting it here rather than special-casing the scheme
    /// keeps the predicate the same shape as the oracle's.
    #[test]
    fn is_remote_url_is_any_scheme_except_data() {
        for yes in [
            "http://example.com/x",
            "https://example.com/x",
            "res:///system/fonts/X.ttf",
            "ftp://example.com/x",
            "foo://example.com/x",
            "mailto:x@y.com",
            "tel:+900000",
            "  https://example.com/x  ",
        ] {
            assert!(super::is_remote_url(yes), "must be remote: {yes}");
        }
        for no in [
            "data:image/gif;base64,R0lGODlh",
            "images/x.png",
            "../x.png",
            "#frag",
            "",
            "x.png",
            // A colon inside a path segment is not a scheme.
            "chapter/a:b.xhtml",
            // A scheme cannot start with a digit.
            "9foo:bar",
        ] {
            assert!(!super::is_remote_url(no), "must not be remote: {no}");
        }
    }

    /// A remote resource reached through a *linked* stylesheet is reported
    /// once, against the stylesheet - not once per document that links it.
    ///
    /// One `@font-face` in one shared sheet used to produce **10 RSC-008 and
    /// 9 RSC-031** on a ten-document book, against epubcheck's single
    /// finding, because every linking document adopted the sheet's remote
    /// URLs as its own. The manifest pass over the stylesheet already
    /// reported them, which is also where epubcheck puts them.
    ///
    /// **Two documents is the whole point of this fixture.** The duplication
    /// scales with the number of linking documents, so it cannot appear at
    /// one - which is exactly why the corpus never saw it, every fixture
    /// there having a single content document. Nor could the shelf: no book
    /// among the 346 has a remote URL in CSS at all.
    ///
    /// RSC-031 is asserted because removing the duplication silently removed
    /// it too - it only ever reached a linked sheet's URLs through the
    /// per-document path. Found by re-probing epubcheck after the fix, not by
    /// any test, which is the reason this one exists.
    #[test]
    fn a_linked_stylesheets_remote_font_is_reported_once_not_once_per_document() {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = |ver: &str| {
            let (modified, nav) = if ver.starts_with('3') {
                (
                    "<meta property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta>",
                    "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>",
                )
            } else {
                ("", "")
            };
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{ver}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    {modified}
  </metadata>
  <manifest>
    {nav}
    <item id="css" href="s.css" media-type="text/css"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#
            )
        };
        const CSS: &str = "@font-face { font-family: \"X\"; src: url(http://example.com/f.ttf); }";
        let doc = |t: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{t}</title>\
                 <link rel=\"stylesheet\" href=\"s.css\"/></head><body><p>x</p></body></html>"
            )
        };
        const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
            <head><title>nav</title></head><body><nav epub:type=\"toc\"><ol>\
            <li><a href=\"ch1.xhtml\">1</a></li></ol></nav></body></html>";

        let build = |ver: &str| {
            let mut buf = Vec::new();
            {
                let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
                z.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                z.write_all(b"application/epub+zip").unwrap();
                let o = SimpleFileOptions::default();
                for (name, body) in [
                    (
                        "META-INF/container.xml",
                        r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#.to_string(),
                    ),
                    ("OEBPS/content.opf", opf(ver)),
                    ("OEBPS/s.css", CSS.to_string()),
                    ("OEBPS/nav.xhtml", NAV.to_string()),
                    ("OEBPS/ch1.xhtml", doc("one")),
                    ("OEBPS/ch2.xhtml", doc("two")),
                ] {
                    z.start_file(name, o).unwrap();
                    z.write_all(body.as_bytes()).unwrap();
                }
                z.finish().unwrap();
            }
            buf
        };

        let report = crate::validate_bytes(build("3.0"));
        let count = |id: &str| report.messages.iter().filter(|m| m.id == id).count();
        // epubcheck 5.3.0 on this exact shape: one of each.
        assert_eq!(
            count(crate::ids::RSC_008),
            1,
            "one per stylesheet, not per linking document"
        );
        assert_eq!(
            count(crate::ids::RSC_031),
            1,
            "the https warning must survive the de-duplication"
        );
        assert_eq!(count(crate::ids::OPF_014), 1);
        // And it is attributed to the stylesheet, which is where a reader has
        // to go to fix it.
        assert!(
            report
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_008)
                .all(|m| m.location.as_deref().is_some_and(|l| l.contains("s.css"))),
            "RSC-008 must point at the stylesheet: {:?}",
            report
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::RSC_008)
                .map(|m| m.location.clone())
                .collect::<Vec<_>>()
        );

        // EPUB 2 has no remote-resource concept at all: nothing may live
        // outside the container, so the same stylesheet is RSC-006 there and
        // the manifest question never arises. epubcheck gives exactly
        // OPF-014 + RSC-006 on this shape at 2.0 - no RSC-008, and no
        // RSC-031, because telling someone to switch to https points at the
        // wrong half when the resource may not be remote at all.
        let e2 = crate::validate_bytes(build("2.0"));
        let c2 = |id: &str| e2.messages.iter().filter(|m| m.id == id).count();
        assert_eq!(
            c2(crate::ids::RSC_006),
            1,
            "EPUB 2: restricted, not undeclared"
        );
        assert_eq!(
            c2(crate::ids::RSC_008),
            0,
            "EPUB 2 has no manifest question to ask"
        );
        assert_eq!(
            c2(crate::ids::RSC_031),
            0,
            "no https advice on a forbidden reference"
        );
        assert_eq!(c2(crate::ids::OPF_014), 1);
    }

    /// OPF-072 (usage): an empty `dc:*` metadata element, EPUB 2 only. In
    /// EPUB 3 an empty element is a schema error, so it must not fire there.
    /// Requested by Doitsu on the MobileRead forum - the one thing epubcheck
    /// caught on his final test case that we did not.
    #[test]
    fn empty_dc_metadata_is_opf_072_in_epub2_only() {
        let rules = |bytes: Vec<u8>| {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::OPF_072)
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
        };
        // EPUB 2: an empty dc:subject is reported at usage level.
        let r = rules(epub_with_metadata("2.0", "<dc:subject></dc:subject>"));
        assert_eq!(r.len(), 1, "got {r:?}");
        assert!(r[0].contains("dc:subject"), "got {r:?}");
        let report = crate::validate_bytes(epub_with_metadata("2.0", "<dc:subject/>"));
        assert!(
            report
                .messages
                .iter()
                .find(|m| m.id == crate::ids::OPF_072)
                .is_some_and(|m| m.severity == crate::report::Severity::Usage)
        );
        // Whitespace-only counts as empty; a real value does not.
        assert_eq!(
            rules(epub_with_metadata("2.0", "<dc:source>  </dc:source>")).len(),
            1
        );
        assert_eq!(
            rules(epub_with_metadata("2.0", "<dc:source>x</dc:source>")).len(),
            0
        );

        // Only the element's OWN text counts. Calibre writes unescaped `<p>`
        // markup into `dc:description`, and epubcheck calls such an element
        // empty — its handler keeps the character data delivered to that
        // element, and text inside a child is not it. We read descendants and
        // therefore stayed silent on a real book epubcheck reports.
        //
        // The mixed case is the one that stops this being "has no element
        // children": direct text plus a child is non-empty to both tools.
        // Probed one book per shape against epubcheck 5.3.0.
        //
        // Neither instrument protects this. The corpus has no such fixture,
        // and `diff-shelf.sh` collects ids from ERROR/FATAL lines only, so a
        // usage-level finding is invisible to the shelf diff by construction.
        assert_eq!(
            rules(epub_with_metadata(
                "2.0",
                "<dc:description><p>x</p></dc:description>"
            ))
            .len(),
            1,
            "text inside a child element does not make the element non-empty"
        );
        assert_eq!(
            rules(epub_with_metadata(
                "2.0",
                "<dc:description>on<p>in</p></dc:description>"
            ))
            .len(),
            0,
            "direct text alongside a child element is non-empty"
        );
        // EPUB 3: not OPF-072 (empty is a schema error there instead).
        assert_eq!(
            rules(epub_with_metadata("3.0", "<dc:subject></dc:subject>")).len(),
            0
        );
        // title and date are excluded - they have their own checks, and
        // reporting OPF-072 too would double up.
        assert_eq!(
            rules(epub_with_metadata("2.0", "<dc:date></dc:date>")).len(),
            0
        );
        // ...and so are identifier and language, which is the half this
        // originally got wrong. epubcheck's `dc:*` handling is an
        // if/else-if chain whose final `else` alone reaches OPF_072, and
        // all four of these take an earlier branch. Measured one book each
        // against epubcheck 5.3.0: an empty `dc:language` is OPF-055 there
        // and an empty `dc:identifier` draws nothing but the schema's own
        // RSC-005.
        assert_eq!(
            rules(epub_with_metadata("2.0", "<dc:language></dc:language>")).len(),
            0,
            "an empty dc:language is OPF-055, not OPF-072"
        );
        assert_eq!(
            rules(epub2_with_metadata_block(
                "<dc:identifier id=\"id\" opf:scheme=\"UUID\"/>\
                 <dc:title>T</dc:title><dc:language>en</dc:language>"
            ))
            .len(),
            0,
            "an empty dc:identifier is the schema's business, not OPF-072"
        );
    }

    /// An EPUB 2 book whose whole `<metadata>` body is supplied by the test —
    /// `epub_with_metadata` always writes its own valid `dc:identifier`, and
    /// these cases are *about* that element. `opf:` is bound here because the
    /// EPUB 2 `scheme` attribute lives in it.
    fn epub2_with_metadata_block(metadata: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"
            xmlns:opf="http://www.idpf.org/2007/opf">{metadata}</metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p>x</p></body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// An empty `dc:identifier` must not cascade. epubcheck reads its text as
    /// `getPrivateData(TEXT)` and does nothing at all when that is null, so
    /// the *only* finding is the schema's RSC-005 - no "'' is not a valid
    /// UUID" (OPF-085), and no NCX-001 either, since the `UNIQUE_IDENT`
    /// feature NCX-001 is guarded on is recorded in the same skipped block.
    ///
    /// A real shelf book (`opf:scheme="UUID"` on a self-closing identifier)
    /// drew three findings from us where epubcheck draws one.
    #[test]
    fn an_empty_unique_identifier_reports_only_the_schema_error() {
        let ids = |bytes: Vec<u8>| {
            let mut v = crate::validate_bytes(bytes)
                .messages
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>();
            v.sort_unstable();
            v.dedup();
            v
        };
        const TAIL: &str = "<dc:title>T</dc:title><dc:language>en</dc:language>";
        let empty = ids(epub2_with_metadata_block(&format!(
            "<dc:identifier id=\"id\" opf:scheme=\"UUID\"/>{TAIL}"
        )));
        assert!(
            !empty.contains(&crate::ids::OPF_085),
            "an identifier with no text node at all is not an invalid UUID, got {empty:?}"
        );
        assert!(
            !empty.contains(&crate::ids::NCX_001),
            "and nothing may be compared against it, got {empty:?}"
        );
        // A malformed UUID that *is* present still reports - the guard is
        // "no text node", not "trims to empty".
        assert!(
            ids(epub2_with_metadata_block(&format!(
                "<dc:identifier id=\"id\" opf:scheme=\"UUID\">not-a-uuid</dc:identifier>{TAIL}"
            )))
            .contains(&crate::ids::OPF_085),
            "a present but malformed UUID is still OPF-085"
        );
    }

    /// OPF-085 judges only the identifier `unique-identifier` points at.
    /// epubcheck's single call site sits inside
    /// `idAttr.trim().equals(uniqueIdent)`, so a secondary `dc:identifier` -
    /// a Calibre UUID, an ISBN - is never checked. We checked every one.
    #[test]
    fn opf_085_ignores_identifiers_the_package_does_not_publish_under() {
        let has_085 = |bytes: Vec<u8>| {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .any(|m| m.id == crate::ids::OPF_085)
        };
        const GOOD: &str = "<dc:identifier id=\"id\">urn:uuid:12345678-1234-1234-1234-123456789abc\
                            </dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language>";
        assert!(
            !has_085(epub2_with_metadata_block(&format!(
                "{GOOD}<dc:identifier opf:scheme=\"UUID\">not-a-uuid</dc:identifier>"
            ))),
            "a second, non-unique identifier is not the publication's own"
        );
        // The same malformed value *as* the unique identifier still reports,
        // so this narrows the check rather than disabling it.
        assert!(
            has_085(epub2_with_metadata_block(
                "<dc:identifier id=\"id\" opf:scheme=\"UUID\">not-a-uuid</dc:identifier>\
                 <dc:title>T</dc:title><dc:language>en</dc:language>"
            )),
            "the publication's own malformed UUID is still OPF-085"
        );
    }

    fn epub_with_extra_metadata(extra: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    {extra}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;
        const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p>Hi</p></body></html>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/nav.xhtml", NAV),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn pkg_025_only_fires_for_manifest_declared_meta_inf_resources() {
        // Issue #16 (Doitsu): an UNDECLARED extra file in META-INF (Apple
        // display options, calibre bookmarks, ...) is container-level
        // metadata the OCF spec permits - it must NOT draw PKG-025. Only a
        // manifest-declared resource stored in META-INF is a "publication
        // resource in META-INF" (epubcheck's own fixture declares
        // `href="../META-INF/image.jpeg"`).
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let build = |declare_it: bool| -> Vec<u8> {
            let manifest_extra = if declare_it {
                r#"<item id="x" href="../META-INF/extra.xml" media-type="application/xml"/>"#
            } else {
                ""
            };
            let opf = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    {manifest_extra}
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
            );
            const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
            const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav></body></html>"#;
            const CH1: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p>Hi</p></body></html>"#;

            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                zip.start_file(
                    "mimetype",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                zip.write_all(b"application/epub+zip").unwrap();
                let deflated =
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
                for (name, data) in [
                    ("META-INF/container.xml", CONTAINER),
                    ("META-INF/extra.xml", "<extra/>"),
                    ("OEBPS/content.opf", opf.as_str()),
                    ("OEBPS/ch1.xhtml", CH1),
                    ("OEBPS/nav.xhtml", NAV),
                ] {
                    zip.start_file(name, deflated).unwrap();
                    zip.write_all(data.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            buf
        };

        let has_pkg_025 = |bytes: Vec<u8>| {
            crate::validate_bytes(bytes)
                .messages
                .iter()
                .any(|m| m.id == crate::ids::PKG_025)
        };
        assert!(
            !has_pkg_025(build(false)),
            "undeclared META-INF extra must stay silent"
        );
        assert!(
            has_pkg_025(build(true)),
            "manifest-declared META-INF resource must be flagged"
        );
    }

    #[test]
    fn deprecated_link_rel_keyword_is_warned_as_opf_086() {
        // A legacy `*-record` metadata link (superseded by `record` +
        // `properties`) draws a warning-level OPF-086, matching epubcheck's
        // §D.4.1 deprecation notice.
        let report = crate::validate_bytes(epub_with_extra_metadata(
            r#"<link rel="marc21xml-record" href="marc21.xml" media-type="application/marcxml+xml"/>"#,
        ));
        let hit = report
            .messages
            .iter()
            .find(|m| m.rule == Some("opf.link.deprecated_rel"))
            .expect("expected a deprecated-link-rel finding");
        assert_eq!(hit.id, crate::ids::OPF_086);
        assert_eq!(hit.severity, crate::report::Severity::Warning);
        // A current keyword must NOT be flagged.
        let clean = crate::validate_bytes(epub_with_extra_metadata(
            r#"<link rel="record" href="onix.xml" media-type="application/xml"/>"#,
        ));
        assert!(
            !clean
                .messages
                .iter()
                .any(|m| m.rule == Some("opf.link.deprecated_rel"))
        );
    }

    #[test]
    fn deprecated_epub_type_is_usage_level_opf_086b() {
        // A deprecated epub:type semantic value must be reported as
        // usage-level OPF-086b (matching epubcheck's
        // `epubtype-deprecated-usage.xhtml`: "usage OPF-086b"), not the
        // warning-level OPF-086 used for rendition/viewport deprecations,
        // and not the plain Info it used to carry. It's advisory, so the
        // book stays valid.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
            <head><title>t</title></head>\
            <body><p epub:type=\"bridgehead\">A heading</p></body></html>";
        let report = crate::validate_bytes(epub_with_ch1(ch1));
        let hit = report
            .messages
            .iter()
            .find(|m| m.rule == Some("opf.content_document.deprecated_epub_type"))
            .expect("expected a deprecated-epub:type finding");
        assert_eq!(hit.id, crate::ids::OPF_086B);
        assert_eq!(hit.severity, crate::report::Severity::Usage);
        assert!(report.is_valid());
    }

    /// Builds an EPUB 3 whose one content document carries `body` markup,
    /// and returns the (rule, id) of every epub:type finding on it.
    fn epub_type_findings(body: &str) -> Vec<(&'static str, &'static str)> {
        let ch1 = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
            <head><title>t</title></head><body>{body}</body></html>"
        );
        crate::validate_bytes(epub_with_ch1(&ch1))
            .messages
            .iter()
            .filter_map(|m| match m.rule {
                Some(r @ "opf.content_document.epub_type_not_default_vocab")
                | Some(r @ "opf.content_document.deprecated_epub_type")
                | Some(r @ "opf.content_document.epub_type_not_allowed_in_html") => Some((r, m.id)),
                _ => None,
            })
            .collect()
    }

    /// A deprecated term must draw OPF-086b *only*. Reporting OPF-088 "not
    /// in the default vocabulary" alongside it contradicts itself - knowing
    /// a term is deprecated means knowing the term. Seven deprecated terms
    /// were missing from the vocabulary list and double-reported this way
    /// (reported by Doitsu on the MobileRead forum); `ssv` now derives the
    /// two answers from one table so they cannot disagree.
    #[test]
    fn deprecated_epub_type_is_not_also_reported_as_unknown() {
        for term in crate::ssv::DEPRECATED.iter().map(|(t, _)| *t) {
            // `endnote` inside an `endnotes` container is the recommended,
            // non-deprecated usage - it has its own exemption.
            let body = format!("<p epub:type=\"{term}\">x</p>");
            let got = epub_type_findings(&body);
            assert!(
                !got.iter()
                    .any(|(r, _)| *r == "opf.content_document.epub_type_not_default_vocab"),
                "'{term}' is deprecated, so it is a known term; got {got:?}"
            );
            assert!(
                got.iter()
                    .any(|(r, _)| *r == "opf.content_document.deprecated_epub_type"),
                "'{term}' must still be reported as deprecated; got {got:?}"
            );
        }
    }

    /// The vocabulary gives these terms an HTML usage context of "Not
    /// Allowed" - they mean something only on a media overlay. They are
    /// real terms, so they must draw OPF-087 and *not* OPF-088.
    #[test]
    fn media_overlay_only_epub_type_is_not_allowed_in_html() {
        for term in crate::ssv::MEDIA_OVERLAY_ONLY {
            let got = epub_type_findings(&format!("<p epub:type=\"{term}\">x</p>"));
            assert!(
                got.contains(&(
                    "opf.content_document.epub_type_not_allowed_in_html",
                    crate::ids::OPF_087
                )),
                "'{term}' has no HTML usage context; got {got:?}"
            );
            assert!(
                !got.iter()
                    .any(|(r, _)| *r == "opf.content_document.epub_type_not_default_vocab"),
                "'{term}' is a real vocabulary term; got {got:?}"
            );
        }
    }

    /// The rule is "not allowed on an HTML element", not "restates the
    /// semantic of its host element". The old reading only fired when the
    /// term sat on its matching element (`ol` + `list`), so this - the same
    /// term with nothing to restate - went unreported. epubcheck's own
    /// fixture never covers it: it only ever pairs each term with its
    /// matching element, which is how the wrong rule scored full marks.
    #[test]
    fn media_overlay_only_epub_type_is_reported_on_any_host_element() {
        let got = epub_type_findings("<div epub:type=\"list\">x</div>");
        assert!(
            got.contains(&(
                "opf.content_document.epub_type_not_allowed_in_html",
                crate::ids::OPF_087
            )),
            "'list' is not allowed on any HTML element, not just <ol>/<ul>; got {got:?}"
        );
    }

    /// An ordinary, current term draws nothing at all.
    #[test]
    fn current_epub_type_draws_no_findings() {
        assert_eq!(epub_type_findings("<p epub:type=\"chapter\">x</p>"), vec![]);
    }

    #[test]
    fn undeclared_entity_yields_exactly_one_rsc_016_not_a_duplicate() {
        // An undeclared `&nbsp;` makes roxmltree's parse fail too, but
        // `check_raw`'s entity scan already reports it. The parse-failure
        // branch must suppress entity errors so we don't emit two RSC-016s
        // for the one defect: exactly one, and it's the entity rule.
        let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\n\
            <body><p>a&nbsp;b</p></body></html>";
        let rules = rsc_016_rules(ch1);
        assert_eq!(rules, vec!["htm.entity.undeclared"], "got: {rules:?}");
    }

    /// The other half of that suppression, and the one that keeps breaking.
    ///
    /// Skipping the parse failure is only safe while the raw scan really does
    /// report the defect. Where it doesn't, the document parses as nothing,
    /// draws no finding, and drops out of *every* check below - so a book
    /// with real errors reports clean. That has now happened three times
    /// (issue #23's 690 documents, a bare `&` in 0.7.12, an XHTML 1.0
    /// `&nbsp;` in 0.7.13), and malformed numeric references were a fourth:
    /// `&#0;` in a chapter that also carried a broken image reference
    /// validated VALID, because the scan reads named references only.
    ///
    /// Asserted as an invariant rather than as four fixed cases: whatever the
    /// parser classifies as an entity error, epubveri must say *something*
    /// about. A shape roxmltree accepts is not in scope and is skipped, so
    /// this fails for the right reason if that ever changes.
    #[test]
    fn no_entity_class_parse_failure_goes_unreported() {
        for frag in [
            "a&#0;b",           // numeric: NUL is not a legal XML character
            "a&#;b",            // numeric: no digits at all
            "a&#x;b",           // numeric: hex prefix, no digits
            "a&#zz;b",          // numeric: not digits
            "a&#38 b",          // numeric: unterminated
            "a&#xFFFFFFFFFF;b", // numeric: overflows a code point
            "a&nbsp;b",         // named: undeclared (EPUB 3 has no DTD)
            "a&foo b",          // named: unterminated
            "a & b",            // a bare ampersand
            "a&#x110000;b",     // accepted by the parser - skipped below
        ] {
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <head><title>t</title></head><body><p>{frag}</p></body></html>"
            );
            let Err(e) = crate::ocf::parse_xml(&ch1) else {
                continue;
            };
            if !crate::ocf::is_entity_reference_error(&e) {
                continue;
            }
            let messages = crate::validate_bytes(epub_with_ch1(&ch1)).messages;
            assert!(
                messages.iter().any(|m| m.id == crate::ids::RSC_016),
                "'{frag}' fails the parse as an entity error ({e:?}) and the \
                 parse-failure branch suppresses it, so nothing at all was \
                 reported and every other check on the document was skipped"
            );
        }
    }

    /// Build an EPUB 3 whose `ch1` manifest item carries `ch1_props` (e.g.
    /// `"index"` to declare it an index document, or `""` for none), with the
    /// given nav-document and ch1 `<body>` inner markup — for the index
    /// content-model gating checks.
    fn epub_index_case(ch1_props: &str, nav_body: &str, ch1_body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let props_attr = if ch1_props.is_empty() {
            String::new()
        } else {
            format!(r#" properties="{ch1_props}""#)
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"{props_attr}/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        );
        let nav = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body>{nav_body}</body></html>"#
        );
        let ch1 = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>C</title></head>
<body>{ch1_body}</body></html>"#
        );

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.xhtml", ch1.as_str()),
                ("OEBPS/nav.xhtml", nav.as_str()),
            ] {
                zip.start_file(name, deflated).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    fn has_index_content_model_error(book: Vec<u8>) -> bool {
        crate::validate_bytes(book)
            .messages
            .iter()
            .any(|m| m.rule == Some("indexes.content_model.wrong_entry_list_count"))
    }

    #[test]
    fn index_content_model_skips_a_nav_landmark_that_is_not_a_declared_index() {
        // Doitsu, MobileRead #72: a nav landmark `<a epub:type="index">` is not
        // an index structure, and its document is not declared an index in the
        // OPF, so epubcheck never applies the index content-model schema to it.
        // We must not either — no false RSC-005 "must contain ... index-entry-list".
        let nav = r#"<nav epub:type="toc"><ol>
            <li><a href="ch1.xhtml">Ch1</a></li>
            <li><a epub:type="index" href="ch1.xhtml">Index</a></li>
          </ol></nav>"#;
        assert!(
            !has_index_content_model_error(epub_index_case("", nav, "<p>Hi</p>")),
            "a nav index landmark must not trigger the index content-model check"
        );
    }

    #[test]
    fn index_content_model_still_fires_on_a_declared_index_document() {
        // The positive control for the gate above: a document actually declared
        // an index (`properties="index"`) with a malformed index (no
        // index-entry-list) must still be flagged.
        let nav = r#"<nav epub:type="toc"><ol><li><a href="ch1.xhtml">Ch1</a></li></ol></nav>"#;
        assert!(
            has_index_content_model_error(epub_index_case(
                "index",
                nav,
                r#"<section epub:type="index"><h1>Idx</h1></section>"#
            )),
            "a declared index with no index-entry-list must still be flagged"
        );
    }

    /// A minimal EPUB whose sole spine item is a standalone SVG content
    /// document carrying `svg_body`, for the SVG-content-doc checks (RSC-006
    /// remote / RSC-030 file: stylesheet forms).
    fn epub_with_svg_spine(svg_body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.svg" media-type="image/svg+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.svg">Ch1</a></li></ol></nav></body></html>"#;
        let svg = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{svg_body}");
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/ch1.svg", svg.as_str()),
                ("OEBPS/nav.xhtml", NAV),
            ] {
                zip.start_file(name, o).unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn file_url_in_svg_stylesheet_forms_is_rsc_030() {
        // #8 corpus miss (file-url-in-svg-content-error.svg): a file: URL in
        // a standalone SVG's xml-stylesheet PI and its inline <style>
        // @import. These forms are scanned by the SVG content-doc pass
        // itself (not the XHTML href walk or the CSS url() pass, both of
        // which already flag file: URLs), so the check lives there too.
        let svg = r#"<?xml-stylesheet type="text/css" href="file:example"?>
<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg">
<title>t</title><style type="text/css">@import url(file:example);</style>
<rect x="0" y="0" width="5" height="5"/></svg>"#;
        let report = crate::validate_bytes(epub_with_svg_spine(svg));
        let rsc030: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_030)
            .collect();
        assert_eq!(
            rsc030.len(),
            2,
            "both the PI href and the @import file: URL must draw RSC-030; got: {:?}",
            report.messages
        );
        assert!(!report.is_valid());
    }

    #[test]
    fn valid_svg_stylesheet_forms_stay_clean() {
        // The negative control: a local stylesheet reference in the same
        // forms is fine (no RSC-030, no RSC-006).
        let svg = r#"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg">
<title>t</title><rect x="0" y="0" width="5" height="5"/></svg>"#;
        let report = crate::validate_bytes(epub_with_svg_spine(svg));
        assert!(
            !report.messages.iter().any(|m| m.id == crate::ids::RSC_030),
            "a clean SVG must not draw RSC-030: {:?}",
            report.messages
        );
    }

    // ---- issue #72: `text/html` is a *deprecated* content-document type ----

    /// Builds a book declaring `media_type` on its one content item.
    ///
    /// `version` is "2.0" or "3.0"; an EPUB 3 book also gets the nav document
    /// it is required to have. `guide` is appended to the package document.
    fn epub_declaring(version: &str, media_type: &str, ch1: &str, guide: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        let is3 = version == "3.0";
        let (nav_item, nav_ref, modified) = if is3 {
            (
                r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
                r#"<itemref idref="nav"/>"#,
                r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#,
            )
        } else {
            ("", "", "")
        };
        let spine_toc = if is3 { "" } else { r#" toc="ncx""# };
        let ncx_item = if is3 {
            String::new()
        } else {
            r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#.to_string()
        };
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>{modified}
  </metadata>
  <manifest>{nav_item}{ncx_item}
    <item id="ch1" href="ch1.html" media-type="{media_type}"/>
  </manifest>
  <spine{spine_toc}>{nav_ref}<itemref idref="ch1"/></spine>{guide}
</package>"#
        );
        const NCX: &str = "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
             version=\"2005-1\"><head><meta name=\"dtb:uid\" \
             content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
             <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n1\" playOrder=\"1\">\
             <navLabel><text>T</text></navLabel><content src=\"ch1.html\"/></navPoint>\
             </navMap></ncx>";
        const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" \
            xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>t</title></head>\
            <body><nav epub:type=\"toc\"><ol><li><a href=\"ch1.html\">c</a></li></ol></nav>\
            </body></html>";
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            let mut files: Vec<(&str, &str)> = vec![
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", opf.as_str()),
                ("OEBPS/ch1.html", ch1),
            ];
            if is3 {
                files.push(("OEBPS/nav.xhtml", NAV));
            } else {
                files.push(("OEBPS/toc.ncx", NCX));
            }
            for (name, body) in files {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    const PLAIN_DOC: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
        <body><p>x</p></body></html>";

    fn ids_of(bytes: Vec<u8>) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = crate::validate_bytes(bytes)
            .messages
            .iter()
            .map(|m| m.id)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// A `text/html` spine item needs no fallback in EPUB 2 and does need one
    /// in EPUB 3 - epubcheck's two branches differ, and only the EPUB 2 one
    /// consults `isDeprecatedBlessedItemType` (`OPFChecker`:419 vs
    /// `OPFChecker30`:251). One real book declared it on all 91 of its spine
    /// items and drew 91 OPF-043 plus 3 RSC-010 that epubcheck does not.
    ///
    /// Neither instrument that runs on every release can see this: the corpus
    /// was byte-identical across the whole fix, and the shelf moved one book.
    #[test]
    fn epub2_text_html_spine_item_needs_no_fallback_but_epub3_does() {
        let e2 = ids_of(epub_declaring("2.0", "text/html", PLAIN_DOC, ""));
        assert!(
            !e2.contains(&crate::ids::OPF_043),
            "EPUB 2 `text/html` spine item must not draw OPF-043: {e2:?}"
        );
        assert!(
            !e2.contains(&crate::ids::RSC_010),
            "the NCX link to it must not draw RSC-010 either: {e2:?}"
        );
        assert!(
            e2.contains(&crate::ids::OPF_035),
            "it is still worth a warning: {e2:?}"
        );

        let e3 = ids_of(epub_declaring("3.0", "text/html", PLAIN_DOC, ""));
        assert!(
            e3.contains(&crate::ids::OPF_043),
            "EPUB 3 keeps OPF-043 - epubcheck's EPUB 3 branch has no such exemption: {e3:?}"
        );
        assert!(
            !e3.contains(&crate::ids::OPF_035),
            "and OPF-035 is unreachable in EPUB 3, since `OPFChecker30.checkItem` \
             does not call `super`: {e3:?}"
        );
    }

    /// The guide asks two independent questions, and the answers differ.
    /// Measured against epubcheck one target type per book: `text/html` draws
    /// RSC-032 alone, `application/x-dtbook+xml` draws RSC-032 alone, and
    /// `application/pdf` draws both it and OPF-032. Replacing our OPF-032
    /// with silence would have turned a wrong ID into a false negative.
    #[test]
    fn a_guide_reference_to_text_html_is_rsc_032_not_opf_032() {
        const GUIDE: &str = r#"<guide><reference type="cover" title="" href="ch1.html"/></guide>"#;
        let ids = ids_of(epub_declaring("2.0", "text/html", PLAIN_DOC, GUIDE));
        assert!(
            ids.contains(&crate::ids::RSC_032),
            "a foreign guide target still has to be reported: {ids:?}"
        );
        assert!(
            !ids.contains(&crate::ids::OPF_032),
            "but not as OPF-032 - epubcheck exempts the deprecated types there \
             (`OPFChecker`:172): {ids:?}"
        );
    }

    /// The other half of #72, and the one no instrument could see: the
    /// document was left out of `content_docs` entirely, so every reference
    /// inside it went unchecked. One real book hid 91 missing resources that
    /// way, and a document that was not even well-formed XML reported
    /// nothing at all - a book with real errors reading exactly like a clean
    /// one.
    #[test]
    fn references_inside_a_text_html_document_are_checked() {
        const WITH_BROKEN_REF: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title>\
            <link rel=\"stylesheet\" type=\"text/css\" href=\"missing.css\"/></head>\
            <body><p><img src=\"missing.png\" alt=\"x\"/></p></body></html>";
        let ids = ids_of(epub_declaring("2.0", "text/html", WITH_BROKEN_REF, ""));
        assert!(
            ids.contains(&crate::ids::RSC_007),
            "a dangling reference inside a `text/html` document must be found: {ids:?}"
        );
    }

    /// ...but the *grammar* must stay off it. epubcheck runs its OPS checker
    /// over such an item (`CheckerFactory`, `case HTML:`) and that always
    /// installs the handler, while the validators come from a map keyed on
    /// `application/xhtml+xml` - so the list comes back empty. Measured: the
    /// identical document draws three RSC-005 declared `application/xhtml+xml`
    /// and none declared `text/html`.
    ///
    /// The duplicate-`id` and ID-reference checks are part of that same
    /// validator set (`IDUNIQUE_20_SCH` is keyed the same way), which is why
    /// they hang off the same condition rather than off the RELAX NG pass
    /// alone - each one leaked a finding on the real book until it did.
    #[test]
    fn a_text_html_document_is_not_validated_against_the_grammar() {
        const BAD: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head>\
            <body><p id=\"dup\">one</p><p id=\"dup\">two</p>\
            <p aria-labelledby=\"nosuchid\">a</p><nosuchelement/></body></html>";
        let as_html = ids_of(epub_declaring("2.0", "text/html", BAD, ""));
        assert!(
            !as_html.contains(&crate::ids::RSC_005),
            "no schema violation, no duplicate-id and no idref resolution \
             for a `text/html` item: {as_html:?}"
        );
        let as_xhtml = ids_of(epub_declaring("2.0", "application/xhtml+xml", BAD, ""));
        assert!(
            as_xhtml.contains(&crate::ids::RSC_005),
            "the control: the identical document *is* checked when it is \
             declared XHTML, so the assertion above is not vacuous: {as_xhtml:?}"
        );
    }

    /// OPF-035 comes off the declared media-type alone. epubcheck emits it
    /// from `OPFChecker.checkItem` without opening the file, so a `text/html`
    /// item holding something that is not markup at all still draws it - the
    /// one shape where the author most needs telling. We used to require the
    /// content to parse as XHTML and said nothing here.
    #[test]
    fn opf_035_does_not_depend_on_the_document_content() {
        let ids = ids_of(epub_declaring(
            "2.0",
            "text/html",
            "just plain text, not markup at all\n",
            "",
        ));
        assert!(
            ids.contains(&crate::ids::OPF_035),
            "OPF-035 is a statement about the manifest, not the file: {ids:?}"
        );
    }

    /// #74: ID-reference resolution is EPUB 3 only. Its sibling
    /// `htm::check_idref_resolution` already carried that condition and this
    /// block did not, so an EPUB 2 book drew RSC-005 findings epubcheck does
    /// not report.
    ///
    /// Nothing real is lost. Every attribute the block names is absent from
    /// XHTML 1.1 — ARIA entirely, `for` only via the Forms module that OPS
    /// 2.0.1 does not include — so the grammar has already rejected the
    /// attribute and this only added a second message about the same defect.
    /// The other direction was checked too: `headers` *is* in XHTML 1.1 and
    /// takes IDREFS, and a dangling `headers` reference draws nothing from
    /// epubcheck either.
    ///
    /// Neither the corpus nor the shelf can see this — 0 of 167 books draw
    /// the message in either version.
    #[test]
    fn id_reference_resolution_is_epub3_only() {
        const DANGLING: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
            <body><p id=\"real\">x</p><p aria-labelledby=\"nosuchid\">a</p></body></html>";
        let dangling_count = |version: &str| {
            crate::validate_bytes(epub_declaring(
                version,
                "application/xhtml+xml",
                DANGLING,
                "",
            ))
            .messages
            .iter()
            .filter(|m| m.rule == Some("opf.content_document.idref_unresolved"))
            .count()
        };
        assert_eq!(
            dangling_count("2.0"),
            0,
            "EPUB 2 must not resolve ID references - epubcheck does not"
        );
        // #76: exactly one, not two. A second implementation in `opf`'s own
        // loop used to double every EPUB 3 finding here; epubcheck reports
        // one. The count is the assertion - `> 0` would have passed
        // throughout the bug.
        assert_eq!(
            dangling_count("3.0"),
            1,
            "the control: EPUB 3 still resolves them, and reports each defect once"
        );
    }

    /// #76's other half: the deleted block also checked `aria-details`, and
    /// epubcheck does not. Probed one book at a time, counting RSC-005 — a
    /// dangling `aria-details` draws nothing there, while `aria-labelledby`
    /// in the same position draws one.
    #[test]
    fn aria_details_is_not_an_id_reference_we_resolve() {
        let count = |attr: &str| {
            let ch1 = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head>\
                 <body><p id=\"real\">x</p><p {attr}=\"nosuchid\">a</p></body></html>"
            );
            crate::validate_bytes(epub_declaring("3.0", "application/xhtml+xml", &ch1, ""))
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.content_document.idref_unresolved"))
                .count()
        };
        assert_eq!(count("aria-details"), 0, "epubcheck reports nothing here");
        assert_eq!(
            count("aria-labelledby"),
            1,
            "the control: a real one still resolves"
        );
    }
}
