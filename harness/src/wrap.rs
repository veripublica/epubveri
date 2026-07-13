//! Synthesizes a temporary `.epub` file for each of epubcheck's own
//! single-file/single-document check modes (bare `.opf`/`.xhtml`/`.svg`/
//! `.smil` fixtures, and plain expanded-directory fixtures) - epubveri
//! only validates full books, so these wrap a minimal, otherwise-valid
//! book around the fixture under test. Faithful port of the `zip_dir`/
//! `wrap_*` functions in the former `scripts/corpus.py`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn temp_epub_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("epubveri-harness-{}-{n}.epub", std::process::id()))
}

fn write_zip(path: &Path, entries: &[(&str, &[u8], CompressionMethod)]) {
    let file = std::fs::File::create(path).expect("create temp epub");
    let mut zip = ZipWriter::new(file);
    for (name, data, comp) in entries {
        let options = SimpleFileOptions::default().compression_method(*comp);
        zip.start_file(*name, options).expect("start_file");
        zip.write_all(data).expect("write entry");
    }
    zip.finish().expect("finish zip");
}

/// Every plain file directly inside `dir` (no recursion - matches
/// Python's `os.listdir` + `os.path.isfile` filter), sorted by name.
fn sorted_siblings(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

fn guess_media_type(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "xhtml" | "html" | "htm" => "application/xhtml+xml",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "otf" | "ttf" => "application/font-sfnt",
        "woff" => "application/font-woff",
        "woff2" => "font/woff2",
        "js" => "text/javascript",
        "ncx" => "application/x-dtbncx+xml",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "xml" => "application/xml",
        "opf" => "application/oebps-package+xml",
        _ => "application/octet-stream",
    }
}

