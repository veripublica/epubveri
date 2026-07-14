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
}

/// Bounded (10-hop, same guard as the existing OPF-043/OPF-065 chain
/// walks) walk of a manifest item's own `fallback` chain, looking for a
/// Core Media Type.
fn fallback_reaches_core(
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
        report.push_full(
            RSC_032,
            Severity::Error,
            format!("{elname} references a foreign resource '{href}' with no fallback"),
            path,
            Position::of(node),
            "foreign.single.no_fallback",
            vec![elname.to_string(), href.to_string()],
        );
    }
}

/// `<audio>`/`<video>` share an intrinsic fallback mechanism `<embed>` etc.
/// don't have: a group of candidate resources (either the element's own
/// `@src`, or its child `<source src>` elements) is fine as long as at
/// least one candidate is usable without a fallback (Core/exempt-video) or
/// has its own fallback chain reaching a Core Media Type.
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
                if reaches_core {
                    any_ok = true;
                }
            }
        }
    }
    if any_known && !any_ok {
        report.push_full(
            RSC_032,
            Severity::Error,
            format!("{elname} references only foreign resources with no fallback"),
            path,
            Position::of(node),
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
                let u = c.trim().split_whitespace().next()?;
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
                    let Some(u) = candidate.trim().split_whitespace().next() else {
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
        } else if name == "embed" {
            if let Some(src) = node.attr_no_ns("src") {
                check_single(src, dir, status, "embed", path, node, report);
            }
        } else if name == "input" && node.attr_no_ns("type") == Some("image") {
            if let Some(src) = node.attr_no_ns("src") {
                check_single(src, dir, status, "input", path, node, report);
            }
        } else if name == "math"
            && node.tag_name().namespace() == Some(MATHML_NS)
            && let Some(altimg) = node.attr_no_ns("altimg")
        {
            check_single(altimg, dir, status, "math altimg", path, node, report);
        }
    }
}
