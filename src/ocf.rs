//! OCF (Open Container Format) layer: the EPUB ZIP, the `mimetype` rules,
//! and locating the OPF package document via `META-INF/container.xml`.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::ids::*;
use crate::report::{Position, Report, Severity};
use crate::xmlext::NodeExt;
use crate::xmlguard;

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
    /// Declined by [`xmlguard::check`] before the parser saw it.
    Refused(xmlguard::Refusal),
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlError::Parse(e) => write!(f, "{e}"),
            XmlError::Refused(r) => write!(f, "{r}"),
        }
    }
}

impl XmlError {
    /// Whether the parser ran out of input rather than meeting a bad token:
    /// a root that never closed, or a stream that ended mid-construct. Both
    /// report position 1:1 in roxmltree, so a caller looking for "how far did
    /// the parser get" must read them as the end of the text instead - the
    /// #139/#141 prefix check otherwise found nothing before 1:1 and checked
    /// nothing in a truncated document, its commonest shape.
    pub(crate) fn ran_out_of_input(&self) -> bool {
        matches!(
            self,
            XmlError::Parse(
                roxmltree::Error::UnclosedRootNode | roxmltree::Error::UnexpectedEndOfStream
            )
        )
    }

    /// The source position to report, mirroring `roxmltree::Error::pos`.
    /// The guard scans bytes rather than tracking rows, so it points at the
    /// start of the document - the file name is the actionable part.
    pub(crate) fn pos(&self) -> roxmltree::TextPos {
        match self {
            XmlError::Parse(e) => e.pos(),
            XmlError::Refused(_) => roxmltree::TextPos::new(1, 1),
        }
    }
}

/// Parse an XML document, allowing a `<!DOCTYPE>` declaration. Real-world
/// EPUB content documents commonly have one (e.g. `<!DOCTYPE html>`), and
/// roxmltree rejects any DTD by default as a security precaution. What makes
/// allowing one safe is not roxmltree alone: it stops entity *loops*, and
/// [`xmlguard::check`] bounds how far loop-free entities may inflate the
/// document. Before that guard existed this comment called DOCTYPEs safe on
/// the strength of the loop check, and a 1.6 KB book used 5 GB.
///
/// Nesting depth is *not* something roxmltree can be asked to bound - its
/// `nodes_limit` counts total nodes, and a real book has 100k+ of them at
/// depth 20, so no setting of it separates a deep document from a large
/// one. Hence the explicit pre-parse guard; see [`xmlguard::MAX_XML_DEPTH`].
pub(crate) fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, XmlError> {
    xmlguard::check(text).map_err(XmlError::Refused)?;
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
    /// Entries that exist but could not be inflated, with the decoder's
    /// error, reported by [`check_resource_limits`] as PKG-008.
    unreadable: Vec<(String, String)>,
    /// Bytes read so far from distinct entries, against [`MAX_BOOK_BYTES`].
    read_total: u64,
    /// Entries already charged to `read_total`; reading one again is free.
    charged: std::collections::HashSet<String>,
    /// Entries [`Ocf::read`] refused because the book had used up
    /// [`MAX_BOOK_BYTES`], reported by [`check_resource_limits`].
    over_budget: Vec<String>,
    /// The budget itself: [`MAX_BOOK_BYTES`], held here so a test can make
    /// it small instead of inflating hundreds of MiB.
    book_budget: u64,
    /// Entries already inflated, so reading one again is a copy rather than
    /// a second decompression. See [`MAX_READ_CACHE_BYTES`].
    read_cache: HashMap<String, Vec<u8>>,
    read_cache_bytes: u64,
    /// Container paths named by a `<CipherReference>` in
    /// `META-INF/encryption.xml`, filled in by [`check_encryption`].
    encrypted: std::collections::HashSet<String>,
    /// The subset of `encrypted` that no file may be: reported once as
    /// SPEC-001 by [`check_encryption`], so the PKG-026 check in `opf.rs`
    /// does not report the same `CipherReference` a second time.
    pub(crate) encrypted_forbidden: std::collections::HashSet<String>,
    /// While set, every read answers `None` without touching the archive or
    /// the budget. Set for the package-only pass over a malformed package
    /// document's well-formed prefix (`opf::check`), which must ask what the
    /// package says without opening a single resource: epubcheck never gets
    /// that far, and a read there would spend this book's budget on files
    /// nothing else asked for.
    reads_suspended: bool,
    /// The first entry is `mimetype` and is not stored, for
    /// [`check_mimetype_stored`].
    mimetype_compressed: bool,
}

impl Ocf {
    /// Whether this container path is declared encrypted.
    ///
    /// Normalized on both sides. The set is built from `encryption.xml`'s
    /// `URI` attributes and asked with the name the zip's central directory
    /// carries, and those two are not required to agree byte for byte: a
    /// container written on macOS can hold a *decomposed* filename while the
    /// document that names it writes the *precomposed* one. Comparing raw
    /// would have failed open — the resource would simply have been parsed as
    /// its declared type again, which is the bug this set exists to stop, and
    /// it would have failed only for non-ASCII names, so nothing on the shelf
    /// or in the corpus would have shown it.
    pub fn is_encrypted(&self, name: &str) -> bool {
        self.encrypted.contains(&crate::opf::nfc(name))
    }

    /// Read an entry **only if its bytes are meant to be examined**.
    ///
    /// An obfuscated or encrypted resource is ciphertext: parsing it as
    /// XHTML, CSS or a PNG produces findings about the encryption, not about
    /// the book. epubcheck says so and means it — `RSC-004: its content will
    /// not be checked` — and we said the same thing and then checked anyway.
    ///
    /// Reported by Doitsu on MobileRead with an obfuscation test book: 15
    /// findings, including **ten fatals**, on five encrypted files. The fatals
    /// were the worst of it, because a fatal drops the rest of that document's
    /// findings, so the false positives were also hiding real ones.
    ///
    /// The rule is about *content*, not about the resource: an encrypted font
    /// is still subject to OPF-097, PKG-026 and the rest, exactly as in
    /// epubcheck's own output for that book. So this is a separate method
    /// rather than a filter inside [`Ocf::read`] — the encryption checks
    /// themselves have to read these entries.
    /// Run `f` with every read answering `None`; see `reads_suspended`.
    pub(crate) fn without_reads<T>(&mut self, f: impl FnOnce(&mut Ocf) -> T) -> T {
        let was = std::mem::replace(&mut self.reads_suspended, true);
        let out = f(self);
        self.reads_suspended = was;
        out
    }

    pub(crate) fn read_content(&mut self, name: &str) -> Option<Vec<u8>> {
        if self.is_encrypted(name) {
            return None;
        }
        self.read(name)
    }

    /// The first `n` bytes of an entry, for a check that decides from a
    /// signature: an image's magic number, a font's first four bytes.
    /// Encrypted entries are refused exactly as [`Ocf::read_content`] does.
    ///
    /// Not subject to [`MAX_ENTRY_BYTES`], because nothing past `n` is read.
    /// That limit exists for the resources this crate *parses*; applying it
    /// to a signature check made a 70 MB PNG an `ERROR LIM-001` and the book
    /// INVALID, while EPUBCheck 5.4.0 reported it valid. The image check
    /// needs twelve bytes and was inflating the whole entry to get them.
    pub(crate) fn read_head_content(&mut self, name: &str, n: u64) -> Option<Vec<u8>> {
        if self.reads_suspended || self.is_encrypted(name) {
            return None;
        }
        self.inflate(name, n)
    }

    /// Up to `limit` inflated bytes of an entry, or `None` when there is no
    /// such entry or it cannot be inflated. An entry that exists and fails
    /// is recorded in `unreadable`, never passed over: a bare `None` reads
    /// as "absent" at every call site, so a corrupt chapter went unchecked
    /// and a book with errors in it reported VALID.
    ///
    /// The CRC is not checked. epubcheck reads through Java's `ZipFile`,
    /// which does not check it either, so a wrong CRC alone draws nothing
    /// there. Checking it here made a valid book FATAL.
    fn inflate(&mut self, name: &str, limit: u64) -> Option<Vec<u8>> {
        if self.is_unreadable(name) {
            return None;
        }
        let index = self.archive.index_for_name(name)?;
        let read = self.inflate_index(index, limit);
        match read {
            Ok(buf) => Some(buf),
            Err(e) => {
                self.unreadable.push((name.to_string(), e));
                None
            }
        }
    }

    /// Stored and deflated entries are read from their raw bytes and
    /// inflated here, bounded by `limit` alone. zip 9 bounds every archive
    /// read by the uncompressed size the header declares (zip-rs/zip2#1022)
    /// and has no option to turn that off, so an entry holding more than its
    /// header says became a read error, where epubcheck (Java's `ZipFile`)
    /// and zip 8 read it whole. The CRC is not checked, as in epubcheck.
    /// Anything else — another method, an encrypted entry — goes through
    /// zip's own reader, which refuses it as before.
    fn inflate_index(&mut self, index: usize, limit: u64) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        let raw = self
            .archive
            .by_index_raw(index)
            .map_err(|e| e.to_string())?;
        if !raw.encrypted() {
            let method = raw.compression();
            if method == zip::CompressionMethod::Stored {
                raw.take(limit)
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                return Ok(buf);
            }
            if method == zip::CompressionMethod::Deflated {
                flate2::read::DeflateDecoder::new(raw)
                    .take(limit)
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                return Ok(buf);
            }
        }
        drop(raw);
        let options = zip::ZipReadOptions::new().ignore_crc32(true);
        let f = self
            .archive
            .by_index_with_options(index, options)
            .map_err(|e| e.to_string())?;
        f.take(limit)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    /// Whether an entry exists but could not be inflated.
    pub(crate) fn is_unreadable(&self, name: &str) -> bool {
        self.unreadable.iter().any(|(n, _)| n == name)
    }

