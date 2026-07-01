//! epubveri — a pure-Rust EPUB validator (measurement spike).
//!
//! A small, fast, JVM-free, embeddable alternative to epubcheck. This spike
//! hand-codes ~20 high-value structural checks (OCF/mimetype, container,
//! OPF metadata, manifest/spine integrity, broken references, EPUB 3 nav) and
//! reports them with epubcheck-style message IDs. The deep XHTML content-model
//! (RelaxNG/Schematron) is intentionally out of scope for now.

pub mod css;
pub mod ids;
pub mod ocf;
pub mod opf;
pub mod report;
pub mod rng;
pub mod schematron;
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
    let opf_path = match ocf::find_rootfile(&mut container, &mut report) {
        Some(p) => p,
        None => return report,
    };
    opf::check(&mut container, &opf_path, &mut report);
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