/// Zip an expanded EPUB directory to a temp `.epub` (mimetype first,
/// stored; everything else deflated) - recurses into subdirectories,
/// preserving their relative paths (matching `os.walk`).
pub fn zip_dir(src_dir: &Path) -> PathBuf {
    let mut all_files: Vec<(String, PathBuf)> = Vec::new();
    collect_files(src_dir, src_dir, &mut all_files);
    all_files.sort_by(|a, b| (a.0 != "mimetype", &a.0).cmp(&(b.0 != "mimetype", &b.0)));

    let tmp = temp_epub_path();
    let file = std::fs::File::create(&tmp).expect("create temp epub");
    let mut zip = ZipWriter::new(file);
    for (rel, full) in &all_files {
        let comp = if rel == "mimetype" {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default().compression_method(comp);
        zip.start_file(rel.as_str(), options).expect("start_file");
        let data = std::fs::read(full).expect("read source file");
        zip.write_all(&data).expect("write entry");
    }
    zip.finish().expect("finish zip");
    tmp
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

const CONTAINER_XML: &str = concat!(
    "<?xml version=\"1.0\"?>\n",
    "<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n",
    "  <rootfiles><rootfile full-path=\"OEBPS/content.opf\" ",
    "media-type=\"application/oebps-package+xml\"/></rootfiles>\n",
    "</container>\n",
);

/// epubcheck has a dedicated "check this as a navigation document"
/// single-file mode ("Given EPUBCheck configured to check a navigation
/// document"), used throughout navigation-document.feature. Unlike
/// [`wrap_single_doc`] (which wraps the target as an ordinary content
/// document behind a *separate* synthetic nav), this wraps the target
/// itself as the book's real nav doc (`properties="nav"`), alongside one
/// dummy ordinary content document so the spine isn't empty. Directory
/// siblings are still included (demoted to inert media types, mirroring
/// `wrap_single_doc`) so the target's own relative references resolve.
pub fn wrap_nav_doc(target_full: &Path, target_name: &str, version: &str) -> PathBuf {
    let nav_content = std::fs::read(target_full).expect("read nav target");
    let src_dir = target_full.parent().unwrap();
    let siblings = sorted_siblings(src_dir);

    const CONTENT_XHTML: &str = concat!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
        "<html xmlns=\"http://www.w3.org/1999/xhtml\">\n",
        "<head><title>t</title></head><body><p>t</p></body>\n",
        "</html>\n",
    );

    let mut manifest_items = vec![
        format!(
            "<item id=\"_navtarget\" href=\"{target_name}\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
        ),
        "<item id=\"_content\" href=\"_content.xhtml\" media-type=\"application/xhtml+xml\"/>"
            .to_string(),
    ];
    for (i, fn_) in siblings.iter().enumerate() {
        if fn_ == target_name {
            continue;
        }
        let mut mt = guess_media_type(fn_);
        if mt == "application/xhtml+xml" || mt == "image/svg+xml" {
            mt = "application/octet-stream";
        }
        manifest_items.push(format!(
            "<item id=\"f{i}\" href=\"{fn_}\" media-type=\"{mt}\"/>"
        ));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"{version}\" unique-identifier=\"id\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         \x20   <dc:identifier id=\"id\">corpus-wrap</dc:identifier>\n\
         \x20   <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\n\
         \x20 </metadata>\n\
         \x20 <manifest>\n    {}\n  </manifest>\n\
         \x20 <spine><itemref idref=\"_content\"/></spine>\n\
         </package>\n",
        manifest_items.join("\n    "),
    );

    let tmp = temp_epub_path();
    let mut extra: Vec<(String, Vec<u8>)> = vec![
        (
            "META-INF/container.xml".to_string(),
            CONTAINER_XML.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        (
            "OEBPS/_content.xhtml".to_string(),
            CONTENT_XHTML.as_bytes().to_vec(),
        ),
        (format!("OEBPS/{target_name}"), nav_content),
    ];
    for fn_ in &siblings {
        if fn_ == target_name {
            continue;
        }
        let data = std::fs::read(src_dir.join(fn_)).expect("read sibling");
        extra.push((format!("OEBPS/{fn_}"), data));
    }
    write_wrapped_zip(&tmp, &extra);
    tmp
}

fn write_wrapped_zip(tmp: &Path, extra: &[(String, Vec<u8>)]) {
    let file = std::fs::File::create(tmp).expect("create temp epub");
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).expect("start_file");
    zip.write_all(b"application/epub+zip")
        .expect("write mimetype");
    for (name, data) in extra {
        zip.start_file(name.as_str(), deflated).expect("start_file");
        zip.write_all(data).expect("write entry");
    }
    zip.finish().expect("finish zip");
}

/// epubcheck can check a single content document in isolation; epubveri
/// only validates full books. So for a bare content-document fixture,
/// build a minimal synthetic EPUB that includes it (plus all of its
/// directory siblings, so any relative reference it makes still resolves
/// - avoiding spurious missing-resource errors that would be an artifact
/// of this harness, not of epubveri) via a synthetic nav doc satisfying
/// the EPUB 3 nav requirement, and the fixture itself as an ordinary
/// (non-nav, non-spine) manifest item, so only the content-model checks
/// are exercised. `version` defaults to `"3.0"` but is set to `"2.0"` for
/// scenarios that originate from an `epub2/` feature file - real corpus
/// fixtures found this matters: several checks (e.g. the XHTML
/// content-model's obsolete-DOCTYPE rule, HTM-004) are EPUB3-only, and an
/// EPUB2-context fixture legitimately uses constructs (like the XHTML 1.1
/// DTD doctype) that would otherwise wrongly get EPUB3 rules applied via
/// a `version="3.0"` wrap.
pub fn wrap_single_doc(
    target_full: &Path,
    target_name: &str,
    version: &str,
    edupub: bool,
    idx: bool,
) -> PathBuf {
    let src_dir = target_full.parent().unwrap();
    let siblings = sorted_siblings(src_dir);
    let is_epub2 = version.starts_with('2');

    // EPUB 2 has no `<nav>`/nav-document concept at all - `<nav>` is
    // itself an HTML5-only element, so using it here would make an
    // EPUB 2-context wrap fail EPUB 2's own new "no HTML5-only elements"
    // DOM check (confirmed the hard way: it broke `minimal.xhtml` and
    // every other epub2 content-model fixture once that check existed).
    // The EPUB 2 wrap uses a plain hyperlink to the target instead, which
    // still gives the harness the same "a reason to include this
    // resource, but keep it out of the spine to isolate the content-
    // model check" property the EPUB 3 nav-based version has.
    let (nav_xhtml, mut manifest_items) = if is_epub2 {
        (
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
                 <head><title>t</title></head>\n\
                 <body><p><a href=\"{target_name}\">t</a></p></body>\n\
                 </html>\n"
            ),
            vec![
                "<item id=\"_nav\" href=\"_nav.xhtml\" media-type=\"application/xhtml+xml\"/>"
                    .to_string(),
            ],
        )
    } else {
        (
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
                 <head><title>Nav</title></head>\n\
                 <body><nav epub:type=\"toc\"><ol><li><a href=\"{target_name}\">t</a></li></ol></nav></body>\n\
                 </html>\n"
            ),
            vec![
                "<item id=\"_nav\" href=\"_nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
                    .to_string(),
            ],
        )
    };

    // Siblings are included so the *target*'s relative references (css,
    // images, fonts, ...) resolve. Other bare xhtml/html/svg siblings are
    // separate, independent test fixtures in their own right - including
    // them as real content documents here would make every single-doc
    // wrap exercise the content-model check against ALL of them at once,
    // not just the one under test, so they're demoted to an inert media
    // type (the target itself keeps its real one).
    for (i, fn_) in siblings.iter().enumerate() {
        let mut mt = guess_media_type(fn_);
        if fn_ != target_name && (mt == "application/xhtml+xml" || mt == "image/svg+xml") {
            mt = "application/octet-stream";
        }
        manifest_items.push(format!(
            "<item id=\"f{i}\" href=\"{fn_}\" media-type=\"{mt}\"/>"
        ));
    }

    // EPUB 2 requires a spine 'toc' (NCX) attribute - without one, an
    // otherwise-clean epub2-context wrap would spuriously fail with "EPUB
    // 2 spine is missing the required toc attribute", a harness artifact
    // from the wrap being minimal, not a real defect in the fixture.
    let mut toc_attr = String::new();
    if is_epub2 {
        manifest_items.push(
            "<item id=\"ncx\" href=\"_toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>"
                .to_string(),
        );
        toc_attr = " toc=\"ncx\"".to_string();
    }

    // The 'edupub' profile is a CLI flag in real epubcheck - since this
    // harness has no profile concept, the wrap simulates it by declaring
    // dc:type=edupub directly (and a schema:accessibilityFeature so an
    // unrelated content-model scenario doesn't spuriously trip the
    // separate accessibility-metadata check the profile also enables).
    let edupub_meta = if edupub {
        "    <dc:type>edupub</dc:type>\n    <meta property=\"schema:accessibilityFeature\">tableOfContents</meta>\n"
    } else {
        ""
    };
    // The 'idx' (EPUB Indexes) profile, same simulation approach.
    let idx_meta = if idx {
        "    <dc:type>index</dc:type>\n"
    } else {
        ""
    };

    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"{version}\" unique-identifier=\"id\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         \x20   <dc:identifier id=\"id\">corpus-wrap</dc:identifier>\n\
         \x20   <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\n\
         {edupub_meta}{idx_meta}\
         \x20 </metadata>\n\
         \x20 <manifest>\n    {}\n  </manifest>\n\
         \x20 <spine{toc_attr}><itemref idref=\"_nav\"/></spine>\n\
         </package>\n",
        manifest_items.join("\n    "),
    );

    let toc_ncx = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
         \x20 <head><meta name=\"dtb:uid\" content=\"corpus-wrap\"/></head>\n\
         \x20 <docTitle><text>Corpus wrap</text></docTitle>\n\
         \x20 <navMap><navPoint id=\"np1\" playOrder=\"1\">\
         <navLabel><text>t</text></navLabel><content src=\"{target_name}\"/>\
         </navPoint></navMap>\n\
         </ncx>\n"
    );

    let tmp = temp_epub_path();
    let mut extra: Vec<(String, Vec<u8>)> = vec![
        (
            "META-INF/container.xml".to_string(),
            CONTAINER_XML.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        ("OEBPS/_nav.xhtml".to_string(), nav_xhtml.into_bytes()),
    ];
    if is_epub2 {
        extra.push(("OEBPS/_toc.ncx".to_string(), toc_ncx.into_bytes()));
    }
    for fn_ in &siblings {
        let data = std::fs::read(src_dir.join(fn_)).expect("read sibling");
        extra.push((format!("OEBPS/{fn_}"), data));
    }
    write_wrapped_zip(&tmp, &extra);
    tmp
}

