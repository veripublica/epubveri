//! SVG content-model checks, confirmed against the real corpus's error
//! *and* valid fixtures for `foreignObject`/`title`/generic SVG content:
//!
//! - `foreignObject`'s content must be ordinary XHTML flow content - reuses
//!   the *existing*, already-tested `schemas/xhtml.rng` flow-content
//!   grammar via a wrap+reparse trick (`Node::range()` gives the exact
//!   original-text byte span of any node, so the inner content can be
//!   reconstructed verbatim and re-validated - no RNG engine changes).
//! - `title`'s content model is far more permissive (a bare `<body>`, even
//!   a whole embedded `<html>` document, are valid title content per a
//!   real fixture) - just a recursive non-XHTML-namespace check, plus one
//!   narrow real HTML5 rule (`href` only valid on a/area/link/base).
//! - Everything else inside `<svg>` gets a generic, usage-level (`RSC-025`)
//!   element-vocabulary check - real epubcheck reports SVG conformance
//!   issues as USAGE, not errors (confirmed via a dedicated fixture).

use crate::xmlext::XmlTokens;
use std::collections::HashMap;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const EPUB_OPS_NS: &str = "http://www.idpf.org/2007/ops";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Real SVG 1.1 element vocabulary. A false negative here is far safer
/// than a false positive, since `RSC-025` findings are usage-level (Info).
/// **Eleven names were missing until 2026-08-27, and their absence was a
/// false positive rather than a gap.** They were found by extracting the
/// element declarations from `schema/20/rng/svg/*.rng` and diffing against
/// this list, while sizing the EPUB 2 half of #93. `altGlyph`,
/// `color-profile` and the `font-face-*` family are ordinary SVG 1.1, and
/// epubcheck is silent on all of them - measured one book each - while we
/// were reporting RSC-025. No book on the shelf uses any of them, which is
/// why `compare` never saw it.
///
/// `feDropShadow` below is SVG 2 rather than 1.1 and is kept deliberately:
/// epubcheck accepts it too, so removing it would start a divergence rather
/// than end one.
pub(crate) const SVG_ELEMENTS: &[&str] = &[
    "altGlyph",
    "altGlyphDef",
    "altGlyphItem",
    "animateColor",
    "color-profile",
    "definition-src",
    "font-face-format",
    "font-face-name",
    "font-face-src",
    "font-face-uri",
    "glyphRef",
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "image",
    "switch",
    "foreignObject",
    "title",
    "desc",
    "metadata",
    "a",
    "style",
    "script",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "path",
    "text",
    "tspan",
    "textPath",
    "tref",
    "marker",
    "pattern",
    "mask",
    "clipPath",
    "filter",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "linearGradient",
    "radialGradient",
    "stop",
    "animate",
    "animateMotion",
    "animateTransform",
    "set",
    "mpath",
    "view",
    "cursor",
    "font",
    "font-face",
    "glyph",
    "missing-glyph",
    "hkern",
    "vkern",
];

/// `feDropShadow` is the one name whose recognition is version-dependent: it
/// is an SVG 2 filter primitive, present in epubcheck's `schema/30/mod/svg11/
/// svg-filter.rnc` and in **none** of `schema/20/rng/svg/`. Diffing our list
/// against the EPUB 2 modules gives exactly this one extra name and nothing
/// missing, which is what makes the EPUB 2 arm below safe to make an error.
const SVG2_ONLY_ELEMENTS: &[&str] = &["feDropShadow"];

fn is_recognized_element(name: &str, is_epub3: bool) -> bool {
    if !is_epub3 && SVG2_ONLY_ELEMENTS.contains(&name) {
        return false;
    }
    SVG_ELEMENTS.contains(&name)
}

/// The target of an SVG paint reference: the inside of `url(…)`, with the
/// optional quotes CSS allows removed (`url('#a')`, `url("#a")`).
///
/// **The quoted spellings are not exotic — one of epubcheck's own fixtures
/// uses `stroke="url('#circle')"`**, and taking the value literally made the
/// target `'#circle'`, a name no document holds, so a valid file drew an
/// RSC-007 for a missing resource. Trimmed inside the parens too, since
/// `url( #a )` is equally legal.
pub(crate) fn url_reference(value: &str) -> Option<&str> {
    let inner = crate::xmlext::trim_xml_space(
        crate::xmlext::trim_xml_space(value)
            .strip_prefix("url(")?
            .strip_suffix(')')?,
    );
    let unquoted = inner
        .strip_prefix('\'')
        .and_then(|r| r.strip_suffix('\''))
        .or_else(|| inner.strip_prefix('"').and_then(|r| r.strip_suffix('"')))
        .unwrap_or(inner);
    Some(crate::xmlext::trim_xml_space(unquoted))
}

/// `RSC-025` (usage): an SVG-namespaced element not in the known
/// vocabulary. Stops descending at `foreignObject`/`title` boundaries
/// (their own, separate content models apply instead - checked via
/// `check_foreign_object`/`check_title_content`) and only ever looks at
/// SVG-namespaced children, so foreign content nested inside (embedded
/// RDF in `<metadata>`, etc.) is never touched by this check.
/// Every container resource this SVG document references, resolved and
/// NFC-normalized. Reports nothing.
///
/// **The same per-source gap as `smil::resource_refs`, found the same way.**
/// A standalone SVG in the spine is a content document, but `content_docs`
/// selects on `application/xhtml+xml`, so an SVG's own references were
/// collected by nothing. W3C's `lay-pp-embedded-images-svg` is eight
/// `<svg><image xlink:href="../images/A.png"/></svg>` plates in the spine;
/// we called all eight PNGs unreferenced and epubcheck called none of them.
///
/// References are gathered per *source* here and per *reference* in
/// epubcheck, so every new source has to be added by hand and nothing fails
/// loudly when one is missed. That is now twice. Before adding a reference
/// kind, ask which per-source lists it must join.
///
/// Walked once so the three callers cannot drift apart: [`resource_refs`],
/// which answers "was this resource referenced" for OPF-097;
/// [`check_resource_references`], which answers "does this reference resolve"
/// for RSC-007; and [`check_fragments`], which answers "does this fragment
/// name a real id" for RSC-012.
///
/// # The set is exactly what epubcheck registers, and it is small
///
/// This used to be "every element's `xlink:href` or `href`", which is far
/// wider than `OPSHandler`'s SVG dispatch and produced errors on markup
/// epubcheck says nothing about. Five, measured one book each against 5.3.0,
/// every one of them an RSC-007 we invented:
/// `<textPath xlink:href="missing.svg#p">`, `<tref>` likewise, a gradient's
/// `xlink:href`, and — at the time — a **plain** `href` on `<use>` or
/// `<image>`. The last of those is no longer invented at EPUB 3: 5.4.0
/// registers it, which is what `plain_href_counts` turns on.
///
/// What it registers, and nothing else:
///
/// | construct | `Reference.Type` | dispatched by |
/// |---|---|---|
/// | `use:href` | `SVG_SYMBOL` | `checkSymbol` |
/// | `image:href` | `IMAGE` | `checkImage` |
/// | `a:href` | `HYPERLINK` | `checkHRef` |
/// | `font-face-uri:href` | `FONT` | `checkSVGFontFaceURI` |
/// | any element's `fill`/`stroke` | `SVG_PAINT` | `checkPaint` |
///
/// Two details of that dispatch are load-bearing and both were probed:
///
/// - **`xlink:href` only *through 5.3.0*, and that is why `plain_href_counts`
///   exists.** Every call site used to pass the namespace explicitly, so SVG
///   2's unprefixed `href` was not a reference to epubcheck at all — the same
///   finding as issue #77 made about `<a>`. We filed it as w3c/epubcheck#1677
///   and 5.4.0 fixed it: `getSVGHrefs` now reads both spellings, and reports
///   HTM-062/063 on the mismatch (see `check_deprecated_xlink_href`). The
///   flag is the EPUB version — the fix lives in the EPUB 3 handler only, so
///   an EPUB 2 book still gets the 5.3.0 answer, and reading a plain `href`
///   there would invent exactly the RSC-007s listed below.
/// - **`checkPaint` takes the value literally**: `startsWith("url(")` and
///   `endsWith(")")`, so `fill="url(#a) red"` registers nothing. Probed.
///
/// The `marker-*` properties look like paint references and are not
/// dispatched, which is why they draw nothing. `clip-path` was in that
/// sentence until 5.4.0 fixed w3c/epubcheck#1678 — ours — and started
/// registering it. The callback receives
/// the reference **already unwrapped** from `url(...)`.
/// Where a reference was written, which decides what its target may be.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefSource {
    /// `href`/`xlink:href` on `use`/`image`/`a`/`font-face-uri`.
    Href,
    /// `fill`/`stroke="url(#…)"` — must reach a paint server.
    Paint,
    /// `clip-path="url(#…)"` — must reach a `clipPath`. Checked by epubcheck
    /// only since 5.4.0 (w3c/epubcheck#1678, ours): the `case` handling it
    /// existed and nothing registered the reference, so both a dangling and
    /// a wrong-typed clip-path passed silently.
    ClipPath,
}

fn for_each_reference<'a>(
    root: roxmltree::Node<'a, 'a>,
    plain_href_counts: bool,
    mut f: impl FnMut(roxmltree::Node<'a, 'a>, &'a str, RefSource),
) {
    for n in root.descendants().filter(|n| n.is_element()) {
        if n.tag_name().namespace() != Some(SVG_NS) {
            continue;
        }
        if matches!(n.tag_name().name(), "use" | "image" | "a" | "font-face-uri") {
            let xhref = n.attribute((XLINK_NS, "href"));
            let href = plain_href_counts.then(|| n.attr_no_ns("href")).flatten();
            // epubcheck's `getSVGHrefs`, both arms: with the two spellings
            // present and different, *both* are references; otherwise
            // whichever one is there.
            match (href, xhref) {
                (Some(h), Some(x)) if h != x => {
                    f(n, h, RefSource::Href);
                    f(n, x, RefSource::Href);
                }
                (Some(h), _) => f(n, h, RefSource::Href),
                (None, Some(x)) => f(n, x, RefSource::Href),
                (None, None) => {}
            }
        }
        for (attr, source) in [
            ("fill", RefSource::Paint),
            ("stroke", RefSource::Paint),
            ("clip-path", RefSource::ClipPath),
        ] {
            if let Some(v) = n.attr_no_ns(attr)
                && let Some(inner) = url_reference(v)
            {
                f(n, inner, source);
            }
        }
    }
}

/// `RSC-007`: a reference from a **standalone** SVG document to something the
/// container does not hold.
///
/// Normative in EPUB 2 and informative in EPUB 3 like the rest of the SVG
/// family? **No** — measured, and this one is an error at both versions,
/// because it is `ResourceReferencesChecker`'s question rather than the
/// grammar's. One book per version.
pub(crate) fn check_resource_references(
    svg_root: roxmltree::Node,
    path: &str,
    base_dir: &str,
    name_index: &std::collections::HashMap<String, String>,
    manifest_paths: &std::collections::HashSet<String>,
    is_epub3: bool,
    report: &mut Report,
) {
    use crate::opf::{is_external, nfc, resolve};
    for_each_reference(svg_root, is_epub3, |n, v, _| {
        let v = crate::url::trim_url(v);
        if v.is_empty() || v.starts_with('#') || is_external(v) || crate::opf::is_remote_url(v) {
            return;
        }
        let path_part = v.split('#').next().unwrap_or(v);
        if path_part.is_empty() {
            return;
        }
        let resolved = nfc(&resolve(base_dir, path_part));
        // A manifest item whose file is missing is RSC-001 at the manifest and
        // nothing more here: epubcheck registers every declared item, so
        // `checkUndeclaredReference`'s RSC-007 never sees it (probed one book
        // against 5.4.0, declared-and-missing vs undeclared-and-missing).
        if name_index.contains_key(&resolved) || manifest_paths.contains(&resolved) {
            return;
        }
        report.push_node(
            RSC_007,
            Severity::Error,
            format!("references a missing resource '{v}'"),
            path,
            n,
            "svg.reference_missing_resource",
            vec![v.to_string()],
        );
    });
}

/// RSC-006 for a remote resource an SVG element pulls in: `image`, `use`, and
/// `url(…)` in `fill`, `stroke` or `clip-path`. epubcheck routes these through
/// `checkImage`, `checkSymbol` and `checkSVGAttributeURLValue`, which register
/// a reference no remote resource may satisfy; a remote `<a>` is a hyperlink
/// and is fine there and here. We skipped every remote URL, so all of these
/// passed. Measured on 5.4.0, standalone and inline, one book per shape:
/// `image` (`href` and `xlink:href`), `use` and `fill` are RSC-006, `a` is
/// not. `stroke` and `clip-path` run through the same Java method as `fill`.
/// `font-face-uri` is left out: a remote font is a different question
/// (`remote-resources`), and it is unmeasured here.
///
/// EPUB 3 only, because only that was measured.
pub(crate) fn check_remote_references(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    if !is_epub3 {
        return;
    }
    for_each_reference(svg_root, is_epub3, |n, v, source| {
        let pulls_in = match source {
            RefSource::Href => matches!(n.tag_name().name(), "image" | "use"),
            _ => true,
        };
        let v = crate::url::trim_url(v);
        if pulls_in && crate::opf::is_remote_url(v) {
            report.push_node(
                RSC_006,
                Severity::Error,
                format!(
                    "remote resource '{v}' is not allowed here; it must be inside the container"
                ),
                path,
                n,
                "svg.remote_resource",
                vec![v.to_string()],
            );
        }
    });
}

pub(crate) fn resource_refs(svg_xml: &str, base_dir: &str) -> Vec<String> {
    use crate::opf::{is_external, nfc, resolve};
    let Ok(doc) = crate::ocf::parse_xml(svg_xml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Unconditionally both spellings, unlike the two checks that report:
    // this only answers "was this resource referenced" (OPF-097), so the
    // worst an over-wide read can do is withhold a usage message, never
    // invent an error. That is the right direction to be wrong in here, and
    // it keeps the extractor's signature free of a version it has no other
    // use for.
    for_each_reference(doc.root_element(), true, |_, v, _| {
        {
            let v = crate::url::trim_url(v);
            // Remote targets come back unresolved, as the SMIL extractor's do
            // and for the same reason: they have no container path, and the
            // caller keys them by the href as written.
            if crate::opf::is_remote_url(v) {
                out.push(v.to_string());
                return;
            }
            if v.is_empty() || v.starts_with('#') || is_external(v) {
                return;
            }
            let path_part = v.split('#').next().unwrap_or(v);
            if !path_part.is_empty() {
                out.push(nfc(&resolve(base_dir, path_part)));
            }
        }
    });
    out
}

pub(crate) fn check_vocabulary(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    // **The same question, asked at both versions with different force**, as
    // `check_required_attributes` already is. `schema/20/rng/content.rng`
    // includes the SVG 1.1 modules directly, so an unknown SVG element in an
    // EPUB 2 book is a normative RSC-005; at 3.0 the strict grammar runs
    // informatively and the same element is RSC-025 usage (issue #93).
    //
    // Measured four ways against 5.3.0 - inline and standalone, each version,
    // one book per cell - and they agree: EPUB 2 error, EPUB 3 usage, for a
    // document referenced as `image/svg+xml` just as for `<svg>` in an XHTML
    // document.
    //
    // What makes the EPUB 2 arm safe to make an *error* is that the list is
    // closed and was checked against the authority rather than assumed: every
    // one of the 81 element names `schema/20/rng/svg/*.rng` declares is in
    // `SVG_ELEMENTS`, and the single name ours has beyond them is handled by
    // `SVG2_ONLY_ELEMENTS`. On the local shelf, 261 EPUB 2 books carry inline
    // SVG and between them use two element names, both recognised - so this
    // adds no finding to any of them.
    let (id, severity) = if is_epub3 {
        (RSC_025, Severity::Usage)
    } else {
        (RSC_005, Severity::Error)
    };
    for child in svg_root.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(SVG_NS) {
            continue;
        }
        let name = child.tag_name().name();
        if !is_recognized_element(name, is_epub3) {
            report.push_at_pos(
                id,
                severity,
                format!("element \"{name}\" not allowed here"),
                path,
                Position::of(child),
            );
        }
        if matches!(name, "foreignObject" | "title") {
            continue;
        }
        check_vocabulary(child, path, is_epub3, report);
    }
}

/// Real SVG 1.1 attribute vocabulary — the union of every unprefixed
/// `attribute` name in the SVG 1.1 modules the strict grammar drives
/// (`mod/svg11/`, reached from `epub-svg-strict-inc.rnc`). The same list
/// extracted independently from epubcheck's *other* copy of SVG 1.1, the
/// XML RelaxNG under `schema/20/rng/svg/`, is a strict subset of it — two
/// sources, one answer, which is the check this project owes any list a
/// script produced.
///
/// In **EPUB 3** it is a flat union, not a per-element table: an attribute
/// valid on some SVG element but used on another passes here. That is a false
/// negative, and the deliberate trade — the same one `SVG_ELEMENTS` makes, and
/// for the same reason (`RSC-025` is usage-level, so a false positive costs
/// more than a miss). **EPUB 2** judges each element by its own list
/// (`svg11::SVG11_ATTRIBUTES`), because there the grammar is normative and a
/// miss is a missing error. What it does catch is the class actually reported:
/// an attribute SVG has no concept of at all, `<image alt="cover image">`
/// being HTML's `alt` reaching into an SVG subtree (Doitsu, MobileRead
/// #138). epubcheck reports that as `USAGE(RSC-025)`, because its full
/// SVG 1.1 grammar runs with `isNormative=false`.
///
/// `inkscape:`/`sodipodi:` attributes are in that grammar too (via
/// `inkscape.rnc`) but are namespaced, so they never reach this list —
/// only unprefixed attributes are checked at all. Note that `inkscape.rnc`
/// *does* have a no-namespace wildcard (`attribute none:*`), but it is
/// reachable only inside `inkscape:`/`sodipodi:` elements, not from
/// `SVG.Core.extra.attrib` — which is why `alt` on `<image>` is reported
/// rather than swallowed by it.
const SVG_ATTRIBUTES: &[&str] = &[
    "accent-height",
    "accumulate",
    "additive",
    "alignment-baseline",
    "alphabetic",
    "amplitude",
    "arabic-form",
    "ascent",
    "attributeName",
    "attributeType",
    "azimuth",
    "baseFrequency",
    "baseProfile",
    "baseline-shift",
    "bbox",
    "begin",
    "bias",
    "by",
    "calcMode",
    "cap-height",
    "class",
    "clip",
    "clip-path",
    "clip-rule",
    "clipPathUnits",
    "color",
    "color-interpolation",
    "color-interpolation-filters",
    "color-profile",
    "color-rendering",
    "contentScriptType",
    "contentStyleType",
    "cursor",
    "cx",
    "cy",
    "d",
    "descent",
    "diffuseConstant",
    "direction",
    "display",
    "divisor",
    "dominant-baseline",
    "dur",
    "dx",
    "dy",
    "edgeMode",
    "elevation",
    "enable-background",
    "end",
    "exponent",
    "externalResourcesRequired",
    "fill",
    "fill-opacity",
    "fill-rule",
    "filter",
    "filterRes",
    "filterUnits",
    "flood-color",
    "flood-opacity",
    "focusable",
    "font-family",
    "font-size",
    "font-size-adjust",
    "font-stretch",
    "font-style",
    "font-variant",
    "font-weight",
    "format",
    "from",
    "fx",
    "fy",
    "g1",
    "g2",
    "glyph-name",
    "glyph-orientation-horizontal",
    "glyph-orientation-vertical",
    "glyphRef",
    "gradientTransform",
    "gradientUnits",
    "hanging",
    "height",
    "horiz-adv-x",
    "horiz-origin-x",
    "horiz-origin-y",
    "href",
    "id",
    "ideographic",
    "image-rendering",
    "in",
    "in2",
    "intercept",
    "k",
    "k1",
    "k2",
    "k3",
    "k4",
    "kernelMatrix",
    "kernelUnitLength",
    "kerning",
    "keyPoints",
    "keySplines",
    "keyTimes",
    "lang",
    "lengthAdjust",
    "letter-spacing",
    "lighting-color",
    "limitingConeAngle",
    "local",
    "marker-end",
    "marker-mid",
    "marker-start",
    "markerHeight",
    "markerUnits",
    "markerWidth",
    "mask",
    "maskContentUnits",
    "maskUnits",
    "mathematical",
    "max",
    "media",
    "method",
    "min",
    "mode",
    "name",
    "numOctaves",
    "offset",
    "onabort",
    "onactivate",
    "onbegin",
    "onclick",
    "onend",
    "onerror",
    "onfocusin",
    "onfocusout",
    "onload",
    "onmousedown",
    "onmousemove",
    "onmouseout",
    "onmouseover",
    "onmouseup",
    "onrepeat",
    "onresize",
    "onscroll",
    "onunload",
    "onzoom",
    "opacity",
    "operator",
    "order",
    "orient",
    "orientation",
    "origin",
    "overflow",
    "overline-position",
    "overline-thickness",
    "paint-order",
    "panose-1",
    "path",
    "pathLength",
    "patternContentUnits",
    "patternTransform",
    "patternUnits",
    "pointer-events",
    "points",
    "pointsAtX",
    "pointsAtY",
    "pointsAtZ",
    "preserveAlpha",
    "preserveAspectRatio",
    "primitiveUnits",
    "r",
    "radius",
    "refX",
    "refY",
    "rel",
    "rendering-intent",
    "repeatCount",
    "repeatDur",
    "requiredExtensions",
    "requiredFeatures",
    "restart",
    "result",
    "rotate",
    "rx",
    "ry",
    "scale",
    "seed",
    "shape-rendering",
    "slope",
    "spacing",
    "specularConstant",
    "specularExponent",
    "spreadMethod",
    "startOffset",
    "stdDeviation",
    "stemh",
    "stemv",
    "stitchTiles",
    "stop-color",
    "stop-opacity",
    "strikethrough-position",
    "strikethrough-thickness",
    "string",
    "stroke",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-width",
    "style",
    "surfaceScale",
    "systemLanguage",
    "tabindex",
    "tableValues",
    "target",
    "targetX",
    "targetY",
    "text-anchor",
    "text-decoration",
    "text-rendering",
    "textLength",
    "title",
    "to",
    "transform",
    "transform-box",
    "transform-origin",
    "type",
    "u1",
    "u2",
    "underline-position",
    "underline-thickness",
    "unicode",
    "unicode-bidi",
    "unicode-range",
    "units-per-em",
    "v-alphabetic",
    "v-hanging",
    "v-ideographic",
    "v-mathematical",
    "values",
    "version",
    "vert-adv-y",
    "vert-origin-x",
    "vert-origin-y",
    "viewBox",
    "viewTarget",
    "visibility",
    "width",
    "widths",
    "word-spacing",
    "writing-mode",
    "x",
    "x-height",
    "x1",
    "x2",
    "xChannelSelector",
    "y",
    "y1",
    "y2",
    "yChannelSelector",
    "z",
    "zoomAndPan",
];

/// The attributes our list carries beyond SVG 1.1, all of them in EPUB 3's
/// grammar and none in EPUB 2's. Diffing `SVG_ATTRIBUTES` against the
/// unprefixed `<attribute name>` declarations in `schema/20/rng/svg/*.rng`
/// gives **nothing missing and exactly these extra**. The first four were
/// probed on their own EPUB 2 and EPUB 3 book against 5.3.0: each is RSC-005
/// at 2.0 and clean at 3.0. `paint-order` and the two `transform-*` were
/// missing from the list until the EPUB 3 grammar was read (2026-10-08), so
/// each drew an RSC-025 at 3.0 that epubcheck does not make. Same shape as
/// [`SVG2_ONLY_ELEMENTS`].
const SVG3_ONLY_ATTRIBUTES: &[&str] = &[
    "focusable",
    "href",
    "paint-order",
    "rel",
    "tabindex",
    "transform-box",
    "transform-origin",
];

/// Which unprefixed attributes each SVG 1.1 element takes in **EPUB 2**,
/// where epubcheck validates SVG against the full SVG 1.1 grammar
/// normatively (`schema/20/rng/svg11.rng` standalone, the same modules
/// through `content.rng` inline).
///
/// **Generated, not transcribed** (2026-10-08, epubcheck 5.4.0 tag v5.4.0).
/// A one-off extractor resolved every `ref` reachable from each `<element>` in
/// the 30 SVG modules, applying `<include>` overrides, and took the no-namespace
/// `<attribute name>`s. It stops at nested elements, and `xlink:`/`xml:`
/// attributes are namespaced, so they never appear. The `notAllowed` defines in
/// the basic modules are all element classes that the full modules extend by
/// `choice`, so a union is exactly the grammar's answer. Reading `content.rng`
/// instead of `svg11.rng` gives the identical table. The groups are the
/// grammar's own attribute classes, factored out by the same run.
///
/// **Checked against epubcheck itself, not only against the grammar.** A probe
/// book put each of the 81 elements, in a valid context, under every attribute
/// of [`SVG_ATTRIBUTES`] (20 per document, 1,053 documents). epubcheck rejected
/// exactly the 18,559 pairs this table leaves out and none of the 2,501 it
/// lists. The union of the table is [`SVG_ATTRIBUTES`] minus
/// [`SVG3_ONLY_ATTRIBUTES`], so the two lists agree.
///
/// To regenerate, run the same walk over a newer checkout's `svg11.rng`. The
/// grammar dates from 2003, so a change would be news.
// Generated data, kept one element per row so a diff reads as a table.
#[rustfmt::skip]
mod svg11 {
    /// `SVG.Presentation.attrib`.
    pub(super) const PRESENTATION: &[&str] = &["alignment-baseline", "baseline-shift", "clip", "clip-path", "clip-rule", "color", "color-interpolation", "color-interpolation-filters", "color-profile", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "enable-background", "fill", "fill-opacity", "fill-rule", "filter", "flood-color", "flood-opacity", "font-family", "font-size", "font-size-adjust", "font-stretch", "font-style", "font-variant", "font-weight", "glyph-orientation-horizontal", "glyph-orientation-vertical", "image-rendering", "kerning", "letter-spacing", "lighting-color", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "overflow", "pointer-events", "shape-rendering", "stop-color", "stop-opacity", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "unicode-bidi", "visibility", "word-spacing", "writing-mode"];

