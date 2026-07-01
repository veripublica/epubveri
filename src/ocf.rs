//! OCF (Open Container Format) layer: the EPUB ZIP, the `mimetype` rules,
//! and locating the OPF package document via `META-INF/container.xml`.

use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::ids::*;
use crate::report::{Report, Severity};

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

/// Open the ZIP and run the OCF `mimetype` checks (PKG-003/006/007).
/// Returns `None` only when the bytes are not a usable ZIP at all.
pub fn open(bytes: Vec<u8>, report: &mut Report) -> Option<Ocf> {
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => {
            report.push(
                PKG_004,
                Severity::Error,
                format!("Not a valid EPUB container (ZIP could not be read): {e}"),
            );
            return None;
        }
    };

    let n = archive.len();
    let mut names = Vec::with_capacity(n);
    let mut first_name = String::new();
    let mut first_stored = false;
    for i in 0..n {
        if let Ok(f) = archive.by_index(i) {
            let name = f.name().to_string();
            if i == 0 {
                first_name = name.clone();
                first_stored = f.compression() == zip::CompressionMethod::Stored;
            }
            names.push(name);
        }
    }

    // mimetype: must be the first entry, stored (uncompressed), exact contents.
    if first_name != "mimetype" {
        report.push(
            PKG_006,
            Severity::Error,
            "The 'mimetype' file must be the first entry in the EPUB ZIP",
        );
    } else if !first_stored {
        report.push(
            PKG_007,
            Severity::Error,
            "The 'mimetype' file must be stored uncompressed",
        );
    }

    let mut ocf = Ocf { archive, names };

    if ocf.has("mimetype") {
        if let Some(b) = ocf.read("mimetype") {
            if b != b"application/epub+zip" {
                report.push(
                    PKG_007,
                    Severity::Error,
                    format!(
                        "'mimetype' must contain exactly 'application/epub+zip' (found {:?})",
                        String::from_utf8_lossy(&b)
                    ),
                );
            }
        }
    }

    Some(ocf)
}

/// Parse `META-INF/container.xml` and return the OPF full-path (RSC-002/003/005).
pub fn find_rootfile(ocf: &mut Ocf, report: &mut Report) -> Option<String> {
    const CONTAINER: &str = "META-INF/container.xml";
    if !ocf.has(CONTAINER) {
        report.push(
            RSC_002,
            Severity::Error,
            "Required META-INF/container.xml is missing",
        );
        return None;
    }

    let bytes = ocf.read(CONTAINER)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = match parse_xml(&text) {
        Ok(d) => d,
        Err(e) => {
            report.push_at(
                RSC_005,
                Severity::Error,
                format!("META-INF/container.xml is not well-formed XML: {e}"),
                CONTAINER,
            );
            return None;
        }
    };

    // RSC-003: need a <rootfile> with the OPF media type and a full-path.
    let rootfile = doc.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "rootfile"
            && n.attribute("media-type") == Some("application/oebps-package+xml")
    });
    match rootfile.and_then(|n| n.attribute("full-path")) {
        Some(p) if !p.is_empty() => Some(p.to_string()),
        _ => {
            report.push_at(
                RSC_003,
                Severity::Error,
                "container.xml has no <rootfile> with media-type \
                 'application/oebps-package+xml' and a full-path (OPF location)",
                CONTAINER,
            );
            None
        }
    }
}

/// If `META-INF/encryption.xml` is present, report each encrypted resource as
/// RSC-004 (INFO) — its content is not validated.
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
            report.push_at(
                RSC_005,
                Severity::Error,
                format!("META-INF/encryption.xml is not well-formed XML: {e}"),
                ENC,
            );
            return;
        }
    };
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "CipherReference")
    {
        if let Some(uri) = n.attribute("URI") {
            report.push(
                RSC_004,
                Severity::Info,
                format!("File \"{uri}\" is encrypted; its content will not be checked"),
            );
        }
    }
}
