//! Re-declaring an EPUB 3 book as EPUB 2, shared by the binaries that need it.
//!
//! Split out of `downgrade.rs` when `versions` needed the same operation for
//! its paired check. The alternative was copying twenty lines, which is how
//! two lists that answer the same question start drifting apart — a shape
//! this project has already been bitten by twice, in `foreign.rs` against
//! `is_resource_reference` and in the per-source reference walks.

use std::io::{Read, Write};
use std::path::Path;

use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// The `<package>` start tag, and the version attribute inside it.
///
/// Two regexes rather than one because the version must be replaced *within*
/// the start tag: `version="3.0"` also occurs in `<meta>` refinements and in
/// prefix declarations in real books, and a document-wide replace would edit
/// those too.
pub fn rewrite_package_version(opf: &str) -> Option<String> {
    let tag_re = Regex::new(r"(?s)<package\b[^>]*>").unwrap();
    let ver_re = Regex::new(r#"(?s)\bversion\s*=\s*("3\.\d+"|'3\.\d+')"#).unwrap();

    let tag = tag_re.find(opf)?;
    let old = tag.as_str();
    let new = ver_re.replace(old, r#"version="2.0""#);
    if new == old {
        return None; // not a 3.x package - already EPUB 2, or no version at all
    }
    let mut out = String::with_capacity(opf.len());
    out.push_str(&opf[..tag.start()]);
    out.push_str(&new);
    out.push_str(&opf[tag.end()..]);
    Some(out)
}

/// `<rootfile full-path="…">` out of `META-INF/container.xml`.
pub fn opf_path(container: &str) -> Option<String> {
    let re = Regex::new(r#"(?s)full-path\s*=\s*["']([^"']+)["']"#).unwrap();
    Some(re.captures(container)?[1].to_string())
}

/// Copy every entry through, replacing the OPF, with `mimetype` written first
/// and stored - an OCF requirement, and getting it wrong would hand epubcheck
/// a PKG-006 that is the harness's fault rather than the book's.
pub fn rewrite_book(src: &Path, dst: &Path) -> Result<bool, String> {
    let file = std::fs::File::open(src).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut container = String::new();
    zip.by_name("META-INF/container.xml")
        .map_err(|_| "no META-INF/container.xml".to_string())?
        .read_to_string(&mut container)
        .map_err(|e| e.to_string())?;
    let opf_name = opf_path(&container).ok_or("no full-path in container.xml")?;

    let mut opf = String::new();
    zip.by_name(&opf_name)
        .map_err(|_| format!("no {opf_name}"))?
        .read_to_string(&mut opf)
        .map_err(|e| e.to_string())?;
    let Some(new_opf) = rewrite_package_version(&opf) else {
        return Ok(false);
    };

    // Read every entry up front: the writer and the reader cannot borrow the
    // archive at the same time, and books here are a few MB at most.
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).map_err(|e| e.to_string())?;
        if e.is_dir() {
            continue;
        }
        let name = e.name().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if name == opf_name {
            buf = new_opf.clone().into_bytes();
        }
        entries.push((name, buf));
    }

    let out = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut w = ZipWriter::new(out);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    w.start_file("mimetype", stored)
        .map_err(|e| e.to_string())?;
    w.write_all(b"application/epub+zip")
        .map_err(|e| e.to_string())?;
    for (name, data) in &entries {
        if name == "mimetype" {
            continue;
        }
        w.start_file(name.as_str(), deflated)
            .map_err(|e| e.to_string())?;
        w.write_all(data).map_err(|e| e.to_string())?;
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_the_package_tag() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata><meta property="schema:version">3.0</meta>
  <meta name="x" content="version=&quot;3.0&quot;"/></metadata>
</package>"#;
        let out = rewrite_package_version(opf).expect("3.0 package is rewritten");
        assert!(out.contains(r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0""#));
        // The two decoys keep their text: a document-wide replace would have
        // edited the refinement, which is what makes this two regexes.
        assert!(out.contains(r#"<meta property="schema:version">3.0</meta>"#));
        assert_eq!(out.matches("2.0").count(), 1);
    }

    #[test]
    fn single_quotes_and_spacing_are_handled() {
        let opf = "<package version = '3.3' >";
        assert_eq!(
            rewrite_package_version(opf).as_deref(),
            Some(r#"<package version="2.0" >"#)
        );
    }

    #[test]
    fn an_epub_2_package_is_skipped_rather_than_rewritten() {
        assert!(rewrite_package_version(r#"<package version="2.0">"#).is_none());
        assert!(rewrite_package_version("<package>").is_none());
    }
}
