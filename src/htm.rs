//! XML declaration / DOCTYPE / entity / encoding checks, plus a handful of
//! attribute- and element-level content-document checks. `roxmltree`
//! exposes no structured API for the XML declaration's version, DOCTYPE
//! entities, or a document's original encoding (confirmed by reading its
//! public API) — those need small hand-written raw byte/text scanners
//! (`check_raw`), same "no new dependency" style as `smil.rs`'s
//! clock-value parser and `layout.rs`'s viewport grammar, not a full
//! DTD/XML-declaration parser. The rest (`check_dom`) works off the
//! already-parsed document.

use crate::ids::*;
use crate::report::{Position, Report, Severity};

/// The byte offset of `needle` within `haystack`, assuming `needle` is
/// literally a subslice of `haystack` (as produced by `&haystack[a..b]` or
/// `.trim_start()`/`.find()`-derived slicing, never a reallocated copy) -
/// lets the raw byte/text scans below (which work with `&str` slices, not
/// `roxmltree` nodes) still report a precise `Position` via
/// `Position::of_offset`.
fn offset_in(haystack: &str, needle: &str) -> usize {
    needle.as_ptr() as usize - haystack.as_ptr() as usize
}

/// Raw byte/text scans on a content document — independent of whether it
/// parses as well-formed XML, so these still fire even when e.g. a UTF-16
/// garbling (before the `css::decode_bytes` fix) would otherwise have
/// broken the parse.
///
/// EPUB3-only: confirmed via the real corpus that all of these checks
/// (`content-document-xhtml.feature`, under `epub3/`) don't apply to
/// EPUB2's XHTML content model, which is its own, more lenient, XHTML
/// 1.1-DTD-based spec section — an EPUB2 content document legitimately
/// declares `<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" ...>`,
/// which would otherwise be misflagged as HTM-004's "obsolete" doctype (a
/// real false positive found via the corpus, not by inspection: dozens of
/// EPUB2 fixtures across the corpus reuse exactly this doctype as their
/// standard template).
pub(crate) fn check_raw(bytes: &[u8], text: &str, path: &str, is_epub3: bool, report: &mut Report) {
    // Entity well-formedness (RSC-016) is a basic XML concern, not an
    // EPUB-3-specific one - a real EPUB 2 fixture (an unknown named
    // entity reference) expects it too. EPUB 2 is passed the version flag
    // so its externally-declared XHTML named entities (`&nbsp;` etc.) are
    // not misflagged as undeclared.
    check_entities(text, path, is_epub3, report);

    if !is_epub3 {
        check_doctype_epub2(text, path, report);
        return;
    }
    if crate::css::has_utf16_bom(bytes) {
        report.push_at_pos(
            HTM_058,
            Severity::Error,
            "content document is not UTF-8 encoded",
            path,
            Position::of_offset(text, 0),
        );
    }

    if let Some(decl_end) = text
        .trim_start()
        .strip_prefix("<?xml")
        .and_then(|rest| rest.find("?>"))
    {
        let decl = &text.trim_start()[..decl_end + "<?xml".len() + "?>".len()];
        if decl.contains("version=\"1.1\"") || decl.contains("version='1.1'") {
            report.push_at_pos(
                HTM_001,
                Severity::Error,
                "XML declaration must not use version 1.1",
                path,
                Position::of_offset(text, offset_in(text, decl)),
            );
        }
    }

    check_doctype(text, path, report);
}