/// epubcheck's 'svg' single-document check mode, for a bare standalone
/// SVG content-document fixture - same shape as [`wrap_single_doc`]
/// (synthetic nav hyperlinking to the target so it has a reason to be
/// included, kept out of the spine to isolate the content-model check;
/// directory siblings included so relative references resolve, with
/// other bare xhtml/html/svg siblings demoted to an inert media type so
/// they don't also get exercised as independent content documents).
/// EPUB3-only - no corpus fixture for this family originates from an
/// `epub2/` feature file, so unlike `wrap_single_doc` there's no version
/// parameter.
pub fn wrap_svg_file(target_full: &Path, target_name: &str) -> PathBuf {
    let src_dir = target_full.parent().unwrap();
    let siblings = sorted_siblings(src_dir);

    let nav_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
         <head><title>Nav</title></head>\n\
         <body><nav epub:type=\"toc\"><ol><li><a href=\"{target_name}\">t</a></li></ol></nav></body>\n\
         </html>\n"
    );
    let mut manifest_items = vec![
        "<item id=\"_nav\" href=\"_nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
            .to_string(),
        format!("<item id=\"_svg\" href=\"{target_name}\" media-type=\"image/svg+xml\"/>"),
    ];
    for (i, fn_) in siblings.iter().enumerate() {
        if fn_ == target_name {
            continue;
        }
        let mut mt = guess_media_type(fn_);
        if mt == "application/xhtml+xml" || mt == "image/svg+xml" {
            mt = "application/octet-stream";
        }
        manifest_items.push(format!(
            "<item id=\"f{i}\" href=\"{fn_}\" media-type=\"{mt}\"/>"
        ));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         \x20   <dc:identifier id=\"id\">corpus-wrap</dc:identifier>\n\
         \x20   <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\n\
         \x20 </metadata>\n\
         \x20 <manifest>\n    {}\n  </manifest>\n\
         \x20 <spine><itemref idref=\"_nav\"/></spine>\n\
         </package>\n",
        manifest_items.join("\n    "),
    );

    let tmp = temp_epub_path();
    let mut extra: Vec<(String, Vec<u8>)> = vec![
        (
            "META-INF/container.xml".to_string(),
            CONTAINER_XML.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        ("OEBPS/_nav.xhtml".to_string(), nav_xhtml.into_bytes()),
    ];
    for fn_ in &siblings {
        let data = std::fs::read(src_dir.join(fn_)).expect("read sibling");
        extra.push((format!("OEBPS/{fn_}"), data));
    }
    write_wrapped_zip(&tmp, &extra);
    tmp
}