    /// `SVG.GraphicalEvents.attrib`.
    pub(super) const GRAPHICAL_EVENTS: &[&str] = &["onactivate", "onclick", "onfocusin", "onfocusout", "onload", "onmousedown", "onmousemove", "onmouseout", "onmouseover", "onmouseup"];

    /// `SVG.DocumentEvents.attrib`.
    pub(super) const DOCUMENT_EVENTS: &[&str] = &["onabort", "onerror", "onresize", "onscroll", "onunload", "onzoom"];

    /// `SVG.AnimationEvents.attrib`.
    pub(super) const ANIMATION_EVENTS: &[&str] = &["onbegin", "onend", "onload", "onrepeat"];

    /// `SVG.AnimationTiming.attrib`.
    pub(super) const ANIMATION_TIMING: &[&str] = &["begin", "dur", "end", "fill", "max", "min", "repeatCount", "repeatDur", "restart"];

    /// `SVG.AnimationValue.attrib`.
    pub(super) const ANIMATION_VALUE: &[&str] = &["by", "calcMode", "from", "keySplines", "keyTimes", "to", "values"];

    /// `SVG.AnimationAddtion.attrib`.
    pub(super) const ANIMATION_ADDITION: &[&str] = &["accumulate", "additive"];

    /// `SVG.AnimationAttribute.attrib`.
    pub(super) const ANIMATION_ATTRIBUTE: &[&str] = &["attributeName", "attributeType"];

    /// `SVG.FilterPrimitiveWithIn.attrib`.
    pub(super) const FILTER_PRIMITIVE_WITH_IN: &[&str] = &["height", "in", "result", "width", "x", "y"];

    /// `SVG.FilterPrimitive.attrib`.
    pub(super) const FILTER_PRIMITIVE: &[&str] = &["height", "result", "width", "x", "y"];

    /// `SVG.Font.attrib`.
    pub(super) const FONT: &[&str] = &["font-family", "font-size", "font-size-adjust", "font-stretch", "font-style", "font-variant", "font-weight"];

    /// `SVG.Conditional.attrib`.
    pub(super) const CONDITIONAL: &[&str] = &["requiredExtensions", "requiredFeatures", "systemLanguage"];

    /// `SVG.Style.attrib`.
    pub(super) const STYLE: &[&str] = &["class", "style"];

    /// `SVG.External.attrib`.
    pub(super) const EXTERNAL: &[&str] = &["externalResourcesRequired"];

