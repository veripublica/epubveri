//! NCX (EPUB 2 table of contents) content checks. Plain XML, so
//! `roxmltree` (via `ocf::parse_xml`) handles it directly — no new parser
//! needed. Before this, the NCX file was only checked for existence and
//! correct media-type (via the spine `toc` attribute, `OPF-050`); its
//! internal structure was never parsed.

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

pub(crate) fn check(ncx_xml: &str, ncx_path: &str, package_uid: &str, report: &mut Report) {
    let d = match crate::ocf::parse_xml(ncx_xml) {
        Ok(d) => d,
        Err(e) => {
            // An NCX that isn't well-formed XML used to return here without a
            // word, so every check below - playOrder, the uid agreement, the
            // whole structure - quietly didn't run and the book reported as
            // though its table of contents were fine. Nothing else parses
            // this file, so the silence was total: a `</navMapX>` typo was
            // invisible.
            //
            // Reporting it costs nothing in false positives. epubcheck's own
            // `ncx-2005-1.dtd` declares four parameter entities and no named
            // character entities at all, so a `&nbsp;` in an NCX is malformed
            // there too - unlike an XHTML content document, this file has no
            // DTD-declared entity set to be lenient about.
            report.push_full(
                RSC_016,
                Severity::Fatal,
                format!(
                    "NCX is not well-formed XML: {}",
                    crate::ocf::parse_error_detail(ncx_xml, &e)
                ),
                ncx_path,
                Position::of_parse_error(&e),
                "ncx.malformed_xml",
                Vec::new(),
            );
            return;
        }
    };
    let root = d.root_element();

    if let Some(head) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "head")
        && let Some(meta) = head.children().find(|n| {
            n.is_element()
                && n.tag_name().name() == "meta"
                && n.attr_no_ns("name") == Some("dtb:uid")
        })
        && let Some(content) = meta.attr_no_ns("content")
    {
        // NCX-004 (usage): the dtb:uid value has leading/trailing whitespace.
        // epubcheck reports this for both EPUB 2 and EPUB 3 (its #669); it is
        // independent of whether the (trimmed) value matches the package id.
        if content != content.trim() {
            report.push_at_pos(
                NCX_004,
                Severity::Usage,
                "the dtb:uid has leading or trailing whitespace".to_string(),
                ncx_path,
                Position::of(meta),
            );
        }
        // NCX-001: the dtb:uid doesn't match the package's identifier.
        //
        // Nothing to compare against when the package has no identifier
        // *value*. epubcheck guards this on
        // `featureReport.hasFeature(UNIQUE_IDENT)`, and that feature is only
        // recorded inside `OPFHandler`'s `if (idval != null)` - so a book
        // whose `<dc:identifier/>` is empty draws no NCX-001 there. Reporting
        // one says the dtb:uid "does not match ''", which blames the NCX for
        // a defect in the OPF that is already reported on its own.
        if !package_uid.trim().is_empty() && content.trim() != package_uid.trim() {
            report.push_full(
                NCX_001,
                Severity::Error,
                format!(
                    "dtb:uid '{}' does not match the package's identifier '{}'",
                    content.trim(),
                    package_uid.trim()
                ),
                ncx_path,
                Position::of(meta),
                "ncx.uid.package_identifier_mismatch",
                vec![content.trim().to_string(), package_uid.trim().to_string()],
            );
        }
    }

    if let Some(doc_title) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "docTitle")
    {
        check_empty_text(doc_title, ncx_path, report);
    }

    for nav_label in d
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "navLabel")
    {
        check_empty_text(nav_label, ncx_path, report);
    }

    check_id_attributes(&d, ncx_path, report);
    check_page_target_types(&d, ncx_path, report);
    check_play_order(&d, ncx_path, report);
    check_nav_point_model(&d, ncx_path, report);
    check_play_order_sequence(&d, ncx_path, report);
    check_page_target_uniqueness(&d, ncx_path, report);
    check_multi_lang_siblings(&d, ncx_path, report);
    check_schema(&d, ncx_path, report);
}

/// Every `id` attribute anywhere in the NCX must be a valid XML NCName
/// (confirmed via a real fixture using `np:1`, invalid only because of the
/// colon) and unique document-wide (confirmed via a real fixture where
/// `navMap` and `navPoint` share one value, reported once *per* colliding
/// element - 2 findings for 2 elements, not 1 for the pair).
fn check_id_attributes(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    let mut by_id: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id") {
            if !is_valid_ncname(id) {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("value of attribute \"id\" is invalid: '{id}'"),
                    ncx_path,
                    n,
                    "ncx.ids.invalid_ncname",
                    vec![id.to_string()],
                );
            }
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("id")
            && by_id.get(id).copied().unwrap_or(0) > 1
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("The \"id\" attribute does not have a unique value: '{id}'"),
                ncx_path,
                n,
                "ncx.ids.duplicate_id",
                vec![id.to_string()],
            );
        }
    }
}

pub(crate) fn is_valid_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A `pageTarget`'s `type` must be one of the three DAISY-defined values.
fn check_page_target_types(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "pageTarget")
    {
        if let Some(ty) = n.attr_no_ns("type")
            && !matches!(ty, "front" | "normal" | "special")
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("value of attribute \"type\" is invalid: '{ty}'"),
                ncx_path,
                n,
                "ncx.page_target.invalid_type",
                vec![ty.to_string()],
            );
        }
    }
}

