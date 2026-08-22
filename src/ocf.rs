//! OCF (Open Container Format) layer: the EPUB ZIP, the `mimetype` rules,
//! and locating the OPF package document via `META-INF/container.xml`.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;

/// The deepest element nesting [`parse_xml`] will accept.
///
/// roxmltree's tokenizer is mutually recursive (`parse_element` ↔
/// `parse_content`), so nesting costs stack in proportion to depth and a
/// deeply-nested document aborts the process. In Rust a stack overflow is
/// `SIGABRT`, **not** a catchable panic - `catch_unwind` cannot save an
/// embedder, so this has to be refused before the parser sees it. Measured
/// on 0.8.6: ~15,000 deep aborts on the 8 MiB main thread and ~4,000 on a
/// 2 MiB spawned thread (what an embedder or worker actually runs), from a
/// 1.1 KB file; wasm's smaller stack is lower again.
///
/// 256 comes from data, not taste: across the 65-book local shelf the
/// deepest document nests **24** elements (median 8, p95 11). That leaves
/// this ~10x above the worst real book and ~15x below the lowest measured
/// crash threshold, so no real book can reach it and no hostile one can
/// reach the abort.
pub(crate) const MAX_XML_DEPTH: usize = 256;

/// Why [`parse_xml`] refused a document.
///
/// A thin wrapper over `roxmltree::Error` so the depth guard can report its
/// own reason instead of borrowing an unrelated parser variant. It exists
/// only because the guard runs *before* the parse - every call site either
/// formats this with `{e}` or hands it to [`is_entity_reference_error`] /
/// `Position::of_parse_error`, which is why adding it touched three
/// signatures rather than twenty call sites.
#[derive(Debug)]
pub(crate) enum XmlError {
    Parse(roxmltree::Error),
    /// Nesting exceeded [`MAX_XML_DEPTH`]; carries the depth reached at the
    /// point the scan gave up (a lower bound, since it stops early).
    TooDeep(usize),
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlError::Parse(e) => write!(f, "{e}"),
            XmlError::TooDeep(d) => write!(
                f,
                "element nesting is deeper than the {MAX_XML_DEPTH}-element limit \
                 (reached {d}), so the document was not parsed"
            ),
        }
    }
}

impl XmlError {
    /// The source position to report, mirroring `roxmltree::Error::pos`.
    /// The depth guard scans bytes rather than tracking rows, so it points
    /// at the start of the document - the file name is the actionable part.
    pub(crate) fn pos(&self) -> roxmltree::TextPos {
        match self {
            XmlError::Parse(e) => e.pos(),
            XmlError::TooDeep(_) => roxmltree::TextPos::new(1, 1),
        }
    }
}

/// Length of a `<!-- -->`-style region starting at `rest[0]`, measured from
/// `from` and including `end`; the whole remainder when unterminated.
fn region_len(rest: &[u8], from: usize, end: &[u8]) -> usize {
    let start = from.min(rest.len());
    match rest[start..].windows(end.len()).position(|w| w == end) {
        Some(p) => start + p + end.len(),
        None => rest.len(),
    }
}

/// Length of a `<?…?>` PI or a `<!…>` declaration. Tracks `[`/`]` so a
/// DOCTYPE's internal subset (which contains its own `>`-bearing
/// declarations) doesn't end the scan early.
fn decl_len(rest: &[u8]) -> usize {
    let (mut j, mut quote, mut brackets) = (2usize, 0u8, 0usize);
    while j < rest.len() {
        let c = rest[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'[' {
            brackets += 1;
        } else if c == b']' {
            brackets = brackets.saturating_sub(1);
        } else if c == b'>' && brackets == 0 {
            return j + 1;
        }
        j += 1;
    }
    rest.len()
}

/// Length of an element tag, and whether it is self-closing. Quote-aware,
/// because `>` is legal inside an attribute value (`<a title="a>b">`) and
/// treating it as the tag end would miss the `/` of a self-closing tag and
/// over-count depth.
fn tag_len(rest: &[u8]) -> (usize, bool) {
    let (mut j, mut quote, mut last) = (1usize, 0u8, 0u8);
    while j < rest.len() {
        let c = rest[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return (j + 1, last == b'/');
        }
        if !c.is_ascii_whitespace() {
            last = c;
        }
        j += 1;
    }
    (rest.len(), false)
}

/// The nesting depth reached if it exceeds `limit`, else `None`.
///
/// Deliberately a raw-byte scan rather than a parse: the entire point is to
/// run *before* roxmltree's recursive tokenizer touches the text. It skips
/// the three regions where `<`/`>` are not markup - comments, CDATA
/// sections and quoted attribute values - so it can neither over-count a
/// legitimate document into a false positive nor be walked past by nesting
/// hidden inside a comment. Malformed input is not its problem: anything it
/// misreads is still handed to the parser, which reports it properly.
fn depth_exceeding(text: &str, limit: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (mut i, mut depth) = (0usize, 0usize);
    while i < b.len() {
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
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            i += decl_len(rest);
            continue;
        }
        let Some(&c) = b.get(i + 1) else { break };
        let closing = c == b'/';
        // A `<` that starts no name is stray text, not a tag - the parser
        // owns that verdict, so just step over it.
        if !closing && !(c.is_ascii_alphabetic() || c == b'_' || c == b':') {
            i += 1;
            continue;
        }
        let (len, self_closing) = tag_len(rest);
        if closing {
            depth = depth.saturating_sub(1);
        } else if !self_closing {
            depth += 1;
            if depth > limit {
                return Some(depth);
            }
        }
        i += len;
    }
    None
}