    /// Each element's attribute groups, sorted by element name in byte order
    /// for a binary search.
    pub(super) const SVG11_ATTRIBUTES: &[(&str, &[&[&str]])] = &[
        ("a", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["id", "target", "transform"]]),
        ("altGlyph", &[GRAPHICAL_EVENTS, FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "dx", "dy", "fill", "fill-opacity", "fill-rule", "filter", "format", "glyph-orientation-horizontal", "glyph-orientation-vertical", "glyphRef", "id", "image-rendering", "kerning", "letter-spacing", "mask", "opacity", "pointer-events", "rotate", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "unicode-bidi", "visibility", "word-spacing", "x", "y"]]),
        ("altGlyphDef", &[&["id"]]),
        ("altGlyphItem", &[&["id"]]),
        ("animate", &[ANIMATION_EVENTS, ANIMATION_TIMING, ANIMATION_VALUE, ANIMATION_ADDITION, ANIMATION_ATTRIBUTE, CONDITIONAL, EXTERNAL, &["id"]]),
        ("animateColor", &[ANIMATION_EVENTS, ANIMATION_TIMING, ANIMATION_VALUE, ANIMATION_ADDITION, ANIMATION_ATTRIBUTE, CONDITIONAL, EXTERNAL, &["id"]]),
        ("animateMotion", &[ANIMATION_EVENTS, ANIMATION_TIMING, ANIMATION_VALUE, ANIMATION_ADDITION, CONDITIONAL, EXTERNAL, &["id", "keyPoints", "origin", "path", "rotate"]]),
        ("animateTransform", &[ANIMATION_EVENTS, ANIMATION_TIMING, ANIMATION_VALUE, ANIMATION_ADDITION, ANIMATION_ATTRIBUTE, CONDITIONAL, EXTERNAL, &["id", "type"]]),
        ("circle", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "cx", "cy", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "mask", "opacity", "pointer-events", "r", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility"]]),
        ("clipPath", &[FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "clipPathUnits", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "fill", "fill-opacity", "fill-rule", "filter", "glyph-orientation-horizontal", "glyph-orientation-vertical", "id", "image-rendering", "kerning", "letter-spacing", "mask", "opacity", "pointer-events", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "transform", "unicode-bidi", "visibility", "word-spacing", "writing-mode"]]),
        ("color-profile", &[&["id", "local", "name", "rendering-intent"]]),
        ("cursor", &[CONDITIONAL, EXTERNAL, &["id", "x", "y"]]),
        ("definition-src", &[&["id"]]),
        ("defs", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["id", "transform"]]),
        ("desc", &[STYLE, &["id"]]),
        ("ellipse", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "cx", "cy", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "mask", "opacity", "pointer-events", "rx", "ry", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility"]]),
        ("feBlend", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "in2", "mode"]]),
        ("feColorMatrix", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "type", "values"]]),
        ("feComponentTransfer", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id"]]),
        ("feComposite", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "in2", "k1", "k2", "k3", "k4", "operator"]]),
        ("feConvolveMatrix", &[FILTER_PRIMITIVE_WITH_IN, &["bias", "color-interpolation-filters", "divisor", "edgeMode", "id", "kernelMatrix", "kernelUnitLength", "order", "preserveAlpha", "targetX", "targetY"]]),
        ("feDiffuseLighting", &[FILTER_PRIMITIVE_WITH_IN, STYLE, &["color", "color-interpolation", "color-interpolation-filters", "color-rendering", "diffuseConstant", "id", "kernelUnitLength", "lighting-color", "surfaceScale"]]),
        ("feDisplacementMap", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "in2", "scale", "xChannelSelector", "yChannelSelector"]]),
        ("feDistantLight", &[&["azimuth", "elevation", "id"]]),
        ("feFlood", &[FILTER_PRIMITIVE_WITH_IN, STYLE, &["color", "color-interpolation", "color-interpolation-filters", "color-rendering", "flood-color", "flood-opacity", "id"]]),
        ("feFuncA", &[&["amplitude", "exponent", "id", "intercept", "offset", "slope", "tableValues", "type"]]),
        ("feFuncB", &[&["amplitude", "exponent", "id", "intercept", "offset", "slope", "tableValues", "type"]]),
        ("feFuncG", &[&["amplitude", "exponent", "id", "intercept", "offset", "slope", "tableValues", "type"]]),
        ("feFuncR", &[&["amplitude", "exponent", "id", "intercept", "offset", "slope", "tableValues", "type"]]),
        ("feGaussianBlur", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "stdDeviation"]]),
        ("feImage", &[PRESENTATION, FILTER_PRIMITIVE, STYLE, EXTERNAL, &["id", "preserveAspectRatio"]]),
        ("feMerge", &[FILTER_PRIMITIVE, &["color-interpolation-filters", "id"]]),
        ("feMergeNode", &[&["id", "in"]]),
        ("feMorphology", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id", "operator", "radius"]]),
        ("feOffset", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "dx", "dy", "id"]]),
        ("fePointLight", &[&["id", "x", "y", "z"]]),
        ("feSpecularLighting", &[FILTER_PRIMITIVE_WITH_IN, STYLE, &["color", "color-interpolation", "color-interpolation-filters", "color-rendering", "id", "kernelUnitLength", "lighting-color", "specularConstant", "specularExponent", "surfaceScale"]]),
        ("feSpotLight", &[&["id", "limitingConeAngle", "pointsAtX", "pointsAtY", "pointsAtZ", "specularExponent", "x", "y", "z"]]),
        ("feTile", &[FILTER_PRIMITIVE_WITH_IN, &["color-interpolation-filters", "id"]]),
        ("feTurbulence", &[FILTER_PRIMITIVE, &["baseFrequency", "color-interpolation-filters", "id", "numOctaves", "seed", "stitchTiles", "type"]]),
        ("filter", &[PRESENTATION, STYLE, EXTERNAL, &["filterRes", "filterUnits", "height", "id", "primitiveUnits", "width", "x", "y"]]),
        ("font", &[PRESENTATION, STYLE, EXTERNAL, &["horiz-adv-x", "horiz-origin-x", "horiz-origin-y", "id", "vert-adv-y", "vert-origin-x", "vert-origin-y"]]),
        ("font-face", &[&["accent-height", "alphabetic", "ascent", "bbox", "cap-height", "descent", "font-family", "font-size", "font-stretch", "font-style", "font-variant", "font-weight", "hanging", "id", "ideographic", "mathematical", "overline-position", "overline-thickness", "panose-1", "slope", "stemh", "stemv", "strikethrough-position", "strikethrough-thickness", "underline-position", "underline-thickness", "unicode-range", "units-per-em", "v-alphabetic", "v-hanging", "v-ideographic", "v-mathematical", "widths", "x-height"]]),
        ("font-face-format", &[&["id", "string"]]),
        ("font-face-name", &[&["id", "name"]]),
        ("font-face-src", &[&["id"]]),
        ("font-face-uri", &[&["id"]]),
        ("foreignObject", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["height", "id", "transform", "width", "x", "y"]]),
        ("g", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["id", "transform"]]),
        ("glyph", &[PRESENTATION, STYLE, &["arabic-form", "d", "glyph-name", "horiz-adv-x", "id", "lang", "orientation", "unicode", "vert-adv-y", "vert-origin-x", "vert-origin-y"]]),
        ("glyphRef", &[FONT, STYLE, &["dx", "dy", "format", "glyphRef", "id", "x", "y"]]),
        ("hkern", &[&["g1", "g2", "id", "k", "u1", "u2"]]),
        ("image", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip", "clip-path", "clip-rule", "color", "color-interpolation", "color-profile", "color-rendering", "cursor", "display", "fill-opacity", "filter", "height", "id", "image-rendering", "mask", "opacity", "overflow", "pointer-events", "preserveAspectRatio", "shape-rendering", "stroke-opacity", "text-rendering", "transform", "visibility", "width", "x", "y"]]),
        ("line", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "pointer-events", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility", "x1", "x2", "y1", "y2"]]),
        ("linearGradient", &[STYLE, EXTERNAL, &["color", "color-interpolation", "color-rendering", "gradientTransform", "gradientUnits", "id", "spreadMethod", "stop-color", "stop-opacity", "x1", "x2", "y1", "y2"]]),
        ("marker", &[PRESENTATION, STYLE, EXTERNAL, &["id", "markerHeight", "markerUnits", "markerWidth", "orient", "preserveAspectRatio", "refX", "refY", "viewBox"]]),
        ("mask", &[PRESENTATION, CONDITIONAL, STYLE, EXTERNAL, &["height", "id", "maskContentUnits", "maskUnits", "width", "x", "y"]]),
        ("metadata", &[&["id"]]),
        ("missing-glyph", &[PRESENTATION, STYLE, &["d", "horiz-adv-x", "id", "vert-adv-y", "vert-origin-x", "vert-origin-y"]]),
        ("mpath", &[EXTERNAL, &["id"]]),
        ("path", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "d", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "pathLength", "pointer-events", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility"]]),
        ("pattern", &[PRESENTATION, CONDITIONAL, STYLE, EXTERNAL, &["height", "id", "patternContentUnits", "patternTransform", "patternUnits", "preserveAspectRatio", "viewBox", "width", "x", "y"]]),
        ("polygon", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "pointer-events", "points", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility"]]),
        ("polyline", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "display", "fill", "fill-opacity", "fill-rule", "filter", "id", "image-rendering", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "pointer-events", "points", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility"]]),
        ("radialGradient", &[STYLE, EXTERNAL, &["color", "color-interpolation", "color-rendering", "cx", "cy", "fx", "fy", "gradientTransform", "gradientUnits", "id", "r", "spreadMethod", "stop-color", "stop-opacity"]]),
        ("rect", &[GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "display", "fill", "fill-opacity", "fill-rule", "filter", "height", "id", "image-rendering", "mask", "opacity", "pointer-events", "rx", "ry", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-rendering", "transform", "visibility", "width", "x", "y"]]),
        ("script", &[EXTERNAL, &["id", "type"]]),
        ("set", &[ANIMATION_EVENTS, ANIMATION_TIMING, ANIMATION_ATTRIBUTE, CONDITIONAL, EXTERNAL, &["id", "to"]]),
        ("stop", &[STYLE, &["color", "color-interpolation", "color-rendering", "id", "offset", "stop-color", "stop-opacity"]]),
        ("style", &[&["id", "media", "title", "type"]]),
        ("svg", &[PRESENTATION, GRAPHICAL_EVENTS, DOCUMENT_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["baseProfile", "contentScriptType", "contentStyleType", "height", "id", "preserveAspectRatio", "version", "viewBox", "width", "x", "y", "zoomAndPan"]]),
        ("switch", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["id", "transform"]]),
        ("symbol", &[PRESENTATION, GRAPHICAL_EVENTS, STYLE, EXTERNAL, &["id", "preserveAspectRatio", "viewBox"]]),
        ("text", &[GRAPHICAL_EVENTS, FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "dx", "dy", "fill", "fill-opacity", "fill-rule", "filter", "glyph-orientation-horizontal", "glyph-orientation-vertical", "id", "image-rendering", "kerning", "lengthAdjust", "letter-spacing", "mask", "opacity", "pointer-events", "rotate", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "textLength", "transform", "unicode-bidi", "visibility", "word-spacing", "writing-mode", "x", "y"]]),
        ("textPath", &[GRAPHICAL_EVENTS, FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "fill", "fill-opacity", "fill-rule", "filter", "glyph-orientation-horizontal", "glyph-orientation-vertical", "id", "image-rendering", "kerning", "lengthAdjust", "letter-spacing", "mask", "method", "opacity", "pointer-events", "shape-rendering", "spacing", "startOffset", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "textLength", "unicode-bidi", "visibility", "word-spacing"]]),
        ("title", &[STYLE, &["id"]]),
        ("tref", &[GRAPHICAL_EVENTS, FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "dx", "dy", "fill", "fill-opacity", "fill-rule", "filter", "glyph-orientation-horizontal", "glyph-orientation-vertical", "id", "image-rendering", "kerning", "lengthAdjust", "letter-spacing", "mask", "opacity", "pointer-events", "rotate", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "textLength", "unicode-bidi", "visibility", "word-spacing", "x", "y"]]),
        ("tspan", &[GRAPHICAL_EVENTS, FONT, CONDITIONAL, STYLE, EXTERNAL, &["alignment-baseline", "baseline-shift", "clip-path", "clip-rule", "color", "color-interpolation", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "dx", "dy", "fill", "fill-opacity", "fill-rule", "filter", "glyph-orientation-horizontal", "glyph-orientation-vertical", "id", "image-rendering", "kerning", "lengthAdjust", "letter-spacing", "mask", "opacity", "pointer-events", "rotate", "shape-rendering", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "text-anchor", "text-decoration", "text-rendering", "textLength", "unicode-bidi", "visibility", "word-spacing", "x", "y"]]),
        ("use", &[PRESENTATION, GRAPHICAL_EVENTS, CONDITIONAL, STYLE, EXTERNAL, &["height", "id", "transform", "width", "x", "y"]]),
        ("view", &[EXTERNAL, &["id", "preserveAspectRatio", "viewBox", "viewTarget", "zoomAndPan"]]),
        ("vkern", &[&["g1", "g2", "id", "k", "u1", "u2"]]),
    ];

    /// Which of the elements that take an attribute a value rule covers.
    pub(super) enum Scope {
        All,
        Only(&'static [&'static str]),
        AllBut(&'static [&'static str]),
    }

    /// What the grammar accepts as the value.
    pub(super) enum Value {
        /// A `token` enumeration: XML whitespace is collapsed first, so
        /// `" evenodd "` is `evenodd`, and case matters.
        OneOf(&'static [&'static str]),
        /// A `string` enumeration: compared as written, whitespace included.
        Exact(&'static [&'static str]),
        /// Both kinds in one choice: the first list compared as written, the
        /// second as tokens (EPUB 3's `writing-mode`).
        ExactOrToken(&'static [&'static str], &'static [&'static str]),
        /// `xsd:NMTOKEN` after the `collapse` facet.
        NmToken,
        /// `xsd:NMTOKENS`: one or more, separated by XML whitespace.
        NmTokens,
        /// `preserveAspectRatio`'s pattern.
        AspectRatio,
        /// The same pattern with an optional leading `defer` (EPUB 3).
        DeferAspectRatio,
        /// `xsd:language` after the `collapse` facet, or exactly empty.
        Language,
        /// `paint-order`: exactly `normal`, or a list of `fill`, `stroke` and
        /// `markers` in any order, repeats included.
        PaintOrder,
    }

    use Scope::*;
    use Value::*;

    /// The attributes whose value the EPUB 2 grammar constrains, generated
    /// by the same walk as [`SVG11_ATTRIBUTES`]: every other attribute is a
    /// plain string, an `ID` (judged by the id checks) or free text. Sorted
    /// by attribute; `operator` and `type` mean different things on
    /// different elements and have a row each.
    pub(super) const SVG11_VALUES: &[(&str, Scope, Value)] = &[
        ("accumulate", All, OneOf(&["none", "sum"])),
        ("additive", All, OneOf(&["replace", "sum"])),
        ("alignment-baseline", All, OneOf(&["after-edge", "alphabetic", "auto", "baseline", "before-edge", "central", "hanging", "ideographic", "inherit", "mathematical", "middle", "text-after-edge", "text-before-edge"])),
        ("calcMode", All, OneOf(&["discrete", "linear", "paced", "spline"])),
        ("class", All, NmTokens),
        ("clip-rule", All, OneOf(&["evenodd", "inherit", "nonzero"])),
        ("clipPathUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("color-interpolation", All, OneOf(&["auto", "inherit", "linearRGB", "sRGB"])),
        ("color-interpolation-filters", All, OneOf(&["auto", "inherit", "linearRGB", "sRGB"])),
        ("color-rendering", All, OneOf(&["auto", "inherit", "optimizeQuality", "optimizeSpeed"])),
        ("direction", All, OneOf(&["inherit", "ltr", "rtl"])),
        ("display", All, OneOf(&["block", "compact", "inherit", "inline", "inline-table", "list-item", "marker", "none", "run-in", "table", "table-caption", "table-cell", "table-column", "table-column-group", "table-footer-group", "table-header-group", "table-row", "table-row-group"])),
        ("dominant-baseline", All, OneOf(&["alphabetic", "auto", "central", "hanging", "ideographic", "inherit", "mathematical", "middle", "no-change", "reset-size", "text-after-edge", "text-before-edge", "use-script"])),
        ("edgeMode", All, OneOf(&["duplicate", "none", "wrap"])),
        ("externalResourcesRequired", All, OneOf(&["false", "true"])),
        ("fill", Only(&["animate", "animateColor", "animateMotion", "animateTransform", "set"]), OneOf(&["freeze", "remove"])),
        ("fill-rule", All, OneOf(&["evenodd", "inherit", "nonzero"])),
        ("filterUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("font-stretch", AllBut(&["font-face"]), OneOf(&["condensed", "expanded", "extra-condensed", "extra-expanded", "inherit", "narrower", "normal", "semi-condensed", "semi-expanded", "ultra-condensed", "ultra-expanded", "wider"])),
        ("font-style", AllBut(&["font-face"]), OneOf(&["inherit", "italic", "normal", "oblique"])),
        ("font-variant", AllBut(&["font-face"]), OneOf(&["inherit", "normal", "small-caps"])),
        ("font-weight", AllBut(&["font-face"]), OneOf(&["100", "200", "300", "400", "500", "600", "700", "800", "900", "bold", "bolder", "inherit", "lighter", "normal"])),
        ("gradientUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("image-rendering", All, OneOf(&["auto", "inherit", "optimizeQuality", "optimizeSpeed"])),
        ("lengthAdjust", All, OneOf(&["spacing", "spacingAndGlyphs"])),
        ("markerUnits", All, OneOf(&["strokeWidth", "userSpaceOnUse"])),
        ("maskContentUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("maskUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("method", All, OneOf(&["align", "stretch"])),
        ("mode", All, OneOf(&["darken", "lighten", "multiply", "normal", "screen"])),
        ("operator", Only(&["feComposite"]), OneOf(&["arithmetic", "atop", "in", "out", "over", "xor"])),
        ("operator", Only(&["feMorphology"]), OneOf(&["dilate", "erode"])),
        ("overflow", All, OneOf(&["auto", "hidden", "inherit", "scroll", "visible"])),
        ("patternContentUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("patternUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("pointer-events", All, OneOf(&["all", "fill", "inherit", "none", "painted", "stroke", "visible", "visibleFill", "visiblePainted", "visibleStroke"])),
        ("preserveAlpha", All, OneOf(&["false", "true"])),
        ("preserveAspectRatio", All, AspectRatio),
        ("primitiveUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("rendering-intent", All, OneOf(&["absolute-colorimetric", "auto", "perceptual", "relative-colorimetric", "saturation"])),
        ("restart", All, OneOf(&["always", "never", "whenNotActive"])),
        ("shape-rendering", All, OneOf(&["auto", "crispEdges", "geometricPrecision", "inherit", "optimizeSpeed"])),
        ("spacing", All, OneOf(&["auto", "exact"])),
        ("spreadMethod", All, OneOf(&["pad", "reflect", "repeat"])),
        ("stitchTiles", All, OneOf(&["noStitch", "stitch"])),
        ("stroke-linecap", All, OneOf(&["butt", "inherit", "round", "square"])),
        ("stroke-linejoin", All, OneOf(&["bevel", "inherit", "miter", "round"])),
        ("target", All, NmToken),
        ("text-anchor", All, OneOf(&["end", "inherit", "middle", "start"])),
        ("text-rendering", All, OneOf(&["auto", "geometricPrecision", "inherit", "optimizeLegibility", "optimizeSpeed"])),
        ("type", Only(&["feFuncA", "feFuncB", "feFuncG", "feFuncR"]), OneOf(&["discrete", "gamma", "identity", "linear", "table"])),
        ("type", Only(&["feTurbulence"]), OneOf(&["fractalNoise", "turbulence"])),
        ("type", Only(&["feColorMatrix"]), OneOf(&["hueRotate", "luminanceToAlpha", "matrix", "saturate"])),
        ("type", Only(&["animateTransform"]), OneOf(&["rotate", "scale", "skewX", "skewY", "translate"])),
        ("unicode-bidi", All, OneOf(&["bidi-override", "embed", "inherit", "normal"])),
        ("version", All, Exact(&["1.1"])),
        ("visibility", All, OneOf(&["hidden", "inherit", "visible"])),
        ("writing-mode", All, OneOf(&["inherit", "lr", "lr-tb", "rl", "rl-tb", "tb", "tb-rl"])),
        ("xChannelSelector", All, OneOf(&["A", "B", "G", "R"])),
        ("yChannelSelector", All, OneOf(&["A", "B", "G", "R"])),
        ("zoomAndPan", All, OneOf(&["disable", "magnify"])),
    ];

    /// The attributes each element takes at **EPUB 3** beyond its EPUB 2 list,
    /// generated by the same walk over `schema/30/mod/svg11/svg11-inc.rnc`
    /// (2026-10-08): `focusable`, `lang` and `tabindex` everywhere, SVG 2's
    /// `href`, `paint-order` and `transform-*`, and all of `feDropShadow`. No
    /// element loses an attribute at 3.0.
    pub(super) const SVG30_ADDED: &[(&str, &[&str])] = &[
        ("a", &["focusable", "href", "lang", "paint-order", "rel", "tabindex", "transform-box", "transform-origin"]),
        ("altGlyph", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("altGlyphDef", &["focusable", "lang", "tabindex"]),
        ("altGlyphItem", &["focusable", "lang", "tabindex"]),
        ("animate", &["focusable", "href", "lang", "tabindex"]),
        ("animateColor", &["focusable", "href", "lang", "tabindex"]),
        ("animateMotion", &["focusable", "href", "lang", "tabindex"]),
        ("animateTransform", &["focusable", "href", "lang", "tabindex"]),
        ("circle", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("clipPath", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("color-profile", &["focusable", "href", "lang", "tabindex"]),
        ("cursor", &["focusable", "href", "lang", "tabindex"]),
        ("definition-src", &["focusable", "href", "lang", "tabindex"]),
        ("defs", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("desc", &["focusable", "lang", "tabindex"]),
        ("ellipse", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("feBlend", &["focusable", "lang", "tabindex"]),
        ("feColorMatrix", &["focusable", "lang", "tabindex"]),
        ("feComponentTransfer", &["focusable", "lang", "tabindex"]),
        ("feComposite", &["focusable", "lang", "tabindex"]),
        ("feConvolveMatrix", &["focusable", "lang", "tabindex"]),
        ("feDiffuseLighting", &["focusable", "lang", "tabindex"]),
        ("feDisplacementMap", &["focusable", "lang", "tabindex"]),
        ("feDistantLight", &["focusable", "lang", "tabindex"]),
        ("feDropShadow", &["alignment-baseline", "baseline-shift", "class", "clip", "clip-path", "clip-rule", "color", "color-interpolation", "color-interpolation-filters", "color-profile", "color-rendering", "cursor", "direction", "display", "dominant-baseline", "dx", "dy", "enable-background", "fill", "fill-opacity", "fill-rule", "filter", "flood-color", "flood-opacity", "focusable", "font-family", "font-size", "font-size-adjust", "font-stretch", "font-style", "font-variant", "font-weight", "glyph-orientation-horizontal", "glyph-orientation-vertical", "height", "id", "image-rendering", "in", "kerning", "lang", "letter-spacing", "lighting-color", "marker-end", "marker-mid", "marker-start", "mask", "opacity", "overflow", "paint-order", "pointer-events", "result", "shape-rendering", "stdDeviation", "stop-color", "stop-opacity", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "stroke-width", "style", "tabindex", "text-anchor", "text-decoration", "text-rendering", "transform-box", "transform-origin", "unicode-bidi", "visibility", "width", "word-spacing", "writing-mode", "x", "y"]),
        ("feFlood", &["focusable", "lang", "tabindex"]),
        ("feFuncA", &["focusable", "lang", "tabindex"]),
        ("feFuncB", &["focusable", "lang", "tabindex"]),
        ("feFuncG", &["focusable", "lang", "tabindex"]),
        ("feFuncR", &["focusable", "lang", "tabindex"]),
        ("feGaussianBlur", &["focusable", "lang", "tabindex"]),
        ("feImage", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("feMerge", &["focusable", "lang", "tabindex"]),
        ("feMergeNode", &["focusable", "lang", "tabindex"]),
        ("feMorphology", &["focusable", "lang", "tabindex"]),
        ("feOffset", &["focusable", "lang", "tabindex"]),
        ("fePointLight", &["focusable", "lang", "tabindex"]),
        ("feSpecularLighting", &["focusable", "lang", "tabindex"]),
        ("feSpotLight", &["focusable", "lang", "tabindex"]),
        ("feTile", &["focusable", "lang", "tabindex"]),
        ("feTurbulence", &["focusable", "lang", "tabindex"]),
        ("filter", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("font", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("font-face", &["focusable", "lang", "tabindex"]),
        ("font-face-format", &["focusable", "lang", "tabindex"]),
        ("font-face-name", &["focusable", "lang", "tabindex"]),
        ("font-face-src", &["focusable", "lang", "tabindex"]),
        ("font-face-uri", &["focusable", "href", "lang", "tabindex"]),
        ("foreignObject", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("g", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("glyph", &["focusable", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("glyphRef", &["focusable", "href", "lang", "tabindex"]),
        ("hkern", &["focusable", "lang", "tabindex"]),
        ("image", &["focusable", "href", "lang", "tabindex", "transform-box", "transform-origin"]),
        ("line", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("linearGradient", &["focusable", "href", "lang", "tabindex"]),
        ("marker", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("mask", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("metadata", &["focusable", "lang", "tabindex"]),
        ("missing-glyph", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("mpath", &["focusable", "href", "lang", "tabindex"]),
        ("path", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("pattern", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("polygon", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("polyline", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("radialGradient", &["focusable", "href", "lang", "tabindex"]),
        ("rect", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("script", &["focusable", "href", "lang", "tabindex"]),
        ("set", &["focusable", "href", "lang", "tabindex"]),
        ("stop", &["focusable", "lang", "tabindex"]),
        ("style", &["lang"]),
        ("svg", &["focusable", "lang", "paint-order", "tabindex", "transform", "transform-box", "transform-origin"]),
        ("switch", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("symbol", &["focusable", "height", "lang", "paint-order", "tabindex", "transform-box", "transform-origin", "width"]),
        ("text", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("textPath", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("title", &["focusable", "lang", "tabindex"]),
        ("tref", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("tspan", &["focusable", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("use", &["focusable", "href", "lang", "paint-order", "tabindex", "transform-box", "transform-origin"]),
        ("view", &["focusable", "lang", "tabindex"]),
        ("vkern", &["focusable", "lang", "tabindex"]),
    ];

    /// The attributes whose value the EPUB 3 grammar constrains, scoped
    /// against the elements that take them at 3.0. Most enumerations here are
    /// `string` values, compared as written: `visibility=" hidden "` is
    /// invalid at 3.0 and valid at 2.0. `href` is an `anyURI` there, which
    /// Jing judges by rules of its own; it is left out until they are
    /// matched.
    pub(super) const SVG30_VALUES: &[(&str, Scope, Value)] = &[
        ("accumulate", All, Exact(&["none", "sum"])),
        ("additive", All, Exact(&["replace", "sum"])),
        ("alignment-baseline", All, Exact(&["after-edge", "alphabetic", "auto", "baseline", "before-edge", "central", "hanging", "ideographic", "inherit", "mathematical", "middle", "text-after-edge", "text-before-edge"])),
        ("calcMode", All, Exact(&["discrete", "linear", "paced", "spline"])),
        ("clip-rule", All, OneOf(&["evenodd", "inherit", "nonzero"])),
        ("clipPathUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("color-interpolation", All, OneOf(&["auto", "inherit", "linearRGB", "sRGB"])),
        ("color-interpolation-filters", All, Exact(&["auto", "inherit", "linearRGB", "sRGB"])),
        ("color-rendering", All, OneOf(&["auto", "inherit", "optimizeQuality", "optimizeSpeed"])),
        ("direction", All, Exact(&["inherit", "ltr", "rtl"])),
        ("display", All, Exact(&["block", "compact", "inherit", "inline", "inline-table", "list-item", "marker", "none", "run-in", "table", "table-caption", "table-cell", "table-column", "table-column-group", "table-footer-group", "table-header-group", "table-row", "table-row-group"])),
        ("dominant-baseline", All, Exact(&["alphabetic", "auto", "central", "hanging", "ideographic", "inherit", "mathematical", "middle", "no-change", "reset-size", "text-after-edge", "text-before-edge", "use-script"])),
        ("edgeMode", All, Exact(&["duplicate", "none", "wrap"])),
        ("externalResourcesRequired", All, OneOf(&["false", "true"])),
        ("fill", Only(&["animate", "animateColor", "animateMotion", "animateTransform", "set"]), Exact(&["freeze", "remove"])),
        ("fill-rule", All, OneOf(&["evenodd", "inherit", "nonzero"])),
        ("filterUnits", All, Exact(&["objectBoundingBox", "userSpaceOnUse"])),
        ("focusable", All, Exact(&["false", "true"])),
        ("font-stretch", AllBut(&["font-face"]), Exact(&["condensed", "expanded", "extra-condensed", "extra-expanded", "inherit", "narrower", "normal", "semi-condensed", "semi-expanded", "ultra-condensed", "ultra-expanded", "wider"])),
        ("font-style", AllBut(&["font-face"]), Exact(&["inherit", "italic", "normal", "oblique"])),
        ("font-variant", AllBut(&["font-face"]), Exact(&["inherit", "normal", "small-caps"])),
        ("font-weight", AllBut(&["font-face"]), Exact(&["100", "200", "300", "400", "500", "600", "700", "800", "900", "bold", "bolder", "inherit", "lighter", "normal"])),
        ("gradientUnits", All, Exact(&["objectBoundingBox", "userSpaceOnUse"])),
        ("image-rendering", All, Exact(&["auto", "inherit", "optimizeQuality", "optimizeSpeed"])),
        ("lang", All, Language),
        ("lengthAdjust", All, Exact(&["spacing", "spacingAndGlyphs"])),
        ("markerUnits", All, Exact(&["strokeWidth", "userSpaceOnUse"])),
        ("maskContentUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("maskUnits", All, OneOf(&["objectBoundingBox", "userSpaceOnUse"])),
        ("method", All, Exact(&["align", "stretch"])),
        ("mode", All, Exact(&["color", "color-burn", "color-dodge", "darken", "difference", "exclusion", "hard-light", "hue", "lighten", "luminosity", "multiply", "normal", "overlay", "saturation", "screen", "soft-light"])),
        ("operator", Only(&["feComposite"]), Exact(&["arithmetic", "atop", "in", "lighter", "out", "over", "xor"])),
        ("operator", Only(&["feMorphology"]), Exact(&["dilate", "erode"])),
        ("overflow", All, Exact(&["auto", "hidden", "inherit", "scroll", "visible"])),
        ("paint-order", All, PaintOrder),
        ("patternContentUnits", All, Exact(&["objectBoundingBox", "userSpaceOnUse"])),
        ("patternUnits", All, Exact(&["objectBoundingBox", "userSpaceOnUse"])),
        ("pointer-events", All, Exact(&["all", "fill", "inherit", "none", "painted", "stroke", "visible", "visibleFill", "visiblePainted", "visibleStroke"])),
        ("preserveAlpha", All, OneOf(&["false", "true"])),
        ("preserveAspectRatio", All, DeferAspectRatio),
        ("primitiveUnits", All, Exact(&["objectBoundingBox", "userSpaceOnUse"])),
        ("rendering-intent", All, Exact(&["absolute-colorimetric", "auto", "perceptual", "relative-colorimetric", "saturation"])),
        ("restart", All, Exact(&["always", "never", "whenNotActive"])),
        ("shape-rendering", All, Exact(&["auto", "crispEdges", "geometricPrecision", "inherit", "optimizeSpeed"])),
        ("spacing", All, Exact(&["auto", "exact"])),
        ("spreadMethod", All, Exact(&["pad", "reflect", "repeat"])),
        ("stitchTiles", All, Exact(&["noStitch", "stitch"])),
        ("stroke-linecap", All, OneOf(&["butt", "inherit", "round", "square"])),
        ("stroke-linejoin", All, OneOf(&["bevel", "inherit", "miter", "round"])),
        ("target", All, NmToken),
        ("text-anchor", All, Exact(&["end", "inherit", "middle", "start"])),
        ("text-rendering", All, Exact(&["auto", "geometricPrecision", "inherit", "optimizeLegibility", "optimizeSpeed"])),
        ("transform-box", All, Exact(&["border-box", "content-box", "fill-box", "stroke-box", "view-box"])),
        ("type", Only(&["feFuncA", "feFuncB", "feFuncG", "feFuncR"]), Exact(&["discrete", "gamma", "identity", "linear", "table"])),
        ("type", Only(&["feTurbulence"]), Exact(&["fractalNoise", "turbulence"])),
        ("type", Only(&["feColorMatrix"]), Exact(&["hueRotate", "luminanceToAlpha", "matrix", "saturate"])),
        ("type", Only(&["animateTransform"]), Exact(&["rotate", "scale", "skewX", "skewY", "translate"])),
        ("unicode-bidi", All, Exact(&["bidi-override", "embed", "inherit", "normal"])),
        ("version", All, Exact(&["1.0", "1.1", "1.2"])),
        ("visibility", All, Exact(&["collapse", "hidden", "inherit", "visible"])),
        ("writing-mode", All, ExactOrToken(&["lr", "lr-tb", "rl", "rl-tb", "tb", "tb-rl"], &["inherit"])),
        ("xChannelSelector", All, Exact(&["A", "B", "G", "R"])),
        ("yChannelSelector", All, Exact(&["A", "B", "G", "R"])),
        ("zoomAndPan", Only(&["svg"]), Exact(&["disable", "magnify"])),
        ("zoomAndPan", Only(&["view"]), OneOf(&["disable", "magnify"])),
    ];
}

fn is_recognized_attribute(name: &str, element: &str, is_epub3: bool) -> bool {
    if !is_epub3 {
        // EPUB 2 validates SVG against the whole SVG 1.1 grammar, so an
        // attribute is judged against its own element's list: `font-size` on
        // a `rect` and `fill` on a `stop` are RSC-005 there. The same table
        // has no ARIA at all and `lang` on `glyph` alone (measured on 5.4.0,
        // inline and standalone, before the table existed).
        if let Ok(i) = svg11::SVG11_ATTRIBUTES.binary_search_by(|(e, _)| (*e).cmp(element)) {
            return svg11::SVG11_ATTRIBUTES[i]
                .1
                .iter()
                .any(|group| group.contains(&name));
        }
        // Not an SVG 1.1 element: the vocabulary check reports the element
        // itself, and its attributes keep the flat list's answer.
        if SVG3_ONLY_ATTRIBUTES.contains(&name) || name == "role" || name.starts_with("aria-") {
            return false;
        }
        if name == "lang" {
            return element == "glyph";
        }
        return SVG_ATTRIBUTES.contains(&name);
    }
    // EPUB 3 judges each SVG 1.1 element (and `feDropShadow`) by its own
    // list too, as epubcheck's informative grammar does: the EPUB 2 list
    // plus `svg11::SVG30_ADDED`. Measured on 5.4.0 (2026-10-08) over 82
    // elements and 267 names: the same 18,779 pairs rejected.
    if is_known_at_3(element) {
        // `aria.global` is folded into `SVG.Core.attrib`, which every element
        // but `style` takes. It is a closed list (`aria-foo` is rejected), but
        // any `aria-*` name is accepted here: a miss, not a false finding.
        if name.starts_with("aria-") {
            return element != "style";
        }
        // `role` comes with the implicit-role groups of 19 elements and no
        // others. This once accepted `role` everywhere, on the reading that
        // the grammar had no `role` at all; the probe shows where it has one.
        if name == "role" {
            return SVG30_ROLE_ELEMENTS.contains(&element);
        }
        return takes_attribute_at_3(element, name);
    }
    // An element the vocabulary does not know: that is its finding, and its
    // attributes keep the flat list's answer.
    SVG_ATTRIBUTES.contains(&name) || name == "role" || name.starts_with("aria-")
}

/// Whether EPUB 3's SVG grammar has an element of this name.
fn is_known_at_3(element: &str) -> bool {
    svg11::SVG11_ATTRIBUTES
        .binary_search_by(|(e, _)| (*e).cmp(element))
        .is_ok()
        || svg11::SVG30_ADDED
            .binary_search_by(|(e, _)| (*e).cmp(element))
            .is_ok()
}

/// The SVG elements that take `role` at EPUB 3, through the grammar's
/// implicit-role groups (`common.attrs.aria.implicit.*`, defined in the
/// XHTML grammar rather than the SVG modules). Measured on 5.4.0
/// (2026-10-08): `role` on each of the 82 elements, rejected on all others.
const SVG30_ROLE_ELEMENTS: &[&str] = &[
    "a",
    "altGlyph",
    "circle",
    "ellipse",
    "foreignObject",
    "g",
    "glyph",
    "glyphRef",
    "image",
    "line",
    "path",
    "polygon",
    "polyline",
    "rect",
    "svg",
    "symbol",
    "text",
    "tspan",
    "use",
];

/// `RSC-025` (usage): an unprefixed attribute on an SVG-namespaced element
/// that SVG 1.1 has no such attribute for. Prefixed attributes are skipped
/// entirely (`xlink:`, `xml:`, `epub:` - which `check_epub_attributes`
/// owns - and the `inkscape:`/`sodipodi:` sets the grammar allows
/// wholesale), so this only ever sees the no-namespace vocabulary.
pub(crate) fn check_attribute_vocabulary(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    // **Normative in EPUB 2, informative in EPUB 3**, exactly as the element
    // vocabulary and the required attributes are (#93). This ran at 3.0 only,
    // on the reasoning that "epubcheck has no opinion in EPUB 2" - true of
    // RSC-025 and false of the validation underneath it. A lowercase
    // `viewbox` in an EPUB 2 book was written off here as our own false
    // positive; handed that book, epubcheck reports ERROR RSC-005. The gate
    // was suppressing a true finding.
    //
    // Cost measured before switching it on: across the shelf's 261 EPUB 2
    // books carrying inline SVG, nine distinct unprefixed attributes appear
    // and three are outside SVG 1.1 - `alt`, `preserveaspectratio` and
    // `viewbox`, one occurrence each in two books. All three confirmed to be
    // errors epubcheck reports.
    check_attrs_of(svg_root, path, is_epub3, report);
    for child in svg_root
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(SVG_NS))
    {
        // Same boundaries as `check_vocabulary`: `foreignObject` holds
        // XHTML and `title` may hold a whole embedded document, so neither
        // subtree is SVG to begin with.
        if matches!(child.tag_name().name(), "foreignObject" | "title") {
            check_attrs_of(child, path, is_epub3, report);
            continue;
        }
        check_attribute_vocabulary(child, path, is_epub3, report);
    }
}

fn check_attrs_of(n: roxmltree::Node, path: &str, is_epub3: bool, report: &mut Report) {
    for attr in n.attributes().filter(|a| a.namespace().is_none()) {
        let name = attr.name();
        // `data-*` is allowed on SVG exactly as it is on XHTML, and this list
        // could never have carried it: it is an open-ended family, not a
        // vocabulary entry. epubcheck's own `data-attribute-valid.svg` fixture
        // says so — its title is "data-\* attributes are allowed" — and we
        // reported RSC-025 on it.
        //
        // Probed rather than read off the grammar, one book per shape against
        // 5.3.0, because the grammar files contain no `data-` at all and the
        // reason is not visible in them: `data-a-b` draws nothing, `data-` and
        // `data-FOO` draw **HTM_061** (the name is malformed, which is a
        // different question), and `data` with no hyphen draws RSC-025, which
        // we already agreed on. So the shape is accepted here and the suffix
        // is judged by `htm::check_dom`, exactly as on the XHTML side — see
        // `is_data_attribute_name`, which deliberately does not re-validate
        // the suffix for the same reason.
        //
        // A control was part of the probe: `zzz-foo` is rejected by both
        // tools, so the grammar really is applied to this document and the
        // silence on `data-epub` is a rule rather than an absence.
        // EPUB 3 only: SVG 1.1 at 2.0 has no `data-*`, and `data-x` there is
        // the plain RSC-005 of any unknown attribute, with no HTM_061 for a
        // malformed suffix (measured on 5.4.0, inline and standalone, as on
        // an EPUB 2 XHTML `p`).
        if is_epub3 && crate::htm::is_data_attribute_name(name) {
            // The suffix is still judged, and on a *standalone* SVG only this
            // site can do it: `htm::check_dom` runs over content documents
            // declared `application/xhtml+xml`, so it already covers inline
            // SVG and never sees a bare `.svg` file. Accepting the shape
            // without this would have traded one wrong finding for silence,
            // which is the worse of the two — epubcheck reports HTM_061 here.
            if let Some(rest) = name.strip_prefix("data-")
                && !crate::htm::is_valid_data_attr_suffix(rest)
            {
                report.push_at_pos(
                    crate::ids::HTM_061,
                    Severity::Error,
                    format!("'data-{rest}' is not a valid data-* attribute name"),
                    path,
                    Position::of(n),
                );
            }
            continue;
        }
        if !is_recognized_attribute(name, n.tag_name().name(), is_epub3) {
            let (id, severity) = if is_epub3 {
                (RSC_025, Severity::Usage)
            } else {
                (RSC_005, Severity::Error)
            };
            report.push_at_pos(
                id,
                severity,
                format!("attribute \"{name}\" not allowed here"),
                path,
                Position::of(n),
            );
        }
    }
}

/// The SVG elements `epub:type` is allowed on — epubcheck's own list, from
/// `svg.renderable.elem` in `mod/epub-svg-forgiving-inc.rnc`. That grammar is
/// the **normative** half of its SVG validation (the full SVG 1.1 grammar
/// runs non-normatively, which is why our vocabulary check is usage-level),
/// and `epub:type` placement is one of only three things it enforces.
///
/// This used to be the inverse — a denylist of `title`/`desc`/`defs`/`tref`
/// plus unrecognized elements, reverse-engineered from the corpus fixtures.
/// It agreed with epubcheck on everything those fixtures exercised and
/// silently disagreed everywhere else: `marker`, `pattern`, `clipPath`,
/// `mask`, `linearGradient`, `stop`, `metadata`, `style` and the rest are
/// recognized SVG elements, so a denylist let `epub:type` through on all of
/// them. An allowlist is also the safer shape here — a new SVG element we
/// don't know about defaults to "not allowed", matching epubcheck, rather
/// than to silence.
///
/// Any *other* `epub:*`-namespaced attribute is always disallowed, on every
/// element, which is unchanged.
const EPUB_TYPE_ALLOWED_ELEMENTS: &[&str] = &[
    "a", "audio", "canvas", "circle", "ellipse", "g", "iframe", "image", "line", "path", "polygon",
    "polyline", "rect", "svg", "switch", "symbol", "text", "textPath", "tspan", "unknown", "use",
    "video",
];

pub(crate) fn check_epub_attributes(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for attr in svg_root.attributes() {
        check_one_epub_attribute(svg_root, attr, path, report);
    }
    for child in svg_root
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(SVG_NS))
    {
        check_epub_attributes_rec(child, path, report);
    }
}

fn check_epub_attributes_rec(n: roxmltree::Node, path: &str, report: &mut Report) {
    for attr in n.attributes() {
        check_one_epub_attribute(n, attr, path, report);
    }
    if matches!(n.tag_name().name(), "foreignObject" | "title") {
        return;
    }
    for child in n
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(SVG_NS))
    {
        check_epub_attributes_rec(child, path, report);
    }
}

fn check_one_epub_attribute(
    n: roxmltree::Node,
    attr: roxmltree::Attribute,
    path: &str,
    report: &mut Report,
) {
    if attr.namespace() != Some(EPUB_OPS_NS) {
        return;
    }
    if attr.name() == "type" {
        let name = n.tag_name().name();
        if !EPUB_TYPE_ALLOWED_ELEMENTS.contains(&name) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "attribute \"epub:type\" not allowed here",
                path,
                n,
                "svg.epub_attributes.type_not_allowed",
                Vec::new(),
            );
        }
    } else if attr.name() == "prefix" {
        // A real, legitimate attribute - checked separately, in full,
        // by `opf::check_prefix_declaration`/`check_prefix_placement`
        // (confirmed via a real fixture declaring `epub:prefix` on an
        // SVG root and expecting zero findings).
    } else {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("attribute \"epub:{}\" not allowed here", attr.name()),
            path,
            n,
            "svg.epub_attributes.attribute_not_allowed",
            vec![attr.name().to_string()],
        );
    }
}

/// `RSC-005`: every `id` attribute anywhere in the SVG document must be a
/// valid XML NCName (a real fixture uses `id="1"`, invalid because it
/// starts with a digit) and unique document-wide (a real fixture shares
/// one id between two elements, reported once *per* colliding element -
/// the same "per-element not per-pair" convention already used
/// elsewhere in this project, e.g. NCX id duplication).
/// `RSC-012`: a fragment in this SVG's own references that names no id in
/// this document.
///
/// **Only same-document fragments**, which is what a standalone SVG almost
/// always carries — `<use xlink:href="#rect">` and `fill="url(#grad)"`. A
/// cross-document fragment is checked against the *other* document and is
/// left to the caller that owns it; in the one probe that reached for it,
/// epubcheck aborted on an RSC-011 before the fragment question was asked.
///
/// **This is the half [`check_resource_references`] could not do**, and the
/// reason a first attempt at it was reverted (issue #130): walked over every
/// `href` the set was far too wide, so we named references epubcheck does
/// not register. With [`for_each_reference`] narrowed to the registered set,
/// the same walk answers this question correctly — measured one book per
/// construct, and the six that draw RSC-012 there now draw it here.
pub(crate) fn check_fragments(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    // The same walk answers two questions, which is why the element is kept
    // beside its id: does the fragment resolve at all (RSC-012, error), and
    // does it resolve to something the reference may point at (RSC-014,
    // usage since 5.4.0).
    let mut by_id: std::collections::HashMap<&str, roxmltree::Node> =
        std::collections::HashMap::new();
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            by_id.entry(id).or_insert(n);
        }
    }
    for_each_reference(svg_root, is_epub3, |n, v, source| {
        let v = crate::url::trim_url(v);
        let Some(frag) = v.strip_prefix('#') else {
            return;
        };
        if frag.is_empty() {
            return;
        }
        let Some(target) = by_id.get(frag) else {
            report.push_node(
                RSC_012,
                Severity::Error,
                format!(
                    "fragment identifier '{v}' does not resolve to an element in this document"
                ),
                path,
                n,
                "svg.fragment_missing_target",
                vec![v.to_string()],
            );
            return;
        };
        // **A standalone SVG's paint and clip-path targets were never
        // type-checked here**, only their existence — the embedded-in-XHTML
        // path has asked this since RefKind existed. epubcheck asks it of
        // both, and 5.4.0's downgrade made it a usage message.
        let kind = crate::opf::IdKind::of(*target);
        let wanted = match source {
            RefSource::Href => return,
            RefSource::Paint => crate::opf::IdKind::SvgPaint,
            RefSource::ClipPath => crate::opf::IdKind::SvgClipPath,
        };
        if kind != wanted {
            report.push_node(
                RSC_014,
                Severity::Usage,
                format!(
                    "reference '{v}' targets {} (incompatible resource type)",
                    kind.describe()
                ),
                path,
                n,
                "svg.fragment_incompatible_target",
                vec![v.to_string()],
            );
        }
    });
}

/// Manifest properties against what a standalone SVG document uses: OPF-014
/// for one it needs and lacks, OPF-015 for those it declares and does not
/// need, OPF-018/OPF-018b for an unneeded `remote-resources`. epubcheck runs
/// the same `OPSHandler30.checkProperties` on SVG as on XHTML; we ran ours on
/// XHTML only, so `properties="scripted"` on a plain SVG, or a `<script>` with
/// no declaration, passed here and failed there.
///
/// What an SVG can *need*: `scripted` (a `script` of a script type, a `form`,
/// or an `on*` attribute, as on the XHTML side), `mathml` (a MathML `math`,
/// in a `foreignObject`) and `remote-resources` (a remote `font-face-uri`,
/// found by epubcheck's own `resources-remote-font-in-svg-valid`, which the
/// first version of this check turned into an OPF-018). Nothing else: `svg` is not required of an SVG
/// document, `switch` is the XHTML `epub:switch`, and a remote image in an
/// SVG is RSC-006 rather than a remote resource. The unneeded set is
/// declared minus needed minus `nav`, `data-nav` and `cover-image`, and is one
/// OPF-015 naming them all, in epubcheck's vocabulary order. Measured on
/// 5.4.0: plain, `<script>`, `onclick`, MathML and SVG-`switch` documents, each
/// with no property and with `scripted`, `mathml`, `remote-resources` and
/// `switch` (30 books).
pub(crate) fn check_properties(
    svg_root: roxmltree::Node,
    path: &str,
    declared: &str,
    report: &mut Report,
) {
    const ORDER: [&str; 12] = [
        "cover-image",
        "data-nav",
        "dictionary",
        "glossary",
        "index",
        "mathml",
        "nav",
        "remote-resources",
        "scripted",
        "search-key-map",
        "svg",
        "switch",
    ];
    let needs_script = svg_root.descendants().find(|n| {
        if !n.is_element() {
            return false;
        }
        let script_type = n.attr_no_ns("type").unwrap_or("");
        let is_script = n.tag_name().name() == "script"
            && (script_type.is_empty() || crate::opf::is_script_media_type(script_type));
        is_script
            || n.tag_name().name() == "form"
            || n.attributes()
                .any(|a| a.namespace().is_none() && a.name().starts_with("on"))
    });
    let needs_math = svg_root.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "math"
            && n.tag_name().namespace() == Some("http://www.w3.org/1998/Math/MathML")
    });
    // A remote font through `font-face-uri` needs `remote-resources`
    // (`OPSHandler30.checkSVGFontFaceURI`); epubcheck's own
    // `resources-remote-font-in-svg-valid` declares it and expects silence.
    let mut needs_remote = None;
    for_each_reference(svg_root, true, |n, v, source| {
        if needs_remote.is_none()
            && matches!(source, RefSource::Href)
            && n.tag_name().name() == "font-face-uri"
            && crate::opf::is_remote_url(crate::url::trim_url(v))
        {
            needs_remote = Some(n);
        }
    });
    let declared: Vec<&str> = declared.xml_tokens().collect();
    // OPF-014 for an undeclared remote font is already reported where the
    // standalone SVG is walked in `opf.rs`, under the rule key a repairer
    // dispatches on (`opf.content_document.property_used_undeclared`), so
    // `needs_remote` only keeps the property out of the unneeded set here.
    for (name, node) in [("scripted", needs_script), ("mathml", needs_math)] {
        if let Some(node) = node
            && !declared.contains(&name)
        {
            report.push_node(
                OPF_014,
                Severity::Error,
                format!("the \"{name}\" property should be declared in the manifest for this SVG document"),
                path,
                node,
                "svg.properties.undeclared",
                vec![name.to_string()],
            );
        }
    }
    let mut unneeded: Vec<&str> = ORDER
        .iter()
        .copied()
        .filter(|p| declared.contains(p))
        .filter(|p| !matches!(*p, "nav" | "data-nav" | "cover-image"))
        .filter(|p| !(*p == "scripted" && needs_script.is_some()))
        .filter(|p| !(*p == "mathml" && needs_math.is_some()))
        .filter(|p| !(*p == "remote-resources" && needs_remote.is_some()))
        .collect();
    if let Some(i) = unneeded.iter().position(|p| *p == "remote-resources") {
        unneeded.remove(i);
        let (id, severity) = if needs_script.is_some() {
            (OPF_018B, Severity::Usage)
        } else {
            (OPF_018, Severity::Warning)
        };
        report.push_at_pos(
            id,
            severity,
            "the \"remote-resources\" property is declared but this SVG document references no remote resource",
            path,
            Position::of(svg_root),
        );
    }
    if !unneeded.is_empty() {
        report.push_node(
            OPF_015,
            Severity::Error,
            format!(
                "the propert{} {} declared but not needed by this SVG document",
                if unneeded.len() == 1 {
                    "y is"
                } else {
                    "ies are"
                },
                unneeded.join(", ")
            ),
            path,
            svg_root,
            "svg.properties.unneeded",
            unneeded.iter().map(|p| p.to_string()).collect(),
        );
    }
}

/// The HTML `id` rule on SVG elements of an EPUB 3 publication: non-empty, and
/// no XML whitespace anywhere in it.
///
/// epubcheck applies it twice. Its informative strict grammar
/// (`epub-svg-30-informative.rnc`, `isNormative=false`) runs on standalone and
/// inline SVG alike, and a failure there is **RSC-025, usage**. The normative
/// XHTML grammar types an *inline* SVG `id` the same way, which makes the
/// same value **RSC-005, an error** there too. A standalone document's
/// normative rule is `xsd:ID` instead (see [`check_ids`]): that one collapses
/// whitespace, so `id=" a "` is only RSC-025 in a `.svg` file and fails the
/// book inside XHTML. Measured on 5.4.0, one book per value and context,
/// root and child element: `" a "`, `" a"`, `"a b"`, `""`, `"a&#9;b"` and
/// `"a&#10;"` are caught; `"a&#160;"`, `"&#12288;a"`, `"1a"`, `"a:b"` and `"-a"`
/// are not (the last three fail `xsd:ID` in a standalone file and nothing
/// inline).
///
/// No EPUB 2 counterpart: epubcheck runs no informative pass there, and its
/// XHTML 1.1 grammar types inline SVG ids differently.
pub(crate) fn check_html_ids(
    svg_root: roxmltree::Node,
    path: &str,
    inline: bool,
    report: &mut Report,
) {
    for n in svg_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
    {
        let Some(id) = n.attr_no_ns("id") else {
            continue;
        };
        if !id.is_empty() && !id.contains(crate::xmlext::is_xml_space) {
            continue;
        }
        if inline {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "value of attribute \"id\" is invalid: '{id}' (empty, or holds whitespace)"
                ),
                path,
                n,
                "svg.ids.invalid_html_id",
                vec![id.to_string()],
            );
        }
        report.push_node(
            RSC_025,
            Severity::Usage,
            format!("value of attribute \"id\" is invalid: '{id}' (empty, or holds whitespace)"),
            path,
            n,
            "svg.ids.informative_invalid_id",
            vec![id.to_string()],
        );
    }
}

pub(crate) fn check_ids(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    let mut by_id: HashMap<&str, u32> = HashMap::new();
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            if !crate::xmlname::is_xsd_id(id) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"id\" is invalid: '{id}'"),
                    path,
                    n,
                    "svg.ids.invalid_ncname",
                    vec![id.to_string()],
                );
            }
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id")
            && by_id.get(id).copied().unwrap_or(0) > 1
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("Duplicate \"id\" value '{id}'"),
                path,
                n,
                "svg.ids.duplicate_id",
                vec![id.to_string()],
            );
        }
    }
}

/// `HTM-062`/`HTM-063`: SVG 2 deprecated `xlink:href` in favour of the
/// no-namespace `href`, and epubcheck 5.4.0 says so (w3c/epubcheck#1677, which
/// we filed; `OPSHandler30.getSVGHrefs`).
///
/// Two findings, both usage: `xlink:href` with no `href` beside it (HTM-062),
/// and both present with different values (HTM-063). Identical values are
/// silent — that is the migration epubcheck is asking for, not a defect.
///
/// **EPUB 3 only, and the four elements are epubcheck's rather than every
/// element that can carry the attribute**: the messages come out of the
/// `getSVGHrefs` override, which the base (EPUB 2) handler leaves alone, and
/// only `use`, `image`, `a` and `font-face-uri` route their URL through it.
pub(crate) fn check_deprecated_xlink_href(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    if !is_epub3 {
        return;
    }
    for n in svg_root.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some(SVG_NS)
            && matches!(n.tag_name().name(), "use" | "image" | "a" | "font-face-uri")
    }) {
        let Some(xhref) = n.attribute((XLINK_NS, "href")) else {
            continue;
        };
        match n.attr_no_ns("href") {
            None => report.push_node(
                HTM_062,
                Severity::Usage,
                "the SVG \"xlink:href\" attribute is deprecated; use \"href\" instead",
                path,
                n,
                "svg.reference.deprecated_xlink_href",
                vec![xhref.to_string()],
            ),
            Some(href) if href != xhref => report.push_node(
                HTM_063,
                Severity::Usage,
                format!(
                    "the \"xlink:href\" value (\"{xhref}\") should equal the \"href\" value (\"{href}\")"
                ),
                path,
                n,
                "svg.reference.xlink_href_mismatch",
                vec![xhref.to_string(), href.to_string()],
            ),
            Some(_) => {}
        }
    }
}

/// `ACC-011` (usage): an SVG `<a>` link with no accessible label, as
/// `OPSHandler30` decides it: a non-empty `xlink:title` or `aria-label`
/// (`Strings.isNullOrEmpty`, so `""` is no label), or an SVG `<title>` or
/// `<text>` element inside the link. **Character data alone is not a label**:
/// SVG does not render text outside `<text>`, and epubcheck sets `hasLabel`
/// only on those two elements. Counting any non-blank text (as this did)
/// missed ACC-011 on four of epubcheck's own fixtures,
/// `url-missing-resource-svg-a-*`, whose `<a>` holds a bare `link`.
pub(crate) fn check_link_labels(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for a in svg_root.descendants().filter(|n| {
        n.is_element() && n.tag_name().name() == "a" && n.tag_name().namespace() == Some(SVG_NS)
    }) {
        let non_empty = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
        let has_label = non_empty(a.attribute((XLINK_NS, "title")))
            || non_empty(a.attr_no_ns("aria-label"))
            || a.descendants().any(|c| {
                c.is_element()
                    && c.tag_name().namespace() == Some(SVG_NS)
                    && matches!(c.tag_name().name(), "title" | "text")
            });
        if !has_label {
            report.push_at_pos(
                ACC_011,
                Severity::Usage,
                "SVG link has no accessible label",
                path,
                Position::of(a),
            );
        }
    }
}

/// A real HTML5 rule: `href` is only a valid attribute on
/// `a`/`area`/`link`/`base` - `schemas/xhtml.rng`'s attribute handling is
/// deliberately permissive (a global catch-all pattern, not a per-element
/// attribute allowlist - see `anyOtherAttr`'s own doc comment), so this
/// isn't caught by the flow-content grammar and needs its own check.
fn check_href_attribute(n: roxmltree::Node, path: &str, report: &mut Report) {
    let name = n.tag_name().name();
    if !matches!(name, "a" | "area" | "link" | "base") && n.has_attr_no_ns("href") {
        report.push_node(
            RSC_005,
            Severity::Error,
            "attribute \"href\" not allowed here",
            path,
            n,
            "svg.content_model.href_not_allowed",
            Vec::new(),
        );
    }
}

/// `RSC-005`: any descendant not in the XHTML namespace ("elements from
/// namespace X are not allowed"), plus `check_href_attribute`. Confirmed
/// this is NOT a flow-content check: a real valid fixture uses a bare
/// `<body>`, and even a whole embedded `<html>` document, as title
/// content.
pub(crate) fn check_title_content(title: roxmltree::Node, path: &str, report: &mut Report) {
    // `descendants()` includes the node itself first - skip it (title's
    // own namespace is SVG, not XHTML, and isn't part of its own content).
    for n in title.descendants().skip(1).filter(|n| n.is_element()) {
        let ns = n.tag_name().namespace();
        if ns != Some(XHTML_NS) {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "elements from namespace \"{}\" are not allowed",
                    ns.unwrap_or("")
                ),
                path,
                n,
                "svg.title.foreign_namespace",
                vec![ns.unwrap_or("").to_string()],
            );
            continue;
        }
        check_href_attribute(n, path, report);
    }
}

/// Re-validates `foreignObject`'s inner content against the existing
/// XHTML flow-content grammar. Reconstructs the exact inner XML via
/// `Node::range()` (the original-text byte span of each child), wraps it
/// in a synthetic document that carries forward every namespace binding
/// from the real document's root (so prefixed content, e.g. `xlink:...`,
/// still resolves), re-parses, and validates via the same
/// `crate::rng::xhtml_grammar()` used for whole content documents - no
/// RNG engine changes needed.
///
/// EPUB3-only: a real EPUB2 fixture (`svg-foreignObject-switch-valid.xhtml`,
/// titled "body allowed inside foreignObject") explicitly permits a bare
/// `<body>` as foreignObject content, unlike EPUB3's own
/// `svg-foreignObject-with-body-error` fixture, which flags the exact same
/// shape as an error - EPUB2's OPS/XHTML content model is its own, more
/// lenient spec section, same precedent as several other EPUB3-only checks
/// in `htm.rs`/`opf.rs`.
pub(crate) fn check_foreign_object(
    fo: roxmltree::Node,
    text: &str,
    root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    wrap_in_body: bool,
    report: &mut Report,
) {
    if !is_epub3 {
        if epub2_grammar_reaches(fo) {
            check_epub2_foreign_object(fo, path, wrap_in_body, report);
        }
        return;
    }
    let mut children = fo.children();
    let Some(first) = children.next() else {
        return;
    };
    let last = fo.children().next_back().unwrap_or(first);
    // **Known upstream limitation, measured 2026-09-10 and deliberately not
    // worked around: roxmltree's `range()` covers only the first run of a text
    // node that mixes CDATA with plain text** (RazrFalcon/roxmltree#112, open
    // since 2023-11 against 0.21.1). Probed directly: `abc<![CDATA[XYZ]]>`
    // yields a range covering `abc` alone, and `<![CDATA[XYZ]]>abc` one
    // covering the CDATA alone — a CDATA-only node is correct.
    //
    // What that costs here: if foreignObject's **first or last direct child**
    // is such a mixed node, this slice loses its tail. It cannot produce a
    // malformed slice — the cut always falls on a run boundary, never inside
    // markup — so the reparse below still succeeds and what is lost is
    // character data, which in flow content is almost always permitted anyway.
    //
    // Population: **0 of 474 shelf books contain `<foreignObject>` at all**,
    // and no epubcheck fixture puts CDATA inside one. So the fix would be
    // reconstructing the inner text by hand for a shape nothing here can
    // exercise, and the test protecting it would be its only user. Re-price it
    // if a real book ever arrives with one.
    let inner = &text[first.range().start..last.range().end];

    // Every *prefixed* namespace binding from the real document's root
    // carries forward, so prefixed content inside the foreignObject still
    // resolves - but the wrapper's own *default* (unprefixed) namespace
    // is always forced to XHTML, regardless of what `root` itself
    // declares. When `root` is an XHTML document's own root, its default
    // already is XHTML, so this changes nothing there - but when `root`
    // is a standalone SVG document's own `<svg>` element (the other real
    // call site), its default is the SVG namespace, and copying it
    // verbatim would put the synthetic `<html>`/`<body>` wrapper itself
    // in the SVG namespace, failing the XHTML grammar check on every
    // single foreignObject regardless of its actual (valid) content - a
    // real bug only ever exposed once standalone SVG single-document
    // checks started actually running through this code path.
    let ns_decls = prefixed_ns_decls(root);
    // Embedded (foreignObject inside an XHTML document's own inline SVG):
    // there's already an ambient XHTML `<body>` in scope, so the content
    // is ordinary flow content and gets wrapped in a synthetic `<body>`
    // (confirmed: a real fixture explicitly flags a *literal* `<body>`
    // element appearing here as its own error, "element \"body\" not
    // allowed here" - body-inside-body). Standalone (a top-level SVG
    // content document with no ambient XHTML context at all): the
    // content itself must directly *be* a single `<body>` element (real
    // fixtures confirm both "non-body content" and "more than one body"
    // are their own distinct errors) - so it replaces the body slot
    // instead of being wrapped inside another one.
    // Standalone, the grammar is `common.inner.flow | body.elem`
    // (`epub-svg-forgiving-inc.rnc` plus `epub-svg-30.rnc`'s `|=`): flow
    // content directly, *or* one `body`. Only the second used to be accepted,
    // so `<p>`, `<div>`, `<math>` or plain text in a standalone foreignObject
    // drew RSC-005 here and nothing there (measured on 5.4.0). A lone XHTML
    // `body` fills the body slot; anything else is wrapped as flow content,
    // which keeps two `body` elements (body inside body) and a `title` (not
    // flow) errors, as epubcheck's own fixtures expect.
    let lone_body = {
        let mut elems = fo.children().filter(|c| c.is_element());
        let only = elems.next();
        elems.next().is_none()
            && only.is_some_and(|b| {
                b.tag_name().name() == "body"
                    && b.tag_name().namespace() == Some("http://www.w3.org/1999/xhtml")
            })
            && fo
                .children()
                .filter(|c| c.is_text())
                .all(|t| t.text().is_none_or(crate::xmlext::is_xml_blank))
    };
    let wrapped = if wrap_in_body || !lone_body {
        format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"{ns_decls}><head><title>t</title></head><body>{inner}</body></html>"
        )
    } else {
        format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"{ns_decls}><head><title>t</title></head>{inner}</html>"
        )
    };
    let Ok(doc) = crate::ocf::parse_xml(&wrapped) else {
        return;
    };
    if !crate::rng::validate_node(&crate::rng::xhtml_grammar(), doc.root_element()) {
        // Genuine catch-all, same caveat as opf.rs's RNG-backed checks:
        // the grammar doesn't expose which rule failed. This now also
        // covers `href` on a non-a/area/link/base host - #33 excepted
        // `href` from the wildcard (needed for a/area's own explicit
        // rules to be unambiguous, see #39), so the grammar itself rejects
        // it anywhere else. A separate `check_href_attribute` pass used to
        // be the only thing catching this inside foreignObject; running
        // both now double-reports the exact same defect (caught by
        // foreign_object_rejects_invalid_attribute expecting a single
        // RSC-005) - removed here, kept in check_title_content above,
        // which doesn't re-validate against the grammar and still needs
        // its own check.
        report.push_node(
            RSC_005,
            Severity::Error,
            "foreignObject content does not conform to the EPUB XHTML content-model schema",
            path,
            fo,
            "svg.foreign_object.schema_violation",
            Vec::new(),
        );
    }
}

/// Every prefixed namespace binding on `root`, as attributes for a synthetic
/// wrapper document, so prefixed content sliced out of the real document
/// still resolves there.
fn prefixed_ns_decls(root: roxmltree::Node) -> String {
    let mut ns_decls = String::new();
    for ns in root.namespaces() {
        match ns.name() {
            // "xml" is always implicitly bound to the fixed XML namespace
            // URI - redeclaring it is unnecessary and, if anything went
            // slightly wrong upstream, a needless source of a parse error.
            Some("xml") => continue,
            Some(prefix) => ns_decls.push_str(&format!(" xmlns:{prefix}=\"{}\"", ns.uri())),
            None => {}
        }
    }
    ns_decls
}

/// Whether epubcheck's EPUB 2 SVG grammar reaches `n` at all.
///
/// EPUB 2 validates SVG through NVDL (`ops20.nvdl` inline, `ops20-svg.nvdl`
/// standalone), and inside SVG it switches to mode `allowForeignNS`: an XHTML
/// element there is *attached* to the SVG section and validated with it, and
/// everything else - an SVG element included - is merely *allowed*. So an
/// SVG element below a non-SVG element that is itself inside SVG (the
/// commonest shape: `foreignObject` > `p` > `svg`) is never validated. We
/// validated it, and a `<rect/>` without `width` there was an RSC-005 that
/// epubcheck does not report. Measured on 5.4.0, inline and standalone: a
/// missing required attribute, a bad enumerated value, a bad
/// `preserveAspectRatio` and a content-model fault in such an SVG are all
/// silent there.
///
/// EPUB 3 is different and keeps its own walk: its standalone SVG does
/// validate that nested SVG (informatively), and inline it reports nothing.
fn epub2_grammar_reaches(n: roxmltree::Node) -> bool {
    let mut below_non_svg = false;
    for a in n.ancestors().skip(1).filter(|a| a.is_element()) {
        if a.tag_name().namespace() == Some(SVG_NS) {
            if below_non_svg {
                return false;
            }
        } else {
            below_non_svg = true;
        }
    }
    true
}

/// EPUB 2: an XHTML element as the child of any SVG element but
/// `foreignObject` is RSC-005.
///
/// SVG 1.1's grammar has no place for foreign elements outside
/// `foreignObject` (its `foreignElement` alternative is commented out in
/// `svg-extensibility.rng`, and `desc`, `title` and `metadata` take text
/// alone), and NVDL attaches XHTML to the SVG section, so the grammar sees
/// it. Elements in any other namespace are allowed, which is why RDF in
/// `metadata` stays clean. Measured on 5.4.0 under `svg`, `g`, `text`,
/// `desc`, `title`, `metadata`, `a` and `switch`, inline and standalone: one
/// RSC-005 per element, none for what it contains, and nothing at all at
/// EPUB 3.
pub(crate) fn check_epub2_xhtml_children(
    svg_root: roxmltree::Node,
    path: &str,
    report: &mut Report,
) {
    for parent in svg_root.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some(SVG_NS)
            && n.tag_name().name() != "foreignObject"
            && epub2_grammar_reaches(*n)
    }) {
        for child in parent
            .children()
            .filter(|c| c.is_element() && c.tag_name().namespace() == Some(XHTML_NS))
        {
            let name = child.tag_name().name();
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "element \"{name}\" not allowed in SVG element \"{}\"",
                    parent.tag_name().name()
                ),
                path,
                child,
                "svg.content_model.xhtml_child",
                vec![name.to_string(), parent.tag_name().name().to_string()],
            );
        }
    }
}

/// EPUB 2's `foreignObject`, which is two different grammars.
///
/// - **Standalone** (`svg11.rng` through `ops20-svg.nvdl`):
///   `SVG.ForeignObjectContent.class` is `svg` alone. Any XHTML element and
///   any text is RSC-005, one per child; elements in other namespaces are
///   allowed, an `svg` child is SVG like any other.
/// - **Inline** (`content.rng`, which hooks XHTML in): the content is any
///   mix of `svg`, `body` and XHTML 1.1 inline and block content. Each `body`
///   is checked in the body slot of the EPUB 2 XHTML grammar, the rest
///   together inside a `div`, whose flow content is exactly inline plus
///   block. Another SVG element there is RSC-005, another namespace allowed.
///   The XHTML is checked as NVDL hands it to the grammar, through
///   [`xhtml_only`]: in mode `allowForeignNS` an element in any other
///   namespace, at any depth, is cut out rather than validated, so MathML in
///   a `p` there is clean although MathML in an EPUB 2 body is not (#92).
///
/// Measured on 5.4.0 against 34 shapes in each place. Before this nothing
/// asked, and every one of the invalid shapes passed the book. The EPUB 2
/// fixture `svg-foreignObject-switch-valid` (a `body` inline) stays valid.
fn check_epub2_foreign_object(fo: roxmltree::Node, path: &str, inline: bool, report: &mut Report) {
    let not_allowed = |report: &mut Report, n: roxmltree::Node, what: String| {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("{what} not allowed in foreignObject"),
            path,
            n,
            "svg.foreign_object.child_not_allowed",
            vec![what],
        );
    };
    let mut rest = String::new();
    let mut bodies = Vec::new();
    for c in fo.children() {
        if c.is_text() {
            if c.text().is_some_and(|t| !crate::xmlext::is_xml_blank(t)) {
                if inline {
                    xhtml_only(c, &mut rest);
                } else {
                    not_allowed(report, c, "text".to_string());
                }
            }
            continue;
        }
        if !c.is_element() {
            continue;
        }
        let name = c.tag_name().name();
        match c.tag_name().namespace() {
            Some(SVG_NS) if name == "svg" => {}
            Some(SVG_NS) => not_allowed(report, c, format!("element \"{name}\"")),
            Some(XHTML_NS) if !inline => not_allowed(report, c, format!("element \"{name}\"")),
            Some(XHTML_NS) if name == "body" => bodies.push(c),
            Some(XHTML_NS) => xhtml_only(c, &mut rest),
            _ => {}
        }
    }
    if !inline {
        return;
    }
    let head = format!(
        "<html xmlns=\"{XHTML_NS}\" xmlns:epub=\"{EPUB_OPS_NS}\" xmlns:xlink=\"{XLINK_NS}\"><head><title>t</title></head>"
    );
    let mut wrappers: Vec<String> = bodies
        .iter()
        .map(|b| {
            let mut body = String::new();
            xhtml_only(*b, &mut body);
            format!("{head}{body}</html>")
        })
        .collect();
    if !rest.is_empty() {
        wrappers.push(format!("{head}<body><div>{rest}</div></body></html>"));
    }
    let grammar = crate::rng::xhtml_grammar_epub2();
    let conforms = wrappers.iter().all(|w| {
        // Rebuilt from parsed nodes, so it always reparses; if it somehow
        // does not, that is not the book's fault and says nothing.
        crate::ocf::parse_xml(w).map_or(true, |doc| {
            crate::rng::validate_node(&grammar, doc.root_element())
        })
    });
    if !conforms {
        report.push_node(
            RSC_005,
            Severity::Error,
            "foreignObject content does not conform to the EPUB XHTML content-model schema",
            path,
            fo,
            "svg.foreign_object.schema_violation",
            Vec::new(),
        );
    }
}

/// `n` serialised as EPUB 2's NVDL hands it to the XHTML grammar inside SVG
/// (mode `allowForeignNS` in `ops20.nvdl`): XHTML elements and text kept;
/// an element in any other namespace dropped with everything in it; of the
/// attributes, those in no namespace and in the XML, OPS and XLink
/// namespaces kept (the ones the mode attaches), the rest dropped.
/// Comments and processing instructions carry nothing the grammar reads.
fn xhtml_only(n: roxmltree::Node, out: &mut String) {
    fn escape(s: &str, out: &mut String) {
        for ch in s.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                c => out.push(c),
            }
        }
    }
    if n.is_text() {
        escape(n.text().unwrap_or(""), out);
        return;
    }
    if !n.is_element() || n.tag_name().namespace() != Some(XHTML_NS) {
        return;
    }
    let name = n.tag_name().name();
    out.push('<');
    out.push_str(name);
    for a in n.attributes() {
        let prefix = match a.namespace() {
            None => "",
            Some("http://www.w3.org/XML/1998/namespace") => "xml:",
            Some(EPUB_OPS_NS) => "epub:",
            Some(XLINK_NS) => "xlink:",
            Some(_) => continue,
        };
        out.push(' ');
        out.push_str(prefix);
        out.push_str(a.name());
        out.push_str("=\"");
        escape(a.value(), out);
        out.push('"');
    }
    out.push('>');
    for c in n.children() {
        xhtml_only(c, out);
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// SVG 1.1 required attributes, enforced for **EPUB 2 only**.
///
/// `schema/20/rng/content.rng` includes the SVG 1.1 modules directly, so
/// inline SVG in an EPUB 2 content document is validated against them
/// *normatively* - epubcheck reports RSC-005 errors. EPUB 3 is the opposite
/// (see the caller's comment): the strict grammar runs informatively there
/// and inline SVG draws nothing, which is why this runs on one version only.
///
/// The table is the whole of what `svg-shape.rng` and `svg-image.rng` require:
/// every `<attribute>` outside an `<optional>` in the eight `attlist.*`
/// defines, each of which has exactly one define (no `combine="interleave"`
/// contributor elsewhere - checked, because a partial read of an interleaved
/// attlist is how this project has been wrong before).
///
/// Every row was then confirmed against epubcheck 5.3.0 one book at a time,
/// including the two negatives: `<line/>` requires nothing, and a complete
/// `<rect width height/>` is silent. Eleven books, eleven agreements.
///
/// This is a slice of #93, not its closure: epubcheck validates the entire
/// SVG 1.1 grammar there - vocabulary, content models, attribute lists,
/// datatypes - and this covers required attributes alone. The slice was
/// chosen because it is closed and enumerable, so it cannot invent a finding
/// epubcheck does not also make.
const SVG_REQUIRED_ATTRS: &[(&str, &[&str])] = &[
    ("animate", &["attributeName"]),
    ("animateColor", &["attributeName"]),
    ("animateTransform", &["attributeName"]),
    ("circle", &["r"]),
    ("color-profile", &["name"]),
    ("ellipse", &["rx", "ry"]),
    ("feBlend", &["in2"]),
    ("feComposite", &["in2"]),
    ("feConvolveMatrix", &["kernelMatrix", "order"]),
    ("feDisplacementMap", &["in2"]),
    ("feFuncA", &["type"]),
    ("feFuncB", &["type"]),
    ("feFuncG", &["type"]),
    ("feFuncR", &["type"]),
    ("font", &["horiz-adv-x"]),
    ("foreignObject", &["height", "width"]),
    ("hkern", &["k"]),
    ("image", &["height", "width"]),
    ("path", &["d"]),
    ("polygon", &["points"]),
    ("polyline", &["points"]),
    ("rect", &["height", "width"]),
    ("script", &["type"]),
    ("set", &["attributeName"]),
    ("stop", &["offset"]),
    ("vkern", &["k"]),
];

/// The ten alignment keywords `preserveAspectRatio` admits, optionally
/// followed by `meet` or `slice`. The grammar states it as a regular
/// expression; this is the same thing without a regex engine.
const SVG_PRESERVE_ASPECT_RATIO: &[&str] = &[
    "none", "xMaxYMax", "xMaxYMid", "xMaxYMin", "xMidYMax", "xMidYMid", "xMidYMin", "xMinYMax",
    "xMinYMid", "xMinYMin",
];

/// Whether `preserveAspectRatio`'s value matches the grammar's pattern:
/// optional whitespace, one alignment keyword, optionally whitespace and
/// `meet` or `slice`, optional whitespace.
fn preserve_aspect_ratio_is_valid(v: &str) -> bool {
    let mut parts = v.xml_tokens();
    let Some(align) = parts.next() else {
        return false;
    };
    if !SVG_PRESERVE_ASPECT_RATIO.contains(&align) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(m) => matches!(m, "meet" | "slice") && parts.next().is_none(),
    }
}

/// A `token`-typed enumeration value: XML whitespace collapsed, then one of
/// `allowed`, case included. `" evenodd "` passes, `"evenodd evenodd"` and
/// `"EVENODD"` do not (measured on 5.4.0).
fn is_one_token_of(value: &str, allowed: &[&str]) -> bool {
    let mut tokens = value.xml_tokens();
    tokens.next().is_some_and(|t| allowed.contains(&t)) && tokens.next().is_none()
}

/// Whether the grammar of the book's version accepts `value` for `attr` on
/// `element`, or `None` when it does not constrain that value. At 2.0 the
/// caller has already established that `element` takes `attr`; at 3.0 this
/// asks again, because an element the vocabulary does not know keeps the
/// flat list there.
fn svg_value_is_valid(element: &str, attr: &str, value: &str, is_epub3: bool) -> Option<bool> {
    use svg11::{Scope, Value};
    let table = if is_epub3 {
        // An attribute the element does not take is reported as such by
        // `check_attribute_vocabulary`, and that is all epubcheck says of it.
        if !takes_attribute_at_3(element, attr) {
            return None;
        }
        svg11::SVG30_VALUES
    } else {
        svg11::SVG11_VALUES
    };
    let (_, _, rule) = table.iter().find(|(a, scope, _)| {
        *a == attr
            && match scope {
                Scope::All => true,
                Scope::Only(els) => els.contains(&element),
                Scope::AllBut(els) => !els.contains(&element),
            }
    })?;
    Some(match rule {
        Value::OneOf(allowed) => is_one_token_of(value, allowed),
        Value::Exact(allowed) => allowed.contains(&value),
        Value::ExactOrToken(exact, tokens) => {
            exact.contains(&value) || is_one_token_of(value, tokens)
        }
        Value::NmToken => {
            let mut tokens = value.xml_tokens();
            tokens.next().is_some_and(crate::xmlname::is_nmtoken) && tokens.next().is_none()
        }
        Value::NmTokens => {
            let mut tokens = value.xml_tokens().peekable();
            tokens.peek().is_some() && tokens.all(crate::xmlname::is_nmtoken)
        }
        Value::AspectRatio => preserve_aspect_ratio_is_valid(value),
        Value::DeferAspectRatio => {
            let v = value.trim_matches(crate::xmlext::is_xml_space);
            match v.strip_prefix("defer") {
                Some(rest) if rest.starts_with(crate::xmlext::is_xml_space) => {
                    preserve_aspect_ratio_is_valid(rest)
                }
                _ => preserve_aspect_ratio_is_valid(value),
            }
        }
        Value::Language => {
            value.is_empty() || crate::rng::datatype::Datatype::Language.allows(value)
        }
        Value::PaintOrder => {
            let mut tokens = value.xml_tokens().peekable();
            value == "normal"
                || (tokens.peek().is_some()
                    && tokens.all(|t| matches!(t, "fill" | "stroke" | "markers")))
        }
    })
}

/// Whether `element` takes `attr` in the EPUB 3 grammar: its EPUB 2 list
/// plus `svg11::SVG30_ADDED`.
fn takes_attribute_at_3(element: &str, attr: &str) -> bool {
    let at_2 = svg11::SVG11_ATTRIBUTES
        .binary_search_by(|(e, _)| (*e).cmp(element))
        .is_ok_and(|i| {
            svg11::SVG11_ATTRIBUTES[i]
                .1
                .iter()
                .any(|g| g.contains(&attr))
        });
    at_2 || svg11::SVG30_ADDED
        .binary_search_by(|(e, _)| (*e).cmp(element))
        .is_ok_and(|i| svg11::SVG30_ADDED[i].1.contains(&attr))
}

/// The elements whose required attribute is the **namespaced** `xlink:href`,
/// which `has_attr_no_ns` cannot see — the reason they were missing from the
/// table above rather than merely unlisted.
///
/// Found while probing the containers with deliberately bare elements, and
/// then enumerated properly: the grammar extraction that produced the rest of
/// the table missed every one of these, because the xlink attributes are
/// declared in their own module rather than in the element's own `attlist`.
/// **The extractor was a candidate generator, not an authority** — each row
/// here and above is a measured book.
///
/// `animateMotion`, `pattern` and `marker` were probed too and require
/// nothing; they are listed here only so the next reader does not re-probe
/// them.
///
/// `a`, `image`, `font-face-uri` and `definition-src` were missing all the
/// same, and the extraction was redone from the grammar to find them: every
/// `attlist.*` that references `SVG.XLinkRequired.attrib`,
/// `SVG.XLinkEmbed.attrib` or `SVG.XLinkReplace.attrib`, the three sets in
/// `svg-xlink-attrib.rng` whose `xlink:href` is not optional. Ten elements;
/// the four new ones measured on 5.4.0, inline and standalone, each with a
/// control carrying `xlink:href` that is clean. A plain `href` does not
/// stand in for it at 2.0: epubcheck reports both the unknown `href` and the
/// missing `xlink:href`.
const SVG_REQUIRED_XLINK_HREF: &[&str] = &[
    "a",
    "cursor",
    "definition-src",
    "feImage",
    "font-face-uri",
    "image",
    "mpath",
    "textPath",
    "tref",
    "use",
];

/// SVG 1.1's **descriptive elements**, allowed inside any graphics element.
const SVG_DESCRIPTIVE_ELEMENTS: &[&str] = &["desc", "metadata", "title"];

/// SVG 1.1's **animation elements**, likewise allowed inside any graphics
/// element.
const SVG_ANIMATION_ELEMENTS: &[&str] = &[
    "animate",
    "animateColor",
    "animateMotion",
    "animateTransform",
    "set",
];

/// The graphics elements whose SVG 1.1 content model is **closed**: any number
/// of descriptive and animation elements, in any order, and nothing else — no
/// other element and no text.
///
/// This is the first slice of the content-model axis, and it was chosen for
/// the property that made the vocabulary slices safe: the rule is closed and
/// enumerable, so it cannot fire where epubcheck stays silent. Eleven cells
/// measured against 5.3.0, one book each — `rect > circle`, `use > rect`,
/// `image > rect` and `line > text` are errors; `rect > desc`, `rect > set`,
/// `image > title`, `path > metadata`, `polygon > animate` are clean; loose
/// text inside a shape is an error and **indentation whitespace is not**,
/// which is the only one of the eleven that could have cost a false positive
/// on a real book.
///
/// Deliberately *not* here: the container elements (`g`, `defs`, `svg`, `a`,
/// `switch`, `marker`, …), whose models are open-ended pools. Those are the
/// part where a from-scratch grammar could invent findings, and they wait for
/// their own increment.
const SVG_CLOSED_MODEL_ELEMENTS: &[&str] = &[
    "circle", "ellipse", "image", "line", "path", "polygon", "polyline", "rect",
    // `tref` belongs here rather than with the text elements below: it names
    // the text it renders through `xlink:href`, so its own model is closed and
    // carries no character data. Measured — `<tref>loose</tref>` is
    // `text not allowed here` and `<tref><desc/></tref>` is clean.
    "tref", "use",
];

/// The text elements, whose model is a **mixed pool**: character data, the
/// descriptive and animation elements, and a short closed list of text
/// children. Unlike the graphics elements above, text here is content rather
/// than a mistake.
const SVG_TEXT_CONTENT_ELEMENTS: &[&str] = &["text", "textPath", "tspan"];

/// What a text element may contain beyond character data, descriptive and
/// animation elements.
///
/// `textPath` is **not** in this list because it is not allowed everywhere the
/// others are: SVG 1.1 admits it directly inside `<text>` and nowhere else, so
/// it is handled as its own case. Measured both ways — `text > textPath` is
/// clean, `tspan > textPath` and `textPath > textPath` are errors.
const SVG_TEXT_CHILDREN: &[&str] = &["a", "altGlyph", "tref", "tspan"];

/// The gradients, whose model is the descriptive and animation elements plus
/// `<stop>`, and no character data.
const SVG_GRADIENT_ELEMENTS: &[&str] = &["linearGradient", "radialGradient"];

/// `<stop>` takes **animation elements only** — not even a `<desc>`, which is
/// the one cell here that memory would have got wrong. Measured:
/// `<stop><set/></stop>` is clean, `<stop><desc/></stop>` is
/// `element "desc" not allowed here`.
const SVG_ANIMATION_ONLY_ELEMENTS: &[&str] = &["stop"];

/// `<clipPath>` takes the **shape** elements plus `<use>` — and not every
/// graphics element: `<g>` and `<image>` are both errors inside one, measured.
const SVG_CLIP_PATH_CHILDREN: &[&str] = &[
    "circle", "ellipse", "line", "path", "polygon", "polyline", "rect", "text", "use",
];

/// The filter primitives a `<filter>` may hold directly. Read off the
/// `<element name>` declarations in `schema/20/rng/svg/svg*filter*.rng` rather
/// than from memory, minus the four sub-children handled below.
/// `feDropShadow` is included on purpose: it is SVG 2, so at EPUB 2 the
/// *vocabulary* check already rejects it, and listing it here keeps the
/// content model from adding a second finding for one mistake.
const SVG_FILTER_PRIMITIVES: &[&str] = &[
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDropShadow",
    "feFlood",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMorphology",
    "feOffset",
    "feSpecularLighting",
    "feTile",
    "feTurbulence",
];

/// The three filter sub-elements whose models are **stricter than everything
/// else here**: they admit neither descriptive nor animation elements, only
/// their own children. Measured one book per cell — `<feMerge><desc/>` and
/// `<feMerge><animate/>` are both errors, which is what forced `animation` to
/// become a field rather than an assumption.
///
/// **Known gap, deliberately outside this increment:** the two lighting
/// primitives also *require* a light-source child, so epubcheck reports two
/// findings for `<feDiffuseLighting><rect/></feDiffuseLighting>` — the
/// containment error this table catches, and an `incomplete` cardinality
/// error it does not. Cardinality is a different axis (its attribute
/// equivalent is `check_required_attributes`) and wants its own measured
/// increment; being one lower is the safe direction meanwhile.
/// The container elements. Their model is **not** the open-ended pool it was
/// taken for in the previous increment: it is descriptive and animation
/// elements plus every SVG element that does not belong to a specific parent,
/// and no character data.
const SVG_CONTAINER_ELEMENTS: &[&str] = &[
    "a", "defs", "g", "marker", "mask", "pattern", "svg", "switch", "symbol",
];

/// The SVG elements a container may **not** hold, because each belongs to a
/// particular parent — the gradient's `stop`, the text children, the filter
/// primitives and their sub-children, the font internals, and
/// `animateMotion`'s `mpath`.
///
/// Stated as an exclusion rather than as a 39-name allow-list because that is
/// the actual rule, and because it cannot drift from `SVG_ELEMENTS` the way a
/// second copy would. **Verified in both directions, one book per name:** all
/// 41 of these are rejected inside a `<g>`, and all 39 remaining SVG element
/// names are accepted there.
const SVG_NON_CONTAINER_CHILDREN: &[&str] = &[
    "altGlyph",
    "altGlyphItem",
    "definition-src",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "font-face-format",
    "font-face-name",
    "font-face-src",
    "font-face-uri",
    "glyph",
    "glyphRef",
    "hkern",
    "missing-glyph",
    "mpath",
    "stop",
    "textPath",
    "tref",
    "tspan",
    "vkern",
];

/// How often, and in what order, a model's `extra` children may appear.
///
/// Three values because three were measured, one book per cell (#93). The
/// default is `Any`; the other two exist for the filter sub-elements, whose
/// models are the only ordered or counted ones in the closed slice.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cardinality {
    /// `extra` may appear any number of times in any order — `<feMerge>` with
    /// two `<feMergeNode>` is clean.
    Any,
    /// `extra` is an **ordered** sequence, each member at most once:
    /// `feFuncR?, feFuncG?, feFuncB?, feFuncA?`. Both halves measured —
    /// `feFuncR` then `feFuncG` is clean, `feFuncG` then `feFuncR` is not, and
    /// neither is a repeated `feFuncR`. Same shape as the EPUB 2 table row
    /// groups in #48, and the same trap: a set would have accepted all three.
    OrderedOptional,
    /// **Exactly one** of `extra`. Zero makes the parent incomplete; a second
    /// one is "not allowed here", which is how epubcheck words it too.
    ExactlyOneOf,
}

const SVG_FILTER_SUBMODELS: &[(&str, &[&str])] = &[
    (
        "feComponentTransfer",
        // **In SVG's order, not alphabetical.** The model is the ordered
        // sequence `feFuncR?, feFuncG?, feFuncB?, feFuncA?`, so sorting this
        // list - harmless while the cardinality was a set membership test -
        // inverts the rule the moment it becomes `OrderedOptional`: it made
        // `feFuncR` followed by `feFuncG` an error and let the reversed pair
        // through. Caught by the two cells that measure exactly that.
        &["feFuncR", "feFuncG", "feFuncB", "feFuncA"],
    ),
    (
        "feDiffuseLighting",
        &["feDistantLight", "fePointLight", "feSpotLight"],
    ),
    ("feMerge", &["feMergeNode"]),
    (
        "feSpecularLighting",
        &["feDistantLight", "fePointLight", "feSpotLight"],
    ),
];

/// The closed half of the SVG content model: what a graphics element may
/// contain.
///
/// Normative in EPUB 2 and informative in EPUB 3, the split the whole SVG
/// family takes — `schema/20/rng/content.rng` includes the SVG 1.1 modules
/// directly while EPUB 3 runs the strict grammar with `isNormative=false`.
/// What one element of the closed slice may contain.
struct SvgModel {
    /// Whether the descriptive elements are admitted.
    descriptive: bool,
    /// Whether the animation elements are admitted. False only for the filter
    /// sub-elements, which was measured rather than assumed — it had been an
    /// unconditional `continue` until `<feMerge><animate/></feMerge>` turned
    /// out to be an error.
    animation: bool,
    /// Element children beyond the descriptive and animation ones.
    extra: &'static [&'static str],
    /// How often and in what order `extra` may appear.
    cardinality: Cardinality,
    /// Whether character data is content rather than a mistake.
    text: bool,
    /// A container: `extra` is not a list but "every SVG element that does not
    /// belong to a specific parent", i.e. the complement of
    /// [`SVG_NON_CONTAINER_CHILDREN`].
    container: bool,
}

/// The content model of `name`, or `None` when it is outside this slice.
///
/// Animation elements are admitted by all four shapes, so they are not listed
/// here.
fn svg_model(name: &str) -> Option<SvgModel> {
    let base = SvgModel {
        descriptive: true,
        animation: true,
        extra: &[],
        cardinality: Cardinality::Any,
        text: false,
        container: false,
    };
    if SVG_CONTAINER_ELEMENTS.contains(&name) {
        return Some(SvgModel {
            container: true,
            // `<a>` is the one container that also carries character data,
            // in both of its contexts: `<g><a>loose</a></g>` and
            // `<text><a>a</a></text>` are both clean, while `<g>loose</g>` is
            // not. It is still a container otherwise — a `<tspan>` inside one
            // is rejected even when the `<a>` sits in a `<text>`. Four cells,
            // and the first version of this table got it wrong, which an
            // assertion written for the text family caught.
            text: name == "a",
            ..base
        });
    }
    if SVG_CLOSED_MODEL_ELEMENTS.contains(&name) {
        return Some(base);
    }
    if SVG_TEXT_CONTENT_ELEMENTS.contains(&name) {
        return Some(SvgModel {
            extra: SVG_TEXT_CHILDREN,
            text: true,
            ..base
        });
    }
    if SVG_GRADIENT_ELEMENTS.contains(&name) {
        return Some(SvgModel {
            extra: &["stop"],
            ..base
        });
    }
    if SVG_ANIMATION_ONLY_ELEMENTS.contains(&name) {
        return Some(SvgModel {
            descriptive: false,
            ..base
        });
    }
    if name == "clipPath" {
        return Some(SvgModel {
            extra: SVG_CLIP_PATH_CHILDREN,
            ..base
        });
    }
    if name == "filter" {
        return Some(SvgModel {
            extra: SVG_FILTER_PRIMITIVES,
            ..base
        });
    }
    if let Some((_, children)) = SVG_FILTER_SUBMODELS.iter().find(|(e, _)| *e == name) {
        let cardinality = match name {
            // `feFuncR?, feFuncG?, feFuncB?, feFuncA?` - ordered, each once.
            "feComponentTransfer" => Cardinality::OrderedOptional,
            // The lighting primitives take exactly one light source.
            "feDiffuseLighting" | "feSpecularLighting" => Cardinality::ExactlyOneOf,
            // `<feMerge>` is `(feMergeNode)*` - repeats are fine.
            _ => Cardinality::Any,
        };
        return Some(SvgModel {
            descriptive: false,
            animation: false,
            extra: children,
            cardinality,
            ..base
        });
    }
    None
}

pub(crate) fn check_content_model(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    let (id, severity) = if is_epub3 {
        (RSC_025, Severity::Usage)
    } else {
        (RSC_005, Severity::Error)
    };
    for parent in svg_root.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some(SVG_NS)
            && (is_epub3 || epub2_grammar_reaches(*n))
    }) {
        let pname = parent.tag_name().name();
        if pname == "font" {
            check_font_content(parent, path, id, severity, report);
            continue;
        }
        // The four closed shapes, each measured cell by cell rather than
        // read off a grammar. `None` means this element is not part of the
        // slice — every container is, deliberately.
        let Some(model) = svg_model(pname) else {
            continue;
        };
        let is_text_element = model.text;
        // Position reached in an `OrderedOptional` sequence, and the count for
        // `ExactlyOneOf`. Both answer "has this child already been used up",
        // which is why one pass settles order, repetition and the
        // required-child question together.
        let mut seq_at = 0usize;
        let mut chosen = 0usize;
        for child in parent.children() {
            if child.is_element() {
                // A foreign-namespaced child is somebody else's question, and
                // `metadata` legitimately carries one — leave the whole class
                // alone rather than guess at it.
                if child.tag_name().namespace() != Some(SVG_NS) {
                    continue;
                }
                let cname = child.tag_name().name();
                if model.animation && SVG_ANIMATION_ELEMENTS.contains(&cname) {
                    continue;
                }
                if model.descriptive && SVG_DESCRIPTIVE_ELEMENTS.contains(&cname) {
                    continue;
                }
                if model.container {
                    // A name the vocabulary does not know is that check's
                    // finding, not this one's - reporting it here as well
                    // would give one mistake two findings.
                    if !SVG_ELEMENTS.contains(&cname)
                        || !SVG_NON_CONTAINER_CHILDREN.contains(&cname)
                    {
                        continue;
                    }
                }
                if let Some(pos) = model.extra.iter().position(|e| *e == cname) {
                    match model.cardinality {
                        Cardinality::Any => continue,
                        Cardinality::OrderedOptional => {
                            // Out of order, or a repeat: both land before the
                            // position already reached, and epubcheck words
                            // both as "not allowed here" rather than as a
                            // cardinality message.
                            if pos >= seq_at {
                                seq_at = pos + 1;
                                continue;
                            }
                        }
                        Cardinality::ExactlyOneOf => {
                            chosen += 1;
                            if chosen == 1 {
                                continue;
                            }
                        }
                    }
                }
                // `textPath` is admitted directly inside `<text>` and nowhere
                // else, `<tspan>` and `<textPath>` included, so it cannot live
                // in `SVG_TEXT_CHILDREN` with the rest.
                if cname == "textPath" && pname == "text" {
                    continue;
                }
                report.push_at_pos(
                    id,
                    severity,
                    format!("element \"{cname}\" is not allowed inside \"{pname}\""),
                    path,
                    Position::of(child),
                );
            } else if !is_text_element
                && child.is_text()
                && child
                    .text()
                    .is_some_and(|t| !crate::xmlext::is_xml_blank(t))
            {
                // Indentation is not content: epubcheck accepts a `<rect>`
                // spread over three lines around its `<desc>`, and rejects
                // `<rect>hello</rect>`. Measured both ways.
                report.push_at_pos(
                    id,
                    severity,
                    format!("text is not allowed inside \"{pname}\""),
                    path,
                    Position::of(child),
                );
            }
        }
        // The one genuinely *missing*-child rule in the closed slice: a
        // lighting primitive with no light source is incomplete. Measured -
        // `<feMerge/>`, `<feComponentTransfer/>`, `<clipPath/>`, `<filter/>`
        // and an empty gradient are all clean, so nothing else here requires
        // a child.
        if model.cardinality == Cardinality::ExactlyOneOf && chosen == 0 {
            let expected = model
                .extra
                .iter()
                .map(|e| format!("\"{e}\""))
                .collect::<Vec<_>>()
                .join(", ");
            report.push_at_pos(
                id,
                severity,
                format!("element \"{pname}\" has incomplete content; expected one of {expected}"),
                path,
                Position::of(parent),
            );
        }
    }
}