/// **The four `playOrder` rules interlock, and satisfying one naively breaks
/// another** — worth reading together before touching any of them. Raised by
/// epubsana (2026-08-09) after building a repairer for them: its first version
/// numbered by file position and was target-blind, which would have *created*
/// `target_mismatch` on a book whose navigation reaches one position by two
/// routes. No shelf book has that shape, so no test of theirs could have caught
/// it; reading the rules together did.
///
/// Taken as a set, with `k` distinct targets:
///
/// - `duplicate` (Match2) — one number, one target;
/// - `target_mismatch` (Match) — one target, one number;
///
/// together these make the target↔number correspondence a **bijection**, and
///
/// - `no_origin` — some element carries `"1"`;
/// - `gap` — every `n > 1` present has `n - 1` present;
///
/// together these make the numbers used exactly `1..=k`.
///
/// **What that does *not* pin is which target gets which number.** epubsana's
/// note says the constraints admit exactly one assignment; they admit `k!` of
/// them, and document order is the one that is *meaningful* rather than the one
/// the rules force. `playOrder` is the reading order by definition, but none of
/// the four rules — nor epubcheck's `ncx.sch`, which they are ported from —
/// compares against document position. A repairer that assigned any permutation
/// would pass all four here and in epubcheck while producing nonsense.
///
/// So the argument for numbering in document order rests on what `playOrder`
/// *means*, not on what these rules check. Worth keeping straight: it is the
/// difference between a constraint we could tighten and a semantic expectation
/// we deliberately do not enforce, since epubcheck does not either.
///
/// `playOrder` is optional, but where present it must be unique across
/// every `navPoint`/`navTarget`/`pageTarget` in the document - it *is* the
/// reading order, so two elements claiming the same position is a
/// contradiction.
///
/// The one exception is the reason this can't be a plain duplicate scan:
/// elements that point at the **same target** may share a playOrder, since
/// they name one position reached by two routes (a navPoint and the
/// pageTarget for the same page, say). So a value is only a violation when
/// the elements carrying it disagree about where they go, and then every one
/// of them is reported - the defect is the collision, not one arbitrary
/// member of it, and a reader given a single line would have to hunt for the
/// other. Matches epubcheck, which reports each colliding element.
///
/// (Reported missing by Doitsu on the MobileRead forum: epubcheck flagged
/// four elements on a real EPUB 2 book where epubveri flagged none.)
/// The `navPoint` content model: one or more `navLabel`, then exactly one
/// `content`, then any number of nested `navPoint`s (`schema/20/rng/ncx.rng`,
/// which is the grammar epubcheck loads - `XMLValidators.NCX_RNG`, not the
/// `ncx-old.rng` sitting beside it, whose `playOrder` is required and would
/// have produced a finding epubcheck does not).
///
/// Nothing here validated the NCX's *structure* before: `ncx.rs` checked
/// playOrder, id uniqueness, duplicate `navLabel`/`navInfo` and empty text,
/// so a `<navPoint id="x"><content src="…"/></navPoint>` - no label at all -
/// was accepted. Reported on MobileRead with a 2.4 KB book (#79).
///
/// The three messages reproduce what the RELAX NG validator says, measured
/// one book per shape against 5.3.0:
///
/// - `content` before any `navLabel` - one error, at the `content`;
/// - a `navLabel` after the `content` - a second error, at the `navLabel`
///   (so `<content/><navLabel/>` is two findings, not one);
/// - no `content` at all - one error, at the `navPoint`.
fn check_nav_point_model(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    for np in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
    {
        let mut seen_label = false;
        let mut seen_content = false;
        for c in np.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "navLabel" => {
                    if seen_content {
                        report.push_node(
                            RSC_005,
                            Severity::Error,
                            "element \"navLabel\" is not allowed here; it must precede \"content\"",
                            ncx_path,
                            c,
                            "ncx.nav_point.label_after_content",
                            Vec::new(),
                        );
                    }
                    seen_label = true;
                }
                "content" => {
                    if !seen_label {
                        report.push_node(
                            RSC_005,
                            Severity::Error,
                            "element \"content\" is not allowed yet; \"navPoint\" requires a \"navLabel\" first",
                            ncx_path,
                            c,
                            "ncx.nav_point.content_before_label",
                            Vec::new(),
                        );
                    }
                    seen_content = true;
                }
                _ => {}
            }
        }
        if !seen_content {
            report.push_node(
                RSC_005,
                Severity::Error,
                "element \"navPoint\" is incomplete; it requires a \"content\" element",
                ncx_path,
                np,
                "ncx.nav_point.missing_content",
                Vec::new(),
            );
        }
    }
}

fn check_play_order(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    use std::collections::HashMap;

    // playOrder -> the elements claiming it, each with the target it names.
    let mut claims: HashMap<&str, Vec<(roxmltree::Node, String)>> = HashMap::new();
    for n in doc.descendants().filter(|n| {
        n.is_element() && matches!(n.tag_name().name(), "navPoint" | "navTarget" | "pageTarget")
    }) {
        let Some(order) = n.attr_no_ns("playOrder") else {
            continue;
        };
        let target = n
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "content")
            .and_then(|c| c.attr_no_ns("src"))
            .unwrap_or_default()
            .trim()
            .to_string();
        claims.entry(order).or_default().push((n, target));
    }

    // Collected first, then reported in document order: `claims` is keyed by
    // a hash, so reporting straight out of it would order the findings
    // differently from run to run. epubcheck reports these in document
    // order, and so should we - a report that reshuffles itself between
    // identical runs is one nobody can diff.
    let mut offenders: Vec<(roxmltree::Node, &str)> = Vec::new();
    for (order, holders) in &claims {
        if holders.len() < 2 {
            continue;
        }
        let first = &holders[0].1;
        if holders.iter().all(|(_, t)| t == first) {
            // One position, reached by several routes - legitimate.
            continue;
        }
        offenders.extend(holders.iter().map(|(n, _)| (*n, *order)));
    }
    offenders.sort_by_key(|(n, _)| n.range().start);
    for (n, order) in offenders {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!(
                "identical playOrder value '{order}' on elements that do not refer to the same target"
            ),
            ncx_path,
            n,
            "ncx.play_order.duplicate",
            vec![order.to_string()],
        );
    }
}

/// The rest of `schema/20/sch/ncx.sch`'s `playOrder` model (#59), alongside
/// `ncx_playOrderMatch2` above:
///
/// - **`ncx_playOrderOrigin`** — if anything carries a `playOrder`, some
///   element must carry `playOrder="1"`. Reading order has to start.
/// - **`ncx_playOrderNoGaps`** — for a `playOrder` above 1, the value one
///   below must exist somewhere.
/// - **`ncx_playOrderMatch`** — the converse of Match2: elements pointing at
///   the *same* target must carry the *same* `playOrder`.
///
/// Two details taken from the Schematron rather than from intuition: origin
/// compares as a **string** (`@playOrder='1'`), so a padded `"01"` does not
/// satisfy it, while no-gaps compares numerically; and every rule is
/// per-element, so a bad NCX names each offender rather than one line.
fn check_play_order_sequence(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    use std::collections::{HashMap, HashSet};

    // Every element carrying a playOrder, with its raw value and the target
    // it names - the same population `check_play_order` walks.
    let holders: Vec<(roxmltree::Node, &str, String)> = doc
        .descendants()
        .filter(|n| {
            n.is_element() && matches!(n.tag_name().name(), "navPoint" | "navTarget" | "pageTarget")
        })
        .filter_map(|n| {
            n.attr_no_ns("playOrder").map(|order| {
                let target = n
                    .children()
                    .find(|c| c.is_element() && c.tag_name().name() == "content")
                    .and_then(|c| c.attr_no_ns("src"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                (n, order.trim(), target)
            })
        })
        .collect();
    if holders.is_empty() {
        return;
    }

    // Origin: string comparison, per the Schematron.
    if !holders.iter().any(|(_, order, _)| *order == "1") {
        for (n, order, _) in &holders {
            report.push_node(
                RSC_005,
                Severity::Error,
                "no element carries playOrder \"1\"; the sequence must start at 1",
                ncx_path,
                *n,
                "ncx.play_order.no_origin",
                vec![order.to_string()],
            );
        }
    }

    // No gaps: numeric. A non-numeric value simply takes no part, exactly as
    // XPath's `number()` makes it NaN and drops it from both sides.
    let numbers: HashSet<i64> = holders
        .iter()
        .filter_map(|(_, order, _)| order.parse::<i64>().ok())
        .collect();
    for (n, order, _) in &holders {
        if let Ok(v) = order.parse::<i64>()
            && v > 1
            && !numbers.contains(&(v - 1))
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("playOrder '{v}' has no predecessor '{}'", v - 1),
                ncx_path,
                *n,
                "ncx.play_order.gap",
                vec![v.to_string()],
            );
        }
    }

    // Match: one target, one position. Only elements that actually name a
    // target take part - the Schematron's context requires `ncx:content`.
    let mut by_target: HashMap<&str, Vec<(roxmltree::Node, &str)>> = HashMap::new();
    for (n, order, target) in &holders {
        if !target.is_empty() {
            by_target.entry(target).or_default().push((*n, order));
        }
    }
    let mut offenders: Vec<(roxmltree::Node, &str)> = Vec::new();
    for group in by_target.values() {
        let first = group[0].1;
        if group.iter().any(|(_, o)| *o != first) {
            offenders.extend(group.iter().copied());
        }
    }
    offenders.sort_by_key(|(n, _)| n.range().start);
    for (n, order) in offenders {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("playOrder '{order}' differs from another element naming the same target"),
            ncx_path,
            n,
            "ncx.play_order.target_mismatch",
            vec![order.to_string()],
        );
    }
}

