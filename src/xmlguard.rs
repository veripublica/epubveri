//! The limits XML text has to meet before it is handed to `roxmltree`.
//!
//! roxmltree has two failure modes a hostile document can reach and no
//! parsing option bounds: nesting deep enough to overflow the stack, which in
//! Rust aborts the process rather than panicking, and loop-free entities that
//! expand a kilobyte file into gigabytes. [`check`] answers, from a scan of the
//! raw text, whether a document would do either, so it can be declined before
//! the parser sees it.
//!
//! **Public because a re-parse is a second door.** A consumer that parses
//! container entries through its own roxmltree call is exposed to both
//! shapes whatever the validator decided about the same file, and a local
//! copy of this scan would drift. Call [`check`] with the text you are about
//! to parse, after any rewrite of it, and decline the document on `Err`.
//!
//! The scan has to read the text the way the parser will, or a document can
//! hide from it: every `<` the scan wrongly treats as non-markup is nesting it
//! does not count. 0.17.1 shipped with nine such shapes (a quote or a `[`
//! inside a processing instruction, a comment in the internal subset, a
//! non-ASCII element name, markup inside an entity value, a `<!DOCTYPE`
//! quoted in an earlier comment); each one aborted the process or reached
//! 4.8 GB here, and each is a test below. The rule that keeps it honest:
//! **on any byte the scan cannot place, keep counting.** Over-counting a
//! malformed document refuses something the parser would have refused
//! anyway; under-counting is the hole.

use std::collections::HashMap;
use std::ops::Range;

/// The deepest element nesting [`check`] accepts.
///
/// roxmltree's tokenizer is mutually recursive (`parse_element` ↔
/// `parse_content`), so nesting costs stack in proportion to depth and a
/// deeply-nested document aborts the process. In Rust a stack overflow is
/// `SIGABRT`, **not** a catchable panic - `catch_unwind` cannot save an
/// embedder, so this has to be refused before the parser sees it.
///
/// Where the whole validation (parse, RELAX NG, Schematron) first fails,
/// measured 2026-09-23 on a build with this limit lifted, nested `<div>` and
/// nested tables alike: the 8 MiB main thread survives 8,000 and aborts at
/// 12,000; a 2 MiB thread (what an embedder's worker runs) survives 2,000
/// and aborts at 3,000; the WASM package under node's default stack
/// survives 1,500 and throws `RangeError` at 2,000. 0.8.6 had measured
/// ~4,000 for the 2 MiB thread, so the threshold falls as the code grows,
/// and the ladder is worth re-running.
///
/// 256 comes from data, not taste: across the 65-book local shelf the
/// deepest document nests **24** elements (median 8, p95 11). That leaves
/// this ~10x above the worst real book and ~6x below the lowest measured
/// failure (WASM), and all three environments give identical findings at
/// exactly 256. It bounds our own recursion and roxmltree's; a caller that
/// walks the parsed tree recursively on a smaller stack needs its own margin.
pub const MAX_XML_DEPTH: usize = 256;

/// How many bytes of text [`check`] lets entity references add to a
/// document: the same 64 MiB a single container entry may inflate to.
///
/// 60 million characters of expansion, which EPUBCheck 5.4.0 still
/// validates normally, stays under it, so a book it accepts is not refused.
pub const MAX_EXPANSION_BYTES: u64 = crate::ocf::MAX_ENTRY_BYTES;

/// The most attributes, namespace declarations included, [`check`] accepts
/// on one element.
///
/// roxmltree checks each attribute against every earlier one on the same
/// element, for duplicates and again for namespace declarations, so one
/// element costs the square of its attribute count (upstream issue
/// RazrFalcon/roxmltree#153, open). Measured here: 40,000 attributes on one
/// `<p>` take 5.3 s, and the same number spread 16 to an element 0.6 s, a gap
/// that widens with every doubling. Across 22,847 XML files on the shelf the
/// most any element carries is **11** (an SVG logo; a package document
/// reaches 8), so 256 is ~23x above real books and makes the square
/// negligible.
pub const MAX_ATTRIBUTES: usize = 256;

/// The most elements [`check`] accepts in one document, counting the ones
/// entity references bring in each time they are referenced.
///
/// Memory is spent per element, not per byte: roxmltree keeps a node for
/// every element and every run of text, and validation keeps more. Measured
/// here, a 60 MiB chapter of `<b/>` (15.7 million elements, 60 KB deflated)
/// peaked at 1.27 GB and took 28 s, while 60 MiB of prose paragraphs took
/// 202 MB. Across the 474-book shelf the most elements in one document is
/// **20,160**, so a million is ~50x above real books and bounds one document
/// to ~100 MB and ~2 s.
pub const MAX_ELEMENTS: usize = 1_000_000;

