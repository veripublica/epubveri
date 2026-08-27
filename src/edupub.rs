//! EDUPUB profile checks (`http://idpf.org/epub/profile/edupub/`),
//! triggered by `<dc:type>edupub</dc:type>` — either in a single-rendition
//! book's own OPF, or in `META-INF/metadata.xml` (a separate,
//! publication-level metadata file used only for multi-rendition
//! packages). Deliberately narrow: only the checks confirmed by real
//! corpus fixtures (HTML5 microdata attributes, the page-list/pagination-
//! source cross-reference, the sectioning and heading rules, and the
//! multi-rendition `dc:type` cardinality checks wired in `opf.rs`) — not
//! the full EDUPUB conformance suite (accessibility metadata, etc.), which
//! the corpus itself only exercises indirectly via `-valid` fixtures with
//! no dedicated error codes to target.

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) fn is_edupub(dc_type: Option<&str>) -> bool {
    dc_type == Some("edupub")
}

/// HTM-051: HTML5 microdata items (rooted at an `itemscope` attribute)
/// aren't allowed in an edupub content document. Only `itemscope` is
/// checked, not `itemtype`/`itemprop` independently - confirmed via the
/// real corpus fixture, which has both an `itemscope`-bearing element and
/// a separate `itemprop`-only element (a property *of* that same item,
/// not a second item) but expects exactly one finding, not two.
pub(crate) fn check_content_doc(d: &roxmltree::Document, path: &str, report: &mut Report) {
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.attr_no_ns("itemscope").is_some() {
            report.push_at_pos(
                HTM_051,
                Severity::Warning,
                "HTML5 microdata items are not allowed in an edupub content document",
                path,
                Position::of(node),
            );
        }
    }
}

const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

fn node_text(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect::<String>()
        .trim()
        .to_string()
}

/// HTML5's real sectioning-content elements - used to decide whether
/// `<body>` is acting as an explicit section (see `is_body_explicit`
/// below). `article` is standard HTML5 sectioning content too; no
/// fixture exercises it, but including it is the conservative,
/// spec-consistent choice.
fn is_sectioning(name: &str) -> bool {
    matches!(name, "section" | "aside" | "nav" | "article")
}

/// `<body>` is "explicit" (acts as its own titled/labeled section, and so
/// requires a heading or aria-label of its own) exactly when it has any
/// direct-child element that isn't itself sectioning content - confirmed
/// via two real fixture pairs: a body containing *only* nav/aside/section
/// needs no heading of its own (implicit), while a body additionally
/// containing an `<h1>` or a plain `<p>` does.
fn is_body_explicit(body: roxmltree::Node) -> bool {
    body.children()
        .filter(|c| c.is_element())
        .any(|c| !is_sectioning(c.tag_name().name()))
}

/// A sectioning container's own heading: a direct-child heading element,
/// or one wrapped in a direct-child `<header>` (confirmed via real
/// fixtures using both forms interchangeably).
///
/// "Is a heading" is `heading_rank`, and it used to be a near-copy that
/// required `role="heading"` to carry an `aria-level` too. That produced a
/// **false positive**: `<body><span role="heading">Top</span>…</body>` drew
/// "The body element requires a heading when it is used as an implied
/// section" from us and nothing from epubcheck, whose selector is a bare
/// `html:*[@role='heading']`. Two predicates answering "is this a heading"
/// with different answers is the shape worth deleting on sight; only one
/// remains, and its `aria-level` default of 2 is epubcheck's.
fn find_heading<'a>(container: roxmltree::Node<'a, 'a>) -> Option<roxmltree::Node<'a, 'a>> {
    for c in container.children().filter(|c| c.is_element()) {
        if heading_rank(c).is_some() {
            return Some(c);
        }
        if c.tag_name().name() == "header"
            && let Some(h) = c
                .children()
                .filter(|gc| gc.is_element())
                .find(|gc| heading_rank(*gc).is_some())
        {
            return Some(h);
        }
    }
    None
}

