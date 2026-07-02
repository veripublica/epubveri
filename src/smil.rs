//! Media Overlays (SMIL) checks. Deliberately scoped to the **EPUB Media
//! Overlays profile** of SMIL, not general SMIL 3.0 (which has ~40-50
//! elements across a dozen modules — animation, visual layout/regions,
//! non-audio media objects, content-control switching, linking, transition
//! effects — almost none of which EPUB Media Overlays uses). The profile
//! actually used is `smil`/`body`/`seq`/`par`/`text`/`audio` plus
//! `epub:textref`/`epub:type` and the `clipBegin`/`clipEnd` clock-value
//! grammar, so that's what's implemented here. Plain XML, so `roxmltree`
//! (via `ocf::parse_xml`) handles it directly — no new parser needed.

use std::collections::HashMap;

use crate::ids::*;
use crate::opf::{is_external, nfc, resolve};
use crate::report::{Report, Severity};

const CORE_AUDIO_TYPES: [&str; 2] = ["audio/mpeg", "audio/mp4"];
const EPUB_NS: &str = "http://www.idpf.org/2007/ops";

/// Parses and checks one SMIL overlay document. Returns `(text_targets,
/// textref_targets)`: `text_targets` is the `(content-doc path, fragment
/// id)` pairs referenced by `<text src="...#id">` elements, which the
/// caller (`opf::check`) cross-references against the package manifest's
/// `media-overlay` attributes (MED-010/011/012/013); `textref_targets` is
/// the same shape for `<seq>`/`<par>` `epub:textref` attributes, which the
/// caller resolves against the target document's real ids (RSC-012, the
/// same shape as the NCX `<content src>` fragment check). Both need the
/// whole-package view (reading another file's DOM), so neither check can
/// finish here.
pub(crate) fn check(
    smil_xml: &str,
    smil_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    media_types: &HashMap<String, String>,
    report: &mut Report,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mut text_targets = Vec::new();
    let mut textref_targets = Vec::new();
    let Ok(doc) = crate::ocf::parse_xml(smil_xml) else {
        return (text_targets, textref_targets);
    };
    let root = doc.root_element();
    // 9.2.2.2: the head container may only hold a <metadata> element (not
    // a bare <meta>, which must be wrapped in one).
    if let Some(head) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "head")
    {
        for child in head.children().filter(|n| n.is_element()) {
            if child.tag_name().name() == "meta" {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "element \"meta\" not allowed here (must be inside a \"metadata\" element)",
                    smil_path,
                );
            }
        }
    }
    if let Some(body) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    {
        check_container(
            body,
            smil_path,
            base_dir,
            name_index,
            media_types,
            report,
            &mut text_targets,
        );
    }

    // 9.3.2.2: epub:textref on <seq>/<par> - a fragment reference to a
    // sectioning element in the target content document, resolved by the
    // caller the same way NCX <content src> fragments are (RSC-012).
    for n in doc.descendants().filter(|n| {
        n.is_element()
            && matches!(n.tag_name().name(), "seq" | "par")
            && n.attribute((EPUB_NS, "textref")).is_some()
    }) {
        let textref = n.attribute((EPUB_NS, "textref")).unwrap();
        if is_external(textref) {
            continue;
        }
        if let Some((path_part, frag)) = textref.split_once('#') {
            if !frag.is_empty() {
                textref_targets.push((nfc(&resolve(base_dir, path_part)), frag.to_string()));
            }
        }
    }

    // 9.3.3: epub:type values must be in the default vocabulary unless
    // custom-prefixed (any token containing ':' - not validated against
    // the package's own declared epub:prefix mapping, same "prefixed =
    // always allowed" exemption already used for manifest item
    // properties, OPF-027).
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.attribute((EPUB_NS, "type")).is_some())
    {
        let value = n.attribute((EPUB_NS, "type")).unwrap();
        for token in value.split_whitespace() {
            if !token.contains(':') && !is_default_vocab_type(token) {
                report.push_at(
                    OPF_088,
                    Severity::Info,
                    format!("epub:type value '{token}' is not in the default vocabulary"),
                    smil_path,
                );
            }
        }
    }

    (text_targets, textref_targets)
}