/// Why [`check`] declined a document. Each variant carries the value the
/// scan had reached when it stopped, which is a lower bound: it stops at the
/// first point past the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// Element nesting passed [`MAX_XML_DEPTH`].
    TooDeep(usize),
    /// Entity references would add more than [`MAX_EXPANSION_BYTES`].
    TooExpansive(u64),
    /// One element carries more than [`MAX_ATTRIBUTES`] attributes.
    TooManyAttributes(usize),
    /// The document holds more than [`MAX_ELEMENTS`] elements.
    TooManyElements(usize),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::TooDeep(d) => write!(
                f,
                "element nesting is deeper than the {MAX_XML_DEPTH}-element limit \
                 (reached {d}), so the document was not parsed"
            ),
            Refusal::TooExpansive(n) => write!(
                f,
                "its entity references would expand it past the {} MiB limit \
                 (to at least {n} bytes), so the document was not parsed",
                MAX_EXPANSION_BYTES / (1024 * 1024)
            ),
            Refusal::TooManyAttributes(n) => write!(
                f,
                "an element carries {n} attributes, more than the {MAX_ATTRIBUTES} \
                 allowed on one, so the document was not parsed"
            ),
            Refusal::TooManyElements(n) => write!(
                f,
                "it holds more than the {MAX_ELEMENTS} elements allowed in one document \
                 (counted {n}), so the document was not parsed"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// Whether `text` is safe to hand to `roxmltree` with `allow_dtd: true`:
/// `Ok` when it nests no deeper than [`MAX_XML_DEPTH`], holds no more than
/// [`MAX_ELEMENTS`] elements, no element carries more than
/// [`MAX_ATTRIBUTES`] attributes, and its entities add no more than
/// [`MAX_EXPANSION_BYTES`], counting the elements an entity's replacement
/// text brings with it.
///
/// It is not a well-formedness check. A malformed document can pass, and the
/// parser then reports it; what the scan promises is only that the parser
/// will not overflow the stack, exhaust memory, or spend time quadratic in
/// the input on it. Linear in the text,
/// plus the declared entities once each.
pub fn check(text: &str) -> Result<(), Refusal> {
    let prolog = Prolog::read(text);
    let mut ents = Entities::new(prolog.entities);
    let body = &text[prolog.body..];
    let deepest = ents.deepest(body, MAX_XML_DEPTH, 0);
    if let Some(n) = ents.too_many_attributes {
        return Err(Refusal::TooManyAttributes(n));
    }
    if ents.elements > MAX_ELEMENTS {
        return Err(Refusal::TooManyElements(ents.elements));
    }
    if deepest > MAX_XML_DEPTH {
        return Err(Refusal::TooDeep(deepest));
    }
    if !ents.decls.is_empty() {
        let mut total = 0u64;
        for name in entity_refs(body) {
            total = total.saturating_add(ents.expanded_len(name, 0));
            if total > MAX_EXPANSION_BYTES {
                return Err(Refusal::TooExpansive(total));
            }
        }
    }
    Ok(())
}

/// Length of a region starting at `rest[0]` and closed by `end`, which is
/// searched for from offset `from`; the whole remainder when unterminated.
fn region_len(rest: &[u8], from: usize, end: &[u8]) -> usize {
    let start = from.min(rest.len());
    match rest[start..].windows(end.len()).position(|w| w == end) {
        Some(p) => start + p + end.len(),
        None => rest.len(),
    }
}

/// Length of a markup declaration inside the internal subset (`<!ENTITY`,
/// `<!ELEMENT`, `<!ATTLIST`, `<!NOTATION`): up to the first `>` outside a
/// quoted literal, since an entity value may hold `>` and `]`.
fn markup_decl_len(rest: &[u8]) -> usize {
    let mut quote = 0u8;
    for (j, &c) in rest.iter().enumerate().skip(2) {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return j + 1;
        }
    }
    rest.len()
}

/// Length of an element tag, whether it is self-closing, and how many
/// attributes it carries. Quote-aware, because `>` is legal inside an
/// attribute value (`<a title="a>b">`) and treating it as the tag end would
/// miss the `/` of a self-closing tag and over-count depth. Every attribute
/// has exactly one quoted value, so counting the values counts them.
fn tag_len(rest: &[u8]) -> (usize, bool, usize) {
    let (mut j, mut quote, mut last, mut attrs) = (1usize, 0u8, 0u8, 0usize);
    while j < rest.len() {
        let c = rest[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
            attrs += 1;
        } else if c == b'>' {
            return (j + 1, last == b'/', attrs);
        }
        if !c.is_ascii_whitespace() {
            last = c;
        }
        j += 1;
    }
    (rest.len(), false, attrs)
}

