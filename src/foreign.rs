//! EPUB 3 §3.3/§3.5 - foreign resources and their required fallbacks.
//!
//! A "foreign" resource (declared manifest media-type that isn't a Core
//! Media Type, §3.2) may only be used if it has a fallback: either a
//! manifest `fallback` chain resolving to a Core Media Type, or (for
//! `<audio>`/`<video>`, which support a `<source>` list) an intrinsic
//! sibling that resolves to one. `<link>`/`<track>` targets, and any
//! `video/*`-typed resource used anywhere, are exempt from this entirely
//! (§3.4). A `<picture>`'s own `<img>` fallback is held to a stricter rule
//! (must itself be a Core Media Type, no manifest-fallback rescue - it's
//! the picture's own "always works" raster fallback), and a `<picture>
//! <source>` is exempt only when it declares a `type` attribute.

use std::collections::HashMap;

use crate::ids::{MED_003, MED_007, RSC_032};
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Core,
    ExemptVideo,
    Foreign,
}

fn classify(mt: &str) -> Category {
    if crate::cmt::is_core_media_type(mt) {
        Category::Core
    } else if crate::cmt::is_exempt_video(mt) {
        Category::ExemptVideo
    } else {
        Category::Foreign
    }
}

pub(crate) struct ResourceStatus {
    category: Category,
    reaches_core_via_fallback: bool,
    /// `audio/*`. Kept beside the category rather than folded into it
    /// because it answers a *different* question: the category says whether
    /// a resource needs a fallback at all, this says whether the `<video>`
    /// position exemption reaches it (see `check_candidate_group`). Audio is
    /// the one thing that exemption does not cover.
    is_audio: bool,
}

/// Bounded (10-hop, same guard as the existing OPF-043/OPF-065 chain
/// walks) walk of a manifest item's own `fallback` chain, looking for a
/// Core Media Type.
pub(crate) fn fallback_reaches_core(
    start_id: &str,
    items: &HashMap<String, (String, String)>,
    fallback_map: &HashMap<String, String>,
) -> bool {
    let mut cur = start_id;
    let mut hops = 0;
    while hops < 10 {
        let Some(next) = fallback_map.get(cur) else {
            return false;
        };
        let Some((_, mt)) = items.get(next.as_str()) else {
            return false;
        };
        if crate::cmt::is_core_media_type(mt) {
            return true;
        }
        cur = next.as_str();
        hops += 1;
    }
    false
}

/// Builds the resolved-resource-key (nfc'd local path, or full remote URL)
/// -> status map every per-content-doc check below looks resources up in.
pub(crate) fn build_resource_status(
    items: &HashMap<String, (String, String)>,
    fallback_map: &HashMap<String, String>,
) -> HashMap<String, ResourceStatus> {
    let mut status = HashMap::new();
    for (id, (path, mt)) in items {
        let category = classify(mt);
        let reaches_core_via_fallback = match category {
            Category::Core => true,
            _ => fallback_reaches_core(id, items, fallback_map),
        };
        status.insert(
            crate::opf::nfc(path),
            ResourceStatus {
                category,
                reaches_core_via_fallback,
                is_audio: crate::cmt::base_media_type(mt).starts_with("audio/"),
            },
        );
    }
    status
}

/// Resolve an href to the same key `build_resource_status` indexed by, or
/// `None` for references this check doesn't apply to (fragment-only,
/// `data:`/`mailto:`/`tel:`, or an unsupported exotic scheme - each
/// handled, if at all, by a separate check).
fn lookup_key(dir: &str, href: &str) -> Option<String> {
    let h = href.trim();
    if h.is_empty()
        || h.starts_with('#')
        || h.starts_with("data:")
        || h.starts_with("mailto:")
        || h.starts_with("tel:")
    {
        return None;
    }
    if crate::opf::is_remote_url(h) {
        let bare = h.split('#').next().unwrap_or(h);
        Some(crate::opf::nfc(bare))
    } else if h.contains("://") {
        None
    } else {
        Some(crate::opf::nfc(&crate::opf::resolve(dir, h)))
    }
}

/// The media-type declared inline in a `data:` URL itself (`data:<media-
/// type>[;params],...` or `data:<media-type>[;params];base64,...`) -
/// there's no manifest item to look a category up from, so it's parsed
/// directly out of the URL.
fn data_url_media_type(href: &str) -> Option<&str> {
    let rest = href.strip_prefix("data:")?;
    let end = rest.find([',', ';'])?;
    (!rest[..end].is_empty()).then(|| &rest[..end])
}

