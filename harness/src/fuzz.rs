//! Mutation fuzzer: corrupts a small, feature-dense valid book thousands of
//! times and asks whether validating any of the results panics.
//!
//! A panic in the library is a crash for everyone who embeds it (epubsana, the
//! plugins, the WASM package, where a panic aborts), and the report the user
//! wanted never arrives. The hostile suite covers the shapes someone thought
//! of; this covers the ones nobody did. Its first run, as a throwaway script
//! during the 2026-09-23 security audit, found `htm::scan_references` slicing
//! inside `à` - 3 crashes in 2,400 books, all at one site, none of which any
//! test, corpus fixture or shelf book had reached.
//!
//! Deterministic by construction: the same `--seed` generates the same books,
//! so a crash reproduces from its seed and index alone, and the pre-flight
//! gate cannot go red on one run and green on the next. Explore new ground
//! with a different seed.
//!
//! Each book is validated in-process twice (as declared with `--advisory`,
//! and forced to EPUB 2 with `-v 2`), inside `catch_unwind`. A crashing book
//! is written under `target/fuzz-crashes/` and the run exits 1.
//!
//! Usage:
//!     cargo run --release -p epubveri-harness --bin fuzz
//!     cargo run --release -p epubveri-harness --bin fuzz -- --seed 7 --books 20000
//!
//! Not a coverage-guided fuzzer: that needs libFuzzer, which is C++ and a
//! nightly toolchain, against the pure-Rust tooling rule. This is the cheap
//! half, and the half that has already paid for itself.

use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/p.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

const OPF: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" prefix="rendition: http://www.idpf.org/vocab/rendition/#">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language><dc:creator id="c">A</dc:creator><meta refines="#c" property="role" scheme="marc:relators">aut</meta><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta><meta property="rendition:layout">reflowable</meta></metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml" properties="svg"/><item id="css" href="s.css" media-type="text/css"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="img" href="i.svg" media-type="image/svg+xml"/></manifest>
<spine toc="ncx"><itemref idref="c1"/></spine><guide><reference type="text" href="c1.xhtml#a" title="t"/></guide></package>"##;

const NAV: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>t</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml#a">1</a></li></ol></nav><nav epub:type="landmarks"><ol><li><a epub:type="bodymatter" href="c1.xhtml">b</a></li></ol></nav></body></html>"#;

const C1: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="en"><head><title>t</title><link rel="stylesheet" href="s.css"/><style>p{margin:1em}</style></head><body><section epub:type="chapter" id="a"><h1>H&amp;</h1><p class="x" aria-level="2" role="heading">T&#x2014;x <a href="#a">l</a></p><table><tr><td colspan="2">c</td></tr></table><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10"><image xlink:href="i.svg" width="10" height="10"/></svg><math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math><img src="i.svg" alt="i"/></section></body></html>"##;

const CSS: &str = "@charset \"utf-8\";\n@media screen and (min-width: 10px){p{font:italic bold 12px/30px Georgia,serif;color:rgb(1,2,3)}}\n@font-face{font-family:\"F\";src:url(i.svg)}\n.x::before{content:\"\\201C\";counter-increment:c 2}\n";

const NCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head><docTitle><text>T</text></docTitle><navMap><navPoint id="n1" playOrder="1"><navLabel><text>1</text></navLabel><content src="c1.xhtml#a"/></navPoint></navMap></ncx>"#;

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"#;

/// Fragments that sit on the boundaries parsers get wrong: markup
/// delimiters, entity starts, CSS structure, a multi-byte letter whose second
/// byte is Unicode whitespace when read alone (`à`), a BOM, a NUL, a number
/// too large for any integer type.
const TOKENS: &[&str] = &[
    "<",
    ">",
    "&",
    "&#",
    "&#x",
    "\"",
    "'",
    "/",
    "</p>",
    "<p>",
    ";",
    "{",
    "}",
    "(",
    ")",
    "\\",
    "@",
    "url(",
    "#",
    "*",
    ":",
    "\u{e0}",
    "\u{c5}",
    "\u{200b}",
    "\u{d7ff}",
    "%",
    "..",
    "\0",
    "\u{feff}",
    "=",
    "-->",
    "<!--",
    "<![CDATA[",
    "]]>",
    "xlink:",
    "epub:",
    "@media",
    "!important",
    "calc(",
    "var(--",
    "\n",
    "0x",
    "99999999999999999999",
    "-1",
    "",
    " ",
];