/// The name of an `&name;` reference starting at `b[at]`, and the offset just
/// past its `;`. `None` for a character reference (`&#...;`, which expands
/// to one character of text, never to markup) and for a bare `&`.
fn entity_ref_at(text: &str, at: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let start = at + 1;
    if b.get(start) == Some(&b'#') {
        return None;
    }
    let mut j = start;
    while j < b.len()
        && !matches!(b[j], b';' | b'&' | b'<' | b'"' | b'\'')
        && !b[j].is_ascii_whitespace()
    {
        j += 1;
    }
    (j < b.len() && b[j] == b';' && j > start).then(|| (&text[start..j], j + 1))
}

/// Every `&name;` in `text`, in order.
fn entity_refs(text: &str) -> impl Iterator<Item = &str> {
    let b = text.as_bytes();
    let mut i = 0usize;
    std::iter::from_fn(move || {
        while i < b.len() {
            if b[i] != b'&' {
                i += 1;
                continue;
            }
            match entity_ref_at(text, i) {
                Some((name, next)) => {
                    i = next;
                    return Some(name);
                }
                None => i += 1,
            }
        }
        None
    })
}

/// What precedes the root element: the general entities of the DOCTYPE's
/// internal subset, and where the body starts.
struct Prolog<'a> {
    entities: HashMap<&'a str, &'a str>,
    body: usize,
    /// The DOCTYPE's extent and its internal subset's, when it has a closing
    /// `>`; see [`doctype_span`].
    doctype: Option<(Range<usize>, Option<Range<usize>>)>,
}

/// Where the document's DOCTYPE is, and the extent of its internal subset
/// (between the brackets, as absolute offsets) if it has one; `None` when
/// the prolog has no DOCTYPE or it never closes.
///
/// The one DOCTYPE reader in the crate, shared with the checks that read
/// the subset, because two readers of the same bytes drift apart: 0.17.1's
/// guard and `htm`'s scanner were each fooled by a `<!DOCTYPE` quoted in an
/// earlier comment, and the scanner then called a declared entity
/// undeclared.
pub(crate) fn doctype_span(text: &str) -> Option<(Range<usize>, Option<Range<usize>>)> {
    Prolog::read(text).doctype
}

impl<'a> Prolog<'a> {
    /// Lexes the prolog the way XML defines it - an optional BOM, then
    /// whitespace, processing instructions and comments, at most one
    /// DOCTYPE - and stops at the first thing that is none of those. A
    /// DOCTYPE is only ever read here, so one quoted inside a comment or a
    /// processing instruction is skipped with it rather than taken for the
    /// real one.
    fn read(text: &'a str) -> Self {
        let b = text.as_bytes();
        let mut i = if b.starts_with("\u{FEFF}".as_bytes()) {
            3
        } else {
            0
        };
        loop {
            while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            let rest = &b[i..];
            if rest.starts_with(b"<?") {
                i += region_len(rest, 2, b"?>");
            } else if rest.starts_with(b"<!--") {
                i += region_len(rest, 4, b"-->");
            } else if rest.starts_with(b"<!DOCTYPE") {
                return Self::doctype(text, i);
            } else {
                return Prolog {
                    entities: HashMap::new(),
                    body: i,
                    doctype: None,
                };
            }
        }
    }

