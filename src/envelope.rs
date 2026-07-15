//! The `--format json` machine envelope — the veripublica shared output format
//! ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md),
//! convention v0.4). One JSON object per run: a top-level verdict plus one
//! `Input` object per `-i`, each carrying its own findings.
//!
//! **This module is the veripublica family's reference implementation of the
//! envelope** — FORMATS.md §2 links here (a non-normative pointer; the JSON is
//! the contract, these types are a convenience). The skeleton is generic over
//! the two *tool-owned* slots FORMATS.md §2 defines — the per-input/-run
//! `summary` aggregate (`S`) and the per-item `data` extras (`D`) — so a
//! verifier (epubveri), a repairer (epubsana) and a transformer (epublift) all
//! build one shape. epubveri's own [`Summary`]/[`Data`] are the defaults, so
//! epubveri code names the types unparameterized (`Envelope`, `Input`, `Item`).
//!
//! Transformer-only envelope fields from the spec are present but skipped when
//! absent, so a verifier never emits them: [`Envelope::dry_run`] (§1.1),
//! [`Input::output`] (§1.2), [`Item::outcome`] (§1.3).
//!
//! **Absent means absent** (FORMATS.md §1.2): every optional field is
//! `skip_serializing_if`, so "no value" serializes as a missing key, never
//! `null`. A clean verifier input is exactly `{path, status, summary, items}`.
//!
//! Promotion trigger (recorded on conventions#27): if a veripublica tool that
//! does **not** already depend on epubveri ever needs the envelope, lift this
//! skeleton into a standalone `veripublica-envelope` crate. Until then every
//! family tool already depends on epubveri, so the reference types live here.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::report::{Message, Report};

/// The convention's stability key, emitted verbatim (FORMATS.md §1.1): compare
/// with string equality, there is nothing finer to parse.
const CONVENTION: &str = "0.4";

/// The whole run: exactly one of these is printed to stdout in `json` mode.
/// Generic over the tool-owned `summary` (`S`) and item `data` (`D`) slots
/// (FORMATS.md §2); epubveri instantiates `Envelope<Summary, Data>` via the
/// defaults.
#[derive(Serialize)]
pub struct Envelope<S = Summary, D = Data> {
    pub tool: &'static str,
    pub tool_version: &'static str,
    pub convention: &'static str,
    /// Mirror of the exit code, aggregated over the inputs: `ok` → 0,
    /// `problems` → 1, `error` → 2.
    pub status: &'static str,
    /// Envelope-level aggregate (FORMATS.md §1.1): tool-owned, optional — a
    /// consumer MUST NOT require it. Absent (no key) when `None`; epubveri does
    /// not emit one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<S>,
    /// Transformer-only (§1.1): whether this run only previewed changes. Absent
    /// (no key) when `false`.
    #[serde(skip_serializing_if = "is_false")]
    pub dry_run: bool,
    /// One [`Input`] per `-i`, in command-line order — an array even for one.
    pub inputs: Vec<Input<S, D>>,
}

impl<S, D> Envelope<S, D> {
    /// Wrap the per-input outcomes for an arbitrary tool, deriving the
    /// aggregate `status` from them (the same precedence as the exit code: any
    /// unprocessable input → `error`; else any input with findings →
    /// `problems`; else `ok`).
    ///
    /// `tool`/`tool_version` are passed in — the reference types belong to no
    /// single tool. `dry_run` defaults to `false` and `summary` is the caller's;
    /// set the public fields directly for a transformer that needs them.
    /// epubveri itself uses the [`Envelope::new`] shorthand.
    pub fn for_tool(
        tool: &'static str,
        tool_version: &'static str,
        summary: Option<S>,
        inputs: Vec<Input<S, D>>,
    ) -> Self {
        let status = if inputs.iter().any(|i| i.status == "error") {
            "error"
        } else if inputs.iter().any(|i| i.status == "problems") {
            "problems"
        } else {
            "ok"
        };
        Envelope {
            tool,
            tool_version,
            convention: CONVENTION,
            status,
            summary,
            dry_run: false,
            inputs,
        }
    }
}

impl Envelope<Summary, Data> {
    /// epubveri's own envelope: identifies as `epubveri` at its build version,
    /// with no envelope-level summary. A thin shorthand over [`for_tool`] whose
    /// signature is kept stable, so existing callers don't break.
    ///
    /// [`for_tool`]: Envelope::for_tool
    pub fn new(inputs: Vec<Input>) -> Self {
        Self::for_tool("epubveri", crate::VERSION, None, inputs)
    }
}

