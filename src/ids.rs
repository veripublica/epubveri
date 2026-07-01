//! epubcheck-compatible message IDs.
//!
//! Reconciled 2026-06-27 against epubcheck's own `MessageBundle.properties` and
//! `.feature` corpus, so the IDs we emit match what epubcheck actually reports
//! (drop-in familiarity). Key lesson from the reconciliation: epubcheck enforces
//! most package-document constraints via RelaxNG + Schematron and surfaces them
//! all under the single catch-all **RSC-005** ("Error while parsing file"). So
//! conditions that *feel* like they deserve a dedicated OPF code (missing
//! dc:title/identifier/language, a missing nav doc, a malformed/duplicate
//! manifest item) are reported by epubcheck as RSC-005 — and we match that.
//!
//! Only a handful of our structural checks have dedicated codes; those are used
//! verbatim below. Our message *wording* is our own (we do not copy epubcheck's).

// --- Packaging / OCF ---
pub const PKG_004: &str = "PKG-004"; // corrupted/unreadable ZIP header (not a usable OCF)
pub const PKG_006: &str = "PKG-006"; // mimetype entry missing or not first in the archive
pub const PKG_007: &str = "PKG-007"; // mimetype compressed, or contents != application/epub+zip

// --- Resources / generic ---
pub const RSC_001: &str = "RSC-001"; // a referenced file could not be found
pub const RSC_002: &str = "RSC-002"; // META-INF/container.xml is missing
pub const RSC_003: &str = "RSC-003"; // no rootfile with the OPF media type in container.xml
pub const RSC_004: &str = "RSC-004"; // a resource is encrypted; its content is not checked (INFO)
                                     // RSC-005 is epubcheck's RelaxNG/Schematron catch-all. We emit it for: XML not
                                     // well-formed; missing required metadata (dc:title / dc:language / dc:identifier);
                                     // a malformed manifest <item> (missing id/href/media-type); a duplicate manifest
                                     // id; and a missing EPUB 3 navigation document.
pub const RSC_005: &str = "RSC-005";

// --- OPF package document (dedicated codes, used verbatim) ---
pub const OPF_001: &str = "OPF-001"; // error parsing the EPUB version
pub const OPF_002: &str = "OPF-002"; // the OPF file was not found in the EPUB
pub const OPF_030: &str = "OPF-030"; // the unique-identifier was not found
pub const OPF_033: &str = "OPF-033"; // the spine contains no linear resources
pub const OPF_034: &str = "OPF-034"; // the spine references the same manifest item more than once
pub const OPF_043: &str = "OPF-043"; // spine item w/ non-content media-type has no fallback
pub const OPF_049: &str = "OPF-049"; // spine itemref idref not found in the manifest
pub const OPF_050: &str = "OPF-050"; // spine 'toc' references a non-NCX resource

// --- CSS (via the styloria parser) ---
pub const CSS_002: &str = "CSS-002"; // @font-face 'src' has an empty url()
pub const CSS_008: &str = "CSS-008"; // CSS syntax error (bad string/url token)
pub const CSS_019: &str = "CSS-019"; // @font-face with an empty declaration block
