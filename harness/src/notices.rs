//! Generate `LICENSE-THIRD-PARTY.txt`: the licence texts of everything compiled
//! into what we ship.
//!
//! **Why this exists.** The release archives and the npm package hold compiled
//! code from other people's crates, and MIT, Apache-2.0, Zlib and Unicode-3.0
//! all ask for their notice to travel with a binary. Our archives carried only
//! our own licence. Written once by hand, the list would go stale at the next
//! `cargo update`, so it is generated, and `--check` fails the pre-flight when
//! it has.
//!
//! **What counts as shipped.** `cargo tree` over `epubveri` and `epubveri-wasm`
//! for the targets we publish, normal dependencies, proc macros left out because
//! they run at build time and leave nothing in the binary. That is an upper bound:
//! the linker may drop a crate listed here, and listing one too many costs nothing.
//!
//! **Where a crate offers a choice** (`MIT OR Apache-2.0`) we take MIT, then Zlib,
//! then Apache-2.0, and print only that text. An unknown licence stops the run, so
//! a new one is read by a person rather than filed under a default.
//!
//! This is a list of what the crates say about themselves, not legal advice.
//!
//! Usage:
//!     cargo run --release -p epubveri-harness --bin notices            # write both copies
//!     cargo run --release -p epubveri-harness --bin notices -- --check # compare, exit 1 if stale

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The eight in `release-binaries.yml`, and the npm package's.
const TARGETS: [&str; 9] = [
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "wasm32-unknown-unknown",
];

/// Same licence and same authors as this project, covered by `LICENSE`.
const OURS: [&str; 3] = ["epubveri", "epubveri-wasm", "styloria"];

/// Files that may hold each licence's text, most specific first.
const FILES: [(&str, &[&str]); 4] = [
    (
        "MIT",
        &[
            "LICENSE-MIT",
            "LICENSE_MIT",
            "LICENSE-MIT.md",
            "LICENSE-MIT.txt",
            "license-mit",
            "LICENSE.md",
            "LICENSE",
            "LICENSE.txt",
        ],
    ),
    (
        "Zlib",
        &["LICENSE-ZLIB", "LICENSE-ZLIB.md", "LICENSE", "LICENSE.md"],
    ),
    (
        "Apache-2.0",
        &[
            "LICENSE-APACHE",
            "LICENSE_APACHE",
            "LICENSE-APACHE.md",
            "LICENSE-APACHE.txt",
            "COPYING",
            "LICENSE",
        ],
    ),
    ("Unicode-3.0", &["LICENSE-UNICODE"]),
];

const MIT_TEMPLATE: &str = "Copyright (c) AUTHORS

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the \"Software\"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN
AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
";

/// The licences to reproduce for an SPDX expression: one per `AND` term, each
/// term's `OR` choice settled by the preference above. `None` for anything else.
fn chosen(expr: &str) -> Option<Vec<&'static str>> {
    let cleaned = expr.replace(['(', ')'], " ").replace('/', " OR ");
    let mut out = Vec::new();
    for term in cleaned.split(" AND ") {
        let offered: Vec<&str> = term.split(" OR ").map(str::trim).collect();
        let pick = ["MIT", "Zlib", "Apache-2.0", "Unicode-3.0"]
            .into_iter()
            .find(|known| offered.contains(known))?;
        out.push(pick);
    }
    Some(out)
}

fn read_licence(dir: &Path, licence: &str) -> Option<String> {
    let names = FILES.iter().find(|(l, _)| *l == licence)?.1;
    names
        .iter()
        .find_map(|name| std::fs::read_to_string(dir.join(name)).ok())
}

struct Crate {
    name: String,
    version: String,
    expr: String,
    authors: Vec<String>,
    dir: PathBuf,
}

/// `name version` of everything `cargo tree` says these packages link.
fn shipped(root: &Path) -> BTreeSet<(String, String)> {
    let out = Command::new("cargo")
        .current_dir(root)
        .args([
            "tree",
            "--locked",
            "-e",
            "normal,no-proc-macro",
            "-p",
            "epubveri",
            "-p",
            "epubveri-wasm",
            "--prefix",
            "none",
            "-f",
            "{p}",
        ])
        .args(TARGETS.iter().flat_map(|t| ["--target", t]))
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let version = words.next()?.strip_prefix('v')?;
            Some((name.to_string(), version.to_string()))
        })
        .collect()
}

fn metadata(root: &Path) -> Vec<Crate> {
    let out = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .expect("cargo metadata runs");
    assert!(out.status.success(), "cargo metadata failed");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata is JSON");
    doc["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| {
            let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
            Crate {
                name: text("name"),
                version: text("version"),
                expr: text("license"),
                authors: p["authors"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.split(" <").next().unwrap_or(s).to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                dir: Path::new(p["manifest_path"].as_str().unwrap_or_default())
                    .parent()
                    .expect("a manifest has a directory")
                    .to_path_buf(),
            }
        })
        .collect()
}