/// `ncx_pageTargUniqValTypeComb` (#59): a `pageTarget`'s `value`+`type`
/// combination must be unique. The Schematron's context is a `pageTarget`
/// *with* a `@value` inside a `pageList`, but it counts across every
/// `pageTarget` in the document - so a colliding target elsewhere still
/// counts against it.
fn check_page_target_uniqueness(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    use std::collections::HashMap;

    let all: Vec<roxmltree::Node> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "pageTarget")
        .collect();
    let mut counts: HashMap<(&str, &str), u32> = HashMap::new();
    for n in &all {
        if let Some(v) = n.attr_no_ns("value") {
            *counts
                .entry((v.trim(), n.attr_no_ns("type").unwrap_or("").trim()))
                .or_default() += 1;
        }
    }
    for n in &all {
        // Only those inside a pageList are reported, matching the context.
        let in_page_list = n
            .parent()
            .is_some_and(|p| p.is_element() && p.tag_name().name() == "pageList");
        let Some(v) = n.attr_no_ns("value") else {
            continue;
        };
        let key = (v.trim(), n.attr_no_ns("type").unwrap_or("").trim());
        if in_page_list && counts.get(&key).copied().unwrap_or(0) > 1 {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("pageTarget value '{}' is not unique for its type", key.0),
                ncx_path,
                *n,
                "ncx.page_target.duplicate_value_type",
                vec![key.0.to_string(), key.1.to_string()],
            );
        }
    }
}

/// `ncx_multiNavLabel` / `ncx_multiNavInfo` (#59): siblings of these types
/// must not repeat an `xml:lang`.
///
/// The Schematron compares `@xml:lang=current()/@xml:lang`, and in XPath an
/// absent attribute is an empty node-set that equals nothing - **not even
/// another absent one**. So two `navLabel`s with no `xml:lang` at all are
/// fine, and only a repeated *explicit* language is an error. Implementing
/// this as a plain duplicate-sibling check would reject the ordinary
/// single-language NCX every EPUB 2 book ships.
fn check_multi_lang_siblings(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    use std::collections::HashMap;
    const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

    for parent in doc.descendants().filter(|n| n.is_element()) {
        for name in ["navLabel", "navInfo"] {
            let mut seen: HashMap<&str, u32> = HashMap::new();
            let sibs: Vec<roxmltree::Node> = parent
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == name)
                .collect();
            for c in &sibs {
                if let Some(lang) = c.attribute((XML_NS, "lang")) {
                    *seen.entry(lang.trim()).or_default() += 1;
                }
            }
            for c in &sibs {
                if let Some(lang) = c.attribute((XML_NS, "lang"))
                    && seen.get(lang.trim()).copied().unwrap_or(0) > 1
                {
                    report.push_node(
                        RSC_005,
                        Severity::Error,
                        format!("more than one <{name}> here carries xml:lang=\"{lang}\""),
                        ncx_path,
                        *c,
                        "ncx.nav_label.duplicate_lang",
                        vec![name.to_string(), lang.trim().to_string()],
                    );
                }
            }
        }
    }
}

fn check_empty_text(container: roxmltree::Node, ncx_path: &str, report: &mut Report) {
    let Some(text_el) = container
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "text")
    else {
        return;
    };
    // `Node::text()` returns content for comment nodes too, not just text
    // nodes - filter to real text first (same gap fixed for the
    // title-empty check in a prior increment).
    let text: String = text_el
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    if text.trim().is_empty() {
        report.push_full(
            NCX_006,
            Severity::Usage,
            "empty text label",
            ncx_path,
            Position::of(text_el),
            "ncx.nav_label.empty_text",
            Vec::new(),
        );
    }
}

/// Validate the NCX against our own grammar (`schemas/ncx.rng`, #83).
///
/// Before this the NCX's structure was checked in exactly one place — the
/// hand-coded `navPoint` model added by #79 — and every other constraint the
/// format states was unenforced. Sixteen shapes were measured against
/// epubcheck 5.3.0, one book each, and all sixteen were silent here: an empty
/// `navMap`, `pageList`, `navList` or `navLabel`; a `navTarget`/`pageTarget`
/// with no `content`; a missing `navMap` or `meta`; a `pageList` nested in the
/// `navMap` or placed before it; an undefined element; a `navPoint` with no
/// `id`; a `content` with no `src`; an undeclared attribute; and an element
/// inside `<text>`.
///
/// Reported by Doitsu on MobileRead (#193) for the first two of those. The
/// grammar rather than two more hand-coded checks, because the hand-coded
/// route closes nine of the format's ~27 constraints and leaves the rest to
/// arrive one forum report at a time — the same per-source shape that cost us
/// the `<guide>` fragment gap.
fn check_schema(doc: &roxmltree::Document, ncx_path: &str, report: &mut Report) {
    // `navPoint`'s own content model stays owned by `check_nav_point_model`
    // above, whose three messages were measured shape-by-shape against
    // epubcheck (#79) and name the fault far better than a grammar can:
    // "requires a navLabel first" against "has incomplete content". The
    // grammar sees the same defects, so without this every one of them was
    // reported twice - three findings against epubcheck's one for a
    // `navPoint` whose `content` precedes its `navLabel`.
    //
    // Asked of the report rather than assumed, the same shape as the
    // obsolete-attribute suppression in `opf.rs`: this skips a grammar blame
    // only where a `navPoint` finding was *actually produced* for that
    // element. So it cannot drift - if `check_nav_point_model` is ever
    // removed or narrowed, the grammar's own blames reappear in its place
    // rather than the defect going silent, which is the failure mode the
    // 0.7.12-0.7.14 silent-skip audit was about.
    let claimed: Vec<String> = report
        .messages
        .iter()
        .filter(|m| {
            m.rule.is_some_and(|r| r.starts_with("ncx.nav_point."))
                && m.location.as_deref() == Some(ncx_path)
        })
        .filter_map(|m| m.element_path.as_ref().map(|p| p.path.clone()))
        .collect();

    let grammar = crate::rng::ncx_grammar();
    for blame in crate::rng::validate_node_report(&grammar, doc.root_element()) {
        if is_nav_point_content_model(&blame)
            && let Some(np) = nav_point_of(blame.node())
        {
            let np_path = crate::xmlext::node_path(np).path;
            if claimed.iter().any(|c| c.starts_with(&np_path)) {
                continue;
            }
        }
        crate::opf::push_blame(report, ncx_path, "ncx.schema_violation", &blame);
    }
}

