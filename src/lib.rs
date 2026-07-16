//! epubveri — a pure-Rust EPUB validator.
//!
//! A small, fast, JVM-free, embeddable alternative to epubcheck. It combines
//! hand-coded structural checks (OCF/mimetype, container, OPF metadata,
//! manifest/spine integrity, broken references, EPUB 3 navigation) with
//! RELAX NG, XPath, and Schematron engines for the XHTML/SVG content model,
//! and reports findings with epubcheck-style message IDs (`RSC-…`, `OPF-…`,
//! `HTM-…`, …). A WebAssembly build ships separately as the `epubveri-wasm`
//! crate.

/// The crate version, carrying git build metadata (`+<short-hash>[.dirty]`)
/// when built from a checkout — the one string the CLI's `-V`, this crate's
/// embedders, and the wasm binding's `version()` all print (veripublica
/// conventions v0.4, CLI.md §3.1). A build with no git (e.g. a crates.io
/// tarball) falls back silently to the plain SemVer, set by `build.rs`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), env!("EPUBVERI_BUILD"));

pub mod cmt;
pub mod css;
pub mod dict;
pub mod edupub;
pub mod envelope;
pub mod filename;
pub mod foreign;
pub mod htm;
pub mod ids;
pub mod image;
pub mod indexes;
pub mod layout;
pub mod mathml;
pub mod navdoc;
pub mod ncx;
pub mod ocf;
pub mod opf;
pub mod previews;
pub mod regionnav;
pub mod renditions;
pub mod report;
pub mod rng;
pub mod schematron;
pub mod smil;
pub mod ssv;
pub mod svg;
pub mod url;
pub mod xmlext;
use crate::xmlext::NodeExt;
pub mod xpath;

use std::path::Path;

use report::Report;

/// A quick, non-reporting peek at an OPF's own declared `version`
/// attribute - used only to decide whether multiple rootfiles are the
/// legitimate EPUB 3 Multiple Renditions feature or an EPUB 2 error
/// (PKG-013); the real, fully-reporting parse happens in `opf::check`.
fn peek_opf_version(ocf: &mut ocf::Ocf, opf_path: &str) -> Option<String> {
    let bytes = ocf.read(opf_path)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let doc = ocf::parse_xml(&text).ok()?;
    doc.root_element().attr_no_ns("version").map(String::from)
}

/// Validate raw EPUB bytes and return a [`Report`].
pub fn validate_bytes(bytes: Vec<u8>) -> Report {
    validate_bytes_with_profile(bytes, None)
}

/// Validate raw EPUB bytes under a specific EPUB extension-spec profile,
/// matching real epubcheck's `--profile <name>` CLI flag (`"dict"`,
/// `"edupub"`, `"idx"`, `"preview"`, or `None`/anything else for default
/// behavior). A profile only ever *forces the "this publication must
/// declare itself as X" gating check* for a book that would otherwise be
/// silently treated as a plain, unrelated publication - it never
/// overrides or duplicates the checks a real `dc:type`/content-based
/// declaration already triggers on its own. Unrecognized profile names
/// are accepted and simply behave like `None` (permissive, matching the
/// project's general design principle: this project doesn't second-
/// guess or reject its own inputs).
pub fn validate_bytes_with_profile(bytes: Vec<u8>, profile: Option<&str>) -> Report {
    let mut report = Report::new();
    let mut container = match ocf::open(bytes, &mut report) {
        Some(c) => c,
        None => return report,
    };
    ocf::check_encryption(&mut container, &mut report);
    ocf::check_signatures(&mut container, &mut report);
    let opf_paths = ocf::find_rootfiles(&mut container, &mut report);
    // Usually a single rootfile; a multi-rendition package (e.g. EDUPUB
    // with a reflowable + fixed-layout rendition) legitimately declares
    // more than one, each validated as its own, independent OPF.
    for opf_path in &opf_paths {
        opf::check(&mut container, opf_path, profile, &mut report);
    }
    // Checked once for the whole publication (not per-rendition): the
    // multi-rendition dc:type cardinality cross-check reads
    // META-INF/metadata.xml, which no single opf::check call ever sees.
    if opf_paths.len() > 1 {
        // Multiple Renditions is an EPUB 3-only feature - a real EPUB 2
        // fixture with two rootfiles (both declaring version="2.0")
        // expects only PKG-013, none of the multi-rendition machinery
        // below (which doesn't apply to EPUB 2 at all).
        let all_epub3 = opf_paths
            .iter()
            .all(|p| peek_opf_version(&mut container, p).is_some_and(|v| v.starts_with('3')));
        if all_epub3 {
            edupub::check_multi_rendition_dc_type(&mut container, &opf_paths, &mut report);
            renditions::check(&mut container, &mut report);
        } else {
            report.push(
                ids::PKG_013,
                report::Severity::Error,
                "container.xml declares more than one rootfile outside of EPUB 3 Multiple Renditions",
            );
        }
    }
    // Bound the RNG engine's pattern-interning cache (see
    // `rng::pattern::clear_intern_cache`) to roughly one book's working set,
    // rather than letting it grow for the life of a long-lived embedded
    // process validating many books.
    rng::clear_intern_cache();
    report
}