/// `font` is the one SVG 1.1 element whose content is an ordered sequence
/// with required members: `(desc | title | metadata)*`, then exactly one
/// `font-face`, then exactly one `missing-glyph`, then any number of
/// `glyph`, `hkern` and `vkern`. The same at 2.0 and 3.0
/// (`svg-basic-font.rng`, `svg-basic-font.rnc`).
///
/// The counting follows epubcheck's, measured on 5.4.0 with 27 sequences,
/// inline and standalone, at both versions:
/// - A member that comes **too early**, before a required one (`glyph` with
///   no `missing-glyph` yet), is one finding. The required members it skipped
///   are then taken as given, so there is no "incomplete" finding after it.
/// - A member that **does not fit at all** (a second `font-face`, a `desc`
///   after `font-face`, a `rect`, an animation) is one finding and is
///   otherwise ignored.
/// - A `font` that ends with `font-face` or `missing-glyph` still owed is
///   one finding.
///
/// Unknown SVG names belong to the vocabulary check and foreign elements are
/// left alone, as everywhere else in the content model.
fn check_font_content(
    font: roxmltree::Node,
    path: &str,
    id: &'static str,
    severity: Severity,
    report: &mut Report,
) {
    const REQUIRED: [&str; 2] = ["font-face", "missing-glyph"];
    // How many of the two required members are behind us.
    let mut stage = 0usize;
    for child in font.children() {
        if child.is_text() {
            if child
                .text()
                .is_some_and(|t| !crate::xmlext::is_xml_blank(t))
            {
                report.push_at_pos(
                    id,
                    severity,
                    "text is not allowed inside \"font\"",
                    path,
                    Position::of(child),
                );
            }
            continue;
        }
        if !child.is_element()
            || child.tag_name().namespace() != Some(SVG_NS)
            || !SVG_ELEMENTS.contains(&child.tag_name().name())
        {
            continue;
        }
        let cname = child.tag_name().name();
        // The stage this member belongs at: before the required pair, at one
        // of them, or after both.
        let at = match cname {
            "desc" | "title" | "metadata" => Some(0),
            "font-face" => Some(0),
            "missing-glyph" => Some(1),
            "glyph" | "hkern" | "vkern" => Some(2),
            _ => None,
        };
        let fits = match (cname, at) {
            ("desc" | "title" | "metadata", Some(0)) => stage == 0,
            (_, Some(at)) => at >= stage,
            (_, None) => false,
        };
        if !fits {
            report.push_at_pos(
                id,
                severity,
                format!("element \"{cname}\" is not allowed inside \"font\""),
                path,
                Position::of(child),
            );
            continue;
        }
        let Some(at) = at else { continue };
        if at > stage {
            report.push_at_pos(
                id,
                severity,
                format!(
                    "element \"{cname}\" comes before the required \"{}\" in \"font\"",
                    REQUIRED[stage]
                ),
                path,
                Position::of(child),
            );
        }
        if cname == "font-face" || cname == "missing-glyph" {
            stage = at + 1;
        } else if at == 2 {
            stage = 2;
        }
    }
    if stage < 2 {
        report.push_at_pos(
            id,
            severity,
            format!(
                "element \"font\" has incomplete content; \"{}\" is missing",
                REQUIRED[stage]
            ),
            path,
            Position::of(font),
        );
    }
}