/// Parse an XML document, allowing a `<!DOCTYPE>` declaration. Real-world
/// EPUB content documents commonly have one (e.g. `<!DOCTYPE html>`);
/// roxmltree rejects any DTD by default as an extra security precaution, but
/// it already has its own billion-laughs protection regardless of this flag,
/// so allowing (harmless, common) DOCTYPEs through is safe.
///
/// Nesting depth is *not* something roxmltree can be asked to bound - its
/// `nodes_limit` counts total nodes, and a real book has 100k+ of them at
/// depth 20, so no setting of it separates a deep document from a large
/// one. Hence the explicit pre-parse guard; see [`MAX_XML_DEPTH`].
pub(crate) fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, XmlError> {
    if let Some(d) = depth_exceeding(text, MAX_XML_DEPTH) {
        return Err(XmlError::TooDeep(d));
    }
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(text, opts).map_err(XmlError::Parse)
}

/// Whether a `roxmltree` parse error is an entity-reference problem — an
/// undeclared named entity (`&nbsp;` with no DTD declaration), a malformed
/// one (missing `;`), or a malformed *numeric* character reference (`&#0;`,
/// `&#zz;`), which the parser puts in the same two variants.
///
/// This is only half of a suppression, never the whole of it. When
/// `htm::check_raw`'s raw-text scanner (`check_entities`) has already
/// reported the defect, re-reporting the parse failure would double up two
/// Fatals on the one defect; when it has *not*, suppressing here would drop
/// the document from every remaining check and let a book with real errors
/// validate clean. So the caller pairs this with a check that a finding was
/// actually produced — see the content-document loop in `opf.rs`. Widening
/// this predicate without that pairing is how the silence keeps coming back.
///
/// Any other parse failure (mismatched/unclosed tags, stray `<`, …) is a
/// genuine well-formedness error nothing else catches.
pub(crate) fn is_entity_reference_error(err: &XmlError) -> bool {
    matches!(
        err,
        XmlError::Parse(
            roxmltree::Error::UnknownEntityReference(..)
                | roxmltree::Error::MalformedEntityReference(..)
        )
    )
}

/// HTML's void elements: the ones an author writes unclosed out of HTML habit
/// and which XHTML requires be self-closed. `<hr>` instead of `<hr/>` is the
/// single most common way a real EPUB stops being well-formed XML.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Our own wording for the "unexpected close tag" parse error, replacing
/// roxmltree's `Display`. `None` for every other error, where the library's
/// wording stands.
///
/// **The library's phrasing inverts the problem.** `expected 'p' tag, not
/// 'body' at 12:1` calls neither of them an *end* tag, so it reads as though a
/// `<p>` element belonged where `<body>` appeared — the opposite of what
/// happened, which is that `</body>` arrived while `</p>` was still owed. It
/// also quotes names in a style nothing else here uses, so the sentence
/// changed hands mid-way once the opener clause below was appended to it.
/// Doitsu reported the pair on MobileRead (#177), against epubcheck's `The
/// element type "p" must be terminated by the matching end-tag "</p>"`.
///
/// **The reported position is the wrong end of the problem**, and that is the
/// other half. roxmltree and epubcheck both point at the close tag that could
/// not match — line 157, `</body>` — when what the author has to fix is the
/// `<hr>` at line 81. Reported by JSWolf on MobileRead (#174) with a book
/// whose single unterminated `<hr>` produced 74 identical errors from
/// epubcheck and one from us; ours named the right defect and still sent the
/// reader to the wrong line. The position stays where both tools put it (see
/// the call site) and the opening line is named in the text instead.
///
/// Walks the source counting starts and ends of that one element name, so a
/// legitimately nested pair earlier in the file cannot be mistaken for the
/// culprit; the unmatched one left on the stack is it. The opener clause is
/// dropped rather than guessed at when the scan cannot find one.
pub(crate) fn unterminated_element_message(text: &str, err: &XmlError) -> Option<String> {
    let XmlError::Parse(roxmltree::Error::UnexpectedCloseTag(expected, actual, _)) = err else {
        return None;
    };
    // The local name, since that is what the source text carries a prefix on.
    let name = expected.rsplit(':').next().unwrap_or(expected);
    if name.is_empty() {
        return None;
    }
    let mut open_lines: Vec<usize> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find('<') {
            let after = &rest[at + 1..];
            let (is_close, tail) = match after.strip_prefix('/') {
                Some(t) => (true, t),
                None => (false, after),
            };
            if let Some(t) = tail.strip_prefix(name)
                && t.chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
            {
                if is_close {
                    open_lines.pop();
                } else {
                    // Self-closing (`<hr/>`) opens nothing. Look ahead to this
                    // tag's `>` and check the character before it.
                    let self_closed = t
                        .find('>')
                        .is_some_and(|end| t[..end].trim_end().ends_with('/'));
                    if !self_closed {
                        open_lines.push(i + 1);
                    }
                }
            }
            rest = &rest[at + 1..];
        }
    }
    // Names are printed as the source spells them, prefix and all, so the
    // reader can search for the text they wrote.
    let opener = match open_lines.last() {
        Some(line) => format!("unclosed <{expected}> at line {line}; "),
        None => String::new(),
    };
    // For a void element the close tag is a red herring: the author did not
    // forget `</hr>`, they wrote HTML in an XHTML document, and there is
    // exactly one fix. Everywhere else the mismatched pair *is* the diagnosis.
    let problem = if VOID_ELEMENTS.contains(&name) {
        format!("XHTML requires <{expected}/>")
    } else {
        format!("expected </{expected}> but found </{actual}>")
    };
    Some(format!("{opener}{problem}"))
}

/// What to print after `… is not well-formed XML:` — ours for the one error
/// whose library wording misleads, the library's for the rest.
///
/// Every file we parse goes through here rather than through `{e}`, because
/// an unclosed element is not a content-document defect: it happens in an OPF,
/// an NCX or a `container.xml` just as readily, and a reader who met the clear
/// sentence once should not meet the backwards one next time.
pub(crate) fn parse_error_detail(text: &str, err: &XmlError) -> String {
    unterminated_element_message(text, err).unwrap_or_else(|| err.to_string())
}