    /// Take an unreadable entry off the PKG-008 list, for a check that
    /// reports it under its own id: epubcheck's `BitmapChecker` reports an
    /// image it cannot read as PKG-021, not PKG-008.
    pub(crate) fn claim_unreadable(&mut self, name: &str) -> bool {
        let before = self.unreadable.len();
        self.unreadable.retain(|(n, _)| n != name);
        self.unreadable.len() != before
    }
}

/// How much of an entry a signature check reads: more than the longest
/// signature matched (WebP's twelve bytes), with room to spare.
pub(crate) const SIGNATURE_BYTES: u64 = 64;

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

/// How many inflated bytes [`Ocf::read`] keeps, so that an entry read again
/// is copied rather than decompressed again.
///
/// The checks ask for the same entry many times: on the shelf, one book's 413
/// entries were read 4,777 times, its stylesheets about 400 times each, once
/// per chapter that links them, and decompression was 15% of the whole run.
/// The largest book on the shelf reads 25.5 MB in all, so 64 MiB holds every
/// real book whole, while a hostile one can add at most this much to the
/// peak; past it, entries are simply read the old way.
pub(crate) const MAX_READ_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// How many bytes [`Ocf::read`] inflates for one publication, summed over
/// distinct entries. Past it, an entry not yet read is refused and reported
/// as LIM-002.
///
/// The entry cap bounds one resource; nothing bounded their number. Time
/// is spent per byte of markup, so a container of many capped chapters,
/// each a few hundred KB deflated, validated for as long as it had chapters.
/// 256 MiB against a measured worst case on the 474-book shelf of
/// **25.5 MB** read by one book (a mathematics textbook), ~10x.
///
/// The check is made before an entry is read, so the last entry admitted may
/// carry the total past the limit by up to [`MAX_ENTRY_BYTES`].
pub(crate) const MAX_BOOK_BYTES: u64 = 256 * 1024 * 1024;

impl Ocf {
    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Read a member's bytes (decompressing as needed), up to an internal
    /// per-entry cap; an entry over it reads as `None` and is reported by
    /// `check_resource_limits` rather than materialised.
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        if self.reads_suspended {
            return None;
        }
        // Already known to be over the cap: the answer cannot change, and
        // asking again would inflate another 64 MiB to learn it (a stylesheet
        // is read from two places, so every oversized one cost that twice).
        if self.oversized.iter().any(|n| n == name) {
            return None;
        }
        // The book's budget is charged once per entry, on its first read, so
        // an entry read from several places costs once, and an entry already
        // read stays readable after the budget runs out: refusing it on a
        // second read would leave two checks disagreeing about one file.
        let first_read = !self.charged.contains(name);
        if first_read && self.read_total > self.book_budget {
            // Only an entry that exists was refused. Checks also ask for
            // optional files (`META-INF/encryption.xml`); naming one of those
            // as "not checked" would be a false statement about the book.
            if self.has(name) && !self.over_budget.iter().any(|n| n == name) {
                self.over_budget.push(name.to_string());
            }
            return None;
        }
        if let Some(bytes) = self.read_cache.get(name) {
            return Some(bytes.clone());
        }
        // One byte past the cap: reading *more* than it is what separates an
        // oversized entry from one that happens to end exactly on it.
        let buf = self.inflate(name, MAX_ENTRY_BYTES + 1)?;
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
        if first_read {
            self.read_total += buf.len() as u64;
            self.charged.insert(name.to_string());
        }
        if self.read_cache_bytes + buf.len() as u64 <= MAX_READ_CACHE_BYTES {
            self.read_cache_bytes += buf.len() as u64;
            self.read_cache.insert(name.to_string(), buf.clone());
        }
        Some(buf)
    }
}

/// Report every entry [`Ocf::read`] refused for exceeding
/// [`MAX_ENTRY_BYTES`]. Called once per run, after the checks, so a
/// resource read from several places is still reported once.
/// PKG-007 for a compressed `mimetype`. EPUB 3.3 § 4.3.3: "EPUB creators MUST
/// NOT compress or encrypt the `mimetype` file" (OCF 2.0.1 says the same).
/// epubcheck 5.4.0 never asks: it compares the inflated contents only. This
/// was NEXT-016 behind `--advisory` from 0.29.0; the spec decides the verdict
/// since 2026-11-08, so it is an error again, under the id whose own message
/// names the fault. The rule key is unchanged.
pub(crate) fn check_mimetype_stored(ocf: &Ocf, report: &mut Report) {
    if ocf.mimetype_compressed {
        report.push_at_rule(
            PKG_007,
            Severity::Error,
            "the 'mimetype' file is compressed; it must be stored uncompressed",
            "mimetype",
            "ocf.mimetype.not_stored_uncompressed",
            Vec::new(),
        );
    }
}

pub(crate) fn check_resource_limits(ocf: &Ocf, report: &mut Report) {
    // PKG-008, as epubcheck's `XMLParser` reports an `IOException` while
    // parsing. Its CSS checker puts the exception's message in the slot
    // where the others put the path; `params` carries the path for every
    // file here, and the decoder's message goes in the text.
    for (name, error) in &ocf.unreadable {
        report.push_at_rule(
            PKG_008,
            Severity::Fatal,
            format!("file '{name}' could not be read: {error}"),
            name.as_str(),
            "ocf.entry.unreadable",
            vec![name.clone()],
        );
    }
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
    for name in &ocf.over_budget {
        report.push_at(
            LIM_002,
            Severity::Error,
            format!(
                "the publication's resources exceed the {} MiB it may read in \
                 total, so this one was not checked",
                MAX_BOOK_BYTES / (1024 * 1024)
            ),
            name.clone(),
        );
    }
}

/// What epubcheck's `OCFZipChecker` says about the file's first 58 bytes.
#[derive(Clone, Copy, PartialEq, Debug)]
enum RawHeader {
    /// PKG-004: long enough, but does not start with `PK`.
    BadSignature,
    /// PKG-006: the first local header does not name `mimetype`.
    NotFirst,
    /// PKG-005: the first local header names `mimetype` with an extra field.
    ExtraField,
}

/// `OCFZipChecker`'s chain on the raw leading bytes, which epubcheck runs on
/// every container, before and apart from opening the archive. Same order,
/// same else-if: the signature, the name length at offset 26, the extra-field
/// length at 28, the name at 30.
///
/// The signature test is `header[0] != 'P' && header[1] != 'K'`, an **and**,
/// so one matching byte falls through to the name checks (#80). A file
/// shorter than 58 bytes is PKG-003, reported by the caller, and gets none
/// of these.
fn judge_raw_header(bytes: &[u8]) -> Option<RawHeader> {
    let h = bytes.get(..58)?;
    if h[0] != b'P' && h[1] != b'K' {
        return Some(RawHeader::BadSignature);
    }
    let name_len = u16::from_le_bytes([h[26], h[27]]);
    let extra_len = u16::from_le_bytes([h[28], h[29]]);
    if name_len != 8 {
        Some(RawHeader::NotFirst)
    } else if extra_len != 0 {
        Some(RawHeader::ExtraField)
    } else if &h[30..38] != b"mimetype" {
        Some(RawHeader::NotFirst)
    } else {
        None
    }
}

fn report_raw_header(finding: RawHeader, report: &mut Report) {
    match finding {
        RawHeader::BadSignature => report.push(
            PKG_004,
            Severity::Fatal,
            "Not a valid EPUB container (corrupted ZIP header)",
        ),
        RawHeader::NotFirst => report.push_rule(
            PKG_006,
            Severity::Error,
            "The 'mimetype' file must be the first entry in the EPUB ZIP",
            "ocf.mimetype.not_first_entry",
            Vec::new(),
        ),
        RawHeader::ExtraField => report.push(
            PKG_005,
            Severity::Error,
            "The 'mimetype' entry's ZIP header must not have an extra field",
        ),
    }
}

