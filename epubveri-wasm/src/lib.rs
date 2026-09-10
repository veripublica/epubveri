//! WebAssembly bindings for [`epubveri`] — validate an EPUB entirely in the
//! browser (or any JS runtime), with no JVM, no server round-trip, no C deps.
//!
//! This crate is a thin boundary: it hands raw `.epub` bytes to the core
//! [`epubveri::validate_bytes_with_profile`] and maps its
//! [`epubveri::report::Report`] into the veripublica machine envelope's
//! **`inputs[i]` shape** (FORMATS.md §1.2) — minus the CLI-only `path`/`error`
//! fields, since a JS caller has neither. A caller therefore reads the *same*
//! object the CLI's `--format json` emits for each input: one shape, one parser,
//! across CLI, CI and the browser demo. These structs mirror
//! [`epubveri::envelope`]; keep them in step.
//!
//! ```js
//! import init, { validate } from "epubveri-wasm";
//! await init();
//! const report = validate(new Uint8Array(epubArrayBuffer), undefined, undefined);
//! // pass `true` as the third argument to also run the opt-in advisory checks.
//! // report.status === "ok" | "problems"
//! // report.summary.errors, report.items[i].code, report.items[i].severity, ...
//! ```

use std::collections::BTreeMap;

use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// One EPUB's validation result — the envelope's `inputs[i]` object without
/// `path`/`error` (a wasm caller has no path, and in-memory bytes are always
/// readable, so there is no unprocessable/`"error"` case here).
//
// NB: everything below is a `//` comment, not `///`, on purpose — a doc comment
// on this struct is copied verbatim into the published `.d.ts` and reaches npm
// consumers in their editor. Internal reasoning does not belong there.
//
// **`into_wasm_abi` is deprecated upstream, and `tsify` is held at 0.5.6 in
// `Cargo.lock` because of it (tsify 0.5.7, its issue #65).** The deprecation is
// written around a *memory leak*, and
// that leak is on the **`from_wasm_abi`** side, which this crate does not use:
// deserializing bad input from JavaScript cannot report failure at the ABI
// boundary, so it ends in `wasm_bindgen::throw_str`, which skips destructors.
// Our only input is `&[u8]`, handled by wasm-bindgen itself.
//
// The direction we do use fails differently — tsify's own changelog:
// *"`into_wasm_abi` panics rather than leaks on failure"* — and that panic is
// unreachable for this type, whose every field is owned `String`/`usize`/
// `Vec` with no serializer that can fail. Upstream states the attributes still
// work and plans no removal.
//
// The replacement, `Ts<T>`, would leave the generated TypeScript **identical**
// (`WasmDescribe for Ts<T>` forwards to `T::JsType::describe()`), so this is a
// Rust-side signature choice, not a consumer-visible one. Revisit it if this
// crate ever takes a structured value *from* JavaScript — that is the case the
// wrapper exists for, and the moment the pin should be lifted.
//
// The pin is not a muzzle: 0.5.7+ makes the attribute a `deprecated` warning,
// CI runs `-D warnings`, and an `#[allow(deprecated)]` on this item does
// **not** reach it — the reference lives in a nested anonymous const the
// derive generates. A private `mod` with an inner `#![allow(deprecated)]` DOES
// scope it (measured 2026-09-10), so the pin is a choice rather than the only
// option — see the Cargo.toml note for what 0.5.8 actually buys and costs.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Report {
    /// `"ok"` (valid) or `"problems"` (error/fatal findings remain). The
    /// warning/info/usage-only case is `"ok"` — those never fail a book.
    pub status: String,
    pub summary: Summary,
    pub items: Vec<Item>,
}

