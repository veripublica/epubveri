//! Per-book speed regression gate: the build about to be released against
//! the previous release, book by book.
//!
//! 0.19.0 shipped with "no book got slower" in its notes, measured against a
//! build that already held that release's changes. Against 0.18.0, seven
//! shelf books were more than 20% slower, the worst 0.11 s to 0.37 s, and a
//! whole-shelf total that went *down* 9% hid all of them. So this compares
//! against the previous *release binary* and judges each book on its own:
//! the same lesson `diff-shelf.sh` learned for findings.
//!
//! ```text
//! cargo run --release -p epubveri-harness --bin speed -- \
//!     --old <previous epubveri> --new <this epubveri> <book.epub|dir>...
//! ```
//!
//! Each binary validates each book in its own process (`-u -i <book>`,
//! output discarded), which is how the plugins call it. CPU time comes from
//! the kernel's accounting for that child (`getrusage(RUSAGE_CHILDREN)`
//! around the wait), not from a wall clock, so another process on the
//! machine costs less noise. Two passes per binary, whole passes and not
//! alternating per book (`docs/BENCHMARK.md` says why), and each book keeps
//! the lower of its two times.
//!
//! A book fails if the new build is more than 20% *and* more than 10 ms
//! slower: both, because a 4 ms book moving 1 ms is noise and a 3 s book
//! moving 20 ms is too. Exit 1 if any book fails, 2 on bad arguments.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const RATIO: f64 = 1.20;
const FLOOR_S: f64 = 0.010;

fn main() {
    let mut old: Option<PathBuf> = None;
    let mut new: Option<PathBuf> = None;
    let mut roots = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--old" => old = args.next().map(PathBuf::from),
            "--new" => new = args.next().map(PathBuf::from),
            _ => roots.push(PathBuf::from(a)),
        }
    }
    let (Some(old), Some(new)) = (old, new) else {
        eprintln!("usage: speed --old <epubveri> --new <epubveri> <book.epub|dir>...");
        std::process::exit(2);
    };
    for b in [&old, &new] {
        if !b.is_file() {
            eprintln!("not a file: {}", b.display());
            std::process::exit(2);
        }
    }
    let mut books = Vec::new();
    for r in &roots {
        collect(r, &mut books);
    }
    books.sort();
    if books.is_empty() {
        eprintln!("no .epub under the given paths");
        std::process::exit(2);
    }
    println!(
        "speed: {} book(s)\n  old {}\n  new {}",
        books.len(),
        version(&old),
        version(&new)
    );

    let old_t = best_of_two(&old, &books);
    let new_t = best_of_two(&new, &books);

    let (old_sum, new_sum): (f64, f64) = (old_t.iter().sum(), new_t.iter().sum());
    println!(
        "  CPU, whole set: old {old_sum:.2} s, new {new_sum:.2} s ({:+.1}%)",
        (new_sum / old_sum - 1.0) * 100.0
    );

    let mut slower: Vec<(f64, f64, &PathBuf)> = books
        .iter()
        .zip(old_t.iter().zip(&new_t))
        .filter(|(_, (o, n))| **n > **o * RATIO && **n - **o > FLOOR_S)
        .map(|(b, (o, n))| (*o, *n, b))
        .collect();
    slower.sort_by(|a, b| (b.1 - b.0).total_cmp(&(a.1 - a.0)));
    if slower.is_empty() {
        println!("  no book more than 20% and 10 ms slower than the old build");
        return;
    }
    println!(
        "  {} book(s) more than 20% and 10 ms slower than the old build:",
        slower.len()
    );
    for (o, n, b) in &slower {
        println!(
            "    {:7.3} s -> {:7.3} s  {}",
            o,
            n,
            b.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    std::process::exit(1);
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() {
        if p.extension().is_some_and(|e| e == "epub") {
            out.push(p.to_path_buf());
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        let path = e.path();
        // diff-shelf's snapshots live beside the books; they are not input.
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        collect(&path, out);
    }
}

fn version(bin: &Path) -> String {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn best_of_two(bin: &Path, books: &[PathBuf]) -> Vec<f64> {
    let first: Vec<f64> = books.iter().map(|b| run(bin, b)).collect();
    books
        .iter()
        .zip(first)
        .map(|(b, t)| t.min(run(bin, b)))
        .collect()
}

/// One validation in its own process; its CPU time where the platform
/// reports it, its wall time otherwise.
fn run(bin: &Path, book: &Path) -> f64 {
    let before = children_cpu();
    let start = Instant::now();
    let status = Command::new(bin)
        .args(["-u", "-i"])
        .arg(book)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let wall = start.elapsed().as_secs_f64();
    if status.is_err() {
        eprintln!("could not run {} on {}", bin.display(), book.display());
        std::process::exit(2);
    }
    match (before, children_cpu()) {
        (Some(b), Some(a)) => a - b,
        _ => wall,
    }
}

/// User plus system CPU of every waited-for child so far.
#[cfg(unix)]
fn children_cpu() -> Option<f64> {
    // SAFETY: getrusage only writes the struct it is given.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut ru) } != 0 {
        return None;
    }
    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    Some(tv(ru.ru_utime) + tv(ru.ru_stime))
}

#[cfg(not(unix))]
fn children_cpu() -> Option<f64> {
    None
}