    /// The DOCTYPE starting at `dt`: its internal subset's general
    /// entities, the offset past its closing `>`, and its extent. The first
    /// declaration
    /// of a name wins, as XML says; parameter and external entities are not
    /// expanded into the document's text, so they are not collected.
    fn doctype(text: &'a str, dt: usize) -> Self {
        let b = text.as_bytes();
        let mut entities = HashMap::new();
        let end = |body: usize, closed: bool| Prolog {
            entities: HashMap::new(),
            body,
            doctype: closed.then_some((dt..body, None)),
        };
        // Up to the subset's `[`, stepping over quoted public/system ids.
        let mut i = dt + 9;
        loop {
            match b.get(i) {
                None => return end(b.len(), false),
                Some(b'[') => break,
                Some(b'>') => return end(i + 1, true),
                Some(&q @ (b'"' | b'\'')) => match text[i + 1..].find(q as char) {
                    Some(p) => i += p + 1,
                    None => return end(b.len(), false),
                },
                Some(_) => {}
            }
            i += 1;
        }
        i += 1;
        let subset_start = i;
        loop {
            let rest = &b[i..];
            match rest.first() {
                None => break,
                Some(b']') => {
                    let close = text[i..].find('>').map(|p| i + p + 1);
                    return Prolog {
                        entities,
                        body: close.unwrap_or(b.len()),
                        doctype: close.map(|c| (dt..c, Some(subset_start..i))),
                    };
                }
                _ if rest.starts_with(b"<!--") => i += region_len(rest, 4, b"-->"),
                _ if rest.starts_with(b"<?") => i += region_len(rest, 2, b"?>"),
                _ if rest.starts_with(b"<!ENTITY") => {
                    let mut j = i + 8;
                    while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    if b.get(j) != Some(&b'%') {
                        let name_start = j;
                        while b
                            .get(j)
                            .is_some_and(|c| !c.is_ascii_whitespace() && *c != b'>')
                        {
                            j += 1;
                        }
                        let name = &text[name_start..j];
                        while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                            j += 1;
                        }
                        if let Some(&q @ (b'"' | b'\'')) = b.get(j)
                            && let Some(p) = text[j + 1..].find(q as char)
                        {
                            entities.entry(name).or_insert(&text[j + 1..j + 1 + p]);
                        }
                    }
                    i += markup_decl_len(rest);
                }
                _ if rest.starts_with(b"<!") => i += markup_decl_len(rest),
                _ => i += 1,
            }
        }
        Prolog {
            entities,
            body: b.len(),
            doctype: None,
        }
    }
}

/// How deep [`Entities`] follows entities that reference entities. It stops
/// a long chain of declarations from turning this guard into a stack
/// overflow of its own. Past it the count **fails closed**: a chain this deep
/// is treated as unbounded and the document is refused. roxmltree happens to
/// refuse such a chain anyway (measured: 8 levels parse, 12 are reported as
/// a probable loop), but a bound that holds only while a dependency keeps an
/// undocumented limit is not a bound.
const MAX_ENTITY_NESTING: usize = 64;

/// The declared general entities, and what each one costs once referenced.
struct Entities<'a> {
    decls: HashMap<&'a str, &'a str>,
    /// Per entity: how deep its elements nest, and how many there are.
    depth: HashMap<&'a str, (usize, usize)>,
    len: HashMap<&'a str, u64>,
    /// Set by the scan when an element carries more than [`MAX_ATTRIBUTES`];
    /// the scan stops there, entity values included.
    too_many_attributes: Option<usize>,
    /// Elements counted so far in the document being scanned; the scan stops
    /// once this passes [`MAX_ELEMENTS`].
    elements: usize,
}

impl<'a> Entities<'a> {
    fn new(decls: HashMap<&'a str, &'a str>) -> Self {
        Entities {
            decls,
            depth: HashMap::new(),
            len: HashMap::new(),
            too_many_attributes: None,
            elements: 0,
        }
    }

    /// The deepest element nesting `s` reaches, counting what the entities
    /// it references bring with them; it stops early once past `limit`.
    ///
    /// A raw-byte scan rather than a parse, since the point is to run
    /// *before* roxmltree's recursive tokenizer touches the text. Comments,
    /// CDATA sections, processing instructions and quoted attribute values
    /// are skipped by their own terminators and nothing else, because those
    /// are the only regions where `<` is not markup. Any other `<`, even one
    /// the parser would reject, is counted.
    fn deepest(&mut self, s: &'a str, limit: usize, nesting: usize) -> usize {
        let b = s.as_bytes();
        let (mut i, mut depth, mut max) = (0usize, 0usize, 0usize);
        while i < b.len() {
            if b[i] == b'&' {
                // An entity's replacement text is parsed in place, so the
                // elements it holds nest under the current one.
                if let Some((name, next)) = entity_ref_at(s, i) {
                    if self.decls.contains_key(name) {
                        let (inner, count) = self.depth_of(name, limit, nesting + 1);
                        max = max.max(depth.saturating_add(inner));
                        self.elements = self.elements.saturating_add(count);
                        if max > limit
                            || self.too_many_attributes.is_some()
                            || self.elements > MAX_ELEMENTS
                        {
                            return max;
                        }
                    }
                    i = next;
                } else {
                    i += 1;
                }
                continue;
            }
            if b[i] != b'<' {
                i += 1;
                continue;
            }
            let rest = &b[i..];
            if rest.starts_with(b"<!--") {
                i += region_len(rest, 4, b"-->");
                continue;
            }
            if rest.starts_with(b"<![CDATA[") {
                i += region_len(rest, 9, b"]]>");
                continue;
            }
            // Only `?>` ends a processing instruction: quotes and brackets
            // inside one mean nothing. Reading them as quotes let 0.17.1 skip
            // the rest of a document after `<?x don't?>`.
            if rest.starts_with(b"<?") {
                i += region_len(rest, 2, b"?>");
                continue;
            }
            let Some(&c) = b.get(i + 1) else { break };
            let closing = c == b'/';
            // A name may start with any non-ASCII letter, so a byte past
            // ASCII opens a tag; `<` before anything else is stray text,
            // which the parser rejects before it can nest further.
            if !closing && !(c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80) {
                i += 1;
                continue;
            }
            let (len, self_closing, attrs) = tag_len(rest);
            if attrs > MAX_ATTRIBUTES {
                self.too_many_attributes = Some(attrs);
                return max;
            }
            if closing {
                depth = depth.saturating_sub(1);
            } else {
                self.elements += 1;
                if self.elements > MAX_ELEMENTS {
                    return max;
                }
                if !self_closing {
                    depth += 1;
                    max = max.max(depth);
                    if max > limit {
                        return max;
                    }
                }
            }
            i += len;
        }
        max
    }