/// Manually scans the ZIP's raw bytes for a genuine exact-duplicate entry
/// name in the central directory - the `zip` crate's own reader can't
/// expose this (see the call site's comment), so this walks the End-Of-
/// Central-Directory record and each fixed-size (46-byte header) central-
/// directory record by hand, extracting just the raw name bytes. Returns
/// the first duplicate name found, or `None` if the structure can't be
/// located/parsed (never treated as an error itself - `ZipArchive` already
/// owns "is this a valid ZIP" reporting).
fn find_exact_duplicate_entry(bytes: &[u8]) -> Option<String> {
    const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
    const CD_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
    // The EOCD record is 22 bytes plus up to a 65535-byte comment; search
    // backward from the end within that window.
    let search_start = bytes.len().saturating_sub(22 + 65535);
    let eocd_pos = bytes[search_start..]
        .windows(4)
        .rposition(|w| w == EOCD_SIG)
        .map(|p| search_start + p)?;
    if bytes.len() < eocd_pos + 22 {
        return None;
    }
    let cd_count =
        u16::from_le_bytes(bytes[eocd_pos + 10..eocd_pos + 12].try_into().ok()?) as usize;
    let cd_offset =
        u32::from_le_bytes(bytes[eocd_pos + 16..eocd_pos + 20].try_into().ok()?) as usize;

    let mut seen: HashSet<String> = HashSet::new();
    let mut pos = cd_offset;
    for _ in 0..cd_count {
        if bytes.len() < pos + 46 || bytes[pos..pos + 4] != CD_SIG {
            break;
        }
        let name_len = u16::from_le_bytes(bytes[pos + 28..pos + 30].try_into().ok()?) as usize;
        let extra_len = u16::from_le_bytes(bytes[pos + 30..pos + 32].try_into().ok()?) as usize;
        let comment_len = u16::from_le_bytes(bytes[pos + 32..pos + 34].try_into().ok()?) as usize;
        let name_start = pos + 46;
        if bytes.len() < name_start + name_len {
            break;
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_start + name_len]).into_owned();
        if !seen.insert(name.clone()) {
            return Some(name);
        }
        pos = name_start + name_len + extra_len + comment_len;
    }
    None
}

/// An opened EPUB container.
pub struct Ocf {
    archive: ZipArchive<Cursor<Vec<u8>>>,
    /// All entry names, in archive order.
    pub names: Vec<String>,
    /// Entries [`Ocf::read`] refused for exceeding [`MAX_ENTRY_BYTES`],
    /// drained by [`check_resource_limits`] at the end of the run.
    oversized: Vec<String>,
}

/// The most bytes [`Ocf::read`] will materialise for one container entry.
///
/// A bare `read_to_end` on a compressed entry is bounded by the *inflated*
/// size, not the file's: deflate reaches ~1000:1, so a 400 KB EPUB drove
/// 1.3 GB of peak RSS here and still reported VALID. Capping the read
/// bounds that regardless of what the entry's header claims, so a header
/// lying about its uncompressed size cannot widen it either - which is why
/// the cap lives on the read rather than on the central directory.
///
/// 64 MiB against a measured worst case of **2.1 MB** (largest single entry
/// across the 65-book local shelf; the largest whole book inflates to
/// 82 MB), so real books have ~30x headroom on their biggest resource.
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

impl Ocf {
    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Read a member's bytes (decompressing as needed), up to
    /// [`MAX_ENTRY_BYTES`].
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let f = self.archive.by_name(name).ok()?;
        let mut buf = Vec::new();
        // One byte past the cap: reading *more* than it is what separates an
        // oversized entry from one that happens to end exactly on it.
        f.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).ok()?;
        if buf.len() as u64 > MAX_ENTRY_BYTES {
            // Never a silent skip. Returning a bare `None` here would be
            // indistinguishable from "resource absent" at all ~25 call
            // sites, and a book with real errors would report clean - the
            // exact failure the 0.7.12-0.7.14 audit kept finding. The name
            // is recorded instead and reported once, by name, at the end.
            if !self.oversized.iter().any(|n| n == name) {
                self.oversized.push(name.to_string());
            }
            return None;
        }
        Some(buf)
    }
}

/// Report every entry [`Ocf::read`] refused for exceeding
/// [`MAX_ENTRY_BYTES`]. Called once per run, after the checks, so a
/// resource read from several places is still reported once.
pub(crate) fn check_resource_limits(ocf: &Ocf, report: &mut Report) {
    for name in &ocf.oversized {
        report.push_at(
            LIM_001,
            Severity::Error,
            format!(
                "resource exceeds the {} MiB size limit and was not checked",
                MAX_ENTRY_BYTES / (1024 * 1024)
            ),
            name.clone(),
        );
    }
}