pub(crate) fn check_required_attributes(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    check_attributes(svg_root, path, is_epub3, true, report);
}

/// The same grammar for a standalone SVG document, where epubcheck applies
/// less of it at 3.0: measured on 5.4.0 with a `.svg` in the spine, a
/// `<rect>` or `<image>` without `width`/`height` draws nothing at 3.0 and
/// RSC-005 at 2.0, while a bad `preserveAspectRatio` is RSC-025 at 3.0 and
/// RSC-005 at 2.0. So EPUB 3 asks only about values, and EPUB 2 asks it all.
/// Neither ran on a standalone SVG before.
pub(crate) fn check_standalone_attributes(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    report: &mut Report,
) {
    check_attributes(svg_root, path, is_epub3, !is_epub3, report);
}

fn check_attributes(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
    include_required: bool,
    report: &mut Report,
) {
    // The same question, asked at both versions with different force -
    // epubcheck runs the SVG 1.1 grammar normatively for EPUB 2 and
    // informatively for EPUB 3, so the id and severity differ while the
    // condition does not. Measured on one book per version: `<rect/>` draws
    // `RSC-005` at 2.0 and `RSC-025 Informative parsing error: …` at 3.0.
    //
    // Running it at 3.0 was missed when this check was added, because the
    // gap that prompted it was an EPUB 2 one. The rule slug is deliberately
    // the same at both versions: it is one finding whose normativity moves,
    // and `severity` already carries that.
    let (id, severity) = if is_epub3 {
        (RSC_025, Severity::Usage)
    } else {
        (RSC_005, Severity::Error)
    };
    for n in svg_root
        .descendants()
        .filter(|_| include_required)
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
        .filter(|n| is_epub3 || epub2_grammar_reaches(*n))
    {
        let Ok(i) = SVG_REQUIRED_ATTRS.binary_search_by_key(&n.tag_name().name(), |(e, _)| e)
        else {
            continue;
        };
        // One finding per element listing everything absent, not one per
        // attribute: epubcheck reports `missing required attributes "height"
        // and "width"` as a single message, and a per-attribute split would
        // double the count on the commonest case.
        let missing: Vec<&str> = SVG_REQUIRED_ATTRS[i]
            .1
            .iter()
            .copied()
            .filter(|a| !n.has_attr_no_ns(a))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let name = n.tag_name().name();
        let list = missing
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(" and ");
        let plural = if missing.len() > 1 { "s" } else { "" };
        report.push_node(
            id,
            severity,
            format!("SVG element \"{name}\" has no required attribute{plural} {list}"),
            path,
            n,
            "opf.content_document.svg_missing_required_attribute",
            missing.iter().map(|a| (*a).to_string()).collect(),
        );
    }
    // The attributes whose value epubcheck constrains, from the generated
    // table of the book's version. Reported in the
    // same pass and with the same severity split as the required attributes:
    // one grammar, one normativity.
    for n in svg_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
        .filter(|n| is_epub3 || epub2_grammar_reaches(*n))
    {
        for attr in n.attributes().filter(|a| a.namespace().is_none()) {
            let name = attr.name();
            let value = attr.value();
            // At 2.0 an attribute its element does not take is reported as
            // such by `check_attribute_vocabulary`, and that is all epubcheck
            // says of it: its grammar never reaches the value. Judging the
            // value too would make one fault two findings.
            if !is_epub3 && !is_recognized_attribute(name, n.tag_name().name(), false) {
                continue;
            }
            let valid = svg_value_is_valid(n.tag_name().name(), name, value, is_epub3);
            if valid != Some(false) {
                continue;
            }
            report.push_full(
                id,
                severity,
                format!("value of attribute \"{name}\" is invalid"),
                path,
                Position::of_attr(n, attr),
                "opf.content_document.svg_invalid_attribute_value",
                vec![name.to_string(), value.to_string()],
            );
        }
    }

    // The six elements whose required attribute is the **namespaced**
    // `xlink:href`. Kept as a second pass rather than folded into the table
    // above because `has_attr_no_ns` cannot see a namespaced attribute at all
    // - which is why these were missing rather than merely unlisted, and why
    // the grammar extraction that produced the rest of the table missed every
    // one of them.
    //
    // **EPUB 2 only.** epubcheck's EPUB 3 copy of the SVG 1.1 modules
    // (`schema/30/mod/svg11/svg-xlink-attrib.rnc`) makes `xlink:href`
    // optional even in `SVG.XLinkRequired.attrib`, beside an optional SVG 2
    // `href`. So at 3.0 nothing requires it, and `<use href="#s"/>`, a bare
    // `<use/>` and a `<textPath>` without one drew a usage RSC-025 here and
    // nothing there (measured on 5.4.0, inline, one book each).
    for n in svg_root
        .descendants()
        .filter(|_| !is_epub3 && include_required)
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
        .filter(|n| epub2_grammar_reaches(*n))
        .filter(|n| SVG_REQUIRED_XLINK_HREF.contains(&n.tag_name().name()))
    {
        if n.attribute((XLINK_NS, "href")).is_some() {
            continue;
        }
        let name = n.tag_name().name();
        report.push_node(
            id,
            severity,
            format!("SVG element \"{name}\" has no required attribute \"xlink:href\""),
            path,
            n,
            "opf.content_document.svg_missing_required_attribute",
            vec![name.to_string(), "xlink:href".to_string()],
        );
    }
}