    /// How deep the elements in entity `name`'s replacement text nest, and
    /// how many of them there are, nested references included. The caller
    /// adds the count once per reference, which is how the parser expands it.
    fn depth_of(&mut self, name: &'a str, limit: usize, nesting: usize) -> (usize, usize) {
        if let Some(&d) = self.depth.get(name) {
            return d;
        }
        let Some(&value) = self.decls.get(name) else {
            return (0, 0);
        };
        if nesting >= MAX_ENTITY_NESTING {
            return (usize::MAX, usize::MAX);
        }
        // Provisional zeros while this entity is being scanned, so a loop
        // back to it terminates; the parser refuses the loop itself.
        self.depth.insert(name, (0, 0));
        // The scan counts into `elements`; take this entity's share back out,
        // since it is charged per reference by the caller.
        let before = self.elements;
        let d = self.deepest(value, limit, nesting);
        let count = self.elements - before;
        self.elements = before;
        self.depth.insert(name, (d, count));
        (d, count)
    }

    /// The text entity `name` expands to, in bytes, counting the entities
    /// its replacement text references in turn. Undeclared names (the
    /// predefined five, or a typo the parser will report) cost nothing, and
    /// so does a loop, which the parser refuses; nesting past
    /// [`MAX_ENTITY_NESTING`] costs everything.
    fn expanded_len(&mut self, name: &'a str, nesting: usize) -> u64 {
        if let Some(&n) = self.len.get(name) {
            return n;
        }
        let Some(&value) = self.decls.get(name) else {
            return 0;
        };
        if nesting >= MAX_ENTITY_NESTING {
            return u64::MAX;
        }
        self.len.insert(name, 0);
        // A declared reference is replaced by its expansion, so its own
        // `&name;` text is not part of the result. Keeping it would compound
        // at every level: three levels of ten counted 7,440 bytes for 3,000
        // of text, and an inflated count is how a book EPUBCheck accepts
        // would be refused here.
        let mut n = value.len() as u64;
        for inner in entity_refs(value) {
            if self.decls.contains_key(inner) {
                n = n
                    .saturating_sub(inner.len() as u64 + 2)
                    .saturating_add(self.expanded_len(inner, nesting + 1));
            }
        }
        self.len.insert(name, n);
        n
    }
}

#[cfg(test)]
mod expansion_tests {
    use super::{MAX_ENTITY_NESTING, MAX_EXPANSION_BYTES, Refusal, check};

    /// The expansion `check` reports past `limit` bytes, using the guard's
    /// internals so the count itself can be pinned at small sizes.
    fn expansion(text: &str, limit: u64) -> Option<u64> {
        let prolog = super::Prolog::read(text);
        let mut ents = super::Entities::new(prolog.entities);
        let mut total = 0u64;
        for name in super::entity_refs(&text[prolog.body..]) {
            total = total.saturating_add(ents.expanded_len(name, 0));
            if total > limit {
                return Some(total);
            }
        }
        None
    }

    /// A document with one entity of `size` characters referenced `refs` times.
    fn doc(size: usize, refs: usize) -> String {
        format!(
            "<!DOCTYPE html [<!ENTITY a \"{}\">]><html><p>{}</p></html>",
            "A".repeat(size),
            "&a;".repeat(refs)
        )
    }

    /// The file that used 5 GB is refused before the parser sees it.
    #[test]
    fn the_quadratic_shape_is_refused() {
        assert!(
            matches!(check(&doc(50_000, 50_000)), Err(Refusal::TooExpansive(n)) if n > MAX_EXPANSION_BYTES)
        );
    }

