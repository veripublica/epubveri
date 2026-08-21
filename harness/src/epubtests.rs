//! Package W3C's `epub-tests` publications and run epubveri over them.
//!
//! **The question this answers that nothing else here does: what do we say
//! about books written by the people who wrote the specification?** The
//! epubcheck corpus is epubcheck's own fixtures, each broken in exactly one
//! deliberate way; the shelf is one person's library, heavy on Calibre output
//! and Project Gutenberg. `epub-tests` is neither — it is W3C's reading-system
//! conformance suite, 209 small publications that exercise parts of the format
//! real trade books never reach: media overlays, fixed layout, scripting,
//! multiple renditions, `file:` URLs, foreign content types.
//!
//! It earned its place on the day it was first run (2026-08-21), finding seven
//! defects that every existing instrument was blind to — among them two
//! reference sources whose links were collected by nothing, a check that
//! reported once per occurrence where epubcheck reports once per constraint,
//! and a Schematron rule ported without its context. None of them could have
//! been found by the corpus or the shelf: **no book on the shelf carries a
//! media overlay, a `rendition:layout` spine override, or a viewport meta in
//! more than one file.**
//!
//! **What it cannot answer.** These are conformance tests, not a sample of what
//! publishers ship, so a count here is a fact about the suite and not about the
//! world — the same caution `docs/COVERAGE.md` carries about the shelf. And
//! most of them are *supposed* to be valid, which makes this primarily a
//! false-positive instrument: an INVALID verdict below is a claim that W3C
//! published a broken test, and is much more likely to be our bug.
//!
//! **The oracle half is `compare`'s job, not this binary's.** This one needs no
//! JVM and answers "what do we say"; pointing `compare` at the packaged
//! directory answers "where do we disagree with epubcheck", which is where six
//! of the seven defects actually showed up. The last line of the output is the
//! command.
//!
//! Setup — the clone is not vendored, for the same reason epubcheck's is not
//! (we do not redistribute someone else's fixtures):
//!     git clone --depth 1 https://github.com/w3c/epub-tests.git corpus/epub-tests
//!
//! Usage:
//!     cargo run --release -p epubveri-harness --bin epubtests
//!     EPUB_TESTS_DIR=… cargo run --release -p epubveri-harness --bin epubtests
//!
//! **Rebuild with `--workspace` after a schema change** — this binary links the
//! epubveri library and the grammars are embedded at compile time, so a plain
//! `cargo build --release` leaves a stale copy reporting the previous schema's
//! findings.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Every file under `dir`, relative to it, with `/` separators, sorted.
///
/// `.DS_Store` is skipped: it is an artefact of having opened the folder on a
/// Mac, not part of anyone's test, and packaging it would have us reporting on
/// our own filesystem.
fn files_under(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == ".DS_Store" {
                    continue;
                }
                if let Ok(rel) = p.strip_prefix(dir) {
                    out.push((p.clone(), rel.to_string_lossy().replace('\\', "/")));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Zip one expanded publication into an OCF container.
///
/// `mimetype` goes first and uncompressed, which is the one part of the
/// packaging the format actually constrains — get it wrong and every book
/// reports PKG-006 and the run measures the harness instead of the tool.
fn package(dir: &Path, dest: &Path) -> std::io::Result<()> {
    let f = fs::File::create(dest)?;
    let mut z = ZipWriter::new(f);
    z.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    z.write_all(&fs::read(dir.join("mimetype"))?)?;
    let opts = SimpleFileOptions::default();
    for (path, rel) in files_under(dir) {
        if rel == "mimetype" {
            continue;
        }
        z.start_file(&rel, opts)?;
        z.write_all(&fs::read(&path)?)?;
    }
    z.finish()?;
    Ok(())
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let clone = std::env::var("EPUB_TESTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("corpus/epub-tests"));
    let tests = clone.join("tests");
    if !tests.is_dir() {
        eprintln!("no epub-tests clone at {}", clone.display());
        eprintln!("  git clone --depth 1 https://github.com/w3c/epub-tests.git corpus/epub-tests");
        eprintln!("  (or set EPUB_TESTS_DIR)");
        std::process::exit(2);
    }

    let out = root.join("corpus/epub-tests-packaged");
    if let Err(e) = fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {e}", out.display());
        std::process::exit(2);
    }

    let mut dirs: Vec<PathBuf> = fs::read_dir(&tests)
        .expect("tests/ is readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("mimetype").is_file())
        .collect();
    dirs.sort();
    if dirs.is_empty() {
        eprintln!("{} holds no publications", tests.display());
        std::process::exit(2);
    }

    let mut books: Vec<(String, PathBuf)> = Vec::new();
    for d in &dirs {
        let name = d.file_name().unwrap().to_string_lossy().into_owned();
        let dest = out.join(format!("{name}.epub"));
        match package(d, &dest) {
            Ok(()) => books.push((name, dest)),
            Err(e) => eprintln!("  ! could not package {name}: {e}"),
        }
    }
    let shown = out.canonicalize().unwrap_or_else(|_| out.clone());
    println!(
        "packaged {} publications -> {}",
        books.len(),
        shown.display()
    );

    let mut by_id: BTreeMap<String, usize> = BTreeMap::new();
    let mut invalid: Vec<(String, String)> = Vec::new();
    let mut valid = 0usize;
    for (name, path) in &books {
        let report = match epubveri::validate_path(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  ! could not read {name}: {e}");
                continue;
            }
        };
        let mut here: BTreeMap<&str, usize> = BTreeMap::new();
        for m in &report.messages {
            *by_id.entry(m.id.to_string()).or_default() += 1;
            *here.entry(m.id).or_default() += 1;
        }
        if report.is_valid() {
            valid += 1;
        } else {
            let ids = here
                .iter()
                .map(|(i, n)| format!("{n}x{i}"))
                .collect::<Vec<_>>()
                .join(" ");
            invalid.push((name.clone(), ids));
        }
    }

    println!("\n-- verdicts --");
    println!("  VALID   {valid}");
    println!("  INVALID {}", invalid.len());

    println!("\n-- findings by id --");
    let mut rows: Vec<(&String, &usize)> = by_id.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (id, n) in rows {
        println!("  {id:10} {n}");
    }

    // Listed in full rather than counted. These are conformance tests: most are
    // meant to be valid, so each line is a claim that W3C published a broken
    // one. Some genuinely are broken on purpose (`pkg-spine-unknown`,
    // `pub-xml-non-validating_unclosed`) and each test says which in its own
    // `dc:description` — read that before believing a verdict here.
    if !invalid.is_empty() {
        println!("\n-- we call these INVALID ({}) --", invalid.len());
        for (name, ids) in &invalid {
            println!("  {name:44} {ids}");
        }
    }

    println!(
        "\nfor the disagreement with epubcheck (needs a JVM):\n  cargo run --release -p epubveri-harness --bin compare -- {}",
        shown.display()
    );
}