/// A generously-inclusive allowlist of real EPUB Structural Semantics
/// vocabulary terms - this is a usage-level (Info) finding, so a false
/// negative (missing a real term) is far safer than a false positive
/// (flagging one), hence biased toward inclusion.
pub(crate) fn is_default_vocab_type(token: &str) -> bool {
    const KNOWN: &[&str] = &[
        "abstract",
        "acknowledgments",
        "afterword",
        "answer",
        "answers",
        "appendix",
        "aside",
        "assessment",
        "assessments",
        "backlink",
        "backmatter",
        "bibliography",
        "biblioentry",
        "bodymatter",
        "bridgehead",
        "chapter",
        "colophon",
        "concludingsentence",
        "conclusion",
        "contributors",
        "copyright-page",
        "cover",
        "credit",
        "credits",
        "dedication",
        "division",
        "endnote",
        "endnotes",
        "epigraph",
        "epilogue",
        "errata",
        "example",
        "footnote",
        "footnotes",
        "foreword",
        "fulltitlepage",
        "glossary",
        "glossdef",
        "glossref",
        "glossterm",
        "halftitlepage",
        "imprimatur",
        "imprint",
        "index",
        "index-editor-note",
        "index-entry",
        "index-entry-list",
        "index-group",
        "index-headnotes",
        "index-legend",
        "index-locator",
        "index-locator-list",
        "index-locator-range",
        "index-term",
        "index-term-categories",
        "index-term-category",
        "index-xref-preferred",
        "index-xref-related",
        "introduction",
        "keyword",
        "landmarks",
        "learning-objective",
        "learning-objectives",
        "learning-outcome",
        "learning-outcomes",
        "learning-resource",
        "learning-resources",
        "learning-standard",
        "learning-standards",
        "list",
        "list-item",
        "loa",
        "loi",
        "lot",
        "lov",
        "marginalia",
        "notice",
        "noteref",
        "ordinal",
        "other-credits",
        "pagebreak",
        "page-list",
        "part",
        "practice",
        "practice-answer",
        "preamble",
        "preface",
        "prologue",
        "pullquote",
        "qna",
        "question",
        "revision-history",
        "region-based",
        "seriespage",
        "subchapter",
        "subtitle",
        "table",
        "table-cell",
        "table-row",
        "tip",
        "title",
        "titlepage",
        "toc",
        "toc-brief",
        "topic-sentence",
        "translator-note",
        "volume",
        "warning",
    ];
    KNOWN.contains(&token)
}

/// Walks `<body>`/`<seq>`/`<par>` nesting. `<par>` may only contain
/// `<text>`/`<audio>`; `<seq>`/`<body>` may only contain `<seq>`/`<par>` —
/// confirmed against the corpus as RSC-005, not a dedicated MED code.
fn check_container(
    node: roxmltree::Node,
    smil_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    media_types: &HashMap<String, String>,
    report: &mut Report,
    text_targets: &mut Vec<(String, String)>,
) {
    let is_par = node.tag_name().name() == "par";
    // A <par> may contain at most one <text> child - confirmed the first
    // is processed normally and every one after it is RSC-005 "not
    // allowed here" instead.
    let mut text_seen = 0;
    for child in node.children().filter(|n| n.is_element()) {
        match (is_par, child.tag_name().name()) {
            (false, "seq") | (false, "par") => {
                check_container(
                    child,
                    smil_path,
                    base_dir,
                    name_index,
                    media_types,
                    report,
                    text_targets,
                );
            }
            (true, "text") => {
                text_seen += 1;
                if text_seen > 1 {
                    report.push_at(
                        RSC_005,
                        Severity::Error,
                        "element \"text\" not allowed here (a <par> may only contain one)",
                        smil_path,
                    );
                } else {
                    check_text(child, smil_path, base_dir, name_index, report, text_targets);
                }
            }
            (true, "audio") => {
                check_audio(child, smil_path, base_dir, name_index, media_types, report);
            }
            (true, "seq") | (true, "par") => {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "a <par> element must not contain a nested <seq>/<par>",
                    smil_path,
                );
            }
            (false, "text") | (false, "audio") => {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    "media clips must be inside a <par> element",
                    smil_path,
                );
            }
            _ => {}
        }
    }
}