/// Validate an EPUB file on disk.
pub fn validate_path(path: &Path) -> std::io::Result<Report> {
    validate_path_with_profile(path, None)
}

/// Validate an EPUB file on disk under a specific EPUB extension-spec
/// profile - see [`validate_bytes_with_profile`].
pub fn validate_path_with_profile(path: &Path, profile: Option<&str>) -> std::io::Result<Report> {
    let mut report = validate_bytes_with_profile(std::fs::read(path)?, profile);
    // PKG-016: the file's own ".epub" extension should be lowercase - a
    // filesystem-level concern `validate_bytes` alone can't see, since it
    // only ever receives raw bytes with no filename attached.
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && ext != "epub"
        && ext.eq_ignore_ascii_case("epub")
    {
        report.push(
            ids::PKG_016,
            report::Severity::Warning,
            "the file extension should be lowercase \".epub\"",
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    /// Builds a minimal, otherwise-valid EPUB 2 in memory whose one content
    /// document has the issue-#23 shape: an XHTML 1.1 DOCTYPE (so `&nbsp;`
    /// is declared by the DTD it references), a `&nbsp;` in the text, and an
    /// `id` the NCX points at. Sigil writes exactly this by default.
    fn epub2_with_dtd_entities(title: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            z.start_file("mimetype", stored).unwrap();
            z.write_all(b"application/epub+zip").unwrap();

            let opts = zip::write::SimpleFileOptions::default();
            let files: &[(&str, &str)] = &[
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
                ),
                (
                    "OEBPS/content.opf",
                    r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:x</dc:identifier>
    <dc:title>T</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="s2" href="Text/Section0002.htm" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="s2"/></spine>
</package>"#,
                ),
                (
                    "OEBPS/toc.ncx",
                    r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:x"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>C1</text></navLabel>
      <content src="Text/Section0002.htm#sigil_toc_id_3"/>
    </navPoint>
  </navMap>
</ncx>"#,
                ),
            ];
            let content_doc = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n\
  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
<head><title>{title}</title></head>\n\
<body>\n\
<h1 class=\"MsoNormal\" id=\"sigil_toc_id_3\">Chapter&nbsp;One</h1>\n\
</body>\n\
</html>\n"
            );
            for (name, body) in files
                .iter()
                .copied()
                .chain([("OEBPS/Text/Section0002.htm", content_doc.as_str())])
            {
                z.start_file(name, opts).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// Issue #23, the half that invents findings. The NCX fragment
    /// resolves - the `id` is right there on the `<h1>` - so no RSC-012 may
    /// be reported. Before the fix `&nbsp;` failed the parse, the id map
    /// came back empty via an `unwrap_or_default()`, and every fragment
    /// pointing into the document was called undefined: 1079 invented
    /// errors across a real 171-book shelf, 86% of all RSC-012 on it.
    #[test]
    fn epub2_dtd_entities_do_not_invent_broken_fragments() {
        let report = crate::validate_bytes(epub2_with_dtd_entities("C1"));
        let bogus: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.id == crate::ids::RSC_012)
            .map(|m| m.text.as_str())
            .collect();
        assert!(
            bogus.is_empty(),
            "the id 'sigil_toc_id_3' is defined; got {bogus:?}"
        );
    }

    /// Issue #23, the half that hides findings - the invisible one. A
    /// document that fails to parse has every DOM check on it skipped, and
    /// the book quietly validates clean.
    ///
    /// This asserts a *positive* observation on purpose: the empty `<title>`
    /// is a real defect sitting behind the `&nbsp;`, and RSC-005 can only
    /// fire if the document was actually read. Asserting the absence of
    /// something here would prove nothing - "no findings" is exactly what
    /// the bug produced.
    #[test]
    fn epub2_dtd_entities_do_not_hide_the_document_from_dom_checks() {
        let report = crate::validate_bytes(epub2_with_dtd_entities(""));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.rule == Some("opf.content_document.empty_title")),
            "the empty <title> behind the &nbsp; must still be seen; got {:?}",
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>()
        );
    }

    /// The document is valid given the DTD it declares, so it must not be
    /// reported as malformed - resurrecting the RSC-016 false positive
    /// v0.5.8 removed, on 690 documents across 48 books, is the one outcome
    /// worse than the bug.
    #[test]
    fn epub2_dtd_entities_are_not_reported_as_malformed() {
        let report = crate::validate_bytes(epub2_with_dtd_entities("C1"));
        let fatals: Vec<_> = report
            .messages
            .iter()
            .filter(|m| m.severity == crate::report::Severity::Fatal)
            .map(|m| (m.id, m.text.as_str()))
            .collect();
        assert!(fatals.is_empty(), "document is valid; got {fatals:?}");
    }
}
