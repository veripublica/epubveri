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

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

fn is_x(n: roxmltree::Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(XHTML_NS)
}

fn is_x_named(n: roxmltree::Node, names: &[&str]) -> bool {
    is_x(n) && names.contains(&n.tag_name().name())
}

const H_NAMES: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];

/// A ranked heading: `h1`..`h6`, or any element whose `role` is exactly
/// `heading`.
fn is_heading(n: roxmltree::Node) -> bool {
    is_x_named(n, H_NAMES) || (is_x(n) && n.attr_no_ns("role") == Some("heading"))
}

/// XPath's `normalize-space`.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An element's string value: all of its descendant text.
fn string_value(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect()
}

/// A heading's rank as epubcheck computes it: `role="heading"` is checked
/// first and reads `aria-level` as a number (NaN when it is not one) or
/// defaults to 2; otherwise the digit of `hN`. NaN is kept, not replaced,
/// because XPath's comparisons with it are what decide the rank rules.
fn heading_rank(n: roxmltree::Node) -> f64 {
    if n.attr_no_ns("role") == Some("heading") {
        return match n.attr_no_ns("aria-level") {
            Some(v) => v.trim().parse::<f64>().unwrap_or(f64::NAN),
            None => 2.0,
        };
    }
    n.tag_name().name()[1..].parse::<f64>().unwrap_or(f64::NAN)
}