/// xorshift64*: enough randomness to walk the input space, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// One to six edits to `s`, on characters so the result stays UTF-8.
fn mutate(s: &str, rng: &mut Rng) -> Vec<u8> {
    let mut c: Vec<char> = s.chars().collect();
    for _ in 0..1 + rng.below(6) {
        let i = rng.below(c.len() + 1);
        match rng.below(100) {
            0..35 => {
                let t = TOKENS[rng.below(TOKENS.len())];
                c.splice(i..i, t.chars());
            }
            35..60 if !c.is_empty() => {
                let end = (i + 1 + rng.below(8)).min(c.len());
                c.drain(i.min(c.len())..end);
            }
            60..80 if !c.is_empty() => {
                let (a, b) = (rng.below(c.len()), rng.below(c.len()));
                let run: Vec<char> = c[a.min(b)..a.max(b)].iter().take(200).copied().collect();
                c.splice(i..i, run);
            }
            _ => {
                let ch = char::from_u32(1 + rng.below(0x2FFF) as u32).unwrap_or('\u{1F600}');
                c.insert(i, ch);
            }
        }
    }
    let mut bytes = String::from_iter(c).into_bytes();
    // Occasionally break the encoding itself: a stray continuation byte or a
    // truncated sequence, which no &str-typed input can express.
    if rng.chance(10) && !bytes.is_empty() {
        let i = rng.below(bytes.len());
        bytes[i] = [0x80, 0xA0, 0x85, 0xC3, 0xE2, 0xFF][rng.below(6)];
    }
    bytes
}

fn book(rng: &mut Rng) -> Vec<u8> {
    let mut files: Vec<(&str, Vec<u8>)> = vec![
        ("META-INF/container.xml", CONTAINER.as_bytes().to_vec()),
        ("OEBPS/p.opf", OPF.as_bytes().to_vec()),
        ("OEBPS/nav.xhtml", NAV.as_bytes().to_vec()),
        ("OEBPS/c1.xhtml", C1.as_bytes().to_vec()),
        ("OEBPS/s.css", CSS.as_bytes().to_vec()),
        ("OEBPS/toc.ncx", NCX.as_bytes().to_vec()),
        ("OEBPS/i.svg", SVG.as_bytes().to_vec()),
    ];
    for _ in 0..1 + rng.below(3) {
        let k = rng.below(files.len());
        let text = String::from_utf8_lossy(&files[k].1).into_owned();
        files[k].1 = mutate(&text, rng);
    }
    let mut out = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut out));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("mimetype", stored).expect("zip");
        zip.write_all(b"application/epub+zip").expect("zip");
        for (name, data) in &files {
            zip.start_file(*name, deflated).expect("zip");
            zip.write_all(data).expect("zip");
        }
        zip.finish().expect("zip");
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |flag: &str, default: u64| -> u64 {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };
    let seed = arg("--seed", 1);
    let books = arg("--books", 3000);
    // Anything slower than this on a book this small is a finding too, even
    // though in-process it cannot be interrupted, only reported afterwards.
    let slow = Duration::from_secs(5);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("fuzz-crashes");
    let arms = [
        epubveri::Options {
            advisory: true,
            ..Default::default()
        },
        epubveri::Options {
            epub_version: Some("2".to_string()),
            ..Default::default()
        },
    ];

    // The default panic hook stays: its message names the panicking line,
    // which is the first thing a crash needs.
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let (mut crashes, mut slows) = (0usize, 0usize);
    let start = Instant::now();
    for i in 0..books {
        let bytes = book(&mut rng);
        for (arm, opts) in arms.iter().enumerate() {
            let t = Instant::now();
            let r = catch_unwind(AssertUnwindSafe(|| {
                epubveri::validate_bytes_with_options(bytes.clone(), opts)
            }));
            let took = t.elapsed();
            let failure = if r.is_err() {
                crashes += 1;
                Some("PANIC")
            } else if took > slow {
                slows += 1;
                Some("SLOW")
            } else {
                None
            };
            if let Some(what) = failure {
                std::fs::create_dir_all(&out).expect("create crash dir");
                let path = out.join(format!("seed{seed}-book{i}-arm{arm}.epub"));
                std::fs::write(&path, &bytes).expect("write crash book");
                println!(
                    "  {what:<5} seed {seed} book {i} arm {arm} ({took:.1?}) -> {}",
                    path.display()
                );
                break;
            }
        }
    }
    println!(
        "fuzz: seed {seed}, {books} books x 2 arms in {:.1?}: {crashes} panic(s), {slows} slow",
        start.elapsed()
    );
    if crashes + slows > 0 {
        std::process::exit(1);
    }
}
