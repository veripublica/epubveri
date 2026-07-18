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

use serde::Serialize;
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

/// One EPUB's validation result — the envelope's `inputs[i]` object without
/// `path`/`error` (a wasm caller has no path, and in-memory bytes are always
/// readable, so there is no unprocessable/`"error"` case here).
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
/// (`fatals` omitted when zero, exactly as the CLI envelope emits it).
#[derive(Serialize, Tsify)]
pub struct Summary {
    #[serde(skip_serializing_if = "is_zero")]
    pub fatals: usize,
    pub errors: usize,
    pub warnings: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
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

/// Tool-specific item extras.
#[derive(Serialize, Tsify)]
pub struct Data {
    pub params: Vec<String>,
}

/// Validate raw EPUB bytes and return the typed [`Report`] (an envelope
/// `inputs[i]` object).
///
/// `profile` mirrors the CLI `--profile` flag — pass `"dict"`, `"edupub"`,
/// `"idx"`, `"preview"`, or `undefined`/`null` for default behavior. Unknown
/// names behave like `undefined` (permissive).
///
/// `advisory` mirrors the CLI `--advisory` flag: pass `true` to also emit the
/// opt-in advisory findings epubcheck has no verdict on (unknown CSS
/// property/descriptor names, `ADV-*`, at usage severity). `undefined`/`false`
/// leaves them off, and with them off the report is byte-identical — so
/// existing two-argument callers are unaffected.
///
/// Note: the CLI-only PKG-016 check (the `.epub` file extension should be
/// lowercase) is filename-based and intentionally not reachable here — this
/// entry point only ever sees bytes, never a filename.
#[wasm_bindgen]
pub fn validate(bytes: &[u8], profile: Option<String>, advisory: Option<bool>) -> Report {
    let report = epubveri::validate_bytes_with_options(
        bytes.to_vec(),
        &epubveri::Options {
            profile,
            advisory: advisory.unwrap_or(false),
        },
    );
    Report {
        status: if report.is_valid() { "ok" } else { "problems" }.to_string(),
        summary: Summary {
            fatals: report.fatals(),
            errors: report.errors(),
            warnings: report.warnings(),
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
                data: (!m.params.is_empty()).then(|| Data {
                    params: m.params.clone(),
                }),
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
