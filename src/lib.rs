//! epubveri — a pure-Rust EPUB validator.
//!
//! A small, fast, JVM-free, embeddable alternative to epubcheck. It combines
//! hand-coded structural checks (OCF/mimetype, container, OPF metadata,
//! manifest/spine integrity, broken references, EPUB 3 navigation) with
//! RELAX NG, XPath, and Schematron engines for the XHTML/SVG content model,
//! and reports findings with epubcheck-style message IDs (`RSC-…`, `OPF-…`,
//! `HTM-…`, …). A WebAssembly build ships separately as the `epubveri-wasm`
//! crate.

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
pub mod svg;
pub mod url;
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
    doc.root_element().attribute("version").map(String::from)
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
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext != "epub" && ext.eq_ignore_ascii_case("epub") {
            report.push(
                ids::PKG_016,
                report::Severity::Warning,
                "the file extension should be lowercase \".epub\"",
            );
        }
    }
    Ok(report)
}