/// Open the ZIP and run the OCF `mimetype` checks (PKG-003/005/006/007/008).
/// Returns `None` only when the bytes are not a usable ZIP at all - a
/// fatal condition, so nothing else is checked (confirmed via real
/// fixtures pairing PKG-008 alone with "no other errors or warnings").
pub fn open(bytes: Vec<u8>, report: &mut Report) -> Option<Ocf> {
    // PKG-003: epubcheck's `OCFZipChecker` reads a **58-byte** header and
    // reports this when the file cannot fill it - so it covers any container
    // shorter than that, not only an empty one, which is all we had (#80).
    // 58 rather than 30 because the check reaches past the local file header
    // to the `mimetype` name at offset 30; an earlier comment in this file
    // said 30 and that misreading is what made the two tools look
    // inconsistent when they were not.
    if bytes.len() < 58 {
        let n = bytes.len();
        report.push(
            PKG_003,
            Severity::Error,
            if n == 0 {
                "the EPUB file is empty".to_string()
            } else {
                format!("the EPUB file is too short to hold a ZIP header ({n} bytes)")
            },
        );
    }
    if bytes.is_empty() {
        report.push_rule(
            PKG_008,
            Severity::Fatal,
            "the zip file is empty",
            "ocf.container.empty_zip",
            Vec::new(),
        );
        return None;
    }
    // If the bytes are actually a recognizable *other* file format (e.g. a
    // plain image mistakenly given an `.epub` extension), that's a more
    // specific defect (a corrupted/wrong ZIP header) than a merely
    // truncated or otherwise-malformed ZIP - confirmed via a real fixture
    // expecting *both* PKG-004 and PKG-008 together, vs. others (a
    // generic truncated archive with no recognizable format) expecting
    // PKG-008 alone.
    let looks_like_other_format = crate::image::sniff_image_type(&bytes).is_some();
    // The `zip` crate's own `ZipArchive` de-duplicates entries sharing a
    // name into a single slot internally (a later central-directory record
    // silently overwrites an earlier one in its IndexMap), so its own
    // entry list can never expose a genuine duplicate-name defect -
    // confirmed via a real fixture with 6 central-directory records but
    // `ZipArchive::len()` reporting only 5. Scanned manually here, on the
    // raw bytes, before they're moved into the archive reader.
    let duplicate_entry_name = find_exact_duplicate_entry(&bytes);
    // PKG-005 reads the **local file header**, not the central directory.
    // The two carry independent extra fields, and tools routinely put an
    // NTFS timestamp in the central directory only: one shelf book has 0
    // bytes local and 36 central, so `extra_data()` (central) made us report
    // an error epubcheck does not - it reads the file's own leading bytes
    // (`OCFZipChecker`, a 58-byte header), and so do we now. mimetype must be the first entry,
    // which is checked separately, so offset 0 is its local header.
    let first_local_extra_len = if bytes.len() >= 30 && bytes.starts_with(b"PK\x03\x04") {
        u16::from_le_bytes([bytes[28], bytes[29]])
    } else {
        0
    };
    // Read before `bytes` is moved into the reader below.
    let bytes_len = bytes.len();
    let bytes_first_two = (
        bytes.first().copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
    );
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => {
            // PKG-004 is epubcheck's "the header is long enough but does not
            // start with `PK`" - `header[0] != 'P' && header[1] != 'K'` in
            // `OCFZipChecker`, an **and**, so one matching byte is enough to
            // fall through. We guarded it on an image sniff instead, so 200
            // random bytes drew PKG-008 alone where epubcheck names the
            // defect (#80). The sniff stays as a second route: a file that
            // *is* a recognisable other format still gets it even if its
            // first two bytes happen to be `PK`.
            let bad_signature =
                bytes_len >= 58 && bytes_first_two.0 != b'P' && bytes_first_two.1 != b'K';
            if bad_signature || looks_like_other_format {
                report.push(
                    PKG_004,
                    Severity::Fatal,
                    "Not a valid EPUB container (corrupted ZIP header)",
                );
            }
            report.push_rule(
                PKG_008,
                Severity::Fatal,
                format!("could not open zip file: {e}"),
                "ocf.container.unreadable_zip",
                vec![e.to_string()],
            );
            return None;
        }
    };

    if let Some(name) = duplicate_entry_name {
        report.push_at_rule(
            OPF_060,
            Severity::Error,
            format!("file name '{name}' is used by more than one ZIP entry"),
            name.as_str(),
            "ocf.container.exact_duplicate_entry",
            vec![name.clone()],
        );
    }

    let n = archive.len();
    let mut names = Vec::with_capacity(n);
    let mut first_name = String::new();
    let mut first_stored = false;
    let mut first_has_extra = false;
    for i in 0..n {
        if let Ok(f) = archive.by_index(i) {
            // EPUB requires every ZIP entry's file name to be UTF-8 - a
            // genuinely non-UTF-8 name (confirmed via a real fixture using
            // raw CP437-encoded bytes) makes the archive unusable enough
            // that real epubcheck stops entirely (fatal, no other findings).
            if std::str::from_utf8(f.name_raw()).is_err() {
                report.push(
                    PKG_027,
                    Severity::Fatal,
                    "a ZIP entry's file name is not valid UTF-8",
                );
                return None;
            }
            let name = f.name().to_string();
            if i == 0 {
                first_name = name.clone();
                first_stored = f.compression() == zip::CompressionMethod::Stored;
                first_has_extra = first_local_extra_len != 0;
            }
            // PKG-009/011/012: checked per path *segment* (a directory
            // name is a file name too), on every real entry regardless of
            // whether it's a "publication resource" (confirmed via a real
            // fixture flagging a forbidden character inside META-INF).
            for segment in name.split('/').filter(|s| !s.is_empty()) {
                if crate::filename::has_forbidden_char(segment) {
                    report.push_at_rule(
                        PKG_009,
                        Severity::Error,
                        format!("file name '{segment}' contains a forbidden character"),
                        name.as_str(),
                        "ocf.filename.forbidden_char",
                        vec![segment.to_string()],
                    );
                }
                if crate::filename::ends_with_full_stop(segment) {
                    report.push_at(
                        PKG_011,
                        Severity::Error,
                        format!("file name '{segment}' must not end with a full stop"),
                        name.as_str(),
                    );
                }
                if crate::filename::has_non_ascii(segment) {
                    report.push_at_rule(
                        PKG_012,
                        Severity::Usage,
                        format!("file name '{segment}' contains non-ASCII characters"),
                        name.as_str(),
                        "ocf.filename.non_ascii",
                        vec![segment.to_string()],
                    );
                }
            }
            names.push(name);
        }
    }

    // OPF-060: two distinct entry names that collide once case-folded and
    // NFC-normalized (exact duplicates, common/full case-folding, or
    // Unicode canonical-normalization equivalence all confirmed via
    // dedicated real fixtures; mere *compatibility*-normalization
    // equivalence is deliberately not flagged, per another).
    {
        let mut seen: HashMap<String, &str> = HashMap::new();
        let mut reported = HashSet::new();
        for name in &names {
            let key = crate::filename::canonical_fold_key(name);
            if let Some(_first) = seen.get(key.as_str()) {
                if reported.insert(name.clone()) {
                    report.push_at_rule(
                        OPF_060,
                        Severity::Error,
                        format!("file name '{name}' collides with another entry after case-folding/normalization"),
                        name.as_str(),
                        "ocf.container.case_fold_collision",
                        vec![name.clone()],
                    );
                }
            } else {
                seen.insert(key, name.as_str());
            }
        }
    }

    // PKG-014: a directory entry (name ends with '/') with no other entry
    // nested inside it is an empty directory - confirmed via a real
    // fixture.
    for name in &names {
        if !name.ends_with('/') {
            continue;
        }
        if !names
            .iter()
            .any(|other| other != name && other.starts_with(name.as_str()))
        {
            report.push_at(
                PKG_014,
                Severity::Warning,
                format!("'{name}' is an empty directory"),
                name.as_str(),
            );
        }
    }

    // mimetype: must be the first entry, stored (uncompressed), exact
    // contents, and its ZIP header must carry no extra field (needed so
    // tools can sniff the media type at a fixed byte offset without a
    // full ZIP parse).
    if first_name != "mimetype" {
        report.push_rule(
            PKG_006,
            Severity::Error,
            "The 'mimetype' file must be the first entry in the EPUB ZIP",
            "ocf.mimetype.not_first_entry",
            Vec::new(),
        );
    } else {
        if !first_stored {
            report.push_rule(
                PKG_007,
                Severity::Error,
                "The 'mimetype' file must be stored uncompressed",
                "ocf.mimetype.not_stored_uncompressed",
                Vec::new(),
            );
        }
        if first_has_extra {
            report.push(
                PKG_005,
                Severity::Error,
                "The 'mimetype' entry's ZIP header must not have an extra field",
            );
        }
    }

    let mut ocf = Ocf {
        archive,
        names,
        oversized: Vec::new(),
    };

    if ocf.has("mimetype")
        && let Some(b) = ocf.read("mimetype")
        && b != b"application/epub+zip"
    {
        report.push_rule(
            PKG_007,
            Severity::Error,
            format!(
                "'mimetype' must contain exactly 'application/epub+zip' (found {:?})",
                String::from_utf8_lossy(&b)
            ),
            "ocf.mimetype.wrong_content",
            vec![String::from_utf8_lossy(&b).into_owned()],
        );
    }

    Some(ocf)
}