/// RSC-005 "Empty ranked heading detected": a heading whose only content
/// is a single `<img>` needs real alternative text (confirmed via a real
/// fixture pair using the same shape with non-empty vs. empty `alt`).
fn check_heading_img_alt(h: roxmltree::Node, path: &str, report: &mut Report) {
    let has_real_text = h
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .any(|t| !t.trim().is_empty());
    if has_real_text {
        return;
    }
    let children: Vec<_> = h.children().filter(|c| c.is_element()).collect();
    if let [img] = children.as_slice()
        && img.tag_name().name() == "img"
    {
        let alt = img.attr_no_ns("alt").unwrap_or("").trim();
        if alt.is_empty() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Empty ranked heading detected",
                path,
                h,
                "edupub.heading.empty_ranked_heading",
                Vec::new(),
            );
        }
    }
}

/// RSC-005: an `aria-label` on a section/body must not duplicate the
/// text of its own heading (confirmed via a real fixture with both body
/// and one of its sections doing this, expecting 2 findings).
fn check_aria_label_match(
    container: roxmltree::Node,
    heading: roxmltree::Node,
    path: &str,
    report: &mut Report,
) {
    let Some(label) = container.attr_no_ns("aria-label") else {
        return;
    };
    let heading_text = node_text(heading);
    if !heading_text.is_empty() && label.trim() == heading_text {
        report.push_node(
            RSC_005,
            Severity::Error,
            "The value of the \"aria-label\" attribute must not be the same as the content of the heading",
            path,
            container,
            "edupub.heading.aria_label_duplicates_text",
            Vec::new(),
        );
    }
}

/// A container's own heading, once found: checks its nesting-level
/// number, image-alt-text, and aria-label-duplication - shared between
/// `<body>` (when explicit) and every `<section>`/`<aside>`/`<nav>`.
fn check_own_heading(
    container: roxmltree::Node,
    heading: roxmltree::Node,
    path: &str,
    report: &mut Report,
) {
    check_heading_img_alt(heading, path, report);
    check_aria_label_match(container, heading, path, report);
}

/// §4.2 Sectioning / §4.3 Titles and Headings: the EDUPUB content-
/// document sectioning and heading rules.
///
/// The heading *rank* check (`check_heading_ranks`) used to be a named gap
/// here, deferred because "real fixtures gave contradictory evidence for the
/// exact depth-counting algorithm". They did not disagree - the algorithm was
/// being inferred from the fixtures instead of read from
/// `edu-structure.sch`, which states it outright, including the two things
/// that made the fixtures look contradictory: the `body-is-section` branch
/// selects a different formula, and the topmost heading (which sets the
/// baseline every other heading is measured against) ignores `aside`/`nav`
/// and anything more than one sectioning level deep.
pub(crate) fn check_sectioning_and_headings(
    doc: &roxmltree::Document,
    path: &str,
    report: &mut Report,
) {
    check_heading_ranks(doc, path, report);

    let Some(body) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return;
    };

    if is_body_explicit(body) {
        let heading = find_heading(body);
        let has_aria_label = body.attr_no_ns("aria-label").is_some();
        match heading {
            Some(h) => check_own_heading(body, h, path, report),
            None if !has_aria_label => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "The body element requires a heading when it is used as an implied section",
                    path,
                    body,
                    "edupub.sectioning.body_missing_heading",
                    Vec::new(),
                );
            }
            None => {}
        }
    }

    for n in doc
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "section" | "aside" | "nav"))
    {
        let has_aria_label = n.attr_no_ns("aria-label").is_some();
        match find_heading(n) {
            Some(h) => check_own_heading(n, h, path, report),
            None if !has_aria_label && n.tag_name().name() != "nav" => {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "section does not have a heading",
                    path,
                    n,
                    "edupub.sectioning.missing_heading",
                    vec![n.tag_name().name().to_string()],
                );
            }
            None => {}
        }
    }

    // A subtitle (`epub:type="subtitle"`) must be wrapped in a `<header>`
    // (a section's own title/subtitle pair) - a figure's own
    // `<figcaption>` title/subtitle pair is a separate, unrelated
    // context and stays exempt (confirmed via a real fixture using both
    // shapes in the same, otherwise-valid document).
    for n in doc.descendants().filter(|n| {
        n.is_element()
            && n.attribute((EPUB_NS, "type"))
                .is_some_and(|t| t.split_whitespace().any(|tok| tok == "subtitle"))
    }) {
        let parent_ok = n
            .parent_element()
            .is_some_and(|p| matches!(p.tag_name().name(), "header" | "figcaption"));
        if !parent_ok {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Section subtitles must be wrapped in a header element",
                path,
                n,
                "edupub.sectioning.subtitle_not_wrapped",
                Vec::new(),
            );
        }
    }
}