/// The standard HTML named character entities declared by the XHTML 1.0/1.1
/// (and OEB 1.2) DTDs - the union of the `HTMLlat1`, `HTMLsymbol` and
/// `HTMLspecial` entity sets. An EPUB 2 content document referencing one of
/// those DTDs (see `EPUB2_XHTML_PUBLIC_IDS`) may use any of these without an
/// internal declaration. The five XML predefined entities are intentionally
/// omitted (handled separately). Order is irrelevant; membership is by
/// linear scan over ~250 short strings, once per named reference.
///
/// Each entry carries its Unicode code point as well as its name, because
/// the table has two jobs: deciding whether a reference is legal
/// (`check_entities`) and re-declaring it so the document actually parses
/// (`declare_dtd_entities`).
const XHTML_NAMED_ENTITIES: &[(&str, u32)] = &[
    // HTMLlat1 (Latin-1 supplement)
    ("nbsp", 0x00A0),
    ("iexcl", 0x00A1),
    ("cent", 0x00A2),
    ("pound", 0x00A3),
    ("curren", 0x00A4),
    ("yen", 0x00A5),
    ("brvbar", 0x00A6),
    ("sect", 0x00A7),
    ("uml", 0x00A8),
    ("copy", 0x00A9),
    ("ordf", 0x00AA),
    ("laquo", 0x00AB),
    ("not", 0x00AC),
    ("shy", 0x00AD),
    ("reg", 0x00AE),
    ("macr", 0x00AF),
    ("deg", 0x00B0),
    ("plusmn", 0x00B1),
    ("sup2", 0x00B2),
    ("sup3", 0x00B3),
    ("acute", 0x00B4),
    ("micro", 0x00B5),
    ("para", 0x00B6),
    ("middot", 0x00B7),
    ("cedil", 0x00B8),
    ("sup1", 0x00B9),
    ("ordm", 0x00BA),
    ("raquo", 0x00BB),
    ("frac14", 0x00BC),
    ("frac12", 0x00BD),
    ("frac34", 0x00BE),
    ("iquest", 0x00BF),
    ("Agrave", 0x00C0),
    ("Aacute", 0x00C1),
    ("Acirc", 0x00C2),
    ("Atilde", 0x00C3),
    ("Auml", 0x00C4),
    ("Aring", 0x00C5),
    ("AElig", 0x00C6),
    ("Ccedil", 0x00C7),
    ("Egrave", 0x00C8),
    ("Eacute", 0x00C9),
    ("Ecirc", 0x00CA),
    ("Euml", 0x00CB),
    ("Igrave", 0x00CC),
    ("Iacute", 0x00CD),
    ("Icirc", 0x00CE),
    ("Iuml", 0x00CF),
    ("ETH", 0x00D0),
    ("Ntilde", 0x00D1),
    ("Ograve", 0x00D2),
    ("Oacute", 0x00D3),
    ("Ocirc", 0x00D4),
    ("Otilde", 0x00D5),
    ("Ouml", 0x00D6),
    ("times", 0x00D7),
    ("Oslash", 0x00D8),
    ("Ugrave", 0x00D9),
    ("Uacute", 0x00DA),
    ("Ucirc", 0x00DB),
    ("Uuml", 0x00DC),
    ("Yacute", 0x00DD),
    ("THORN", 0x00DE),
    ("szlig", 0x00DF),
    ("agrave", 0x00E0),
    ("aacute", 0x00E1),
    ("acirc", 0x00E2),
    ("atilde", 0x00E3),
    ("auml", 0x00E4),
    ("aring", 0x00E5),
    ("aelig", 0x00E6),
    ("ccedil", 0x00E7),
    ("egrave", 0x00E8),
    ("eacute", 0x00E9),
    ("ecirc", 0x00EA),
    ("euml", 0x00EB),
    ("igrave", 0x00EC),
    ("iacute", 0x00ED),
    ("icirc", 0x00EE),
    ("iuml", 0x00EF),
    ("eth", 0x00F0),
    ("ntilde", 0x00F1),
    ("ograve", 0x00F2),
    ("oacute", 0x00F3),
    ("ocirc", 0x00F4),
    ("otilde", 0x00F5),
    ("ouml", 0x00F6),
    ("divide", 0x00F7),
    ("oslash", 0x00F8),
    ("ugrave", 0x00F9),
    ("uacute", 0x00FA),
    ("ucirc", 0x00FB),
    ("uuml", 0x00FC),
    ("yacute", 0x00FD),
    ("thorn", 0x00FE),
    ("yuml", 0x00FF),
    // HTMLsymbol (Greek letters + mathematical/technical symbols)
    ("fnof", 0x0192),
    ("Alpha", 0x0391),
    ("Beta", 0x0392),
    ("Gamma", 0x0393),
    ("Delta", 0x0394),
    ("Epsilon", 0x0395),
    ("Zeta", 0x0396),
    ("Eta", 0x0397),
    ("Theta", 0x0398),
    ("Iota", 0x0399),
    ("Kappa", 0x039A),
    ("Lambda", 0x039B),
    ("Mu", 0x039C),
    ("Nu", 0x039D),
    ("Xi", 0x039E),
    ("Omicron", 0x039F),
    ("Pi", 0x03A0),
    ("Rho", 0x03A1),
    ("Sigma", 0x03A3),
    ("Tau", 0x03A4),
    ("Upsilon", 0x03A5),
    ("Phi", 0x03A6),
    ("Chi", 0x03A7),
    ("Psi", 0x03A8),
    ("Omega", 0x03A9),
    ("alpha", 0x03B1),
    ("beta", 0x03B2),
    ("gamma", 0x03B3),
    ("delta", 0x03B4),
    ("epsilon", 0x03B5),
    ("zeta", 0x03B6),
    ("eta", 0x03B7),
    ("theta", 0x03B8),
    ("iota", 0x03B9),
    ("kappa", 0x03BA),
    ("lambda", 0x03BB),
    ("mu", 0x03BC),
    ("nu", 0x03BD),
    ("xi", 0x03BE),
    ("omicron", 0x03BF),
    ("pi", 0x03C0),
    ("rho", 0x03C1),
    ("sigmaf", 0x03C2),
    ("sigma", 0x03C3),
    ("tau", 0x03C4),
    ("upsilon", 0x03C5),
    ("phi", 0x03C6),
    ("chi", 0x03C7),
    ("psi", 0x03C8),
    ("omega", 0x03C9),
    ("thetasym", 0x03D1),
    ("upsih", 0x03D2),
    ("piv", 0x03D6),
    ("bull", 0x2022),
    ("hellip", 0x2026),
    ("prime", 0x2032),
    ("Prime", 0x2033),
    ("oline", 0x203E),
    ("frasl", 0x2044),
    ("weierp", 0x2118),
    ("image", 0x2111),
    ("real", 0x211C),
    ("trade", 0x2122),
    ("alefsym", 0x2135),
    ("larr", 0x2190),
    ("uarr", 0x2191),
    ("rarr", 0x2192),
    ("darr", 0x2193),
    ("harr", 0x2194),
    ("crarr", 0x21B5),
    ("lArr", 0x21D0),
    ("uArr", 0x21D1),
    ("rArr", 0x21D2),
    ("dArr", 0x21D3),
    ("hArr", 0x21D4),
    ("forall", 0x2200),
    ("part", 0x2202),
    ("exist", 0x2203),
    ("empty", 0x2205),
    ("nabla", 0x2207),
    ("isin", 0x2208),
    ("notin", 0x2209),
    ("ni", 0x220B),
    ("prod", 0x220F),
    ("sum", 0x2211),
    ("minus", 0x2212),
    ("lowast", 0x2217),
    ("radic", 0x221A),
    ("prop", 0x221D),
    ("infin", 0x221E),
    ("ang", 0x2220),
    ("and", 0x2227),
    ("or", 0x2228),
    ("cap", 0x2229),
    ("cup", 0x222A),
    ("int", 0x222B),
    ("there4", 0x2234),
    ("sim", 0x223C),
    ("cong", 0x2245),
    ("asymp", 0x2248),
    ("ne", 0x2260),
    ("equiv", 0x2261),
    ("le", 0x2264),
    ("ge", 0x2265),
    ("sub", 0x2282),
    ("sup", 0x2283),
    ("nsub", 0x2284),
    ("sube", 0x2286),
    ("supe", 0x2287),
    ("oplus", 0x2295),
    ("otimes", 0x2297),
    ("perp", 0x22A5),
    ("sdot", 0x22C5),
    ("lceil", 0x2308),
    ("rceil", 0x2309),
    ("lfloor", 0x230A),
    ("rfloor", 0x230B),
    ("lang", 0x2329),
    ("rang", 0x232A),
    ("loz", 0x25CA),
    ("spades", 0x2660),
    ("clubs", 0x2663),
    ("hearts", 0x2665),
    ("diams", 0x2666),
    // HTMLspecial (excluding the five XML predefined entities)
    ("OElig", 0x0152),
    ("oelig", 0x0153),
    ("Scaron", 0x0160),
    ("scaron", 0x0161),
    ("Yuml", 0x0178),
    ("circ", 0x02C6),
    ("tilde", 0x02DC),
    ("ensp", 0x2002),
    ("emsp", 0x2003),
    ("thinsp", 0x2009),
    ("zwnj", 0x200C),
    ("zwj", 0x200D),
    ("lrm", 0x200E),
    ("rlm", 0x200F),
    ("ndash", 0x2013),
    ("mdash", 0x2014),
    ("lsquo", 0x2018),
    ("rsquo", 0x2019),
    ("sbquo", 0x201A),
    ("ldquo", 0x201C),
    ("rdquo", 0x201D),
    ("bdquo", 0x201E),
    ("dagger", 0x2020),
    ("Dagger", 0x2021),
    ("permil", 0x2030),
    ("lsaquo", 0x2039),
    ("rsaquo", 0x203A),
    ("euro", 0x20AC),
];

/// The five entities XML predefines, legal everywhere without a
/// declaration.
const PREDEFINED_ENTITIES: &[&str] = &["amp", "lt", "gt", "apos", "quot"];

/// The Unicode code point of a standard HTML named entity, or `None` if the
/// name isn't one.
fn xhtml_entity_codepoint(name: &str) -> Option<u32> {
    XHTML_NAMED_ENTITIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, cp)| *cp)
}

/// One named entity reference found in a document's raw text: its byte
/// `offset` (at the `&`), its `name`, and whether it was `terminated` by
/// the required `;`.
struct EntityRef<'a> {
    offset: usize,
    name: &'a str,
    terminated: bool,
}

/// Every named entity reference in `text`, in order.
///
/// `text` must already be masked (see `mask_comments_and_cdata`). Numeric
/// character references (`&#39;`/`&#x27;`) are always well-formed and out
/// of scope — only named references (`&foo;`) are yielded.
///
/// Both the entity *check* and the entity *declaration* injection walk this
/// one scanner, so they cannot drift apart on what counts as a reference —
/// injecting a declaration for something the check would still reject (or
/// vice versa) is exactly how the two would contradict each other.
fn named_entity_refs(text: &str) -> impl Iterator<Item = EntityRef<'_>> {
    let bytes = text.as_bytes();
    let mut i = 0;
    core::iter::from_fn(move || {
        loop {
            let amp = i + text[i..].find('&')?;
            let after = &text[amp + 1..];
            if after.starts_with('#') {
                i = amp + 1;
                continue;
            }
            let name_len = after
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(after.len());
            if name_len == 0 {
                i = amp + 1;
                continue;
            }
            let name = &after[..name_len];
            let terminated = bytes.get(amp + 1 + name_len) == Some(&b';');
            i = amp + 1 + name_len;
            return Some(EntityRef {
                offset: amp,
                name,
                terminated,
            });
        }
    })
}

