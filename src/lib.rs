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

/// The EPUB extension-spec profiles this tool recognizes, matching epubcheck's
/// own `--profile` values. Public because the CLI validates its `--profile`
/// argument against exactly this list, and PKG-023 asks the same question of
/// the same list — two callers, one answer, so there is only one list to
/// change when a profile is added.
pub const PROFILES: [&str; 4] = ["dict", "edupub", "idx", "preview"];

/// Optional validation settings. `Options::default()` is exactly the behavior
/// of [`validate_bytes`] — no profile, advisory off — so passing it changes
/// nothing.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// EPUB extension-spec profile — see [`validate_bytes_with_profile`].
    pub profile: Option<String>,
    /// Validate against this EPUB version (`"2"` or `"3"`) whatever the
    /// package document declares — epubcheck's `-v` flag. When it disagrees
    /// with the declared version, PKG-001 reports the disagreement and the
    /// *requested* version wins, as epubcheck does; expect a long report,
    /// since a 3.0 book checked as 2.0 breaks a great many EPUB 2 rules.
    /// `None` (the default) validates every book as the version it declares,
    /// which is what whole-EPUB validation normally wants.
    pub epub_version: Option<String>,
    /// Enable opt-in *advisory* checks: currently unknown CSS property and
    /// descriptor names (`ADV-*`, `Usage` severity, via the `styloria`
    /// validator). Off by default, and with it off the output is byte-identical
    /// to before. epubcheck has no verdict on these, so they are never on by
    /// default — this is the "beyond epubcheck's verdict is opt-in, never
    /// default" stance made concrete.
    pub advisory: bool,
}

/// Validate raw EPUB bytes and return a [`Report`].
pub fn validate_bytes(bytes: Vec<u8>) -> Report {
    validate_bytes_with_options(bytes, &Options::default())
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
    validate_bytes_with_options(
        bytes,
        &Options {
            profile: profile.map(String::from),
            ..Options::default()
        },
    )
}