/// One input's outcome — self-contained, so a wasm binding can return it as-is.
/// Generic over the tool-owned `summary`/`data` slots (see [`Envelope`]).
#[derive(Serialize)]
pub struct Input<S = Summary, D = Data> {
    pub path: String,
    /// `ok` (valid), `problems` (findings remain), or `error` (no verdict was
    /// possible — see `error`).
    pub status: &'static str,
    /// Present only with `status: "error"` (§1.2): absent (no key) otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Transformer-only (§1.2): the path this input's transformed output was
    /// written to. Absent (no key) when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<S>,
    pub items: Vec<Item<D>>,
}

impl Input<Summary, Data> {
    /// An input that produced a verdict: `ok`/`problems` by the error-and-above
    /// threshold, with its findings.
    pub fn from_report(path: String, report: &Report) -> Self {
        Input {
            path,
            status: if report.is_valid() { "ok" } else { "problems" },
            error: None,
            output: None,
            summary: Some(Summary::of(report)),
            items: report.messages.iter().map(Item::finding_of).collect(),
        }
    }

    /// An input that could not be read at all: `error`, no verdict.
    pub fn from_error(path: String, error: String) -> Self {
        Input {
            path,
            status: "error",
            error: Some(error),
            output: None,
            summary: None,
            items: Vec::new(),
        }
    }
}

/// Small aggregate counts for one input (tool-owned; a consumer MUST NOT
/// require it — it is derivable from `items`). epubveri's own `summary`
/// vocabulary; a transformer supplies its own `S`.
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

/// One item in an input's `items` array (FORMATS.md §1.3). Generic over the
/// tool-owned `data` (`D`) slot. Build via [`Item::finding`] (verifier),
/// [`Item::fix`] or [`Item::operation`] (transformer) so the `outcome`
/// invariant — required on `fix`/`operation`, never on `finding` — holds by
/// construction.
#[derive(Serialize)]
pub struct Item<D = Data> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// Transformer-only (§1.3): `applied | skipped | proposed` — required on a
    /// `fix`/`operation` item, never present on a `finding`. Absent (no key)
    /// when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<&'static str>,
    /// The finding/target code (e.g. epubveri's epubcheck-compatible `RSC-005`,
    /// or a repairer's `addresses_id`). A `String`: a verifier's are compile-
    /// time constants, but a transformer's may be built at runtime.
    pub code: String,
    /// A tool's finer semantic sub-code, when the site carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<&'static str>,
    /// Lowercase severity: `fatal|error|warning|info|usage`.
    pub severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    pub message: String,
    /// Tool-specific extras (FORMATS.md §2, tool-owned). Absent (no key) when
    /// `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<D>,
}

impl<D> Item<D> {
    /// A `finding` item (a verifier reporting a defect): never carries an
    /// `outcome`.
    pub fn finding(
        code: String,
        rule: Option<&'static str>,
        severity: &'static str,
        location: Option<String>,
        position: Option<Position>,
        message: String,
        data: Option<D>,
    ) -> Self {
        Item {
            kind: "finding",
            outcome: None,
            code,
            rule,
            severity,
            location,
            position,
            message,
            data,
        }
    }

    /// A `fix` item (a repairer applied/declined/proposed a fix to a finding):
    /// `outcome` (`applied|skipped|proposed`) is required.
    // A shared item genuinely has this many fields (FORMATS.md §1.3); grouping
    // them would be an artificial abstraction for a reference type.
    #[allow(clippy::too_many_arguments)]
    pub fn fix(
        outcome: &'static str,
        code: String,
        rule: Option<&'static str>,
        severity: &'static str,
        location: Option<String>,
        position: Option<Position>,
        message: String,
        data: Option<D>,
    ) -> Self {
        Item {
            kind: "fix",
            outcome: Some(outcome),
            code,
            rule,
            severity,
            location,
            position,
            message,
            data,
        }
    }

    /// An `operation` item (a transformer performed a change not tied to a
    /// finding): `outcome` (`applied|skipped|proposed`) is required.
    #[allow(clippy::too_many_arguments)]
    pub fn operation(
        outcome: &'static str,
        code: String,
        rule: Option<&'static str>,
        severity: &'static str,
        location: Option<String>,
        position: Option<Position>,
        message: String,
        data: Option<D>,
    ) -> Self {
        Item {
            kind: "operation",
            outcome: Some(outcome),
            code,
            rule,
            severity,
            location,
            position,
            message,
            data,
        }
    }
}

impl Item<Data> {
    /// epubveri's `finding` builder from one of its [`Message`]s.
    fn finding_of(m: &Message) -> Self {
        Item::finding(
            m.id.to_string(),
            m.rule,
            m.severity.as_str(),
            m.location.clone(),
            m.position.map(|p| Position {
                line: p.line,
                column: p.column,
            }),
            m.text.clone(),
            (!m.params.is_empty() || m.element_path.is_some()).then(|| Data {
                params: m.params.clone(),
                element_path: m.element_path.as_ref().map(|p| p.path.clone()),
                namespaces: m
                    .element_path
                    .as_ref()
                    .map(|p| p.namespaces.clone())
                    .unwrap_or_default(),
            }),
        )
    }
}

