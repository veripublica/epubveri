//! The `--format json` machine envelope — the veripublica shared output format
//! ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md),
//! convention v0.4). One JSON object per run: a top-level verdict plus one
//! `Input` object per `-i`, each carrying its own findings.
//!
//! These are the family's *shared* shape, so the same types back the wasm
//! binding's return value (an [`Input`] is self-contained on purpose): CLI, CI
//! and the browser demo then read one shape with one parser.

use serde::Serialize;

use crate::report::{Message, Report};

/// The convention's stability key, emitted verbatim (FORMATS.md §1.1): compare
/// with string equality, there is nothing finer to parse.
const CONVENTION: &str = "0.4";

/// The whole run: exactly one of these is printed to stdout in `json` mode.
#[derive(Serialize)]
pub struct Envelope {
    pub tool: &'static str,
    pub tool_version: &'static str,
    pub convention: &'static str,
    /// Mirror of the exit code, aggregated over the inputs: `ok` → 0,
    /// `problems` → 1, `error` → 2.
    pub status: &'static str,
    /// One [`Input`] per `-i`, in command-line order — an array even for one.
    pub inputs: Vec<Input>,
}

impl Envelope {
    /// Wrap the per-input reports, deriving the aggregate `status` from them
    /// (the same precedence as the exit code: any unprocessable input →
    /// `error`; else any input with findings → `problems`; else `ok`).
    pub fn new(inputs: Vec<Input>) -> Self {
        let status = if inputs.iter().any(|i| i.status == "error") {
            "error"
        } else if inputs.iter().any(|i| i.status == "problems") {
            "problems"
        } else {
            "ok"
        };
        Envelope {
            tool: "epubveri",
            tool_version: env!("CARGO_PKG_VERSION"),
            convention: CONVENTION,
            status,
            inputs,
        }
    }
}

/// One input's outcome — self-contained, so a wasm binding can return it as-is.
#[derive(Serialize)]
pub struct Input {
    pub path: String,
    /// `ok` (valid), `problems` (findings remain), or `error` (no verdict was
    /// possible — see `error`).
    pub status: &'static str,
    /// Present only with `status: "error"`: why this input could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    pub items: Vec<Item>,
}

impl Input {
    /// An input that produced a verdict: `ok`/`problems` by the error-and-above
    /// threshold, with its findings.
    pub fn from_report(path: String, report: &Report) -> Self {
        Input {
            path,
            status: if report.is_valid() { "ok" } else { "problems" },
            error: None,
            summary: Some(Summary::of(report)),
            items: report.messages.iter().map(Item::of).collect(),
        }
    }

    /// An input that could not be read at all: `error`, no verdict.
    pub fn from_error(path: String, error: String) -> Self {
        Input {
            path,
            status: "error",
            error: Some(error),
            summary: None,
            items: Vec::new(),
        }
    }
}

/// Small aggregate counts for one input (tool-owned; a consumer MUST NOT
/// require it — it is derivable from `items`).
#[derive(Serialize)]
pub struct Summary {
    #[serde(skip_serializing_if = "is_zero")]
    pub fatals: usize,
    pub errors: usize,
    pub warnings: usize,
}

impl Summary {
    fn of(report: &Report) -> Self {
        Summary {
            fatals: report.fatals(),
            errors: report.errors(),
            warnings: report.warnings(),
        }
    }
}

/// One finding, in the shared item shape (FORMATS.md §1.3).
#[derive(Serialize)]
pub struct Item {
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// The epubcheck-compatible message ID (e.g. `"RSC-005"`).
    pub code: &'static str,
    /// epubveri's finer semantic sub-code, when the site carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<&'static str>,
    /// Lowercase severity: `fatal|error|warning|info|usage`.
    pub severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    pub message: String,
    /// Tool-specific extras. Carries the message's interpolation `params` (the
    /// values behind `rule`'s template) until a second tool's json needs them
    /// promoted to a shared field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Data>,
}

impl Item {
    fn of(m: &Message) -> Self {
        Item {
            kind: "finding",
            code: m.id,
            rule: m.rule,
            severity: m.severity.as_str(),
            location: m.location.clone(),
            position: m.position.map(|p| Position {
                line: p.line,
                column: p.column,
            }),
            message: m.text.clone(),
            data: (!m.params.is_empty()).then(|| Data {
                params: m.params.clone(),
            }),
        }
    }
}

#[derive(Serialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize)]
pub struct Data {
    pub params: Vec<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}
