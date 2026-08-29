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
/// `xlink:href` and plain `href` both, since SVG 2 allows the unprefixed
/// form; a fragment-only value addresses this document and is not a
/// container resource.
/// The attributes an SVG element can hold a resource reference in, walked once
/// so the two callers cannot drift apart: [`resource_refs`], which answers
/// "was this resource referenced" for OPF-097, and
/// [`check_resource_references`], which answers "does this reference resolve"
/// for RSC-007. The second was missing entirely — a standalone SVG pointing at
/// a file that is not in the container validated clean here and drew RSC-007
/// from epubcheck.
fn for_each_reference<'a>(
    root: roxmltree::Node<'a, 'a>,
    mut f: impl FnMut(roxmltree::Node<'a, 'a>, &'a str),
) {
    const XLINK: &str = "http://www.w3.org/1999/xlink";
    for n in root.descendants().filter(|n| n.is_element()) {
        for v in [n.attribute((XLINK, "href")), n.attr_no_ns("href")]
            .into_iter()
            .flatten()
        {
            f(n, v);
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
    report: &mut Report,
) {
    use crate::opf::{is_external, nfc, resolve};
    for_each_reference(svg_root, |n, v| {
        let v = v.trim();
        if v.is_empty() || v.starts_with('#') || is_external(v) || crate::opf::is_remote_url(v) {
            return;
        }
        let path_part = v.split('#').next().unwrap_or(v);
        if path_part.is_empty() {
            return;
        }
        if name_index.contains_key(&nfc(&resolve(base_dir, path_part))) {
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

pub(crate) fn resource_refs(svg_xml: &str, base_dir: &str) -> Vec<String> {
    use crate::opf::{is_external, nfc, resolve};
    let Ok(doc) = crate::ocf::parse_xml(svg_xml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for_each_reference(doc.root_element(), |_, v| {
        {
            let v = v.trim();
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
/// It is a flat union, not a per-element table: an attribute valid on some
/// SVG element but used on another passes here. That is a false negative,
/// and the deliberate trade — the same one `SVG_ELEMENTS` makes, and for
/// the same reason (`RSC-025` is usage-level, so a false positive costs
/// more than a miss). What it does catch is the class actually reported:
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

/// The four attributes our list carries beyond SVG 1.1. Diffing
/// `SVG_ATTRIBUTES` against the unprefixed `<attribute name>` declarations in
/// `schema/20/rng/svg/*.rng` gives **nothing missing and exactly these four
/// extra**; each was then probed on its own EPUB 2 and EPUB 3 book against
/// 5.3.0, and each is RSC-005 at 2.0 and clean at 3.0. Same shape as
/// [`SVG2_ONLY_ELEMENTS`].
const SVG3_ONLY_ATTRIBUTES: &[&str] = &["focusable", "href", "rel", "tabindex"];

fn is_recognized_attribute(name: &str, is_epub3: bool) -> bool {
    // `role` and `aria-*`: `epub-svg-strict-inc.rnc` folds `aria.global`
    // into `SVG.Core.attrib`, which covers the `aria-*` set but not `role`
    // itself. `role` is allowed here anyway - accepting it is a miss, and
    // rejecting an accessibility attribute that authors do put on SVG
    // would be the expensive direction of wrong.
    if !is_epub3 && SVG3_ONLY_ATTRIBUTES.contains(&name) {
        return false;
    }
    SVG_ATTRIBUTES.contains(&name) || name == "role" || name.starts_with("aria-")
}

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
        if crate::htm::is_data_attribute_name(name) {
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
        if !is_recognized_attribute(name, is_epub3) {
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

fn is_valid_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// `RSC-005`: every `id` attribute anywhere in the SVG document must be a
/// valid XML NCName (a real fixture uses `id="1"`, invalid because it
/// starts with a digit) and unique document-wide (a real fixture shares
/// one id between two elements, reported once *per* colliding element -
/// the same "per-element not per-pair" convention already used
/// elsewhere in this project, e.g. NCX id duplication).
pub(crate) fn check_ids(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    let mut by_id: HashMap<&str, u32> = HashMap::new();
    for n in svg_root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            if !is_valid_ncname(id) {
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

/// `ACC-011` (usage): an SVG `<a>` link with no accessible label at all -
/// no `xlink:title` attribute, no `<title>` child, no `aria-label`, and
/// no real text content anywhere inside it (confirmed via a real fixture
/// exercising all four labeling mechanisms as valid, plus a fifth `<a>`
/// with none of them).
pub(crate) fn check_link_labels(svg_root: roxmltree::Node, path: &str, report: &mut Report) {
    for a in svg_root.descendants().filter(|n| {
        n.is_element() && n.tag_name().name() == "a" && n.tag_name().namespace() == Some(SVG_NS)
    }) {
        let has_label = a.attribute((XLINK_NS, "title")).is_some()
            || a.attr_no_ns("aria-label").is_some()
            || a.children()
                .any(|c| c.is_element() && c.tag_name().name() == "title")
            || a.descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .any(|t| !t.trim().is_empty());
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
        return;
    }
    let mut children = fo.children();
    let Some(first) = children.next() else {
        return;
    };
    let last = fo.children().next_back().unwrap_or(first);
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
    let wrapped = if wrap_in_body {
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

/// The SVG attributes whose **value** epubcheck actually constrains.
///
/// Five of them, and that is the whole axis. `schema/20/rng/svg/svg-datatypes.rng`
/// declares 22 datatypes and **17 are a plain `<data type="string"/>`** —
/// `SVGLength`, `Number`, `OpacityValue`, `TransformList`, `PathData`, `SVGURI`
/// and the rest carry their meaning in an `<a:documentation>` and constrain
/// nothing. Probed rather than inferred: `width="abc"`, `r="-1"`,
/// `opacity="junk"`, `transform="notafunction(1)"` and an invalid path `d` are
/// all clean, against a control that confirms the document is being validated.
///
/// So constraining lengths, numbers or path data would be **inventing errors
/// epubcheck does not make**, which is the restrictive direction the `ADV-*`
/// mechanism exists to keep out of the verdict. Only these five are a gap.
const SVG_ENUM_ATTRS: &[(&str, &[&str])] = &[
    ("clip-rule", &["evenodd", "inherit", "nonzero"]),
    ("externalResourcesRequired", &["false", "true"]),
    ("fill-rule", &["evenodd", "inherit", "nonzero"]),
    ("preserveAlpha", &["false", "true"]),
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
    let mut parts = v.split_whitespace();
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
const SVG_REQUIRED_XLINK_HREF: &[&str] = &["cursor", "feImage", "mpath", "textPath", "tref", "use"];

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
    for parent in svg_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
    {
        let pname = parent.tag_name().name();
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
                && child.text().is_some_and(|t| !t.trim().is_empty())
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

pub(crate) fn check_required_attributes(
    svg_root: roxmltree::Node,
    path: &str,
    is_epub3: bool,
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
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
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
    // The five attributes whose value epubcheck constrains. Reported in the
    // same pass and with the same severity split as the required attributes:
    // one grammar, one normativity.
    for n in svg_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
    {
        for attr in n.attributes().filter(|a| a.namespace().is_none()) {
            let name = attr.name();
            let value = attr.value();
            let bad = if name == "preserveAspectRatio" {
                !preserve_aspect_ratio_is_valid(value)
            } else if let Ok(i) = SVG_ENUM_ATTRS.binary_search_by_key(&name, |(a, _)| a) {
                !SVG_ENUM_ATTRS[i].1.contains(&value)
            } else {
                continue;
            };
            if !bad {
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
    for n in svg_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(SVG_NS))
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
        use std::collections::HashMap;
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

    /// **Five SVG attributes have a constrained value, and that is the whole
    /// datatype axis.** `schema/20/rng/svg/svg-datatypes.rng` declares 22
    /// datatypes and 17 of them are a plain `<data type="string"/>`: length,
    /// number, opacity, transform list, path data and URI carry their meaning
    /// in documentation and constrain nothing.
    ///
    /// Probed rather than inferred, against a control that confirms the
    /// document is validated at all: `width="abc"`, `width="-5"`, `r="-1"`,
    /// `opacity="junk"`, `transform="notafunction(1)"` and an invalid path
    /// `d` are every one of them clean in epubcheck. Constraining those would
    /// be inventing errors it does not make.
    #[test]
    fn only_five_svg_attribute_values_are_constrained() {
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
            r#"<rect width="1" height="1" preserveAspectRatio="junk"/>"#,
            // A valid keyword with an invalid qualifier, and a valid one with
            // something after it - both fail the grammar's pattern.
            r#"<rect width="1" height="1" preserveAspectRatio="xMidYMid tight"/>"#,
            r#"<rect width="1" height="1" preserveAspectRatio="xMidYMid meet extra"/>"#,
        ] {
            assert_eq!(bad(body).len(), 1, "should be one finding: {body}");
        }

        for body in [
            r#"<rect width="1" height="1" fill-rule="evenodd"/>"#,
            r#"<rect width="1" height="1" clip-rule="inherit"/>"#,
            r#"<rect width="1" height="1" externalResourcesRequired="true"/>"#,
            // The three spellings the local shelf actually uses, 309 times
            // across 260 books.
            r#"<rect width="1" height="1" preserveAspectRatio="xMidYMid meet"/>"#,
            r#"<rect width="1" height="1" preserveAspectRatio="none"/>"#,
            r#"<rect width="1" height="1" preserveAspectRatio="xMidYMid"/>"#,
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