/// A heading's *rank* for the nesting rule.
///
/// Deliberately not `heading_level`, which answers a different question and
/// gives up on a `role="heading"` carrying no `aria-level`. Here epubcheck's
/// Schematron defaults that case to **2** (`if (current()/@aria-level) then
/// number(...) else 2`), and the default is load-bearing: without it such a
/// heading vanishes from the rule instead of being ranked.
fn heading_rank(n: roxmltree::Node) -> Option<u32> {
    let name = n.tag_name().name();
    if let Some(digits) = name.strip_prefix('h')
        && let Ok(rank) = digits.parse::<u32>()
        && (1..=6).contains(&rank)
    {
        return Some(rank);
    }
    if n.attr_no_ns("role") == Some("heading") {
        return Some(
            n.attr_no_ns("aria-level")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        );
    }
    None
}

/// How deeply a node sits in sectioning content, epubcheck's count: the
/// number of `section`/`article`/`aside`/`nav` ancestors.
fn sectioning_depth(n: roxmltree::Node) -> u32 {
    n.ancestors()
        .skip(1)
        .filter(|a| a.is_element() && is_sectioning(a.tag_name().name()))
        .count() as u32
}

/// RSC-005: a heading's rank must match its nesting depth (`edu-structure.sch`,
/// pattern `edupub.headings`).
///
/// The whole rule is relative, which is why it needs the two file-level
/// values below rather than "an `<h1>` at depth 0". epubcheck derives an
/// *expected* rank for every heading from whichever heading came first, so a
/// document that starts at `h2` is consistent as long as it keeps stepping by
/// one; a document that starts at `h2` and then uses `h2` again one section
/// deep is not.
///
/// Ported from the Schematron rather than from its prose, because three
/// parts are easy to get subtly wrong: the topmost heading ignores anything
/// inside an `aside`/`nav` and anything more than one sectioning level deep;
/// `body-is-section` selects a *different* formula (`rank - nest + depth`
/// versus `rank + depth - 1`); and the rule stops caring past `h6`, where it
/// asks only that the heading be an `h6`.
///
/// Checked against epubcheck 5.3.0 on its own `edupub-titles-*` fixtures, one
/// heading at a time - the corpus scenario asserts three findings for one
/// document, so a port that produced three findings for the wrong three
/// headings would have scored as a pass.
fn check_heading_ranks(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    let Some(body) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return;
    };
    let headings: Vec<roxmltree::Node> = body
        .descendants()
        .filter(|n| n.is_element() && heading_rank(*n).is_some())
        .collect();

    // `exists(//html:body/(html:* except (html:article | html:section)))` -
    // note this is *not* `is_body_explicit`, which also excludes `aside` and
    // `nav`. A body holding only a `<nav>` is an implied section for this
    // rule and an implicit one for that one.
    let body_is_section = body
        .children()
        .filter(|c| c.is_element())
        .any(|c| !matches!(c.tag_name().name(), "article" | "section"));
    let body_label = body
        .attr_no_ns("aria-label")
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    // The topmost heading: document order, skipping anything inside an
    // `aside`/`nav`, and no deeper than one `section`/`article`.
    let topmost = headings.iter().find(|h| {
        !h.ancestors()
            .skip(1)
            .any(|a| a.is_element() && matches!(a.tag_name().name(), "aside" | "nav"))
            && h.ancestors()
                .skip(1)
                .filter(|a| a.is_element() && matches!(a.tag_name().name(), "section" | "article"))
                .count()
                <= 1
    });
    let (topmost_rank, topmost_nest) = if !body_label.is_empty() {
        (1, 0)
    } else {
        match topmost {
            Some(h) => {
                let nest = u32::from(h.ancestors().skip(1).any(|a| {
                    a.is_element() && matches!(a.tag_name().name(), "section" | "article" | "nav")
                }));
                (heading_rank(*h).unwrap_or(1), nest)
            }
            None => (1, 0),
        }
    };

    for h in &headings {
        let rank = heading_rank(*h).unwrap_or(1);
        if h.ancestors()
            .skip(1)
            .any(|a| a.is_element() && matches!(a.tag_name().name(), "figure" | "blockquote"))
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Ranked headings are not valid in figure or blockquote",
                path,
                *h,
                "edupub.headings.rank_in_sectioning_root",
                Vec::new(),
            );
            continue;
        }
        let depth = sectioning_depth(*h);
        // Saturating, because the second formula subtracts: a document whose
        // topmost heading is at depth 0 and is not a section gives
        // `rank + 0 - 1`, and an `h1` there must not wrap around.
        let expected = if body_is_section {
            (topmost_rank + depth).saturating_sub(topmost_nest)
        } else {
            (topmost_rank + depth).saturating_sub(1)
        };
        if expected < 6 {
            if rank != expected {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!(
                        "The heading rank h{rank} does not match the current nesting level ({expected})"
                    ),
                    path,
                    *h,
                    "edupub.headings.rank_mismatch",
                    vec![rank.to_string(), expected.to_string()],
                );
            }
        } else if rank < 6 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "The current heading rank should be h6",
                path,
                *h,
                "edupub.headings.rank_should_be_h6",
                Vec::new(),
            );
        }
    }
}