/// Why Java's `ZipFile`, which epubcheck opens every book with, refuses the
/// archive's end record, or `None` when it accepts it or when the question
/// is left to zip.
///
/// The two readers choose the end record differently. Java takes the last
/// `PK\5\6` within reach of the end whose comment length ends exactly at the
/// end of the file, or, failing that, whose central directory and first
/// local header carry their signatures. It then refuses the archive if that
/// record is inconsistent. zip passes over such a record to an earlier one
/// that works (GHSA-wjr6-mhvj-cjvm, still so in 9.1.3), so a book with a
/// broken end record appended was VALID here and FATAL PKG-008 there.
///
/// This follows OpenJDK 26's `ZipFile.Source.findEND` and the end-record
/// checks at the top of `initCEN`, read from its source; the logic has been
/// stable for many releases, but other JDKs were not measured. A ZIP64
/// locator ahead of the record is left to zip. The central directory itself
/// goes to [`java_central_directory_error`].
fn java_end_header_error(bytes: &[u8]) -> Option<String> {
    const ENDHDR: usize = 22;
    const CENHDR: u64 = 46;
    let sig_at = |pos: u64, sig: &[u8; 4]| {
        usize::try_from(pos)
            .ok()
            .and_then(|p| bytes.get(p..p.checked_add(4)?))
            .is_some_and(|b| b == sig)
    };
    let len = bytes.len();
    if len < ENDHDR {
        return None;
    }
    let lowest = len.saturating_sub(0xFFFF + ENDHDR);
    for pos in (lowest..=len - ENDHDR).rev() {
        let end = &bytes[pos..pos + ENDHDR];
        if end[..4] != *b"PK\x05\x06" {
            continue;
        }
        let total = u64::from(u16::from_le_bytes([end[10], end[11]]));
        let cen_len = u64::from(u32::from_le_bytes([end[12], end[13], end[14], end[15]]));
        let cen_off = u64::from(u32::from_le_bytes([end[16], end[17], end[18], end[19]]));
        let comment_len = usize::from(u16::from_le_bytes([end[20], end[21]]));
        let end_pos = pos as u64;
        if pos + ENDHDR + comment_len != len {
            // Java's second chance for a record whose comment does not reach
            // the end: both signatures where the record says.
            let found = end_pos
                .checked_sub(cen_len)
                .and_then(|cen| Some((cen, cen.checked_sub(cen_off)?)))
                .is_some_and(|(cen, loc)| sig_at(cen, b"PK\x01\x02") && sig_at(loc, b"PK\x03\x04"));
            if !found {
                continue;
            }
        }
        if pos >= 20 && bytes[pos - 20..pos - 16] == *b"PK\x06\x07" {
            return None;
        }
        if end_pos == 0 {
            return None;
        }
        if cen_len > end_pos {
            return Some("invalid END header (bad central directory size)".to_string());
        }
        if end_pos - cen_len < cen_off {
            return Some("invalid END header (bad central directory offset)".to_string());
        }
        if total > cen_len / CENHDR {
            return Some("invalid END header (total entries count too large)".to_string());
        }
        let cen = (end_pos - cen_len) as usize..end_pos as usize;
        return java_central_directory_error(&bytes[cen]);
    }
    Some("zip END header not found".to_string())
}