/// Parse `META-INF/container.xml` and return every OPF full-path
/// (RSC-002/003/005). Usually a single rootfile, but a multi-rendition
/// package (e.g. an EDUPUB publication with a reflowable + a fixed-layout
/// rendition) legitimately declares more than one — each is validated as
/// its own, independent OPF.
pub fn find_rootfiles(ocf: &mut Ocf, report: &mut Report) -> Vec<String> {
    const CONTAINER: &str = "META-INF/container.xml";
    if !ocf.has(CONTAINER) {
        report.push(
            RSC_002,
            Severity::Fatal,
            "Required META-INF/container.xml is missing",
        );
        return Vec::new();
    }

    let Some(bytes) = ocf.read(CONTAINER) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                format!(
                    "META-INF/container.xml is not well-formed XML: {}",
                    parse_error_detail(&text, &e)
                ),
                CONTAINER,
                Position::of_parse_error(&e),
            );
            return Vec::new();
        }
    };

    // OPF-016/017: a `<rootfile>` that cannot say where its package document
    // is. Reported on *every* rootfile, whatever its media type - epubcheck's
    // handler runs before it looks at the type, so a mistyped rootfile still
    // gets told its full-path is missing rather than being passed over in
    // silence. Both cases then drop out of `paths` below, as they must: there
    // is no path to follow.
    for rf in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "rootfile")
    {
        match rf.attr_no_ns("full-path") {
            None => report.push_node(
                OPF_016,
                Severity::Error,
                "element \"rootfile\" is missing required attribute \"full-path\"",
                CONTAINER,
                rf,
                "ocf.rootfile.missing_full_path",
                vec![],
            ),
            // Whitespace-only counts as empty, matching epubcheck's
            // `fullPath.trim().isEmpty()`.
            Some(p) if p.trim().is_empty() => report.push_node(
                OPF_017,
                Severity::Error,
                "attribute \"full-path\" on element \"rootfile\" must not be empty",
                CONTAINER,
                rf,
                "ocf.rootfile.empty_full_path",
                vec![],
            ),
            Some(_) => {}
        }
    }

    // RSC-003: need at least one <rootfile> with the OPF media type and a full-path.
    let paths: Vec<String> = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "rootfile"
                && n.attr_no_ns("media-type") == Some("application/oebps-package+xml")
        })
        .filter_map(|n| n.attr_no_ns("full-path"))
        .filter(|p| !p.trim().is_empty())
        .map(String::from)
        .collect();

    if paths.is_empty() {
        report.push_at_pos(
            RSC_003,
            Severity::Error,
            "container.xml has no <rootfile> with media-type \
             'application/oebps-package+xml' and a full-path (OPF location)",
            CONTAINER,
            Position::of(doc.root_element()),
        );
    }

    // `<container>`'s only real children are `<rootfiles>` and `<links>`
    // (the Rendition Mapping Document reference) - any other direct child
    // (confirmed via a real fixture, a stray `<foo/>`) is a content-model
    // violation.
    for child in doc.root_element().children().filter(|n| n.is_element()) {
        if !matches!(child.tag_name().name(), "rootfiles" | "links") {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("element \"{}\" not allowed here", child.tag_name().name()),
                CONTAINER,
                child,
                "ocf.container.unexpected_child",
                vec![child.tag_name().name().to_string()],
            );
        }
    }

    paths
}

