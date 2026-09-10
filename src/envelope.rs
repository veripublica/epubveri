//! The `--format json` machine envelope — the veripublica shared output format
//! ([FORMATS.md](https://github.com/veripublica/conventions/blob/main/FORMATS.md),
//! convention v0.5). One JSON object per run: a top-level verdict plus one
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

/// **epubveri's own** stability key, emitted verbatim (FORMATS.md §1.1):
/// compare with string equality, there is nothing finer to parse.
///
/// It is deliberately not visible to [`Envelope::for_tool`], which takes the
/// key as a required parameter instead. FORMATS §1.1 (conventions 0.5.0): *"the
/// key is asserted by the emitting tool about itself: a shared implementation
/// takes it from the tool rather than stamping its own"*. Until 0.14.0 this
/// constant reached epubsana's envelopes through the shared skeleton, so a
/// routine `epubveri` bump would have made their output claim a convention
/// version they had not implemented. A defaulted parameter would have preserved
/// exactly that; the compile error is the notification.
const CONVENTION: &str = "0.5";

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
    /// `tool`/`tool_version`/`convention` are passed in — the reference types
    /// belong to no single tool, and **the stability key is the caller's
    /// assertion about itself** (FORMATS §1.1), never this crate's. `dry_run`
    /// defaults to `false` and `summary` is the caller's; set the public fields
    /// directly for a transformer that needs them. epubveri itself uses the
    /// [`Envelope::new`] shorthand.
    ///
    /// Pass the key your tool has *implemented*, which is not always the newest
    /// one published: raising an `epubveri` dependency is not the same event as
    /// adopting a convention release, and conflating the two is the defect this
    /// parameter exists to prevent.
    pub fn for_tool(
        tool: &'static str,
        tool_version: &'static str,
        convention: &'static str,
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
            convention,
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
        Self::for_tool("epubveri", crate::VERSION, CONVENTION, None, inputs)
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
    /// `suppressed` names the severities a format-level filter was in effect
    /// for — see [`Summary::suppressed`] for why it is an argument rather than
    /// something set afterwards. Pass `&[]` for an unfiltered run.
    pub fn from_report(path: String, report: &Report, suppressed: &[&'static str]) -> Self {
        Input {
            path,
            status: if report.is_valid() { "ok" } else { "problems" },
            error: None,
            output: None,
            summary: Some(Summary::of(report, suppressed)),
            items: report.messages.iter().map(Item::finding_of).collect(),
        }
    }

    /// An input that could not be read at all: `error`, no verdict.
    ///
    /// **No summary, and therefore no `suppressed` marker — this is conformant,
    /// not a gap** (FORMATS §1.4): an input that produced no report carries
    /// neither, because its `status` already says the counters do not exist. An
    /// earlier draft of that rule would have obliged five zero counters here,
    /// making an input that was never opened byte-identical in the counter block
    /// to a clean book.
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
///
/// **The keys are the severity vocabulary itself** — `fatal`, `error`,
/// `warning`, `info`, `usage` — and that is the reason they are singular rather
/// than a grammatical preference: an item's `severity` holds exactly these
/// words, so `summary[item.severity]` is a direct lookup. Plural keys would
/// force every consumer to carry an `error` → `errors` mapping table.
///
/// (The prompt was Doitsu's, on MobileRead #231 — "information has no plural
/// and usages doesn't make sense", which is true of `infos`/`usages` and is a
/// narrower point than the one above. Recorded so the weaker reason is not
/// mistaken for the whole of it.)
///
/// Counts describe **what the output contains**, not what the validator found:
/// without `-u` the usage count is 0, as epubcheck's `nUsage` is. The library
/// is where a complete count lives — and [`suppressed`] is what tells a consumer
/// which of these two it is holding.
///
/// **Every counter is emitted, including zero** (FORMATS §1.4, conventions
/// 0.5.0): a counter over a closed set the specification declares reports every
/// member the tool has a concept of, and `report::Severity` has exactly these
/// five. Until 0.14.0 `fatal`, `info` and `usage` were omitted at zero, which
/// turned *"the key is absent"* into *"this book has no usage findings"* — a
/// false statement rather than an ambiguous one.
///
/// [`suppressed`]: Summary::suppressed
#[derive(Serialize)]
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
    /// The severities a format-level filter was in effect for (FORMATS §1.4) —
    /// **the gate, not the outcome**: it is set whether or not the filter
    /// removed anything on this run. Absent (no key) when empty, which means no
    /// filter was in effect and every item the run produced is present.
    ///
    /// It is a **completeness** marker rather than a "something is hidden" flag:
    /// it answers *can I trust these counters?* A severity named here may be
    /// **incompletely** represented — with `--advisory` and no `-u`, epubveri
    /// emits its `ADV-*`/`NEXT-*` findings (which are usage-severity and exempt
    /// by ID) while withholding every other usage finding, so a non-zero `usage`
    /// count sits beside `suppressed: ["usage"]` and both are true.
    ///
    /// A **reserved non-counter member** of this object: summing a summary's
    /// values was never safe, and this makes it plainly unsafe.
    #[serde(skip_serializing_if = "<[&str]>::is_empty")]
    pub suppressed: Vec<&'static str>,
}

impl Summary {
    /// The counts for one report, qualified by the severities a format-level
    /// filter was in effect for.
    ///
    /// `suppressed` is a **required** argument rather than a defaulted field on
    /// purpose. FORMATS §1.4 forbids dodging the marker, and a caller that has
    /// filtered is exactly the caller who will not remember to set it
    /// afterwards; making it impossible to build the object without answering
    /// the question is the same instrument as `for_tool`'s convention key.
    fn of(report: &Report, suppressed: &[&'static str]) -> Self {
        Summary {
            fatals: report.fatals(),
            errors: report.errors(),
            warnings: report.warnings(),
            infos: report.infos(),
            usages: report.usages(),
            suppressed: suppressed.to_vec(),
        }
    }
}

/// A transformer item's `outcome` — the closed set FORMATS §1.3 declares
/// (`applied | skipped | proposed`), as a type rather than as a doc comment.
///
/// **Why this is an enum.** The set was previously prose in three comments here
/// and a `&'static str` on the wire, so `"revert"`, `"Applied"` or `"propsed"`
/// all passed. FORMATS §1.4 keys a counter rule on this set, and a set enforced
/// by nothing is an honour system. epubveri emits only `finding` items and so
/// never constructs one; the value is that *our* implementation can no longer
/// violate the set by accident, which is a narrower claim than the rule itself
/// and the only one the type earns.
///
/// **A tool keeps its own vocabulary.** [`Item::fix`] and [`Item::operation`]
/// take `impl Into<Outcome>`, so a repairer with its own `Outcome` writes one
/// `From` and passes its own value (epubsana's request, 2026-09-10). Their
/// reason is the better one: a shared type makes two vocabularies identical by
/// fiat, while a conversion makes the *place they meet* a thing the compiler
/// forces you to update. That guarantee holds only while such a `From` is a
/// wildcard-free `match` — a `_ =>` arm turns the build error back into
/// silence, the same trap `violation_kind` carries.
///
/// `reverted` is **not** here: conventions accepted it (#31) and deliberately
/// did not ship the text, because its emitter (epubsana#7) is unstarted. It
/// arrives with the batch that carries the mechanism.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// The change was made.
    Applied,
    /// Presented and not done.
    Skipped,
    /// No decision exists yet — a dry run.
    Proposed,
}

impl Outcome {
    /// The lowercase spelling the envelope uses — the same string `Serialize`
    /// emits, exposed for a human report or a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Applied => "applied",
            Outcome::Skipped => "skipped",
            Outcome::Proposed => "proposed",
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
    /// Transformer-only (§1.3): required on a `fix`/`operation` item, never
    /// present on a `finding`. Absent (no key) when `None`.
    ///
    /// Typed rather than a string **because this field is `pub`** — typing only
    /// the constructors would leave a struct literal free to put any spelling
    /// here, which is the hole [`Outcome`] exists to close.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
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
    /// [`Outcome`] is required. Takes `impl Into<Outcome>`, so a tool with its
    /// own outcome type passes its own value through one `From`.
    // A shared item genuinely has this many fields (FORMATS.md §1.3); grouping
    // them would be an artificial abstraction for a reference type.
    #[allow(clippy::too_many_arguments)]
    pub fn fix(
        outcome: impl Into<Outcome>,
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
            outcome: Some(outcome.into()),
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
    /// finding): [`Outcome`] is required, the same `impl Into<Outcome>` as
    /// [`Item::fix`].
    #[allow(clippy::too_many_arguments)]
    pub fn operation(
        outcome: impl Into<Outcome>,
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
            outcome: Some(outcome.into()),
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
            {
                let basis = crate::ids::advisory_basis(m.id);
                (!m.params.is_empty() || m.element_path.is_some() || basis.is_some()).then(|| {
                    Data {
                        params: m.params.clone(),
                        element_path: m.element_path.as_ref().map(|p| p.path.clone()),
                        namespaces: m
                            .element_path
                            .as_ref()
                            .map(|p| p.namespaces.clone())
                            .unwrap_or_default(),
                        advisory_basis: basis.map(|b| b.as_str()),
                        violation_kind: m.violation_kind.map(|k| k.as_str()),
                    }
                })
            },
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
    /// `spec-ahead` | `spec-silent` — what an `ADV-*` finding is grounded in.
    /// Present only on advisory findings; absent everywhere else.
    ///
    /// In `data` rather than on the shared `Item` deliberately: the envelope is
    /// a family-wide contract (FORMATS.md) and this is a fact about *our*
    /// advisory family, which no other tool has. See
    /// [`AdvisoryBasis`](crate::ids::AdvisoryBasis) for what the two mean and
    /// why only one of them is temporary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory_basis: Option<&'static str>,
    /// Which of the six [`ViolationKind`](crate::report::ViolationKind)s a
    /// schema violation is, as its stable machine spelling. Present only on
    /// findings from a rule that carries kinds; absent everywhere else, and
    /// never absent *within* such a rule — see
    /// [`Message::violation_kind`](crate::report::Message::violation_kind),
    /// which also documents what `params[0]` means alongside it.
    ///
    /// In `data` for the same reason `advisory_basis` is: the envelope is a
    /// family-wide contract (FORMATS.md) and this is a fact about our RELAX NG
    /// engine, which no other tool in the family has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_kind: Option<&'static str>,
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
                infos: 0,
                usages: 0,
                suppressed: Vec::new(),
            }),
            items: vec![],
        };
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(key_set(&v), ["items", "path", "status", "summary"]);
    }

    #[test]
    fn every_counter_is_present_at_zero_and_suppressed_is_not_a_counter() {
        // FORMATS §1.4: a counter over a closed set reports every member the
        // tool has a concept of, including zero. `Severity` has exactly five,
        // so a clean book carries five counters — never three, which is what
        // `skip_serializing_if = "is_zero"` produced before 0.14.0 and which
        // read as "this book has no usage findings".
        let clean = Summary::of(&Report::new(), &[]);
        let v = serde_json::to_value(&clean).unwrap();
        assert_eq!(
            key_set(&v),
            ["error", "fatal", "info", "usage", "warning"],
            "all five counters, and no `suppressed` key on an unfiltered run"
        );
        for k in ["fatal", "error", "warning", "info", "usage"] {
            assert_eq!(v[k], 0, "{k} must be present as 0, not absent");
        }
    }

    #[test]
    fn suppressed_records_the_gate_not_the_outcome() {
        // The marker is set by the filter being *in effect*, not by anything
        // having been removed — a clean book under `-u`-off says so too, which
        // is the whole point of a completeness marker. If this ever becomes
        // count-derived, this test is what fails.
        let nothing_to_hide = Summary::of(&Report::new(), &["usage"]);
        let v = serde_json::to_value(&nothing_to_hide).unwrap();
        assert_eq!(v["suppressed"], serde_json::json!(["usage"]));
        assert_eq!(v["usage"], 0, "nothing was withheld, and it is still gated");
    }

    #[test]
    fn a_filtered_input_carries_the_marker_and_an_unreadable_one_carries_no_summary() {
        // The two halves of FORMATS §1.4's "omission may not dodge the marker":
        // an input that produced a report under a filter MUST carry it, and one
        // that produced no report carries neither it nor a summary — its
        // `status` already says the counters do not exist.
        let reported = Input::from_report("book.epub".into(), &Report::new(), &["usage"]);
        let v = serde_json::to_value(&reported).unwrap();
        assert_eq!(v["summary"]["suppressed"], serde_json::json!(["usage"]));

        let unreadable = Input::<Summary, Data>::from_error("dir".into(), "is a directory".into());
        let v = serde_json::to_value(&unreadable).unwrap();
        assert!(
            v.get("summary").is_none(),
            "an input that produced no report manufactures no counters"
        );
    }

    #[test]
    fn a_foreign_outcome_reaches_the_wire_through_from() {
        // The shape epubsana asked for: their enum stays theirs, one `From` is
        // the meeting point, and the wire spelling is ours. A wildcard-free
        // match in that `From` is what makes a new member a build error there.
        #[derive(Clone, Copy)]
        enum TheirOutcome {
            Skipped,
        }
        impl From<TheirOutcome> for Outcome {
            fn from(o: TheirOutcome) -> Self {
                match o {
                    TheirOutcome::Skipped => Outcome::Skipped,
                }
            }
        }
        let item: Item = Item::fix(
            TheirOutcome::Skipped,
            "OPF-002".into(),
            None,
            "error",
            None,
            None,
            "declined".into(),
            None,
        );
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["outcome"], "skipped", "lowercase, as §1.3 declares");
    }

    #[test]
    fn clean_envelope_omits_top_level_summary_and_dry_run() {
        // epubveri passes `summary: None` and never a dry run, so neither key
        // appears (skip-if-none / skip-if-false).
        let env: Envelope = Envelope::for_tool("epubveri", "1.2.3", "0.5", None, vec![]);
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
            Outcome::Applied,
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
            violation_kind: None,
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
