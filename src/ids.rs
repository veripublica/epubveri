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
pub const PKG_026: &str = "PKG-026"; // font-obfuscated resource isn't a font Core Media Type

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
pub const RSC_017: &str = "RSC-017"; // a deprecated construct is used (e.g. epub:switch)

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
pub const CSS_001: &str = "CSS-001"; // use of the 'direction' or 'unicode-bidi' property
pub const CSS_002: &str = "CSS-002"; // @font-face 'src' has an empty url()
pub const CSS_003: &str = "CSS-003"; // a stylesheet is UTF-16 encoded
pub const CSS_004: &str = "CSS-004"; // @charset value isn't utf-8 or utf-16
pub const CSS_008: &str = "CSS-008"; // CSS syntax error (bad string/url token)
pub const CSS_019: &str = "CSS-019"; // @font-face with an empty declaration block
pub const CSS_029: &str = "CSS-029"; // well-known media-overlay class used but its property isn't declared (usage)
pub const CSS_030: &str = "CSS-030"; // declared media-overlay active-class has no matching CSS selector

// --- Media Overlays (SMIL) ---
pub const MED_005: &str = "MED-005"; // <audio> resource is not a Core Media Type
pub const MED_008: &str = "MED-008"; // clipBegin is after clipEnd
pub const MED_009: &str = "MED-009"; // clipBegin equals clipEnd
pub const MED_010: &str = "MED-010"; // content doc's ids aren't covered by its declared overlay
pub const MED_011: &str = "MED-011"; // content doc is referenced by more than one overlay
pub const MED_012: &str = "MED-012"; // content doc is referenced by the wrong overlay
pub const MED_013: &str = "MED-013"; // content doc declares an overlay that doesn't reference it
pub const MED_014: &str = "MED-014"; // <audio src> has a URL fragment (use clipBegin/clipEnd instead)
pub const MED_015: &str = "MED-015"; // SMIL <text> order doesn't match the content doc's DOM order
pub const MED_016: &str = "MED-016"; // media:duration total doesn't match the sum of overlay durations
pub const MED_017: &str = "MED-017"; // scheme-based fragment on an XHTML media-overlay text target
pub const MED_018: &str = "MED-018"; // invalid SVG fragment identifier on a media-overlay text target

// --- Navigation document ---
pub const NAV_010: &str = "NAV-010"; // external link in a toc/page-list/landmarks nav
pub const NAV_011: &str = "NAV-011"; // toc nav link order doesn't match reading order

// --- NCX (EPUB 2 table of contents) ---
pub const NCX_001: &str = "NCX-001"; // dtb:uid doesn't match the package's dc:identifier
pub const NCX_006: &str = "NCX-006"; // an empty docTitle/navLabel text element

// --- Fixed-layout viewport/viewBox ---
pub const HTM_046: &str = "HTM-046"; // fixed-layout XHTML doc has no viewport meta
pub const HTM_047: &str = "HTM-047"; // viewport content has a blank value after '='
pub const HTM_048: &str = "HTM-048"; // fixed-layout SVG doc's root <svg> has no viewBox
pub const HTM_056: &str = "HTM-056"; // viewport content is missing the width or height key
pub const HTM_057: &str = "HTM-057"; // viewport width/height value fails the format grammar
pub const HTM_059: &str = "HTM-059"; // viewport width or height key appears more than once
pub const HTM_060: &str = "HTM-060"; // extra viewport meta, or viewport on a reflowable doc (usage)

// --- XML/encoding/doctype, misc attributes ---
pub const HTM_001: &str = "HTM-001"; // XML declaration has version="1.1" (only 1.0 is allowed)
pub const HTM_003: &str = "HTM-003"; // an entity is declared SYSTEM/PUBLIC (external)
pub const HTM_004: &str = "HTM-004"; // a DOCTYPE has a PUBLIC identifier (obsolete)
pub const HTM_007: &str = "HTM-007"; // ssml:ph attribute with an empty/blank value
pub const HTM_009: &str = "HTM-009"; // the OPF document has a DOCTYPE
pub const HTM_054: &str = "HTM-054"; // custom attribute uses a reserved (w3.org/idpf.org) namespace
pub const HTM_055: &str = "HTM-055"; // a discouraged element (base/embed/rp) is used (usage)
pub const HTM_058: &str = "HTM-058"; // content document isn't UTF-8 encoded
pub const HTM_061: &str = "HTM-061"; // an invalid data-* attribute name
