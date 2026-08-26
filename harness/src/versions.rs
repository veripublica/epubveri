//! Which of our findings are version-scoped in epubcheck, and are ours gated
//! the same way?
//!
//! **Why this is a binary and not a habit.** The same defect class was found
//! by hand three times in one week: ten EPUB 3 rules firing on EPUB 2 books
//! (0.12.1), three more (#95), and OPF-042 running the other way — an EPUB 2
//! rule we emitted at 3.0 (#91). Each time the method was identical: take an
//! id, find its call sites in epubcheck's Java, ask whether they all sit in
//! EPUB-3-only classes, then check our own gating. There are **485 emission
//! sites** in this library, so doing that by hand is weeks of work; doing it
//! mechanically is a script that narrows 200-odd ids to a handful.
//!
//! **It reports candidates, never verdicts.** Every row it prints still needs
//! the same one-book probe against epubcheck that settled the previous
//! fifteen, because the classification below is a heuristic about class names
//! and method overrides, not a reading of epubcheck's control flow. A row
//! here means "go and ask", exactly like `compare`'s two id lists.
//!
//! The two directions, and why the second one is harder:
//!
//! - **EPUB 3 only** — every call site is in a class named `…30` or
//!   `Overlay…`. epubcheck cannot reach it for an EPUB 2 book, so if we can
//!   emit it without an `is_epub3` gate, that is a candidate.
//! - **EPUB 2 only** — every call site is in a *base* class, but the method
//!   holding it is overridden in the `…30` subclass, so the EPUB 3 path never
//!   runs it. This is the OPF-042 shape, and it is invisible to a plain grep:
//!   `OPFChecker.checkSpineItem` looks version-neutral until you notice
//!   `OPFChecker30` overrides it.
//!
//! Usage:
//!     … --bin versions
//!     … --bin versions -- --all      # also list the ids judged neutral

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// Every `.java` file under a root, as (class name, contents).
fn java_files(root: &Path) -> Vec<(String, PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "java") {
                let name = p.file_stem().unwrap().to_string_lossy().to_string();
                let body = read(&p);
                out.push((name, p, body));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A class epubcheck only ever runs for EPUB 3.
///
/// The `…30` suffix is epubcheck's own convention for the version-3 subclass,
/// and the media-overlay handlers have no EPUB 2 counterpart because media
/// overlays are an EPUB 3 feature.
fn is_epub3_class(class: &str) -> bool {
    class.ends_with("30") || class.starts_with("Overlay")
}

/// The name of the method containing `line` (1-based), by scanning back for a
/// Java member signature at class-body indentation.
fn enclosing_method(body: &str, line: usize) -> Option<String> {
    let sig = Regex::new(r"^\s{2,4}(?:public|protected|private)\s+[^;=]*?\b(\w+)\s*\(").unwrap();
    let lines: Vec<&str> = body.lines().collect();
    for i in (0..line.min(lines.len())).rev() {
        if let Some(c) = sig.captures(lines[i]) {
            return Some(c[1].to_string());
        }
    }
    None
}

/// Whether `class` overrides `method` **without** delegating to the base
/// implementation.
///
/// The delegation test is what makes this usable at all. A first version
/// asked only "is the method overridden", and called OPF-030 and OPF-033
/// EPUB 2 only — but `OPFChecker30.checkPackage` and `.checkContent` both
/// call `super`, so the base code runs at 3.0 and epubcheck reports those ids
/// for an EPUB 3 book. `checkItem`, `checkSpineItem` and `endElement` do not
/// delegate, which is exactly why OPF-042 was version-scoped and why the
/// other two were not.
///
/// The whole override body is scanned, not the first few lines: a `super`
/// call late in a long method delegates just as thoroughly as one on line
/// two, and a truncated read would resurrect the same false positives.
fn overrides_without_super(body: &str, method: &str) -> bool {
    let sig = Regex::new(&format!(
        r"^(\s{{2,4}})(?:public|protected|private)\s+[^;=]*?\b{}\s*\(",
        regex::escape(method)
    ))
    .unwrap();
    let lines: Vec<&str> = body.lines().collect();
    let Some(start) = lines.iter().position(|l| sig.is_match(l)) else {
        return false; // not overridden at all
    };
    let indent = sig.captures(lines[start]).map(|c| c[1].len()).unwrap_or(2);
    let close = format!("{}}}", " ".repeat(indent));
    let end = lines[start + 1..]
        .iter()
        .position(|l| *l == close)
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    let call = format!("super.{method}(");
    !lines[start..end].iter().any(|l| l.contains(&call))
}

struct Site {
    class: String,
    method: Option<String>,
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ec = root.join("corpus/epubcheck/src/main/java");
    if !ec.is_dir() {
        eprintln!("epubcheck source not found at {}", ec.display());
        eprintln!("this needs the epubcheck checkout under corpus/, like `coverage` does");
        std::process::exit(2);
    }
    let show_all = std::env::args().any(|a| a == "--all");

    // --- our ids, and which of them we actually emit ---
    let ids_rs = read(&root.join("src/ids.rs"));
    let re_id =
        Regex::new(r#"pub const ([A-Z0-9_]+): &str = "([A-Z]+[-_][0-9]+[a-z]?)";"#).unwrap();
    let mut ours: Vec<(String, String)> = Vec::new(); // (const, id)
    for c in re_id.captures_iter(&ids_rs) {
        ours.push((c[1].to_string(), c[2].to_string()));
    }

    // Uses of each constant outside ids.rs. The dead-id rule cuts both ways:
    // a declared constant no line emits is not coverage, and here it would be
    // a candidate we could never actually trip.
    let mut src = String::new();
    for entry in ["src", "src/rng"] {
        let d = root.join(entry);
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "rs")
                    && p.file_name().is_some_and(|n| n != "ids.rs")
                {
                    src.push_str(&read(&p));
                    src.push('\n');
                }
            }
        }
    }

    // --- epubcheck call sites per message constant ---
    let files = java_files(&ec);
    let re_msg = Regex::new(r"MessageId\.([A-Z]+_[0-9]+[a-z]?)\b").unwrap();
    let mut sites: BTreeMap<String, Vec<Site>> = BTreeMap::new();
    for (class, path, body) in &files {
        let base = path.file_name().unwrap().to_string_lossy();
        if base == "MessageId.java" || base == "DefaultSeverities.java" {
            continue;
        }
        for (n, line) in body.lines().enumerate() {
            if !line.contains("MessageId.") {
                continue;
            }
            for c in re_msg.captures_iter(line) {
                sites.entry(c[1].to_string()).or_default().push(Site {
                    class: class.clone(),
                    method: enclosing_method(body, n),
                });
            }
        }
    }

    let class_index: BTreeMap<&str, &String> =
        files.iter().map(|(c, _, b)| (c.as_str(), b)).collect();

    let mut only3: Vec<(String, Vec<String>)> = Vec::new();
    let mut only2: Vec<(String, Vec<String>)> = Vec::new();
    let mut neutral: Vec<String> = Vec::new();
    let mut unemitted = 0usize;
    let mut no_site = 0usize;

    for (konst, id) in &ours {
        if !src.contains(konst.as_str()) {
            unemitted += 1;
            continue;
        }
        // Our constant name uppercases epubcheck's trailing letter; derive the
        // Java spelling from the id string instead of the constant name.
        let java_const = id.replacen('-', "_", 1);
        let Some(s) = sites.get(&java_const) else {
            no_site += 1;
            continue;
        };
        let classes: BTreeSet<&str> = s.iter().map(|x| x.class.as_str()).collect();
        let where_: Vec<String> = s
            .iter()
            .map(|x| format!("{}::{}", x.class, x.method.clone().unwrap_or("?".into())))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        if classes.iter().all(|c| is_epub3_class(c)) {
            only3.push((id.clone(), where_));
            continue;
        }
        // The OPF-042 shape: no EPUB 3 class emits it, and every method that
        // does is overridden in the corresponding `…30` subclass.
        let overridden = s.iter().all(|x| {
            if is_epub3_class(&x.class) {
                return false;
            }
            let Some(m) = &x.method else { return false };
            let sub = format!("{}30", x.class);
            class_index
                .get(sub.as_str())
                .is_some_and(|b| overrides_without_super(b, m))
        });
        if overridden && !s.is_empty() {
            only2.push((id.clone(), where_));
        } else {
            neutral.push(id.clone());
        }
    }

    println!("=== version scope of the ids we emit, from epubcheck's own call sites ===\n");
    println!(
        "ids declared: {}   emitted by us: {}   (skipped: {} unemitted, {} with no epubcheck call site)",
        ours.len(),
        ours.len() - unemitted - no_site,
        unemitted,
        no_site
    );

    let show = |title: &str, note: &str, rows: &[(String, Vec<String>)]| {
        println!("\n--- {title} ({} ids) ---", rows.len());
        println!("    {note}");
        if rows.is_empty() {
            println!("  (none)");
            return;
        }
        for (id, where_) in rows {
            println!("  {id:9}  {}", where_.join("  "));
        }
    };
    show(
        "EPUB 3 only in epubcheck",
        "our emission must be gated on is_epub3 — probe each on an EPUB 2 book",
        &only3,
    );
    show(
        "EPUB 2 only in epubcheck (base method overridden in the …30 subclass)",
        "we must NOT emit these at 3.0 — this is the OPF-042 shape",
        &only2,
    );
    if show_all {
        println!("\n--- version-neutral ({} ids) ---", neutral.len());
        println!("  {}", neutral.join(" "));
    } else {
        println!(
            "\n{} further ids look version-neutral (--all to list)",
            neutral.len()
        );
    }
    println!(
        "\nEvery row above is a candidate, not a finding. Settle each the way the\n\
         previous fifteen were settled: one book, both tools, one difference."
    );
}