#[derive(Serialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// epubveri's `data` vocabulary; a transformer supplies its own `D`.
#[derive(Serialize)]
pub struct Data {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// A machine-resolvable, XPath-style path to the offending node (issue #18),
    /// e.g. `/opf:package[1]/opf:metadata[1]/dc:contributor[1]/@opf:role`.
    /// Present only on node-anchored findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_path: Option<String>,
    /// Prefix -> namespace-URI bindings needed to resolve `element_path`. Every
    /// namespaced name in the path carries a bound, non-empty prefix (there is
    /// no default-namespace / empty-string key), because XPath 1.0 — the engine
    /// behind libxml2/lxml — cannot bind a default namespace. Empty when there's
    /// no `element_path`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub namespaces: BTreeMap<String, String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_set(v: &serde_json::Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    #[test]
    fn clean_verifier_input_is_exactly_path_status_summary_items() {
        // conventions#27: a clean input serializes to precisely these four
        // keys — `error`/`output` are absent (no key), never `null`.
        let input = Input::<Summary, Data> {
            path: "book.epub".into(),
            status: "ok",
            error: None,
            output: None,
            summary: Some(Summary {
                fatals: 0,
                errors: 0,
                warnings: 0,
            }),
            items: vec![],
        };
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(key_set(&v), ["items", "path", "status", "summary"]);
    }

    #[test]
    fn clean_envelope_omits_top_level_summary_and_dry_run() {
        // epubveri passes `summary: None` and never a dry run, so neither key
        // appears (skip-if-none / skip-if-false).
        let env: Envelope = Envelope::for_tool("epubveri", "1.2.3", None, vec![]);
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(
            key_set(&v),
            ["convention", "inputs", "status", "tool", "tool_version"]
        );
    }

    #[test]
    fn a_finding_has_type_finding_and_no_outcome_key() {
        let item: Item = Item::finding(
            "RSC-005".into(),
            None,
            "error",
            None,
            None,
            "bad".into(),
            None,
        );
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["type"], "finding");
        assert!(!v.as_object().unwrap().contains_key("outcome"));
    }

    #[test]
    fn a_fix_item_carries_its_required_outcome() {
        // The transformer shape a repairer (epubsana) builds: `type: "fix"`
        // with a required `outcome`, unconstructible without one.
        let item: Item = Item::fix(
            "applied",
            "OPF-002".into(),
            None,
            "error",
            None,
            None,
            "repaired".into(),
            None,
        );
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["type"], "fix");
        assert_eq!(v["outcome"], "applied");
    }

    fn msg_with(element_path: Option<crate::xmlext::NodePath>, params: Vec<String>) -> Message {
        Message {
            id: "RSC-005",
            severity: crate::report::Severity::Error,
            text: "x".into(),
            location: Some("EPUB/package.opf".into()),
            position: None,
            rule: Some("opf.spine.duplicate_itemref"),
            params,
            element_path,
        }
    }

    #[test]
    fn element_path_and_namespaces_reach_the_finding_data() {
        // A node-anchored finding (issue #18) carries a resolvable path plus the
        // prefix->URI bindings needed to resolve it (every prefix non-empty).
        let np = crate::xmlext::NodePath {
            path: "/opf:package[1]/opf:spine[1]/opf:itemref[2]".into(),
            namespaces: BTreeMap::from([(
                "opf".to_string(),
                "http://www.idpf.org/2007/opf".to_string(),
            )]),
        };
        let item = Item::finding_of(&msg_with(Some(np), vec!["content_001".into()]));
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(
            v["data"]["element_path"],
            "/opf:package[1]/opf:spine[1]/opf:itemref[2]"
        );
        assert_eq!(
            v["data"]["namespaces"]["opf"],
            "http://www.idpf.org/2007/opf"
        );
        assert_eq!(v["data"]["params"][0], "content_001");
    }

    #[test]
    fn element_path_only_finding_omits_the_empty_params_key() {
        // params is skipped when empty, so an element_path-only finding's data is
        // exactly {element_path, namespaces} — never a stray "params": [].
        let np = crate::xmlext::NodePath {
            path: "/html[1]/body[1]".into(),
            namespaces: BTreeMap::new(),
        };
        let item = Item::finding_of(&msg_with(Some(np), Vec::new()));
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(key_set(&v["data"]), ["element_path"]);
    }

    #[test]
    fn a_finding_without_path_or_params_has_no_data_key() {
        let item = Item::finding_of(&msg_with(None, Vec::new()));
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("data").is_none());
    }
}
