//! epubveri measurement spike — a self-contained coverage sanity check.
//!
//! Builds a set of synthetic EPUB fixtures (one valid + several each
//! deliberately tripping ONE high-value check), validates each one by
//! calling `epubveri::validate_bytes` directly (in-process - no
//! subprocess, no separately-built binary needed), and reports coverage:
//! did we catch the issue each fixture is designed to expose?
//!
//! This measures the *mechanism* against hand-built fixtures, not the
//! real epubcheck corpus (see `epubveri-corpus`, the sibling binary, for
//! that).
//!
//! Usage: `cargo run --release --bin spike`

use std::io::Write;
use std::path::PathBuf;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type Entry = (String, Vec<u8>, CompressionMethod);

const CONTAINER: &str = concat!(
    "<?xml version=\"1.0\"?>\n",
    "<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n",
    "  <rootfiles>\n",
    "    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n",
    "  </rootfiles>\n",
    "</container>\n",
);

const NAV: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
    "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n",
    "<head><title>Nav</title></head>\n",
    "<body><nav epub:type=\"toc\"><ol><li><a href=\"chapter1.xhtml\">Ch1</a></li></ol></nav></body>\n",
    "</html>\n",
);

struct OpfOpts {
    version: Option<&'static str>,
    uid: Option<&'static str>,
    ident: bool,
    title: bool,
    lang: bool,
    modified: bool,
    manifest: Option<Vec<String>>,
    spine: Option<Vec<String>>,
    spine_toc: Option<&'static str>,
}

impl Default for OpfOpts {
    fn default() -> Self {
        OpfOpts {
            version: Some("3.0"),
            uid: Some("pub-id"),
            ident: true,
            title: true,
            lang: true,
            modified: true,
            manifest: None,
            spine: None,
            spine_toc: None,
        }
    }
}

fn opf(opts: OpfOpts) -> String {
    let mut md = vec!["  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">".to_string()];
    if opts.ident {
        md.push(
            "    <dc:identifier id=\"pub-id\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>"
                .to_string(),
        );
    }
    if opts.title {
        md.push("    <dc:title>Test Book</dc:title>".to_string());
    }
    if opts.lang {
        md.push("    <dc:language>en</dc:language>".to_string());
    }
    if opts.modified {
        md.push("    <meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>".to_string());
    }
    md.push("  </metadata>".to_string());

    let manifest = opts.manifest.unwrap_or_else(|| {
        vec![
            "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
                .to_string(),
            "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
        ]
    });
    let spine = opts
        .spine
        .unwrap_or_else(|| vec!["    <itemref idref=\"ch1\"/>".to_string()]);

    let ver = opts
        .version
        .map(|v| format!(" version=\"{v}\""))
        .unwrap_or_default();
    let uidattr = opts
        .uid
        .map(|u| format!(" unique-identifier=\"{u}\""))
        .unwrap_or_default();
    let toc = opts
        .spine_toc
        .map(|t| format!(" toc=\"{t}\""))
        .unwrap_or_default();

    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\"{ver}{uidattr}>\n\
         {md}\n\
         \x20 <manifest>\n{manifest}\n  </manifest>\n\
         \x20 <spine{toc}>\n{spine}\n  </spine>\n\
         </package>\n",
        md = md.join("\n"),
        manifest = manifest.join("\n"),
        spine = spine.join("\n"),
    )
}

fn chapter(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
         <head><title>Chapter 1</title></head>\n<body>{body}</body>\n</html>\n"
    )
}

fn base_entries() -> Vec<Entry> {
    vec![
        (
            "mimetype".to_string(),
            b"application/epub+zip".to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "META-INF/container.xml".to_string(),
            CONTAINER.as_bytes().to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "OEBPS/content.opf".to_string(),
            opf(OpfOpts::default()).into_bytes(),
            CompressionMethod::Deflated,
        ),
        (
            "OEBPS/nav.xhtml".to_string(),
            NAV.as_bytes().to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "OEBPS/chapter1.xhtml".to_string(),
            chapter("<h1>Chapter 1</h1><p>Hello.</p>").into_bytes(),
            CompressionMethod::Deflated,
        ),
    ]
}

/// Replace (in place, preserving order) or append an entry.
fn put(
    entries: &[Entry],
    name: &str,
    data: impl Into<Vec<u8>>,
    comp: CompressionMethod,
) -> Vec<Entry> {
    let data = data.into();
    let mut out = Vec::with_capacity(entries.len() + 1);
    let mut found = false;
    for (n, d, c) in entries {
        if n == name {
            out.push((name.to_string(), data.clone(), comp));
            found = true;
        } else {
            out.push((n.clone(), d.clone(), *c));
        }
    }
    if !found {
        out.push((name.to_string(), data, comp));
    }
    out
}

