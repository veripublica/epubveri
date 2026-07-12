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
pub const PKG_003: &str = "PKG-003"; // the zip file is empty
pub const PKG_004: &str = "PKG-004"; // corrupted/unreadable ZIP header (not a usable OCF)
pub const PKG_005: &str = "PKG-005"; // the mimetype entry's ZIP header has a non-empty extra field
pub const PKG_006: &str = "PKG-006"; // mimetype entry missing or not first in the archive
pub const PKG_007: &str = "PKG-007"; // mimetype compressed, or contents != application/epub+zip
pub const PKG_008: &str = "PKG-008"; // the ZIP archive could not be opened at all
pub const PKG_009: &str = "PKG-009"; // a file name contains a forbidden character
pub const PKG_010: &str = "PKG-010"; // an href contains unencoded spaces
pub const PKG_011: &str = "PKG-011"; // a file name ends with a full stop
pub const PKG_012: &str = "PKG-012"; // a file name contains non-ASCII characters (usage)
pub const PKG_013: &str = "PKG-013"; // container.xml declares more than one OPF rootfile (EPUB2)
pub const PKG_014: &str = "PKG-014"; // an empty directory is present in the OCF container
pub const PKG_016: &str = "PKG-016"; // the file's own ".epub" extension is not lowercase
pub const PKG_021: &str = "PKG-021"; // an image resource is corrupt (its bytes don't match any known image format)
pub const PKG_022: &str = "PKG-022"; // an image resource's file extension doesn't match its actual format
pub const PKG_025: &str = "PKG-025"; // a publication resource is stored inside META-INF
pub const PKG_026: &str = "PKG-026"; // font-obfuscated resource isn't a font Core Media Type
pub const PKG_027: &str = "PKG-027"; // a ZIP entry's file name isn't valid UTF-8

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
pub const RSC_006: &str = "RSC-006"; // a remote resource is referenced from an HTML "a" element
pub const RSC_007: &str = "RSC-007"; // a reference points to a resource missing from the publication
pub const RSC_008: &str = "RSC-008"; // a remote resource is used but not declared in the manifest
pub const RSC_009: &str = "RSC-009"; // a non-SVG image is referenced with a URL fragment identifier
pub const RSC_010: &str = "RSC-010"; // a toc nav link targets a resource that isn't a Content Document
pub const RSC_011: &str = "RSC-011"; // a hyperlinked document isn't listed in the spine
pub const RSC_012: &str = "RSC-012"; // a content src/href fragment identifier doesn't resolve
pub const RSC_013: &str = "RSC-013"; // a stylesheet reference has a URL fragment identifier
pub const RSC_014: &str = "RSC-014"; // a hyperlink targets an incompatible resource type (e.g. an SVG symbol)
pub const RSC_015: &str = "RSC-015"; // an SVG "use" element's href has no fragment identifier
pub const RSC_016: &str = "RSC-016"; // a malformed or unknown XML entity reference
pub const RSC_017: &str = "RSC-017"; // a deprecated construct is used (e.g. epub:switch)
pub const RSC_019: &str = "RSC-019"; // a multi-rendition publication has no META-INF/metadata.xml
pub const RSC_020: &str = "RSC-020"; // a URL is not conforming (spaces, unparseable host, missing slashes)
pub const RSC_025: &str = "RSC-025"; // SVG content-model violation (usage)
pub const RSC_029: &str = "RSC-029"; // a data URL is used where it isn't allowed
pub const RSC_027: &str = "RSC-027"; // the OPF/content document is UTF-16 encoded (warning)
pub const RSC_028: &str = "RSC-028"; // the OPF/content document uses a disallowed non-UTF-8 encoding
pub const RSC_030: &str = "RSC-030"; // a file: URL is used, which is not allowed
pub const RSC_031: &str = "RSC-031"; // a remote resource uses http instead of https
pub const RSC_032: &str = "RSC-032"; // a foreign resource is used with no required fallback
pub const RSC_033: &str = "RSC-033"; // a local reference has a URL query string
pub const RSC_026: &str = "RSC-026"; // a URL is path-absolute or escapes the container root

