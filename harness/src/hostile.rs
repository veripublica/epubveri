//! Generate EPUBs designed to break the validator, run them, and say
//! whether any of them still wins.
//!
//! **Why this exists.** The two instruments this project trusts — the
//! epubcheck corpus and the real-book shelf — both answer one question:
//! *what verdict do we give a well-formed book?* Neither can see a crash, a
//! resource-exhaustion bug, or a performance cliff, because none of those
//! inputs is a well-formed book. Six ways an ordinary `.epub` could kill the
//! process that validated it were fixed in 0.9.0/0.9.1, and the corpus was
//! byte-identical and the shelf unchanged per book through every one.
//!
//! So the adversarial input has to be written by hand, and it is the
//! generators rather than the files that are worth keeping: the guards
//! already have unit tests, but these are what would find the *next* class.
//! Every shape below is one that actually beat a shipped release, except
//! `xxe` and `entry-count`, which never did and are kept so that stays true.
//!
//! **What counts as a failure** is not "reported an error" — every one of
//! these files should draw a finding, and several are meant to be INVALID.
//! The failures are an **abort** (the process died; in Rust a stack overflow
//! is `SIGABRT`, not a catchable panic, so an embedder cannot defend against
//! it downstream), a **panic**, or a **timeout** (for a hostile input, no
//! answer is the same denial of service as a crash).
//!
//! **What this deliberately does not catch.** Two of the six bugs it was
//! built from would pass. The zip bomb against v0.8.6 exits 0 and reports
//! VALID — it merely consumes 1.3 GB doing so, and neither peak memory nor
//! the verdict is asserted here. Watch memory yourself when adding a shape
//! whose failure mode is exhaustion rather than a signal.
//!
//! Usage:
//!     cargo run --release -p epubveri-harness --bin hostile
//!     cargo run --release -p epubveri-harness --bin hostile -- --scale
//!     ... -- --bin /path/to/older/epubveri     # check the alarm still rings
//!
//! Output goes under `target/`, so no generated EPUB is ever committed.
//! This replaced a Python generator and a bash runner (both 2026-08-03, same
//! day): the bash version could not enforce a timeout on macOS, which ships
//! no `timeout(1)`, and measured wall time by parsing `/usr/bin/time -p`.
//! Both are ordinary here.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A 1x1 PNG, embedded rather than generated: building one needs a CRC and a
/// deflate stream, and `flate2`/`crc32fast` are only in this workspace as
/// private transitive dependencies of `zip`. 69 bytes is cheaper than a
/// dependency edge.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f,
    0x00, 0x05, 0xfe, 0x02, 0xfe, 0x0d, 0xef, 0x46, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

const CONTAINER: &str = concat!(
    r#"<?xml version="1.0"?><container version="1.0" "#,
    r#"xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>"#,
    r#"<rootfile full-path="OEBPS/c.opf" "#,
    r#"media-type="application/oebps-package+xml"/></rootfiles></container>"#
);

/// A minimal, otherwise-valid package document. `items`/`spine` are appended
/// to the nav document's own entries, so every generated book is valid apart
/// from the one thing it is testing.
fn opf(items: &str, spine: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" "#,
            r#"version="3.0" unique-identifier="i"><metadata "#,
            r#"xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:identifier id="i">x</dc:identifier><dc:title>t</dc:title>"#,
            r#"<dc:language>en</dc:language>"#,
            r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#,
            r#"</metadata><manifest><item id="n" href="n.xhtml" "#,
            r#"media-type="application/xhtml+xml" properties="nav"/>{}</manifest>"#,
            r#"<spine><itemref idref="n"/>{}</spine></package>"#
        ),
        items, spine
    )
}

fn nav(head: &str, body: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" "#,
            r#"xmlns:epub="http://www.idpf.org/2007/ops"><head><title>n</title>{}"#,
            r#"</head><body><nav epub:type="toc"><ol><li><a href="n.xhtml">n</a>"#,
            r#"</li></ol></nav>{}</body></html>"#
        ),
        head, body
    )
}

/// Deliberately not shared with `wrap.rs`'s near-identical writer: that one
/// belongs to the corpus binary and carries its fixture-specific shapes, and
/// reaching into it would mean compiling all of it into this binary for ten
/// lines.
fn write_epub(path: &Path, entries: &[(String, Vec<u8>)]) {
    let file = std::fs::File::create(path).expect("create epub");
    let mut zip = ZipWriter::new(file);
    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )
    .expect("mimetype");
    zip.write_all(b"application/epub+zip").expect("mimetype");
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(name.as_str(), deflated).expect("start_file");
        zip.write_all(data).expect("write entry");
    }
    zip.finish().expect("finish zip");
}