    /// 60 million characters of expansion, which EPUBCheck 5.4.0 validates
    /// normally, is under the limit, so a book it accepts still parses here.
    #[test]
    fn what_epubcheck_accepts_is_not_refused() {
        assert_eq!(check(&doc(10_000, 6_000)), Ok(()));
    }

    #[test]
    fn nested_entities_multiply() {
        // Three levels of ten: 1,000 copies of a 3-byte leaf, exactly.
        let t = concat!(
            "<!DOCTYPE r [<!ENTITY l0 \"abc\">",
            "<!ENTITY l1 \"&l0;&l0;&l0;&l0;&l0;&l0;&l0;&l0;&l0;&l0;\">",
            "<!ENTITY l2 \"&l1;&l1;&l1;&l1;&l1;&l1;&l1;&l1;&l1;&l1;\">",
            "<!ENTITY l3 \"&l2;&l2;&l2;&l2;&l2;&l2;&l2;&l2;&l2;&l2;\">]><r>&l3;</r>"
        );
        assert_eq!(expansion(t, 2_999), Some(3_000));
        assert_eq!(expansion(t, 3_000), None);
    }

    /// The guard must not become the thing it guards against: a loop
    /// terminates, and a long chain of declarations does not overflow the
    /// stack.
    #[test]
    fn loops_and_long_chains_terminate() {
        // A loop costs nothing here and is left to the parser, which refuses it.
        let t = "<!DOCTYPE r [<!ENTITY a \"&b;\"><!ENTITY b \"&a;\">]><r>&a;</r>";
        assert_eq!(expansion(t, 0), None);
        assert_eq!(check(t), Ok(()));
        let mut chain = String::from("<!DOCTYPE r [<!ENTITY e0 \"x\">");
        for i in 1..100_000 {
            chain.push_str(&format!("<!ENTITY e{i} \"&e{};\">", i - 1));
        }
        chain.push_str("]><r>&e99999;</r>");
        assert_eq!(expansion(&chain, MAX_EXPANSION_BYTES), Some(u64::MAX));
        assert!(check(&chain).is_err());
        // Just inside the cap, a chain is counted exactly.
        let mut short = String::from("<!DOCTYPE r [<!ENTITY e0 \"x\">");
        for i in 1..MAX_ENTITY_NESTING {
            short.push_str(&format!("<!ENTITY e{i} \"&e{};\">", i - 1));
        }
        short.push_str(&format!("]><r>&e{};</r>", MAX_ENTITY_NESTING - 1));
        assert_eq!(expansion(&short, 0), Some(1));
    }

    /// What a naive scan would get wrong: a `]` or `>` inside an entity
    /// value does not end the subset, a commented-out declaration declares
    /// nothing, parameter entities and character references are not general
    /// entities, and the first declaration of a name wins.
    #[test]
    fn reads_the_subset_the_way_the_parser_does() {
        let t = concat!(
            "<!DOCTYPE r SYSTEM \"x]>y\" [<!ENTITY a \"]>]>\"><!-- <!ENTITY c \"cccc\"> -->",
            "<!ENTITY % p \"pppppp\"><!ENTITY a \"longer than the first\">]>",
            "<r>&a;&c;&#x41;&amp;</r>"
        );
        assert_eq!(expansion(t, 3), Some(4));
        assert_eq!(expansion(t, 4), None);
    }

    /// No internal subset, no cost: the common EPUB DOCTYPEs are never read.
    #[test]
    fn a_plain_doctype_is_not_scanned() {
        assert_eq!(expansion("<!DOCTYPE html><html>&a;</html>", 0), None);
    }

    /// The three shapes that walked past 0.17.1's scan to 4.8 GB. Each one
    /// hid the internal subset from it: a quote inside a processing
    /// instruction in the subset, and a `<!DOCTYPE` quoted in a comment or a
    /// processing instruction before the real one, which the scan took for
    /// the real one.
    #[test]
    fn the_subset_cannot_be_hidden() {
        let ent = format!("<!ENTITY a \"{}\">", "A".repeat(50_000));
        let refs = "&a;".repeat(50_000);
        for (name, prefix, subset) in [
            ("pi in subset", "", "<?x don't?>"),
            ("doctype in comment", "<!-- <!DOCTYPE x> -->", ""),
            ("doctype in pi", "<?x <!DOCTYPE y> ?>", ""),
            ("comment with a quote in subset", "", "<!-- don't -->"),
        ] {
            let t = format!(
                "<?xml version=\"1.0\"?>{prefix}<!DOCTYPE html [{subset}{ent}]><r>{refs}</r>"
            );
            assert!(
                matches!(check(&t), Err(Refusal::TooExpansive(_))),
                "{name}: {:?}",
                check(&t)
            );
        }
    }
}