/// NAV-003 / OPF-066: an edupub publication that identifies a print-source
/// for pagination (`dc:source` + `<meta property="source-of"
/// refines="#...">pagination</meta>`) must have a `page-list` nav, and
/// vice versa - a `page-list` nav implies a print-source should be named.
pub(crate) fn check_page_list(
    has_pagination_source: bool,
    has_page_list_nav: bool,
    opf_path: &str,
    report: &mut Report,
) {
    match (has_pagination_source, has_page_list_nav) {
        (true, false) => {
            report.push_at(
                NAV_003,
                Severity::Error,
                "a pagination source is identified but the navigation document has no page-list nav",
                opf_path,
            );
        }
        (false, true) => {
            report.push_at(
                OPF_066,
                Severity::Error,
                "a page-list nav is present but no print-source for pagination is identified",
                opf_path,
            );
        }
        _ => {}
    }
}

const EPUB_TYPE: (&str, &str) = ("http://www.idpf.org/2007/ops", "type");
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

/// EDUPUB nav-completeness (NAV-004..008): epubcheck's `OPFChecker30.checkNav`
/// compares content-document features against the navigation document's
/// special-nav lists. This accumulates those features across the publication;
/// [`check`](NavCompleteness::check) then emits the findings. All are USAGE.
#[derive(Default)]
pub(crate) struct NavCompleteness {
    // Content-document features (nav document excluded).
    audio: bool,
    video: bool,
    figure: bool,
    table: bool,
    /// epubcheck's SECTIONS count, summed over linear content documents.
    sections: usize,
    // Navigation-document features.
    toc_links: usize,
    loa: bool,
    loi: bool,
    lot: bool,
    lov: bool,
}

impl NavCompleteness {
    /// Fold in a content document's media features (`<audio>`/`<video>`/
    /// `<figure>`/`<table>`). Call for every content document *except* the
    /// navigation document, linear or not - epubcheck reports these from its
    /// content-document handler.
    pub(crate) fn add_media(&mut self, d: &roxmltree::Document) {
        for n in d.descendants().filter(|n| n.is_element()) {
            match n.tag_name().name() {
                "audio" => self.audio = true,
                "video" => self.video = true,
                "figure" => self.figure = true,
                "table" => self.table = true,
                _ => {}
            }
        }
    }