/// A minimal, well-formedness-only entity-reference scanner. Runs on the
/// raw text regardless of whether the document parses as XML at all —
/// `roxmltree` simply fails to parse a document with a malformed or
/// undeclared entity reference (confirmed: neither of the two real corpus
/// fixtures for this parse successfully today), so this is the only place
/// these two conditions can be caught. Numeric character references
/// (`&#39;`/`&#x27;`) are always well-formed and out of scope here — only
/// named references (`&foo;`) are checked.
fn check_entities(orig_text: &str, path: &str, is_epub3: bool, report: &mut Report) {
    let declared = declared_entity_names(orig_text);
    // An EPUB 2 XHTML content document references an external DTD (XHTML 1.1
    // or OEB 1.2) through its DOCTYPE, and that DTD declares the full set of
    // HTML named character entities (`&nbsp;`, `&eacute;`, `&copy;`, ...).
    // `roxmltree` does not resolve external DTDs, so without this every one
    // of those legitimate references would be reported as an undeclared
    // RSC-016 - a false positive epubcheck (which bundles the DTDs) never
    // emits, and painful in practice since `&nbsp;` is ubiquitous. Accept
    // the standard set only when the document actually pulls in one of those
    // DTDs via a recognized DOCTYPE. EPUB 3 keeps the strict rule: only the
    // five predefined entities plus any declared in the internal subset are
    // legal there (named references must be written as numeric ones).
    let allow_xhtml_named = !is_epub3 && has_epub2_xhtml_doctype(orig_text);
    // `&foo;` inside a comment or CDATA section is literal text, not a
    // real entity reference (confirmed via a real corpus fixture titled
    // exactly this) - mask any '&' found there so the scan below skips it,
    // without disturbing any other byte offset in the text.
    let masked = mask_comments_and_cdata(orig_text);
    for EntityRef {
        offset: amp,
        name,
        terminated,
    } in named_entity_refs(&masked)
    {
        if !terminated {
            report.push_full(
                RSC_016,
                Severity::Fatal,
                format!("entity reference '&{name}' must end with the ';' delimiter"),
                path,
                Position::of_offset(orig_text, amp),
                "htm.entity.missing_semicolon",
                vec![name.to_string()],
            );
        } else if !PREDEFINED_ENTITIES.contains(&name)
            && !declared.iter().any(|d| d == name)
            && !(allow_xhtml_named && xhtml_entity_codepoint(name).is_some())
        {
            report.push_full(
                RSC_016,
                Severity::Fatal,
                format!("entity '{name}' was referenced, but not declared"),
                path,
                Position::of_offset(orig_text, amp),
                "htm.entity.undeclared",
                vec![name.to_string()],
            );
        }
    }
}

/// Give the parser the entity declarations the document's DOCTYPE promises,
/// by declaring them inline.
///
/// An EPUB 2 XHTML content document pulls in an external DTD that declares
/// the standard HTML named entities, and `roxmltree` never fetches an
/// external DTD. So `&nbsp;` — the single most ordinary thing in a
/// real-world EPUB 2 — fails the parse as an unknown entity, and *every*
/// DOM-based check on that document is silently skipped (issue #23: 690 of
/// 7207 content documents across a real 171-book shelf, every one of them
/// valid). Worse, the checks that did run treated the unreadable document
/// as *empty*, inventing broken-fragment errors against ids that are
/// plainly there.
///
/// Nothing needs to be fetched to fix this: the entity set is fixed and
/// known (`XHTML_NAMED_ENTITIES`), so the referenced ones are re-declared
/// in an internal subset. XML gives the internal subset precedence over the
/// external one, so a document that declares its own `&nbsp;` keeps its own
/// definition — `declared_entity_names` skips those here anyway.
///
/// **Positions.** The declarations go in immediately before the DOCTYPE's
/// own closing `>`, all on one line and adding no newline, so every *line*
/// number in the returned text still matches the original. *Columns* cannot
/// be preserved by construction — inserting text necessarily pushes
/// anything after it on the same line to the right — so the returned
/// [`DtdShift`] says exactly what to subtract, and the content-document walk
/// applies it to every finding before the report is handed out
/// (`correct_dtd_shift`). Callers that only read the DOM (an id map, say)
/// and never report a position can ignore it.
///
/// Growing text is unavoidable here: fitting the declarations inside the
/// DOCTYPE's own footprint would leave room for about three entities, and a
/// French EPUB 2 routinely uses more. Skipping the injection for the awkward
/// documents is worse still — they would go back to not parsing, which
/// silently skips every check on them, and that is the very defect this
/// function exists to fix.
///
/// Returns `text` untouched (and no shift) unless it is an EPUB 2 document
/// with a recognized XHTML/OEB DOCTYPE (see `EPUB2_XHTML_PUBLIC_IDS`) that
/// really does reference a standard named entity it hasn't declared itself.
/// In particular EPUB 3 is left strictly alone: named references other than
/// the predefined five are a genuine error there, and making them parse
/// would be papering over one.
pub(crate) fn declare_dtd_entities(text: String, is_epub3: bool) -> (String, Option<DtdShift>) {
    if is_epub3 || !has_epub2_xhtml_doctype(&text) {
        return (text, None);
    }
    let declared = declared_entity_names(&text);
    let masked = mask_comments_and_cdata(&text);
    let mut needed: Vec<&str> = Vec::new();
    for r in named_entity_refs(&masked) {
        if r.terminated
            && !PREDEFINED_ENTITIES.contains(&r.name)
            && !declared.iter().any(|d| d == r.name)
            && !needed.contains(&r.name)
            && xhtml_entity_codepoint(r.name).is_some()
        {
            needed.push(r.name);
        }
    }
    if needed.is_empty() {
        return (text, None);
    }
    let mut decls = String::new();
    for name in &needed {
        let cp = xhtml_entity_codepoint(name).expect("filtered to known entities above");
        decls.push_str(&format!("<!ENTITY {name} \"&#{cp};\">"));
    }
    let Some((doctype, subset)) = doctype_span(&text) else {
        return (text, None);
    };
    let dt_start = offset_in(&text, doctype);
    let (at, decls) = match subset.map(|r| r.end) {
        // An internal subset is already there - add ours to it, rather
        // than opening a second (illegal) one. The position is the
        // scanner's answer, not a `rfind(']')`: a public identifier is
        // free to contain a `]`, and inserting at one would land inside
        // the literal.
        Some(close) => (dt_start + close, decls),
        None => (dt_start + doctype.len() - 1, format!("[{decls}]")),
    };
    // Where the insertion lands in the *original* text - that is the line
    // whose columns move, and everything at a greater column on it moves by
    // exactly the inserted width.
    let anchor = Position::of_offset(&text, at);
    let shift = DtdShift {
        line: anchor.line,
        after_column: anchor.column,
        chars: decls.chars().count() as u32,
    };
    let mut out = String::with_capacity(text.len() + decls.len());
    out.push_str(&text[..at]);
    out.push_str(&decls);
    out.push_str(&text[at..]);
    (out, Some(shift))
}

/// How far `declare_dtd_entities` pushed one line's columns to the right.
///
/// Only a single line is ever affected (the one holding the DOCTYPE's
/// closing `>`), and only its columns: the injection adds no newline, so
/// line numbers are already exact everywhere.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DtdShift {
    /// The 1-based line the declarations were inserted into.
    line: u32,
    /// Columns strictly greater than this, on that line, moved right.
    after_column: u32,
    /// How far they moved, in chars (positions count chars, not bytes).
    chars: u32,
}

/// Undo a [`DtdShift`] across the findings a content document produced, so
/// every reported position describes the file the author actually has rather
/// than the augmented text epubveri parsed.
///
/// This matters beyond tidiness: downstream repair tools (epubsana) locate
/// nodes by the position epubveri reports and edit the file in place. A
/// column that is right for our parser and wrong for the file on disk is
/// worse than no column at all.
pub(crate) fn correct_dtd_shift(messages: &mut [crate::report::Message], path: &str, s: DtdShift) {
    for m in messages {
        if m.location.as_deref() == Some(path)
            && let Some(p) = m.position.as_mut()
            && p.line == s.line
            && p.column > s.after_column
        {
            p.column -= s.chars;
        }
    }
}