/// A resource reference's Core-Media-Type category and whether it has a
/// fallback - either a manifest-declared resource (looked up in `status`),
/// or a `data:` URL (classified directly from its own inline media-type; a
/// `data:` URL can never have a manifest `fallback` chain, so it never
/// reaches a Core Media Type through one - only an intrinsic mechanism,
/// e.g. a `<picture><source type=...>`, can rescue a foreign one).
fn resolve_ref(
    dir: &str,
    href: &str,
    status: &HashMap<String, ResourceStatus>,
) -> Option<(Category, bool)> {
    let h = href.trim();
    if h.starts_with("data:") {
        let mt = data_url_media_type(h).unwrap_or("text/plain");
        return Some((classify(mt), false));
    }
    let key = lookup_key(dir, h)?;
    let st = status.get(&key)?;
    Some((st.category, st.reaches_core_via_fallback))
}

/// The plain "needs a manifest fallback chain to a Core Media Type"
/// rule - embed/input[image]/math-altimg/video-poster/plain-img all share
/// this; no intrinsic alternative-markup mechanism applies to them.
/// Does this element count as palpable content — HTML's notion of "something
/// the reader would actually perceive"?
///
/// It exists here for one reason: **an `<object>` carries its own fallback in
/// its child content**, so asking whether that content is palpable is the
/// difference between "this foreign resource has no fallback" and a false
/// positive on every correctly-authored `<object>` in existence.
///
/// epubcheck's rule (`OPSHandler30.isPalpable`), followed closely because
/// getting it wrong is expensive in both directions:
/// - `hidden` makes anything impalpable, whatever it is. Its own
///   `foreign-xhtml-object-no-fallback-error` fixture turns on exactly this:
///   the object *has* a `<p>` child, and that `<p>` is `hidden`, so the
///   fallback is not really there.
/// - embedded content is palpable by being present;
/// - `svg` and `math` roots are palpable;
/// - the document-structure elements never are;
/// - anything else is palpable if it *contains* something palpable, counting
///   non-whitespace text.
fn is_palpable(n: roxmltree::Node) -> bool {
    const XHTML: &str = "http://www.w3.org/1999/xhtml";
    const SVG: &str = "http://www.w3.org/2000/svg";
    const MATHML: &str = "http://www.w3.org/1998/Math/MathML";
    if n.attr_no_ns("hidden").is_some() {
        return false;
    }
    match n.tag_name().namespace() {
        Some(SVG) => n.tag_name().name() == "svg",
        Some(MATHML) => n.tag_name().name() == "math",
        // A document with no namespace at all is malformed and reported
        // elsewhere; treating its elements as XHTML keeps this from becoming a
        // second, quieter complaint about the same thing.
        Some(XHTML) | None => match n.tag_name().name() {
            "audio" | "canvas" | "embed" | "iframe" | "img" | "object" | "picture" | "video" => {
                true
            }
            "html" | "head" | "script" | "link" | "meta" | "title" | "style" => false,
            _ => has_palpable_content(n),
        },
        _ => false,
    }
}

/// Non-whitespace text, or a palpable descendant.
fn has_palpable_content(n: roxmltree::Node) -> bool {
    n.children().any(|c| {
        if c.is_text() {
            c.text().is_some_and(|t| !t.trim().is_empty())
        } else {
            c.is_element() && is_palpable(c)
        }
    })
}