    /// Add a linear content document's SECTIONS count, mirroring epubcheck's
    /// handler exactly (in document order, HTML namespace only): each
    /// `<section>` counts, and a `<body>` whose first child element isn't a
    /// `<section>` counts once (its content is one implicit section).
    pub(crate) fn add_sections(&mut self, d: &roxmltree::Document) {
        let mut in_body = false;
        for n in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().namespace() == Some(XHTML_NS))
        {
            match n.tag_name().name() {
                "body" => in_body = true,
                "section" => {
                    in_body = false;
                    self.sections += 1;
                }
                _ if in_body => {
                    self.sections += 1;
                    in_body = false;
                }
                _ => {}
            }
        }
    }

    /// Record the navigation document: count the `toc` nav's hyperlinks and
    /// note which of the `loa`/`loi`/`lot`/`lov` special navs are present.
    pub(crate) fn set_nav(&mut self, d: &roxmltree::Document) {
        for nav in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "nav")
        {
            match nav.attribute(EPUB_TYPE) {
                Some("toc") => {
                    self.toc_links += nav
                        .descendants()
                        .filter(|n| {
                            n.is_element()
                                && n.tag_name().name() == "a"
                                && n.attribute("href").is_some_and(|h| !h.trim().is_empty())
                        })
                        .count();
                }
                Some("loa") => self.loa = true,
                Some("loi") => self.loi = true,
                Some("lot") => self.lot = true,
                Some("lov") => self.lov = true,
                _ => {}
            }
        }
    }

    /// Emit NAV-004..008 (all USAGE). Only meaningful for an EDUPUB
    /// publication - the caller gates on [`is_edupub`].
    pub(crate) fn check(&self, opf_path: &str, report: &mut Report) {
        if self.sections != self.toc_links {
            report.push_at(
                NAV_004,
                Severity::Usage,
                "the navigation document's heading hierarchy is incomplete: the number \
                 of sections doesn't match the number of toc links",
                opf_path,
            );
        }
        for (present, has_nav, id, kind, list) in [
            (self.audio, self.loa, NAV_005, "audio", "loa"),
            (self.figure, self.loi, NAV_006, "figure", "loi"),
            (self.table, self.lot, NAV_007, "table", "lot"),
            (self.video, self.lov, NAV_008, "video", "lov"),
        ] {
            if present && !has_nav {
                report.push_at(
                    id,
                    Severity::Usage,
                    format!(
                        "content documents contain <{kind}> elements but the navigation \
                         document has no \"{list}\" nav"
                    ),
                    opf_path,
                );
            }
        }
    }
}

/// §3.4 Teacher's Editions, §8.1 Profile Identification, §8.3
/// Accessibility Metadata - all confirmed via real, single-Package-
/// Document (bare `.opf`) fixtures. A teacher's edition should (warning) name
/// its corresponding student edition via `dc:source`; a confirmed edupub
/// publication needs at least one `schema:accessibilityFeature` declaration,
/// and "none" is specifically insufficient there (though a legitimate
/// general-purpose schema.org value otherwise).
///
/// **`dc:type=teacher-edition` alone does not turn the profile on**, and the
/// note that used to say so has expired twice over. It read: "a real, distinct
/// content signal, unlike bare `dc:type=edupub` detection which needs real
/// CLI-profile support this project doesn't build - named, accepted gap". The
/// gap is closed — `--profile edupub` exists and works — and the premise was
/// wrong anyway: epubcheck's `PublicationType` knows `edupub`, `dictionary`,
/// `index` and `preview`, and nothing named `teacher-edition`, so it applies
/// the EDUPUB rules only under the profile or a real `dc:type=edupub`.
/// Handed `edupub-teacher-edition-metadata-type-missing-error.opf` with no
/// profile it reports the missing file and nothing else, and with
/// `--profile edupub` both tools report the same RSC-005 — measured both ways.
/// The corpus passes the profile (the feature file's Background says so), so
/// the scenario is unaffected; only a `compare` run, which passes none, could
/// see this.
pub(crate) fn check_teacher_edition_and_accessibility(
    dc_types: &[String],
    profile: Option<&str>,
    metadata: Option<roxmltree::Node>,
    opf_path: &str,
    report: &mut Report,
) {
    let is_edupub_pub = dc_types.iter().any(|t| t == "edupub");
    let is_teacher_edition = dc_types.iter().any(|t| t == "teacher-edition");

    if !is_edupub_pub && profile == Some("edupub") {
        match metadata {
            Some(md) => report.push_node(
                RSC_005,
                Severity::Error,
                "The dc:type identifier \"edupub\" is required",
                opf_path,
                md,
                "edupub.metadata.missing_dc_type",
                Vec::new(),
            ),
            None => report.push_at_rule(
                RSC_005,
                Severity::Error,
                "The dc:type identifier \"edupub\" is required",
                opf_path,
                "edupub.metadata.missing_dc_type",
                Vec::new(),
            ),
        }
        if !is_teacher_edition {
            // Pure profile-forced detection with no other real edupub
            // signal at all - a real fixture (a bare, single-Package-
            // Document check with no accessibility metadata either)
            // expects exactly this one finding, not the accessibility
            // check below cascading on content that was never meant to
            // satisfy it.
            return;
        }
    }
    let Some(md) = metadata else { return };

    if is_teacher_edition {
        let has_source = md.children().any(|n| {
            n.is_element()
                && n.tag_name().name() == "source"
                && n.tag_name().namespace() == Some(DC_NS)
        });
        if !has_source {
            report.push_node(
                RSC_017,
                Severity::Warning,
                "A teacher\u{2019}s edition should identify the corresponding student edition",
                opf_path,
                md,
                "edupub.metadata.teacher_edition_missing_source",
                Vec::new(),
            );
        }
    }

    if is_edupub_pub {
        let features: Vec<String> = md
            .children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property") == Some("schema:accessibilityFeature")
            })
            .map(elem_text)
            .collect();
        if features.is_empty() {
            report.push_node(
                RSC_005,
                Severity::Error,
                "At least one schema:accessibilityFeature declaration is required",
                opf_path,
                md,
                "edupub.metadata.missing_accessibility_feature",
                Vec::new(),
            );
        } else if features.iter().any(|f| f == "none") {
            report.push_node(
                RSC_005,
                Severity::Error,
                "value \"none\" is not valid in edupub",
                opf_path,
                md,
                "edupub.metadata.invalid_accessibility_feature_none",
                Vec::new(),
            );
        }
    }
}

