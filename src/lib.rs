//! epubveri — a pure-Rust EPUB validator (measurement spike).
//!
//! A small, fast, JVM-free, embeddable alternative to epubcheck. This spike
//! hand-codes ~20 high-value structural checks (OCF/mimetype, container,
//! OPF metadata, manifest/spine integrity, broken references, EPUB 3 nav) and
//! reports them with epubcheck-style message IDs. The deep XHTML content-model
//! (RelaxNG/Schematron) is intentionally out of scope for now.

pub mod cmt;
pub mod css;
pub mod edupub;
pub mod filename;
pub mod foreign;
pub mod htm;
pub mod ids;
pub mod image;
pub mod layout;
pub mod mathml;
pub mod navdoc;
pub mod ncx;
pub mod ocf;
pub mod opf;
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

/// Validate raw EPUB bytes and return a [`Report`].
pub fn validate_bytes(bytes: Vec<u8>) -> Report {
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
        opf::check(&mut container, opf_path, &mut report);
    }
    // Checked once for the whole publication (not per-rendition): the
    // multi-rendition dc:type cardinality cross-check reads
    // META-INF/metadata.xml, which no single opf::check call ever sees.
    if opf_paths.len() > 1 {
        edupub::check_multi_rendition_dc_type(&mut container, &opf_paths, &mut report);
        renditions::check(&mut container, &mut report);
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
    Ok(validate_bytes(std::fs::read(path)?))
}
