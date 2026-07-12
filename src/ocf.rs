//! OCF (Open Container Format) layer: the EPUB ZIP, the `mimetype` rules,
//! and locating the OPF package document via `META-INF/container.xml`.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::ids::*;
use crate::report::{Position, Report, Severity};

/// Parse an XML document, allowing a `<!DOCTYPE>` declaration. Real-world
/// EPUB content documents commonly have one (e.g. `<!DOCTYPE html>`);
/// roxmltree rejects any DTD by default as an extra security precaution, but
/// it already has its own billion-laughs protection regardless of this flag,
/// so allowing (harmless, common) DOCTYPEs through is safe.
pub(crate) fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, roxmltree::Error> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(text, opts)
}

/// Whether a `roxmltree` parse error is an entity-reference problem — an
/// undeclared named entity (`&nbsp;` with no DTD declaration) or a malformed
/// one (missing `;`). These are reported separately, and earlier, by
/// `htm::check_raw`'s raw-text entity scanner (`check_entities`), so when a
/// document fails to parse *for this reason* it must NOT also be re-reported
/// as a generic well-formedness `RSC-016` — that would double up two Fatals
/// on the one defect. Any other parse failure (mismatched/unclosed tags,
/// stray `<`, …) is a genuine well-formedness error nothing else catches.
pub(crate) fn is_entity_reference_error(err: &roxmltree::Error) -> bool {
    matches!(
        err,
        roxmltree::Error::UnknownEntityReference(..)
            | roxmltree::Error::MalformedEntityReference(..)
    )
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
}

impl Ocf {
    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Read a member's bytes (decompressing as needed).
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut f = self.archive.by_name(name).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        Some(buf)
    }
}

/// Open the ZIP and run the OCF `mimetype` checks (PKG-003/005/006/007/008).
/// Returns `None` only when the bytes are not a usable ZIP at all - a
/// fatal condition, so nothing else is checked (confirmed via real
/// fixtures pairing PKG-008 alone with "no other errors or warnings").
pub fn open(bytes: Vec<u8>, report: &mut Report) -> Option<Ocf> {
    if bytes.is_empty() {
        report.push(PKG_003, Severity::Error, "the EPUB file is empty");
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
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => {
            if looks_like_other_format {
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
                first_has_extra = f.extra_data().is_some_and(|d| !d.is_empty());
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
        report.push(
            PKG_006,
            Severity::Error,
            "The 'mimetype' file must be the first entry in the EPUB ZIP",
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

    let mut ocf = Ocf { archive, names };

    if ocf.has("mimetype") {
        if let Some(b) = ocf.read("mimetype") {
            if b != b"application/epub+zip" {
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
        }
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
                format!("META-INF/container.xml is not well-formed XML: {e}"),
                CONTAINER,
                Position::of_parse_error(&e),
            );
            return Vec::new();
        }
    };

    // RSC-003: need at least one <rootfile> with the OPF media type and a full-path.
    let paths: Vec<String> = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "rootfile"
                && n.attribute("media-type") == Some("application/oebps-package+xml")
        })
        .filter_map(|n| n.attribute("full-path"))
        .filter(|p| !p.is_empty())
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
            report.push_full(
                RSC_005,
                Severity::Error,
                format!("element \"{}\" not allowed here", child.tag_name().name()),
                CONTAINER,
                Position::of(child),
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
                format!("META-INF/encryption.xml is not well-formed XML: {e}"),
                ENC,
                Position::of_parse_error(&e),
            );
            return;
        }
    };

    if doc.root_element().tag_name().name() != "encryption" {
        report.push_full(
            RSC_005,
            Severity::Error,
            "expected element \"encryption\" as the root of META-INF/encryption.xml",
            ENC,
            Position::of(doc.root_element()),
            "ocf.encryption.wrong_root_element",
            Vec::new(),
        );
        return;
    }

    // Every `Id` attribute (on any element - EncryptedKey/EncryptedData/...)
    // must be unique across the whole document; a value shared by more
    // than one element is reported once *per element* sharing it
    // (confirmed via a real 2-element-sharing-1-id fixture expecting
    // exactly 2 findings, not 1).
    let mut by_id: HashMap<&str, u32> = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attribute("Id") {
            *by_id.entry(id).or_insert(0) += 1;
        }
    }
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attribute("Id") {
            if by_id.get(id).copied().unwrap_or(0) > 1 {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    format!("Duplicate \"Id\" value '{id}'"),
                    ENC,
                    Position::of(n),
                    "ocf.encryption.duplicate_id",
                    vec![id.to_string()],
                );
            }
        }
    }

    // The IDPF compression extension's <Compression Method="0|8"
    // OriginalLength="<non-negative integer>"/>.
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Compression")
    {
        if let Some(method) = n.attribute("Method") {
            if !matches!(method, "0" | "8") {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "value of attribute \"Method\" is invalid",
                    ENC,
                    Position::of(n),
                    "ocf.encryption.invalid_compression_method",
                    vec![method.to_string()],
                );
            }
        }
        if let Some(len) = n.attribute("OriginalLength") {
            if len.is_empty() || !len.bytes().all(|b| b.is_ascii_digit()) {
                report.push_full(
                    RSC_005,
                    Severity::Error,
                    "value of attribute \"OriginalLength\" is invalid",
                    ENC,
                    Position::of(n),
                    "ocf.encryption.invalid_original_length",
                    vec![len.to_string()],
                );
            }
        }
    }

    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "CipherReference")
    {
        if let Some(uri) = n.attribute("URI") {
            report.push_at_pos(
                RSC_004,
                Severity::Info,
                format!("File \"{uri}\" is encrypted; its content will not be checked"),
                ENC,
                Position::of(n),
            );
        }
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
                format!("META-INF/signatures.xml is not well-formed XML: {e}"),
                SIG,
                Position::of_parse_error(&e),
            );
            return;
        }
    };
    if doc.root_element().tag_name().name() != "signatures" {
        report.push_full(
            RSC_005,
            Severity::Error,
            "expected element \"signatures\" as the root of META-INF/signatures.xml",
            SIG,
            Position::of(doc.root_element()),
            "ocf.signatures.wrong_root_element",
            Vec::new(),
        );
    }
}