/// If `META-INF/encryption.xml` is present, report each encrypted resource as
/// RSC-004 (INFO) — its content is not validated. Also checks the file's
/// own content model: root element name, `Id`-attribute uniqueness, and
/// (IDPF compression extension) `Compression` attribute validity.
pub fn check_encryption(ocf: &mut Ocf, report: &mut Report) {
    const ENC: &str = "META-INF/encryption.xml";
    if !ocf.has(ENC) {
        return;
    }
    let Some(bytes) = ocf.read(ENC) else { return };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                format!(
                    "META-INF/encryption.xml is not well-formed XML: {}",
                    parse_error_detail(&text, &e)
                ),
                ENC,
                Position::of_parse_error(&e),
            );
            return;
        }
    };

    if doc.root_element().tag_name().name() != "encryption" {
        report.push_node(
            RSC_005,
            Severity::Error,
            "expected element \"encryption\" as the root of META-INF/encryption.xml",
            ENC,
            doc.root_element(),
            "ocf.encryption.wrong_root_element",
            Vec::new(),
        );
        return;
    }

    check_encryption_children(doc.root_element(), report);

    // Every `Id` attribute (on any element - EncryptedKey/EncryptedData/...)
    // must be unique across the whole document; a value shared by more
    // than one element is reported once *per element* sharing it
    // (confirmed via a real 2-element-sharing-1-id fixture expecting
    // exactly 2 findings, not 1).
    let mut by_id: HashMap<&str, u32> = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("Id") {
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attr_no_ns("Id")
            && by_id.get(id).copied().unwrap_or(0) > 1
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                format!("Duplicate \"Id\" value '{id}'"),
                ENC,
                n,
                "ocf.encryption.duplicate_id",
                vec![id.to_string()],
            );
        }
    }

    // The IDPF compression extension's <Compression Method="0|8"
    // OriginalLength="<non-negative integer>"/>.
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Compression")
    {
        if let Some(method) = n.attr_no_ns("Method")
            && !matches!(method, "0" | "8")
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                "value of attribute \"Method\" is invalid",
                ENC,
                n,
                "ocf.encryption.invalid_compression_method",
                vec![method.to_string()],
            );
        }
        if let Some(len) = n.attr_no_ns("OriginalLength")
            && (len.is_empty() || !len.bytes().all(|b| b.is_ascii_digit()))
        {
            report.push_node(
                RSC_005,
                Severity::Error,
                "value of attribute \"OriginalLength\" is invalid",
                ENC,
                n,
                "ocf.encryption.invalid_original_length",
                vec![len.to_string()],
            );
        }
    }

    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "CipherReference")
    {
        if let Some(uri) = n.attr_no_ns("URI") {
            report.push_full(
                RSC_004,
                Severity::Info,
                format!("File \"{uri}\" is encrypted; its content will not be checked"),
                ENC,
                Position::of(n),
                "ocf.resource.encrypted_not_checked",
                vec![uri.to_string()],
            );
        }
    }
}

/// The content model of `<encryption>`: one or more `EncryptedData` or
/// `EncryptedKey` children from the XML Encryption namespace, and nothing else.
///
/// epubcheck's `ocf-encryption-30.rnc` wraps the whole xmlenc grammar in
/// `element encryption { grammar { … }+ }` with `start = xenc_EncryptedData |
/// xenc_EncryptedKey`, which is the entire rule: the `+` is the cardinality and
/// the `start` is the vocabulary. Nothing else about xmlenc is checked here —
/// the surrounding function's scope is deliberately "presence and shape", and
/// this closes the one gap a user actually met.
///
/// Reported by JSWolf (MobileRead #221) after deleting an obfuscated font from
/// a book and leaving its now-childless `encryption.xml` behind — an editing
/// accident, not a producer bug, which is why a shelf of finished books says
/// nothing about it: only 2 of 385 carry the file at all and both are canonical.
///
/// **Both halves are needed, not just the one that was reported.** A book with
/// a foreign child draws two findings from epubcheck — the child is rejected
/// *and* the element is still incomplete — so implementing only the emptiness
/// case would leave `<encryption><foo/></encryption>` reported as complete. That
/// is the gap-between-two-checks shape this project keeps meeting: the case no
/// check owns reports nothing at all.
fn check_encryption_children(root: roxmltree::Node, report: &mut Report) {
    const XENC: &str = "http://www.w3.org/2001/04/xmlenc#";
    const EXPECTED: &str = "expected one of \"enc:EncryptedData\", \"enc:EncryptedKey\"";

    let mut encrypted_items = 0usize;
    for child in root.children().filter(|n| n.is_element()) {
        let name = child.tag_name();
        if name.namespace() == Some(XENC) && matches!(name.name(), "EncryptedData" | "EncryptedKey")
        {
            encrypted_items += 1;
            continue;
        }
        // Named with the conventional `enc:` prefix when it is in the xmlenc
        // namespace, for the same reason `qualified_attribute_name` does it in
        // the RELAX NG engine: `roxmltree` hands back the local name, and a
        // bare "EncryptedDataX" sends the reader hunting for something their
        // file does not literally contain.
        let shown = if name.namespace() == Some(XENC) {
            format!("enc:{}", name.name())
        } else {
            name.name().to_string()
        };
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("element \"{shown}\" is not allowed here; {EXPECTED}"),
            "META-INF/encryption.xml",
            child,
            "ocf.encryption.child_not_allowed",
            vec![shown.clone()],
        );
    }

    if encrypted_items == 0 {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!("element \"encryption\" has incomplete content; {EXPECTED}"),
            "META-INF/encryption.xml",
            root,
            "ocf.encryption.incomplete_content",
            Vec::new(),
        );
    }
}