fn split_fragment(src: &str) -> (&str, Option<&str>) {
    match src.split_once('#') {
        Some((path, frag)) => (path, Some(frag)),
        None => (src, None),
    }
}

fn check_text(
    node: roxmltree::Node,
    smil_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    report: &mut Report,
    text_targets: &mut Vec<(String, String)>,
) {
    let Some(src) = node.attribute("src") else {
        return;
    };
    if is_external(src) {
        return;
    }
    let (path_part, frag) = split_fragment(src);
    let resolved = resolve(base_dir, path_part);
    let resolved_nfc = nfc(&resolved);
    if !name_index.contains_key(&resolved_nfc) {
        report.push_at(
            RSC_001,
            Severity::Error,
            format!("references a missing resource '{src}'"),
            smil_path,
        );
        return;
    }
    if let Some(f) = frag {
        check_fragment_scheme(path_part, f, smil_path, report);
        text_targets.push((resolved_nfc, f.to_string()));
    }
}

/// A media-overlay `<text>` target's fragment is expected to be a plain id
/// on XHTML targets, or a plain id / the SVG `svgView(...)` view-fragment
/// form on SVG targets. `xpointer(...)`-style scheme-based fragments on
/// XHTML (confirmed via the real corpus fixture
/// `mediaoverlays-textref-fragment-schemebased-warning`) and anything else
/// on SVG (confirmed via `mediaoverlays-textref-svg-fragment-invalid-warning`,
/// e.g. `#box=0,0,50,50`) are warned about, not hard errors.
fn check_fragment_scheme(path_part: &str, frag: &str, smil_path: &str, report: &mut Report) {
    let lower = path_part.to_ascii_lowercase();
    if lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm") {
        if frag.contains('(') {
            report.push_at(
                MED_017,
                Severity::Warning,
                format!("scheme-based fragment '{frag}' should be a plain id"),
                smil_path,
            );
        }
    } else if lower.ends_with(".svg") {
        let is_plain_id = !frag.contains(['(', '=', ',']);
        let is_svg_view = frag.starts_with("svgView(");
        if !is_plain_id && !is_svg_view {
            report.push_at(
                MED_018,
                Severity::Warning,
                format!("invalid SVG fragment identifier '{frag}'"),
                smil_path,
            );
        }
    }
}

fn check_audio(
    node: roxmltree::Node,
    smil_path: &str,
    base_dir: &str,
    name_index: &HashMap<String, String>,
    media_types: &HashMap<String, String>,
    report: &mut Report,
) {
    let Some(src) = node.attribute("src") else {
        return;
    };
    if is_external(src) {
        return;
    }
    let (path_part, frag) = split_fragment(src);
    if frag.is_some() {
        report.push_at(
            MED_014,
            Severity::Error,
            format!("audio 'src' has a URL fragment '{src}' (use clipBegin/clipEnd instead)"),
            smil_path,
        );
    }
    let resolved = resolve(base_dir, path_part);
    let resolved_nfc = nfc(&resolved);
    if !name_index.contains_key(&resolved_nfc) {
        report.push_at(
            RSC_001,
            Severity::Error,
            format!("references a missing resource '{src}'"),
            smil_path,
        );
    } else if let Some(media_type) = media_types.get(&resolved_nfc) {
        if !CORE_AUDIO_TYPES.contains(&media_type.as_str()) {
            report.push_at(
                MED_005,
                Severity::Error,
                format!("audio resource '{src}' is not a Core Media Type ({media_type})"),
                smil_path,
            );
        }
    }

    for attr_name in ["clipBegin", "clipEnd"] {
        if let Some(v) = node.attribute(attr_name) {
            if parse_clock_value(v).is_none() {
                report.push_at(
                    RSC_005,
                    Severity::Error,
                    format!("{attr_name} value '{v}' is not a valid SMIL clock value"),
                    smil_path,
                );
            }
        }
    }
    let begin = node.attribute("clipBegin").and_then(parse_clock_value);
    let end = node.attribute("clipEnd").and_then(parse_clock_value);
    if let (Some(b), Some(e)) = (begin, end) {
        if b > e {
            report.push_at(
                MED_008,
                Severity::Error,
                "clipBegin is after clipEnd",
                smil_path,
            );
        } else if b == e {
            report.push_at(
                MED_009,
                Severity::Error,
                "clipBegin equals clipEnd",
                smil_path,
            );
        }
    }
}

