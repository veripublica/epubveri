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

use std::path::PathBuf;

mod redeclare;
use redeclare::rewrite_book;

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