/// Elements whose one attribute points at a publication resource with no
/// intrinsic fallback of its own, so the resource must be a Core Media Type
/// or carry a manifest `fallback`.
///
/// **This list is the whole reason RSC-032 keeps being missed, so it is now
/// written down against its source.** epubcheck asks the fallback question of
/// every reference it registered as IMAGE, AUDIO, VIDEO or GENERIC
/// (`ResourceReferencesChecker.checkFallbacks`), and the GENERIC registrations
/// in `OPSHandler30` are exactly `startInput`, `startEmbed`, `checkScript`,
/// `checkIFrame` and `endObject`. Three of those are plain and live here;
/// `endObject` needs the palpable-content exemption and is handled below, as
/// are `img` (srcset candidates) and the media elements.
///
/// **`checkScript` is deliberately absent, and must stay absent.** EPUB 3.4
/// exempts a resource referenced from `<script src>` from the fallback
/// requirement (the spec editor's w3c/epubcheck#1654, accepted), so epubcheck
/// still reports RSC-032 there and we do not. That is the permissive
/// direction, which this project ships without a flag, and it is pinned by
/// `tests::script_src_is_exempt_from_the_fallback_requirement`. Doitsu
/// reported the resulting difference from MobileRead #248; the test caught an
/// attempt to "fix" it within the hour, which is exactly what it was written
/// for.
///
/// `iframe` and `input` were genuinely missing, and neither failed loudly.
/// `input` was worse than absent: it was gated on `type="image"`, while
/// epubcheck registers `input@src` whatever the type is. Before adding a
/// reference kind anywhere, check it against this list and against `opf.rs`\x27s
/// `is_resource_reference`, which asks a different question about the same
/// markup and had drifted from it.
const PLAIN_RESOURCE_ATTRS: &[(&str, &str)] =
    &[("embed", "src"), ("iframe", "src"), ("input", "src")];

fn check_single(
    href: &str,
    dir: &str,
    status: &HashMap<String, ResourceStatus>,
    elname: &str,
    path: &str,
    node: roxmltree::Node,
    report: &mut Report,
) {
    let Some((category, reaches_core)) = resolve_ref(dir, href, status) else {
        return;
    };
    if category == Category::Foreign && !reaches_core {
        report.push_node(
            RSC_032,
            Severity::Error,
            format!("{elname} references a foreign resource '{href}' with no fallback"),
            path,
            node,
            "foreign.single.no_fallback",
            vec![elname.to_string(), href.to_string()],
        );
    }
}

/// Whether an href resolves to an `audio/*` resource. Mirrors `resolve_ref`,
/// including its `data:` handling, and answers only the one question the
/// `<video>` position exemption turns on.
fn is_audio_ref(dir: &str, href: &str, status: &HashMap<String, ResourceStatus>) -> bool {
    let h = href.trim();
    if h.starts_with("data:") {
        let mt = data_url_media_type(h).unwrap_or("text/plain");
        return crate::cmt::base_media_type(mt).starts_with("audio/");
    }
    lookup_key(dir, h)
        .and_then(|k| status.get(&k))
        .is_some_and(|st| st.is_audio)
}

/// `<audio>`/`<video>` share an intrinsic fallback mechanism `<embed>` etc.
/// don't have: a group of candidate resources (either the element's own
/// `@src`, or its child `<source src>` elements) is fine as long as at
/// least one candidate is usable without a fallback (Core/exempt-video) or
/// has its own fallback chain reaching a Core Media Type.
///
/// **A `<video>` exempts more than that, by position** (w3c/epubcheck
/// [#1662](https://github.com/w3c/epubcheck/issues/1662), opened by the spec
/// editor and measured here 2026-08-19 — epubcheck and epubveri reported the
/// same false positive on the same book). EPUB 3.3 §3.4: *"All video codecs
/// referenced from the HTML video — including any child source elements — are
/// exempt resources."* Unconditional, and about **where** the resource is
/// referenced rather than what its media type is: a streaming playlist
/// (`application/x-mpegurl`) plays straight from the element and can carry no
/// manifest fallback, which is exactly the case that drew RSC-032.
///
/// The type-based exemption stays alongside it — a `video/*` resource is
/// exempt wherever it is used — which is what makes this change **purely
/// permissive**: nothing that validated before becomes an error.
///
/// **Audio is deliberately not covered.** The same section keeps it
/// restrictive: *"The requirement for fallbacks only applies to audio foreign
/// resources referenced from audio and video elements."* An `audio/*` foreign
/// resource inside a `<video>` still needs its fallback, which is why
/// `is_audio` sits beside the category rather than inside it.
///
fn check_candidate_group(
    hrefs: &[&str],
    dir: &str,
    status: &HashMap<String, ResourceStatus>,
    elname: &str,
    path: &str,
    node: roxmltree::Node,
    report: &mut Report,
) {
    let mut any_known = false;
    let mut any_ok = false;
    for href in hrefs {
        let Some((category, reaches_core)) = resolve_ref(dir, href, status) else {
            continue;
        };
        any_known = true;
        match category {
            Category::Core | Category::ExemptVideo => any_ok = true,
            Category::Foreign => {
                // Either its own fallback chain rescues it, or the `<video>`
                // position exemption covers it — see the doc comment above.
                if reaches_core || (elname == "video" && !is_audio_ref(dir, href, status)) {
                    any_ok = true;
                }
            }
        }
    }
    if any_known && !any_ok {
        report.push_node(
            RSC_032,
            Severity::Error,
            format!("{elname} references only foreign resources with no fallback"),
            path,
            node,
            "foreign.candidate_group.no_fallback",
            vec![elname.to_string()],
        );
    }
}