// --- OPF package document (dedicated codes, used verbatim) ---
pub const OPF_007: &str = "OPF-007"; // a reserved vocabulary prefix is redeclared
pub const OPF_001: &str = "OPF-001"; // error parsing the EPUB version
pub const OPF_002: &str = "OPF-002"; // the OPF file was not found in the EPUB
pub const OPF_029: &str = "OPF-029"; // a resource's declared media-type doesn't match its actual (sniffed) format
pub const OPF_030: &str = "OPF-030"; // the unique-identifier was not found
pub const OPF_033: &str = "OPF-033"; // the spine contains no linear resources
pub const OPF_034: &str = "OPF-034"; // the spine references the same manifest item more than once
pub const OPF_043: &str = "OPF-043"; // spine item w/ non-content media-type has no fallback
pub const OPF_049: &str = "OPF-049"; // spine itemref idref not found in the manifest
pub const OPF_050: &str = "OPF-050"; // spine 'toc' references a non-NCX resource
pub const OPF_012: &str = "OPF-012"; // Data Navigation Document isn't application/xhtml+xml
pub const OPF_013: &str = "OPF-013"; // a declared type attribute doesn't match the resource's actual media-type
pub const OPF_066: &str = "OPF-066"; // an edupub page-list nav exists but no print-source is identified
pub const OPF_086: &str = "OPF-086"; // warning: a deprecated rendition property/value or deprecated meta viewport
pub const OPF_086B: &str = "OPF-086b"; // same family, usage-level: a deprecated epub:type semantic value
pub const OPF_087: &str = "OPF-087"; // epub:type value only restates its host element's own native semantic (usage)
pub const OPF_088: &str = "OPF-088"; // epub:type value isn't in the default vocabulary (usage)
pub const OPF_090: &str = "OPF-090"; // a non-preferred (but valid) Core Media Type is used (usage)
pub const OPF_077: &str = "OPF-077"; // the Data Navigation Document is referenced from the spine
pub const OPF_003: &str = "OPF-003"; // a container resource isn't listed in the manifest (usage)
pub const OPF_014: &str = "OPF-014"; // a content property (remote-resources/scripted/svg) is used but not declared
pub const OPF_015: &str = "OPF-015"; // a content property is declared but not needed
pub const OPF_018: &str = "OPF-018"; // a content property is declared but not needed (warning variant)
pub const OPF_025: &str = "OPF-025"; // an attribute value must be a single token, not a list
pub const OPF_026: &str = "OPF-026"; // a meta property name is not well-formed
pub const OPF_027: &str = "OPF-027"; // an unknown/unprefixed value is used for a known-vocabulary attribute
pub const OPF_040: &str = "OPF-040"; // fallback references an unknown manifest id
pub const OPF_045: &str = "OPF-045"; // fallback references its own item id
pub const OPF_048: &str = "OPF-048"; // package is missing its required unique-identifier attribute
pub const OPF_065: &str = "OPF-065"; // a refines chain forms a cycle
pub const OPF_070: &str = "OPF-070"; // a collection role used as a URL is not a valid URL
pub const OPF_071: &str = "OPF-071"; // an index collection links to a non-XHTML resource
pub const OPF_075: &str = "OPF-075"; // a preview collection link does not target an XHTML Content Document
pub const OPF_076: &str = "OPF-076"; // a preview collection link uses an EPUB CFI fragment
pub const OPF_073: &str = "OPF-073"; // a manifest resource's DOCTYPE external identifier is disallowed or mismatched
pub const OPF_074: &str = "OPF-074"; // two manifest items represent the same resource
pub const OPF_060: &str = "OPF-060"; // two container entry names collide after case-folding/NFC normalization
pub const OPF_085: &str = "OPF-085"; // a urn:uuid: dc:identifier isn't a valid UUID
pub const OPF_091: &str = "OPF-091"; // a manifest item href must not have a fragment identifier
pub const OPF_031: &str = "OPF-031"; // a guide reference is not declared in the manifest
pub const OPF_032: &str = "OPF-032"; // a guide reference targets a non-Content-Document resource
pub const OPF_035: &str = "OPF-035"; // a manifest item declares an HTML media-type instead of XHTML (warning)
pub const OPF_037: &str = "OPF-037"; // a manifest item uses the deprecated OEB 1.x CSS media-type (warning)
pub const OPF_038: &str = "OPF-038"; // a manifest item uses a legacy OEBPS 1.2 CSS media-type (warning)
pub const OPF_039: &str = "OPF-039"; // a manifest item uses a legacy OEBPS 1.2 HTML media-type (warning)
pub const OPF_041: &str = "OPF-041"; // a fallback-style attribute references an unknown manifest item
pub const OPF_042: &str = "OPF-042"; // a spine item's media-type is an image (not a Content Document)
pub const OPF_052: &str = "OPF-052"; // a dc:creator/contributor role isn't a recognized MARC relator code
pub const OPF_053: &str = "OPF-053"; // a dc:date value doesn't follow recommended ISO 8601 syntax (warning, EPUB3)
pub const OPF_054: &str = "OPF-054"; // a dc:date value is empty or doesn't conform to ISO 8601 (error, EPUB2)
pub const OPF_055: &str = "OPF-055"; // a dc:title value is empty (warning)
pub const OPF_063: &str = "OPF-063"; // a page-map reference doesn't resolve to a real id (warning)
pub const OPF_092: &str = "OPF-092"; // a language tag is empty/whitespace or not well-formed
pub const OPF_093: &str = "OPF-093"; // a local link is missing its required media-type attribute
pub const OPF_096: &str = "OPF-096"; // non-linear content isn't reachable from the reading order
pub const OPF_096B: &str = "OPF-096b"; // same, but usage-level: the book uses scripting, which could add reachability dynamically
pub const OPF_098: &str = "OPF-098"; // a link target must not reference a manifest item id
pub const OPF_099: &str = "OPF-099"; // a manifest item references the package document itself

