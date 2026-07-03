//! Thin CLI: `epubveri [--format human|ids] [--profile <name>] <file.epub>`
//!
//! Exit codes: 0 = valid (no errors), 1 = invalid (errors found), 2 = usage/IO error.

use std::path::Path;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage: epubveri [--format human|ids] [--profile dict|edupub|idx|preview] <file.epub>"
    );
    ExitCode::from(2)
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
                    None => return usage(),
                }
            }
            "--profile" => {
                i += 1;
                match args.get(i) {
                    Some(v) => profile = Some(v.clone()),
                    None => return usage(),
                }
            }
            "-h" | "--help" => return usage(),
            s => path = Some(s.to_string()),
        }
        i += 1;
    }

    let Some(path) = path else { return usage() };
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
                    .map(|l| format!(" [{l}]"))
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