/// What Java's `ZipFile` says about a central directory, if it refuses it, in
/// its own words: `OCFChecker` tells PKG-027 from PKG-008 by the message, and
/// only `invalid CEN header`, in lower case, is PKG-027.
///
/// zip reads an encrypted entry, a comment that is not UTF-8 and more besides,
/// and refuses others for its own reasons, so a book could be VALID here and
/// FATAL there. This follows `Source.initCEN`, `checkAndAddEntry` and
/// `checkExtraFields` in OpenJDK 26, read from the source and measured on 27.
/// Older JDKs may check less, and were not measured. epubcheck opens the
/// archive as UTF-8 (`OCFZipResources`), so names and comments are UTF-8
/// whatever the entry's flag says.
fn java_central_directory_error(cen: &[u8]) -> Option<String> {
    const CENHDR: usize = 46;
    const ZIP64_MAGIC: u32 = 0xFFFF_FFFF;
    const ZIP64_MAGIC_COUNT: u16 = 0xFFFF;
    let u16_at = |p: usize| u16::from_le_bytes([cen[p], cen[p + 1]]);
    let u32_at = |p: usize| u32::from_le_bytes([cen[p], cen[p + 1], cen[p + 2], cen[p + 3]]);
    let mut pos = 0;
    while pos + CENHDR <= cen.len() {
        if cen[pos..pos + 4] != *b"PK\x01\x02" {
            return Some("invalid CEN header (bad signature)".to_string());
        }
        if u16_at(pos + 8) & 1 != 0 {
            return Some("invalid CEN header (encrypted entry)".to_string());
        }
        let method = u16_at(pos + 10);
        if method != 0 && method != 8 {
            return Some(format!(
                "invalid CEN header (bad compression method: {method})"
            ));
        }
        let (name_len, extra_len, comment_len) = (
            usize::from(u16_at(pos + 28)),
            usize::from(u16_at(pos + 30)),
            usize::from(u16_at(pos + 32)),
        );
        let header_size = CENHDR + name_len + extra_len + comment_len;
        if header_size > 0xFFFF || pos + header_size > cen.len() {
            return Some("invalid CEN header (bad header size)".to_string());
        }
        let (csize, size, disk, offset) = (
            u32_at(pos + 20),
            u32_at(pos + 24),
            u16_at(pos + 34),
            u32_at(pos + 42),
        );
        let needs_zip64 = csize == ZIP64_MAGIC
            || size == ZIP64_MAGIC
            || offset == ZIP64_MAGIC
            || disk == ZIP64_MAGIC_COUNT;
        let name_at = pos + CENHDR;
        let extra_at = name_at + name_len;
        if extra_len == 0 && needs_zip64 {
            return Some("Invalid CEN header (invalid zip64 extra len size)".to_string());
        }
        let extra_end = extra_at + extra_len;
        let mut at = extra_at;
        while at + 4 <= extra_end {
            let (tag, block) = (u16_at(at), usize::from(u16_at(at + 2)));
            at += 4;
            if at + block > extra_end {
                return Some(format!(
                    "Invalid CEN header (invalid extra data field size for tag: 0x{tag:04x} at {pos})"
                ));
            }
            if tag == 1 {
                let expected = usize::from(csize == ZIP64_MAGIC) * 8
                    + usize::from(size == ZIP64_MAGIC) * 8
                    + usize::from(offset == ZIP64_MAGIC) * 8
                    + usize::from(disk == ZIP64_MAGIC_COUNT) * 4;
                if (block == 0 && needs_zip64) || (block != 0 && block != expected) {
                    return Some(
                        "Invalid CEN header (invalid zip64 extra data field size)".to_string(),
                    );
                }
                // Only the sign is left to judge, and the size was just proved
                // to cover each field that is present.
                if block != 0 {
                    let mut value = at;
                    for (field, label) in [
                        (size, "size"),
                        (csize, "compressed size"),
                        (offset, "LOC OFFSET"),
                    ] {
                        if field == ZIP64_MAGIC {
                            if cen[value + 7] & 0x80 != 0 {
                                return Some(format!("Invalid zip64 extra block {label} value"));
                            }
                            value += 8;
                        }
                    }
                }
            }
            at += block;
        }
        let comment_at = extra_end;
        if std::str::from_utf8(&cen[name_at..extra_at]).is_err()
            || std::str::from_utf8(&cen[comment_at..comment_at + comment_len]).is_err()
        {
            return Some("invalid CEN header (bad entry name or comment)".to_string());
        }
        pos += header_size;
    }
    (pos != cen.len()).then(|| "invalid CEN header (bad header size)".to_string())
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
    let raw_header = judge_raw_header(&bytes);
    // Java's reader is asked first: where it and zip choose different
    // end records, epubcheck has already refused the archive.
    let opened = match java_end_header_error(&bytes) {
        Some(e) => Err(e),
        None => ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string()),
    };
    let mut archive = match opened {
        Ok(a) => a,
        Err(e) => {
            // The image sniff is a second route to PKG-004 here: a file that
            // *is* a recognisable other format still gets it even if its
            // first two bytes happen to be `PK` (#80).
            if looks_like_other_format && raw_header != Some(RawHeader::BadSignature) {
                report_raw_header(RawHeader::BadSignature, report);
            } else if let Some(finding) = raw_header {
                report_raw_header(finding, report);
            }
            // `OCFChecker.checkContainerStructure` tells the two apart by the
            // exception's message.
            if e.starts_with("invalid CEN header") {
                report.push_rule(
                    PKG_027,
                    Severity::Fatal,
                    format!("could not read the ZIP central directory: {e}"),
                    "ocf.container.invalid_central_directory",
                    vec![e.clone()],
                );
                return None;
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
            // zip 9 returns the decoded name as a `Result`; the name was just
            // checked to be UTF-8, so the fallback is never taken.
            let name = f
                .name()
                .map(|n| n.into_owned())
                .unwrap_or_else(|_| String::from_utf8_lossy(f.name_raw()).into_owned());
            if i == 0 {
                first_name = name.clone();
                first_stored = f.compression() == zip::CompressionMethod::Stored;
            }
            // PKG-009/011/012: checked per path *segment* (a directory
            // name is a file name too), on every real entry regardless of
            // whether it's a "publication resource" (confirmed via a real
            // fixture flagging a forbidden character inside META-INF).
            //
            // **A directory ENTRY is skipped, though, and that is not the
            // same statement.** `OCFChecker` runs `OCFFilenameChecker` only
            // in the `else` of `if (resource.isDirectory())`, so epubcheck
            // never asks these of a directory entry — while a directory
            // *name* is still asked, as a segment of the paths of the files
            // inside it. Checking the entry as well reported the same
            // segment twice: `ocf-filepath-utf8-valid` carries both
            // `EPUB/spécial/` and `EPUB/spécial/styles.css`, and drew two
            // PKG-012 against epubcheck's one. An empty offending directory
            // now says nothing, which is also what epubcheck says.
            //
            // The entry stays in `names`: OPF-060 and PKG-014 below both
            // need it.
            if !name.ends_with('/') {
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
                    // PKG-010 belongs here with its three siblings, and it was the
                    // one missing from the set. epubcheck asks all four in
                    // `OCFFilenameChecker`, which in full-publication mode runs
                    // over **container entries**; we were asking this one of the
                    // manifest *href* instead, in `opf.rs`. That happens to agree
                    // whenever the file exists and disagrees whenever it does not
                    // — probed: a declared-but-absent `a b.xhtml` draws RSC-001
                    // alone from epubcheck and RSC-001 plus PKG-010 from us, and
                    // an `a%20b.xhtml` href whose entry is literally named that
                    // draws OPF-003 there and OPF-003 plus PKG-010 here, because
                    // the space exists only after percent-decoding.
                    //
                    // Asking the container instead also covers the case no
                    // manifest mentions at all, which the href form could not see.
                    if segment.contains(' ') {
                        report.push_at_rule(
                            PKG_010,
                            Severity::Warning,
                            format!("file name '{segment}' contains a space"),
                            name.as_str(),
                            "ocf.filename.contains_space",
                            vec![segment.to_string()],
                        );
                    }
                }
            }
            names.push(name);
        } else if let Some(Ok(name)) = archive.name_for_index(i) {
            // Listed in the central directory, but its local header cannot
            // be read: Java's reader lists it too, and fails on opening it.
            // So it is named here and its read becomes PKG-008 like any
            // damaged entry's. Dropped, it read as absent: a container.xml
            // that cannot be read was "missing", RSC-002, where epubcheck
            // reports PKG-008 and RSC-003.
            names.push(name.into_owned());
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

    // mimetype: must be the first entry, exact contents, and its ZIP header
    // must carry no extra field (needed so tools can sniff the media type at
    // a fixed byte offset without a full ZIP parse). That it is stored is
    // asked by `check_mimetype_stored`.
    //
    // "First" is judged on the file's leading bytes, as epubcheck judges it,
    // not on the central directory's order. The two part when the directory
    // lists something the bytes do not start with: an end-of-central-
    // directory signature inside the archive comment gave both readers an
    // empty directory, and we reported PKG-006 on a book whose first bytes
    // are the `mimetype` entry. And 16 junk bytes before the first header,
    // which both readers skip, drew nothing here and FATAL PKG-004 there.
    if let Some(finding) = raw_header {
        report_raw_header(finding, report);
    }

    let mut ocf = Ocf {
        archive,
        names,
        oversized: Vec::new(),
        unreadable: Vec::new(),
        read_total: 0,
        charged: std::collections::HashSet::new(),
        over_budget: Vec::new(),
        book_budget: MAX_BOOK_BYTES,
        read_cache: HashMap::new(),
        read_cache_bytes: 0,
        encrypted: std::collections::HashSet::new(),
        encrypted_forbidden: std::collections::HashSet::new(),
        reads_suspended: false,
        mimetype_compressed: first_name == "mimetype" && !first_stored,
    };

    // epubcheck's `OCFChecker.checkMimetypeFile` compares the inflated
    // contents, and ignores an `IOException` from reading them ("missing file
    // is reported elsewhere"), so a `mimetype` that cannot be read is no
    // PKG-008 there.
    let mimetype = ocf.read("mimetype");
    ocf.claim_unreadable("mimetype");
    if ocf.has("mimetype")
        && let Some(b) = mimetype
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
        // Unreadable rather than absent (absence was reported above): PKG-008
        // comes at the end of the run, and epubcheck, having found no
        // rootfile in what it could not read, adds RSC-003.
        if ocf.is_unreadable(CONTAINER) {
            report.push_at(
                RSC_003,
                Severity::Error,
                "container.xml could not be read, so no <rootfile> with media-type \
                 'application/oebps-package+xml' was found",
                CONTAINER,
            );
        }
        return Vec::new();
    };
    let text = crate::encoding::decode_xml(&bytes);
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
    // Only container-namespace `<rootfile>`s count, as only those reach
    // epubcheck's `OCFContainerFileHandler`: a container.xml in the wrong
    // namespace declares no package document at all (RSC-003), where we used
    // to match by local name and validate the book as usual.
    for rf in doc
        .descendants()
        .filter(|n| is_container_element(n, "rootfile"))
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
    // The media type is trimmed first (`processRootFile`), so padding
    // around it still declares a package document.
    let paths: Vec<String> = doc
        .descendants()
        .filter(|n| {
            is_container_element(n, "rootfile")
                && n.attr_no_ns("media-type").map(str::trim)
                    == Some("application/oebps-package+xml")
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

    paths
}

/// A container-namespace element named `name` that survives the removal both
/// specifications apply first: an element from another namespace goes with
/// its whole content (EPUB 3.3 § 4.2.6.3.1, OCF 2.0.1 § 3.5.1), so a
/// `rootfile` or `link` inside one is not there.
pub(crate) fn is_container_element(n: &roxmltree::Node, name: &str) -> bool {
    n.is_element()
        && n.tag_name().name() == name
        && n.ancestors()
            .filter(|a| a.is_element())
            .all(|a| a.tag_name().namespace() == Some(crate::rng::CONTAINER_NS))
}

/// RSC-005: `META-INF/container.xml` against `schemas/container.rng`, entered
/// at the grammar for the version the publication is validated as.
///
/// epubcheck validates the container file only once it has a package
/// document to follow (`OCFChecker.checkOtherMetaFiles`, after
/// `checkDeclaredPackageDocuments`): no rootfile is RSC-003 and nothing else,
/// a rootfile whose file is missing is OPF-002 and nothing else, and a
/// first package document that will not parse stops the check there too. So
/// this runs under the same three conditions, and a schema error never comes
/// on top of one of those.
pub fn check_container_schema(
    ocf: &mut Ocf,
    opf_paths: &[String],
    epub3: bool,
    report: &mut Report,
) {
    const CONTAINER: &str = "META-INF/container.xml";
    if opf_paths.is_empty() || !opf_paths.iter().all(|p| ocf.has(p)) {
        return;
    }
    let first_parses = ocf
        .read(&opf_paths[0])
        .is_some_and(|b| parse_xml(&crate::encoding::decode_xml(&b)).is_ok());
    if !first_parses {
        return;
    }
    let Some(bytes) = ocf.read(CONTAINER) else {
        return;
    };
    let text = crate::encoding::decode_xml(&bytes);
    let Ok(doc) = parse_xml(&text) else {
        return;
    };
    // Both specifications validate the file only after removing elements and
    // attributes from other namespaces (EPUB 3.3 § 4.2.6.3.1, OCF 2.0.1
    // § 3.5.1). epubcheck 5.4.0 validates them as written and reports RSC-005
    // (w3c/epubcheck#1754); we follow the specifications. At EPUB 3 the
    // rendition namespace is part of the definition, since Multiple-Rendition
    // Publications extends the container with it; at EPUB 2 it is another
    // namespace like any other.
    let grammar = crate::rng::container_grammar(epub3);
    let keep: &[&str] = if epub3 {
        &[crate::rng::CONTAINER_NS, crate::renditions::RENDITION_NS]
    } else {
        &[crate::rng::CONTAINER_NS]
    };
    for blame in crate::rng::validate_node_report_in_namespaces(&grammar, doc.root_element(), keep)
    {
        crate::opf::push_blame(report, CONTAINER, "ocf.container.schema_violation", &blame);
    }
    if epub3 {
        crate::renditions::check_container_rules(&doc, report);
    }
}

/// If `META-INF/encryption.xml` is present, report each encrypted resource as
/// RSC-004 (INFO) — its content is not validated. Also checks the file's
/// own content model: root element name, `Id`-attribute uniqueness, and
/// (IDPF compression extension) `Compression` attribute validity.
/// The files no publication may encrypt, besides its package documents:
/// EPUB 3.3 § 4.2.6.3.2.1 ("EPUB creators MUST NOT encrypt the following
/// files") and OCF 2.0.1 § 3.5.2 ("The following files MUST never be
/// encrypted") give the same list.
const NEVER_ENCRYPTED: [&str; 7] = [
    "mimetype",
    "META-INF/container.xml",
    "META-INF/encryption.xml",
    "META-INF/manifest.xml",
    "META-INF/metadata.xml",
    "META-INF/rights.xml",
    "META-INF/signatures.xml",
];

pub fn check_encryption(
    ocf: &mut Ocf,
    opf_paths: &[String],
    report: &mut Report,
    epub3: Option<bool>,
) {
    const ENC: &str = "META-INF/encryption.xml";
    if !ocf.has(ENC) {
        return;
    }
    let Some(bytes) = ocf.read(ENC) else { return };
    // Collected locally and handed to `ocf` at the end: the walk below
    // borrows a document parsed out of these bytes, so `ocf` cannot be
    // mutated while it is alive.
    let mut encrypted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut forbidden: std::collections::HashSet<String> = std::collections::HashSet::new();
    let text = crate::encoding::decode_xml(&bytes);
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

    // A wrong root is one schema error, and the schema reads no further. But
    // it does not stop epubcheck reading the entries: `OCFEncryptionFileHandler`
    // looks for `CipherReference` under `EncryptedData` whatever the root is
    // called, so the files stay encrypted (RSC-004, RSC-007, OBS-001 as
    // usual). Returning here left them unregistered, and their content was
    // parsed as if plain.
    let root_ok = doc.root_element().tag_name().name() == "encryption";
    if !root_ok {
        report.push_node(
            RSC_005,
            Severity::Error,
            "expected element \"encryption\" as the root of META-INF/encryption.xml",
            ENC,
            doc.root_element(),
            "ocf.encryption.wrong_root_element",
            Vec::new(),
        );
    } else {
        check_encryption_children(doc.root_element(), report, epub3);
    }

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
        .filter(|n| root_ok && n.is_element() && n.tag_name().name() == "Compression")
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

    // A `CipherReference` names a container entry, and the entry can be gone —
    // the shape a user meets after deleting an obfuscated font and leaving its
    // `encryption.xml` behind (JSWolf, MobileRead #223). epubcheck reports
    // **RSC-007 instead of RSC-004** there: an encrypted resource that is not
    // in the container is not a file whose content was skipped, it is a
    // reference to nothing.
    //
    // **This gap had a comment claiming it was covered.** The PKG-026 check in
    // `opf.rs` skips a cipher reference whose target is absent, saying "a
    // missing resource is already reported elsewhere (RSC-001/004)" — and
    // nothing did: RSC-004 says a file is *encrypted*, never that it is
    // missing, and no site emits RSC-001 for this. That is the silent-skip
    // shape this project keeps meeting, where a case falls between two checks
    // and reports nothing at all, which is the one failure a user cannot
    // notice.
    //
    // It is also the per-source reference problem again: epubcheck resolves
    // every registered reference through one path, while resolution here is
    // written per source — NCX `<content src>`, content-document hrefs,
    // `epub:textref`, the `<guide>` (added after 0.9.14) — and this source was
    // never added to the list.
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "CipherReference")
    {
        if let Some(uri) = n.attr_no_ns("URI") {
            // Container-relative, not relative to META-INF — the same base the
            // PKG-026 check resolves against, confirmed there against real
            // fixtures whose OPF sits in a subdirectory.
            let resolved = crate::opf::nfc(&crate::opf::resolve("", uri));
            if !ocf.names.iter().any(|n| crate::opf::nfc(n) == resolved) {
                report.push_node(
                    RSC_007,
                    Severity::Error,
                    format!("encrypted resource '{uri}' is not present in the container"),
                    ENC,
                    n,
                    "ocf.encryption.missing_resource",
                    vec![uri.to_string()],
                );
                continue;
            }
            // The set is the whole of this pass's output now. **RSC-004
            // itself moved to the manifest** (`opf::check_encrypted_resources`)
            // because that is where epubcheck reports it: from
            // `PublicationResourceChecker`, which `OPFChecker` constructs once
            // per manifest item. A file that is encrypted but *undeclared*
            // therefore draws OPF-003 from epubcheck and nothing else — no
            // checker ever runs over it, so there is nothing that "will not be
            // checked" — while reporting from here invented the message for
            // exactly those files. One of epubcheck's own fixtures
            // (`ocf-encryption-compression-attributes-invalid-error`) is that
            // shape.
            //
            // The set still has to be built here rather than there: `read_content`
            // has to decline these entries during every pass, and one of the
            // renditions of a multi-rendition book may not declare a file the
            // other does.
            // SPEC-001: a file the specifications say no publication may
            // encrypt. epubcheck 5.4.0 has no check for this list; its PKG-026
            // asks only of manifest items, and none of these is one.
            if NEVER_ENCRYPTED.contains(&resolved.as_str())
                || opf_paths.iter().any(|p| crate::opf::nfc(p) == resolved)
            {
                report.push_node(
                    crate::ids::SPEC_001,
                    Severity::Error,
                    format!("'{uri}' is declared encrypted, and this file must never be encrypted"),
                    ENC,
                    n,
                    "ocf.encryption.forbidden_file",
                    vec![uri.to_string()],
                );
                forbidden.insert(resolved.clone());
            }
            encrypted.insert(resolved.clone());
        }
    }
    // Hand the set to the container so the content passes can skip these
    // entries. Every early `return` above happens before any
    // `<CipherReference>` was read, so there is nothing to lose on those
    // paths.
    ocf.encrypted = encrypted;
    ocf.encrypted_forbidden = forbidden;
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
fn check_encryption_children(root: roxmltree::Node, report: &mut Report, epub3: Option<bool>) {
    const XENC: &str = "http://www.w3.org/2001/04/xmlenc#";
    const EXPECTED: &str = "expected one of \"enc:EncryptedData\", \"enc:EncryptedKey\"";

    let mut encrypted_items = 0usize;
    for child in root.children().filter(|n| n.is_element()) {
        let name = child.tag_name();
        if name.namespace() == Some(XENC) && matches!(name.name(), "EncryptedData" | "EncryptedKey")
        {
            encrypted_items += 1;
            check_encrypted_item(child, report, epub3);
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

/// The content model *inside* one `EncryptedData`/`EncryptedKey`, which is
/// where the version split lives (Doitsu, MobileRead #233; issue #88).
///
/// `check_encryption_children` above asks only which children `<encryption>`
/// has; nothing looked inside them, so an `<enc:EncryptedData>` with no
/// `<enc:CipherData>` — the exact file that was reported — was accepted, and
/// the RSC-004/RSC-007 pass then found no `CipherReference` to walk and stayed
/// silent too. Two checks, and the case between them reported nothing at all.
///
/// **The requirement inverts between versions**, which a single rule would get
/// wrong for half of all books. Measured against epubcheck 5.3.0, one book per
/// shape, rather than read off the spec:
///
/// | inside `EncryptedData` | EPUB 2 | EPUB 3 |
/// |---|---|---|
/// | nothing | missing `enc:EncryptionMethod` | missing `enc:CipherData` |
/// | `EncryptionMethod` only | *silent* | missing `enc:CipherData` |
/// | `CipherData` only | `CipherData` "not allowed yet" | *silent* |
/// | empty `CipherData` | expected `CipherReference`/`CipherValue` | same |
///
/// which is `schema/20/rng/xenc-schema.rng` (method required, cipher optional)
/// against `schema/30/mod/security/xenc-schema.rnc` (method optional, cipher
/// required). The EPUB 2 arm is a *sequence*, so a `CipherData` preceding the
/// method is reported even when both are present.
///
/// `epub3 == None` means no rootfile parsed and the version is genuinely
/// unknown. Only the version-independent rule runs then: guessing would invent
/// an error on a book already being reported for something worse.
fn check_encrypted_item(item: roxmltree::Node, report: &mut Report, epub3: Option<bool>) {
    const XENC: &str = "http://www.w3.org/2001/04/xmlenc#";
    let shown = format!("enc:{}", item.tag_name().name());

    let child = |local: &str| {
        item.children().find(|n| {
            n.is_element() && n.tag_name().namespace() == Some(XENC) && n.tag_name().name() == local
        })
    };
    let index = |local: &str| {
        item.children()
            .filter(|n| n.is_element())
            .position(|n| n.tag_name().namespace() == Some(XENC) && n.tag_name().name() == local)
    };

    let method = child("EncryptionMethod");
    let cipher = child("CipherData");

    // Which element this version requires, or `None` when the version is
    // unknown and neither may be asserted. Written as one lookup and one
    // report rather than an arm each: the two branches differ only in the
    // element they name, and a second copy of the `push_node` is where the
    // two would drift apart.
    let required = match epub3 {
        Some(true) if cipher.is_none() => Some("enc:CipherData"),
        Some(false) if method.is_none() => Some("enc:EncryptionMethod"),
        _ => None,
    };
    if let Some(required) = required {
        report.push_node(
            RSC_005,
            Severity::Error,
            format!(
                "element \"{shown}\" has incomplete content; \
                 missing required element \"{required}\""
            ),
            "META-INF/encryption.xml",
            item,
            "ocf.encryption.item_incomplete",
            vec![shown.clone(), required.to_string()],
        );
    }

    // **Order is version-independent, and assuming otherwise was a mistake
    // caught by probing the case rather than by reasoning about it.** Both
    // `EncryptedType` definitions are sequences, not interleaves, so a
    // `CipherData` before the `EncryptionMethod` is an error at 3.0 too - and
    // the first version of this check sat inside the EPUB 2 arm, which left
    // 3.0 silent on a file epubcheck reports.
    //
    // epubcheck words the two versions differently ("not allowed yet" plus a
    // second error at 2.0, "not allowed here" alone at 3.0) and we report one
    // finding either way. Lower than epubcheck at 2.0, never higher: the same
    // cascade-suppression shape already documented for RSC-005.
    if let (Some(m), Some(c)) = (index("EncryptionMethod"), index("CipherData"))
        && c < m
    {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"enc:CipherData\" must follow \"enc:EncryptionMethod\"",
            "META-INF/encryption.xml",
            cipher.expect("indexed above"),
            "ocf.encryption.item_out_of_order",
            vec![
                "enc:CipherData".to_string(),
                "enc:EncryptionMethod".to_string(),
            ],
        );
    }

    // Also version-independent: `CipherData` is
    // `(CipherValue | CipherReference)` in both grammars.
    if let Some(c) = cipher
        && !c.children().any(|n| {
            n.is_element()
                && n.tag_name().namespace() == Some(XENC)
                && matches!(n.tag_name().name(), "CipherValue" | "CipherReference")
        })
    {
        report.push_node(
            RSC_005,
            Severity::Error,
            "element \"enc:CipherData\" has incomplete content; \
             expected element \"enc:CipherReference\" or \"enc:CipherValue\"",
            "META-INF/encryption.xml",
            c,
            "ocf.encryption.cipher_data_incomplete",
            Vec::new(),
        );
    }
}

/// If `META-INF/signatures.xml` is present, checks its root element name
/// (RSC-005 if not "signatures") - its actual signature content isn't
/// validated, same "presence/shape only" scope as `check_encryption`.
/// The encoding of each `META-INF` file epubcheck parses, graded as
/// [`crate::encoding`] grades any XML resource (all of them are checked as
/// `application/xml`, so UTF-16 is RSC-027 here).
///
/// `container.xml` is parsed whenever it is present. `encryption.xml`,
/// `metadata.xml` and `signatures.xml` are parsed only once the container
/// file has named package documents that all exist: `OCFChecker.check()`
/// returns before `checkEncryptionFile` / `checkOtherMetaFiles` otherwise.
/// `manifest.xml` and `rights.xml` are never parsed, so never graded.
pub fn check_meta_inf_encodings(
    ocf: &mut Ocf,
    opf_paths: &[String],
    validated_as_3: bool,
    report: &mut Report,
) {
    let mut files = vec!["META-INF/container.xml"];
    if !opf_paths.is_empty() && opf_paths.iter().all(|p| ocf.has(p)) {
        files.extend([
            "META-INF/encryption.xml",
            "META-INF/metadata.xml",
            "META-INF/signatures.xml",
        ]);
    }
    for name in files {
        if !ocf.has(name) {
            continue;
        }
        if let Some(head) = ocf.read_head_content(name, crate::encoding::SNIFF_LEN as u64) {
            crate::encoding::check(&head, name, "application/xml", validated_as_3, report);
            // HTM-001, as for every XML file epubcheck parses (OCF 2.0.1
            // § 1.3: "compatible with the XML 1.0 specification").
            crate::htm::check_xml_version(&crate::encoding::decode_xml(&head), name, report);
        }
    }
}

pub fn check_signatures(ocf: &mut Ocf, report: &mut Report) {
    const SIG: &str = "META-INF/signatures.xml";
    if !ocf.has(SIG) {
        return;
    }
    let Some(bytes) = ocf.read(SIG) else { return };
    let text = crate::encoding::decode_xml(&bytes);
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
    /// `(rule, message)` pairs, in document order. `epub3` is the publication
    /// version, which the content model inside an `EncryptedData` depends on
    /// (issue #88); the callers that predate it pass `None`, the version this
    /// file's own root-level rules do not vary with.
    fn findings_v(body: &str, epub3: Option<bool>) -> Vec<(&'static str, String)> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">{body}</encryption>"#
        );
        let doc = crate::ocf::parse_xml(&xml).expect("fixture must parse");
        let mut report = Report::new();
        super::check_encryption_children(doc.root_element(), &mut report, epub3);
        report
            .messages
            .iter()
            .map(|m| (m.rule.expect("a rule"), m.text.clone()))
            .collect()
    }

    fn findings(body: &str) -> Vec<(&'static str, String)> {
        findings_v(body, None)
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

    /// Doitsu, MobileRead #233 (issue #88): the content model *inside* an
    /// `EncryptedData`. Every cell below was measured against epubcheck 5.3.0
    /// with one book per shape, not read off the spec, because the
    /// requirement **inverts between versions** and a rule written from either
    /// grammar alone is a false positive on the other half of the world.
    ///
    /// The `None` row is the case where no rootfile parsed: only the shared
    /// rule may fire, since guessing a version would invent an error on a book
    /// already being reported for something worse.
    #[test]
    fn an_encrypted_item_content_model_depends_on_the_version() {
        const EMPTY: &str = "<enc:EncryptedData></enc:EncryptedData>";
        const METHOD_ONLY: &str = "<enc:EncryptedData><enc:EncryptionMethod \
             Algorithm=\"http://www.idpf.org/2008/embedding\"/></enc:EncryptedData>";
        const CIPHER_ONLY: &str = "<enc:EncryptedData><enc:CipherData>\
             <enc:CipherReference URI=\"OEBPS/f.otf\"/></enc:CipherData></enc:EncryptedData>";

        let rules = |body: &str, epub3: Option<bool>| -> Vec<&'static str> {
            findings_v(body, epub3)
                .into_iter()
                .map(|(r, _)| r)
                .collect()
        };

        // EPUB 3 wants the cipher; the method is optional.
        assert_eq!(
            rules(EMPTY, Some(true)),
            vec!["ocf.encryption.item_incomplete"]
        );
        assert_eq!(
            rules(METHOD_ONLY, Some(true)),
            vec!["ocf.encryption.item_incomplete"]
        );
        assert_eq!(rules(CIPHER_ONLY, Some(true)), Vec::<&str>::new());

        // EPUB 2 wants the method; the cipher is optional. Exactly reversed.
        assert_eq!(
            rules(EMPTY, Some(false)),
            vec!["ocf.encryption.item_incomplete"]
        );
        assert_eq!(rules(METHOD_ONLY, Some(false)), Vec::<&str>::new());
        assert_eq!(
            rules(CIPHER_ONLY, Some(false)),
            vec!["ocf.encryption.item_incomplete"]
        );

        // Unknown version: neither requirement is asserted.
        for body in [EMPTY, METHOD_ONLY, CIPHER_ONLY] {
            assert_eq!(rules(body, None), Vec::<&str>::new(), "body {body}");
        }

        // `EncryptedKey` takes the same model - measured, and it is the half
        // an implementation written from the reported case alone would miss.
        assert_eq!(
            rules("<enc:EncryptedKey></enc:EncryptedKey>", Some(true)),
            vec!["ocf.encryption.item_incomplete"]
        );
    }

    /// The two rules that do **not** vary with the version, which is the part
    /// the first draft of this check got wrong: it put the ordering rule
    /// inside the EPUB 2 arm on the assumption that only OPF 2.0.1's grammar
    /// is a sequence. Both are, so EPUB 3 was left silent on a file epubcheck
    /// reports. Probing the case is what found it - the assumption read fine.
    #[test]
    fn cipher_data_shape_and_ordering_do_not_vary_with_the_version() {
        const SWAPPED: &str = "<enc:EncryptedData><enc:CipherData>\
             <enc:CipherReference URI=\"OEBPS/f.otf\"/></enc:CipherData>\
             <enc:EncryptionMethod Algorithm=\"http://www.idpf.org/2008/embedding\"/>\
             </enc:EncryptedData>";
        const EMPTY_CIPHER: &str = "<enc:EncryptedData>\
             <enc:EncryptionMethod Algorithm=\"http://www.idpf.org/2008/embedding\"/>\
             <enc:CipherData></enc:CipherData></enc:EncryptedData>";

        for v in [Some(true), Some(false), None] {
            let swapped: Vec<_> = findings_v(SWAPPED, v).into_iter().map(|(r, _)| r).collect();
            assert!(
                swapped.contains(&"ocf.encryption.item_out_of_order"),
                "a CipherData before the EncryptionMethod is an error at every version ({v:?})"
            );
            let empty: Vec<_> = findings_v(EMPTY_CIPHER, v)
                .into_iter()
                .map(|(r, _)| r)
                .collect();
            assert_eq!(
                empty,
                vec!["ocf.encryption.cipher_data_incomplete"],
                "an empty CipherData needs a CipherReference or CipherValue ({v:?})"
            );
        }
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
mod book_budget_tests {
    use super::{Ocf, check_resource_limits};
    use crate::ids::LIM_002;
    use crate::report::Report;
    use std::io::Write;

    fn container(entries: &[(&str, usize)]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        z.start_file("mimetype", stored).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        for (name, len) in entries {
            z.start_file(*name, stored).unwrap();
            z.write_all(&vec![b'x'; *len]).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    /// The budget is charged once per entry on its first read; once it is
    /// spent, a new entry is refused and reported by name, and an entry
    /// already read stays readable.
    #[test]
    fn a_spent_budget_refuses_new_entries_only() {
        let bytes = container(&[("a", 10), ("b", 10), ("c", 10)]);
        let mut report = Report::default();
        let mut ocf: Ocf = super::open(bytes, &mut report).expect("opens");
        // `open` has read `mimetype` already; the budget counts from there.
        ocf.book_budget = ocf.read_total + 15;
        assert!(ocf.read("a").is_some());
        assert!(ocf.read("a").is_some(), "a second read is free");
        assert!(ocf.read("b").is_some(), "10 more read, under 15: admitted");
        assert!(ocf.read("c").is_none(), "20 more read, past 15: refused");
        assert!(ocf.read("a").is_some(), "already read, still readable");
        assert!(ocf.read("c").is_none());
        assert!(ocf.read("absent").is_none());
        let mut report = Report::default();
        check_resource_limits(&ocf, &mut report);
        let hits: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.id == LIM_002)
            .map(|m| m.location.clone())
            .collect();
        assert_eq!(hits.len(), 1, "c alone, once, by name: {hits:?}");
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

#[cfg(test)]
mod unreadable_header_tests {
    use crate::report::Report;

    /// A local file header naming `name` with an extra field of `extra`
    /// bytes, then padding: long enough for epubcheck's 58-byte read, with no
    /// end-of-central-directory record, so no archive reader can open it.
    fn broken(sig: &[u8; 4], name: &[u8], extra: &[u8]) -> Vec<u8> {
        let mut b = sig.to_vec();
        b.extend([10, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        b.extend([0; 4]);
        b.extend(20u32.to_le_bytes());
        b.extend(20u32.to_le_bytes());
        b.extend((name.len() as u16).to_le_bytes());
        b.extend((extra.len() as u16).to_le_bytes());
        b.extend(name);
        b.extend(extra);
        b.extend(b"application/epub+zip");
        b.extend([0; 80]);
        b
    }

    fn ids(bytes: Vec<u8>) -> Vec<&'static str> {
        let mut report = Report::new();
        assert!(super::open(bytes, &mut report).is_none(), "must not open");
        let mut v: Vec<_> = report.messages.iter().map(|m| m.id).collect();
        v.sort();
        v
    }

    /// Each case measured against epubcheck 5.4.0, one file apiece: the raw
    /// header is judged even when the archive cannot be read.
    #[test]
    fn a_container_that_will_not_open_still_has_its_header_judged() {
        let pk = b"PK\x03\x04";
        assert_eq!(ids(broken(pk, b"mimetypeX", b"")), ["PKG-006", "PKG-008"]);
        assert_eq!(ids(broken(pk, b"mimetypX", b"")), ["PKG-006", "PKG-008"]);
        assert_eq!(
            ids(broken(pk, b"mimetype", b"\x01\x02\x03\x04")),
            ["PKG-005", "PKG-008"]
        );
        assert_eq!(ids(broken(pk, b"mimetype", b"")), ["PKG-008"]);
        // epubcheck tests `header[0] != 'P' && header[1] != 'K'`, so one
        // matching byte of the signature is enough to reach the rest of the
        // chain; "PX" is judged on its name like any "PK" header.
        assert_eq!(
            ids(broken(b"PX\x03\x04", b"mimetypeX", b"")),
            ["PKG-006", "PKG-008"]
        );
    }

    /// A two-entry book, with `patch` applied to its second central-directory
    /// header. What Java says is recorded from OpenJDK 27 through epubcheck 5.4.0.
    fn java_says(patch: impl Fn(&mut Vec<u8>, usize)) -> Option<String> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
        let mut w = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();
        w.start_file("a.xhtml", deflated).unwrap();
        w.write_all(b"<a/>").unwrap();
        let mut d = w.finish().unwrap().into_inner();
        let second = d.windows(4).rposition(|b| b == b"PK\x01\x02").unwrap();
        patch(&mut d, second);
        crate::ocf::java_end_header_error(&d)
    }

    /// Grow the central directory by `data`, placed after the entry's name.
    fn insert_after_name(d: &mut Vec<u8>, entry: usize, data: &[u8]) {
        let name = usize::from(u16::from_le_bytes([d[entry + 28], d[entry + 29]]));
        let at = entry + 46 + name;
        d.splice(at..at, data.iter().copied());
        let end = d.windows(4).rposition(|b| b == b"PK\x05\x06").unwrap();
        let size = u32::from_le_bytes([d[end + 12], d[end + 13], d[end + 14], d[end + 15]]);
        d[end + 12..end + 16].copy_from_slice(&(size + data.len() as u32).to_le_bytes());
    }

    #[test]
    fn the_central_directory_is_judged_as_java_judges_it() {
        assert_eq!(java_says(|_, _| {}), None);
        let says = |patch: fn(&mut Vec<u8>, usize), want: &str| {
            assert_eq!(java_says(patch).as_deref(), Some(want));
        };
        says(|d, e| d[e + 3] = 9, "invalid CEN header (bad signature)");
        says(|d, e| d[e + 8] |= 1, "invalid CEN header (encrypted entry)");
        says(
            |d, e| d[e + 10] = 99,
            "invalid CEN header (bad compression method: 99)",
        );
        says(
            |d, e| d[e + 29] = 0xEE,
            "invalid CEN header (bad header size)",
        );
        says(
            |d, e| d[e + 46] = 0xE9,
            "invalid CEN header (bad entry name or comment)",
        );
        says(
            |d, e| {
                d[e + 32] = 1;
                insert_after_name(d, e, &[0xE9]);
            },
            "invalid CEN header (bad entry name or comment)",
        );
        // Capitalised, so PKG-008 and not PKG-027.
        says(
            |d, e| d[e + 24..e + 28].copy_from_slice(&[0xFF; 4]),
            "Invalid CEN header (invalid zip64 extra len size)",
        );
        says(
            |d, e| {
                d[e + 30] = 9;
                insert_after_name(d, e, &[1, 0, 5, 0, 0, 0, 0, 0, 0]);
            },
            "Invalid CEN header (invalid zip64 extra data field size)",
        );
        // A short trailing extra is ignored.
        assert_eq!(
            java_says(|d, e| {
                d[e + 30] = 3;
                insert_after_name(d, e, &[1, 2, 3]);
            }),
            None
        );
    }
}

/// `container.xml` as epubcheck judges it: the schema for the version the
/// book is validated as, its Schematron rules, and the conditions under which
/// either runs. Every case here was measured against epubcheck 5.4.0, one
/// book apiece, on books otherwise clean in both tools.
#[cfg(test)]
mod container_document_tests {
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    const NS: &str = "xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" \
                      xmlns:rendition=\"http://www.idpf.org/2013/rendition\"";
    const RF: &str =
        "<rootfile full-path=\"OEBPS/p.opf\" media-type=\"application/oebps-package+xml\"/>";

    fn book(version: &str, container: &str, extra: &[(&str, &str)]) -> Vec<u8> {
        let opf = if version == "3.0" {
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
             <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\
             <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
             <dc:identifier id=\"id\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>\
             <dc:title>T</dc:title><dc:language>en</dc:language>\
             <meta property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta></metadata>\
             <manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
             <item id=\"c\" href=\"c.xhtml\" media-type=\"application/xhtml+xml\"/></manifest>\
             <spine><itemref idref=\"c\"/></spine></package>"
        } else {
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
             <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"2.0\" unique-identifier=\"id\">\
             <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
             <dc:identifier id=\"id\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>\
             <dc:title>T</dc:title><dc:language>en</dc:language></metadata>\
             <manifest><item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\
             <item id=\"c\" href=\"c.xhtml\" media-type=\"application/xhtml+xml\"/></manifest>\
             <spine toc=\"ncx\"><itemref idref=\"c\"/></spine></package>"
        };
        const NAV: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
            <head><title>n</title></head><body><nav epub:type=\"toc\"><ol>\
            <li><a href=\"c.xhtml\">c</a></li></ol></nav></body></html>";
        const NCX: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><head>\
            <meta name=\"dtb:uid\" content=\"urn:uuid:12345678-1234-1234-1234-123456789abc\"/></head>\
            <docTitle><text>T</text></docTitle><navMap><navPoint id=\"n\" playOrder=\"1\">\
            <navLabel><text>c</text></navLabel><content src=\"c.xhtml\"/></navPoint></navMap></ncx>";
        let ch = if version == "3.0" {
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html>\
             <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>c</title></head>\
             <body><p>c</p></body></html>"
        } else {
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
             <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>c</title></head>\
             <body><p>c</p></body></html>"
        };
        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            let mut files = vec![
                ("META-INF/container.xml", container),
                ("OEBPS/p.opf", opf),
                ("OEBPS/c.xhtml", ch),
                if version == "3.0" {
                    ("OEBPS/nav.xhtml", NAV)
                } else {
                    ("OEBPS/toc.ncx", NCX)
                },
            ];
            files.extend_from_slice(extra);
            for (name, body) in files {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    fn ids(version: &str, container: &str, extra: &[(&str, &str)]) -> Vec<&'static str> {
        let mut v: Vec<_> = crate::validate_bytes(book(version, container, extra))
            .messages
            .iter()
            .map(|m| m.id)
            .collect();
        v.sort();
        v
    }

    fn c(attrs: &str, body: &str) -> String {
        format!("<?xml version=\"1.0\"?><container {attrs} {NS}>{body}</container>")
    }

    #[test]
    fn the_baselines_are_clean_in_both_versions() {
        let x = c("version=\"1.0\"", &format!("<rootfiles>{RF}</rootfiles>"));
        assert_eq!(ids("3.0", &x, &[]), Vec::<&str>::new());
        assert_eq!(ids("2.0", &x, &[]), Vec::<&str>::new());
    }

    /// EPUB 3 fixes `version` at "1.0"; OCF 2.0.1's schema takes any value.
    /// Neither takes a missing one.
    #[test]
    fn the_version_attribute_is_judged_by_the_books_version() {
        let v2 = c("version=\"2.0\"", &format!("<rootfiles>{RF}</rootfiles>"));
        assert_eq!(ids("3.0", &v2, &[]), ["RSC-005"]);
        assert_eq!(ids("2.0", &v2, &[]), Vec::<&str>::new());
        let none = c("", &format!("<rootfiles>{RF}</rootfiles>"));
        assert_eq!(ids("3.0", &none, &[]), ["RSC-005"]);
        assert_eq!(ids("2.0", &none, &[]), ["RSC-005"]);
    }

    /// Structure the schema owns, which nothing reported before.
    #[test]
    fn the_schema_reports_what_the_content_model_forbids() {
        let rf_extra = RF.replace("/>", " foo=\"x\"/>");
        for body in [
            format!("<rootfiles>{rf_extra}</rootfiles>"),
            format!("<rootfiles foo=\"x\">{RF}</rootfiles>"),
            format!("<rootfiles>hello{RF}</rootfiles>"),
            format!("<rootfiles>{RF}</rootfiles><links/>"),
            format!("<rootfiles>{RF}</rootfiles><links><link href=\"x.xml\"/></links>"),
        ] {
            let x = c("version=\"1.0\"", &body);
            assert_eq!(ids("3.0", &x, &[]), ["RSC-005"], "{body}");
            assert_eq!(ids("2.0", &x, &[]), ["RSC-005"], "{body}");
        }
        let bad_layout = RF.replace("/>", " rendition:layout=\"bogus\"/>");
        let x = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{bad_layout}</rootfiles>"),
        );
        assert_eq!(ids("3.0", &x, &[]), ["RSC-005"]);
    }

    /// EPUB 3.3 § 4.2.6.3.1 and OCF 2.0.1 § 3.5.1: valid "after removing all
    /// elements and attributes from other namespaces". epubcheck 5.4.0
    /// reports these as RSC-005 (w3c/epubcheck#1754); we follow the
    /// specifications. An unqualified unknown attribute is still an error, a
    /// foreign element's content goes with it, and at EPUB 2 the rendition
    /// namespace is just another namespace.
    #[test]
    fn elements_and_attributes_from_other_namespaces_are_removed_first() {
        const EXT: &str = "xmlns:ext=\"https://example.org/ns\"";
        let rf_ext = RF.replace("/>", " ext:note=\"x\" xml:lang=\"en\"/>");
        for body in [
            format!("<rootfiles>{RF}</rootfiles><ext:note>hi<rootfile/></ext:note>"),
            format!("<ext:note/><rootfiles ext:note=\"x\">{RF}<ext:n/></rootfiles>"),
            format!("<rootfiles>{rf_ext}</rootfiles>"),
            format!(
                "<rootfiles>{RF}</rootfiles><links><ext:x/>\
                     <link href=\"x.xml\" rel=\"r\" ext:a=\"1\"/></links>"
            ),
        ] {
            let x = c(&format!("version=\"1.0\" ext:v=\"1\" {EXT}"), &body);
            assert_eq!(ids("3.0", &x, &[]), Vec::<&str>::new(), "{body}");
            if !body.contains("<links>") {
                assert_eq!(ids("2.0", &x, &[]), Vec::<&str>::new(), "{body}");
            }
        }
        let x = c(
            &format!("version=\"1.0\" {EXT}"),
            &format!(
                "<rootfiles><ext:n/>{}</rootfiles>",
                RF.replace("/>", " foo=\"x\"/>")
            ),
        );
        assert_eq!(ids("3.0", &x, &[]), ["RSC-005"]);
        let bad_layout = RF.replace("/>", " rendition:layout=\"bogus\"/>");
        let x = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{bad_layout}</rootfiles>"),
        );
        assert_eq!(ids("2.0", &x, &[]), Vec::<&str>::new());
        // Removing the only rootfile leaves nothing to validate: the package
        // document is never found.
        let x = c(
            &format!("version=\"1.0\" {EXT}"),
            "<rootfiles><ext:rootfile full-path=\"OEBPS/p.opf\" \
             media-type=\"application/oebps-package+xml\"/></rootfiles>",
        );
        assert_eq!(ids("3.0", &x, &[]), ["RSC-003"]);
    }

    /// SPEC-001: EPUB 3.3 § 4.2.6.3.2.1 and OCF 2.0.1 § 3.5.2 forbid
    /// encrypting `mimetype`, the six `META-INF` files and the package
    /// document; epubcheck 5.4.0 checks none of them. One finding per entry:
    /// an obfuscated package document is SPEC-001, not PKG-026 as well. A
    /// missing target stays RSC-007 alone.
    #[test]
    fn a_file_that_must_never_be_encrypted_is_spec_001() {
        let x = c("version=\"1.0\"", &format!("<rootfiles>{RF}</rootfiles>"));
        let enc = |uri: &str, alg: &str| {
            format!(
                "<?xml version=\"1.0\"?><encryption \
                 xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" \
                 xmlns:enc=\"http://www.w3.org/2001/04/xmlenc#\">\
                 <enc:EncryptedData><enc:EncryptionMethod Algorithm=\"{alg}\"/>\
                 <enc:CipherData><enc:CipherReference URI=\"{uri}\"/></enc:CipherData>\
                 </enc:EncryptedData></encryption>"
            )
        };
        const OBF: &str = "http://www.idpf.org/2008/embedding";
        const AES: &str = "http://www.w3.org/2001/04/xmlenc#aes256-cbc";
        for version in ["3.0", "2.0"] {
            for uri in ["OEBPS/p.opf", "META-INF/container.xml", "mimetype"] {
                let e = enc(uri, AES);
                let got = ids(version, &x, &[("META-INF/encryption.xml", &e)]);
                assert_eq!(got, ["SPEC-001"], "{version} {uri}");
            }
            let e = enc("OEBPS/p.opf", OBF);
            let got = ids(version, &x, &[("META-INF/encryption.xml", &e)]);
            assert!(got.contains(&"SPEC-001"), "{version} {got:?}");
            assert!(!got.contains(&"PKG-026"), "{version} {got:?}");
            let e = enc("META-INF/rights.xml", AES);
            let got = ids(version, &x, &[("META-INF/encryption.xml", &e)]);
            assert_eq!(got, ["RSC-007"], "{version}");
        }
    }

    /// A wrong root is one RSC-005, and the entries are still read, as
    /// epubcheck's handler does: the font stays encrypted (RSC-004) and is not
    /// parsed.
    #[test]
    fn a_wrong_encryption_root_still_registers_its_entries() {
        let x = c("version=\"1.0\"", &format!("<rootfiles>{RF}</rootfiles>"));
        let e = "<?xml version=\"1.0\"?><encryptions \
                 xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" \
                 xmlns:enc=\"http://www.w3.org/2001/04/xmlenc#\">\
                 <enc:EncryptedData><enc:EncryptionMethod \
                 Algorithm=\"http://www.w3.org/2001/04/xmlenc#aes256-cbc\"/>\
                 <enc:CipherData><enc:CipherReference URI=\"OEBPS/c.xhtml\"/>\
                 </enc:CipherData></enc:EncryptedData></encryptions>";
        let book = book("3.0", &x, &[("META-INF/encryption.xml", e)]);
        let r = crate::validate_bytes(book);
        let mut got: Vec<_> = r.messages.iter().map(|m| m.id).collect();
        got.sort();
        assert_eq!(got, ["RSC-004", "RSC-005"], "{:?}", r.messages);
    }

    /// The handler reads only container-namespace rootfiles, so a document in
    /// another namespace declares no package document: RSC-003, and the schema
    /// never runs. A missing package file is OPF-002 and stops it too.
    #[test]
    fn the_schema_runs_only_once_a_package_document_is_found() {
        let wrong_ns = format!(
            "<?xml version=\"1.0\"?><container version=\"2.0\" xmlns=\"urn:example:x\">\
             <rootfiles>{RF}</rootfiles></container>"
        );
        assert_eq!(ids("3.0", &wrong_ns, &[]), ["RSC-003"]);
        let gone = c(
            "version=\"2.0\"",
            "<rootfiles><rootfile full-path=\"OEBPS/gone.opf\" \
             media-type=\"application/oebps-package+xml\"/></rootfiles>",
        );
        assert_eq!(ids("3.0", &gone, &[]), ["OPF-002"]);
    }

    /// `rootfile[position() > 1]`: per `rootfiles`, over every rootfile. Two
    /// `rootfiles` elements are a schema error and no RSC-017; a second
    /// rootfile without a package media type still needs a selection
    /// attribute.
    #[test]
    fn rsc_017_counts_rootfiles_within_their_parent() {
        let two_parents = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{RF}</rootfiles><rootfiles>{RF}</rootfiles>"),
        );
        let got = ids("3.0", &two_parents, &[]);
        assert!(got.contains(&"RSC-005"), "{got:?}");
        assert!(!got.contains(&"RSC-017"), "{got:?}");
        let untyped_second = c(
            "version=\"1.0\"",
            &format!(
                "<rootfiles>{RF}<rootfile full-path=\"OEBPS/p.opf\" media-type=\"\"/></rootfiles>"
            ),
        );
        assert_eq!(ids("3.0", &untyped_second, &[]), ["RSC-005", "RSC-017"]);
    }

    /// The Schematron rules run on a single-rendition book too, and a
    /// mapping link is the one whose whole `rel` is `mapping`.
    #[test]
    fn the_container_schematron_runs_on_every_epub3_book() {
        let dup = RF.replace("/>", " rendition:accessMode=\"textual textual\"/>");
        let x = c("version=\"1.0\"", &format!("<rootfiles>{dup}</rootfiles>"));
        assert_eq!(ids("3.0", &x, &[]), ["RSC-005"]);
        const MAP: &str = "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"/>";
        let link = |rel: &str, mt: &str| {
            format!("<link href=\"map.xhtml\" rel=\"{rel}\" media-type=\"{mt}\"/>")
        };
        let html = link("mapping", "text/html");
        let x = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{RF}</rootfiles><links>{html}</links>"),
        );
        assert_eq!(ids("3.0", &x, &[("map.xhtml", MAP)]), ["RSC-005"]);
        let xh = link("mapping", "application/xhtml+xml");
        let x = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{RF}</rootfiles><links>{xh}{xh}</links>"),
        );
        assert_eq!(ids("3.0", &x, &[("map.xhtml", MAP)]), ["RSC-005"]);
        // `rel="mapping foo"` is no mapping link: its target is undeclared.
        let foo = link("mapping foo", "text/html");
        let x = c(
            "version=\"1.0\"",
            &format!("<rootfiles>{RF}</rootfiles><links>{foo}{foo}</links>"),
        );
        assert_eq!(ids("3.0", &x, &[("map.xhtml", MAP)]), ["OPF-003"]);
    }
}
