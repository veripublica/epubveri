//! WebAssembly bindings for [`epubveri`] — validate an EPUB entirely in the
//! browser (or any JS runtime), with no JVM, no server round-trip, no C deps.
//!
//! This crate is a thin boundary: it hands raw `.epub` bytes to the core
//! [`epubveri::validate_bytes_with_profile`] and maps its [`epubveri::report::Report`]
//! into serde+tsify structs so JS/TS callers get a plain object with a real
//! generated `.d.ts`.
//!
//! ```js
//! import init, { validate } from "epubveri-wasm";
//! await init();
//! const report = validate(new Uint8Array(epubArrayBuffer), undefined);
//! // report.valid, report.errors, report.messages[i].id, ...
//! ```

use serde::Serialize;
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

/// A 1-indexed line/column position, mirroring [`epubveri::report::Position`].
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// One diagnostic, mirroring [`epubveri::report::Message`] with the message ID
/// and severity flattened to strings for the JS boundary.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Message {
    /// epubcheck-compatible message ID, e.g. `"RSC-005"`.
    pub id: String,
    /// `"ERROR"`, `"WARNING"`, or `"INFO"`.
    pub severity: String,
    /// Human-readable message text (epubveri's own wording).
    pub text: String,
    /// Optional location hint (path / element), when the check provides one.
    pub location: Option<String>,
    /// Optional exact source position, when the check provides one.
    pub position: Option<Position>,
}

/// The full validation result for one EPUB.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Report {
    /// `true` when there are zero `ERROR`-severity messages (warnings are allowed).
    pub valid: bool,
    /// Count of `ERROR`-severity messages.
    pub errors: usize,
    /// Count of `WARNING`-severity messages.
    pub warnings: usize,
    /// Every diagnostic, in the order the validator produced them.
    pub messages: Vec<Message>,
}

/// Validate raw EPUB bytes and return a typed [`Report`].
///
/// `profile` mirrors the CLI `--profile` flag — pass `"dict"`, `"edupub"`,
/// `"idx"`, `"preview"`, or `undefined`/`null` for default behavior. Unknown
/// names behave like `undefined` (permissive).
///
/// Note: the CLI-only PKG-016 check (the `.epub` file extension should be
/// lowercase) is filename-based and intentionally not reachable here — this
/// entry point only ever sees bytes, never a filename.
#[wasm_bindgen]
pub fn validate(bytes: &[u8], profile: Option<String>) -> Report {
    let report = epubveri::validate_bytes_with_profile(bytes.to_vec(), profile.as_deref());
    Report {
        valid: report.is_valid(),
        errors: report.errors(),
        warnings: report.warnings(),
        messages: report
            .messages
            .iter()
            .map(|m| Message {
                id: m.id.to_string(),
                severity: m.severity.to_string(),
                text: m.text.clone(),
                location: m.location.clone(),
                position: m.position.map(|p| Position {
                    line: p.line,
                    column: p.column,
                }),
            })
            .collect(),
    }
}

/// The `epubveri-wasm` crate version (matches this crate's `Cargo.toml`).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