/// Small aggregate counts, mirroring the envelope's per-input `summary`
/// exactly — same singular key names, and **every counter present, including
/// zero**.
///
/// **These counts are never filtered**, unlike the CLI's, where `-u` decides
/// what the output contains and the counts describe the output. This binding
/// has no such flag: it is a machine interface and always returns everything,
/// so its counts always describe the whole report.
///
/// That is also why there is **no `suppressed` member** here, and why its
/// absence is the correct statement rather than a gap: under FORMATS §1.4 an
/// absent marker means *no format-level filter was in effect*, which is
/// permanently true of this binding. If a filter is ever added here, the marker
/// comes with it.
//
// The counters became unconditional in 0.14.0, tracking the CLI envelope.
// FORMATS §1.4 does not reach a library — conventions recorded that silence as
// deliberate — so this moved on **our own promise**, not on the rule: the
// sentence above claims this type mirrors the CLI envelope, that envelope
// changed, and a binding that promises to mirror follows what it names. The
// citable form is "a binding that mirrors an envelope follows it", never
// "FORMATS binds library interfaces".
#[derive(Serialize, Tsify)]
pub struct Summary {
    #[serde(rename = "fatal")]
    pub fatals: usize,
    #[serde(rename = "error")]
    pub errors: usize,
    #[serde(rename = "warning")]
    pub warnings: usize,
    #[serde(rename = "info")]
    pub infos: usize,
    #[serde(rename = "usage")]
    pub usages: usize,
}

/// One finding, in the shared item shape (FORMATS.md §1.3).
#[derive(Serialize, Tsify)]
pub struct Item {
    /// Always `"finding"` for a verifier.
    #[serde(rename = "type")]
    pub kind: String,
    /// epubcheck-compatible message ID, e.g. `"RSC-005"`.
    pub code: String,
    /// epubveri's finer semantic sub-code, when the site carries one.
    pub rule: Option<String>,
    /// Lowercase severity: `"fatal" | "error" | "warning" | "info" | "usage"`.
    pub severity: String,
    /// Container-relative path the finding concerns, when known.
    pub location: Option<String>,
    /// Exact source position, when known.
    pub position: Option<Position>,
    /// Human-readable message text (epubveri's own wording).
    pub message: String,
    /// Tool-specific extras — carries the message's interpolation `params`.
    pub data: Option<Data>,
}

/// A 1-indexed line/column position, mirroring [`epubveri::report::Position`].
#[derive(Serialize, Tsify)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// Tool-specific item extras — the same slot the CLI's `--format json` fills,
/// with the same contents.
///
/// Kept in step with [`epubveri::envelope::Data`] deliberately: a browser
/// consumer that has to fall back to the CLI for a field is a consumer this
/// package failed. Through 0.9.x this struct carried `params` alone, so
/// `element_path`, `namespaces`, `advisory_basis` and `violation_kind` were
/// reachable from the command line and not from the web. That was an omission
/// rather than a decision — nothing about the browser makes them harder to
/// produce — and 0.10.0 closes it.
///
/// **Absent means absent**, as in the CLI envelope: every optional field is
/// omitted rather than emitted as `null`, so a consumer tests for presence.
#[derive(Serialize, Tsify)]
pub struct Data {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// XPath-style path to the offending node, resolvable with `namespaces`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_path: Option<String>,
    /// Prefix -> namespace-URI bindings needed to resolve `element_path`.
    ///
    /// **This arrives in JavaScript as a `Map`, not a plain object** — that is
    /// how serde-wasm-bindgen renders a map, and it is the one place this
    /// binding's shape differs from the CLI's JSON, where the same field is an
    /// object. So `data.namespaces.get("opf")`, never
    /// `data.namespaces["opf"]`, which would silently be `undefined`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub namespaces: BTreeMap<String, String>,
    /// `spec-ahead` | `spec-silent` — what an advisory finding is grounded in.
    /// Present only on `ADV-*`/`NEXT-*` findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory_basis: Option<String>,
    /// Which of the six kinds of schema violation this is, when the rule
    /// carries kinds. `None` says the rule has no kinds, never that the kind
    /// could not be determined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_kind: Option<String>,
}