/// If `META-INF/signatures.xml` is present, checks its root element name
/// (RSC-005 if not "signatures") - its actual signature content isn't
/// validated, same "presence/shape only" scope as `check_encryption`.
pub fn check_signatures(ocf: &mut Ocf, report: &mut Report) {
    const SIG: &str = "META-INF/signatures.xml";
    if !ocf.has(SIG) {
        return;
    }
    let Some(bytes) = ocf.read(SIG) else { return };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at_pos(
                RSC_005,
                Severity::Error,
                format!(
                    "META-INF/signatures.xml is not well-formed XML: {}",
                    parse_error_detail(&text, &e)
                ),
                SIG,
                Position::of_parse_error(&e),
            );
            return;
        }
    };
    if doc.root_element().tag_name().name() != "signatures" {
        report.push_node(
            RSC_005,
            Severity::Error,
            "expected element \"signatures\" as the root of META-INF/signatures.xml",
            SIG,
            doc.root_element(),
            "ocf.signatures.wrong_root_element",
            Vec::new(),
        );
    }
}

#[cfg(test)]
mod encryption_content_model_tests {
    use crate::report::Report;

    /// Runs the `<encryption>` content model over `body` and returns the
    /// `(rule, message)` pairs, in document order.
    fn findings(body: &str) -> Vec<(&'static str, String)> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">{body}</encryption>"#
        );
        let doc = crate::ocf::parse_xml(&xml).expect("fixture must parse");
        let mut report = Report::new();
        super::check_encryption_children(doc.root_element(), &mut report);
        report
            .messages
            .iter()
            .map(|m| (m.rule.expect("a rule"), m.text.clone()))
            .collect()
    }

    const ENCRYPTED_DATA: &str = r#"
  <enc:EncryptedData>
    <enc:CipherData><enc:CipherReference URI="OEBPS/f.otf"/></enc:CipherData>
  </enc:EncryptedData>"#;

    /// The control, and the half that decides whether this rule may ship at
    /// all: a real encrypted book must stay silent. Both shelf books that
    /// carry `encryption.xml` have exactly this shape.
    #[test]
    fn a_conforming_encryption_file_is_silent() {
        assert_eq!(findings(ENCRYPTED_DATA), vec![]);
    }

    /// JSWolf, MobileRead #221: an `encryption.xml` left behind after its
    /// obfuscated font was deleted. epubcheck reports
    /// `element "encryption" incomplete`; we reported nothing at all.
    #[test]
    fn an_encryption_element_with_no_encrypted_items_is_incomplete() {
        let f = findings("\n");
        assert_eq!(
            f.iter().map(|(r, _)| *r).collect::<Vec<_>>(),
            vec!["ocf.encryption.incomplete_content"]
        );
        assert!(
            f[0].1.contains("enc:EncryptedData") && f[0].1.contains("enc:EncryptedKey"),
            "the message must name what was expected; got {}",
            f[0].1
        );
    }

    /// `EncryptedKey` satisfies the model on its own - the grammar's `start`
    /// is a choice, not a sequence, so requiring `EncryptedData` would invent
    /// an error on a key-only file.
    #[test]
    fn an_encrypted_key_alone_satisfies_the_model() {
        assert_eq!(findings("<enc:EncryptedKey/>"), vec![]);
    }

    /// **Two findings, not one.** A foreign child is rejected *and* leaves the
    /// element incomplete, which is what epubcheck reports for this shape.
    /// Implementing only the emptiness half would have called
    /// `<encryption><foo/></encryption>` complete - the gap-between-two-checks
    /// shape, where the case no check owns reports nothing.
    #[test]
    fn a_foreign_child_is_rejected_and_still_leaves_the_element_incomplete() {
        // Sorted, because insertion order here is not the order a user sees:
        // `validate_bytes` re-sorts the whole report into document order, so
        // pinning the raw order would couple this test to an internal detail.
        let mut rules: Vec<_> = findings("\n  <somethingElse/>\n")
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        rules.sort_unstable();
        assert_eq!(
            rules,
            vec![
                "ocf.encryption.child_not_allowed",
                "ocf.encryption.incomplete_content",
            ],
            "both halves must fire"
        );
    }

    /// The namespace is load-bearing: `<EncryptedData>` in no namespace is a
    /// different element from `xenc:EncryptedData`, and matching on the local
    /// name alone would accept it. epubcheck's grammar names the namespace,
    /// so this is parity and not strictness for its own sake.
    #[test]
    fn an_unnamespaced_encrypted_data_does_not_satisfy_the_model() {
        let mut rules: Vec<_> = findings("<EncryptedData xmlns=\"\"/>")
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        rules.sort_unstable();
        assert_eq!(
            rules,
            vec![
                "ocf.encryption.child_not_allowed",
                "ocf.encryption.incomplete_content",
            ]
        );
    }
}

#[cfg(test)]
mod depth_guard_tests {
    use super::{MAX_XML_DEPTH, depth_exceeding};

    fn nested(depth: usize) -> String {
        format!("{}x{}", "<d>".repeat(depth), "</d>".repeat(depth))
    }

