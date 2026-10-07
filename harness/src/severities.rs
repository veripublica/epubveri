//! Does every finding we emit carry the severity epubcheck gives its id?
//!
//! **Why this exists.** OPF-004, OPF-004e and OPF-004f were errors here and
//! are warnings in epubcheck's `DefaultSeverities`, so a `prefix` with one
//! trailing space made a book INVALID that epubcheck passes. The ids had been
//! measured against epubcheck; the severities never were, and no epubcheck
//! fixture exercises those three, so `corpus` could not see it. A severity is
//! the part of a finding that moves the verdict, and it is a table lookup on
//! epubcheck's side - so it is checked against that table, mechanically.
//!
//! **How.** Every emission site in `src/` (outside test modules) where one of
//! our id constants is followed within three lines by `Severity::X` is
//! compared with `DefaultSeverities.java`. An id seen with two severities is
//! checked once per severity.
//!
//! **Blind spot.** A site whose severity is not written next to its id - a
//! helper that picks it, a severity held in a variable - is not seen. Those
//! are few, and the runtime half of the question (the severities the binary
//! actually prints, across corpus fixtures, shelf and probe books) caught the
//! same three and nothing else on 2026-10-07; if one is ever found that way,
//! write it into `ALLOWED` or fix the site, and say which here.
//!
//! **Deliberate differences** live in `ALLOWED`, each with its reason. An
//! entry that no longer matches anything fails the run too, so the list
//! cannot outlive what it excuses.
//!
//! Usage:
//!     … --bin severities          # exit 1 on any unexplained difference

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

/// (id, our severity, why it differs from epubcheck on purpose).
const ALLOWED: &[(&str, &str, &str)] = &[(
    "RSC-007",
    "Warning",
    "EPUB 2 arm of the package <link> check (src/opf.rs, `opf.link.missing_resource`): \
     EPUB 3 uses epubcheck's RSC-007w warning; OPF 2.0 has no package <link>, so the \
     bare id keeps the warning rather than make a restrictive change on a shape no \
     valid book contains.",
)];

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {e}", p.display());
        std::process::exit(2);
    })
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| {
            eprintln!("cannot list {}: {e}", dir.display());
            std::process::exit(2);
        })
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ds_path = root
        .join("corpus/epubcheck/src/main/java/com/adobe/epubcheck/messages/DefaultSeverities.java");
    if !ds_path.is_file() {
        eprintln!("epubcheck sources not found at {}", ds_path.display());
        eprintln!("this needs the corpus submodule checked out");
        std::process::exit(2);
    }

    // epubcheck's table: OPF_004e -> "OPF-004e" -> "Warning".
    let re_sev = Regex::new(r"MessageId\.([A-Z]+)_([0-9]+[a-z]?),\s*Severity\.([A-Z]+)").unwrap();
    let mut theirs: BTreeMap<String, String> = BTreeMap::new();
    for c in re_sev.captures_iter(&read(&ds_path)) {
        let s = &c[3];
        theirs.insert(
            format!("{}-{}", &c[1], &c[2]),
            format!("{}{}", &s[..1], s[1..].to_lowercase()),
        );
    }

    // Our constants: name -> id, with the id's first `_` folded to `-`
    // (`HTM_060a` is spelled the way epubcheck prints it).
    let re_const =
        Regex::new(r#"pub const ([A-Z0-9_]+): &str = "([A-Z]+[-_][0-9]+[a-z]?)""#).unwrap();
    let mut consts: BTreeMap<String, String> = BTreeMap::new();
    for c in re_const.captures_iter(&read(&root.join("src/ids.rs"))) {
        consts.insert(c[1].to_string(), c[2].replacen('_', "-", 1));
    }

    let re_name = Regex::new(r"\b([A-Z]{3}_[0-9]{3}[A-Z]?)\b").unwrap();
    let re_severity = Regex::new(r"Severity::([A-Za-z]+)").unwrap();
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);

    // (id, severity) -> first site seen.
    let mut ours: BTreeMap<(String, String), String> = BTreeMap::new();
    for f in &files {
        if f.ends_with("src/ids.rs") {
            continue;
        }
        let text = read(f);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            if line.trim_start().starts_with("//") {
                continue;
            }
            for c in re_name.captures_iter(line) {
                let Some(id) = consts.get(&c[1]) else {
                    continue;
                };
                let end = (i + 3).min(lines.len());
                let window = lines[i..end].join("\n");
                if let Some(s) = re_severity.captures(&window) {
                    let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
                    ours.entry((id.clone(), s[1].to_string()))
                        .or_insert_with(|| format!("{rel}:{}", i + 1));
                }
            }
        }
    }

    let allowed: BTreeSet<(&str, &str)> = ALLOWED.iter().map(|(i, s, _)| (*i, *s)).collect();
    let mut used: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut bad = 0;
    for ((id, sev), site) in &ours {
        let Some(want) = theirs.get(id) else { continue };
        if sev == want {
            continue;
        }
        if let Some(k) = allowed.iter().find(|(i, s)| i == id && s == sev) {
            used.insert(*k);
            continue;
        }
        bad += 1;
        println!("{id:10} epubcheck={want:9} ours={sev:9} {site}");
    }
    for (id, sev, _) in ALLOWED {
        if !used.contains(&(*id, *sev)) {
            bad += 1;
            println!("{id:10} ALLOWED as {sev} but no site emits it any more - drop the entry");
        }
    }
    let checked = ours
        .keys()
        .filter(|(id, _)| theirs.contains_key(id))
        .count();
    println!(
        "{checked} (id, severity) emission pairs checked against DefaultSeverities, \
         {} allowed on purpose, {bad} unexplained",
        used.len()
    );
    if bad > 0 {
        std::process::exit(1);
    }
}
