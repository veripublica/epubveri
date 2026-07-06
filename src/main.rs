//! Thin CLI: `epubveri [--format human|ids] [--profile <name>] <file.epub>`
//!
//! Exit codes: 0 = valid (no errors), 1 = invalid (errors found), 2 = usage/IO error.

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
epubveri — a pure-Rust EPUB validator

USAGE:
    epubveri [OPTIONS] <file.epub>

OPTIONS:
    --format <human|ids>                 output format (default: human)
    --profile <dict|edupub|idx|preview>  also check against an EPUB extension profile
    -V, --version                        print version and exit
    -h, --help                           print this help and exit

Exit codes: 0 = valid, 1 = errors found, 2 = usage/IO error.";

/// Help was explicitly requested: print to stdout and exit successfully.
fn help() -> ExitCode {
    println!("{USAGE}");
    ExitCode::SUCCESS
}

/// The invocation was wrong (missing/invalid arguments): print to stderr and
/// exit with the usage-error code.
fn usage_error() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn version() -> ExitCode {
    println!("epubveri {}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut format = String::from("human");
    let mut profile: Option<String> = None;
    let mut path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => {
                i += 1;
                match args.get(i) {
                    Some(v) => format = v.clone(),
                    None => return usage_error(),
                }
            }
            "--profile" => {
                i += 1;
                match args.get(i) {
                    Some(v) => profile = Some(v.clone()),
                    None => return usage_error(),
                }
            }
            "-h" | "--help" => return help(),
            "-V" | "--version" => return version(),
            s => path = Some(s.to_string()),
        }
        i += 1;
    }

    let Some(path) = path else {
        return usage_error();
    };
    let report = match epubveri::validate_path_with_profile(Path::new(&path), profile.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    match format.as_str() {
        "ids" => {
            for m in &report.messages {
                println!("{}", m.id);
            }
        }
        _ => {
            for m in &report.messages {
                let loc = m
                    .location
                    .as_deref()
                    .map(|l| match m.position {
                        Some(p) => format!(" [{l}:{}:{}]", p.line, p.column),
                        None => format!(" [{l}]"),
                    })
                    .unwrap_or_default();
                println!("{} {}: {}{}", m.severity, m.id, m.text, loc);
            }
            println!(
                "— {} error(s), {} warning(s): {}",
                report.errors(),
                report.warnings(),
                if report.is_valid() {
                    "VALID"
                } else {
                    "INVALID"
                }
            );
        }
    }

    if report.is_valid() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