#[cfg(test)]
mod depth_tests {
    use super::{MAX_XML_DEPTH, Refusal, check};

    fn nested(depth: usize) -> String {
        format!("{}x{}", "<d>".repeat(depth), "</d>".repeat(depth))
    }

    fn too_deep(t: &str) -> bool {
        matches!(check(t), Err(Refusal::TooDeep(_)))
    }

    /// The guard exists to stop an abort, so the threshold itself is the
    /// contract: one under the limit parses, one over is refused.
    #[test]
    fn triggers_only_past_the_limit() {
        assert_eq!(check(&nested(MAX_XML_DEPTH)), Ok(()));
        assert!(too_deep(&nested(MAX_XML_DEPTH + 1)));
    }

    /// The measured worst case on the 65-book shelf is 24 deep. A guard
    /// that rejected real books would be a false positive on every one of
    /// them, which is worse than the bug it fixes.
    #[test]
    fn accepts_the_deepest_real_book() {
        assert_eq!(check(&nested(24)), Ok(()));
    }

    /// Self-closing and closing tags must decrement, or a long *flat*
    /// document would accumulate depth it doesn't have - the most likely
    /// shape of a false positive, since real books are wide, not deep.
    #[test]
    fn flat_documents_do_not_accumulate_depth() {
        let flat = "<r>".to_string() + &"<img/><p>t</p>".repeat(5_000) + "</r>";
        assert_eq!(check(&flat), Ok(()));
    }

    /// `<` and `>` are not markup inside comments, CDATA, processing
    /// instructions or attribute values. Miscounting either way is a bug:
    /// over-counting rejects a valid book, under-counting lets the abort
    /// back in.
    #[test]
    fn non_markup_regions_are_not_counted() {
        let deep = "<d>".repeat(1_000);
        assert_eq!(check(&format!("<r><!-- {deep} --></r>")), Ok(()));
        assert_eq!(check(&format!("<r><![CDATA[ {deep} ]]></r>")), Ok(()));
        assert_eq!(check(&format!("<r><?x {deep} ?></r>")), Ok(()));
        // `>` is legal inside an attribute value; treating it as the tag
        // end would hide the `/` and count each tag as an open.
        let attr_gt = "<r>".to_string() + &r#"<a t="x>y"/>"#.repeat(5_000) + "</r>";
        assert_eq!(check(&attr_gt), Ok(()));
        // An apostrophe in text is not a quote.
        assert_eq!(
            check(&format!("<r>{}</r>", "<p>don't</p>".repeat(5_000))),
            Ok(())
        );
    }

    /// A DOCTYPE's internal subset carries its own `>`-bearing
    /// declarations; ending the scan at the first one would leave the rest
    /// of the subset to be miscounted as elements.
    #[test]
    fn doctype_internal_subset_is_skipped() {
        let doc = format!(
            "<!DOCTYPE r [ <!ENTITY a \"x\"> <!ENTITY b \"y\"> ]><r>{}</r>",
            "<d></d>".repeat(100)
        );
        assert_eq!(check(&doc), Ok(()));
    }

    /// Depth hidden inside a comment is skipped, but depth *after* one
    /// still counts - the skip must not swallow the rest of the document.
    #[test]
    fn scanning_resumes_after_a_skipped_region() {
        let doc = format!("<r><!-- c -->{}</r>", "<d>".repeat(MAX_XML_DEPTH + 5));
        assert!(too_deep(&doc));
    }

    /// The six shapes that walked past 0.17.1's scan and aborted the
    /// process on 500,000 levels. Each hid the nesting from it: the scan read
    /// a quote or a `[` inside a processing instruction as the start of a
    /// literal, read a comment's apostrophe in the subset the same way,
    /// ignored element names starting past ASCII, and did not look inside
    /// entity values, whose markup the parser expands in place.
    #[test]
    fn the_nesting_cannot_be_hidden() {
        let deep = nested(MAX_XML_DEPTH + 1);
        let nonascii = format!(
            "{}x{}",
            "<é>".repeat(MAX_XML_DEPTH + 1),
            "</é>".repeat(MAX_XML_DEPTH + 1)
        );
        for (name, t) in [
            ("pi quote in prolog", format!("<?x don't?>{deep}")),
            ("pi bracket in prolog", format!("<?x [?>{deep}")),
            ("pi quote in body", format!("<r><?x it's?>{deep}</r>")),
            (
                "comment quote in subset",
                format!("<!DOCTYPE r [<!-- don't -->]>{deep}"),
            ),
            ("non-ascii name", nonascii),
            (
                "markup in an entity",
                format!("<!DOCTYPE r [<!ENTITY a \"{deep}\">]><r>&a;</r>"),
            ),
        ] {
            assert!(too_deep(&t), "{name}: {:?}", check(&t));
        }
    }