/// Blanks out (with spaces, preserving every other byte offset) any '&'
/// found inside `<!-- -->` comments or `<![CDATA[ ]]>` sections, so
/// `check_entities`'s scan never mistakes literal comment/CDATA text for a
/// real entity reference.
fn mask_comments_and_cdata(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>")] {
        let mut i = 0;
        while let Some(rel) = text[i..].find(open) {
            let start = i + rel + open.len();
            let Some(end_rel) = text[start..].find(close) else {
                break;
            };
            let end = start + end_rel;
            for b in &mut out[start..end] {
                if *b == b'&' {
                    *b = b' ';
                }
            }
            i = end + close.len();
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Entity names declared in the DOCTYPE's internal subset
/// (`<!ENTITY name "...">`), so a legitimately custom-declared entity
/// reference isn't misflagged as unknown.
fn declared_entity_names(text: &str) -> Vec<String> {
    // Bounded by the DOCTYPE, via the same scanner `declare_dtd_entities`
    // uses - see `doctype_span`. Searching the document for a `[` instead
    // was the #25 bug, and it is not harmless here either: a document with
    // no internal subset but a `[1]` in its body would have that body text
    // scanned as if it were the subset, and a subset holding a quoted `]`
    // (`<!ENTITY x "]">`) would be cut short, so a genuinely declared entity
    // would be missed and reported as an undeclared fatal.
    let Some((doctype, Some(range))) = doctype_span(text) else {
        return Vec::new();
    };
    let subset = &doctype[range];
    let mut names = Vec::new();
    let mut i = 0;
    while let Some(rel) = subset[i..].find("<!ENTITY") {
        let rest = subset[i + rel + "<!ENTITY".len()..].trim_start();
        let name_len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        if name_len > 0 {
            names.push(rest[..name_len].to_string());
        }
        i += rel + "<!ENTITY".len();
    }
    names
}

/// Finds the full `<!DOCTYPE ...>` declaration, correctly skipping past
/// any `>` inside an internal subset (`[...]`) to find the declaration's
/// *own* closing `>`.
/// The `<!DOCTYPE ...>` declaration, plus the extent of its internal subset
/// (the range *between* the brackets, relative to the declaration's start)
/// if it has one.
///
/// Scanned character by character rather than searched for, because every
/// shortcut here has a counterexample in a real book:
///
/// - The declaration's own `>` is not the first `>` after it: an internal
///   subset is full of them (`<!ENTITY nbsp "&#160;">`). So `>` only ends
///   the declaration at subset depth 0.
/// - A `[` is only the subset's opener if it comes *before* the declaration
///   has ended. Searching the rest of the document for one finds the `[1]`
///   footnote marker three chapters down and swallows everything in between
///   (issue #25: that over-capture made `declare_dtd_entities` inject entity
///   declarations into the middle of the body, breaking 11 real books).
/// - Neither bracket counts inside a quoted literal — a public identifier
///   may contain anything.
fn doctype_span(text: &str) -> Option<(&str, Option<std::ops::Range<usize>>)> {
    let start = text.find("<!DOCTYPE")?;
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0usize;
    let mut subset_open = None;
    let mut subset: Option<std::ops::Range<usize>> = None;
    let mut i = start + "<!DOCTYPE".len();
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'[' => {
                    if depth == 0 {
                        subset_open = Some(i - start + 1);
                    }
                    depth += 1;
                }
                b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0
                        && let Some(open) = subset_open
                    {
                        subset = Some(open..i - start);
                    }
                }
                // The declaration's own '>' - the one at depth 0.
                b'>' if depth == 0 => return Some((&text[start..=i], subset)),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Finds the full `<!DOCTYPE ...>` declaration - see [`doctype_span`].
fn extract_doctype(text: &str) -> Option<&str> {
    doctype_span(text).map(|(d, _)| d)
}

/// EPUB 2's own DOCTYPE rule for XHTML content documents - the *opposite*
/// polarity from EPUB 3's `check_doctype` (any PUBLIC id is obsolete
/// there), confirmed via three real fixtures: EPUB 2's content model is
/// XHTML-1.1-DTD-based, so the DOCTYPE must carry a recognized XHTML/OEB
/// PUBLIC identifier - a missing one (a bare HTML5-style `<!DOCTYPE
/// html>`) or an unrecognized/malformed one (a typo'd "1.1/EN" missing a
/// slash, or a nonsense "FOO") is HTM-004.
/// The PUBLIC identifiers whose external DTDs (XHTML 1.1, OEB 1.2 Document)
/// an EPUB 2 XHTML content document legitimately references. Both DTDs
/// declare the full set of standard HTML named character entities, so a
/// document carrying one of these DOCTYPEs may use `&nbsp;` and friends
/// without an internal `<!ENTITY>` declaration (see `check_entities`).
const EPUB2_XHTML_PUBLIC_IDS: [&str; 2] = [
    "-//W3C//DTD XHTML 1.1//EN",
    "+//ISBN 0-9673008-1-9//DTD OEB 1.2 Document//EN",
];

/// True when the document's DOCTYPE carries a recognized EPUB 2 XHTML/OEB
/// PUBLIC identifier - i.e. it pulls in an external DTD that declares the
/// standard HTML named entities.
fn has_epub2_xhtml_doctype(text: &str) -> bool {
    extract_doctype(text).is_some_and(|dt| EPUB2_XHTML_PUBLIC_IDS.iter().any(|id| dt.contains(id)))
}

fn check_doctype_epub2(text: &str, path: &str, report: &mut Report) {
    let Some(doctype) = extract_doctype(text) else {
        return;
    };
    if !EPUB2_XHTML_PUBLIC_IDS.iter().any(|id| doctype.contains(id)) {
        report.push_full(
            HTM_004,
            Severity::Error,
            "DOCTYPE does not have a recognized XHTML PUBLIC identifier",
            path,
            Position::of_offset(text, offset_in(text, doctype)),
            "htm.doctype.epub2_unrecognized_public_id",
            Vec::new(),
        );
    }
}

fn check_doctype(text: &str, path: &str, report: &mut Report) {
    let Some(doctype) = extract_doctype(text) else {
        return;
    };
    if doctype.contains(" PUBLIC ") {
        report.push_full(
            HTM_004,
            Severity::Error,
            "DOCTYPE has an obsolete PUBLIC identifier",
            path,
            Position::of_offset(text, offset_in(text, doctype)),
            "htm.doctype.epub3_obsolete_public_id",
            Vec::new(),
        );
    }
    // The subset comes from the scanner, not from `find('[')`/`rfind(']')`
    // over the declaration: a public identifier may contain either bracket,
    // and locating the subset by searching for them is the mistake #25 was
    // made of.
    if let Some((doctype, Some(range))) = doctype_span(text) {
        let subset = &doctype[range];
        for (i, _) in subset.match_indices("<!ENTITY") {
            let rest = &subset[i..];
            let Some(end) = rest.find('>') else { continue };
            let decl = &rest[..end];
            if decl.contains("SYSTEM") || decl.contains("PUBLIC") {
                report.push_at_pos(
                    HTM_003,
                    Severity::Error,
                    "entity is declared external (SYSTEM/PUBLIC)",
                    path,
                    Position::of_offset(text, offset_in(text, decl)),
                );
            }
        }
    }
}

/// A DOCTYPE in the OPF only makes sense if its declared root name matches
/// the OPF's own root element, `package`. Confirmed via the real corpus:
/// the legacy OEB 1.2 `<!DOCTYPE package PUBLIC "...OEB 1.2 Package//EN" ...>`
/// is explicitly *valid* (root name "package" matches), while
/// `<!DOCTYPE html PUBLIC "...OEB 1.2 Document//EN" ...>` is invalid (root
/// name "html" doesn't match) — the PUBLIC identifier itself isn't what's
/// being judged, just whether the declared root is sane for this document.
pub(crate) fn check_opf_doctype(text: &str, opf_path: &str, report: &mut Report) {
    let Some(start) = text.find("<!DOCTYPE") else {
        return;
    };
    let after = text[start + "<!DOCTYPE".len()..].trim_start();
    let root_name = after
        .split(|c: char| c.is_whitespace() || c == '>' || c == '[')
        .next()
        .unwrap_or("");
    if root_name != "package" {
        report.push_at_pos(
            HTM_009,
            Severity::Error,
            format!("OPF document's DOCTYPE root '{root_name}' does not match <package>"),
            opf_path,
            Position::of_offset(text, offset_in(text, root_name)),
        );
    }
}

const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SSML_NS: &str = "http://www.w3.org/2001/10/synthesis";

/// Legitimate w3.org/idpf.org namespaces content documents actually use
/// (XHTML itself, EPUB's own `epub:` ops namespace, XML/XLink, SVG,
/// MathML) — HTM-054 is about *custom* attributes impersonating a
/// w3.org/idpf.org affiliation, not these standard, expected ones.
const KNOWN_NAMESPACES: [&str; 7] = [
    XHTML_NS,
    "http://www.idpf.org/2007/ops",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/1999/xlink",
    SSML_NS,
    "http://www.w3.org/2000/svg",
    "http://www.w3.org/1998/Math/MathML",
];

/// A curated (not exhaustive) set of elements HTML5 introduced that don't
/// exist in the older XHTML 1.1 module set EPUB 2's content model is
/// built on - confirmed via a real fixture using `<aside>` (the only
/// HTML5-only element actually used anywhere in the whole EPUB 2 corpus).
const HTML5_ONLY_ELEMENTS: [&str; 26] = [
    "aside",
    "section",
    "article",
    "nav",
    "header",
    "footer",
    "hgroup",
    "figure",
    "figcaption",
    "main",
    "mark",
    "time",
    "meter",
    "progress",
    "output",
    "details",
    "summary",
    "dialog",
    "canvas",
    "audio",
    "video",
    "source",
    "track",
    "embed",
    "data",
    "wbr",
];

/// EPUB 2's own content-document DOM rules - the opposite scope from
/// `check_dom` below (EPUB 2 only, not EPUB 3): no HTML5-only elements,
/// no custom-namespaced attributes at all (XHTML 1.1 is closed, unlike
/// EPUB 3's HTML5-based, more extensible profile), and `<a>` may never
/// nest another `<a>` (all three confirmed via dedicated real fixtures).
pub(crate) fn check_dom_epub2(d: &roxmltree::Document, path: &str, report: &mut Report) {
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.tag_name().namespace() == Some(XHTML_NS)
            && HTML5_ONLY_ELEMENTS.contains(&node.tag_name().name())
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("element \"{}\" not allowed here", node.tag_name().name()),
                path,
                node,
                "htm.epub2_dom.html5_only_element",
                vec![node.tag_name().name().to_string()],
            );
        }
        for attr in node.attributes() {
            if let Some(ns) = attr.namespace()
                && !KNOWN_NAMESPACES.contains(&ns)
            {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    format!("attribute \"{}\" not allowed here", attr.name()),
                    path,
                    node,
                    "htm.epub2_dom.custom_namespaced_attribute",
                    vec![attr.name().to_string()],
                );
            }
        }
        if node.tag_name().namespace() == Some(XHTML_NS) && node.tag_name().name() == "a" {
            let nested = node.descendants().skip(1).any(|d| {
                d.is_element()
                    && d.tag_name().namespace() == Some(XHTML_NS)
                    && d.tag_name().name() == "a"
            });
            if nested {
                report.push_node(
                    RSC_005,
                    Severity::Error,
                    "The \"a\" element cannot contain any nested \"a\" elements",
                    path,
                    node,
                    "htm.epub2_dom.nested_anchor",
                    Vec::new(),
                );
            }
        }
    }
    // XHTML 1.1's `body` content model is block-level only (`(%Block.mix;)*`, no
    // `#PCDATA`), so text directly inside `<body>` is a content-model error in
    // EPUB 2 - unlike EPUB 3, whose HTML5 body allows flow content including bare
    // text (#13; the EPUB 2 half, unambiguous and matching epubcheck). Reported
    // per occurrence, anchored at the text run for a precise line:column.
    for body in d.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some(XHTML_NS)
            && n.tag_name().name() == "body"
    }) {
        for child in body.children() {
            if child.is_text() && child.text().is_some_and(|t| !t.trim().is_empty()) {
                report.push_node_text(
                    RSC_005,
                    Severity::Error,
                    "text is not allowed directly in \"body\"; EPUB 2 requires block-level content",
                    path,
                    child,
                    "htm.epub2_dom.bare_text_in_body",
                    Vec::new(),
                );
            }
        }
    }
}