const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

fn elem_text(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect::<String>()
        .trim()
        .to_string()
}

fn dc_type_of(ocf: &mut crate::ocf::Ocf, path: &str) -> Option<String> {
    let bytes = ocf.read(path)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = crate::ocf::parse_xml(&text).ok()?;
    doc.descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "type"
                && n.tag_name().namespace() == Some(DC_NS)
        })
        .map(elem_text)
}

/// Multi-rendition `dc:type` cardinality (both RSC-005): a multi-rendition
/// publication is "edupub" if *either* `META-INF/metadata.xml` (the
/// publication-level metadata) or *any* rendition's own OPF declares
/// `dc:type=edupub` - confirmed via the real corpus fixtures, where the
/// "publication-level missing" scenario has metadata.xml's own dc:type
/// commented out while *both* renditions still declare edupub (proving
/// the trigger isn't "metadata.xml always needs a dc:type", which would
/// have been a false positive on every ordinary, non-edupub multi-
/// rendition package). Once a publication is edupub by that definition,
/// every level (metadata.xml and each rendition) must declare it too;
/// whichever level doesn't gets its own RSC-005. Checked once for the
/// whole publication (not per-rendition, unlike the other EDUPUB checks)
/// since it needs `metadata.xml`, which `opf::check` never sees.
pub(crate) fn check_multi_rendition_dc_type(
    ocf: &mut crate::ocf::Ocf,
    opf_paths: &[String],
    report: &mut Report,
) {
    const METADATA: &str = "META-INF/metadata.xml";
    if !ocf.has(METADATA) {
        return;
    }
    let pub_dc_type = dc_type_of(ocf, METADATA);
    let rendition_dc_types: Vec<(String, Option<String>)> = opf_paths
        .iter()
        .map(|p| (p.clone(), dc_type_of(ocf, p)))
        .collect();

    let is_edupub_pub = is_edupub(pub_dc_type.as_deref())
        || rendition_dc_types
            .iter()
            .any(|(_, t)| is_edupub(t.as_deref()));
    if !is_edupub_pub {
        return;
    }

    if !is_edupub(pub_dc_type.as_deref()) {
        report.push_at_rule(
            RSC_005,
            Severity::Error,
            "META-INF/metadata.xml is missing the publication-level dc:type",
            METADATA,
            "edupub.multi_rendition.missing_publication_dc_type",
            Vec::new(),
        );
    }
    for (opf_path, dc_type) in &rendition_dc_types {
        if !is_edupub(dc_type.as_deref()) {
            report.push_at_rule(
                RSC_005,
                Severity::Error,
                "this rendition is missing dc:type for an edupub multi-rendition publication",
                opf_path.clone(),
                "edupub.multi_rendition.missing_rendition_dc_type",
                Vec::new(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranks(body: &str) -> Vec<(u32, String)> {
        let xml = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\">\
             <head><title>t</title></head><body>{body}</body></html>"
        );
        let d = crate::ocf::parse_xml(&xml).unwrap();
        let mut report = Report::new();
        check_heading_ranks(&d, "c.xhtml", &mut report);
        report
            .messages
            .iter()
            .map(|m| (m.position.map(|p| p.line).unwrap_or(0), m.text.clone()))
            .collect()
    }

    /// The rule is relative, so the interesting assertion is *which* headings
    /// are blamed and with what expected rank - not how many.
    ///
    /// epubcheck's own `edupub-titles-invalid-missing-error.xhtml` asserts
    /// "RSC-005 is reported 3 times" and nothing more, so a port that blamed
    /// three *different* headings, or got every expected rank wrong, would
    /// score as a pass against the corpus. These are the three findings
    /// epubcheck 5.3.0 actually produces on that file, ranks included.
    #[test]
    fn heading_rank_must_match_sectioning_depth() {
        // The corpus fixture, one line per element so positions are readable.
        let got = ranks(
            "\n<h2>Explicit body section</h2>\
             \n<nav epub:type=\"toc\"><h2>Table of Contents</h2></nav>\
             \n<section aria-label=\"test\">\
             \n<section aria-label=\"implied grouping\">\
             \n<aside><header><h4>Prelim</h4></header><p>x</p></aside>\
             \n<section><h3>Sub-sub-subsection</h3><p>x</p></section>\
             \n</section></section>",
        );
        let texts: Vec<&str> = got.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "The heading rank h2 does not match the current nesting level (3)",
                "The heading rank h4 does not match the current nesting level (5)",
                "The heading rank h3 does not match the current nesting level (5)",
            ],
            "got {got:?}"
        );
        // The body's own h2 is the topmost heading and sets the baseline, so
        // it must NOT be blamed - the whole point of a relative rule.
        assert_eq!(got.len(), 3);
    }

    /// A document that steps by one from its own starting rank is silent,
    /// whatever that starting rank is. Without this the rule could be "h1 at
    /// depth 0" and still pass the test above.
    #[test]
    fn a_consistent_hierarchy_is_silent_at_any_starting_rank() {
        for (top, sub) in [("h1", "h2"), ("h2", "h3"), ("h3", "h4")] {
            let got = ranks(&format!(
                "<{top}>T</{top}><section><{sub}>S</{sub}></section>"
            ));
            assert!(got.is_empty(), "{top}/{sub} should be silent, got {got:?}");
        }
    }

    /// A `role="heading"` with no `aria-level` is still a heading.
    ///
    /// epubcheck's selector is a bare `html:*[@role='heading']`; ours had a
    /// second, stricter predicate that also demanded `aria-level`, so a body
    /// whose only heading was such an element was reported as having none -
    /// a false positive against 5.3.0, which is silent on the same book.
    #[test]
    fn a_role_heading_without_aria_level_counts_as_a_heading() {
        let xml = "<html xmlns=\"http://www.w3.org/1999/xhtml\">\
             <head><title>t</title></head>\
             <body><span role=\"heading\">Top</span>\
             <section><h3>Sub</h3></section></body></html>";
        let d = crate::ocf::parse_xml(xml).unwrap();
        let body = d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "body")
            .unwrap();
        assert!(find_heading(body).is_some());

        let mut report = Report::new();
        check_sectioning_and_headings(&d, "c.xhtml", &mut report);
        assert!(
            !report
                .messages
                .iter()
                .any(|m| m.text.contains("requires a heading")),
            "got {:?}",
            report.messages.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
    }

    /// Past h6 the rule stops counting and only asks for an h6.
    #[test]
    fn beyond_h6_the_rule_only_requires_h6() {
        let deep = "<section><section><section><section><section><section>\
                    <h3>x</h3></section></section></section></section></section></section>";
        let got = ranks(&format!("<h1>T</h1>{deep}"));
        assert_eq!(
            got.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
            vec!["The current heading rank should be h6"],
            "got {got:?}"
        );
    }
}