fn show_rank(r: f64) -> String {
    if r.is_nan() {
        "NaN".to_string()
    } else if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn has_ancestor(n: roxmltree::Node, names: &[&str]) -> bool {
    n.ancestors().skip(1).any(|a| is_x_named(a, names))
}

/// The empty-heading test: the heading's text, the `alt` of its `img`
/// children and every `aria-label` on or inside it, joined, are blank.
fn is_blank_heading(h: roxmltree::Node) -> bool {
    let mut all = string_value(h);
    for c in h.children().filter(|c| is_x_named(*c, &["img"])) {
        all.push_str(c.attr_no_ns("alt").unwrap_or(""));
    }
    for d in h.descendants().filter(|d| d.is_element()) {
        all.push_str(d.attr_no_ns("aria-label").unwrap_or(""));
    }
    normalize(&all).is_empty()
}

/// §4.2 Sectioning and §4.3 Titles and Headings: `edu-structure.sch`'s three
/// patterns, rule for rule.
///
/// Rewritten 2026-09-25 against epubcheck 5.4.0, one book per rule. The first
/// version approximated the rules from the corpus fixtures and missed four of
/// them outright (more than one heading, a blank heading, an empty
/// `aria-label`, content after a section) while applying the section rule to
/// `aside` and `nav`, which epubcheck does not, and not to `article`, which it
/// does. What a container *owns* is the Schematron's own definition: for the
/// body, headings with no sectioning ancestor; for a section or article,
/// headings whose innermost sectioning ancestor is that element - not only
/// direct children and `header` children.
pub(crate) fn check_sectioning_and_headings(
    doc: &roxmltree::Document,
    path: &str,
    report: &mut Report,
) {
    const SECTIONS: &[&str] = &["section", "article", "aside", "nav"];
    let Some(body) = doc.descendants().find(|n| is_x_named(*n, &["body"])) else {
        return;
    };
    let body_label_len = normalize(body.attr_no_ns("aria-label").unwrap_or(""))
        .chars()
        .count();

    // The container rules, shared by the body (when it is an implied
    // section) and by every section and article.
    let container = |c: roxmltree::Node, owned: Vec<roxmltree::Node>, report: &mut Report| {
        let is_body = is_x_named(c, &["body"]);
        let label = c.attr_no_ns("aria-label");
        let label_len = normalize(label.unwrap_or("")).chars().count();
        if label.is_some() && label_len == 0 {
            report.push_node(
                RSC_005,
                Severity::Error,
                "the \"aria-label\" attribute is empty",
                path,
                c,
                "edupub.sectioning.empty_aria_label",
                Vec::new(),
            );
        }
        if label_len == 0 && owned.is_empty() {
            if is_body {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "The body element requires a heading when it is used as an implied section",
                    path,
                    c,
                    "edupub.sectioning.body_missing_heading",
                    Vec::new(),
                );
            } else {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("{} does not have a heading", c.tag_name().name()),
                    path,
                    c,
                    "edupub.sectioning.missing_heading",
                    vec![c.tag_name().name().to_string()],
                );
            }
        }
        if owned.len() > 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "{} has more than one ranked heading of its own",
                    c.tag_name().name()
                ),
                path,
                c,
                "edupub.sectioning.multiple_headings",
                vec![owned.len().to_string()],
            );
        }
        if owned.len() == 1 && is_blank_heading(owned[0]) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Empty ranked heading detected",
                path,
                c,
                "edupub.heading.empty_ranked_heading",
                Vec::new(),
            );
        }
        // `normalize-space($headings) = normalize-space(@aria-label)`: with no
        // heading the left side is the empty string, so an empty label also
        // "matches"; with several, only the first is compared. Both measured
        // against 5.4.0.
        if let Some(label) = label {
            let heading = owned
                .first()
                .map(|h| normalize(&string_value(*h)))
                .unwrap_or_default();
            if heading == normalize(label) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "The value of the \"aria-label\" attribute must not be the same as the content of the heading",
                    path,
                    c,
                    "edupub.heading.aria_label_duplicates_text",
                    Vec::new(),
                );
            }
        }
    };

    // Body: an implied section when it has an XHTML child that is not
    // sectioning content.
    if body.children().any(|c| is_x(c) && !is_x_named(c, SECTIONS)) {
        let owned: Vec<_> = body
            .descendants()
            .filter(|d| is_heading(*d) && !has_ancestor(*d, SECTIONS))
            .collect();
        container(body, owned, report);
    }
    // Sections and articles (not aside or nav).
    for c in doc
        .descendants()
        .filter(|n| is_x_named(*n, &["section", "article"]))
    {
        let owned: Vec<_> = c
            .descendants()
            .filter(|d| {
                is_heading(*d)
                    && d.ancestors().skip(1).find(|a| is_x_named(*a, SECTIONS)) == Some(c)
            })
            .collect();
        container(c, owned, report);
    }

    check_heading_ranks(body, body_label_len, path, report);

    // edupub.sectioning: nothing but sections after the first section, in
    // the body or in a section.
    for n in doc.descendants().filter(|n| {
        n.is_element()
            && !is_x_named(*n, &["section"])
            && n.parent()
                .is_some_and(|p| is_x_named(p, &["body", "section"]))
    }) {
        if n.prev_siblings().any(|s| is_x_named(s, &["section"])) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "only sections may follow a section",
                path,
                n,
                "edupub.sectioning.content_after_section",
                Vec::new(),
            );
        }
    }

    // edupub.subtitles: a `p` typed exactly `subtitle` that follows a heading
    // must sit inside a `header`.
    for n in doc.descendants().filter(|n| {
        is_x_named(*n, &["p"])
            && n.attribute((EPUB_NS, "type")) == Some("subtitle")
            && n.prev_siblings().any(|s| is_x_named(s, H_NAMES))
    }) {
        if !has_ancestor(n, &["header"]) {
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

/// RSC-005: a heading's rank must match its nesting depth (`edu-structure.sch`,
/// the `edupub.headings` heading rule).
///
/// The whole rule is relative: epubcheck derives an *expected* rank for every
/// heading from the topmost one, so a document that starts at `h2` is
/// consistent as long as it keeps stepping by one. Three parts are easy to get
/// wrong and are the Schematron's: the topmost heading ignores anything inside
/// an `aside`/`nav` and anything more than one `section`/`article` deep;
/// `body-is-section` picks a *different* formula (`rank - nest + depth` versus
/// `rank + depth - 1`) and counts `aside`/`nav` children as non-section; and
/// past `h5` the rule asks only for an `h6`. The ranks are floats because a
/// non-numeric `aria-level` is NaN there, and every comparison with NaN is
/// false - which is what decides whether such a heading is reported.
fn check_heading_ranks(
    body: roxmltree::Node,
    body_label_len: usize,
    path: &str,
    report: &mut Report,
) {
    let headings: Vec<roxmltree::Node> = body.descendants().filter(|n| is_heading(*n)).collect();
    let body_is_section = body
        .children()
        .any(|c| is_x(c) && !is_x_named(c, &["article", "section"]));
    let topmost = headings.iter().find(|h| {
        !has_ancestor(**h, &["aside", "nav"])
            && h.ancestors()
                .skip(1)
                .filter(|a| is_x_named(*a, &["section", "article"]))
                .count()
                <= 1
    });
    let (top_rank, top_nest) = if body_label_len > 0 {
        (1.0, 0.0)
    } else {
        match topmost {
            Some(h) => (
                heading_rank(*h),
                if has_ancestor(*h, &["section", "article", "nav"]) {
                    1.0
                } else {
                    0.0
                },
            ),
            None => (1.0, 0.0),
        }
    };
    for h in &headings {
        let rank = heading_rank(*h);
        if has_ancestor(*h, &["figure", "blockquote"]) {
            report.push_node(
                RSC_005,
                Severity::Error,
                "Ranked headings are not valid in figure or blockquote",
                path,
                *h,
                "edupub.headings.rank_in_sectioning_root",
                Vec::new(),
            );
        }
        let depth = h
            .ancestors()
            .skip(1)
            .filter(|a| is_x_named(*a, &["section", "article", "aside", "nav"]))
            .count() as f64;
        let expected = if body_is_section {
            top_rank - top_nest + depth
        } else {
            top_rank + depth - 1.0
        };
        #[allow(clippy::float_cmp)]
        let matches = rank == expected;
        if expected < 6.0 && !matches {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!(
                    "The heading rank h{} does not match the current nesting level ({})",
                    show_rank(rank),
                    show_rank(expected)
                ),
                path,
                *h,
                "edupub.headings.rank_mismatch",
                vec![show_rank(rank), show_rank(expected)],
            );
        }
        if expected > 5.0 && rank < 6.0 {
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
    /// Where the first `toc` link is: epubcheck reports NAV-004..008 there,
    /// and against the package only when the `toc` has no links.
    first_toc_link: Option<(String, Position)>,
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
    pub(crate) fn set_nav(&mut self, d: &roxmltree::Document, path: &str) {
        for nav in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "nav")
        {
            match nav.attribute(EPUB_TYPE) {
                Some("toc") => {
                    let links: Vec<_> = nav
                        .descendants()
                        .filter(|n| {
                            n.is_element()
                                && n.tag_name().name() == "a"
                                && n.attribute("href").is_some_and(|h| !h.trim().is_empty())
                        })
                        .collect();
                    if self.first_toc_link.is_none()
                        && let Some(a) = links.first()
                    {
                        self.first_toc_link = Some((path.to_string(), Position::of(*a)));
                    }
                    self.toc_links += links.len();
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
        let push = |report: &mut Report, id, text: String| match &self.first_toc_link {
            Some((nav, pos)) => report.push_at_pos(id, Severity::Usage, text, nav.as_str(), *pos),
            None => report.push_at(id, Severity::Usage, text, opf_path),
        };
        if self.sections != self.toc_links {
            push(
                report,
                NAV_004,
                "the navigation document's heading hierarchy is incomplete: the number \
                 of sections doesn't match the number of toc links"
                    .to_string(),
            );
        }
        for (present, has_nav, id, kind, list) in [
            (self.audio, self.loa, NAV_005, "audio", "loa"),
            (self.figure, self.loi, NAV_006, "figure", "loi"),
            (self.table, self.lot, NAV_007, "table", "lot"),
            (self.video, self.lov, NAV_008, "video", "lov"),
        ] {
            if present && !has_nav {
                push(
                    report,
                    id,
                    format!(
                        "content documents contain <{kind}> elements but the navigation \
                         document has no \"{list}\" nav"
                    ),
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
    // `edu-opf.sch` runs when the profile is EDUPUB or a dc:type is, the
    // type matched case-insensitively (`OPFChecker`'s validator map). Its
    // rules then ask for the exact value, so `EDUPUB` selects the schema and
    // fails its first rule.
    let applies =
        profile == Some("edupub") || dc_types.iter().any(|t| t.eq_ignore_ascii_case("edupub"));
    if !applies {
        return;
    }
    let push_md = |report: &mut Report, id, sev, text: &str, rule: &'static str| match metadata {
        Some(md) => report.push_node(id, sev, text, opf_path, md, rule, Vec::new()),
        None => report.push_at_rule(id, sev, text, opf_path, rule, Vec::new()),
    };
    if !dc_types.iter().any(|t| t == "edupub") {
        push_md(
            report,
            RSC_005,
            Severity::Error,
            "The dc:type identifier \"edupub\" is required",
            "edupub.metadata.missing_dc_type",
        );
    }
    let Some(md) = metadata else { return };
    let metas = |property: &str| -> Vec<roxmltree::Node> {
        md.children()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() == "meta"
                    && n.attr_no_ns("property").map(str::trim) == Some(property)
            })
            .collect()
    };

    let features = metas("schema:accessibilityFeature");
    if features.is_empty() {
        push_md(
            report,
            RSC_005,
            Severity::Error,
            "At least one schema:accessibilityFeature declaration is required",
            "edupub.metadata.missing_accessibility_feature",
        );
    }
    if features.iter().any(|f| elem_text(*f) == "none") {
        push_md(
            report,
            RSC_005,
            Severity::Error,
            "value \"none\" is not valid in edupub",
            "edupub.metadata.invalid_accessibility_feature_none",
        );
    }

    // One warning per `teacher-edition` dc:type, as the Schematron's rule
    // context is the dc:type element itself.
    let has_source = md.children().any(|n| {
        n.is_element() && n.tag_name().name() == "source" && n.tag_name().namespace() == Some(DC_NS)
    });
    if !has_source {
        for _ in dc_types.iter().filter(|t| *t == "teacher-edition") {
            push_md(
                report,
                RSC_017,
                Severity::Warning,
                "A teacher\u{2019}s edition should identify the corresponding student edition",
                "edupub.metadata.teacher_edition_missing_source",
            );
        }
    }

    // The audience recommendations. epubcheck's patterns for higher
    // education and professional audiences can never fire (their variable
    // only ever holds a "schools" meta), and the schools one fires only when
    // the value is written exactly `schools`: the meta is selected
    // case-insensitively but compared as written.
    let schools = metas("schema:audienceType")
        .into_iter()
        .any(|m| string_value(m) == "schools");
    if schools {
        let present = |p: &str| !metas(p).is_empty();
        for (property, strength) in [
            ("schema:educationalAlignment", "recommended"),
            ("schema:educationalRole", "strongly recommended"),
            ("schema:educationalUse", "strongly recommended"),
            ("schema:interactivityType", "strongly recommended"),
            ("schema:isBasedOnUrl", "recommended"),
            ("schema:learningResourceType", "recommended"),
            ("schema:typicalAgeRange", "recommended"),
        ] {
            if !present(property) {
                report.push_node(
                    RSC_017,
                    Severity::Warning,
                    format!("for a schools audience, a {property} property is {strength}"),
                    opf_path,
                    md,
                    "edupub.metadata.schools_audience_property",
                    vec![property.to_string()],
                );
            }
        }
    }
}

/// The EDUPUB semantic structure rules (`edu-semantics.sch`). Each context
/// is an element whose `epub:type` is *exactly* the term - the Schematron
/// compares the whole attribute, so `epub:type="answers chapter"` is not
/// asked anything - and each asks for a descendant carrying another term
/// exactly.
pub(crate) fn check_semantics(doc: &roxmltree::Document, path: &str, report: &mut Report) {
    let typed =
        |n: roxmltree::Node, t: &str| n.is_element() && n.attribute((EPUB_NS, "type")) == Some(t);
    let has_desc = |n: roxmltree::Node, t: &str| n.descendants().skip(1).any(|d| typed(d, t));
    let count_desc =
        |n: roxmltree::Node, t: &str| n.descendants().skip(1).filter(|d| typed(*d, t)).count();
    const CONTAINERS: &[(&str, &str)] = &[
        ("answers", "answer"),
        ("assessments", "assessment"),
        ("bibliography", "biblioentry"),
        ("credits", "credit"),
        ("footnotes", "footnote"),
        ("keywords", "keyword"),
        ("learning-objectives", "learning-objective"),
        ("learning-outcomes", "learning-outcome"),
        ("learning-resources", "learning-resource"),
        ("learning-standards", "learning-standard"),
        ("practices", "practice"),
        ("rearnotes", "rearnote"),
    ];
    // "multiple-choice", not "multiple-choice-problem": that is the term the
    // Schematron's context names.
    const PROBLEMS: &[&str] = &[
        "fill-in-the-blank-problem",
        "general-problem",
        "match-problem",
        "multiple-choice",
        "true-false-problem",
    ];
    for n in doc.descendants().filter(|n| n.is_element()) {
        for (outer, inner) in CONTAINERS {
            if typed(n, outer) && !has_desc(n, inner) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("an element typed \"{outer}\" holds no element typed \"{inner}\""),
                    path,
                    n,
                    "edupub.semantics.missing_member",
                    vec![outer.to_string(), inner.to_string()],
                );
            }
        }
        for problem in PROBLEMS {
            if typed(n, problem) {
                if !has_desc(n, "question") {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!(
                            "an element typed \"{problem}\" holds no element typed \"question\""
                        ),
                        path,
                        n,
                        "edupub.semantics.problem_without_question",
                        vec![problem.to_string()],
                    );
                }
                if count_desc(n, "answer") >= 2 {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!("an element typed \"{problem}\" holds more than one element typed \"answer\""),
                        path,
                        n,
                        "edupub.semantics.problem_with_answers",
                        vec![problem.to_string()],
                    );
                }
            }
        }
        if typed(n, "ordinal") && has_desc(n, "ordinal") {
            report.push_node(
                RSC_005,
                Severity::Error,
                "an element typed \"ordinal\" contains another",
                path,
                n,
                "edupub.semantics.nested_ordinal",
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
        let body = d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "body")
            .unwrap();
        let label = normalize(body.attr_no_ns("aria-label").unwrap_or(""))
            .chars()
            .count();
        check_heading_ranks(body, label, "c.xhtml", &mut report);
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

    /// Rule keys the structure and semantics rules give a body, sorted.
    fn structure(body: &str) -> Vec<&'static str> {
        let xml = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\">\
             <head><title>t</title></head><body>{body}</body></html>"
        );
        let d = crate::ocf::parse_xml(&xml).unwrap();
        let mut report = Report::new();
        check_sectioning_and_headings(&d, "c.xhtml", &mut report);
        check_semantics(&d, "c.xhtml", &mut report);
        let mut v: Vec<_> = report.messages.iter().filter_map(|m| m.rule).collect();
        v.sort();
        v
    }

    /// One case per `edu-structure.sch` / `edu-semantics.sch` rule, each
    /// compared with epubcheck 5.4.0 on a book of its own (53 books, equal in
    /// ids and lines) on 2026-09-25.
    #[test]
    fn edupub_content_rules_match_epubcheck() {
        assert!(structure("<h1>T</h1><p>x</p>").is_empty());
        assert!(structure("<section aria-label=\"Part\"><p>x</p></section>").is_empty());
        assert_eq!(
            structure("<h1>A</h1><h1>B</h1><p>x</p>"),
            ["edupub.sectioning.multiple_headings"]
        );
        assert_eq!(
            structure("<h1> </h1><p>x</p>"),
            ["edupub.heading.empty_ranked_heading"]
        );
        assert!(structure("<h1><img src=\"i.png\" alt=\"Title\"/></h1><p>x</p>").is_empty());
        // An article is asked, an aside is not.
        assert_eq!(
            structure("<article><p>x</p></article>"),
            ["edupub.sectioning.missing_heading"]
        );
        assert!(structure("<h1>T</h1><aside><p>x</p></aside>").is_empty());
        // A heading inside a div still belongs to its section.
        assert!(structure("<section><div><h1>A</h1></div><p>x</p></section>").is_empty());
        assert_eq!(
            structure("<section aria-label=\"\"><p>x</p></section>"),
            [
                "edupub.heading.aria_label_duplicates_text",
                "edupub.sectioning.empty_aria_label",
                "edupub.sectioning.missing_heading",
            ]
        );
        // With two headings the label is compared with the first.
        assert_eq!(
            structure("<section aria-label=\"A\"><h1>A</h1><h1>B</h1></section>"),
            [
                "edupub.heading.aria_label_duplicates_text",
                "edupub.sectioning.multiple_headings"
            ]
        );
        assert_eq!(
            structure("<section aria-label=\"B\"><h1>A</h1><h1>B</h1></section>"),
            ["edupub.sectioning.multiple_headings"]
        );
        assert_eq!(
            structure("<section><h1>A</h1></section><p>after</p>"),
            [
                "edupub.sectioning.body_missing_heading",
                "edupub.sectioning.content_after_section"
            ]
        );
        assert_eq!(
            structure("<h1>A</h1><p epub:type=\"subtitle\">s</p>"),
            ["edupub.sectioning.subtitle_not_wrapped"]
        );
        assert!(structure("<h1>A</h1><p epub:type=\"subtitle chapter\">s</p>").is_empty());
        // Semantics compare the whole attribute.
        assert_eq!(
            structure("<h1>A</h1><div epub:type=\"answers\"><p>x</p></div>"),
            ["edupub.semantics.missing_member"]
        );
        assert!(
            structure("<h1>A</h1><div epub:type=\"answers chapter\"><p>x</p></div>").is_empty()
        );
        assert_eq!(
            structure(
                "<h1>A</h1><div epub:type=\"multiple-choice\"><p epub:type=\"answer\">a</p><p epub:type=\"answer\">b</p></div>"
            ),
            [
                "edupub.semantics.problem_with_answers",
                "edupub.semantics.problem_without_question"
            ]
        );
        assert!(
            structure("<h1>A</h1><div epub:type=\"multiple-choice-problem\"><p>x</p></div>")
                .is_empty()
        );
        assert_eq!(
            structure(
                "<h1>A</h1><span epub:type=\"ordinal\">1<span epub:type=\"ordinal\">a</span></span>"
            ),
            ["edupub.semantics.nested_ordinal"]
        );
    }

    /// `edu-opf.sch`, measured against 5.4.0 on 2026-09-25.
    #[test]
    fn edupub_package_rules_match_epubcheck() {
        let run = |metadata: &str, profile: Option<&str>| {
            let xml = format!(
                "<package xmlns=\"http://www.idpf.org/2007/opf\"><metadata \
                 xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{metadata}</metadata></package>"
            );
            let d = crate::ocf::parse_xml(&xml).unwrap();
            let md = d
                .descendants()
                .find(|n| n.tag_name().name() == "metadata")
                .unwrap();
            let types: Vec<String> = md
                .children()
                .filter(|n| n.tag_name().name() == "type")
                .map(elem_text)
                .collect();
            let mut report = Report::new();
            check_teacher_edition_and_accessibility(
                &types,
                profile,
                Some(md),
                "p.opf",
                &mut report,
            );
            let mut v: Vec<_> = report.messages.iter().filter_map(|m| m.rule).collect();
            v.sort();
            v
        };
        const OK: &str = "<dc:type>edupub</dc:type><meta property=\"schema:accessibilityFeature\">tableOfContents</meta>";
        assert!(run(OK, None).is_empty());
        // Not an edupub: a teacher's edition alone is not asked anything
        // (this was a false positive, a warning epubcheck never gives).
        assert!(run("<dc:type>teacher-edition</dc:type>", None).is_empty());
        // One warning per teacher-edition type.
        assert_eq!(
            run(
                &format!(
                    "{OK}<dc:type>teacher-edition</dc:type><dc:type>teacher-edition</dc:type>"
                ),
                None
            ),
            ["edupub.metadata.teacher_edition_missing_source"; 2]
        );
        assert_eq!(
            run(
                "<dc:type>edupub</dc:type><meta property=\"schema:accessibilityFeature\">none</meta>",
                None
            ),
            ["edupub.metadata.invalid_accessibility_feature_none"]
        );
        // A forced profile asks everything, not only for the dc:type.
        assert_eq!(
            run("", Some("edupub")),
            [
                "edupub.metadata.missing_accessibility_feature",
                "edupub.metadata.missing_dc_type"
            ]
        );
        // Selected by "EDUPUB", then failed by its exact-value rule.
        assert_eq!(
            run(
                "<dc:type>EDUPUB</dc:type><meta property=\"schema:accessibilityFeature\">tableOfContents</meta>",
                None
            ),
            ["edupub.metadata.missing_dc_type"]
        );
        let schools = |v: &str| {
            run(
                &format!("{OK}<meta property=\"schema:audienceType\">{v}</meta>"),
                None,
            )
            .len()
        };
        assert_eq!(schools("schools"), 7);
        assert_eq!(schools("Schools"), 0);
        assert_eq!(schools(" schools "), 0);
        assert_eq!(schools("higher-ed"), 0);
    }
}