/// An `<img>`'s (or a `<picture>`'s own `<img>`'s) candidate URLs: when
/// `srcset` is present it's authoritative (the resolution-selection list;
/// `src` is then just a same-content duplicate for legacy browsers and
/// isn't independently checked - confirmed via a real corpus fixture pair
/// where checking `src` too would over-count), otherwise fall back to
/// plain `src`.
fn img_candidates(node: roxmltree::Node) -> Vec<String> {
    if let Some(srcset) = node.attr_no_ns("srcset") {
        srcset
            .split(',')
            .filter_map(|c| {
                let u = c.split_whitespace().next()?;
                (!u.is_empty()).then(|| u.to_string())
            })
            .collect()
    } else if let Some(src) = node.attr_no_ns("src") {
        vec![src.to_string()]
    } else {
        Vec::new()
    }
}

fn check_audio_video(
    node: roxmltree::Node,
    dir: &str,
    status: &HashMap<String, ResourceStatus>,
    path: &str,
    report: &mut Report,
) {
    let name = node.tag_name().name();
    if name == "video"
        && let Some(poster) = node.attr_no_ns("poster")
    {
        check_single(poster, dir, status, "video poster", path, node, report);
    }
    let mut candidates: Vec<&str> = Vec::new();
    if let Some(src) = node.attr_no_ns("src") {
        candidates.push(src);
    } else {
        for child in node
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "source")
        {
            if let Some(src) = child.attr_no_ns("src") {
                candidates.push(src);
            }
        }
    }
    if !candidates.is_empty() {
        check_candidate_group(&candidates, dir, status, name, path, node, report);
    }
}