fn generate(root: &Path) -> String {
    let wanted = shipped(root);
    let mut by_text: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut seen = 0;
    for c in metadata(root) {
        if !wanted.contains(&(c.name.clone(), c.version.clone())) || OURS.contains(&c.name.as_str())
        {
            continue;
        }
        seen += 1;
        let licences = chosen(&c.expr).unwrap_or_else(|| {
            panic!(
                "{} {}: read this licence and add it to `chosen`: {}",
                c.name, c.version, c.expr
            )
        });
        for licence in licences {
            let text = read_licence(&c.dir, licence).unwrap_or_else(|| {
                // MIT asks for the copyright notice, and the manifest is the
                // only place left that names the holders.
                assert!(
                    licence == "MIT" && !c.authors.is_empty(),
                    "{} {}: no {licence} text in the package",
                    c.name,
                    c.version
                );
                MIT_TEMPLATE.replace("AUTHORS", &c.authors.join(", "))
            });
            let text = text.replace("\r\n", "\n").trim().to_string();
            by_text
                .entry(text)
                .or_default()
                .insert(format!("{} {} ({licence})", c.name, c.version));
        }
    }
    assert!(seen > 0, "no crates found: the instrument did not run");

    let mut sections: Vec<(Vec<String>, String)> = by_text
        .into_iter()
        .map(|(text, crates)| (crates.into_iter().collect(), text))
        .collect();
    sections.sort();

    let mut out = String::from(
        "Third-party licences for epubveri\n\
         \n\
         The epubveri programs and the WebAssembly package contain compiled code from\n\
         the crates below. Where a crate offers a choice of licences, the one named here\n\
         is the one taken. epubveri itself is under LICENSE or LICENSE-COMMERCIAL.md.\n\
         styloria is by the same author, under the same terms.\n\
         \n\
         Generated by `cargo run -p epubveri-harness --bin notices`; do not edit.\n",
    );
    for (crates, text) in sections {
        out.push_str("\n\n================================================================\n");
        for c in crates {
            out.push_str(&c);
            out.push('\n');
        }
        out.push_str("================================================================\n\n");
        out.push_str(&text);
        out.push('\n');
    }
    out
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let check = std::env::args().any(|a| a == "--check");
    let text = generate(&root);
    // wasm-pack copies `LICENSE*` from the crate it builds, so the npm package
    // and the demo site get their copy from the second path.
    let paths = [
        root.join("LICENSE-THIRD-PARTY.txt"),
        root.join("epubveri-wasm/LICENSE-THIRD-PARTY.txt"),
    ];
    if check {
        let stale: Vec<_> = paths
            .iter()
            .filter(|p| std::fs::read_to_string(p).ok().as_deref() != Some(text.as_str()))
            .collect();
        if !stale.is_empty() {
            eprintln!("stale: {stale:?}; run --bin notices");
            std::process::exit(1);
        }
        println!("LICENSE-THIRD-PARTY.txt is up to date");
    } else {
        for p in &paths {
            std::fs::write(p, &text).expect("write the notices");
        }
        println!("wrote {} bytes to {} files", text.len(), paths.len());
    }
}

#[cfg(test)]
mod tests {
    use super::chosen;

    #[test]
    fn a_choice_settles_on_the_preferred_licence() {
        assert_eq!(chosen("MIT OR Apache-2.0"), Some(vec!["MIT"]));
        assert_eq!(chosen("Apache-2.0 OR MIT"), Some(vec!["MIT"]));
        assert_eq!(chosen("MIT/Apache-2.0"), Some(vec!["MIT"]));
        assert_eq!(chosen("Zlib OR Apache-2.0 OR MIT"), Some(vec!["MIT"]));
        assert_eq!(chosen("Unlicense OR MIT"), Some(vec!["MIT"]));
        assert_eq!(
            chosen("MIT OR Apache-2.0 OR LGPL-2.1-or-later"),
            Some(vec!["MIT"])
        );
        assert_eq!(chosen("Zlib"), Some(vec!["Zlib"]));
        assert_eq!(chosen("Apache-2.0"), Some(vec!["Apache-2.0"]));
        assert_eq!(
            chosen("(MIT OR Apache-2.0) AND Unicode-3.0"),
            Some(vec!["MIT", "Unicode-3.0"])
        );
    }

    #[test]
    fn an_unknown_licence_is_not_filed_under_a_default() {
        assert_eq!(chosen("GPL-3.0-only"), None);
        assert_eq!(chosen("MIT AND GPL-3.0-only"), None);
        assert_eq!(chosen(""), None);
    }
}