/// Parses a SMIL clock-value (`clipBegin`/`clipEnd`) into seconds, per the
/// three grammar forms below. `None` means the syntax itself doesn't match
/// any of them — the caller doesn't report that here; malformed clock-value
/// *syntax* is RSC-005 territory (confirmed against the real corpus), kept
/// separate from the begin-vs-end *comparison* (MED-008/009), which only
/// applies once both sides did parse.
pub(crate) fn parse_clock_value(s: &str) -> Option<f64> {
    let s = s.trim();
    parse_full_clock(s)
        .or_else(|| parse_partial_clock(s))
        .or_else(|| parse_timecount(s))
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Splits an optional ".Fraction" suffix off a numeric string, returning
/// the integer part and the fractional value to add (0.0 if absent).
fn split_fraction(s: &str) -> Option<(&str, f64)> {
    match s.split_once('.') {
        Some((int_part, frac)) => {
            if !all_digits(frac) {
                return None;
            }
            let frac_val: f64 = format!("0.{frac}").parse().ok()?;
            Some((int_part, frac_val))
        }
        None => Some((s, 0.0)),
    }
}

// Full-clock-value ::= Hours ":" Minutes ":" Seconds ("." Fraction)?
// Minutes/Seconds are exactly 2 digits, 00-59; Hours is unbounded.
fn parse_full_clock(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    let [hours, minutes, sec_and_frac] = parts.as_slice() else {
        return None;
    };
    if !all_digits(hours) || minutes.len() != 2 || !all_digits(minutes) {
        return None;
    }
    let (seconds, frac) = split_fraction(sec_and_frac)?;
    if seconds.len() != 2 || !all_digits(seconds) {
        return None;
    }
    let h: f64 = hours.parse().ok()?;
    let m: f64 = minutes.parse().ok()?;
    let sec: f64 = seconds.parse().ok()?;
    if m > 59.0 || sec > 59.0 {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + sec + frac)
}

// Partial-clock-value ::= Minutes ":" Seconds ("." Fraction)?
// Minutes/Seconds are exactly 2 digits, 00-59 (same constraint as full-clock).
fn parse_partial_clock(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    let [minutes, sec_and_frac] = parts.as_slice() else {
        return None;
    };
    if minutes.len() != 2 || !all_digits(minutes) {
        return None;
    }
    let (seconds, frac) = split_fraction(sec_and_frac)?;
    if seconds.len() != 2 || !all_digits(seconds) {
        return None;
    }
    let m: f64 = minutes.parse().ok()?;
    let sec: f64 = seconds.parse().ok()?;
    if m > 59.0 || sec > 59.0 {
        return None;
    }
    Some(m * 60.0 + sec + frac)
}

// Timecount-value ::= Timecount ("." Fraction)? Metric?
// Metric ::= "h" | "min" | "s" | "ms" ; no metric defaults to seconds.
// Longer suffixes must be checked before shorter ones that are also their
// suffix ("ms" before "s", "min" before nothing else colliding) or e.g.
// "10ms" would be misread as "10m" + trailing "s".
fn parse_timecount(s: &str) -> Option<f64> {
    const METRICS: [(&str, f64); 4] = [("ms", 0.001), ("min", 60.0), ("h", 3600.0), ("s", 1.0)];
    let (num_part, multiplier) = METRICS
        .iter()
        .find_map(|(suffix, mult)| s.strip_suffix(suffix).map(|n| (n, *mult)))
        .unwrap_or((s, 1.0));
    let (int_part, frac) = split_fraction(num_part)?;
    if !all_digits(int_part) {
        return None;
    }
    let n: f64 = int_part.parse().ok()?;
    Some((n + frac) * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(paths: &[&str]) -> HashMap<String, String> {
        paths
            .iter()
            .map(|p| (p.to_string(), p.to_string()))
            .collect()
    }

    fn run(
        smil: &str,
        name_index: &HashMap<String, String>,
        media_types: &HashMap<String, String>,
    ) -> (Vec<&'static str>, Vec<(String, String)>) {
        let mut report = Report::new();
        let (targets, _textref_targets) = check(
            smil,
            "content.smil",
            "OEBPS",
            name_index,
            media_types,
            &mut report,
        );
        (report.messages.iter().map(|m| m.id).collect(), targets)
    }

    #[test]
    fn clean_overlay_no_findings() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body>
                <par id="par1">
                    <text src="chapter1.xhtml#t1"/>
                    <audio src="chapter1.mp3" clipBegin="0s" clipEnd="10s"/>
                </par>
            </body>
        </smil>"#;
        let names = idx(&["OEBPS/chapter1.xhtml", "OEBPS/chapter1.mp3"]);
        let mut types = HashMap::new();
        types.insert("OEBPS/chapter1.mp3".to_string(), "audio/mpeg".to_string());
        let (findings, targets) = run(smil, &names, &types);
        assert!(findings.is_empty());
        assert_eq!(
            targets,
            vec![("OEBPS/chapter1.xhtml".to_string(), "t1".to_string())]
        );
    }

    #[test]
    fn seq_with_direct_media_children() {
        // The real corpus scenario for this exact shape (text + audio both
        // directly under <seq>) expects RSC-005 reported *twice*, once per
        // offending child - not deduplicated per container.
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body>
                <seq id="seq1">
                    <text src="chapter1.xhtml#t1"/>
                    <audio src="chapter1.mp3" clipBegin="0s" clipEnd="10s"/>
                </seq>
            </body>
        </smil>"#;
        let names = idx(&["OEBPS/chapter1.xhtml", "OEBPS/chapter1.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![RSC_005, RSC_005]);
    }

    #[test]
    fn par_with_seq_child() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body>
                <par id="par1">
                    <text src="chapter1.xhtml#t1"/>
                    <audio src="chapter1.mp3" clipBegin="0s" clipEnd="10s"/>
                    <seq id="seq1">
                        <par>
                            <text src="chapter1.xhtml#t2"/>
                            <audio src="chapter1.mp3" clipBegin="10s" clipEnd="15s"/>
                        </par>
                    </seq>
                </par>
            </body>
        </smil>"#;
        let names = idx(&["OEBPS/chapter1.xhtml", "OEBPS/chapter1.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![RSC_005]);
    }

    #[test]
    fn audio_src_with_fragment() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3#frag" clipBegin="0s" clipEnd="1s"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(findings.contains(&MED_014));
    }

    #[test]
    fn non_core_audio_media_type() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.wav"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.wav"]);
        let mut types = HashMap::new();
        types.insert("OEBPS/c.wav".to_string(), "audio/wav".to_string());
        let (findings, _) = run(smil, &names, &types);
        assert!(findings.contains(&MED_005));
    }

    #[test]
    fn clip_begin_after_end() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3" clipBegin="10s" clipEnd="5s"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(findings.contains(&MED_008));
    }

    #[test]
    fn clip_begin_equals_end() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3" clipBegin="5s" clipEnd="5s"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(findings.contains(&MED_009));
    }

    #[test]
    fn missing_text_and_audio_resources() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="missing.xhtml#t"/><audio src="missing.mp3" clipBegin="0s" clipEnd="1s"/></par></body>
        </smil>"#;
        let (findings, _) = run(smil, &HashMap::new(), &HashMap::new());
        assert_eq!(findings, vec![RSC_001, RSC_001]);
    }

    #[test]
    fn xpointer_fragment_on_xhtml_warns() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#xpointer(id('c01'))"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(findings.contains(&MED_017));
    }

    #[test]
    fn invalid_svg_fragment_warns() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.svg#box=0,0,50,50"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.svg", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(findings.contains(&MED_018));
    }

    #[test]
    fn svg_view_fragment_is_valid() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.svg#svgView(viewBox(0,200,1000,1000))"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.svg", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(!findings.contains(&MED_018));
    }

    #[test]
    fn plain_id_fragment_is_valid() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#c01"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(!findings.contains(&MED_017));
        assert!(!findings.contains(&MED_018));
    }

    #[test]
    fn clock_value_parsing() {
        assert_eq!(parse_clock_value("10s"), Some(10.0));
        assert_eq!(parse_clock_value("1min"), Some(60.0));
        assert_eq!(parse_clock_value("500ms"), Some(0.5));
        assert_eq!(parse_clock_value("1h"), Some(3600.0));
        assert_eq!(parse_clock_value("5"), Some(5.0));
        assert_eq!(parse_clock_value("00:10.500"), Some(10.5));
        assert_eq!(parse_clock_value("00:00:10.500"), Some(10.5));
        assert_eq!(parse_clock_value("01:02:03"), Some(3723.0));
    }

    #[test]
    fn clock_value_syntax_errors_from_the_real_corpus() {
        // "0:00:60.000" - seconds out of range (00-59)
        assert_eq!(parse_clock_value("0:00:60.000"), None);
        // "0:200:00.000" - minutes must be exactly 2 digits
        assert_eq!(parse_clock_value("0:200:00.000"), None);
        // "10m" - "m" isn't a valid metric (must be "min")
        assert_eq!(parse_clock_value("10m"), None);
        // "100:00.000" - partial-clock minutes must be exactly 2 digits
        assert_eq!(parse_clock_value("100:00.000"), None);
        // ".5s" - timecount requires at least one digit before the fraction
        assert_eq!(parse_clock_value(".5s"), None);
        // "00:00:10.999ms" - mixes full-clock and timecount syntax
        assert_eq!(parse_clock_value("00:00:10.999ms"), None);
    }

    #[test]
    fn invalid_clock_value_syntax_reports_rsc005() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3" clipBegin="10m" clipEnd="0:00:60.000"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![RSC_005, RSC_005]);
    }

    #[test]
    fn bare_meta_in_head_reports_rsc005() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <head><meta name="foo" content="bar"/></head>
            <body><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![RSC_005]);
    }

    #[test]
    fn par_with_two_text_children_reports_rsc005_once() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" version="3.0">
            <body><par id="p"><text src="c.xhtml#t1"/><text src="c.xhtml#t2"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, targets) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![RSC_005]);
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn textref_fragment_is_collected() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" xmlns:epub="http://www.idpf.org/2007/ops" version="3.0">
            <body><seq epub:textref="c.xhtml#sec1"><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3"/></par></seq></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let mut report = Report::new();
        let (_targets, textref_targets) = check(
            smil,
            "content.smil",
            "OEBPS",
            &names,
            &HashMap::new(),
            &mut report,
        );
        assert_eq!(
            textref_targets,
            vec![("OEBPS/c.xhtml".to_string(), "sec1".to_string())]
        );
    }

    #[test]
    fn unknown_epubtype_reports_usage() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" xmlns:epub="http://www.idpf.org/2007/ops" version="3.0">
            <body epub:type="chapter unknown"><par id="p"><text src="c.xhtml#t"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert_eq!(findings, vec![OPF_088]);
    }

    #[test]
    fn custom_prefixed_epubtype_is_allowed() {
        let smil = r#"<smil xmlns="http://www.w3.org/ns/SMIL" xmlns:epub="http://www.idpf.org/2007/ops" version="3.0">
            <body epub:type="aside my:sidebar"><par id="p" epub:type="my:title"><text src="c.xhtml#t"/><audio src="c.mp3"/></par></body>
        </smil>"#;
        let names = idx(&["OEBPS/c.xhtml", "OEBPS/c.mp3"]);
        let (findings, _) = run(smil, &names, &HashMap::new());
        assert!(!findings.contains(&OPF_088));
    }
}
