//! Bakes a git build-metadata suffix into the crate version so `-V`,
//! [`epubveri::VERSION`] and the wasm binding all print the exact source they
//! were built from (veripublica conventions v0.4, CLI.md §3.1).
//!
//! Emits `EPUBVERI_BUILD` as `+<short-hash>[.dirty]`, or **empty** when there is
//! no git checkout to read (e.g. a crates.io tarball, which ships no `.git`) —
//! the plain SemVer then stands, silently. Build metadata is ignored for
//! SemVer precedence, so consumers see an unchanged version shape either way.

use std::path::Path;
use std::process::Command;

fn main() {
    let build = git_build_metadata().unwrap_or_default();
    println!("cargo:rustc-env=EPUBVERI_BUILD={build}");

    // Re-bake when the checked-out commit or the staging state changes. HEAD
    // covers a branch switch; the branch ref covers a new commit; the index
    // covers staging (the coarse signal behind `.dirty`). A build from a
    // tarball with no `.git` simply watches nothing and stays on the plain
    // version.
    watch(".git/HEAD");
    watch(".git/index");
    if let Ok(head) = std::fs::read_to_string(".git/HEAD")
        && let Some(refname) = head.strip_prefix("ref:")
    {
        watch(&format!(".git/{}", refname.trim()));
    }
}

fn watch(path: &str) {
    if Path::new(path).exists() {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn git_build_metadata() -> Option<String> {
    let hash = git(&["rev-parse", "--short=7", "HEAD"])?;
    let dirty = match git(&["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => ".dirty",
        _ => "",
    };
    Some(format!("+{hash}{dirty}"))
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.trim().to_string())
}