/// Validate raw EPUB bytes and return the typed [`Report`] (an envelope
/// `inputs[i]` object).
///
/// `profile` mirrors the CLI `--profile` flag — pass `"dict"`, `"edupub"`,
/// `"idx"`, `"preview"`, or `undefined`/`null` for default behavior. Unknown
/// names behave like `undefined` (permissive).
///
/// `advisory` mirrors the CLI `--advisory` flag: pass `true` to also emit the
/// opt-in findings epubcheck has no verdict on, in two families, both at
/// `usage` severity — `NEXT-*` (a published specification requires it and
/// epubcheck has not implemented it yet, so it becomes an ordinary error once
/// it does) and `ADV-*` (no specification says anything, but the book is still
/// probably wrong). `undefined`/`false` leaves them off, and with them off the
/// report is byte-identical — so existing two-argument callers are unaffected.
/// Neither family can move `status`.
///
/// `epub_version` mirrors the CLI `-v` flag — pass `"2"`, `"2.0"`, `"3"`,
/// `"3.0"` to validate against that version whatever the book declares, or
/// `undefined`/`null` to judge it as what it declares. On a disagreement
/// PKG-001 reports it and the requested version wins, so forcing a 3.0 book
/// to 2.0 produces a long report. Unrecognized values behave like
/// `undefined`, matching how `profile` treats an unknown name.
///
/// Note: the CLI-only PKG-016 check (the `.epub` file extension should be
/// lowercase) is filename-based and intentionally not reachable here — this
/// entry point only ever sees bytes, never a filename.
#[wasm_bindgen]
pub fn validate(
    bytes: &[u8],
    profile: Option<String>,
    advisory: Option<bool>,
    epub_version: Option<String>,
) -> Report {
    let report = epubveri::validate_bytes_with_options(
        bytes.to_vec(),
        &epubveri::Options {
            profile,
            epub_version,
            advisory: advisory.unwrap_or(false),
        },
    );
    Report {
        status: if report.is_valid() { "ok" } else { "problems" }.to_string(),
        summary: Summary {
            fatals: report.fatals(),
            errors: report.errors(),
            warnings: report.warnings(),
            infos: report.infos(),
            usages: report.usages(),
        },
        items: report
            .messages
            .iter()
            .map(|m| Item {
                kind: "finding".to_string(),
                code: m.id.to_string(),
                rule: m.rule.map(str::to_string),
                severity: m.severity.as_str().to_string(),
                location: m.location.clone(),
                position: m.position.map(|p| Position {
                    line: p.line,
                    column: p.column,
                }),
                message: m.text.clone(),
                data: {
                    let basis = epubveri::ids::advisory_basis(m.id);
                    (!m.params.is_empty()
                        || m.element_path.is_some()
                        || basis.is_some()
                        || m.violation_kind.is_some())
                    .then(|| Data {
                        params: m.params.clone(),
                        element_path: m.element_path.as_ref().map(|p| p.path.clone()),
                        namespaces: m
                            .element_path
                            .as_ref()
                            .map(|p| p.namespaces.clone())
                            .unwrap_or_default(),
                        advisory_basis: basis.map(|b| b.as_str().to_string()),
                        violation_kind: m.violation_kind.map(|k| k.as_str().to_string()),
                    })
                },
            })
            .collect(),
    }
}