/// Validate raw EPUB bytes under explicit [`Options`] (profile + advisory).
pub fn validate_bytes_with_options(bytes: Vec<u8>, options: &Options) -> Report {
    let mut report = Report::new();
    let mut container = match ocf::open(bytes, &mut report) {
        Some(c) => c,
        None => return report,
    };
    ocf::check_signatures(&mut container, &mut report);
    let opf_paths = ocf::find_rootfiles(&mut container, &mut report);
    // `encryption.xml`'s content model is version-dependent, so the rootfiles
    // have to be found first (issue #88). Measured against epubcheck 5.3.0, one
    // book per shape: an empty `<enc:EncryptedData>` is "missing required
    // element enc:EncryptionMethod" at 2.0 and "... enc:CipherData" at 3.0,
    // because `schema/20/rng/xenc-schema.rng` requires the method and makes the
    // cipher optional while `schema/30/mod/security/xenc-schema.rnc` does the
    // reverse. Moving this call is free: push order does not survive
    // `sort_by_document_order` below.
    //
    // `None` when no rootfile parsed - the version is genuinely unknown then,
    // and the check applies only the rules both versions share rather than
    // guessing one and inventing an error under uncertainty.
    let epub3 = opf_paths
        .first()
        .and_then(|p| peek_opf_version(&mut container, p))
        .map(|v| v.starts_with('3'));
    ocf::check_encryption(&mut container, &mut report, epub3);
    // Usually a single rootfile; a multi-rendition package (e.g. EDUPUB
    // with a reflowable + fixed-layout rendition) legitimately declares
    // more than one, each validated as its own, independent OPF.
    for opf_path in &opf_paths {
        opf::check(&mut container, opf_path, options, &mut report);
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
    // Reported after the checks, not inside `Ocf::read`: a resource read
    // from several call sites is one finding, and `read` has no `Report`.
    ocf::check_resource_limits(&container, &mut report);
    // Bound the RNG engine's pattern-interning cache (see
    // `rng::pattern::clear_intern_cache`) to roughly one book's working set,
    // rather than letting it grow for the life of a long-lived embedded
    // process validating many books.
    rng::clear_intern_cache();
    // Present findings in document order, not check-execution order (#32).
    report.sort_by_document_order();
    report
}

/// Validate an EPUB file on disk.
pub fn validate_path(path: &Path) -> std::io::Result<Report> {
    validate_path_with_options(path, &Options::default())
}

/// Validate an EPUB file on disk under a specific EPUB extension-spec
/// profile - see [`validate_bytes_with_profile`].
pub fn validate_path_with_profile(path: &Path, profile: Option<&str>) -> std::io::Result<Report> {
    validate_path_with_options(
        path,
        &Options {
            profile: profile.map(String::from),
            ..Options::default()
        },
    )
}

/// Validate an EPUB file on disk under explicit [`Options`] (profile + advisory).
pub fn validate_path_with_options(path: &Path, options: &Options) -> std::io::Result<Report> {
    let mut report = validate_bytes_with_options(std::fs::read(path)?, options);
    // The file's own extension - a filesystem-level concern `validate_bytes`
    // alone can't see, since it only ever receives raw bytes with no filename
    // attached. epubcheck's `OCFExtensionChecker` splits three ways:
    //
    //   ".epub"            -> fine
    //   ".EPUB"/".ePub"    -> PKG-016, the right extension in the wrong case
    //   anything else      -> PKG-024 (usage) on EPUB 3, PKG-017 (warning) on
    //                         EPUB 2 - same condition, different ID and
    //                         severity per version
    //
    // The version split is why `Report` carries `epub_version`: it is decided
    // inside the package document, which this layer never sees. With no
    // version at all (an unreadable or version-less book) there is nothing to
    // choose between, and the extension is the least of that book's problems -
    // so say nothing rather than guess.
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && ext != "epub"
    {
        if ext.eq_ignore_ascii_case("epub") {
            report.push(
                ids::PKG_016,
                report::Severity::Warning,
                "the file extension should be lowercase \".epub\"",
            );
        } else if let Some(version) = report.epub_version.clone() {
            let (id, severity) = if version.starts_with('3') {
                (ids::PKG_024, report::Severity::Usage)
            } else {
                (ids::PKG_017, report::Severity::Warning)
            };
            report.push(
                id,
                severity,
                format!("\".{ext}\" is an uncommon extension for an EPUB file"),
            );
        }
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
        epub2_with_body(title, "")
    }

    /// The same book with extra markup appended to the body - for checks
    /// about the content document's own contents rather than its encoding.
    fn epub2_with_link(extra_body: &str) -> Vec<u8> {
        epub2_with_body("C1", extra_body)
    }

    /// A minimal valid EPUB 3, for checks whose behaviour differs by version.
    fn epub3_minimal() -> Vec<u8> {
        epub3_with_container(None)
    }

    /// The same book with `META-INF/container.xml` replaced - for checks that
    /// live in the container document itself, where the rest of the book has
    /// to stay valid so nothing else reports.
    fn epub3_with_container(container: Option<&str>) -> Vec<u8> {
        const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const NAV: &str = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav></body></html>"#;
        const CH1: &str = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body><p>x</p></body></html>"#;
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in [
                ("META-INF/container.xml", container.unwrap_or(CONTAINER)),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/ch1.xhtml", CH1),
            ] {
                z.start_file(name, opts).unwrap();
                z.write_all(data.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    fn epub2_with_body(title: &str, extra_body: &str) -> Vec<u8> {
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
{extra_body}\n\
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

    /// PKG-001 (#61): the caller can demand a version, and epubcheck's rule is
    /// that the demand wins — it reports the disagreement and then validates
    /// against what was asked for.
    ///
    /// The last assertion is the one that matters. Reporting PKG-001 while
    /// quietly carrying on with the declared version would look identical in
    /// any test that only counted messages, and would make `-v` a lie: the
    /// same invocation would mean one thing in epubcheck and another here,
    /// which defeats the compatibility the flag exists for.
    /// A `<rootfile>` that cannot say where the package document is gets its
    /// own message, rather than only contributing to "no usable rootfile
    /// found". Both were silently filtered out before, so a container.xml
    /// with a typo'd attribute name reported RSC-003 and left the user to
    /// guess which of the two mistakes they had made.
    #[test]
    fn a_rootfile_with_no_usable_full_path_says_so() {
        let container = |rootfile: &str| {
            format!(
                r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>{rootfile}</rootfiles>
</container>"#
            )
        };
        let ids = |rootfile: &str| {
            let r = crate::validate_bytes(epub3_with_container(Some(&container(rootfile))));
            r.messages
                .iter()
                .map(|m| m.id.to_string())
                .collect::<Vec<_>>()
        };
        const MT: &str = r#"media-type="application/oebps-package+xml""#;

        assert!(ids(&format!("<rootfile {MT}/>")).contains(&crate::ids::OPF_016.to_string()));
        assert!(
            ids(&format!(r#"<rootfile full-path="" {MT}/>"#))
                .contains(&crate::ids::OPF_017.to_string())
        );
        // Whitespace-only is empty too, matching epubcheck's `trim()`. The
        // old filter tested `is_empty()`, so "   " passed as a path and the
        // book was then reported for a missing OPF at that name.
        assert!(
            ids(&format!(r#"<rootfile full-path="   " {MT}/>"#))
                .contains(&crate::ids::OPF_017.to_string())
        );
        // Reported whatever the media type says: epubcheck's handler asks
        // for the path before it looks at the type, so a rootfile that is
        // wrong in both ways still hears about the path.
        assert!(
            ids(r#"<rootfile media-type="application/oops"/>"#)
                .contains(&crate::ids::OPF_016.to_string())
        );

        let clean = ids(&format!(
            r#"<rootfile full-path="OEBPS/content.opf" {MT}/>"#
        ));
        assert!(
            !clean
                .iter()
                .any(|id| id == crate::ids::OPF_016 || id == crate::ids::OPF_017),
            "a well-formed rootfile reports neither: {clean:?}"
        );
    }

    #[test]
    fn a_requested_epub_version_wins_over_the_declared_one() {
        let check = |bytes: Vec<u8>, requested: Option<&str>| {
            crate::validate_bytes_with_options(
                bytes,
                &crate::Options {
                    epub_version: requested.map(String::from),
                    ..Default::default()
                },
            )
        };
        let pkg_001 = |r: &crate::report::Report| {
            r.messages
                .iter()
                .filter(|m| m.id == crate::ids::PKG_001)
                .count()
        };

        assert_eq!(pkg_001(&check(epub2_with_dtd_entities("C1"), None)), 0);
        assert_eq!(
            pkg_001(&check(epub2_with_dtd_entities("C1"), Some("2"))),
            0,
            "agreeing with the book is not a disagreement"
        );
        assert_eq!(pkg_001(&check(epub2_with_dtd_entities("C1"), Some("3"))), 1);

        // The EPUB 2 book, checked as EPUB 3, is now held to an EPUB 3 rule
        // it was never subject to before - so the override reached the rules
        // and not just the message.
        let forced = check(epub2_with_dtd_entities("C1"), Some("3"));
        assert!(
            forced
                .messages
                .iter()
                .any(|m| m.rule == Some("opf.package.missing_nav_document")),
            "a book validated as EPUB 3 must be asked for a nav document; got {:?}",
            forced.messages.iter().map(|m| m.rule).collect::<Vec<_>>()
        );
        assert!(
            !check(epub2_with_dtd_entities("C1"), None)
                .messages
                .iter()
                .any(|m| m.rule == Some("opf.package.missing_nav_document")),
            "and must not be, without the override"
        );
    }

    /// PKG-023 keys on the version being *validated against*, not the one the
    /// book declares - which is why it lives next to that decision rather
    /// than beside the call site. An EPUB 3 book forced to EPUB 2 has no
    /// profiles either.
    #[test]
    fn a_profile_is_reported_against_the_version_being_validated() {
        let report = crate::validate_bytes_with_options(
            epub3_minimal(),
            &crate::Options {
                profile: Some("edupub".to_string()),
                epub_version: Some("2".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(
            report
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::PKG_023)
                .count(),
            1
        );
    }

    /// PKG-023: validation profiles are an EPUB 3 feature, so `--profile` on
    /// an EPUB 2 book quietly does nothing. epubcheck says so, rather than
    /// leaving the user to believe their profile ran.
    ///
    /// The third case is the one worth having a test for: an unrecognized
    /// profile name already means "the default profile" everywhere else in
    /// this tool, so reporting it here would describe a request the user
    /// never successfully made.
    #[test]
    fn a_profile_requested_for_an_epub2_book_is_reported_as_not_applying() {
        let count = |bytes: Vec<u8>, profile: Option<&str>| {
            crate::validate_bytes_with_profile(bytes, profile)
                .messages
                .iter()
                .filter(|m| m.id == crate::ids::PKG_023)
                .count()
        };
        assert_eq!(count(epub2_with_dtd_entities("C1"), Some("edupub")), 1);
        assert_eq!(count(epub2_with_dtd_entities("C1"), None), 0);
        assert_eq!(
            count(epub2_with_dtd_entities("C1"), Some("nonsense")),
            0,
            "an unrecognized name is the default profile, not an ignored one"
        );
        assert_eq!(
            count(epub3_minimal(), Some("edupub")),
            0,
            "profiles do apply to EPUB 3"
        );
    }

    /// PKG-016/017/024 (#49): the three-way split on the container's own
    /// file extension. PKG-017 vs PKG-024 is decided by EPUB version, which
    /// is why `Report` carries `epub_version` - this layer has the filename
    /// and no OPF, the OPF layer has the version and no filename.
    #[test]
    fn container_file_extension_splits_by_version() {
        let dir = std::env::temp_dir().join(format!("epubveri-ext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ids_for = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            let r = crate::validate_path(&p).unwrap();
            let out: Vec<&str> = r
                .messages
                .iter()
                .map(|m| m.id)
                .filter(|id| id.starts_with("PKG-01") || *id == crate::ids::PKG_024)
                .collect();
            std::fs::remove_file(&p).ok();
            out
        };
        let epub2 = epub2_with_dtd_entities("C1");
        assert!(
            !ids_for("book.epub", &epub2).contains(&crate::ids::PKG_017),
            "a plain .epub extension is fine"
        );
        assert!(
            ids_for("book.EPUB", &epub2).contains(&crate::ids::PKG_016),
            "the right extension in the wrong case is PKG-016, not PKG-017"
        );
        assert!(
            ids_for("book.zip", &epub2).contains(&crate::ids::PKG_017),
            "an EPUB 2 book with a foreign extension is PKG-017 (warning)"
        );
        // The same condition on an EPUB 3 book carries the other ID, at usage
        // severity - the whole reason the version had to be plumbed through.
        let epub3 = epub3_minimal();
        assert!(
            ids_for("book.zip", &epub3).contains(&crate::ids::PKG_024),
            "an EPUB 3 book with a foreign extension is PKG-024 (usage)"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// HTM-045 (#56): an empty `href` resolves to the containing document.
    /// That is legal - so this is a usage hint, and must not turn into a
    /// broken-reference error. The paired assertion is the point: RSC-007
    /// firing here would mean we had started treating "" as a missing file.
    #[test]
    fn empty_href_is_a_usage_hint_not_a_broken_reference() {
        let report = crate::validate_bytes(epub2_with_link("<p><a href=\"\">self</a></p>"));
        assert!(
            report.messages.iter().any(|m| m.id == crate::ids::HTM_045),
            "an empty href should draw HTM-045; got {:?}",
            report.messages.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
        assert!(
            !report.messages.iter().any(|m| m.id == crate::ids::RSC_007),
            "an empty href is a self-reference, not a missing resource"
        );
        // A real relative link stays silent.
        let ok = crate::validate_bytes(epub2_with_link("<p><a href=\"#sigil_toc_id_3\">x</a></p>"));
        assert!(
            !ok.messages.iter().any(|m| m.id == crate::ids::HTM_045),
            "a resolvable href must not draw HTM-045"
        );
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
    /// This asserts a *positive* observation on purpose: the obsolete
    /// `<font>` is a real defect sitting behind the `&nbsp;`, and RSC-005 can
    /// only fire if the document was actually read. Asserting the absence of
    /// something here would prove nothing - "no findings" is exactly what
    /// the bug produced.
    ///
    /// (The probe used to be an empty `<title>`, until the shelf run showed
    /// that is *valid* in EPUB 2 - XHTML 1.1 types `<title>` as `<text/>` and
    /// only `epub-xhtml-30.sch` asserts non-empty. The probe has to be a
    /// defect in the version the fixture actually declares.)
    #[test]
    fn epub2_dtd_entities_do_not_hide_the_document_from_dom_checks() {
        let report = crate::validate_bytes(epub2_with_link("<p>x <font>y</font></p>"));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.id == crate::ids::RSC_005 && m.text.contains("font")),
            "the obsolete <font> behind the &nbsp; must still be seen; got {:?}",
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

    /// Builds a minimal EPUB 3 whose `ch1.xhtml` body is `body` - a
    /// manifest-declared, spine-referenced resource, so the checks actually
    /// read it, which is what the resource-limit fixtures below need.
    fn epub3_with(body: &str) -> Vec<u8> {
        const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        const NAV: &str = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>T</title></head>
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">C</a></li></ol></nav></body></html>"#;
        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let ch1 = format!(
            r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C</title></head><body>{body}</body></html>"#
        );
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/nav.xhtml", NAV),
                ("OEBPS/ch1.xhtml", &ch1),
            ] {
                z.start_file(name, opts).unwrap();
                z.write_all(data.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    fn nested_body(depth: usize) -> String {
        format!(
            "{}<p>x</p>{}",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        )
    }

    /// The headline regression: before the guard this aborted the whole
    /// process with `fatal runtime error: stack overflow` from a ~1 KB
    /// file, inside roxmltree's mutually-recursive tokenizer. A Rust stack
    /// overflow is `SIGABRT`, so an embedder could not catch it - the test
    /// passing at all *is* the assertion (a regression kills the runner,
    /// it does not fail an assert).
    #[test]
    fn pathological_nesting_does_not_abort_the_process() {
        let report = crate::validate_bytes(epub3_with(&nested_body(50_000)));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.text.contains("nesting is deeper")),
            "expected the depth guard to report; got {:?}",
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>()
        );
    }

    /// Just past the limit is refused with a reason naming the limit -
    /// never silently dropped from the checks, which is the failure mode
    /// the 0.7.12-0.7.14 audits kept turning up.
    #[test]
    fn over_limit_nesting_is_reported_with_a_reason() {
        let report = crate::validate_bytes(epub3_with(&nested_body(crate::ocf::MAX_XML_DEPTH + 1)));
        let hit = report
            .messages
            .iter()
            .find(|m| m.text.contains("nesting is deeper"))
            .expect("depth guard should report");
        assert_eq!(hit.severity, crate::report::Severity::Fatal);
    }

    /// The false-positive direction, and the one that matters more: the
    /// deepest document across the 65-book shelf nests 24 elements, so a
    /// book at that depth must stay clean.
    #[test]
    fn real_world_nesting_depth_still_validates() {
        let report = crate::validate_bytes(epub3_with(&nested_body(24)));
        assert!(
            !report
                .messages
                .iter()
                .any(|m| m.text.contains("nesting is deeper")),
            "24-deep is the real-book worst case and must not trip the guard"
        );
    }

    /// A zip bomb: highly compressible bytes past the per-entry cap. Before
    /// the cap a 400 KB EPUB drove 1.3 GB of peak RSS and still reported
    /// VALID; now the read is bounded and the skipped resource is named.
    #[test]
    fn oversized_entry_is_reported_not_silently_skipped() {
        // A declared, spine-referenced content document past the cap: the
        // checks reach for it, so the cap is exercised on the read path.
        let big =
            "<p>".to_string() + &"A".repeat(crate::ocf::MAX_ENTRY_BYTES as usize + 1) + "</p>";
        let report = crate::validate_bytes(epub3_with(&big));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.id == crate::ids::LIM_001
                    || m.text.contains("exceeds the 64 MiB size limit")),
            "expected LIM-001 for the oversized entry; got {:?}",
            report.messages.iter().map(|m| m.id).collect::<Vec<_>>()
        );
    }
}