#[cfg(test)]
mod tests {

    /// EPUB 2 places XHTML in SVG by NVDL, and none of it was asked. Each
    /// verdict below was measured on epubcheck 5.4.0, one book per shape.
    fn epub2_rules(svg: &str, inline: bool) -> Vec<&'static str> {
        let doc = crate::ocf::parse_xml(svg).unwrap();
        let root = doc.root_element();
        let mut report = Report::default();
        for fo in root
            .descendants()
            .filter(|n| n.tag_name().name() == "foreignObject")
        {
            check_foreign_object(fo, svg, root, "s.svg", false, inline, &mut report);
        }
        check_epub2_xhtml_children(root, "s.svg", &mut report);
        check_required_attributes(root, "s.svg", false, &mut report);
        check_content_model(root, "s.svg", false, &mut report);
        report.messages.iter().filter_map(|m| m.rule).collect()
    }

    const S: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1">"#;
    const X: &str = r#"xmlns="http://www.w3.org/1999/xhtml""#;
    const M: &str = r#"xmlns="http://www.w3.org/1998/Math/MathML""#;
    const RECT: &str = r#"<rect width="1" height="1"/>"#;

    /// EPUB 2's SVG 1.1 attribute rules that the flat EPUB 3 list hid,
    /// each measured on 5.4.0 inline and standalone.
    #[test]
    fn epub2_svg_attributes_follow_svg_1_1_not_the_epub3_list() {
        let ids = |body: &str, is_epub3: bool| -> Vec<&'static str> {
            let svg = format!(r#"{S}{body}</svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_attribute_vocabulary(doc.root_element(), "s.svg", is_epub3, &mut report);
            check_required_attributes(doc.root_element(), "s.svg", is_epub3, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };
        const XL: &str = r#"xmlns:xlink="http://www.w3.org/1999/xlink""#;
        for attr in [
            r#"role="img""#,
            r#"aria-label="x""#,
            r#"lang="en""#,
            r#"data-x="1""#,
        ] {
            let rect = format!(r#"<rect width="1" height="1" {attr}/>"#);
            assert_eq!(ids(&rect, false), [RSC_005], "{attr} at 2.0");
            assert!(
                ids(&rect, true).iter().all(|id| *id != RSC_005),
                "{attr} at 3.0"
            );
        }
        // A malformed `data-` suffix is the same plain RSC-005 at 2.0.
        assert_eq!(
            ids(r#"<rect width="1" height="1" data-FOO="1"/>"#, false),
            [RSC_005]
        );
        // `lang` is SVG 1.1's on `glyph`.
        assert!(
            ids(
                r#"<defs><font horiz-adv-x="1"><glyph lang="en"/></font></defs>"#,
                false
            )
            .is_empty()
        );
        // `xlink:href` is required on `image` and `a` too, and a plain `href`
        // does not stand in for it: both faults are reported.
        assert_eq!(ids(r#"<image width="1" height="1"/>"#, false), [RSC_005]);
        assert_eq!(
            ids(r#"<a><rect width="1" height="1"/></a>"#, false),
            [RSC_005]
        );
        assert_eq!(
            ids(r#"<image width="1" height="1" href="i.png"/>"#, false),
            [RSC_005, RSC_005]
        );
        assert!(
            ids(
                &format!(r#"<image {XL} width="1" height="1" xlink:href="i.png"/>"#),
                false
            )
            .is_empty()
        );
        assert!(ids(r#"<image width="1" height="1"/>"#, true).is_empty());
    }

    /// The enumerated values at 2.0, each shape among the 8,325 values probed
    /// against 5.4.0 (2026-10-08): `token` values collapse whitespace and keep
    /// case, `version` is a `string` compared as written, and `fill`, `type`,
    /// `operator` and the `font-*` properties mean different things on
    /// different elements.
    #[test]
    fn epub2_svg_values_follow_the_generated_table() {
        let invalid = |body: &str, is_epub3: bool| -> usize {
            let svg = format!(r#"{S}{body}</svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_required_attributes(doc.root_element(), "s.svg", is_epub3, &mut report);
            report
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.content_document.svg_invalid_attribute_value"))
                .count()
        };
        for (body, at_2) in [
            (r#"<rect width="1" height="1" visibility="hide"/>"#, 1),
            (r#"<rect width="1" height="1" visibility=" hidden "/>"#, 0),
            (r#"<rect width="1" height="1" visibility="Hidden"/>"#, 1),
            (
                r#"<rect width="1" height="1" visibility="hidden hidden"/>"#,
                1,
            ),
            (r#"<g display="flex"/>"#, 1),
            (r#"<g display="inline-table"/>"#, 0),
            (r#"<animate attributeName="x" fill="freeze"/>"#, 0),
            (r#"<animate attributeName="x" fill="red"/>"#, 1),
            (r#"<rect width="1" height="1" fill="red"/>"#, 0),
            (r#"<g font-weight="bold"/>"#, 0),
            (r#"<g font-weight="heavy"/>"#, 1),
            (
                r#"<defs><font horiz-adv-x="1"><font-face font-weight="all"/></font></defs>"#,
                0,
            ),
            (r#"<svg version="1.1"/>"#, 0),
            (r#"<svg version="1.0"/>"#, 1),
            (r#"<svg version=" 1.1"/>"#, 1),
            (r#"<g class="a b"/>"#, 0),
            (r#"<g class=""/>"#, 1),
            (r#"<g class="a/b"/>"#, 1),
            (
                r#"<defs><filter><feColorMatrix type="matrix"/></filter></defs>"#,
                0,
            ),
            (
                r#"<defs><filter><feTurbulence type="matrix"/></filter></defs>"#,
                1,
            ),
            (
                r#"<defs><filter><feMorphology operator="xor"/></filter></defs>"#,
                1,
            ),
            (r#"<script type="anything"/>"#, 0),
        ] {
            assert_eq!(invalid(body, false), at_2, "{body} at 2.0");
        }
        // The five checked at 3.0 compare as tokens too: a padded value was a
        // false positive at both versions until the table came.
        for body in [
            r#"<rect width="1" height="1" fill-rule=" evenodd&#9;"/>"#,
            r#"<rect width="1" height="1" clip-rule=" nonzero "/>"#,
            r#"<svg preserveAspectRatio=" none "/>"#,
        ] {
            assert_eq!(invalid(body, false), 0, "{body} at 2.0");
            assert_eq!(invalid(body, true), 0, "{body} at 3.0");
        }
        assert_eq!(
            invalid(r#"<rect width="1" height="1" fill-rule="junk"/>"#, true),
            1
        );
    }

    /// `font`'s ordered content, counted as epubcheck counts it: each row
    /// was one of the 27 sequences measured on 5.4.0 (2026-10-08), with the
    /// element each finding names.
    #[test]
    fn font_content_is_an_ordered_sequence_with_two_required_members() {
        let named = |inner: &str, is_epub3: bool| -> Vec<String> {
            let svg = format!(r#"{S}<font horiz-adv-x="1">{inner}</font></svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_content_model(doc.root_element(), "s.svg", is_epub3, &mut report);
            report
                .messages
                .iter()
                .map(|m| match m.text.starts_with("text ") {
                    true => "text".to_string(),
                    false => m.text.split('"').nth(1).unwrap_or_default().to_string(),
                })
                .collect()
        };
        const FF: &str = "<font-face/>";
        const MG: &str = "<missing-glyph/>";
        for (inner, expected) in [
            ("", &["font"][..]),
            (FF, &["font"]),
            (MG, &["missing-glyph"]),
            ("<glyph/>", &["glyph"]),
            ("<glyph/><glyph/>", &["glyph"]),
            ("<font-face/><glyph/>", &["glyph"]),
            ("<font-face/><hkern k=\"1\"/>", &["hkern"]),
            (
                "<missing-glyph/><font-face/>",
                &["missing-glyph", "font-face"],
            ),
            (
                "<font-face/><glyph/><missing-glyph/>",
                &["glyph", "missing-glyph"],
            ),
            ("<font-face/><desc>d</desc><missing-glyph/>", &["desc"]),
            ("<font-face/><font-face/><missing-glyph/>", &["font-face"]),
            (
                "<font-face/><missing-glyph/><missing-glyph/>",
                &["missing-glyph"],
            ),
            ("<font-face/><missing-glyph/><desc>d</desc>", &["desc"]),
            ("<rect width=\"1\" height=\"1\"/>", &["rect", "font"]),
            (
                "<animate attributeName=\"x\"/><font-face/><missing-glyph/>",
                &["animate"],
            ),
            ("<font-face/><missing-glyph/>text", &["text"]),
            ("<desc>d</desc>", &["font"]),
            ("<font-face/><missing-glyph/>", &[]),
            (
                "<title>t</title><metadata/><font-face/><missing-glyph/>",
                &[],
            ),
            (
                "<desc>d</desc><font-face/><missing-glyph/><glyph/><hkern k=\"1\"/><glyph/>",
                &[],
            ),
            ("  <font-face/>  <missing-glyph/>  ", &[]),
            ("<font-face/><missing-glyph/><x:y xmlns:x=\"urn:x\"/>", &[]),
        ] {
            assert_eq!(named(inner, false), expected, "{inner} at 2.0");
            assert_eq!(named(inner, true), expected, "{inner} at 3.0");
        }
    }

    /// The values at 3.0, each shape among the 10,314 probed against 5.4.0
    /// (2026-10-08). Most enumerations there are `string` values compared as
    /// written, a few are tokens, and the attributes SVG 2 added are known.
    #[test]
    fn epub3_svg_values_follow_their_own_grammar() {
        let invalid = |body: &str| -> usize {
            let svg = format!(r#"{S}{body}</svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_required_attributes(doc.root_element(), "s.svg", true, &mut report);
            report
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.content_document.svg_invalid_attribute_value"))
                .count()
        };
        for (body, expected) in [
            // `string`: exact, so padding is invalid at 3.0 (valid at 2.0).
            (r#"<rect width="1" height="1" visibility="hidden"/>"#, 0),
            (r#"<rect width="1" height="1" visibility=" hidden "/>"#, 1),
            (r#"<rect width="1" height="1" visibility="collapse"/>"#, 0),
            // `token`: padding is fine.
            (r#"<rect width="1" height="1" fill-rule=" evenodd "/>"#, 0),
            // Both kinds in one choice.
            (r#"<text writing-mode="lr">a</text>"#, 0),
            (r#"<text writing-mode=" lr">a</text>"#, 1),
            (r#"<text writing-mode=" inherit ">a</text>"#, 0),
            (r#"<svg version="1.2"/>"#, 0),
            (r#"<svg version="2.0"/>"#, 1),
            (r#"<g class=""/>"#, 0),
            (r#"<g lang=""/>"#, 0),
            (r#"<g lang=" en-US"/>"#, 0),
            (r#"<g lang="en_US"/>"#, 1),
            (
                r#"<rect width="1" height="1" paint-order="stroke markers"/>"#,
                0,
            ),
            (
                r#"<rect width="1" height="1" paint-order="normal fill"/>"#,
                1,
            ),
            (r#"<svg preserveAspectRatio="defer xMidYMid meet"/>"#, 0),
            (r#"<svg preserveAspectRatio="defer"/>"#, 1),
            (r#"<svg zoomAndPan=" magnify"/>"#, 1),
            (r#"<view zoomAndPan=" magnify"/>"#, 0),
            (
                r#"<defs><filter><feComposite in2="a" operator="lighter"/></filter></defs>"#,
                0,
            ),
            (
                r#"<defs><filter><feBlend in2="a" mode="hue"/></filter></defs>"#,
                0,
            ),
        ] {
            assert_eq!(invalid(body), expected, "{body}");
        }
        // SVG 2's attributes are names the EPUB 3 grammar knows and EPUB 2's
        // does not.
        for attr in ["paint-order", "transform-box", "transform-origin"] {
            assert!(is_recognized_attribute(attr, "rect", true), "{attr} at 3.0");
            assert!(
                !is_recognized_attribute(attr, "rect", false),
                "{attr} at 2.0"
            );
        }
    }

    /// Every value rule names an attribute some element takes, and every
    /// element a rule is scoped to takes it.
    #[test]
    fn the_svg11_value_table_agrees_with_the_attribute_table() {
        use svg11::Scope;
        let takes = |e: &str, a: &str| is_recognized_attribute(a, e, false);
        for (attr, scope, _) in svg11::SVG11_VALUES {
            let els: &[&str] = match scope {
                Scope::All => &[],
                Scope::Only(els) | Scope::AllBut(els) => els,
            };
            assert!(els.iter().all(|e| takes(e, attr)), "{attr}");
            assert!(
                svg11::SVG11_ATTRIBUTES.iter().any(|(e, _)| takes(e, attr)),
                "{attr}"
            );
        }
        assert!(svg11::SVG11_VALUES.windows(2).all(|w| w[0].0 <= w[1].0));
        for (attr, scope, _) in svg11::SVG30_VALUES {
            if let Scope::Only(els) | Scope::AllBut(els) = scope {
                assert!(els.iter().all(|e| takes_attribute_at_3(e, attr)), "{attr}");
            }
        }
        assert!(svg11::SVG30_VALUES.windows(2).all(|w| w[0].0 <= w[1].0));
        assert!(svg11::SVG30_ADDED.windows(2).all(|w| w[0].0 < w[1].0));
    }

    /// An attribute is judged against its own element's list at both
    /// versions: RSC-005 at 2.0, RSC-025 at 3.0. Each row was among the pairs
    /// probed against 5.4.0 (see `svg11`), 21,060 at 2.0 and 21,894 at 3.0.
    #[test]
    fn svg_attributes_are_judged_per_element() {
        let count = |body: &str, is_epub3: bool| -> usize {
            let svg = format!(r#"{S}{body}</svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_attribute_vocabulary(doc.root_element(), "s.svg", is_epub3, &mut report);
            report.messages.len()
        };
        for (body, rejected) in [
            (r#"<rect width="1" height="1" font-size="9"/>"#, 1),
            (r#"<rect width="1" height="1" text-anchor="end"/>"#, 1),
            (r#"<image width="1" height="1" fill="red"/>"#, 1),
            (r#"<g x="1"/>"#, 1),
            (
                r#"<defs><linearGradient><stop offset="0" fill="red"/></linearGradient></defs>"#,
                1,
            ),
            (r#"<text font-size="9" text-anchor="end">a</text>"#, 0),
            (r#"<g font-size="9" fill="red" stroke="red"/>"#, 0),
            (r#"<rect width="1" height="1" fill="red" opacity="1"/>"#, 0),
            (
                r#"<defs><linearGradient><stop offset="0" stop-color="red"/></linearGradient></defs>"#,
                0,
            ),
        ] {
            assert_eq!(count(body, false), rejected, "{body} at 2.0");
            assert_eq!(count(body, true), rejected, "{body} at 3.0");
        }
        // What only 3.0 adds, and the ARIA and `role` it allows.
        for (body, at_2, at_3) in [
            (r#"<rect width="1" height="1" paint-order="fill"/>"#, 1, 0),
            (r#"<rect width="1" height="1" tabindex="0"/>"#, 1, 0),
            (r#"<symbol width="1"/>"#, 1, 0),
            (r#"<rect width="1" height="1" aria-label="x"/>"#, 1, 0),
            (r#"<style aria-label="x"/>"#, 1, 1),
            (r#"<rect width="1" height="1" role="img"/>"#, 1, 0),
            (r#"<defs role="img"/>"#, 1, 1),
            (r#"<g transform-box="fill-box"/>"#, 1, 0),
        ] {
            assert_eq!(count(body, false), at_2, "{body} at 2.0");
            assert_eq!(count(body, true), at_3, "{body} at 3.0");
        }
        // An attribute its element does not take is one finding, not a
        // second one for its value, at both versions.
        let both = |body: &str, is_epub3: bool| -> Vec<String> {
            let svg = format!(r#"{S}{body}</svg>"#);
            let doc = crate::ocf::parse_xml(&svg).unwrap();
            let mut report = Report::default();
            check_attribute_vocabulary(doc.root_element(), "s.svg", is_epub3, &mut report);
            check_required_attributes(doc.root_element(), "s.svg", is_epub3, &mut report);
            report.messages.iter().map(|m| m.text.clone()).collect()
        };
        assert_eq!(
            both(r#"<g preserveAspectRatio="bad"/>"#, false),
            [r#"attribute "preserveAspectRatio" not allowed here"#]
        );
        assert_eq!(
            both(r#"<g preserveAspectRatio="bad"/>"#, true),
            [r#"attribute "preserveAspectRatio" not allowed here"#]
        );
        assert_eq!(
            both(r#"<svg preserveAspectRatio="bad"/>"#, false),
            [r#"value of attribute "preserveAspectRatio" is invalid"#]
        );
    }

    /// The generated table is sorted for its binary search, and its union is
    /// the flat list minus what only EPUB 3 adds.
    #[test]
    fn the_svg11_table_is_sorted_and_agrees_with_the_flat_list() {
        let table = svg11::SVG11_ATTRIBUTES;
        assert_eq!(table.len(), 81);
        assert!(table.windows(2).all(|w| w[0].0 < w[1].0));
        let union: std::collections::BTreeSet<&str> = table
            .iter()
            .flat_map(|(_, groups)| groups.iter().flat_map(|g| g.iter().copied()))
            .collect();
        let flat: std::collections::BTreeSet<&str> = SVG_ATTRIBUTES
            .iter()
            .copied()
            .filter(|a| !SVG3_ONLY_ATTRIBUTES.contains(a))
            .collect();
        assert_eq!(union, flat);
    }

    #[test]
    fn epub2_standalone_foreign_object_takes_svg_alone() {
        let fo = |c: &str| {
            format!(r#"{S}<foreignObject width="1" height="1">{c}</foreignObject></svg>"#)
        };
        let not_allowed = "svg.foreign_object.child_not_allowed";
        assert!(epub2_rules(&fo(""), false).is_empty());
        assert!(epub2_rules(&fo("<svg/>"), false).is_empty());
        assert!(epub2_rules(&fo(&format!("<math {M}/>")), false).is_empty());
        assert_eq!(epub2_rules(&fo("hello"), false), [not_allowed]);
        assert_eq!(epub2_rules(&fo(RECT), false), [not_allowed]);
        assert_eq!(
            epub2_rules(&fo(&format!("<body {X}><p>x</p></body>")), false),
            [not_allowed]
        );
        assert_eq!(
            epub2_rules(&fo(&format!("hello<p {X}>x</p>")), false),
            [not_allowed, not_allowed]
        );
    }

    #[test]
    fn epub2_inline_foreign_object_takes_body_or_flow_with_foreign_cut_out() {
        let fo = |c: &str| {
            format!(r#"{S}<foreignObject width="1" height="1">{c}</foreignObject></svg>"#)
        };
        let bad = "svg.foreign_object.schema_violation";
        for ok in [
            "hello".to_string(),
            "<svg/>".to_string(),
            format!("<body {X}><p>x</p></body>"),
            format!("<body {X}><p>a</p></body><body {X}><p>b</p></body>"),
            format!("<p {X}>x</p><span {X}>y</span>"),
            // Cut out by NVDL, so clean here although MathML in an EPUB 2
            // body is not (#92), and an SVG `rect` in a `p` is never checked.
            format!("<p {X}><math {M}><mi>x</mi></math></p>"),
            format!("<p {X}>a<rect xmlns=\"http://www.w3.org/2000/svg\"/></p>"),
            format!("<ul {X}><math {M}/><li>a</li></ul>"),
            format!("<p {X} xmlns:x=\"urn:x\" x:a=\"1\">a</p>"),
        ] {
            assert!(epub2_rules(&fo(&ok), true).is_empty(), "{ok}");
        }
        for invalid in [
            format!("<body {X}>x</body>"),
            format!("<title {X}>t</title>"),
            format!("<p {X}><blink>x</blink></p>"),
            // Cutting the MathML out leaves the `ul` without an `li`.
            format!("<ul {X}><math {M}/></ul>"),
        ] {
            assert_eq!(epub2_rules(&fo(&invalid), true), [bad], "{invalid}");
        }
        assert_eq!(
            epub2_rules(&fo(RECT), true),
            ["svg.foreign_object.child_not_allowed"]
        );
    }

    #[test]
    fn epub2_xhtml_outside_foreign_object_is_rsc_005_and_svg_below_it_is_not_checked() {
        let child = "svg.content_model.xhtml_child";
        assert_eq!(
            epub2_rules(&format!("{S}<p {X}>x</p></svg>"), true),
            [child]
        );
        assert_eq!(
            epub2_rules(&format!("{S}<g><div {X}><p>x</p></div></g></svg>"), true),
            [child]
        );
        assert_eq!(
            epub2_rules(&format!("{S}<desc><p {X}>x</p></desc></svg>"), false),
            [child]
        );
        assert!(
            epub2_rules(&format!("{S}<g><x:foo xmlns:x=\"urn:x\"/></g></svg>"), true).is_empty()
        );
        // Below an XHTML element inside a foreignObject the SVG 1.1 grammar
        // never looks: a `rect` without its required attributes, an `svg` it
        // would reject, an XHTML child of a `g` - all clean.
        let nested = |c: &str| {
            format!(
                r#"{S}<foreignObject width="1" height="1"><p {X}><svg xmlns="http://www.w3.org/2000/svg">{c}</svg></p></foreignObject></svg>"#
            )
        };
        assert!(epub2_rules(&nested("<rect/>"), true).is_empty());
        assert!(
            epub2_rules(
                &nested(r#"<rect width="1" height="1"><circle r="1"/></rect>"#),
                true
            )
            .is_empty()
        );
        assert!(epub2_rules(&nested(&format!("<g><p {X}>x</p></g>")), true).is_empty());
    }

    /// A **standalone** SVG's own references were resolved by nothing.
    /// `resource_refs` existed, but only to answer "was this resource
    /// referenced" for OPF-097; nothing asked whether the reference itself
    /// resolves, so a book whose SVG points at a missing image validated clean
    /// here and drew `RSC-007` from epubcheck.
    ///
    /// Error at **both** versions, measured one book each — this is
    /// `ResourceReferencesChecker`'s question rather than the grammar's, so it
    /// does not take the normative/informative split the rest of the SVG
    /// family does.
    ///
    /// The two walks now share `for_each_reference` so they cannot drift, and
    /// the fragment and remote arms are asserted because those are the shapes
    /// a naive "does this file exist" check gets wrong.
    #[test]
    fn a_standalone_svg_reference_that_does_not_resolve_is_reported() {
        use std::collections::{HashMap, HashSet};
        let mut index = HashMap::new();
        index.insert("EPUB/there.png".to_string(), "EPUB/there.png".to_string());
        index.insert("EPUB/pic.svg".to_string(), "EPUB/pic.svg".to_string());

        let refs = |body: &str| -> Vec<String> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_resource_references(
                d.root_element(),
                "EPUB/pic.svg",
                "EPUB",
                &index,
                &HashSet::new(),
                true,
                &mut report,
            );
            report
                .messages
                .iter()
                .map(|m| m.params.first().cloned().unwrap_or_default())
                .collect()
        };

        assert_eq!(
            refs(r#"<image xlink:href="missing.png" width="1" height="1"/>"#),
            vec!["missing.png".to_string()]
        );
        assert!(
            refs(r#"<image xlink:href="there.png" width="1" height="1"/>"#).is_empty(),
            "a reference that resolves is silent"
        );
        // A fragment into this document is not a container reference, a remote
        // one has no container path, and an empty href addresses the document
        // itself. None of the three is a missing resource.
        for body in [
            r##"<use xlink:href="#z"/>"##,
            r#"<image xlink:href="https://example.org/a.png" width="1" height="1"/>"#,
            r#"<image xlink:href="" width="1" height="1"/>"#,
        ] {
            assert!(refs(body).is_empty(), "should be silent: {body}");
        }
        // The fragment is stripped before the lookup, so a resolvable target
        // with one stays silent and an unresolvable one is still named whole.
        assert!(refs(r##"<use xlink:href="there.png#z"/>"##).is_empty());
        assert_eq!(
            refs(r##"<use xlink:href="gone.png#z"/>"##),
            vec!["gone.png#z".to_string()]
        );
    }

    /// A reference to a manifest item whose file is missing is RSC-001 at the
    /// manifest and nothing here; epubcheck registers every declared item, so
    /// RSC-007 (`checkUndeclaredReference`) is reserved for a target that is
    /// neither declared nor in the container. Probed one book per shape
    /// against 5.4.0.
    #[test]
    fn a_standalone_svg_reference_to_a_declared_but_missing_item_is_silent() {
        use std::collections::{HashMap, HashSet};
        let index: HashMap<String, String> = HashMap::new();
        let mut declared = HashSet::new();
        declared.insert("EPUB/pic.png".to_string());
        let run = |declared: &HashSet<String>| {
            let d = doc(r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title>
                        <image xlink:href="pic.png" width="1" height="1"/></svg>"#);
            let mut report = Report::new();
            check_resource_references(
                d.root_element(),
                "EPUB/pic.svg",
                "EPUB",
                &index,
                declared,
                true,
                &mut report,
            );
            report.messages.len()
        };
        assert_eq!(run(&declared), 0, "declared and missing: RSC-001 only");
        assert_eq!(run(&HashSet::new()), 1, "undeclared and missing: RSC-007");
    }

    /// **The datatypes constrain nothing; the enumerations do.**
    /// `schema/20/rng/svg/svg-datatypes.rng` declares 22 datatypes and 17 of
    /// them are a plain `<data type="string"/>`: length, number, opacity,
    /// transform list, path data and URI carry their meaning in documentation
    /// and constrain nothing. The enumerated values are a separate table
    /// (`epub2_svg_values_follow_the_generated_table`).
    ///
    /// Probed rather than inferred, against a control that confirms the
    /// document is validated at all: `width="abc"`, `width="-5"`, `r="-1"`,
    /// `opacity="junk"`, `transform="notafunction(1)"` and an invalid path
    /// `d` are every one of them clean in epubcheck. Constraining those would
    /// be inventing errors it does not make.
    #[test]
    fn svg_datatype_values_are_unconstrained() {
        let bad = |body: &str| -> Vec<String> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
                   <title>s</title>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_required_attributes(d.root_element(), "c.xhtml", false, &mut report);
            report
                .messages
                .iter()
                .filter(|m| m.rule == Some("opf.content_document.svg_invalid_attribute_value"))
                .map(|m| m.text.clone())
                .collect()
        };

        for body in [
            r#"<rect width="1" height="1" fill-rule="junk"/>"#,
            r#"<rect width="1" height="1" clip-rule="junk"/>"#,
            r#"<rect width="1" height="1" externalResourcesRequired="maybe"/>"#,
            // On a nested `svg`, which takes the attribute: `rect` does not,
            // and there epubcheck never reaches the value (see
            // `epub2_svg_attributes_are_judged_per_element`).
            r#"<svg preserveAspectRatio="junk"/>"#,
            // A valid keyword with an invalid qualifier, and a valid one with
            // something after it - both fail the grammar's pattern.
            r#"<svg preserveAspectRatio="xMidYMid tight"/>"#,
            r#"<svg preserveAspectRatio="xMidYMid meet extra"/>"#,
        ] {
            assert_eq!(bad(body).len(), 1, "should be one finding: {body}");
        }

        for body in [
            r#"<rect width="1" height="1" fill-rule="evenodd"/>"#,
            r#"<rect width="1" height="1" clip-rule="inherit"/>"#,
            r#"<rect width="1" height="1" externalResourcesRequired="true"/>"#,
            // The three spellings the local shelf actually uses, 309 times
            // across 260 books.
            r#"<svg preserveAspectRatio="xMidYMid meet"/>"#,
            r#"<svg preserveAspectRatio="none"/>"#,
            r#"<svg preserveAspectRatio="xMidYMid"/>"#,
            // The seventeen unconstrained datatypes. Every one of these is
            // clean in epubcheck, measured, and reporting them would be a
            // restrictive divergence rather than a gap closed.
            r#"<rect width="abc" height="1"/>"#,
            r#"<rect width="-5" height="1"/>"#,
            r#"<circle r="-1"/>"#,
            r#"<rect width="50%" height="1"/>"#,
            r#"<g opacity="junk"><rect width="1" height="1"/></g>"#,
            r#"<g transform="notafunction(1)"><rect width="1" height="1"/></g>"#,
            r#"<path d="totally invalid"/>"#,
        ] {
            assert!(bad(body).is_empty(), "should be silent: {body}");
        }
    }

    /// The reference set is exactly the one `OPSHandler` registers, and it is
    /// much smaller than "every `href` in the document" (issue #130).
    ///
    /// The wide walk this replaces produced five errors on markup epubcheck
    /// says nothing about, each measured one book at a time against 5.3.0:
    /// `<textPath xlink:href>`, `<tref xlink:href>`, a gradient's
    /// `xlink:href`, and — against 5.3.0 — a **plain** `href` on `<use>` or
    /// `<image>`. That last pair moved: we filed it as w3c/epubcheck#1677 and
    /// 5.4.0 registers the unprefixed SVG 2 spelling, so at EPUB 3 it belongs
    /// in the registered set and at EPUB 2 it still does not.
    ///
    /// Asserted as a whole set rather than case by case, because the failure
    /// this guards against is the set quietly widening again.
    #[test]
    fn the_svg_reference_set_is_the_one_epubcheck_registers() {
        use std::collections::{HashMap, HashSet};
        let index: HashMap<String, String> = HashMap::new();
        let named_at = |body: &str, is_epub3: bool| -> Vec<String> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_resource_references(
                d.root_element(),
                "EPUB/pic.svg",
                "EPUB",
                &index,
                &HashSet::new(),
                is_epub3,
                &mut report,
            );
            report
                .messages
                .iter()
                .map(|m| m.params.first().cloned().unwrap_or_default())
                .collect()
        };
        let named = |body: &str| named_at(body, true);

        // Registered: the four xlink elements and the two paint properties.
        for body in [
            r#"<use xlink:href="gone.svg"/>"#,
            r#"<image xlink:href="gone.svg"/>"#,
            r#"<a xlink:href="gone.svg"><rect/></a>"#,
            r#"<font-face-uri xlink:href="gone.svg"/>"#,
            r#"<rect fill="url(gone.svg)"/>"#,
            r#"<rect stroke="url(gone.svg)"/>"#,
            // The unprefixed SVG 2 spelling, registered since epubcheck
            // 5.4.0 (w3c/epubcheck#1677, ours) — and *not* at EPUB 2, which
            // the block after this one holds.
            r#"<use href="gone.svg"/>"#,
            r#"<image href="gone.svg"/>"#,
            // `clip-path` likewise: the `case` for it existed upstream with
            // nothing registering the reference, which is what we filed as
            // w3c/epubcheck#1678 and 5.4.0 fixed.
            r#"<rect clip-path="url(gone.svg)"/>"#,
        ] {
            assert_eq!(
                named(body),
                vec!["gone.svg".to_string()],
                "registered: {body}"
            );
        }

        // Not registered — every one of these was a false positive.
        for body in [
            r#"<text><textPath xlink:href="gone.svg"/></text>"#,
            r#"<text><tref xlink:href="gone.svg"/></text>"#,
            r#"<linearGradient xlink:href="gone.svg"/>"#,
            r#"<line marker-start="url(gone.svg)"/>"#,
            // `checkPaint` takes the value literally: it must be exactly
            // `url(…)`, so a paint list registers nothing. Probed.
            r#"<rect fill="url(gone.svg) red"/>"#,
        ] {
            assert!(named(body).is_empty(), "not registered: {body}");
        }

        // EPUB 2 keeps the 5.3.0 answer, because the fix lives in
        // epubcheck's EPUB 3 handler alone. Reading the plain spelling here
        // would re-create two of the false positives above.
        for body in [r#"<use href="gone.svg"/>"#, r#"<image href="gone.svg"/>"#] {
            assert!(
                named_at(body, false).is_empty(),
                "EPUB 2 registers only xlink:href: {body}"
            );
            assert_eq!(named_at(body, true).len(), 1, "but EPUB 3 does: {body}");
        }
    }

    /// RSC-012 on a same-document fragment that names no id, over that same
    /// set — the half of issue #130 that was implemented and reverted.
    ///
    /// The revert was right: walked over every `href`, we reported two
    /// references on `epubtype-valid.svg` where epubcheck reports one, the
    /// extra being a `<textPath>`. Narrowing the set is what makes the check
    /// correct rather than tuning the check itself.
    #[test]
    fn a_same_document_fragment_must_name_a_real_id() {
        let frags = |body: &str| -> Vec<String> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title><rect id="here"/>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_fragments(d.root_element(), "EPUB/pic.svg", true, &mut report);
            report
                .messages
                .iter()
                .map(|m| m.params.first().cloned().unwrap_or_default())
                .collect()
        };

        assert_eq!(
            frags(r##"<use xlink:href="#nosuch"/>"##),
            vec!["#nosuch".to_string()]
        );
        assert_eq!(
            frags(r##"<rect fill="url(#nosuch)"/>"##),
            vec!["#nosuch".to_string()]
        );
        assert!(
            frags(r##"<use xlink:href="#here"/>"##).is_empty(),
            "a fragment that resolves is silent"
        );
        // The whole point of the narrowing: this one is not a reference.
        assert!(
            frags(r##"<text><textPath xlink:href="#nosuch"/></text>"##).is_empty(),
            "a textPath fragment is not registered, so it is not checked"
        );
        // A cross-document fragment belongs to the other document.
        assert!(frags(r##"<use xlink:href="other.svg#nosuch"/>"##).is_empty());
    }

    /// The required-attribute table, extended from seven elements to
    /// twenty-six, plus six that require the namespaced `xlink:href`.
    ///
    /// Found while probing the containers, because those probes used
    /// deliberately bare elements and epubcheck kept reporting a second
    /// finding the containment question had nothing to do with. Twenty-five
    /// cells, one book each against 5.3.0.
    ///
    /// **The grammar extraction that produced most of the table was a
    /// candidate generator, not an authority.** It missed every one of the
    /// `xlink:href` rows, because those attributes are declared in their own
    /// module rather than in the element's `attlist` — and that is the same
    /// set our own `has_attr_no_ns` cannot see, which is why they were
    /// missing rather than merely unlisted.
    #[test]
    fn the_svg_required_attribute_table_covers_more_than_the_shapes() {
        let missing = |body: &str| -> Vec<String> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_required_attributes(d.root_element(), "c.xhtml", false, &mut report);
            report.messages.iter().map(|m| m.text.clone()).collect()
        };

        for (body, want) in [
            (r#"<g><animate/></g>"#, "\"attributeName\""),
            (r#"<g><set/></g>"#, "\"attributeName\""),
            (r#"<g><animateTransform/></g>"#, "\"attributeName\""),
            (r#"<g><animateColor/></g>"#, "\"attributeName\""),
            (
                r#"<defs><linearGradient id="g"><stop/></linearGradient></defs>"#,
                "\"offset\"",
            ),
            (r#"<g><foreignObject/></g>"#, "\"height\" and \"width\""),
            (
                r#"<defs><filter id="f"><feBlend/></filter></defs>"#,
                "\"in2\"",
            ),
            (
                r#"<defs><filter id="f"><feConvolveMatrix/></filter></defs>"#,
                "\"kernelMatrix\" and \"order\"",
            ),
            (
                r#"<defs><filter id="f"><feComponentTransfer><feFuncR/></feComponentTransfer></filter></defs>"#,
                "\"type\"",
            ),
            (r#"<script/>"#, "\"type\""),
            (r#"<defs><color-profile/></defs>"#, "\"name\""),
            (r#"<defs><font><hkern/></font></defs>"#, "\"horiz-adv-x\""),
            // The namespaced ones, which the no-namespace lookup cannot see.
            (r#"<g><use/></g>"#, "\"xlink:href\""),
            (r#"<text x="0" y="0"><tref/></text>"#, "\"xlink:href\""),
            (r#"<text x="0" y="0"><textPath/></text>"#, "\"xlink:href\""),
            (r#"<g><cursor/></g>"#, "\"xlink:href\""),
            (
                r#"<defs><filter id="f"><feImage/></filter></defs>"#,
                "\"xlink:href\"",
            ),
        ] {
            let got = missing(body);
            assert!(
                got.iter().any(|m| m.contains(want)),
                "{body} should name {want}, got {got:?}"
            );
        }

        // Present, so silent - the control that keeps the assertions above
        // from passing against a check that always fires.
        for body in [
            r##"<g><use xlink:href="#z"/></g>"##,
            r#"<g><animate attributeName="x"/></g>"#,
            r#"<defs><linearGradient id="g"><stop offset="0"/></linearGradient></defs>"#,
            // Probed and requiring nothing, listed so the next reader does not
            // re-probe them.
            r#"<g><animateMotion/></g>"#,
            r#"<defs><pattern id="pt"/></defs>"#,
            r#"<defs><marker id="mk"/></defs>"#,
        ] {
            assert!(missing(body).is_empty(), "should be silent: {body}");
        }
    }

    /// An SVG `id` that is empty or holds XML whitespace: RSC-025 everywhere
    /// in EPUB 3, plus RSC-005 when the SVG is inline. Rows measured on 5.4.0,
    /// one book each, standalone and inline. See `check_html_ids`.
    #[test]
    fn an_svg_id_with_whitespace_is_informative_and_inline_normative() {
        let ids = |id: &str, inline: bool| -> Vec<&'static str> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><rect id="{id}" width="1" height="1"/></svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_html_ids(d.root_element(), "s.svg", inline, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };
        for bad in [" a ", " a", "a b", "", "a&#9;b", "a&#10;"] {
            assert_eq!(ids(bad, false), vec![RSC_025], "standalone {bad:?}");
            assert_eq!(ids(bad, true), vec![RSC_005, RSC_025], "inline {bad:?}");
        }
        for ok in ["a", "1a", "a:b", "-a", "a\u{a0}", "\u{3000}a"] {
            assert!(ids(ok, false).is_empty(), "standalone {ok:?}");
            assert!(ids(ok, true).is_empty(), "inline {ok:?}");
        }
    }

    /// Eleven ordinary SVG 1.1 element names were missing from
    /// [`SVG_ELEMENTS`], so we reported `RSC-025` for markup epubcheck
    /// accepts — a false positive, at usage level, that had been there all
    /// along.
    ///
    /// Nothing found it because no book on the shelf uses SVG fonts,
    /// `altGlyph` or a colour profile, so `compare` never had a chance. It
    /// surfaced only from extracting the element declarations out of
    /// `schema/20/rng/svg/*.rng` and diffing them against this list while
    /// sizing the EPUB 2 half of #93 — and the diff was done *before* turning
    /// the list into an error, which is the only reason it did not ship as
    /// eleven wrong errors instead of eleven wrong usage notes.
    ///
    /// Each was confirmed silent in epubcheck 5.3.0 on its own book.
    /// `data-*` is allowed on SVG, and its *suffix* is still judged.
    ///
    /// epubcheck's own `data-attribute-valid.svg`, whose title is "data-\*
    /// attributes are allowed" and on which it reports nothing; we reported
    /// RSC-025. The vocabulary list could never have carried the rule — an
    /// open-ended family is not a vocabulary entry.
    ///
    /// **Settled by probe, not by reading the grammar**, which is the part
    /// worth keeping: the SVG schema files contain no `data-` at all, so the
    /// reason is not visible in them. One book per shape against 5.3.0 gives
    /// all six answers below, and the `zzz-foo` control is what makes the
    /// silence on `data-epub` a rule rather than an absence — it proves the
    /// grammar is applied to this document at all.
    ///
    /// The two HTM_061 rows are why accepting the shape is not the whole fix.
    /// `htm::check_dom` judges the suffix for anything declared
    /// `application/xhtml+xml`, so inline SVG was always covered and a bare
    /// `.svg` file never is. Accepting the shape without this would have
    /// traded a wrong finding for silence, which is the worse of the two.
    /// The closed half of the SVG content model: a graphics element holds
    /// descriptive and animation elements and nothing else — no other
    /// element, no text, but **indentation whitespace is not text**.
    ///
    /// Fifteen cells measured against 5.3.0, one book each, and the whole
    /// point of the slice is that it is closed: it cannot fire where
    /// epubcheck is silent. The last two assertions are the ones that would
    /// have cost real books — a shape spread over several lines is the
    /// commonest formatting there is.
    #[test]
    fn a_graphics_element_holds_only_descriptive_and_animation_children() {
        let ids = |body: &str, is_epub3: bool| -> Vec<&'static str> {
            let xml = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"
                        xmlns:xlink="http://www.w3.org/1999/xlink"
                        viewBox="0 0 10 10"><title>s</title>{body}</svg>"#
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_content_model(d.root_element(), "c.xhtml", is_epub3, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };

        // Rejected, and the id moves with the version: normative in EPUB 2,
        // informative in EPUB 3.
        for body in [
            r#"<rect width="1" height="1"><circle r="1"/></rect>"#,
            r##"<use xlink:href="#s"><rect width="1" height="1"/></use>"##,
            r#"<image xlink:href="x.png" width="1" height="1"><rect width="1" height="1"/></image>"#,
            r#"<line x1="0" y1="0" x2="1" y2="1"><text x="0" y="0">t</text></line>"#,
            r#"<rect width="1" height="1">hello</rect>"#,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }

        // Accepted in both versions.
        for body in [
            r#"<rect width="1" height="1"><desc>d</desc></rect>"#,
            r#"<rect width="1" height="1"><set attributeName="x" to="1"/></rect>"#,
            r#"<image xlink:href="x.png" width="1" height="1"><title>t</title></image>"#,
            r#"<path d="M0 0"><metadata><x xmlns="urn:x"/></metadata></path>"#,
            r#"<polygon points="0,0 1,1"><animate attributeName="x" dur="1s"/></polygon>"#,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3: {body}");
        }

        // Indentation is not content — the one cell that could have cost a
        // false positive on a real book, since a shape spread over several
        // lines is ordinary formatting.
        assert!(
            ids(
                "<rect width=\"1\" height=\"1\">\n      <desc>d</desc>\n  </rect>",
                false
            )
            .is_empty(),
            "whitespace around a legal child is not loose text"
        );
        // The container elements are deliberately outside this slice: their
        // models are open-ended pools, and inventing one is how a
        // from-scratch grammar starts reporting things epubcheck does not.
        assert!(
            ids(
                r#"<g><rect width="1" height="1"/><circle r="1"/></g>"#,
                false
            )
            .is_empty(),
            "containers are not judged by this check"
        );

        // --- the text family. A mixed pool rather than a closed model:
        // character data is content here, not a mistake. Fourteen more cells
        // against 5.3.0, one book each.
        for body in [
            // `textPath` is admitted directly inside `<text>` and nowhere
            // else — not in a `<tspan>` and not in another `<textPath>`.
            r##"<text x="0" y="0"><tspan><textPath xlink:href="#pp">a</textPath></tspan></text>"##,
            r##"<text x="0" y="0"><textPath xlink:href="#pp"><textPath xlink:href="#pp">a</textPath></textPath></text>"##,
            r#"<text x="0" y="0"><rect width="1" height="1"/></text>"#,
            r#"<text x="0" y="0"><tspan><rect width="1" height="1"/></tspan></text>"#,
            // `tref` names the text it renders, so it carries none of its
            // own — it sits in the closed set, not the text pool.
            r##"<text x="0" y="0"><tref xlink:href="#tt">loose</tref></text>"##,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }
        for body in [
            r#"<text x="0" y="0"><tspan>a</tspan></text>"#,
            r##"<text x="0" y="0"><textPath xlink:href="#pp">a</textPath></text>"##,
            r#"<text x="0" y="0">plain</text>"#,
            r#"<text x="0" y="0"><desc>d</desc>a</text>"#,
            r##"<text x="0" y="0"><tref xlink:href="#tt"><desc>d</desc></tref></text>"##,
            r##"<text x="0" y="0"><a xlink:href="#tt">a</a></text>"##,
            r#"<text x="0" y="0"><tspan><tspan>a</tspan></tspan></text>"#,
            r##"<text x="0" y="0"><textPath xlink:href="#pp"><tspan>a</tspan></textPath></text>"##,
            r##"<text x="0" y="0"><altGlyph xlink:href="#tt">a</altGlyph></text>"##,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3 clean: {body}");
        }
        // --- gradients. A third shape: the descriptive and animation
        // elements plus `<stop>`, and no character data. Ten more cells.
        for body in [
            r#"<defs><linearGradient id="a"><rect width="1" height="1"/></linearGradient></defs>"#,
            r#"<defs><linearGradient id="a">loose</linearGradient></defs>"#,
            r#"<defs><radialGradient id="b"><rect width="1" height="1"/></radialGradient></defs>"#,
            // `<stop>` is a fourth shape: **animation elements only**, not
            // even a `<desc>`. This is the cell memory would have got wrong.
            r#"<defs><linearGradient id="a"><stop offset="0"><desc>d</desc></stop></linearGradient></defs>"#,
            r#"<defs><linearGradient id="a"><stop offset="0">loose</stop></linearGradient></defs>"#,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }
        for body in [
            r#"<defs><linearGradient id="a"><stop offset="0"/></linearGradient></defs>"#,
            r#"<defs><linearGradient id="a"><desc>d</desc></linearGradient></defs>"#,
            r#"<defs><linearGradient id="a"><animate attributeName="x" dur="1s"/></linearGradient></defs>"#,
            r#"<defs><radialGradient id="b"><stop offset="0"/></radialGradient></defs>"#,
            r#"<defs><linearGradient id="a"><stop offset="0"><set attributeName="offset" to="1"/></stop></linearGradient></defs>"#,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3 clean: {body}");
        }
        // --- clipPath and the filter family. Twenty more cells.
        //
        // `<clipPath>` takes the shape elements plus `<use>`, and not every
        // graphics element — `<g>` and `<image>` are errors inside one. The
        // filter sub-elements are stricter than anything else here: neither
        // descriptive nor animation, only their own children.
        for body in [
            r#"<defs><clipPath id="a"><g><rect width="1" height="1"/></g></clipPath></defs>"#,
            r##"<defs><clipPath id="a"><image xlink:href="x.png" width="1" height="1"/></clipPath></defs>"##,
            r#"<defs><filter id="f"><rect width="1" height="1"/></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge><rect width="1" height="1"/></feMerge></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge><desc>d</desc></feMerge></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge><animate attributeName="x" dur="1s"/></feMerge></filter></defs>"#,
            r#"<defs><filter id="f"><feComponentTransfer><desc>d</desc></feComponentTransfer></filter></defs>"#,
            // The light source keeps this cell about containment alone -
            // without it the parent is *also* incomplete, and epubcheck
            // reports two. Measured both ways.
            r#"<defs><filter id="f"><feDiffuseLighting><desc>d</desc><feDistantLight/></feDiffuseLighting></filter></defs>"#,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }
        for body in [
            r#"<defs><clipPath id="a"><rect width="1" height="1"/></clipPath></defs>"#,
            r#"<defs><clipPath id="a"><text x="0" y="0">t</text></clipPath></defs>"#,
            r##"<defs><clipPath id="a"><use xlink:href="#z"/></clipPath></defs>"##,
            r#"<defs><clipPath id="a"><desc>d</desc></clipPath></defs>"#,
            r#"<defs><clipPath id="a"><animate attributeName="x" dur="1s"/></clipPath></defs>"#,
            r#"<defs><filter id="f"><feGaussianBlur stdDeviation="1"/></filter></defs>"#,
            r#"<defs><filter id="f"><desc>d</desc></filter></defs>"#,
            r#"<defs><filter id="f"><animate attributeName="x" dur="1s"/></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge><feMergeNode/></feMerge></filter></defs>"#,
            r#"<defs><filter id="f"><feComponentTransfer><feFuncR type="identity"/></feComponentTransfer></filter></defs>"#,
            r#"<defs><filter id="f"><feDiffuseLighting><feDistantLight/></feDiffuseLighting></filter></defs>"#,
            r#"<defs><filter id="f"><feSpecularLighting><feSpotLight/></feSpecularLighting></filter></defs>"#,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3 clean: {body}");
        }
        // --- cardinality. Thirteen more cells, and the axis turned out to be
        // two rules rather than a family: the lighting primitives take
        // **exactly one** light source, and `feComponentTransfer`'s children
        // are an **ordered** at-most-once sequence. Everything else that could
        // have required a child does not - `<feMerge/>`, `<feComponentTransfer/>`,
        // `<clipPath/>`, `<filter/>` and an empty gradient are all clean.
        for body in [
            // No light source at all: the parent is incomplete.
            r#"<defs><filter id="f"><feDiffuseLighting/></filter></defs>"#,
            r#"<defs><filter id="f"><feSpecularLighting/></filter></defs>"#,
            // Two of them: the second is "not allowed here", which is how
            // epubcheck words it too - a second light source is a containment
            // error there, not a cardinality one.
            r#"<defs><filter id="f"><feDiffuseLighting><feDistantLight/><fePointLight/></feDiffuseLighting></filter></defs>"#,
            // Out of SVG's order, and a repeat: both rejected.
            r#"<defs><filter id="f"><feComponentTransfer><feFuncG type="identity"/><feFuncR type="identity"/></feComponentTransfer></filter></defs>"#,
            r#"<defs><filter id="f"><feComponentTransfer><feFuncR type="identity"/><feFuncR type="identity"/></feComponentTransfer></filter></defs>"#,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }
        for body in [
            r#"<defs><filter id="f"><feComponentTransfer><feFuncR type="identity"/><feFuncG type="identity"/></feComponentTransfer></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge><feMergeNode/><feMergeNode/></feMerge></filter></defs>"#,
            r#"<defs><filter id="f"><feMerge/></filter></defs>"#,
            r#"<defs><filter id="f"><feComponentTransfer/></filter></defs>"#,
            r#"<defs><clipPath id="a"/></defs>"#,
            r#"<defs><filter id="f"/></defs>"#,
            r#"<defs><linearGradient id="g"/></defs>"#,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3 clean: {body}");
        }

        // `feDropShadow` is SVG 2, so at EPUB 2 the *vocabulary* check rejects
        // it and the content model must not add a second finding for the same
        // mistake — which is why it is in the filter's allowed set.
        assert!(
            ids(
                r#"<defs><filter id="f"><feDropShadow dx="1" dy="1"/></filter></defs>"#,
                false
            )
            .is_empty(),
            "one mistake, one finding"
        );

        // --- the containers. Not the open-ended pool the earlier increment
        // took them for: descriptive and animation elements plus every SVG
        // element that does not belong to a specific parent, and no character
        // data. Verified in both directions with one book per name against
        // 5.3.0 — all 41 excluded names are rejected inside a `<g>`, all 39
        // remaining ones are accepted.
        for body in [
            r#"<g><stop offset="0"/></g>"#,
            r#"<g><tspan>a</tspan></g>"#,
            r#"<g><feBlend/></g>"#,
            r#"<g><feMergeNode/></g>"#,
            r#"<defs><stop offset="0"/></defs>"#,
            r#"<g>loose text</g>"#,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
            assert_eq!(ids(body, true), vec![RSC_025], "EPUB 3: {body}");
        }
        for body in [
            r#"<g><rect width="1" height="1"/></g>"#,
            r#"<g><text x="0" y="0">t</text></g>"#,
            r#"<g><defs><filter id="f"/></defs></g>"#,
            r#"<g><clipPath id="c"/></g>"#,
            r#"<g><linearGradient id="lg"/></g>"#,
            r#"<switch><rect width="1" height="1"/></switch>"#,
            r#"<mask id="m"><rect width="1" height="1"/></mask>"#,
            r#"<marker id="mk"><rect width="1" height="1"/></marker>"#,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
            assert!(ids(body, true).is_empty(), "EPUB 3 clean: {body}");
        }
        // An unknown name inside a container is the *vocabulary* check's
        // finding. Reporting it here as well would give one mistake two
        // findings, which is the shape a whole release was spent removing.
        assert!(
            ids(r#"<g><notarealsvgelement/></g>"#, false).is_empty(),
            "the vocabulary check owns unknown names"
        );
        // `<a>` is the one container that also carries character data, in both
        // of its contexts — and it is still a container otherwise. Four cells;
        // the first version of the table gave it the plain container model and
        // an assertion written for the text family caught it.
        for body in [
            r##"<g><a xlink:href="#z">loose</a></g>"##,
            r##"<text x="0" y="0"><a xlink:href="#z">a</a></text>"##,
            r##"<g><a xlink:href="#z"><rect width="1" height="1"/></a></g>"##,
        ] {
            assert!(ids(body, false).is_empty(), "EPUB 2 clean: {body}");
        }
        for body in [
            r##"<g><a xlink:href="#z"><tspan>x</tspan></a></g>"##,
            r##"<text x="0" y="0"><a xlink:href="#z"><tspan>x</tspan></a></text>"##,
        ] {
            assert_eq!(
                ids(body, false),
                vec![crate::ids::RSC_005],
                "EPUB 2: {body}"
            );
        }
    }

    #[test]
    fn svg_allows_data_attributes_and_still_checks_their_names() {
        let ids = |attrs: &str| -> Vec<&'static str> {
            let svg = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<svg viewBox="0 0 12 4" xmlns="http://www.w3.org/2000/svg" xml:lang="en">
<title>t</title><desc>d</desc><rect x="1" y="2" width="7" height="3" {attrs}/>
</svg>"#
            );
            let d = roxmltree::Document::parse(&svg).unwrap();
            let mut report = Report::default();
            check_attribute_vocabulary(d.root_element(), "c.svg", true, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };

        assert!(ids(r#"data-epub="allowed""#).is_empty(), "the plain case");
        assert!(
            ids(r#"data-a-b="x""#).is_empty(),
            "hyphens in the suffix are fine"
        );
        assert_eq!(
            ids(r#"data-="x""#),
            vec![crate::ids::HTM_061],
            "empty suffix"
        );
        assert_eq!(
            ids(r#"data-FOO="x""#),
            vec![crate::ids::HTM_061],
            "uppercase is not a valid data-* name"
        );
        assert_eq!(
            ids(r#"data="x""#),
            vec![RSC_025],
            "`data` with no hyphen is not the family at all"
        );
        assert_eq!(
            ids(r#"zzz-foo="x""#),
            vec![RSC_025],
            "the control: an unknown attribute is still rejected, so the check is live"
        );
    }

    #[test]
    fn the_svg_vocabulary_covers_all_of_svg_1_1() {
        for name in [
            "altGlyph",
            "altGlyphDef",
            "altGlyphItem",
            "animateColor",
            "color-profile",
            "definition-src",
            "font-face-format",
            "font-face-name",
            "font-face-src",
            "font-face-uri",
            "glyphRef",
        ] {
            for v3 in [true, false] {
                assert!(
                    is_recognized_element(name, v3),
                    "{name} is SVG 1.1 and epubcheck accepts it (epub3={v3})"
                );
            }
        }
        // The control: the check still has teeth. A name that is in no
        // version of SVG must still be recognised as unknown, or this test
        // would pass against a predicate that answers `true` for everything.
        for v3 in [true, false] {
            assert!(!is_recognized_element("notanelement", v3));
            assert!(!is_recognized_element("recct", v3));
        }
        // `feDropShadow` is SVG 2, and this is the one name whose answer
        // moves with the version: it is declared in
        // `schema/30/mod/svg11/svg-filter.rnc` and in none of
        // `schema/20/rng/svg/`. Both arms measured on their own book against
        // 5.3.0 — clean at 3.0, RSC-005 at 2.0 (#93). This assertion used to
        // read "epubcheck accepts it too", which was true only of EPUB 3.
        assert!(is_recognized_element("feDropShadow", true));
        assert!(!is_recognized_element("feDropShadow", false));
    }
    use super::*;
    use crate::report::Report;

    fn doc(xml: &str) -> roxmltree::Document<'_> {
        crate::ocf::parse_xml(xml).unwrap()
    }

    const XHTML_OPEN: &str = concat!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" ",
        "xmlns:svg=\"http://www.w3.org/2000/svg\" ",
        "xmlns:xlink=\"http://www.w3.org/1999/xlink\">"
    );

    /// `epub:type` placement is one of only three things epubcheck's
    /// *normative* SVG grammar enforces, and it uses an allowlist
    /// (`svg.renderable.elem`). We used a denylist reverse-engineered from
    /// the corpus, which agreed on everything the fixtures exercised and
    /// silently let `epub:type` through on every other recognized SVG
    /// element — `marker`, `linearGradient`, `clipPath` and the rest.
    #[test]
    fn epub_type_is_allowed_only_on_renderable_svg_elements() {
        let svg_with = |el: &str| {
            format!(
                "{XHTML_OPEN}<body><svg:svg xmlns:epub=\"http://www.idpf.org/2007/ops\">\
                 <svg:{el} epub:type=\"pagebreak\"/></svg:svg></body></html>"
            )
        };
        let flagged = |el: &str| {
            let xml = svg_with(el);
            let d = doc(&xml);
            let root = d
                .descendants()
                .find(|n| n.tag_name().name() == "svg")
                .unwrap();
            let mut report = Report::new();
            check_epub_attributes(root, "c.xhtml", &mut report);
            report
                .messages
                .iter()
                .any(|m| m.rule == Some("svg.epub_attributes.type_not_allowed"))
        };

        // On the list: renderable shape/text/structural elements.
        for ok in ["circle", "g", "path", "text", "tspan", "use", "a", "image"] {
            assert!(!flagged(ok), "epub:type is allowed on <{ok}>");
        }
        // Off it — recognized SVG elements the old denylist let through.
        for bad in [
            "marker",
            "linearGradient",
            "clipPath",
            "mask",
            "pattern",
            "stop",
            "metadata",
            "desc",
            "title",
            "defs",
        ] {
            assert!(flagged(bad), "epub:type is not allowed on <{bad}>");
        }
    }

    /// HTML's `alt` reaching into an SVG subtree - `<image alt="cover
    /// image">`, which calibre-style cover pages emit. SVG 1.1 has no such
    /// attribute on any element, so epubcheck's non-normative full SVG
    /// grammar reports it as `USAGE(RSC-025)` (Doitsu, MobileRead #138).
    #[test]
    fn svg_attribute_vocabulary_flags_alt_and_keeps_real_svg_attributes() {
        let attrs_on_image = |attrs: &str| {
            let xml = format!(
                "{XHTML_OPEN}<body><svg:svg viewBox=\"0 0 600 800\" width=\"100%\" \
                 height=\"100%\" preserveAspectRatio=\"xMidYMid meet\" version=\"1.1\">\
                 <svg:image {attrs} xlink:href=\"c.png\"/></svg:svg></body></html>"
            );
            let d = doc(&xml);
            let root = d
                .descendants()
                .find(|n| n.tag_name().name() == "svg")
                .unwrap();
            let mut report = Report::new();
            check_attribute_vocabulary(root, "c.xhtml", true, &mut report);
            report
                .messages
                .iter()
                .filter(|m| m.id == RSC_025)
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
        };
        // The same document at EPUB 2, where the SVG 1.1 grammar is
        // normative: the finding is `RSC-005` at error severity rather than
        // `RSC-025` usage (#93). This check ran at 3.0 only, on a comment
        // that read the absence of RSC-025 in EPUB 2 as an absence of any
        // opinion; epubcheck has one, and it is stricter.
        let epub2_ids = |attrs: &str| -> Vec<(&'static str, Severity)> {
            let xml = format!(
                "{XHTML_OPEN}<body><svg:svg viewBox=\"0 0 600 800\" width=\"100%\" \
                 height=\"100%\" preserveAspectRatio=\"xMidYMid meet\" version=\"1.1\">\
                 <svg:image {attrs} xlink:href=\"c.png\"/></svg:svg></body></html>"
            );
            let d = doc(&xml);
            let root = d
                .descendants()
                .find(|n| n.tag_name().name() == "svg")
                .unwrap();
            let mut report = Report::new();
            check_attribute_vocabulary(root, "c.xhtml", false, &mut report);
            report
                .messages
                .iter()
                .map(|m| (m.id, m.severity))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            epub2_ids("alt=\"cover image\" width=\"600\" height=\"800\""),
            vec![(crate::ids::RSC_005, Severity::Error)],
            "EPUB 2 runs the SVG grammar normatively"
        );
        assert!(
            epub2_ids("width=\"600\" height=\"800\"").is_empty(),
            "and stays silent on attributes that are real SVG"
        );

        assert_eq!(
            attrs_on_image("alt=\"cover image\" width=\"600\" height=\"800\"").len(),
            1,
            "`alt` is not an SVG attribute; the width/height beside it are"
        );
        // The root's own attributes are checked too, and every one of them
        // here is real SVG - a case-correct `viewBox`/`preserveAspectRatio`
        // must stay silent, since the lowercase spellings are what real
        // books actually get wrong (two on the local shelf).
        assert!(attrs_on_image("width=\"600\" height=\"800\"").is_empty());
        // Prefixed attributes are never this check's business: `xlink:href`
        // above, `epub:type` (check_epub_attributes owns it), and the
        // `inkscape:`/`sodipodi:` sets the grammar allows wholesale.
        assert!(attrs_on_image("class=\"c\" id=\"i\" style=\"x\" role=\"img\"").is_empty());
    }

    #[test]
    fn standalone_foreign_object_takes_flow_content_or_one_body() {
        // Each measured on 5.4.0 as a standalone .svg in the spine.
        let ids = |inner: &str| -> Vec<&'static str> {
            let xml = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\">\
                 <foreignObject width=\"1\" height=\"1\">{inner}</foreignObject></svg>"
            );
            let d = doc(&xml);
            let fo = d
                .descendants()
                .find(|n| n.tag_name().name() == "foreignObject")
                .unwrap();
            let mut report = Report::new();
            check_foreign_object(
                fo,
                &xml,
                d.root_element(),
                "x.svg",
                true,
                false,
                &mut report,
            );
            report.messages.iter().map(|m| m.id).collect()
        };
        const X: &str = "xmlns=\"http://www.w3.org/1999/xhtml\"";
        for ok in [
            format!("<p {X}>x</p>"),
            format!("<div {X}><p>x</p></div>"),
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mi>x</mi></math>".to_string(),
            "hello".to_string(),
            format!("<body {X}><p>x</p></body>"),
        ] {
            assert!(ids(&ok).is_empty(), "{ok}");
        }
        for bad in [
            format!("<body {X}><p>x</p></body><body {X}><p>y</p></body>"),
            format!("<title {X}>t</title>"),
        ] {
            assert_eq!(ids(&bad), vec![RSC_005], "{bad}");
        }
    }

    #[test]
    fn standalone_svg_properties_match_what_it_uses() {
        // Each row a whole book on 5.4.0 (30 in all; see `check_properties`).
        let ids = |body: &str, declared: &str| -> Vec<&'static str> {
            let xml = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\">{body}</svg>"
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_properties(d.root_element(), "x.svg", declared, &mut report);
            let mut v: Vec<_> = report.messages.iter().map(|m| m.id).collect();
            v.sort_unstable();
            v
        };
        let rect = "<rect width=\"1\" height=\"1\"/>";
        let script = "<script>var a=1;</script>";
        let onclick = "<rect width=\"1\" height=\"1\" onclick=\"f()\"/>";
        assert!(ids(rect, "").is_empty());
        assert_eq!(ids(rect, "scripted"), [OPF_015]);
        assert_eq!(ids(rect, "mathml switch"), [OPF_015]);
        assert_eq!(ids(rect, "remote-resources"), [OPF_018]);
        assert_eq!(ids(script, ""), [OPF_014]);
        assert_eq!(ids(onclick, ""), [OPF_014]);
        assert!(ids(script, "scripted").is_empty());
        assert_eq!(ids(script, "remote-resources"), [OPF_014, OPF_018B]);
        assert_eq!(ids(rect, "cover-image"), Vec::<&str>::new());
    }

    #[test]
    fn remote_images_uses_and_paints_in_svg_are_rsc_006() {
        let ids = |body: &str| -> Vec<&'static str> {
            let xml = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 1 1\">{body}</svg>"
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_remote_references(d.root_element(), "x.svg", true, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };
        const R: &str = "https://example.com/a";
        for bad in [
            format!("<image width=\"1\" height=\"1\" href=\"{R}.png\"/>"),
            format!("<image width=\"1\" height=\"1\" xlink:href=\"{R}.png\"/>"),
            format!("<use href=\"{R}.svg#s\"/>"),
            format!("<rect width=\"1\" height=\"1\" fill=\"url({R}.svg#g)\"/>"),
        ] {
            assert_eq!(ids(&bad), vec![RSC_006], "{bad}");
        }
        // A hyperlink may point anywhere.
        assert!(
            ids(&format!(
                "<a href=\"{R}.html\"><rect width=\"1\" height=\"1\"/></a>"
            ))
            .is_empty()
        );
    }

    #[test]
    fn an_svg_link_is_named_by_title_text_or_attributes_not_by_bare_text() {
        // OPSHandler30: xlink:title / aria-label (non-empty), or an SVG
        // <title> or <text> inside. epubcheck's four
        // `url-missing-resource-svg-a-*` fixtures hold a bare "link" and get
        // ACC-011 there.
        let acc = |a: &str| -> usize {
            let xml = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\">{a}</svg>"
            );
            let d = doc(&xml);
            let mut report = Report::new();
            check_link_labels(d.root_element(), "x.svg", &mut report);
            report.messages.len()
        };
        assert_eq!(acc("<a href=\"c.xhtml\">link</a>"), 1);
        assert_eq!(acc("<a href=\"c.xhtml\" aria-label=\"\">link</a>"), 1);
        assert_eq!(acc("<a href=\"c.xhtml\"><text>link</text></a>"), 0);
        assert_eq!(acc("<a href=\"c.xhtml\"><title>t</title></a>"), 0);
        assert_eq!(acc("<a href=\"c.xhtml\" xlink:title=\"t\"/>"), 0);
        assert_eq!(acc("<a href=\"c.xhtml\" aria-label=\"t\"/>"), 0);
    }

    #[test]
    fn a_standalone_svg_gets_values_at_3_and_everything_at_2() {
        // Measured on 5.4.0 with a .svg in the spine: at 3.0 a missing
        // width/height is silent and a bad preserveAspectRatio is RSC-025; at
        // 2.0 both are RSC-005.
        let ids = |body: &str, is_epub3: bool| -> Vec<&'static str> {
            let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" {body}");
            let d = doc(&xml);
            let mut report = Report::new();
            check_standalone_attributes(d.root_element(), "x.svg", is_epub3, &mut report);
            report.messages.iter().map(|m| m.id).collect()
        };
        let no_wh = "viewBox=\"0 0 1 1\"><rect x=\"1\"/></svg>";
        let bad_par = "viewBox=\"0 0 1 1\" preserveAspectRatio=\"bogus\"><rect width=\"1\" height=\"1\"/></svg>";
        assert!(ids(no_wh, true).is_empty());
        assert_eq!(ids(bad_par, true), vec![RSC_025]);
        assert_eq!(ids(no_wh, false), vec![RSC_005]);
        assert_eq!(ids(bad_par, false), vec![RSC_005]);
    }

    #[test]
    fn epub3_svg_never_requires_xlink_href() {
        let xml = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\">\
                   <use/><use href=\"#s\"/><text><textPath>t</textPath></text></svg>";
        let d = doc(xml);
        for (is_epub3, want) in [(true, 0), (false, 3)] {
            let mut report = Report::new();
            check_required_attributes(d.root_element(), "x.svg", is_epub3, &mut report);
            let n = report
                .messages
                .iter()
                .filter(|m| m.text.contains("xlink:href"))
                .count();
            assert_eq!(n, want, "is_epub3={is_epub3}");
        }
    }

    #[test]
    fn foreign_object_rejects_body_element() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <body><div>disallowed</div></body>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_rejects_invalid_attribute() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <p href=\"#error\">Hello</p>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_rejects_non_flow_content() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <title>Hello</title>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn foreign_object_accepts_flow_content() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <p>Hello</p>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn foreign_object_accepts_whitespace_only() {
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject> \
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            true,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn foreign_object_body_allowed_in_epub2() {
        // A real EPUB2 fixture, titled exactly "body allowed inside
        // foreignObject" - EPUB2's OPS/XHTML content model is more lenient
        // than EPUB3's here.
        let xml = format!(
            "{XHTML_OPEN}<body><svg:svg><svg:foreignObject>\
             <body><div>Part I:</div></body>\
             </svg:foreignObject></svg:svg></body></html>"
        );
        let d = doc(&xml);
        let fo = d
            .descendants()
            .find(|n| n.tag_name().name() == "foreignObject")
            .unwrap();
        let mut report = Report::new();
        check_foreign_object(
            fo,
            &xml,
            d.root_element(),
            "c.xhtml",
            false,
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn title_rejects_foreign_namespace_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><not:html xmlns:not=\"https://example.org\">x</not:html></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn title_rejects_nested_foreign_namespace_inside_xhtml_body() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><body xmlns=\"http://www.w3.org/1999/xhtml\">",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Inner</title></svg>",
            "</body></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        // Only the nested svg (and its own nested title) are foreign - the
        // xhtml <body> itself must not be flagged.
        assert!(!report.messages.is_empty());
    }

    #[test]
    fn title_accepts_bare_body_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><body xmlns=\"http://www.w3.org/1999/xhtml\">text</body></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn title_rejects_href_on_span() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title><span href=\"#error\" xmlns=\"http://www.w3.org/1999/xhtml\">t</span></title>",
            "</svg>"
        );
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_005]
        );
    }

    #[test]
    fn title_accepts_plain_text() {
        let xml = "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Plain text</title></svg>";
        let d = doc(xml);
        let title = d
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .unwrap();
        let mut report = Report::new();
        check_title_content(title, "c.xhtml", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn vocabulary_rejects_unknown_element() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<title>Title</title><foo>Invalid</foo>",
            "</svg>"
        );
        let d = doc(xml);
        let svg_root = d.root_element();
        let mut report = Report::new();
        check_vocabulary(svg_root, "c.xhtml", true, &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_025],
            "EPUB 3 runs the SVG grammar informatively"
        );
        // The same element is a normative RSC-005 in EPUB 2, because
        // `schema/20/rng/content.rng` includes the SVG 1.1 modules directly
        // (#93). Measured against 5.3.0 inline and standalone.
        let mut two = Report::new();
        check_vocabulary(svg_root, "c.xhtml", false, &mut two);
        assert_eq!(
            two.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![crate::ids::RSC_005],
            "EPUB 2 runs it normatively"
        );
        assert_eq!(two.messages[0].severity, Severity::Error);

        // `feDropShadow` is the one name whose recognition moves with the
        // version: SVG 2, present in `schema/30/mod/svg11/svg-filter.rnc` and
        // in none of `schema/20/rng/svg/`. Both arms measured on their own
        // book.
        let fd = doc(concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<defs><filter id=\"f\"><feDropShadow dx=\"1\" dy=\"1\"/></filter></defs>",
            "</svg>"
        ));
        let mut three = Report::new();
        check_vocabulary(fd.root_element(), "c.xhtml", true, &mut three);
        assert!(
            three.messages.is_empty(),
            "SVG 2 filter primitive, valid at 3.0"
        );
        let mut older = Report::new();
        check_vocabulary(fd.root_element(), "c.xhtml", false, &mut older);
        assert_eq!(
            older.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![crate::ids::RSC_005],
            "SVG 1.1 has no feDropShadow"
        );
    }

    #[test]
    fn vocabulary_accepts_svg_own_anchor_with_xlink() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">",
            "<desc>Example</desc>",
            "<a xlink:href=\"https://example.org\" xlink:title=\"example\" target=\"_blank\" rel=\"noreferrer\">link</a>",
            "</svg>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_vocabulary(d.root_element(), "c.xhtml", true, &mut report);
        check_vocabulary(d.root_element(), "c.xhtml", false, &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn vocabulary_ignores_foreign_namespaced_metadata_content() {
        let xml = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<metadata><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description/></rdf:RDF></metadata>",
            "</svg>"
        );
        let d = doc(xml);
        let mut report = Report::new();
        check_vocabulary(d.root_element(), "c.xhtml", true, &mut report);
        check_vocabulary(d.root_element(), "c.xhtml", false, &mut report);
        assert!(report.messages.is_empty());
    }
}
