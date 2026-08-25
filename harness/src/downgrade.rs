//! Re-declare EPUB 3 books as EPUB 2 and hand the result to `compare`.
//!
//! **Whose idea this is.** JSWolf, on the MobileRead thread: *"changing an
//! ePub3 to ePub2 is pretty good at finding errors in epubveri"*. He is right,
//! and the reason is a gap no other instrument here can reach.
//!
//! **The gap.** This project's EPUB 2 branch has always been the weak one -
//! ten false positives in one month, all of them an EPUB 3 rule applied to an
//! EPUB 2 book, where XHTML 1.1 is more permissive than HTML5. The shelf
//! cannot find that class: its EPUB 2 books are Calibre output and Project
//! Gutenberg trade titles, which contain no HTML5 vocabulary, no ARIA, no
//! `epub:type`, no MathML. The corpus cannot either - it was byte-identical
//! through every one of those ten fixes. So the *markup* that stresses the
//! EPUB 2 branch exists on the shelf only inside books declaring 3.0, where
//! the EPUB 2 rules never run.
//!
//! Flipping the version declaration moves that markup under the EPUB 2 rules
//! without inventing a single byte of it. Every content document is real,
//! produced by a real tool, and now being asked the other version's question.
//!
//! **The transformation is deliberately minimal**: `version="3.0"` becomes
//! `version="2.0"` on the `<package>` element, and nothing else. No NCX is
//! synthesized, no `properties` attributes are stripped, no nav document is
//! converted. That is not an oversight and the output is not a conversion -
//! these books are massively invalid on purpose.
//!
//! **Why an invalid book is still a valid measurement.** The question this
//! feeds is not "is the book sound" but "do the two tools say the same thing
//! about it", and `compare` answers that by diffing IDs against epubcheck.
//! epubcheck is handed exactly the same bytes. An ID only we report is a
//! false-positive candidate whether or not the book deserves the other 600
//! findings.
//!
//! **Read the ID sets, not the counts.** On a book this broken the RSC-005
//! count gap is meaningless - it is our documented cascade suppression
//! multiplied by several hundred occurrences. The signal is an ID appearing on
//! one side only.
//!
//! Usage:
//!     … --bin downgrade -- --out DIR ~/Documents/Projects/ebook-shelf
//!     … --bin compare -- DIR

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// The `<package>` start tag, and the version attribute inside it.
///
/// Two regexes rather than one because the version must be replaced *within*
/// the start tag: `version="3.0"` also occurs in `<meta>` refinements and in
/// prefix declarations in real books, and a document-wide replace would edit
/// those too.
fn rewrite_package_version(opf: &str) -> Option<String> {
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
fn opf_path(container: &str) -> Option<String> {
    let re = Regex::new(r#"(?s)full-path\s*=\s*["']([^"']+)["']"#).unwrap();
    Some(re.captures(container)?[1].to_string())
}

fn collect(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut stack = vec![p.clone()];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x == "epub") {
                        out.push(p);
                    }
                }
            }
        } else {
            out.push(p.clone());
        }
    }
    out.sort();
    out
}

/// Copy every entry through, replacing the OPF, with `mimetype` written first
/// and stored - an OCF requirement, and getting it wrong would hand epubcheck
/// a PKG-006 that is the harness's fault rather than the book's.
fn rewrite_book(src: &Path, dst: &Path) -> Result<bool, String> {
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out_dir: Option<PathBuf> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out_dir = it.next().map(PathBuf::from),
            _ if a.starts_with("--") => {
                eprintln!("unknown flag {a}");
                std::process::exit(2);
            }
            _ => paths.push(PathBuf::from(a)),
        }
    }
    let Some(out_dir) = out_dir else {
        eprintln!("usage: downgrade --out <dir> <book.epub|dir>…");
        std::process::exit(2);
    };
    if paths.is_empty() {
        eprintln!("usage: downgrade --out <dir> <book.epub|dir>…");
        std::process::exit(2);
    }
    std::fs::create_dir_all(&out_dir).expect("create --out dir");

    let books = collect(&paths);
    let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

    for book in &books {
        let stem = book.file_stem().unwrap().to_string_lossy().to_string();
        // Names collide across shelf subdirectories, and `compare` prints the
        // file name alone - two books answering to `book.epub` would make its
        // output unreadable rather than wrong.
        let mut name = format!("{stem}.epub");
        let mut n = 2;
        while !used.insert(name.clone()) {
            name = format!("{stem}-{n}.epub");
            n += 1;
        }
        match rewrite_book(book, &out_dir.join(&name)) {
            Ok(true) => done += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                failed += 1;
                eprintln!("  {}: {e}", book.display());
            }
        }
    }

    println!(
        "{done} book(s) re-declared as EPUB 2 in {}\n{skipped} skipped (not 3.x), {failed} failed",
        out_dir.display()
    );
    if done > 0 {
        println!("\nnext: … --bin compare -- {}", out_dir.display());
    }
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