/// Wrap a bare `.opf` fixture (epubcheck's single-file package-document
/// check mode) in a minimal synthetic book: mimetype + container.xml
/// pointing straight at it. Package/Schematron-level checks (id
/// uniqueness, unique-identifier resolution, dcterms:modified, ...) only
/// need the OPF itself; manifest items it references won't exist in this
/// minimal wrap (same harness limitation as `wrap_single_doc`), so
/// RSC-001 from these is excluded from scoring the same way. Read as raw
/// bytes, not decoded text - some fixtures are deliberately non-UTF-8
/// (encoding tests), and the bytes get written straight into the zip
/// either way.
pub fn wrap_opf_file(full: &Path, name: &str) -> PathBuf {
    let opf_content = std::fs::read(full).expect("read opf fixture");
    let container_xml = format!(
        "<?xml version=\"1.0\"?>\n\
         <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
         \x20 <rootfiles><rootfile full-path=\"{name}\" media-type=\"application/oebps-package+xml\"/></rootfiles>\n\
         </container>\n"
    );
    let tmp = temp_epub_path();
    write_zip(
        &tmp,
        &[
            (
                "mimetype",
                b"application/epub+zip",
                CompressionMethod::Stored,
            ),
            (
                "META-INF/container.xml",
                container_xml.as_bytes(),
                CompressionMethod::Deflated,
            ),
            (name, &opf_content, CompressionMethod::Deflated),
        ],
    );
    tmp
}