// --- CSS (via the styloria parser) ---
pub const CSS_001: &str = "CSS-001"; // use of the 'direction' or 'unicode-bidi' property
pub const CSS_002: &str = "CSS-002"; // @font-face 'src' has an empty url()
pub const CSS_007: &str = "CSS-007"; // an exempt (foreign) font is used without a fallback (usage)
pub const CSS_005: &str = "CSS-005"; // a stylesheet link's class conflicts between alt style tags (usage)
pub const CSS_003: &str = "CSS-003"; // a stylesheet is UTF-16 encoded
pub const CSS_004: &str = "CSS-004"; // @charset value isn't utf-8 or utf-16
pub const CSS_008: &str = "CSS-008"; // CSS syntax error (bad string/url token)
pub const CSS_015: &str = "CSS-015"; // an alternate stylesheet link is missing or has an empty title
pub const CSS_019: &str = "CSS-019"; // @font-face with an empty declaration block
pub const CSS_029: &str = "CSS-029"; // well-known media-overlay class used but its property isn't declared (usage)
pub const CSS_030: &str = "CSS-030"; // declared media-overlay active-class has no matching CSS selector

// --- Media Overlays (SMIL) ---
pub const MED_003: &str = "MED-003"; // a <picture> element's own <img> fallback references a foreign resource
pub const MED_004: &str = "MED-004"; // an image resource is corrupt
pub const MED_005: &str = "MED-005"; // <audio> resource is not a Core Media Type
pub const MED_007: &str = "MED-007"; // a <picture> <source> references a foreign resource with no type attribute
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
pub const NAV_003: &str = "NAV-003"; // edupub publication with a pagination source but no page-list nav
pub const NAV_009: &str = "NAV-009"; // region-based nav target isn't a fixed-layout document
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
pub const HTM_010: &str = "HTM-010"; // an unrecognized epub: namespace URI is bound (informative)
pub const HTM_025: &str = "HTM-025"; // a URL uses an unregistered scheme
pub const HTM_001: &str = "HTM-001"; // XML declaration has version="1.1" (only 1.0 is allowed)
pub const HTM_003: &str = "HTM-003"; // an entity is declared SYSTEM/PUBLIC (external)
pub const HTM_004: &str = "HTM-004"; // a DOCTYPE has a PUBLIC identifier (obsolete)
pub const HTM_007: &str = "HTM-007"; // ssml:ph attribute with an empty/blank value
pub const HTM_009: &str = "HTM-009"; // the OPF document has a DOCTYPE
pub const HTM_054: &str = "HTM-054"; // custom attribute uses a reserved (w3.org/idpf.org) namespace
pub const HTM_055: &str = "HTM-055"; // a discouraged element (base/embed/rp) is used (usage)
pub const HTM_058: &str = "HTM-058"; // content document isn't UTF-8 encoded
pub const HTM_061: &str = "HTM-061"; // an invalid data-* attribute name

// --- Extension profiles: EDUPUB, Region-Based Navigation ---
pub const HTM_051: &str = "HTM-051"; // HTML5 microdata attribute in an edupub content document
pub const HTM_052: &str = "HTM-052"; // region-based nav found outside the Data Navigation Document

// --- Accessibility ---
pub const ACC_009: &str = "ACC-009"; // MathML markup has no alternative text (usage)
pub const ACC_011: &str = "ACC-011"; // an SVG link has no accessible label (usage)

// --- EPUB Dictionaries & Glossaries 1.0 ---
pub const RSC_021: &str = "RSC-021"; // a search-key-group href targets an incompatible resource type
pub const OPF_078: &str = "OPF-078"; // dc:type=dictionary but no content document has dictionary content
pub const OPF_079: &str = "OPF-079"; // dictionary content detected, but dc:type=dictionary isn't declared (usage)
pub const OPF_080: &str = "OPF-080"; // a Search Key Map document's file name doesn't end in .xml (usage)
pub const OPF_081: &str = "OPF-081"; // a dictionary collection link target isn't declared in the manifest
pub const OPF_082: &str = "OPF-082"; // a dictionary collection contains more than one Search Key Map Document
pub const OPF_083: &str = "OPF-083"; // a dictionary collection contains no Search Key Map Document
pub const OPF_084: &str = "OPF-084"; // a dictionary collection link targets neither a Search Key Map nor an XHTML doc

// --- D-vocabularies: prefix attribute / vocabulary association ---
// Real epubcheck's feature-file scenarios label several distinct
// sub-conditions "OPF-004c"/"OPF-007a"/"OPF-007b"/"OPF-007c" - but (like
// the earlier "HTM-060a" case) these are Gherkin-authoring sub-case
// labels, not real distinct message IDs: `scripts/corpus.py`'s own
// `ID_RE` strips the trailing lowercase letter when parsing expectations,
// so the real, scored ID is plain OPF-004/OPF-007 in every case.
pub const OPF_004: &str = "OPF-004"; // the prefix attribute value has a syntax error
pub const OPF_028: &str = "OPF-028"; // an undeclared (and non-reserved) prefix is used
pub const OPF_089: &str = "OPF-089"; // an "alternate" link is combined with another rel keyword
pub const OPF_094: &str = "OPF-094"; // a "record"/"voicing" link is missing a media-type
pub const OPF_095: &str = "OPF-095"; // a "voicing" link's media-type isn't an audio type
