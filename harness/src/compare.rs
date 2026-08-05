//! Run epubcheck and epubveri over the same books and diff their findings by
//! message ID.
//!
//! **Why this is worth a binary.** Every other instrument here answers half a
//! question. The epubcheck *corpus* asks "do we catch what epubcheck catches"
//! — but its fixtures are synthetic and each trips exactly one rule. The
//! *shelf* asks "did our own output change", which is blind to anything that
//! has always been wrong. Neither can see a disagreement with epubcheck on a
//! real book, and that disagreement is the whole product: an ID only we report
//! is a false-positive candidate, an ID only epubcheck reports is a gap.
//!
//! This is the artefact a forum user once produced by hand — two text files,
//! epubcheck's output and ours for one book — which resolved into three causes
//! plus a false positive nothing else had found. Generating it for a whole
//! shelf is the same idea with the manual step removed.
//!
//! **Read the output as candidates, not verdicts.** A disagreement can also be
//! epubcheck reporting one finding per occurrence where we report per value,
//! an ID we deliberately don't implement (see `docs/COVERAGE.md`), or an EPUB
//! 3.4 item we ship ahead of them. Check each against `schema/20`/`schema/30`
//! before calling anything a bug — most of what real books trip is real.
//!
//! Needs a JVM and epubcheck's jar, neither of which this project otherwise
//! depends on; both are found by default under the gitignored `corpus/tools/`
//! and can be overridden:
//!     EPUBCHECK_JAR=… EPUBCHECK_JAVA=… cargo run --release -p epubveri-harness --bin compare -- <paths…>
//!
//! **Rebuild with `--workspace` after changing a schema.** This binary links
//! the epubveri library, and the grammars are embedded into it at compile
//! time by `build.rs`. A plain `cargo build --release` builds the root package
//! only, leaving a stale `compare` that reports the *previous* schema's
//! findings — silently, and looking exactly like "the fix did nothing". That
//! happened on the first change this tool was used to verify.
//!
//! Usage:
//!     … --bin compare -- ~/Documents/Projects/ebook-shelf   # every .epub under a dir
//!     … --bin compare -- book.epub other.epub
//!     … --bin compare -- --verbose ~/Documents/Projects/ebook-shelf   # per-book detail

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

/// `SEVERITY(ID): path(line,col): message`
fn epubcheck_ids(out: &str) -> BTreeMap<String, usize> {
    let re = Regex::new(r"(?m)^(FATAL|ERROR|WARNING|INFO|USAGE)\(([A-Z]+-[0-9]+)\)").unwrap();
    let mut m = BTreeMap::new();
    for c in re.captures_iter(out) {
        *m.entry(c[2].to_string()).or_insert(0) += 1;
    }
    m
}

fn epubveri_ids(path: &Path) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    match epubveri::validate_path(path) {
        Ok(report) => {
            for msg in &report.messages {
                *m.entry(msg.id.to_string()).or_insert(0) += 1;
            }
        }
        Err(e) => {
            eprintln!("  epubveri could not read {}: {e}", path.display());
        }
    }
    m
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

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let java = std::env::var("EPUBCHECK_JAVA")
        .unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".to_string());
    let jar = std::env::var("EPUBCHECK_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("corpus/tools/epubcheck-5.3.0/epubcheck.jar"));

    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let paths: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if paths.is_empty() {
        eprintln!("usage: compare [--verbose] <book.epub|dir>…");
        std::process::exit(2);
    }
    if !jar.is_file() {
        eprintln!("epubcheck jar not found at {}", jar.display());
        eprintln!("download it into corpus/tools/ or set EPUBCHECK_JAR");
        std::process::exit(2);
    }
    if Command::new(&java).arg("-version").output().is_err() {
        eprintln!("no JVM at {java} — set EPUBCHECK_JAVA");
        std::process::exit(2);
    }

    let books = collect(&paths);
    println!("comparing {} book(s) against epubcheck\n", books.len());

    // ID -> (books where only we report it, books where only epubcheck does)
    let mut only_ours: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut only_theirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut agreed = 0usize;

    for book in &books {
        let name = book.file_name().unwrap().to_string_lossy().to_string();
        // `-u` is load-bearing: epubcheck suppresses USAGE-severity messages
        // by default, and a large share of our output is USAGE. Without it the
        // first run of this tool listed eleven "false-positive candidates"
        // that were nothing of the kind — we reported them at usage level and
        // epubcheck simply was not printing its own.
        let ec_out = match Command::new(&java)
            .arg("-jar")
            .arg(&jar)
            .arg("-u")
            .arg(book)
            .output()
        {
            Ok(o) => {
                let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
                s.push_str(&String::from_utf8_lossy(&o.stderr));
                s
            }
            Err(e) => {
                eprintln!("  epubcheck failed on {name}: {e}");
                continue;
            }
        };
        let theirs = epubcheck_ids(&ec_out);
        let ours = epubveri_ids(book);

        let mine: Vec<&String> = ours.keys().filter(|k| !theirs.contains_key(*k)).collect();
        let hers: Vec<&String> = theirs.keys().filter(|k| !ours.contains_key(*k)).collect();

        if mine.is_empty() && hers.is_empty() {
            agreed += 1;
        }
        for id in &mine {
            only_ours
                .entry((*id).clone())
                .or_default()
                .push(name.clone());
        }
        for id in &hers {
            only_theirs
                .entry((*id).clone())
                .or_default()
                .push(name.clone());
        }

        if verbose && !(mine.is_empty() && hers.is_empty()) {
            println!("{name}");
            for id in &mine {
                println!("    only ours     {id} x{}", ours[*id]);
            }
            for id in &hers {
                println!("    only epubcheck {id} x{}", theirs[*id]);
            }
        }
    }

    // An ID both tools know about is not interesting; the two lists below are
    // the whole point, and they are ranked by how many books show them because
    // a disagreement on one book is likelier to be that book's oddity than a
    // rule difference.
    let show = |title: &str, m: &BTreeMap<String, Vec<String>>| {
        println!("\n--- {title} ---");
        if m.is_empty() {
            println!("  (none)");
            return;
        }
        let mut rows: Vec<(&String, &Vec<String>)> = m.iter().collect();
        rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        for (id, books) in rows {
            println!("  {id:9} in {} book(s)   e.g. {}", books.len(), {
                let b = &books[0];
                b.chars().take(46).collect::<String>()
            });
        }
    };
    show("IDs only WE report — false-positive candidates", &only_ours);
    show(
        "IDs only EPUBCHECK reports — coverage gaps (or its own known-dead IDs)",
        &only_theirs,
    );

    println!(
        "\n{agreed} of {} book(s) agreed on the ID set exactly.",
        books.len()
    );
}