/// The validator version — [`epubveri::VERSION`], the one string the CLI's
/// `-V` and the demo footer also print, with git build metadata
/// (`+<short-hash>[.dirty]`) when built from a checkout.
#[wasm_bindgen]
pub fn version() -> String {
    epubveri::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal EPUB 2 built to exercise every `data` field at once:
    ///
    /// - an EPUB 3 attribute on the `<spine>`, so the grammar rejects it —
    ///   `params`, `element_path`, `namespaces`, `violation_kind`;
    /// - a container entry the manifest does not list — a `usage` finding,
    ///   which the CLI's human report hides and this binding must not;
    /// - a stylesheet declaring a property CSS does not define — an `ADV-*`
    ///   finding, the only way to reach `advisory_basis`.
    fn book() -> Vec<u8> {
        use std::io::Write;
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title>T</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="s.css" media-type="text/css"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx" page-progression-direction="ltr"><itemref idref="c1"/></spine>
</package>"#;
        const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>T</text></docTitle>
  <navMap><navPoint id="n1" playOrder="1"><navLabel><text>T</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
        const CH1: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
            <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title>\
            <link rel=\"stylesheet\" type=\"text/css\" href=\"s.css\"/></head>\
            <body><p>x</p></body></html>";
        // `wobble` is not a CSS property, which is ADV-001.
        const CSS: &str = "p { wobble: 3px; }\n";

        let mut buf = Vec::new();
        {
            let mut z = ZipWriter::new(std::io::Cursor::new(&mut buf));
            z.start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            let o = SimpleFileOptions::default();
            for (name, body) in [
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/toc.ncx", NCX),
                ("OEBPS/ch1.xhtml", CH1),
                ("OEBPS/s.css", CSS),
                // Undeclared on purpose: one usage-severity OPF-003.
                ("OEBPS/stray.txt", "x"),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf
    }

    /// **The browser sees everything the CLI's `--format json` does.**
    ///
    /// Through 0.9.x this binding's `Data` carried `params` alone, so
    /// `element_path`, `namespaces`, `advisory_basis` and `violation_kind` were
    /// reachable from the command line and not from the web — an omission
    /// nothing reported, because the two shapes were written separately and
    /// only one of them was ever compared against the envelope.
    ///
    /// So this asserts the fields are *populated*, not merely present in the
    /// type: a `Data` that compiles and forwards `None` forever would satisfy
    /// a weaker test and would be the same bug.
    #[test]
    fn a_findings_data_carries_everything_the_cli_envelope_carries() {
        let report = validate(&book(), None, None, None);
        let hit = report
            .items
            .iter()
            .find(|i| i.code == "RSC-005")
            .expect("an EPUB 2 spine may not carry page-progression-direction");
        let data = hit.data.as_ref().expect("the finding must carry data");

        assert_eq!(data.params, vec!["page-progression-direction".to_string()]);
        assert_eq!(
            data.element_path.as_deref(),
            Some("/opf:package[1]/opf:spine[1]/@page-progression-direction")
        );
        assert_eq!(
            data.namespaces.get("opf").map(String::as_str),
            Some("http://www.idpf.org/2007/opf"),
            "the path is unresolvable without its bindings"
        );
        assert_eq!(
            data.violation_kind.as_deref(),
            Some("attribute_not_allowed")
        );
    }

    /// `advisory_basis` is the one `data` field that needs the flag on, so it
    /// gets its own case rather than riding on the finding above.
    #[test]
    fn an_advisory_finding_carries_its_basis() {
        let report = validate(&book(), None, Some(true), None);
        let advisory = report
            .items
            .iter()
            .filter_map(|i| i.data.as_ref().and_then(|d| d.advisory_basis.as_deref()))
            .next();
        assert!(
            matches!(advisory, Some("spec-ahead") | Some("spec-silent")),
            "with --advisory on, an ADV-*/NEXT-* finding must name its basis; got {advisory:?}"
        );
    }

    /// A tripwire, the same one `main.rs` carries for `--help` and for the
    /// same reason: this crate's `README.md` **enumerates** the advisory
    /// families in prose, it is what npm publishes as the package page, and
    /// nothing links that prose to `ids.rs`.
    ///
    /// It had already gone stale once — `ADV-010` shipped while the README
    /// still listed four `ADV-*` checks — and nothing could have caught it,
    /// because a stale README breaks no check, changes no verdict and no
    /// instrument reads it. The CLI's copy of this problem has had a guard
    /// since 0.9.25; this one had none.
    ///
    /// When it fails, an advisory check was added or removed. Describe it in
    /// the README's "Advisory checks" section, then update the count.
    #[test]
    fn a_new_advisory_check_must_be_described_in_this_crates_readme() {
        let declared: Vec<&str> = include_str!("../../src/ids.rs")
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                t.strip_prefix("pub const ADV_")
                    .or_else(|| t.strip_prefix("pub const NEXT_"))
            })
            .filter_map(|l| l.split('"').nth(1))
            .collect();
        assert_eq!(
            declared.len(),
            10,
            "the advisory families changed ({declared:?}) — describe the new \
             check in README.md's \"Advisory checks\" section, then update \
             this count"
        );
        // The count means nothing if the section it guards has gone.
        let readme = include_str!("../README.md");
        let section = readme
            .split("### Advisory checks")
            .nth(1)
            .expect("README documents the advisory checks");
        assert!(
            section.contains("`NEXT-*`") && section.contains("`ADV-*`") && section.len() > 200,
            "the advisory section should still describe both families"
        );
        // ...and the signature block has to keep every parameter the binding
        // takes. `epub_version` was absent from it for four releases.
        assert!(
            readme.contains("epubVersion?: string | null"),
            "README's validate() signature must list every parameter"
        );
    }

    /// Nothing is filtered here, whatever the CLI's human report does with
    /// usage findings — this is a machine interface.
    #[test]
    fn usage_findings_are_never_hidden_from_the_binding() {
        let report = validate(&book(), None, None, None);
        assert!(
            report.items.iter().any(|i| i.severity == "usage"),
            "usage findings must reach a browser consumer"
        );
    }
}