/// DOM/attribute-level checks on an already-parsed content document.
/// EPUB3-only, same reasoning as `check_raw` above (all confirmed from the
/// `epub3/06-content-document/content-document-xhtml.feature` section).
pub(crate) fn check_dom(d: &roxmltree::Document, path: &str, is_epub3: bool, report: &mut Report) {
    if !is_epub3 {
        return;
    }
    for node in d.descendants().filter(|n| n.is_element()) {
        if node.tag_name().namespace() == Some(XHTML_NS)
            && matches!(node.tag_name().name(), "base" | "embed" | "rp")
        {
            report.push_at_pos(
                HTM_055,
                Severity::Usage,
                format!("'{}' is a discouraged construct", node.tag_name().name()),
                path,
                Position::of(node),
            );
        }

        for attr in node.attributes() {
            match attr.namespace() {
                Some(ns) if ns == SSML_NS && attr.name() == "ph" => {
                    if attr.value().trim().is_empty() {
                        report.push_at_pos(
                            HTM_007,
                            Severity::Warning,
                            "ssml:ph must not be empty",
                            path,
                            Position::of(node),
                        );
                    }
                }
                Some(ns) if !KNOWN_NAMESPACES.contains(&ns) && reserved_namespace_host(ns) => {
                    report.push_at_pos(
                        HTM_054,
                        Severity::Error,
                        format!("attribute uses a reserved namespace '{ns}'"),
                        path,
                        Position::of(node),
                    );
                }
                None => {
                    if let Some(rest) = attr.name().strip_prefix("data-")
                        && !is_valid_data_attr_suffix(rest)
                    {
                        report.push_at_pos(
                            HTM_061,
                            Severity::Error,
                            format!("'data-{rest}' is not a valid data-* attribute name"),
                            path,
                            Position::of(node),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn reserved_namespace_host(uri: &str) -> bool {
    let after_scheme = uri.split("://").nth(1).unwrap_or(uri);
    let host = after_scheme.split('/').next().unwrap_or(after_scheme);
    host == "w3.org"
        || host.ends_with(".w3.org")
        || host == "idpf.org"
        || host.ends_with(".idpf.org")
}

fn is_valid_data_attr_suffix(rest: &str) -> bool {
    !rest.is_empty() && !rest.starts_with('-') && !rest.chars().any(|c| c.is_ascii_uppercase())
}

/// The HTML5 `<time datetime>` microsyntax - reverse-engineered directly
/// from the real corpus's exhaustive valid/invalid fixture pairs (not from
/// memory of the spec text), since several of its rules are non-obvious:
/// the separator between date and time may be either "T" or a literal
/// space; an offset may be "+HHMM" or "+HH:MM" but never combined with a
/// trailing "Z"; a fractional-seconds part (in a plain time, a global
/// date-time, or a duration) is capped at 1-3 digits even though nothing
/// else here is digit-count-limited; and a "duration" string has two
/// entirely different valid shapes - an ISO-8601-like "P...T..." form, or
/// a bare whitespace-separated sequence of "<number><unit>" components
/// with no "P"/"T" markers at all (both are exercised by real fixtures).
pub(crate) fn is_valid_html5_datetime(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || !s.is_ascii() {
        return false;
    }
    is_valid_year(s)
        || is_valid_month(s)
        || is_valid_date(s)
        || is_valid_yearless_date(s)
        || is_valid_time(s)
        || is_valid_week(s)
        || is_valid_global_datetime(s)
        || is_valid_duration(s)
}

fn all_digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

fn digit_range(s: &str, lo: u32, hi: u32) -> bool {
    all_digits(s, 2) && s.parse::<u32>().is_ok_and(|n| n >= lo && n <= hi)
}

fn is_valid_year(s: &str) -> bool {
    all_digits(s, 4)
}

fn is_valid_month(s: &str) -> bool {
    let Some((y, m)) = s.split_once('-') else {
        return false;
    };
    all_digits(y, 4) && digit_range(m, 1, 12)
}

fn is_valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    matches!(parts.as_slice(), [y, m, d] if all_digits(y, 4) && digit_range(m, 1, 12) && digit_range(d, 1, 31))
}

fn is_valid_yearless_date(s: &str) -> bool {
    let s = s.strip_prefix("--").unwrap_or(s);
    let parts: Vec<&str> = s.split('-').collect();
    matches!(parts.as_slice(), [m, d] if digit_range(m, 1, 12) && digit_range(d, 1, 31))
}

fn is_valid_week(s: &str) -> bool {
    let Some((y, w)) = s.split_once("-W") else {
        return false;
    };
    all_digits(y, 4) && digit_range(w, 1, 53)
}

fn valid_seconds(s: &str) -> bool {
    match s.split_once('.') {
        Some((whole, frac)) => {
            digit_range(whole, 0, 59)
                && !frac.is_empty()
                && frac.len() <= 3
                && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => digit_range(s, 0, 59),
    }
}

fn is_valid_time(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.as_slice() {
        [h, m] => digit_range(h, 0, 23) && digit_range(m, 0, 59),
        [h, m, sec] => digit_range(h, 0, 23) && digit_range(m, 0, 59) && valid_seconds(sec),
        _ => false,
    }
}

/// A full date, "T" or a single space, a time, and an optional "Z" or
/// numeric UTC offset (never both).
fn is_valid_global_datetime(s: &str) -> bool {
    if s.len() < 11 {
        return false;
    }
    let (date_part, rest) = s.split_at(10);
    if !is_valid_date(date_part) {
        return false;
    }
    if rest[0..1] != *"T" && rest[0..1] != *" " {
        return false;
    }
    let time_and_offset = &rest[1..];
    if time_and_offset.starts_with(' ') {
        return false;
    }
    if let Some(time_part) = time_and_offset.strip_suffix('Z') {
        return is_valid_time(time_part);
    }
    if time_and_offset.len() >= 6 {
        let (maybe_time, off) = time_and_offset.split_at(time_and_offset.len() - 6);
        if (off.starts_with('+') || off.starts_with('-'))
            && &off[3..4] == ":"
            && digit_range(&off[1..3], 0, 23)
            && digit_range(&off[4..6], 0, 59)
        {
            return is_valid_time(maybe_time);
        }
    }
    if time_and_offset.len() >= 5 {
        let (maybe_time, off) = time_and_offset.split_at(time_and_offset.len() - 5);
        if (off.starts_with('+') || off.starts_with('-'))
            && digit_range(&off[1..3], 0, 23)
            && digit_range(&off[3..5], 0, 59)
        {
            return is_valid_time(maybe_time);
        }
    }
    is_valid_time(time_and_offset)
}

fn is_valid_duration(s: &str) -> bool {
    match s.strip_prefix('P') {
        Some(rest) => is_valid_p_duration(rest),
        None => is_valid_flat_duration(s),
    }
}

/// Consumes leading `<digits><unit>` runs from `cursor` in the given unit
/// order, returning `None` if any unit's preceding digit-run isn't purely
/// digits (which also naturally rejects out-of-order units, since a unit
/// letter appearing "too early" ends up embedded inside a supposedly
/// all-digit span for a later unit).
fn consume_units<'a>(mut cursor: &'a str, units: &[char], any: &mut bool) -> Option<&'a str> {
    for unit in units {
        if let Some(idx) = cursor.find(*unit) {
            let num = &cursor[..idx];
            if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *any = true;
            cursor = &cursor[idx + 1..];
        }
    }
    Some(cursor)
}

fn is_valid_p_duration(rest: &str) -> bool {
    let (date_part, time_part) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut any = false;
    let Some(leftover) = consume_units(date_part, &['Y', 'M', 'D'], &mut any) else {
        return false;
    };
    if !leftover.is_empty() {
        return false;
    }
    if let Some(t) = time_part {
        if t.is_empty() {
            return false;
        }
        let mut any_time = false;
        let Some(leftover) = consume_units(t, &['H', 'M'], &mut any_time) else {
            return false;
        };
        let leftover = match leftover.strip_suffix('S') {
            Some(rest_s) if !rest_s.is_empty() => {
                let ok = match rest_s.split_once('.') {
                    Some((whole, frac)) => {
                        !whole.is_empty()
                            && whole.bytes().all(|b| b.is_ascii_digit())
                            && !frac.is_empty()
                            && frac.len() <= 3
                            && frac.bytes().all(|b| b.is_ascii_digit())
                    }
                    None => rest_s.bytes().all(|b| b.is_ascii_digit()),
                };
                if !ok {
                    return false;
                }
                any_time = true;
                ""
            }
            _ => leftover,
        };
        if !leftover.is_empty() || !any_time {
            return false;
        }
        any = true;
    }
    any
}

fn is_valid_duration_component(token: &str) -> bool {
    let Some(unit) = token.chars().next_back() else {
        return false;
    };
    if !matches!(unit, 'Y' | 'M' | 'W' | 'D' | 'H' | 'S') {
        return false;
    }
    let num = &token[..token.len() - unit.len_utf8()];
    match num.split_once('.') {
        Some((whole, frac)) => {
            !whole.is_empty()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && !frac.is_empty()
                && frac.len() <= 3
                && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()),
    }
}

fn is_valid_flat_duration(s: &str) -> bool {
    let mut any = false;
    for token in s.split_whitespace() {
        if !is_valid_duration_component(token) {
            return false;
        }
        any = true;
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_raw(text: &str) -> Vec<&'static str> {
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    fn run_dom(xhtml: &str) -> Vec<&'static str> {
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_dom(&d, "content.xhtml", true, &mut report);
        report.messages.iter().map(|m| m.id).collect()
    }

    /// EPUB 2 DOM checks all share the `RSC-005` id, so tests key off the
    /// finer `rule` sub-code.
    fn run_dom_epub2(xhtml: &str) -> Vec<&'static str> {
        let d = crate::ocf::parse_xml(xhtml).unwrap();
        let mut report = Report::new();
        check_dom_epub2(&d, "content.xhtml", &mut report);
        report.messages.iter().filter_map(|m| m.rule).collect()
    }

    #[test]
    fn epub2_bare_text_in_body_is_a_content_model_error() {
        // XHTML 1.1 `body` is block-level only, so loose text there is an error
        // in EPUB 2 (#13). Reported once per text run; whitespace is ignored;
        // text wrapped in a block element is fine.
        let bare =
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>Call me Ishmael.</body></html>"#;
        assert_eq!(run_dom_epub2(bare), vec!["htm.epub2_dom.bare_text_in_body"]);

        let two =
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>one<p>ok</p>two</body></html>"#;
        assert_eq!(
            run_dom_epub2(two),
            vec![
                "htm.epub2_dom.bare_text_in_body",
                "htm.epub2_dom.bare_text_in_body"
            ]
        );

        let wrapped =
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>  <p>ok</p>  </body></html>"#;
        assert!(run_dom_epub2(wrapped).is_empty());
    }

    #[test]
    fn xml_11_declaration_errors() {
        assert_eq!(run_raw("<?xml version=\"1.1\"?><html/>"), vec![HTM_001]);
        assert!(run_raw("<?xml version=\"1.0\"?><html/>").is_empty());
    }

    #[test]
    fn utf16_bom_errors() {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend("<html/>".encode_utf16().flat_map(|c| c.to_be_bytes()));
        let mut report = Report::new();
        check_raw(&bytes, "<html/>", "content.xhtml", true, &mut report);
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_058]);
    }

    #[test]
    fn obsolete_public_doctype_errors() {
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        assert_eq!(run_raw(text), vec![HTM_004]);
    }

    #[test]
    fn legacy_compat_doctype_is_valid() {
        let text = r#"<!DOCTYPE html SYSTEM "about:legacy-compat"><html/>"#;
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn epub2_xhtml11_dtd_doctype_is_not_flagged() {
        // A real false positive found via the corpus, not by inspection:
        // dozens of EPUB2 fixtures across the corpus legitimately use this
        // exact doctype as their standard XHTML 1.1 content-document
        // template - HTM-004 only applies to EPUB3's stricter content
        // model.
        let text = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html/>"#;
        let mut report = Report::new();
        check_raw(text.as_bytes(), text, "content.xhtml", false, &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn external_entity_errors() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo SYSTEM \"sample.dtd\">\n]><html/>";
        assert_eq!(run_raw(text), vec![HTM_003]);
    }

    #[test]
    fn internal_only_entity_is_valid() {
        let text = "<!DOCTYPE html [\n  <!ENTITY foo \"foo\">\n]><html/>";
        assert!(run_raw(text).is_empty());
    }

    #[test]
    fn reserved_namespace_attribute_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:w3="http://example.w3.org" xmlns:idpf="http://example.idpf.org" xmlns:ok="http://example.org/w3.org">
            <body w3:attr="disallowed" idpf:attr="disallowed" ok:attr="allowed"/>
        </html>"#;
        let ids = run_dom(xhtml);
        assert_eq!(ids, vec![HTM_054, HTM_054]);
    }

    #[test]
    fn known_epub_and_xhtml_namespaces_are_not_reserved() {
        // epub:type is a legitimate, standard attribute - not a custom
        // attribute impersonating a w3.org/idpf.org affiliation. A real
        // false positive found via a real nav fixture
        // (epub:type="toc"), not by inspection.
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
            <body><nav epub:type="toc"/></body>
        </html>"#;
        assert!(run_dom(xhtml).is_empty());
    }

    #[test]
    fn discouraged_elements() {
        for el in ["base", "embed", "rp"] {
            let xhtml = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><{el}/></body></html>"#
            );
            assert_eq!(run_dom(&xhtml), vec![HTM_055], "element {el}");
        }
    }

    #[test]
    fn ssml_ph_empty_errors() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:ssml="http://www.w3.org/2001/10/synthesis">
            <body><span ssml:ph=" "></span><span ssml:ph="ok"></span></body>
        </html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_007]);
    }

    #[test]
    fn invalid_data_attrs() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div data-=""/>
            <div data--test=""/>
            <div data-ERR=""/>
            <div data-ok=""/>
        </body></html>"#;
        assert_eq!(run_dom(xhtml), vec![HTM_061, HTM_061, HTM_061]);
    }

    #[test]
    fn opf_doctype_errors() {
        let mut report = Report::new();
        check_opf_doctype(
            "<!DOCTYPE html PUBLIC \"x\" \"y\"><package/>",
            "content.opf",
            &mut report,
        );
        let ids: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![HTM_009]);

        let mut report = Report::new();
        check_opf_doctype("<package/>", "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn opf_doctype_with_matching_root_name_is_valid() {
        // A real false positive found via the corpus: the legacy OEB 1.2
        // *Package* doctype's root name ("package") matches the OPF's own
        // root element, so it's explicitly valid - only a root-name
        // mismatch (e.g. an "html" doctype in a "package" document) is an
        // actual error; the PUBLIC identifier itself isn't what's judged.
        let text = r#"<!DOCTYPE package PUBLIC "+//ISBN 0-9673008-1-9//DTD OEB 1.2 Package//EN" "http://openebook.org/dtds/oeb-1.2/oebpkg12.dtd"><package/>"#;
        let mut report = Report::new();
        check_opf_doctype(text, "content.opf", &mut report);
        assert!(report.messages.is_empty());
    }

    #[test]
    fn clean_document_no_findings() {
        let xhtml = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body data-ok="1"/></html>"#;
        assert!(run_raw(xhtml).is_empty());
        assert!(run_dom(xhtml).is_empty());
    }

    #[test]
    fn datetime_rejects_all_25_real_invalid_values() {
        // Every one of `time-error.xhtml`'s 25 real corpus datetime values,
        // each individually confirmed invalid.
        const INVALID: &[&str] = &[
            "201",
            "09999001",
            "01--31",
            "---12-31",
            "3123-05-31-12",
            "2019-01-25T 12:12:12Z",
            "2019-01-2512:12:12",
            "2019-01-25A12:12:12Z",
            "2019-01-25T12:12:12-0500Z",
            "2019-01-25 12:12:12-05 00",
            "2019-01-25 12:12:12.33777",
            "2018-W522",
            "2019W01",
            "08::40",
            "19:24:291",
            "14:08:59.999999",
            "P32DT",
            "P32D223T12H",
            "P23DT32M12H",
            "PT2.1112S",
            "PT12H9",
            "P12T431M",
            "9123W12",
            "  1231 23D  ",
            "343HD",
        ];
        for v in INVALID {
            assert!(!is_valid_html5_datetime(v), "expected invalid: {v}");
        }
    }

    #[test]
    fn datetime_accepts_all_real_valid_values() {
        // Every one of `time-valid.xhtml`'s real corpus datetime values.
        const VALID: &[&str] = &[
            "2019",
            "0001",
            "01-31",
            "02-28",
            "--12-31",
            "3123-05-31",
            "1200-08",
            "2019-01-25T12:12:12Z",
            "2019-01-25 12:12:12",
            "2019-01-25 12:12:12Z",
            "2019-01-25 12:12:12-0500",
            "2019-01-25 12:12:12-05:00",
            "2019-01-25 12:12:12.777",
            "2018-W52",
            "2019-W01",
            "08:40",
            "19:24:29",
            "14:08:59.999",
            "P32D",
            "P32DT12H",
            "P23DT12H32M1231S",
            "PT12H23M12.112S",
            "PT12H",
            "PT431M",
            "PT12.433S",
            "9123W",
            "  123123D  ",
            "343H",
            "1M",
            "12S",
            "12.12S",
            "123W 123H   32D 12S",
            "2014-03",
        ];
        for v in VALID {
            assert!(is_valid_html5_datetime(v), "expected valid: {v}");
        }
    }

    #[test]
    fn entity_missing_semicolon_and_unknown_name() {
        let mut report = Report::new();
        check_entities("&amp ", "content.xhtml", true, &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );

        let mut report = Report::new();
        check_entities("&foo;", "content.xhtml", true, &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );
    }

    #[test]
    fn entity_predefined_and_declared_are_valid() {
        let mut report = Report::new();
        check_entities("&amp; &lt; &gt;", "content.xhtml", true, &mut report);
        assert!(report.messages.is_empty());

        let mut report = Report::new();
        check_entities(
            "<!DOCTYPE html [<!ENTITY foo \"bar\">]><p>&foo;</p>",
            "content.xhtml",
            true,
            &mut report,
        );
        assert!(report.messages.is_empty());
    }

    #[test]
    fn epub2_xhtml_named_entities_are_valid() {
        // A recognized EPUB 2 XHTML DOCTYPE pulls in the external DTD that
        // declares `&nbsp;` & friends - they must not be flagged (forum
        // report: French ebooks, `<p>&nbsp;</p>` spacing).
        let doc = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" ",
            "\"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">",
            "<html><body><p>&nbsp;&eacute;&copy;&mdash;</p></body></html>"
        );
        let mut report = Report::new();
        check_entities(doc, "content.xhtml", false, &mut report);
        assert!(report.messages.is_empty(), "{:?}", report.messages);
    }

    #[test]
    fn epub3_named_entities_still_undeclared() {
        // EPUB 3 keeps the strict rule: `&nbsp;` without an internal
        // declaration is an undeclared-entity RSC-016 (must be numeric).
        let mut report = Report::new();
        check_entities(
            "<html><body><p>&nbsp;</p></body></html>",
            "c.xhtml",
            true,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );
    }

    #[test]
    fn epub2_named_entity_without_known_doctype_still_flagged() {
        // EPUB 2 but no recognized XHTML DOCTYPE => the external DTD isn't
        // pulled in, so the entity is genuinely undeclared (matches the
        // HTM-004 the DOCTYPE check separately raises).
        let mut report = Report::new();
        check_entities(
            "<html><body><p>&nbsp;</p></body></html>",
            "c.xhtml",
            false,
            &mut report,
        );
        assert_eq!(
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![RSC_016]
        );
    }

    /// The exact shape from issue #23: an XHTML 1.1 DOCTYPE plus `&nbsp;`,
    /// which is what Sigil writes by default. It must parse, and the id
    /// must be visible to the id-based checks.
    #[test]
    fn declare_dtd_entities_makes_the_epub2_nbsp_shape_parse() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>\n",
            "<h1 id=\"sigil_toc_id_3\">a&nbsp;b</h1>\n",
            "</body></html>",
        );
        assert!(
            crate::ocf::parse_xml(text).is_err(),
            "precondition: the raw document is what roxmltree cannot parse"
        );
        let (out, _) = declare_dtd_entities(text.to_string(), false);
        let doc = crate::ocf::parse_xml(&out).expect("declared entities should let it parse");
        let h1 = doc.descendants().find(|n| n.has_tag_name("h1")).unwrap();
        assert_eq!(h1.attribute("id"), Some("sigil_toc_id_3"));
        assert_eq!(h1.text(), Some("a\u{a0}b"));
    }

    /// Issue #25. A `[1]` footnote marker in the body - one of the most
    /// ordinary things in a real book - used to make `extract_doctype`
    /// search the whole rest of the document for an internal subset, decide
    /// the footnote's bracket was one, and inject the entity declarations
    /// into the middle of the body. 78 false fatals across 11 books, on
    /// documents that are perfectly valid.
    #[test]
    fn declare_dtd_entities_is_not_confused_by_brackets_in_the_body() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>\n",
            "<p>A footnote marker [1] in the body.</p>\n",
            "<p>a&nbsp;b</p>\n",
            "</body></html>",
        );
        let (out, _) = declare_dtd_entities(text.to_string(), false);
        assert!(
            out.contains("[1] in the body"),
            "the body must come through untouched; got:\n{out}"
        );
        let doc = crate::ocf::parse_xml(&out).expect("a valid document must still parse");
        let ps: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("p"))
            .filter_map(|n| n.text().map(str::to_string))
            .collect();
        assert_eq!(ps[0], "A footnote marker [1] in the body.");
        assert_eq!(ps[1], "a\u{a0}b");
    }

    /// The DOCTYPE's own `>` is not the first `>` after it: an internal
    /// subset is full of them. Nor is a `]` inside a quoted public
    /// identifier the subset's close.
    #[test]
    fn doctype_span_is_bounded_correctly() {
        // No subset, `[` and `>` further down the document.
        let (dt, subset) = doctype_span("<!DOCTYPE html SYSTEM \"a.dtd\">\n<p>[x]</p>").unwrap();
        assert_eq!(dt, "<!DOCTYPE html SYSTEM \"a.dtd\">");
        assert_eq!(subset, None);

        // A subset: the declaration ends *after* it, not at the `>` of the
        // `<!ENTITY>` inside it, and the reported range is its contents.
        let text = "<!DOCTYPE html [<!ENTITY x \"&#65;\">]>\n<p>y</p>";
        let (dt, subset) = doctype_span(text).unwrap();
        assert_eq!(dt, "<!DOCTYPE html [<!ENTITY x \"&#65;\">]>");
        assert_eq!(subset.map(|r| &dt[r]), Some("<!ENTITY x \"&#65;\">"));

        // Brackets inside a quoted literal belong to the literal.
        let (dt, subset) =
            doctype_span("<!DOCTYPE html PUBLIC \"a[b]c\" \"d.dtd\">\n<p>z</p>").unwrap();
        assert_eq!(dt, "<!DOCTYPE html PUBLIC \"a[b]c\" \"d.dtd\">");
        assert_eq!(subset, None, "the ']' is inside the public identifier");

        // A `]` inside an entity value does not close the subset: cutting it
        // short there would drop `y`'s declaration, and a declared entity
        // reported as undeclared is a fatal.
        let text = "<!DOCTYPE html [<!ENTITY x \"]\"><!ENTITY y \"&#66;\">]>\n<p>z</p>";
        let (dt, subset) = doctype_span(text).unwrap();
        assert_eq!(
            subset.map(|r| &dt[r]),
            Some("<!ENTITY x \"]\"><!ENTITY y \"&#66;\">")
        );
        assert_eq!(declared_entity_names(text), vec!["x", "y"]);
    }

    /// The property the whole approach rests on: declarations are injected
    /// without adding a newline, so every position epubveri reports still
    /// points at the same place in the file the user actually has.
    #[test]
    fn declare_dtd_entities_preserves_line_numbers() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>\n",
            "<p>&nbsp;</p>\n",
            "<h1 id=\"x\">t</h1>\n",
            "</body></html>",
        );
        let (out, _) = declare_dtd_entities(text.to_string(), false);
        assert_eq!(
            out.lines().count(),
            text.lines().count(),
            "no newline may be added"
        );
        let doc = crate::ocf::parse_xml(&out).unwrap();
        let h1 = doc.descendants().find(|n| n.has_tag_name("h1")).unwrap();
        let pos = doc.text_pos_at(h1.range().start);
        assert_eq!((pos.row, pos.col), (5, 1), "h1 is on line 5, column 1");
    }

    /// EPUB 3 is deliberately untouched: a named reference other than the
    /// predefined five is a real error there, and making it parse would
    /// paper over the RSC-016 that should fire.
    #[test]
    fn declare_dtd_entities_leaves_epub3_alone() {
        let text = "<!DOCTYPE html>\n<html><body><p>a&nbsp;b</p></body></html>";
        assert_eq!(declare_dtd_entities(text.to_string(), true).0, text);
        // ... and so is an EPUB 2 document with no XHTML DTD to promise them.
        assert_eq!(declare_dtd_entities(text.to_string(), false).0, text);
    }

    /// Only entities the DTD actually declares are covered; an invented one
    /// stays undeclared, so the parse still fails and `check_entities`
    /// still reports it (the #12 invariant).
    #[test]
    fn declare_dtd_entities_ignores_nonstandard_names() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
            "<html><body><p>a&madeupname;b</p></body></html>",
        );
        let (out, _) = declare_dtd_entities(text.to_string(), false);
        assert_eq!(out, text, "nothing to declare");
        assert!(crate::ocf::parse_xml(&out).is_err());
        let mut report = Report::default();
        check_entities(text, "c.xhtml", false, &mut report);
        assert_eq!(
            report.messages.iter().map(|m| m.rule).collect::<Vec<_>>(),
            vec![Some("htm.entity.undeclared")],
            "the scan must still own this one"
        );
    }

    /// A document that already has an internal subset must get our
    /// declarations *inside* it - a second `[...]` would be malformed - and
    /// its own declaration of a name must win over ours.
    #[test]
    fn declare_dtd_entities_merges_into_an_existing_internal_subset() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\" [\n",
            "  <!ENTITY nbsp \"X\">\n",
            "]>\n",
            "<html><body><p>a&nbsp;b&mdash;c</p></body></html>",
        );
        let (out, _) = declare_dtd_entities(text.to_string(), false);
        assert_eq!(out.matches('[').count(), 1, "no second internal subset");
        let doc = crate::ocf::parse_xml(&out).unwrap();
        let p = doc.descendants().find(|n| n.has_tag_name("p")).unwrap();
        assert_eq!(
            p.text(),
            Some("aXb\u{2014}c"),
            "the document's own &nbsp; wins; &mdash; comes from ours"
        );
    }

    /// An entity mentioned only inside a comment isn't a reference, so
    /// there is nothing to declare and the text is left alone.
    #[test]
    fn declare_dtd_entities_ignores_entities_in_comments() {
        let text = concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
            "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
            "<html><body><!-- &nbsp; --><p>x</p></body></html>",
        );
        assert_eq!(declare_dtd_entities(text.to_string(), false).0, text);
    }

    /// Every name the table offers must actually be declarable: a bad code
    /// point would produce a declaration that fails to parse.
    /// The named-entity table is a lookup: two entries for one name would
    /// make the first silently shadow the second, and a name that is also
    /// one of the five predefined entities would be declared twice in an
    /// augmented document. Neither is reachable through a fixture - both
    /// produce a document that still parses - so only a table invariant
    /// catches them.
    #[test]
    fn named_entities_are_unique_and_disjoint_from_predefined() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in XHTML_NAMED_ENTITIES {
            assert!(seen.insert(*name), "'{name}' appears twice in the table");
            assert!(
                !PREDEFINED_ENTITIES.contains(name),
                "'{name}' is XML-predefined and must not also be in the table"
            );
        }
    }

    #[test]
    fn every_table_entry_declares_and_parses() {
        let refs: String = XHTML_NAMED_ENTITIES
            .iter()
            .map(|(n, _)| format!("&{n};"))
            .collect();
        let text = format!(
            concat!(
                "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n",
                "  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n",
                "<html><body><p>{}</p></body></html>",
            ),
            refs
        );
        let (out, _) = declare_dtd_entities(text, false);
        let doc = crate::ocf::parse_xml(&out).expect("all 248 must parse");
        let p = doc.descendants().find(|n| n.has_tag_name("p")).unwrap();
        let got: Vec<char> = p.text().unwrap().chars().collect();
        assert_eq!(got.len(), XHTML_NAMED_ENTITIES.len());
        for (i, (name, cp)) in XHTML_NAMED_ENTITIES.iter().enumerate() {
            assert_eq!(
                got[i],
                char::from_u32(*cp).unwrap(),
                "&{name}; expanded to the wrong character"
            );
        }
    }
}