fn remove_entry(entries: &[Entry], name: &str) -> Vec<Entry> {
    entries
        .iter()
        .filter(|(n, _, _)| n != name)
        .cloned()
        .collect()
}

fn build(path: &std::path::Path, entries: &[Entry]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    for (name, data, comp) in entries {
        let options = SimpleFileOptions::default().compression_method(*comp);
        zip.start_file(name, options)?;
        zip.write_all(data)?;
    }
    zip.finish()?;
    Ok(())
}

/// (filename, expected message ID or `None` for "must stay clean", entries)
type Fixture = (&'static str, Option<&'static str>, Vec<Entry>);

fn fixtures() -> Vec<Fixture> {
    let mut fx: Vec<Fixture> = Vec::new();
    fx.push(("valid.epub", None, base_entries()));

    // OCF / mimetype
    fx.push(("mimetype_compressed.epub", Some("PKG-007"), {
        let mut e = base_entries();
        e[0].2 = CompressionMethod::Deflated;
        e
    }));
    fx.push((
        "mimetype_wrong.epub",
        Some("PKG-007"),
        put(
            &base_entries(),
            "mimetype",
            b"application/zip".to_vec(),
            CompressionMethod::Stored,
        ),
    ));
    fx.push(("mimetype_not_first.epub", Some("PKG-006"), {
        let e = base_entries();
        let mut reordered = vec![e[1].clone(), e[0].clone()];
        reordered.extend(e[2..].iter().cloned());
        reordered
    }));

    // container.xml
    fx.push((
        "no_container.epub",
        Some("RSC-002"),
        remove_entry(&base_entries(), "META-INF/container.xml"),
    ));
    fx.push((
        "bad_container_xml.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "META-INF/container.xml",
            "<container><rootfiles>".to_string().into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "no_rootfile.epub",
        Some("RSC-003"),
        put(
            &base_entries(),
            "META-INF/container.xml",
            concat!(
                "<?xml version=\"1.0\"?><container version=\"1.0\" ",
                "xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles/></container>"
            )
            .to_string()
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // OPF presence / well-formedness / version
    fx.push((
        "opf_missing.epub",
        Some("OPF-002"),
        remove_entry(&base_entries(), "OEBPS/content.opf"),
    ));
    fx.push((
        "opf_malformed.epub",
        Some("RSC-016"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            "<?xml version=\"1.0\"?><package><metadata>"
                .to_string()
                .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "opf_no_version.epub",
        Some("OPF-001"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                version: None,
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // required metadata
    fx.push((
        "missing_title.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                title: false,
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "missing_language.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                lang: false,
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "missing_identifier.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                ident: false,
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // manifest / spine integrity
    fx.push((
        "manifest_href_missing.epub",
        Some("RSC-001"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                manifest: Some(vec![
                    "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                    "    <item id=\"gone\" href=\"missing.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "manifest_no_mediatype.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                manifest: Some(vec![
                    "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "dup_manifest_id.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                manifest: Some(vec![
                    "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                    "    <item id=\"ch1\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "spine_empty.epub",
        Some("OPF-033"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                spine: Some(vec![]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));
    fx.push((
        "spine_unresolved.epub",
        Some("OPF-049"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                spine: Some(vec!["    <itemref idref=\"nope\"/>".to_string()]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // EPUB 3 nav doc
    fx.push((
        "nav_missing.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                manifest: Some(vec![
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // broken internal reference in content - RSC-001 is reserved for a
    // manifest item/@href missing from the container (confirmed corpus-wide);
    // a content document's own broken reference is RSC-007.
    fx.push((
        "broken_ref.epub",
        Some("RSC-007"),
        put(
            &base_entries(),
            "OEBPS/chapter1.xhtml",
            chapter("<p><img src=\"missing.png\" alt=\"x\"/></p>").into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // duplicate spine reference -> RSC-005 in EPUB3 (real corpus fixtures
    // confirm the identically-shaped EPUB2 case is OPF-034 instead, a
    // version-scoped ID split - this default-version (3.0) fixture expects
    // the EPUB3 side)
    fx.push((
        "duplicate_spine.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                spine: Some(vec![
                    "    <itemref idref=\"ch1\"/>".to_string(),
                    "    <itemref idref=\"ch1\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // EPUB 2 spine with no NCX 'toc' attribute -> RSC-005
    fx.push((
        "epub2_no_toc.epub",
        Some("RSC-005"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                version: Some("2.0"),
                manifest: Some(vec![
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // spine 'toc' pointing to a non-NCX resource -> OPF-050
    fx.push((
        "toc_non_ncx.epub",
        Some("OPF-050"),
        put(
            &base_entries(),
            "OEBPS/content.opf",
            opf(OpfOpts {
                version: Some("2.0"),
                spine_toc: Some("ch1"),
                manifest: Some(vec![
                    "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
                ]),
                ..Default::default()
            })
            .into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // encrypted resource declared in encryption.xml -> RSC-004 (INFO)
    let enc_xml = concat!(
        "<?xml version=\"1.0\"?>\n",
        "<encryption xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" ",
        "xmlns:enc=\"http://www.w3.org/2001/04/xmlenc#\">\n",
        "  <enc:EncryptedData>\n",
        "    <enc:CipherData><enc:CipherReference URI=\"OEBPS/chapter1.xhtml\"/></enc:CipherData>\n",
        "  </enc:EncryptedData>\n",
        "</encryption>\n",
    );
    fx.push((
        "encryption.epub",
        Some("RSC-004"),
        put(
            &base_entries(),
            "META-INF/encryption.xml",
            enc_xml.to_string().into_bytes(),
            CompressionMethod::Deflated,
        ),
    ));

    // obfuscated font resource declared with a non-font media-type -> PKG-026
    let obfuscated_font_opf = opf(OpfOpts {
        manifest: Some(vec![
            "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
            "    <item id=\"ch1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string(),
            "    <item id=\"font\" href=\"font.otf\" media-type=\"application/xml\"/>".to_string(),
        ]),
        ..Default::default()
    });
    let obfuscation_xml = concat!(
        "<?xml version=\"1.0\"?>\n",
        "<encryption xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" ",
        "xmlns:enc=\"http://www.w3.org/2001/04/xmlenc#\">\n",
        "  <enc:EncryptedData>\n",
        "    <enc:EncryptionMethod Algorithm=\"http://www.idpf.org/2008/embedding\"/>\n",
        "    <enc:CipherData><enc:CipherReference URI=\"OEBPS/font.otf\"/></enc:CipherData>\n",
        "  </enc:EncryptedData>\n",
        "</encryption>\n",
    );
    let mut e = put(
        &base_entries(),
        "OEBPS/content.opf",
        obfuscated_font_opf.into_bytes(),
        CompressionMethod::Deflated,
    );
    e = put(
        &e,
        "META-INF/encryption.xml",
        obfuscation_xml.to_string().into_bytes(),
        CompressionMethod::Deflated,
    );
    e = put(
        &e,
        "OEBPS/font.otf",
        vec![0u8; 16],
        CompressionMethod::Deflated,
    );
    fx.push(("obfuscated_font_bad_type.epub", Some("PKG-026"), e));

    fx
}

fn run_ids(bytes: Vec<u8>) -> Vec<&'static str> {
    let report = epubveri::validate_bytes(bytes);
    report.messages.iter().map(|m| m.id).collect()
}

fn main() -> std::process::ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fix_dir = root.join("tests").join("fixtures");
    if let Err(e) = std::fs::create_dir_all(&fix_dir) {
        eprintln!("cannot create {}: {e}", fix_dir.display());
        return std::process::ExitCode::from(1);
    }

    let fx = fixtures();
    let mut covered = 0usize;
    struct Row {
        name: &'static str,
        expected: String,
        ok: bool,
        detected: String,
    }
    let mut rows: Vec<Row> = Vec::new();

    for (name, expected, entries) in &fx {
        let path = fix_dir.join(name);
        if let Err(e) = build(&path, entries) {
            eprintln!("failed to write fixture {name}: {e}");
            return std::process::ExitCode::from(1);
        }
        let bytes = std::fs::read(&path).expect("just wrote this file");
        let ids = run_ids(bytes);

        let ok = match expected {
            None => ids.is_empty(),
            Some(id) => ids.contains(id),
        };
        covered += ok as usize;
        rows.push(Row {
            name,
            expected: expected.unwrap_or("(valid)").to_string(),
            ok,
            detected: if ids.is_empty() {
                "-".to_string()
            } else {
                ids.join(",")
            },
        });
    }

    let w = rows.iter().map(|r| r.name.len()).max().unwrap_or(7).max(7);
    println!(
        "\n{:<w$}  {:<10}  {:<6}  detected",
        "fixture",
        "expected",
        "caught",
        w = w
    );
    println!("{}", "-".repeat(w + 40));
    for r in &rows {
        println!(
            "{:<w$}  {:<10}  {:<6}  {}",
            r.name,
            r.expected,
            if r.ok { "yes" } else { "NO" },
            r.detected,
            w = w
        );
    }
    let total = fx.len();
    let pct = 100.0 * covered as f64 / total as f64;
    println!("{}", "-".repeat(w + 40));
    println!("\nCoverage: {covered}/{total} fixtures = {pct:.1}%\n");

    if covered == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(2)
    }
}