/// Wrap a bare `.smil` fixture (epubcheck's single-document media-overlay
/// check mode) in a minimal synthetic book. Scans the SMIL's own
/// `<text src>`/`<audio src>` attributes to generate matching stub
/// resources (a content document with an anchor for every referenced
/// fragment id, and an empty audio file) so those references resolve -
/// avoiding a harness-artifact RSC-001 the same way `wrap_single_doc`/
/// `wrap_opf_file` do (also excluded from scoring via `single_doc_wrap`,
/// belt-and-suspenders).
pub fn wrap_smil_file(full: &Path, name: &str) -> PathBuf {
    let smil_content = std::fs::read_to_string(full).expect("read smil fixture");
    let src_re = Regex::new(r#"src="([^"]*)""#).unwrap();

    let mut xhtml_names: std::collections::BTreeSet<String> = Default::default();
    let mut audio_names: std::collections::BTreeSet<String> = Default::default();
    let mut anchors: std::collections::BTreeSet<String> = Default::default();
    for cap in src_re.captures_iter(&smil_content) {
        let src = &cap[1];
        let (path, frag) = match src.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (src, None),
        };
        if path.ends_with(".xhtml") || path.ends_with(".html") || path.ends_with(".htm") {
            xhtml_names.insert(path.to_string());
            if let Some(f) = frag {
                anchors.insert(f.to_string());
            }
        } else {
            audio_names.insert(path.to_string());
        }
    }
    if xhtml_names.is_empty() {
        xhtml_names.insert("chapter1.xhtml".to_string());
    }
    let body = if anchors.is_empty() {
        "<p>t</p>".to_string()
    } else {
        anchors
            .iter()
            .map(|a| format!("<p id=\"{a}\">t</p>"))
            .collect::<Vec<_>>()
            .join("")
    };
    let content_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
         <head><title>t</title></head>\n<body>{body}</body>\n</html>\n"
    );
    let first_xhtml = xhtml_names.iter().next().unwrap().clone();
    let nav_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
         <head><title>Nav</title></head>\n\
         <body><nav epub:type=\"toc\"><ol><li><a href=\"{first_xhtml}\">t</a></li></ol></nav></body>\n\
         </html>\n"
    );

    let mut manifest_items = vec![
        "<item id=\"_nav\" href=\"_nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
            .to_string(),
        format!("<item id=\"mo\" href=\"{name}\" media-type=\"application/smil+xml\"/>"),
    ];
    let mut spine_items = vec!["<itemref idref=\"_nav\"/>".to_string()];
    for (i, fn_) in xhtml_names.iter().enumerate() {
        manifest_items.push(format!(
            "<item id=\"c{i}\" href=\"{fn_}\" media-type=\"application/xhtml+xml\" media-overlay=\"mo\"/>"
        ));
        spine_items.push(format!("<itemref idref=\"c{i}\"/>"));
    }
    for (i, fn_) in audio_names.iter().enumerate() {
        manifest_items.push(format!(
            "<item id=\"a{i}\" href=\"{fn_}\" media-type=\"audio/mpeg\"/>"
        ));
    }

    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         \x20   <dc:identifier id=\"id\">corpus-wrap</dc:identifier>\n\
         \x20   <dc:title>Corpus wrap</dc:title>\n    <dc:language>en</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\n\
         \x20   <meta property=\"media:duration\">1s</meta>\n\
         \x20   <meta property=\"media:duration\" refines=\"#mo\">1s</meta>\n\
         \x20 </metadata>\n\
         \x20 <manifest>\n    {}\n  </manifest>\n\
         \x20 <spine>{}</spine>\n\
         </package>\n",
        manifest_items.join("\n    "),
        spine_items.join(""),
    );

    let tmp = temp_epub_path();
    let mut extra: Vec<(String, Vec<u8>)> = vec![
        (
            "META-INF/container.xml".to_string(),
            CONTAINER_XML.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        ("OEBPS/_nav.xhtml".to_string(), nav_xhtml.into_bytes()),
    ];
    for fn_ in &xhtml_names {
        extra.push((format!("OEBPS/{fn_}"), content_xhtml.clone().into_bytes()));
    }
    for fn_ in &audio_names {
        extra.push((format!("OEBPS/{fn_}"), vec![0u8; 16]));
    }
    extra.push((format!("OEBPS/{name}"), smil_content.into_bytes()));
    write_wrapped_zip(&tmp, &extra);
    tmp
}