    /// The guard exists to stop an abort, so the threshold itself is the
    /// contract: one under the limit parses, one over is refused.
    #[test]
    fn triggers_only_past_the_limit() {
        assert_eq!(depth_exceeding(&nested(MAX_XML_DEPTH), MAX_XML_DEPTH), None);
        assert!(depth_exceeding(&nested(MAX_XML_DEPTH + 1), MAX_XML_DEPTH).is_some());
    }

    /// The measured worst case on the 65-book shelf is 24 deep. A guard
    /// that rejected real books would be a false positive on every one of
    /// them, which is worse than the bug it fixes.
    #[test]
    fn accepts_the_deepest_real_book() {
        assert_eq!(depth_exceeding(&nested(24), MAX_XML_DEPTH), None);
    }

    /// Self-closing and closing tags must decrement, or a long *flat*
    /// document would accumulate depth it doesn't have - the most likely
    /// shape of a false positive, since real books are wide, not deep.
    #[test]
    fn flat_documents_do_not_accumulate_depth() {
        let flat = "<r>".to_string() + &"<img/><p>t</p>".repeat(5_000) + "</r>";
        assert_eq!(depth_exceeding(&flat, MAX_XML_DEPTH), None);
    }

    /// `<` and `>` are not markup inside comments, CDATA or attribute
    /// values. Miscounting either way is a bug: over-counting rejects a
    /// valid book, under-counting lets the abort back in.
    #[test]
    fn non_markup_regions_are_not_counted() {
        let commented = format!("<r><!-- {} --></r>", "<d>".repeat(1_000));
        assert_eq!(depth_exceeding(&commented, MAX_XML_DEPTH), None);

        let cdata = format!("<r><![CDATA[ {} ]]></r>", "<d>".repeat(1_000));
        assert_eq!(depth_exceeding(&cdata, MAX_XML_DEPTH), None);

        // `>` is legal inside an attribute value; treating it as the tag
        // end would hide the `/` and count each tag as an open.
        let attr_gt = "<r>".to_string() + &r#"<a t="x>y"/>"#.repeat(5_000) + "</r>";
        assert_eq!(depth_exceeding(&attr_gt, MAX_XML_DEPTH), None);
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
        assert_eq!(depth_exceeding(&doc, MAX_XML_DEPTH), None);
    }

    /// Depth hidden inside a comment is skipped, but depth *after* one
    /// still counts - the skip must not swallow the rest of the document.
    #[test]
    fn scanning_resumes_after_a_skipped_region() {
        let doc = format!("<r><!-- c -->{}</r>", "<d>".repeat(MAX_XML_DEPTH + 5));
        assert!(depth_exceeding(&doc, MAX_XML_DEPTH).is_some());
    }
}

#[cfg(test)]
mod unterminated_tag_tests {
    use super::{parse_xml, unterminated_element_message};

    fn hint(body: &str) -> Option<String> {
        let xml = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
             <head><title>t</title></head>\n<body>\n{body}\n</body></html>"
        );
        let err = parse_xml(&xml).expect_err("must not parse");
        unterminated_element_message(&xml, &err)
    }

    /// JSWolf, MobileRead #174: one unterminated `<hr>` at line 81 drew 74
    /// identical errors from epubcheck and one from us — and both tools
    /// pointed at line 157, the `</body>` where the parse finally failed,
    /// rather than at the tag the author has to fix.
    #[test]
    fn the_message_names_the_line_the_element_was_opened_on() {
        // `<hr>` is on line 4 of the document built above.
        let h = hint("<p>a</p>\n<hr>\n<p>b</p>").expect("a message");
        assert!(h.contains("line 5"), "got {h}");
        assert!(h.contains("XHTML requires <hr/>"), "got {h}");
    }

    /// A self-closed sibling earlier in the file opens nothing, and a
    /// correctly matched pair must not be mistaken for the culprit — the two
    /// ways a naive "find the first `<hr`" would pick the wrong line.
    #[test]
    fn earlier_self_closed_and_matched_tags_are_not_the_culprit() {
        // `<hr/>` line 4, `<p>` line 5, the unclosed `<hr>` line 6.
        let h = hint("<hr/>\n<p>a</p>\n<hr>\n<p>b</p>").expect("a message");
        assert!(h.contains("line 6"), "got {h}");

        let h = hint("<div><p>a</p></div>\n<div>\n<p>b</p>").expect("a message");
        assert!(h.contains("line 5"), "got {h}");
        // Not a void element, so no XHTML advice - the fix is a close tag.
        assert!(!h.contains("XHTML requires"), "got {h}");
    }

    /// Doitsu, MobileRead #177. roxmltree renders this as `expected 'p' tag,
    /// not 'body'`, which names neither as an end tag and so reads as if a
    /// `<p>` element were wanted where `<body>` sits — backwards. Both end
    /// tags must appear as end tags, and the library's phrasing must not
    /// survive anywhere in the sentence.
    #[test]
    fn an_unexpected_close_tag_is_described_as_a_missing_end_tag() {
        // `<p>` opens on line 4 and is never closed; `</body>` is line 5.
        let h = hint("<p>Lorem ipsum<p>").expect("a message");
        assert_eq!(h, "unclosed <p> at line 4; expected </p> but found </body>");
        assert!(!h.contains("tag, not"), "library wording leaked: {h}");
    }

    /// The opener scan is best-effort, and when it finds nothing the rest of
    /// the sentence still has to stand on its own — that fallback is where
    /// the library's wording used to come back.
    #[test]
    fn the_pair_is_named_even_when_the_opening_line_cannot_be_found() {
        // A close tag for an element that was never opened at all: nothing
        // for the scan to count, so there is no line to name.
        let xml = "<html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
                   <head><title>t</title></head>\n<body></p>\n</body></html>";
        let err = parse_xml(xml).expect_err("must not parse");
        let h = unterminated_element_message(xml, &err).expect("a message");
        assert_eq!(h, "expected </body> but found </p>");
    }
}