/// The grammar blames `check_nav_point_model` also produces: the `navPoint`
/// itself reported incomplete, or one of the two children whose *ordering* it
/// polices rejected at its position. A missing `id` on the `navPoint`, or any
/// blame on a deeper descendant, is the grammar's alone.
fn is_nav_point_content_model(blame: &crate::rng::Blame) -> bool {
    use crate::rng::{Blame, ElementFault};
    match blame {
        Blame::Element(n, ElementFault::IncompleteContent { .. }) => {
            n.tag_name().name() == "navPoint"
        }
        Blame::Element(n, ElementFault::NotAllowed(_)) => {
            matches!(n.tag_name().name(), "navLabel" | "content")
                && n.parent()
                    .is_some_and(|p| p.is_element() && p.tag_name().name() == "navPoint")
        }
        _ => false,
    }
}

/// The `navPoint` a blame belongs to - the element itself, or its parent when
/// the blame is on one of its children.
fn nav_point_of<'d, 'i>(node: roxmltree::Node<'d, 'i>) -> Option<roxmltree::Node<'d, 'i>> {
    if node.tag_name().name() == "navPoint" {
        return Some(node);
    }
    node.parent()
        .filter(|p| p.is_element() && p.tag_name().name() == "navPoint")
}

/// The text of a `navPoint`'s first `navLabel`, for naming it in a message.
fn nav_label_text(np: roxmltree::Node) -> String {
    np.children()
        .find(|c| c.is_element() && c.tag_name().name() == "navLabel")
        .and_then(|l| {
            l.children()
                .find(|t| t.is_element() && t.tag_name().name() == "text")
        })
        .and_then(|t| t.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// ADV-009: two *sibling* navigation entries resolve to the same document,
/// with no fragment to tell them apart — so whichever the reader picks, they
/// land in the same place and one of the two names nothing reachable.
///
/// Neither tool errors here and both are right. JSWolf reported the shape on
/// MobileRead (#195) expecting an error, having found two `navPoint`s sharing
/// a `playOrder` *and* a `content src`; epubcheck's `ncx_playOrderMatch`
/// **obliges** two entries pointing at one target to share a `playOrder`, so
/// "fixing" the duplicate number would make a valid NCX invalid. The real
/// defect is one level up, and no validator catches it — which is what this
/// advisory is for.
///
/// **The sibling restriction is structural, not a tuning.** `<content>` is
/// mandatory inside `navPoint` (see `schemas/ncx.rng`), so a purely
/// structural parent — a part heading, an omnibus volume title — has nowhere
/// to point but its first child's document. A parent/child duplicate is the
/// only legal way to write that, and reporting it would be reporting the
/// format. Measured across the shelf's 364 NCX files (2026-08-20): 12
/// duplicate targets in 6 books, of which **8 are parent/child and every one
/// of those is legitimate** — 7 in one book alone, a translation of *Der
/// Zauberberg* whose seven part headings each share a file with their first
/// chapter. Of the 4 sibling pairs, 3 are genuine defects (a chapter, a
/// second author's biography, and a diagram, each unreachable from the table
/// of contents) and 1 is Calibre listing a title page twice.
///
/// So the bar ADV-003 set is cleared on both counts: **one false alarm in 375
/// books**, against the 1-in-16.8 of the ADV-003 version that was rejected for
/// crying wolf. Note what the earlier record got wrong, because the numbers
/// are quotable and were: it counted *books* rather than findings (6, not 12),
/// and it recorded "nesting is not a discriminator — all pairs are siblings",
/// which is false and had discarded the correct first guess.
///
/// Worded as an observation rather than a verdict, deliberately. The finding
/// is *factually true* in all 12 cases — two entries really do resolve to one
/// document — and only the inference "that is probably a mistake" is
/// sometimes wrong. That is a different class of advisory from one that
/// asserts something untrue about the book.
///
/// `navPoint` only: a `pageList`'s `pageTarget`s legitimately mark positions
/// within one document, and no measurement here covers them.
pub(crate) fn check_duplicate_targets(
    doc: &roxmltree::Document,
    ncx_path: &str,
    report: &mut Report,
) {
    use std::collections::HashMap;

    // Walked in document order, so the finding lands on the later entry and
    // the message can name the earlier one.
    let mut seen: HashMap<&str, Vec<roxmltree::Node>> = HashMap::new();
    for np in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
    {
        let Some(src) = np
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "content")
            .and_then(|c| c.attr_no_ns("src"))
        else {
            continue;
        };
        let src = src.trim();
        // A fragment is precisely how the format says "a different place in
        // the same file", so entries carrying one are not landing together.
        // A remote target is nobody's table of contents entry to fix.
        if src.is_empty() || src.contains('#') || crate::url::is_absolute(src) {
            continue;
        }
        let prior = seen.entry(src).or_default();
        // Only an entry that is not an ancestor of this one counts; see the
        // structural argument above. Document order makes this one-sided.
        if let Some(earlier) = prior
            .iter()
            .find(|p| !np.ancestors().any(|a| a == **p))
            .copied()
        {
            let (a, b) = (nav_label_text(earlier), nav_label_text(np));
            report.push_node(
                ADV_009,
                Severity::Usage,
                format!(
                    "two navigation entries point to '{src}' with no fragment to \
                     tell them apart: '{a}' and '{b}'. Whichever the reader \
                     picks, they land in the same place"
                ),
                ncx_path,
                np,
                "ncx.nav_point.duplicate_target",
                vec![src.to_string(), a, b],
            );
        }
        prior.push(np);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reports (rule, line) for every finding, so a test can assert *which*
    /// elements were named rather than just how many.
    fn run_at(ncx: &str) -> Vec<(Option<&'static str>, u32)> {
        let mut report = Report::new();
        check(ncx, "toc.ncx", "uid", &mut report);
        report
            .messages
            .iter()
            .map(|m| (m.rule, m.position.map(|p| p.line).unwrap_or(0)))
            .collect()
    }

    /// The ADV-009 findings for a navMap body, as `(target, label_a, label_b)`.
    fn dup_targets(body: &str) -> Vec<(String, String, String)> {
        let xml = ncx_with(body);
        let doc = crate::ocf::parse_xml(&xml).expect("fixture parses");
        let mut report = Report::new();
        check_duplicate_targets(&doc, "toc.ncx", &mut report);
        report
            .messages
            .iter()
            .filter(|m| m.id == ADV_009)
            .map(|m| {
                (
                    m.params[0].clone(),
                    m.params[1].clone(),
                    m.params[2].clone(),
                )
            })
            .collect()
    }

    fn np(id: &str, label: &str, src: &str, children: &str) -> String {
        format!(
            "<navPoint id=\"{id}\"><navLabel><text>{label}</text></navLabel>\
             <content src=\"{src}\"/>{children}</navPoint>"
        )
    }

    /// ADV-009 reports two *sibling* navigation entries landing on one
    /// document, and stays silent when one is the other's parent.
    ///
    /// The parent case is the whole reason the check is shippable, so it is
    /// asserted first and by itself. `<content>` is mandatory in `navPoint`,
    /// so a part heading has nowhere to point but its first child's file —
    /// reporting it would be reporting the format, and it is the shape that
    /// produced 8 of the 12 duplicate targets on a 375-book shelf, every one
    /// of them legitimate.
    #[test]
    fn a_parent_sharing_its_childs_target_is_not_a_duplicate() {
        // Der Zauberberg's shape: a part heading whose first chapter is in
        // the same file. Seven of these in one real book.
        let part = np(
            "p1",
            "PART ONE",
            "ch1.xhtml",
            &np("c1", "Arrival", "ch1.xhtml", ""),
        );
        assert!(
            dup_targets(&format!("<navMap>{part}</navMap>")).is_empty(),
            "a parent pointing at its own child's document is how the format \
             expresses a section heading"
        );
        // Two levels up is the same argument, so an ancestor at any depth
        // counts - not only the direct parent.
        let deep = np(
            "p1",
            "PART ONE",
            "ch1.xhtml",
            &np(
                "s1",
                "Section",
                "ch1a.xhtml",
                &np("c1", "Arrival", "ch1.xhtml", ""),
            ),
        );
        assert!(dup_targets(&format!("<navMap>{deep}</navMap>")).is_empty());
    }

    /// The other half: siblings *are* reported, a fragment tells two entries
    /// apart, and a distinct target says nothing. Without these the test
    /// above would pass on a check that never fires at all.
    #[test]
    fn two_sibling_entries_on_one_document_are_reported() {
        let two = |a: &str, b: &str| {
            format!(
                "<navMap>{}{}</navMap>",
                np("n1", "XXXVIII", a, ""),
                np("n2", "XXXIX", b, "")
            )
        };
        assert_eq!(
            dup_targets(&two("ch38.xhtml", "ch38.xhtml")),
            vec![(
                "ch38.xhtml".to_string(),
                "XXXVIII".to_string(),
                "XXXIX".to_string()
            )],
            "one finding, on the later entry, naming both"
        );
        // A fragment is how the format says "elsewhere in the same file".
        assert!(dup_targets(&two("ch38.xhtml", "ch38.xhtml#b")).is_empty());
        assert!(dup_targets(&two("ch38.xhtml#a", "ch38.xhtml#b")).is_empty());
        // And the ordinary case stays silent.
        assert!(dup_targets(&two("ch38.xhtml", "ch39.xhtml")).is_empty());
    }

    /// Wraps a navMap body in a minimal, otherwise-valid NCX.
    fn ncx_with(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" xmlns:xml="http://www.w3.org/XML/1998/namespace" version="2005-1">
  <head><meta name="dtb:uid" content="uid"/></head>
  <docTitle><text>T</text></docTitle>
  {body}
</ncx>"#
        )
    }

    fn rules_for(body: &str) -> Vec<&'static str> {
        run_at(&ncx_with(body))
            .into_iter()
            .filter_map(|(r, _)| r)
            .collect()
    }

    /// An NCX that isn't well-formed XML used to return from `check` without
    /// reporting, and nothing else in the codebase parses this file - so a
    /// `</navMapX>` typo took every NCX check with it and the book came back
    /// clean. Measured on a probe EPUB: byte-identical output to the same
    /// book with a valid NCX.
    ///
    /// No leniency is owed here the way an XHTML content document is owed it
    /// for its DTD-declared `&nbsp;`: epubcheck's `ncx-2005-1.dtd` declares
    /// four parameter entities and no named character entities, so a named
    /// reference in an NCX is malformed for epubcheck too.
    #[test]
    fn malformed_ncx_is_reported_rather_than_skipped() {
        for (label, ncx) in [
            ("mismatched tag", ncx_with("<navMap></navMapX>")),
            ("bad numeric reference", ncx_with("<navMap>&#0;</navMap>")),
            (
                "undeclared named entity",
                ncx_with("<navMap>&nbsp;</navMap>"),
            ),
        ] {
            let rules = run_at(&ncx)
                .into_iter()
                .filter_map(|(r, _)| r)
                .collect::<Vec<_>>();
            assert!(
                rules.contains(&"ncx.malformed_xml"),
                "{label}: an unparsable NCX must say so; got {rules:?}"
            );
        }
    }

    /// #79: the `navPoint` content model — `navLabel`+, then `content`.
    ///
    /// Nothing checked the NCX's structure before, so a `navPoint` carrying
    /// only a `<content>` was accepted. Reported on MobileRead with a test
    /// book; each shape below was measured against epubcheck 5.3.0 one book
    /// per run, and the **counts** are the assertion — `<content/>` followed
    /// by `<navLabel/>` is two findings there, not one, because the order
    /// violation and the missing-label violation are separate.
    #[test]
    fn nav_point_requires_a_label_before_its_content() {
        let np = |inner: &str| format!("<navMap><navPoint id=\"n1\">{inner}</navPoint></navMap>");
        let count = |body: &str| rules_for(body).len();

        assert_eq!(
            count(&np(
                "<navLabel><text>x</text></navLabel><content src=\"1.xhtml\"/>"
            )),
            0
        );
        assert_eq!(count(&np("<content src=\"1.xhtml\"/>")), 1);
        assert_eq!(
            count(&np(
                "<content src=\"1.xhtml\"/><navLabel><text>x</text></navLabel>"
            )),
            2
        );
        assert_eq!(count(&np("<navLabel><text>x</text></navLabel>")), 1);

        // The slugs, so a future reshuffle cannot quietly swap which
        // violation is reported for which shape.
        assert!(
            rules_for(&np("<content src=\"1.xhtml\"/>"))
                .contains(&"ncx.nav_point.content_before_label")
        );
        assert!(
            rules_for(&np("<navLabel><text>x</text></navLabel>"))
                .contains(&"ncx.nav_point.missing_content")
        );
    }

    /// #59: `ncx_playOrderOrigin` and `ncx_playOrderNoGaps`.
    ///
    /// Origin compares as a **string** in the Schematron (`@playOrder='1'`),
    /// so a padded `"01"` does not satisfy it even though it is numerically
    /// one - that asymmetry with no-gaps (which is numeric) is deliberate and
    /// is why both are pinned here.
    #[test]
    fn play_order_sequence_origin_and_gaps() {
        let np = |id: &str, order: &str, src: &str| {
            format!(
                "<navPoint id=\"{id}\" playOrder=\"{order}\"><navLabel><text>x</text></navLabel>\
                 <content src=\"{src}\"/></navPoint>"
            )
        };
        // 1,2,3 - clean.
        let ok = format!(
            "<navMap>{}{}{}</navMap>",
            np("a", "1", "1.xhtml"),
            np("b", "2", "2.xhtml"),
            np("c", "3", "3.xhtml")
        );
        assert!(
            rules_for(&ok).is_empty(),
            "a clean sequence: {:?}",
            rules_for(&ok)
        );

        // Starts at 2 - no origin.
        let no_origin = format!(
            "<navMap>{}{}</navMap>",
            np("a", "2", "1.xhtml"),
            np("b", "3", "2.xhtml")
        );
        assert!(rules_for(&no_origin).contains(&"ncx.play_order.no_origin"));

        // 1,2,4 - a gap at 3.
        let gap = format!(
            "<navMap>{}{}{}</navMap>",
            np("a", "1", "1.xhtml"),
            np("b", "2", "2.xhtml"),
            np("c", "4", "3.xhtml")
        );
        assert!(rules_for(&gap).contains(&"ncx.play_order.gap"));

        // Same target, different positions - ncx_playOrderMatch.
        let mismatch = format!(
            "<navMap>{}{}</navMap>",
            np("a", "1", "same.xhtml"),
            np("b", "2", "same.xhtml")
        );
        assert!(rules_for(&mismatch).contains(&"ncx.play_order.target_mismatch"));
    }

    /// #59: `ncx_pageTargUniqValTypeComb` - the value+type *combination* is
    /// what must be unique, so the same value under a different type is fine.
    #[test]
    fn page_target_value_type_combination_must_be_unique() {
        let pt = |id: &str, value: &str, ty: &str| {
            format!(
                "<pageTarget id=\"{id}\" type=\"{ty}\" value=\"{value}\" playOrder=\"1\">\
                 <navLabel><text>{value}</text></navLabel><content src=\"a.xhtml\"/></pageTarget>"
            )
        };
        let dup = format!(
            "<pageList><navLabel><text>P</text></navLabel>{}{}</pageList>",
            pt("p1", "1", "normal"),
            pt("p2", "1", "normal")
        );
        assert!(rules_for(&dup).contains(&"ncx.page_target.duplicate_value_type"));

        let differing_type = format!(
            "<pageList><navLabel><text>P</text></navLabel>{}{}</pageList>",
            pt("p1", "1", "normal"),
            pt("p2", "1", "front")
        );
        assert!(
            !rules_for(&differing_type).contains(&"ncx.page_target.duplicate_value_type"),
            "same value under a different type is a different combination"
        );
    }

    /// #59: `ncx_multiNavLabel`/`ncx_multiNavInfo`.
    ///
    /// The negative case is the important one. The Schematron compares
    /// `@xml:lang=current()/@xml:lang`, and in XPath an absent attribute is
    /// an empty node-set that equals nothing - not even another absent one.
    /// So two `navLabel`s carrying no `xml:lang` are valid, and reading this
    /// rule as "duplicate sibling" would reject the ordinary single-language
    /// NCX that every EPUB 2 book ships.
    #[test]
    fn repeated_xml_lang_on_sibling_nav_labels() {
        let body = "<navMap><navPoint id=\"a\" playOrder=\"1\">\
             <navLabel xml:lang=\"en\"><text>A</text></navLabel>\
             <navLabel xml:lang=\"en\"><text>B</text></navLabel>\
             <content src=\"a.xhtml\"/></navPoint></navMap>";
        assert!(rules_for(body).contains(&"ncx.nav_label.duplicate_lang"));

        // Different languages: the point of allowing several.
        let multilingual = "<navMap><navPoint id=\"a\" playOrder=\"1\">\
             <navLabel xml:lang=\"en\"><text>A</text></navLabel>\
             <navLabel xml:lang=\"tr\"><text>B</text></navLabel>\
             <content src=\"a.xhtml\"/></navPoint></navMap>";
        assert!(!rules_for(multilingual).contains(&"ncx.nav_label.duplicate_lang"));

        // No xml:lang at all - must stay silent.
        let unlabelled = "<navMap><navPoint id=\"a\" playOrder=\"1\">\
             <navLabel><text>A</text></navLabel>\
             <navLabel><text>B</text></navLabel>\
             <content src=\"a.xhtml\"/></navPoint></navMap>";
        assert!(
            !rules_for(unlabelled).contains(&"ncx.nav_label.duplicate_lang"),
            "an absent xml:lang equals nothing, not even another absent one"
        );
    }

    const PLAY_ORDER_NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="uid"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Cover</text></navLabel>
      <content src="cover.xhtml"/>
    </navPoint>
    <navPoint id="n2" playOrder="2">
      <navLabel><text>Ch1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
  </navMap>
  <pageList id="pl">
    <navLabel><text>Pages</text></navLabel>
    <pageTarget id="p1" type="normal" value="1" playOrder="1">
      <navLabel><text>1</text></navLabel>
      <content src="chapter1.xhtml#page_1"/>
    </pageTarget>
    <pageTarget id="p2" type="normal" value="2" playOrder="2">
      <navLabel><text>2</text></navLabel>
      <content src="chapter1.xhtml#page_2"/>
    </pageTarget>
  </pageList>
</ncx>"#;

    /// `playOrder` is the reading position, so two elements claiming the same
    /// one while pointing somewhere different is a contradiction. Every
    /// colliding element is named: the defect is the collision, and a reader
    /// handed one line would have to hunt for its partner.
    ///
    /// Reported missing by Doitsu (MobileRead): epubcheck flags four elements
    /// on this shape, epubveri flagged none.
    #[test]
    fn duplicate_play_order_reports_every_colliding_element() {
        let got = run_at(PLAY_ORDER_NCX);
        let dups: Vec<u32> = got
            .iter()
            .filter(|(r, _)| *r == Some("ncx.play_order.duplicate"))
            .map(|(_, line)| *line)
            .collect();
        // The two navPoints and the two pageTargets, in document order.
        assert_eq!(dups, vec![6, 10, 17, 21], "got {got:?}");
    }

    /// Document order, every time. The grouping is keyed by a hash, so
    /// reporting straight out of it reshuffles the findings between
    /// identical runs — which was the first version's actual behaviour.
    #[test]
    fn duplicate_play_order_is_reported_in_a_stable_order() {
        let first = run_at(PLAY_ORDER_NCX);
        for _ in 0..8 {
            assert_eq!(run_at(PLAY_ORDER_NCX), first);
        }
    }

    /// The exception that stops this being a plain duplicate scan: one
    /// position reached by two routes is legitimate, so a shared playOrder
    /// whose elements name the *same* target must stay silent.
    #[test]
    fn same_play_order_pointing_at_the_same_target_is_valid() {
        let ncx = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="uid"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Ch1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
  </navMap>
  <pageList id="pl">
    <navLabel><text>Pages</text></navLabel>
    <pageTarget id="p1" type="normal" value="1" playOrder="1">
      <navLabel><text>1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </pageTarget>
  </pageList>
</ncx>"#;
        assert!(
            !run_at(ncx)
                .iter()
                .any(|(r, _)| *r == Some("ncx.play_order.duplicate")),
            "one position reached by two routes is not a collision"
        );
    }

    /// playOrder is optional; a document that omits it entirely has nothing
    /// to collide.
    #[test]
    fn absent_play_order_is_not_a_collision() {
        let ncx = PLAY_ORDER_NCX
            .replace(" playOrder=\"1\"", "")
            .replace(" playOrder=\"2\"", "");
        assert!(
            !run_at(&ncx)
                .iter()
                .any(|(r, _)| *r == Some("ncx.play_order.duplicate"))
        );
    }

    fn run(ncx: &str, uid: &str) -> Vec<&'static str> {
        let mut report = Report::new();
        check(ncx, "toc.ncx", uid, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    const CLEAN: &str = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
        <head><meta name="dtb:uid" content="NOID"/></head>
        <docTitle><text>Moby Dick</text></docTitle>
        <navMap>
            <navPoint id="np-1" playOrder="1">
                <navLabel><text>Loomings</text></navLabel>
                <content src="content_001.xhtml"/>
            </navPoint>
        </navMap>
    </ncx>"#;

    #[test]
    fn clean_ncx_no_findings() {
        assert!(run(CLEAN, "NOID").is_empty());
    }

    #[test]
    fn uid_match_allows_surrounding_whitespace() {
        assert!(run(CLEAN, "  NOID  ").is_empty());
    }

    #[test]
    fn uid_mismatch_errors() {
        let findings = run(CLEAN, "something-else");
        assert!(findings.contains(&NCX_001));
    }

    #[test]
    fn dtb_uid_whitespace_is_ncx004() {
        // dtb:uid content with surrounding whitespace draws NCX-004 (usage).
        // Its trimmed value still matches the package id, so no NCX-001.
        let ncx = CLEAN.replace("content=\"NOID\"", "content=\" NOID \"");
        let findings = run(&ncx, "NOID");
        assert!(findings.contains(&NCX_004));
        assert!(!findings.contains(&NCX_001));
    }

    #[test]
    fn empty_doc_title_and_nav_label() {
        let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
            <head><meta name="dtb:uid" content="NOID"/></head>
            <docTitle><text></text></docTitle>
            <navMap>
                <navPoint id="np-1" playOrder="1">
                    <navLabel><text></text></navLabel>
                    <content src="content_001.xhtml"/>
                </navPoint>
            </navMap>
        </ncx>"#;
        let findings = run(ncx, "NOID");
        assert_eq!(findings, vec![NCX_006, NCX_006]);
    }

    #[test]
    fn comment_only_label_counts_as_empty() {
        let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
            <head><meta name="dtb:uid" content="NOID"/></head>
            <docTitle><text><!--empty--></text></docTitle>
            <navMap>
                <navPoint id="np-1" playOrder="1">
                    <navLabel><text>Loomings</text></navLabel>
                    <content src="content_001.xhtml"/>
                </navPoint>
            </navMap>
        </ncx>"#;
        let findings = run(ncx, "NOID");
        assert_eq!(findings, vec![NCX_006]);
    }

    /// Texts of every finding, for the assertions below that care *what* was
    /// said rather than only how many were said.
    fn texts_for(ncx: &str) -> Vec<String> {
        let mut report = Report::new();
        check(ncx, "toc.ncx", "uid", &mut report);
        report.messages.iter().map(|m| m.text.clone()).collect()
    }

    /// #83 — the NCX grammar (`schemas/ncx.rng`), against which sixteen shapes
    /// were measured one book each on epubcheck 5.3.0. Every one of them was
    /// silent here before: only `navPoint`'s model was checked (#79), and the
    /// format's other ~26 constraints were not checked at all.
    ///
    /// Reported by Doitsu on MobileRead (#193) for the first two rows. The
    /// count is asserted, not just the presence of a finding, because the
    /// grammar and the hand-coded `navPoint` check see some of the same
    /// defects and a presence assertion would pass while double-reporting them
    /// — which is exactly what the first build of this did (three findings
    /// against epubcheck's one).
    ///
    /// The right-hand column is epubcheck's own message for the same book, so
    /// a future divergence is visible here rather than only in a `compare`
    /// run.
    #[test]
    fn ncx_grammar_matches_epubcheck_on_the_shapes_it_reports() {
        let pt = "<pageTarget id=\"p1\" type=\"normal\" value=\"1\">\
                  <navLabel><text>1</text></navLabel><content src=\"a.xhtml\"/></pageTarget>";
        let nav = "<navMap><navPoint id=\"n1\"><navLabel><text>S</text></navLabel>\
                   <content src=\"a.xhtml\"/></navPoint></navMap>";
        // (case, NCX body, findings epubcheck gives, a fragment of its message)
        let cases: Vec<(&str, String, usize, &str)> = vec![
            // element "navMap" incomplete; missing required element "navPoint"
            ("empty navMap", "<navMap></navMap>".into(), 1, "navPoint"),
            // element "pageList" incomplete; missing required element "pageTarget"
            (
                "empty pageList",
                format!("{nav}<pageList></pageList>"),
                1,
                "pageTarget",
            ),
            // element "navList" incomplete; missing required element "navLabel"
            (
                "empty navList",
                format!("{nav}<navList></navList>"),
                1,
                "navLabel",
            ),
            // element "navList" incomplete; missing required element "navTarget"
            (
                "navList without navTarget",
                format!("{nav}<navList><navLabel><text>L</text></navLabel></navList>"),
                1,
                "navTarget",
            ),
            // element "navTarget" incomplete; missing required element "content"
            (
                "navTarget without content",
                format!(
                    "{nav}<navList><navLabel><text>L</text></navLabel>\
                     <navTarget id=\"t1\"><navLabel><text>A</text></navLabel></navTarget></navList>"
                ),
                1,
                "content",
            ),
            // element "pageTarget" incomplete; missing required element "content"
            (
                "pageTarget without content",
                format!(
                    "{nav}<pageList><pageTarget id=\"p1\" type=\"normal\" value=\"1\">\
                     <navLabel><text>1</text></navLabel></pageTarget></pageList>"
                ),
                1,
                "content",
            ),
            // element "navLabel" incomplete; missing required element "text"
            (
                "empty navLabel",
                "<navMap><navPoint id=\"n1\"><navLabel></navLabel>\
                 <content src=\"a.xhtml\"/></navPoint></navMap>"
                    .into(),
                1,
                "text",
            ),
            // element "ncx" incomplete; missing required element "navMap"
            ("no navMap at all", String::new(), 1, "navMap"),
            // element "pageList" not allowed here; expected element "navInfo",
            // "navLabel" or "navPoint" — plus the two incomplete containers.
            // Doitsu's snippet verbatim.
            (
                "pageList nested in navMap",
                "<navMap><pageList></pageList></navMap>".into(),
                3,
                "not allowed here",
            ),
            // element "bogus" not allowed anywhere
            (
                "element the format does not define",
                format!("{nav}<bogus/>"),
                1,
                "not allowed",
            ),
            // element "navPoint" missing required attribute "id"
            (
                "navPoint without id",
                "<navMap><navPoint><navLabel><text>S</text></navLabel>\
                 <content src=\"a.xhtml\"/></navPoint></navMap>"
                    .into(),
                1,
                "required attribute",
            ),
            // element "content" missing required attribute "src"
            (
                "content without src",
                "<navMap><navPoint id=\"n1\"><navLabel><text>S</text></navLabel>\
                 <content/></navPoint></navMap>"
                    .into(),
                1,
                "required attribute",
            ),
            // attribute "bogus" not allowed here
            (
                "attribute the format does not define",
                "<navMap><navPoint id=\"n1\" bogus=\"x\"><navLabel><text>S</text></navLabel>\
                 <content src=\"a.xhtml\"/></navPoint></navMap>"
                    .into(),
                1,
                "not allowed",
            ),
            // A control that must stay silent, or every row above proves
            // nothing: the same constructs, correctly formed.
            (
                "valid navMap, pageList and navList",
                format!(
                    "{nav}<pageList id=\"pl\" class=\"c\">{pt}</pageList>\
                     <navList><navLabel><text>L</text></navLabel>\
                     <navTarget id=\"t1\"><navLabel><text>A</text></navLabel>\
                     <content src=\"a.xhtml\"/></navTarget></navList>"
                ),
                0,
                "",
            ),
        ];
        for (case, body, want, fragment) in cases {
            let texts = texts_for(&ncx_with(&body));
            assert_eq!(
                texts.len(),
                want,
                "{case}: expected {want} finding(s), got {texts:?}"
            );
            if !fragment.is_empty() {
                assert!(
                    texts.iter().any(|t| t.contains(fragment)),
                    "{case}: no finding mentioning {fragment:?} in {texts:?}"
                );
            }
        }
    }

    /// The `navPoint` model is reported once, by `check_nav_point_model`,
    /// even though the grammar sees the same three defects — and the counts
    /// are epubcheck's, measured one book each.
    ///
    /// The suppression asks the report whether a `navPoint` finding was
    /// actually produced, so removing that check would restore the grammar's
    /// own blames rather than silence the defect. This test is what would
    /// notice if the two ever both fired again.
    #[test]
    fn nav_point_defects_are_reported_once_not_twice() {
        let np = |inner: &str| format!("<navMap><navPoint id=\"n1\">{inner}</navPoint></navMap>");
        let label = "<navLabel><text>x</text></navLabel>";
        let content = "<content src=\"a.xhtml\"/>";
        // epubcheck: one error, "content not allowed yet; missing required
        // element navLabel".
        assert_eq!(texts_for(&ncx_with(&np(content))).len(), 1);
        // epubcheck: two errors — the misplaced content, then the trailing
        // label.
        assert_eq!(
            texts_for(&ncx_with(&np(&format!("{content}{label}")))).len(),
            2
        );
        // epubcheck: one error, "navPoint incomplete; missing required
        // element content".
        assert_eq!(texts_for(&ncx_with(&np(label))).len(), 1);
        // A missing `id` is the grammar's alone — the hand-coded check has no
        // opinion on it, so it must survive the suppression.
        assert_eq!(
            texts_for(&ncx_with(
                "<navMap><navPoint><navLabel><text>x</text></navLabel>\
                                 <content src=\"a.xhtml\"/></navPoint></navMap>"
            ))
            .len(),
            1
        );
    }

    /// Three places where we deliberately accept what epubcheck rejects, all
    /// measured against 5.3.0 and all in the looser direction. See the header
    /// of `schemas/ncx.rng` for why. Pinned here because each is a silence,
    /// and a silence is exactly what no other instrument can see.
    #[test]
    fn deliberate_divergences_from_epubchecks_own_ncx_grammar() {
        let pt = "<pageTarget id=\"p1\" type=\"normal\" value=\"1\">\
                  <navLabel><text>1</text></navLabel><content src=\"a.xhtml\"/></pageTarget>";
        let nav = "<navMap><navPoint id=\"n1\"><navLabel><text>S</text></navLabel>\
                   <content src=\"a.xhtml\"/></navPoint></navMap>";
        // epubcheck: `element "pageList" missing required attribute "class"`.
        // It wraps the pair in one <optional>, so it demands them together.
        assert!(
            texts_for(&ncx_with(&format!(
                "{nav}<pageList id=\"pl\">{pt}</pageList>"
            )))
            .is_empty()
        );
        // epubcheck: `... missing required attribute "id"`, the same quirk
        // seen from the other side.
        assert!(
            texts_for(&ncx_with(&format!(
                "{nav}<pageList class=\"c\">{pt}</pageList>"
            )))
            .is_empty()
        );
        // epubcheck allows at most one navLabel in a pageList and fixes the
        // order as navLabel-then-navInfo; the format's own order is the
        // reverse, so either choice invents an error on books following the
        // other.
        assert!(
            texts_for(&ncx_with(&format!(
                "{nav}<pageList><navInfo><text>I</text></navInfo>\
                 <navLabel><text>A</text></navLabel><navLabel xml:lang=\"de\"><text>B</text></navLabel>{pt}</pageList>"
            )))
            .is_empty()
        );
    }
}
