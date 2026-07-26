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
                format!("NCX is not well-formed XML: {e}"),
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
        if content.trim() != package_uid.trim() {
            report.push_at_pos(
                NCX_001,
                Severity::Error,
                format!(
                    "dtb:uid '{}' does not match the package's identifier '{}'",
                    content.trim(),
                    package_uid.trim()
                ),
                ncx_path,
                Position::of(meta),
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
    check_play_order_sequence(&d, ncx_path, report);
    check_page_target_uniqueness(&d, ncx_path, report);
    check_multi_lang_siblings(&d, ncx_path, report);
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

fn is_valid_ncname(s: &str) -> bool {
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
        report.push_at_pos(
            NCX_006,
            Severity::Usage,
            "empty text label",
            ncx_path,
            Position::of(text_el),
        );
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
}