    /// The attribute limit is per element, counts namespace declarations,
    /// reaches into entity values, and is not fooled by quotes in text or by
    /// a `>` inside a value.
    #[test]
    fn attributes_are_limited_per_element() {
        use super::{MAX_ATTRIBUTES, Refusal};
        let attrs = |n: usize, prefix: &str| -> String {
            (0..n).map(|i| format!(" {prefix}{i}=\"a>b\"")).collect()
        };
        let el = |n: usize, prefix: &str| format!("<p{}>don't</p>", attrs(n, prefix));
        assert_eq!(
            check(&format!("<r>{}</r>", el(MAX_ATTRIBUTES, "a"))),
            Ok(())
        );
        // Many elements each at the limit are fine: the cost is per element.
        assert_eq!(
            check(&format!("<r>{}</r>", el(MAX_ATTRIBUTES, "a").repeat(50))),
            Ok(())
        );
        for (name, t) in [
            ("plain", format!("<r>{}</r>", el(MAX_ATTRIBUTES + 1, "a"))),
            (
                "namespaces",
                format!("<r>{}</r>", el(MAX_ATTRIBUTES + 1, "xmlns:n")),
            ),
            (
                "self-closing root",
                format!("<r{}/>", attrs(MAX_ATTRIBUTES + 1, "a")),
            ),
        ] {
            assert_eq!(
                check(&t),
                Err(Refusal::TooManyAttributes(MAX_ATTRIBUTES + 1)),
                "{name}"
            );
        }
        let attrs: String = (0..=MAX_ATTRIBUTES).map(|i| format!(" a{i}='x'")).collect();
        let t = format!("<!DOCTYPE r [<!ENTITY e \"<p{attrs}/>\">]><r>&e;</r>");
        assert!(matches!(check(&t), Err(Refusal::TooManyAttributes(_))));
    }

    /// The element limit counts every start tag, self-closing ones included,
    /// and charges an entity's elements once per reference, since the parser
    /// expands each reference in place.
    #[test]
    fn elements_are_limited_per_document() {
        use super::{MAX_ELEMENTS, Refusal};
        // The root is one element, so MAX_ELEMENTS - 1 children fill it.
        let at = format!("<r>{}</r>", "<b/>".repeat(MAX_ELEMENTS - 1));
        assert_eq!(check(&at), Ok(()));
        let over = format!("<r>{}</r>", "<b/>".repeat(MAX_ELEMENTS));
        assert!(matches!(check(&over), Err(Refusal::TooManyElements(_))));
        // Closing tags, comments and CDATA are not elements.
        let flat = format!(
            "<r>{}</r>",
            "<p>x</p><!-- <b/> --><![CDATA[<b/>]]>".repeat(1000)
        );
        assert_eq!(check(&flat), Ok(()));
        // 1,000 elements in an entity, referenced 1,000 times: a small file,
        // but the parser builds a million and one elements from it.
        let refs = |n: usize| {
            format!(
                "<!DOCTYPE r [<!ENTITY e \"{}\">]><r>{}</r>",
                "<b/>".repeat(1_000),
                "&e;".repeat(n)
            )
        };
        assert_eq!(check(&refs(999)), Ok(()), "1 + 999 x 1,000");
        assert_eq!(
            check(&refs(1_000)),
            Err(Refusal::TooManyElements(1_000_001))
        );
    }

    /// Entity depth adds to the depth at the point of reference, and nests
    /// through entities that reference entities.
    #[test]
    fn entity_depth_adds_where_it_is_referenced() {
        let half = MAX_XML_DEPTH / 2;
        let inner = nested(half);
        let t = format!(
            "<!DOCTYPE r [<!ENTITY a \"{inner}\"><!ENTITY b \"<e>&a;</e>\">]>{}&b;{}",
            "<d>".repeat(half),
            "</d>".repeat(half)
        );
        // half + 1 + half = MAX + 1.
        assert!(too_deep(&t));
        let ok = format!("<!DOCTYPE r [<!ENTITY a \"{inner}\">]><r>&a;&a;&a;</r>");
        assert_eq!(check(&ok), Ok(()));
    }
}