/// `<picture>`'s own `<img>` must itself be a Core Media Type (no
/// manifest-fallback rescue - MED-003, unconditional on foreign-ness); its
/// `<source>` elements are exempt from the foreign-resource check entirely
/// when they declare a `type` attribute, otherwise any foreign candidate in
/// their `srcset` is MED-007 (also unconditional on any manifest fallback -
/// confirmed via a real fixture where a manifest fallback exists but
/// MED-007 still fires because `type` is absent).
fn check_picture(
    node: roxmltree::Node,
    dir: &str,
    status: &HashMap<String, ResourceStatus>,
    path: &str,
    report: &mut Report,
) {
    for child in node.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "source" => {
                if child.attr_no_ns("type").is_some() {
                    continue;
                }
                let Some(srcset) = child.attr_no_ns("srcset") else {
                    continue;
                };
                let mut any_foreign = false;
                for candidate in srcset.split(',') {
                    let Some(u) = candidate.split_whitespace().next() else {
                        continue;
                    };
                    if resolve_ref(dir, u, status).is_some_and(|(cat, _)| cat == Category::Foreign)
                    {
                        any_foreign = true;
                    }
                }
                if any_foreign {
                    report.push_at_pos(
                        MED_007,
                        Severity::Error,
                        "picture source references a foreign resource with no type attribute",
                        path,
                        Position::of(child),
                    );
                }
            }
            "img" => {
                for href in img_candidates(child) {
                    if resolve_ref(dir, &href, status)
                        .is_some_and(|(cat, _)| cat == Category::Foreign)
                    {
                        report.push_at_pos(
                            MED_003,
                            Severity::Error,
                            format!("picture img fallback references a foreign resource '{href}'"),
                            path,
                            Position::of(child),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Entry point: walks a content document once, dispatching each element to
/// the right rule. `<link>`/`<track>` targets are exempt (§3.4) and never
/// checked; `<picture>` and `<audio>`/`<video>` get their own subtree
/// handling (including any nested `<img>`/`<source>`, so the generic
/// `<img>` pass below must skip elements already covered by those).
pub(crate) fn check_content_doc(
    d: &roxmltree::Document,
    path: &str,
    dir: &str,
    status: &HashMap<String, ResourceStatus>,
    report: &mut Report,
) {
    const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
    for node in d.descendants().filter(|n| n.is_element()) {
        let name = node.tag_name().name();
        match name {
            "link" | "track" => continue,
            "picture" => {
                check_picture(node, dir, status, path, report);
                continue;
            }
            "audio" | "video" => {
                check_audio_video(node, dir, status, path, report);
                continue;
            }
            _ => {}
        }
        if matches!(name, "img" | "source")
            && node.ancestors().skip(1).any(|a| {
                a.is_element() && matches!(a.tag_name().name(), "picture" | "audio" | "video")
            })
        {
            continue;
        }
        if name == "img" {
            for href in img_candidates(node) {
                check_single(&href, dir, status, "img", path, node, report);
            }
        } else if let Some((_, attr)) = PLAIN_RESOURCE_ATTRS.iter().find(|(e, _)| *e == name) {
            if let Some(href) = node.attr_no_ns(attr) {
                check_single(href, dir, status, name, path, node, report);
            }
        } else if name == "object" {
            // `<object>` was simply never added to this list, which is the
            // per-source shape again: the elements that can point at a
            // foreign resource are enumerated here by hand, and nothing fails
            // loudly when one is missing. epubcheck reports RSC-032 on its
            // own `foreign-xhtml-object-no-fallback-error` fixture; we
            // reported nothing at all.
            //
            // The resource is `data`, and the element's *own content* is its
            // fallback — so an object that has palpable content owes nothing,
            // whatever the manifest says.
            if let Some(data) = node.attr_no_ns("data")
                && !has_palpable_content(node)
            {
                check_single(data, dir, status, "object", path, node, report);
            }
        } else if name == "math"
            && node.tag_name().namespace() == Some(MATHML_NS)
            && let Some(altimg) = node.attr_no_ns("altimg")
        {
            check_single(altimg, dir, status, "math altimg", path, node, report);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EPUB 3.4 (spec editor's issue w3c/epubcheck#1654): a resource
    /// referenced only from `<script src>` is exempt from the
    /// foreign-resource fallback requirement.
    ///
    /// **This needs no code — `check_content_doc` walks `img`/`embed`/
    /// `input[type=image]`/`math@altimg` plus the `picture`/`audio`/`video`
    /// subtrees, and `script` is in none of them, so a script target has
    /// never entered the check.** It is asserted so that a future widening
    /// of that walk cannot silently take the exemption away, which is the
    /// same reason `cmt::epub34_core_media_types` pins the 3.4 audio types
    /// it did not have to add.
    ///
    /// The negative half is what gives it teeth: the identical resource
    /// referenced from `<embed>` *does* draw RSC-032, so this test fails if
    /// the exemption is ever lost, rather than passing because the fixture
    /// was toothless.
    #[test]
    fn script_src_is_exempt_from_the_fallback_requirement() {
        let mut items = HashMap::new();
        items.insert(
            "w".to_string(),
            ("mod.wasm".to_string(), "application/wasm".to_string()),
        );
        let status = build_resource_status(&items, &HashMap::new());
        // No fallback declared, and application/wasm is not a Core Media
        // Type, so the resource is foreign with nothing to rescue it.
        assert!(!status["mod.wasm"].reaches_core_via_fallback);

        let findings = |body: &str| {
            let doc = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
            );
            let d = roxmltree::Document::parse(&doc).unwrap();
            let mut report = Report::default();
            check_content_doc(&d, "ch.xhtml", "", &status, &mut report);
            report.messages.iter().filter(|m| m.id == RSC_032).count()
        };

        assert_eq!(
            findings(r#"<script src="mod.wasm" type="application/wasm"></script>"#),
            0,
            "a <script src> target is exempt (EPUB 3.4)"
        );
        assert_eq!(
            findings(r#"<embed src="mod.wasm" type="application/wasm"/>"#),
            1,
            "the same resource from <embed> is not exempt - the test has teeth"
        );
    }

    /// A `<video>` exempts what it references by **position**, not by media
    /// type — w3c/epubcheck [#1662](https://github.com/w3c/epubcheck/issues/1662),
    /// opened by the spec editor after being asked whether an HLS playlist
    /// reference is valid.
    ///
    /// EPUB 3.3 §3.4: *"All video codecs referenced from the HTML video —
    /// including any child source elements — are exempt resources."*
    /// `application/x-mpegurl` plays straight from the element and can carry
    /// no manifest fallback, so requiring one is a false positive. **We had
    /// the same bug**, measured one book each against epubcheck 5.3.0 on
    /// 2026-08-19: both tools reported RSC-032.
    ///
    /// Three boundaries, because the exemption must not spread — each was
    /// measured against epubcheck too, and it agrees on both negatives:
    ///
    /// - foreign **audio** inside a `<video>` is still reported (the same
    ///   section keeps audio restrictive);
    /// - the same foreign resource inside an `<audio>` is still reported;
    /// - and the positive case is the only thing that changed.
    #[test]
    fn a_video_exempts_its_references_by_position_except_audio() {
        let mut items = HashMap::new();
        items.insert(
            "p".to_string(),
            (
                "stream.m3u8".to_string(),
                "application/x-mpegurl".to_string(),
            ),
        );
        items.insert(
            "a".to_string(),
            ("sound.wav".to_string(), "audio/x-wav".to_string()),
        );
        let status = build_resource_status(&items, &HashMap::new());
        // Both are foreign with no fallback to rescue them, or the rows
        // below would pass for the wrong reason.
        assert!(!status["stream.m3u8"].reaches_core_via_fallback);
        assert!(!status["sound.wav"].reaches_core_via_fallback);

        let findings = |body: &str| {
            let doc = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
            );
            let d = roxmltree::Document::parse(&doc).unwrap();
            let mut report = Report::default();
            check_content_doc(&d, "ch.xhtml", "", &status, &mut report);
            report.messages.iter().filter(|m| m.id == RSC_032).count()
        };

        assert_eq!(
            findings(
                r#"<video controls=""><source src="stream.m3u8" type="application/x-mpegurl"/></video>"#
            ),
            0,
            "a non-audio resource referenced from <video> is exempt"
        );
        assert_eq!(
            findings(r#"<video controls="" src="stream.m3u8"></video>"#),
            0,
            "the element's own @src is exempt the same way"
        );
        assert_eq!(
            findings(r#"<video controls=""><source src="sound.wav" type="audio/x-wav"/></video>"#),
            1,
            "audio inside a <video> still needs its fallback"
        );
        assert_eq!(
            findings(
                r#"<audio controls=""><source src="stream.m3u8" type="application/x-mpegurl"/></audio>"#
            ),
            1,
            "the exemption is the <video> element's alone"
        );
    }

    /// `iframe@src` and `input@src` are publication-resource references and
    /// owe a fallback, and neither was being asked (MobileRead #248 — Doitsu
    /// reported the `<script>` difference, and chasing it found these two).
    ///
    /// `input` was worse than absent: it was gated on `type="image"`, while
    /// epubcheck's `startInput` registers `input@src` whatever the type is.
    ///
    /// Measured against epubcheck 5.3.0, one book per row. `<script>` is the
    /// deliberate exception and has its own test above; it is repeated here as
    /// the negative, so that widening this walk again fails loudly.
    #[test]
    fn iframe_and_input_owe_a_fallback_and_script_does_not() {
        let mut items = HashMap::new();
        items.insert(
            "w".to_string(),
            ("x.bin".to_string(), "application/octet-stream".to_string()),
        );
        let status = build_resource_status(&items, &HashMap::new());
        assert!(!status["x.bin"].reaches_core_via_fallback);

        let findings = |body: &str| {
            let doc = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
            );
            let d = roxmltree::Document::parse(&doc).unwrap();
            let mut report = Report::default();
            check_content_doc(&d, "ch.xhtml", "", &status, &mut report);
            report.messages.iter().filter(|m| m.id == RSC_032).count()
        };

        for body in [
            r#"<iframe src="x.bin"></iframe>"#,
            r#"<input type="image" src="x.bin" alt="a"/>"#,
            // Not an image input, and epubcheck asks all the same.
            r#"<input type="text" src="x.bin"/>"#,
            r#"<embed src="x.bin"/>"#,
        ] {
            assert_eq!(findings(body), 1, "expected RSC-032 for {body}");
        }

        assert_eq!(
            findings(r#"<script src="x.bin"></script>"#),
            0,
            "EPUB 3.4 exempts a <script src> target (w3c/epubcheck#1654)"
        );
    }
}