fn book(path: &Path, opf_xml: String, nav_xml: String, extra: Vec<(String, Vec<u8>)>) {
    let mut entries = vec![
        ("META-INF/container.xml".to_string(), CONTAINER.into()),
        ("OEBPS/c.opf".to_string(), opf_xml.into_bytes()),
        ("OEBPS/n.xhtml".to_string(), nav_xml.into_bytes()),
    ];
    entries.extend(extra);
    write_epub(path, &entries);
}

// --- the shapes, each with the failure it used to cause ----------------------

/// Stack overflow in roxmltree's mutually recursive tokenizer. Aborted at
/// ~15,000 deep on an 8 MiB main thread and ~4,000 on a 2 MiB worker, from a
/// file of about 1.1 KB. Guard: `ocf::MAX_XML_DEPTH`.
fn gen_xml_depth(out: &Path) {
    let d = 50_000;
    let body = format!("{}x{}", "<div>".repeat(d), "</div>".repeat(d));
    book(
        &out.join("xml-depth.epub"),
        opf("", ""),
        nav("", &body),
        Vec::new(),
    );
}

/// Unbounded inflation of one compressed entry: a 400 KB EPUB drove 1.3 GB of
/// peak RSS and still reported VALID, because the read was a bare
/// `read_to_end`. Guard: `ocf::MAX_ENTRY_BYTES`, reported as LIM-001.
fn gen_zip_bomb(out: &Path) {
    let mut big = String::with_capacity(400 * 1024 * 1024 + 128);
    big.push_str(r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml">"#);
    big.push_str("<head><title>b</title></head><body><p>");
    big.extend(std::iter::repeat_n('A', 400 * 1024 * 1024));
    big.push_str("</p></body></html>");
    book(
        &out.join("zip-bomb.epub"),
        opf(
            r#"<item id="b" href="big.xhtml" media-type="application/xhtml+xml"/>"#,
            r#"<itemref idref="b"/>"#,
        ),
        nav("", ""),
        vec![("OEBPS/big.xhtml".to_string(), big.into_bytes())],
    );
}

/// Four routes into the same mutual recursion in styloria's parser
/// (`consume_component_value` <-> `consume_simple_block`). All four aborted
/// between 10,000 and 20,000 deep on an 8 MiB main thread, from stylesheets of
/// about 1.2 KB. Guard: `styloria::MAX_NESTING_DEPTH`, reported as CSS-008 /
/// `css.stylesheet.nesting_too_deep`.
fn gen_css_nesting(out: &Path) {
    let n = 100_000;
    let shapes: [(&str, String); 4] = [
        (
            "paren",
            format!("a{{color:{}red{}}}", "(".repeat(n), ")".repeat(n)),
        ),
        (
            "curly",
            format!("{}a{{color:red}}{}", "@media all{".repeat(n), "}".repeat(n)),
        ),
        (
            "function",
            format!("a{{color:{}1{}}}", "rgb(".repeat(n), ")".repeat(n)),
        ),
        (
            "selector",
            format!("{}a{}{{color:red}}", ":is(".repeat(n), ")".repeat(n)),
        ),
    ];
    for (name, css) in shapes {
        book(
            &out.join(format!("css-{name}.epub")),
            opf(r#"<item id="s" href="s.css" media-type="text/css"/>"#, ""),
            nav(
                r#"<link rel="stylesheet" href="s.css" type="text/css"/>"#,
                "",
            ),
            vec![("OEBPS/s.css".to_string(), css.into_bytes())],
        );
    }
}

/// An external-entity reference. Never exploitable — roxmltree does no I/O —
/// but cheap to keep honest about, and the shape a reviewer asks for.
fn gen_xxe(out: &Path) {
    let opf_xml = concat!(
        r#"<?xml version="1.0"?><!DOCTYPE package [<!ENTITY xxe SYSTEM "#,
        r#""file:///etc/passwd">]><package "#,
        r#"xmlns="http://www.idpf.org/2007/opf" version="3.0" "#,
        r#"unique-identifier="i"><metadata "#,
        r#"xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
        r#"<dc:identifier id="i">&xxe;</dc:identifier><dc:title>t</dc:title>"#,
        r#"<dc:language>en</dc:language>"#,
        r#"<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>"#,
        r#"</metadata><manifest><item id="n" href="n.xhtml" "#,
        r#"media-type="application/xhtml+xml" properties="nav"/></manifest>"#,
        r#"<spine><itemref idref="n"/></spine></package>"#
    );
    book(
        &out.join("xxe.epub"),
        opf_xml.to_string(),
        nav("", ""),
        Vec::new(),
    );
}

/// Many tiny ZIP entries. Measured linear and fast — kept so that stays true,
/// not because it ever failed.
fn gen_entry_count(out: &Path) {
    let extra: Vec<(String, Vec<u8>)> = (0..100_000)
        .map(|i| (format!("OEBPS/pad{i}.txt"), b"x".to_vec()))
        .collect();
    book(
        &out.join("entry-count.epub"),
        opf("", ""),
        nav("", ""),
        extra,
    );
}

/// The manifest-size ladder that exposed the quadratic. Every resource is
/// referenced from the content document, so no finding is produced and what
/// is measured is pure validation. Before 0.9.1 this was 42.6s at 4,000
/// items; it is linear now, so a regression shows up as the ladder bending
/// rather than as any one number.
fn gen_scale(out: &Path) {
    for n in [1000usize, 2000, 4000, 8000] {
        let mut items = String::new();
        let mut body = String::new();
        let mut extra = Vec::with_capacity(n);
        for i in 0..n {
            items.push_str(&format!(
                r#"<item id="p{i}" href="p{i}.png" media-type="image/png"/>"#
            ));
            body.push_str(&format!(r#"<p><img src="p{i}.png" alt="a"/></p>"#));
            extra.push((format!("OEBPS/p{i}.png"), TINY_PNG.to_vec()));
        }
        book(
            &out.join(format!("scale-{n}.epub")),
            opf(&items, ""),
            nav("", &body),
            extra,
        );
    }
}

// --- running -----------------------------------------------------------------

enum Outcome {
    Ok(Duration),
    Abort(String),
    Panic,
    Timeout,
}

/// Run the validator on one file, bounded by `timeout`.
///
/// Polls rather than blocking on `wait()`, because the child has to stay
/// killable from here — that is the whole reason this is not a shell script,
/// where the same thing needs a `timeout(1)` that macOS does not ship. The
/// 1 ms interval bounds the timing error well under the 0.13s floor the scale
/// ladder measures.
fn run_one(bin: &Path, epub: &Path, timeout: Duration) -> Outcome {
    let start = Instant::now();
    let mut child = match Command::new(bin)
        .arg("-i")
        .arg(epub)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Outcome::Abort(format!("could not run {}: {e}", bin.display())),
    };

    // Drained on a thread so a chatty child cannot fill the pipe and block
    // while we are waiting for it to exit.
    let mut pipe = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = pipe.as_mut() {
            p.read_to_string(&mut s).ok();
        }
        s
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Outcome::Timeout;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => return Outcome::Abort(format!("wait failed: {e}")),
        }
    };
    let elapsed = start.elapsed();
    let err = reader.join().unwrap_or_default();

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return Outcome::Abort(format!("signal {sig}"));
        }
    }
    if err.contains("stack overflow") {
        return Outcome::Abort("stack overflow".to_string());
    }
    if status.code() == Some(101) || err.contains("panicked") {
        return Outcome::Panic;
    }
    Outcome::Ok(elapsed)
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| i + 1);

    let scale = args.iter().any(|a| a == "--scale");
    let out = flag("--out")
        .and_then(|i| args.get(i))
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/hostile"));
    let bin = flag("--bin")
        .and_then(|i| args.get(i))
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/epubveri"));
    let timeout = Duration::from_secs(
        flag("--timeout")
            .and_then(|i| args.get(i))
            .and_then(|s| s.parse().ok())
            .unwrap_or(120),
    );

    if !bin.is_file() {
        eprintln!("no validator at {} — cargo build --release", bin.display());
        std::process::exit(2);
    }
    std::fs::create_dir_all(&out).expect("create output dir");

    eprintln!("generating into {}", out.display());
    gen_xml_depth(&out);
    gen_zip_bomb(&out);
    gen_css_nesting(&out);
    gen_xxe(&out);
    gen_entry_count(&out);
    if scale {
        gen_scale(&out);
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(&out)
        .expect("read output dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "epub"))
        .collect();
    files.sort();

    let mut failed = 0usize;
    for f in &files {
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match run_one(&bin, f, timeout) {
            Outcome::Ok(d) if name.starts_with("scale-") => {
                println!("  ok    {name:<22} {:.2}s", d.as_secs_f64())
            }
            Outcome::Ok(_) => println!("  ok    {name:<22}"),
            Outcome::Abort(why) => {
                println!("  FAIL  {name:<22} ABORT ({why})");
                failed += 1;
            }
            Outcome::Panic => {
                println!("  FAIL  {name:<22} PANIC");
                failed += 1;
            }
            Outcome::Timeout => {
                println!("  FAIL  {name:<22} TIMEOUT (>{}s)", timeout.as_secs());
                failed += 1;
            }
        }
    }

    if failed > 0 {
        eprintln!("hostile corpus: {failed} failure(s) above");
        std::process::exit(1);
    }
    println!("hostile corpus: no aborts, panics or timeouts");
}
