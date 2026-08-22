//! Validation report: a flat list of diagnostics with epubcheck-style message IDs.

use std::fmt;

/// Severity of a diagnostic, in rank order, mirroring epubcheck's five-value
/// vocabulary — the same set the shared machine format reserves (FORMATS.md
/// §1.3, conventions v0.4). `Fatal` means processing of the input stopped;
/// `Usage` is an advisory that sits *below* `Info` — surfaced, never a failure.
/// Only `Error` and `Fatal` cross the valid/invalid line (see [`Report::is_valid`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Fatal,
    Error,
    Warning,
    Info,
    Usage,
}

impl Severity {
    /// The lowercase spelling the shared json envelope uses (FORMATS.md §1.3).
    /// The uppercase [`Display`](fmt::Display) form is for the human CLI report.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Fatal => "fatal",
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Usage => "usage",
        }
    }
}

impl fmt::Display for Severity {
    // Uppercase, epubcheck-familiar, for the human CLI report. The json envelope
    // spells severity lowercase instead — use [`Severity::as_str`] there.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Fatal => "FATAL",
            Severity::Error => "ERROR",
            Severity::Warning => "WARNING",
            Severity::Info => "INFO",
            Severity::Usage => "USAGE",
        })
    }
}

/// A 1-indexed line/column position in a source file's original text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    /// Position of `node` in its document's original text. DOM-based
    /// checks always have a `roxmltree::Node` in scope for the violation
    /// being reported, so this needs no extra plumbing.
    pub(crate) fn of(node: roxmltree::Node) -> Position {
        let p = node.document().text_pos_at(node.range().start);
        Position {
            line: p.row,
            column: p.col,
        }
    }

    /// Position of `attr` itself - the first character of its name - rather
    /// than of the element carrying it.
    ///
    /// The distinction is the whole finding for an attribute fault: a reader
    /// (or a Sigil/calibre plugin placing a cursor) sent to the element start
    /// has to hunt along the start tag for the attribute we named, and on a
    /// long start tag that is the difference between a usable column and a
    /// decorative one. `element_path` has pinned the attribute since #18; this
    /// is the human half catching up.
    ///
    /// Not a parity change: epubcheck's SAX locator reports attribute faults at
    /// the character *after* the start tag's `>`, so its column pointed at
    /// neither the element nor the attribute and ours never matched it.
    pub(crate) fn of_attr(node: roxmltree::Node, attr: roxmltree::Attribute) -> Position {
        let p = node.document().text_pos_at(attr.range().start);
        Position {
            line: p.row,
            column: p.col,
        }
    }

    /// Position of a byte `offset` into raw `text`. For checks that scan
    /// bytes/text directly instead of a parsed `roxmltree::Document`
    /// (e.g. `htm.rs`'s XML-declaration/DOCTYPE checks, which must still
    /// fire on documents that don't parse as well-formed XML).
    ///
    /// Column is counted in **chars**, not bytes, to match `Position::of`
    /// (which delegates to `roxmltree`'s own char-based column counting) -
    /// counting bytes instead would silently disagree with `of` on any line
    /// containing multi-byte UTF-8 text before the offset.
    pub(crate) fn of_offset(text: &str, offset: usize) -> Position {
        let before = &text[..offset.min(text.len())];
        let line = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let column = match before.rfind('\n') {
            Some(nl) => before[nl + 1..].chars().count() as u32 + 1,
            None => before.chars().count() as u32 + 1,
        };
        Position { line, column }
    }

    /// Position reported by a `roxmltree` parse error (its own row/column).
    /// For the "not well-formed XML" branches, which have a concrete parse
    /// error but no parsed node to point at - surfacing the exact spot the
    /// parser failed is far more actionable for a downstream fixer (e.g.
    /// epublift) than a bare file name.
    pub(crate) fn of_parse_error(err: &crate::ocf::XmlError) -> Position {
        let p = err.pos();
        Position {
            line: p.row,
            column: p.col,
        }
    }
}

/// What kind of thing a schema violation is, independent of how the message
/// happens to be worded.
///
/// **These six are not a classification invented for consumers — they are the
/// discriminant the RELAX NG engine already computes.** `rng::Blame` has exactly
/// these six states; `push_blame` reads them to pick the right anchor and
/// `Blame::describe` then renders them into an English sentence, at which point
/// the discriminant used to be dropped. epubsana was reconstructing it by
/// slicing the leading words of that sentence, which splits or merges its groups
/// silently whenever a message is improved — twice in the two weeks before this
/// shipped. So `violation_kind` restores a value we were deleting rather than
/// adding one we now have to maintain.
///
/// The mapping in [`crate::rng::Blame::kind`] **must stay a wildcard-free
/// `match`**. That is not style: it is the only thing that turns a seventh
/// engine state into a compile error here rather than a silent
/// reclassification, and a consumer's [`ALL`](ViolationKind::ALL) test is the
/// backstop for a property nothing but this note enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ViolationKind {
    /// The element is not permitted at this position.
    ElementNotAllowed,
    /// A required child of the element is absent.
    IncompleteContent,
    /// The element is missing a required attribute.
    MissingAttribute,
    /// Character data where the content model admits none.
    StrayText,
    /// No attribute of this name is permitted at this position.
    AttributeNotAllowed,
    /// The attribute name is permitted; its value does not satisfy the datatype.
    InvalidAttributeValue,
}

impl ViolationKind {
    /// Every kind, so a consumer can assert the set it knows about and notice a
    /// seventh the moment it resolves a new version.
    ///
    /// This exists because the compile error people expect from an exhaustive
    /// enum does not actually fire for the two ways a consumer uses this:
    /// grouping needs `Ord`/`Hash`/`Eq`, and fixer dispatch is equality, and
    /// neither is a `match` (epubsana, 2026-08-22). Charging a major version for
    /// a signal a consumer has to build a deliberate tripwire to receive is a
    /// bad trade; `ALL` gives them the signal without it, and keeps giving it
    /// after this enum becomes `#[non_exhaustive]` at 1.0.
    pub const ALL: &'static [ViolationKind] = &[
        ViolationKind::ElementNotAllowed,
        ViolationKind::IncompleteContent,
        ViolationKind::MissingAttribute,
        ViolationKind::StrayText,
        ViolationKind::AttributeNotAllowed,
        ViolationKind::InvalidAttributeValue,
    ];

    /// The stable machine spelling, and what the json envelope carries. Fixed
    /// like a message ID: changing one is a breaking change for a consumer
    /// grouping on it, not a rewording.
    pub fn as_str(&self) -> &'static str {
        match self {
            ViolationKind::ElementNotAllowed => "element_not_allowed",
            ViolationKind::IncompleteContent => "incomplete_content",
            ViolationKind::MissingAttribute => "missing_attribute",
            ViolationKind::StrayText => "stray_text",
            ViolationKind::AttributeNotAllowed => "attribute_not_allowed",
            ViolationKind::InvalidAttributeValue => "invalid_attribute_value",
        }
    }
}

impl fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    /// epubcheck-compatible message ID (e.g. "RSC-001"). See `ids.rs`. The one
    /// exception is the tool-owned `ADV-*` advisory family (opt-in, `--advisory`),
    /// which epubcheck has no equivalent for.
    pub id: &'static str,
    pub severity: Severity,
    pub text: String,
    pub location: Option<String>,
    pub position: Option<Position>,
    /// epubveri's own stable, semantic sub-code (e.g.
    /// `"opf.spine.duplicate_itemref"`), distinguishing the many unrelated
    /// violations a shared, epubcheck-compatible `id` (esp. `RSC-005`) can
    /// mean. `None` until a check site is retrofitted - rollout is
    /// incremental, by priority, not all at once (see issue #2). `id`
    /// itself never absorbs this: it stays exactly the epubcheck-
    /// compatibility contract it always was.
    pub rule: Option<&'static str>,
    /// The positional values interpolated into `text` (mirroring
    /// epubcheck's own Java message-template `{0}`/`{1}` approach) - lets
    /// a consumer eventually re-render `text` from a localized template
    /// keyed by `rule`, instead of parsing the English sentence. Empty
    /// when `rule` is `None` or the message has no interpolated values.
    pub params: Vec<String>,
    /// A machine-resolvable, XPath-style path to the offending node, with the
    /// namespace bindings needed to resolve it (issue #18). `Some` only at
    /// sites emitted through [`push_node`](Report::push_node) /
    /// [`push_node_attr`](Report::push_node_attr) — i.e. that had a
    /// `roxmltree` node in hand. Rollout is incremental, like `rule`/`params`.
    pub element_path: Option<crate::xmlext::NodePath>,
    /// Which of the six [`ViolationKind`]s a schema violation is, for a
    /// consumer that needs to group or dispatch on the *kind* of fault without
    /// parsing `text`.
    ///
    /// **`None` is a statement about the rule, never about the finding.** A rule
    /// that carries kinds always sets it — the mapping is a total `match` over
    /// the engine's six states, so there is no path that produces a kindless
    /// schema violation — and every other rule leaves it `None`. A consumer
    /// meeting `None` therefore knows it is looking at a rule outside this
    /// family, not at a violation whose kind we failed to determine
    /// (epubsana's requirement, 2026-08-22).
    ///
    /// ## What `params[0]` means when this is `Some`
    ///
    /// `params[0]` is the name of the finding's subject, and **the spelling
    /// differs by kind**, deliberately:
    ///
    /// - [`AttributeNotAllowed`](ViolationKind::AttributeNotAllowed) and
    ///   [`InvalidAttributeValue`](ViolationKind::InvalidAttributeValue): the
    ///   attribute name *as qualified for display*, carrying the conventional
    ///   prefix for the `epub`, `xml`, `xlink` and `opf` namespaces and bare
    ///   otherwise.
    /// - The element kinds and [`StrayText`](ViolationKind::StrayText): the
    ///   **local name** of the element (for stray text, of its containing
    ///   element), never prefixed.
    ///
    /// The two spellings never meet inside one group key, because the kind
    /// already separates attribute faults from element faults — which is why
    /// this is left as it is rather than made uniform. Changing either spelling
    /// is a release-note event.
    ///
    /// **`params[0]` is not a string that appears in the document.** The
    /// attribute prefix is reconstructed from the namespace rather than read
    /// from the source (see `rng::qualified_attribute_name`), so a book binding
    /// `xmlns:e="http://www.idpf.org/2007/ops"` and writing `e:type` still
    /// yields `"epub:type"`. It is an identity token for display and grouping,
    /// and **must not be used as a lookup key into the source text**.
    ///
    /// One known limit, inherited rather than introduced: element names are
    /// local, so `(violation_kind, params[0])` cannot distinguish two
    /// namespaces — an SVG `title` and an XHTML `title` share a key, as do a
    /// no-namespace `html` and a real one. The message can say so (#84); the
    /// key cannot represent it.
    pub violation_kind: Option<ViolationKind>,
}

impl Message {
    /// One finding, in the exact line format the `epubveri` CLI prints. No
    /// trailing newline.
    ///
    /// ```text
    /// ERROR RSC-020: NCX content src 'a b.xhtml' contains unencoded spaces [toc.ncx:18:7]
    /// ```
    ///
    /// **This is the primitive on purpose, and [`Report::render_human`] is
    /// built on it rather than the other way round.** epubsana asked for it
    /// that way (2026-08-21) and the reasoning generalises to any consumer:
    /// its own output groups findings by rule, and by message shape inside
    /// `schema_violation`, because a flat 3,113-line dump is the experience it
    /// exists to improve. A whole-report call cannot serve that — a consumer
    /// that only has one would reimplement this line and drift from it the
    /// first time either side touches a severity word, the location brackets
    /// or the spacing, silently and with nothing failing.
    ///
    /// So: group however you like, and never diverge on the line itself.
    pub fn render_human(&self) -> String {
        let loc = self
            .location
            .as_deref()
            .map(|l| match self.position {
                Some(p) => format!(" [{l}:{}:{}]", p.line, p.column),
                None => format!(" [{l}]"),
            })
            .unwrap_or_default();
        format!("{} {}: {}{}", self.severity, self.id, self.text, loc)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub messages: Vec<Message>,
    /// The `version` the package document declared (`"3.0"`, `"2.0"`, …), as
    /// written — `None` when no OPF was reached or it declared no version.
    ///
    /// Recorded because some checks are version-dependent but run outside the
    /// package document entirely: PKG-017 vs PKG-024 pick their ID and
    /// severity by EPUB version, yet the filename they judge is only known to
    /// the file-level entry point, which never sees the OPF. Consumers get it
    /// for free — epubcheck likewise reports which version it validated.
    ///
    /// A multi-rendition publication has one per rootfile; this holds the
    /// last one checked. They must agree for the book to be valid anyway
    /// (a mixed set is PKG-013).
    pub epub_version: Option<String>,
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: &'static str, severity: Severity, text: impl Into<String>) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: None,
            position: None,
            rule: None,
            params: Vec::new(),
            element_path: None,
            violation_kind: None,
        });
    }

    pub fn push_at(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: None,
            rule: None,
            params: Vec::new(),
            element_path: None,
            violation_kind: None,
        });
    }

    /// Like `push_at`, but also records the exact source position of the
    /// violation (see `Position::of`/`Position::of_offset`).
    pub fn push_at_pos(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        position: Position,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: Some(position),
            rule: None,
            params: Vec::new(),
            element_path: None,
            violation_kind: None,
        });
    }

    /// Like `push`, but also records a stable semantic sub-code (`rule`)
    /// and the values interpolated into `text` (`params`) - for sites
    /// retrofitted for issue #2's `rule`/`params` rollout where there's
    /// no `location` at all (e.g. a whole-container failure detected
    /// before any file/OPF is even identified).
    pub fn push_rule(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: None,
            position: None,
            rule: Some(rule),
            params,
            element_path: None,
            violation_kind: None,
        });
    }

    /// Like `push_at`, but also records a stable semantic sub-code
    /// (`rule`) and the values interpolated into `text` (`params`) - for
    /// sites retrofitted for issue #2's `rule`/`params` rollout where no
    /// node (and so no `Position`) is available. See `push_full` for the
    /// position-carrying equivalent.
    pub fn push_at_rule(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: None,
            rule: Some(rule),
            params,
            element_path: None,
            violation_kind: None,
        });
    }

    /// Like `push_at_pos`, but also records a stable semantic sub-code
    /// (`rule`) and the values interpolated into `text` (`params`) - see
    /// `Message::rule`/`Message::params`. The most complete variant;
    /// used only at call sites retrofitted for issue #2's incremental
    /// `rule`/`params` rollout (`RSC-005` first).
    #[allow(clippy::too_many_arguments)]
    pub fn push_full(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        position: Position,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: Some(position),
            rule: Some(rule),
            params,
            element_path: None,
            violation_kind: None,
        });
    }

    /// Like `push_full`, but with a **pre-computed** `position` and
    /// `element_path` (issue #22). For findings emitted after the document that
    /// held the offending node has already gone out of scope - the source
    /// location is captured earlier (while the node is live) and carried here.
    /// e.g. RSC-011 anchors at the source `<a>` hyperlink, collected in an
    /// earlier per-document pass.
    #[allow(clippy::too_many_arguments)]
    pub fn push_full_path(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        position: Position,
        element_path: crate::xmlext::NodePath,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: Some(position),
            rule: Some(rule),
            params,
            element_path: Some(element_path),
            violation_kind: None,
        });
    }

    /// Like `push_full`, but derives both the source `position` and a
    /// machine-resolvable `element_path` (issue #18) from the `roxmltree`
    /// node the finding is anchored at, instead of a pre-computed `Position`.
    /// For node-anchored sites whose finding is about a whole element.
    #[allow(clippy::too_many_arguments)]
    pub fn push_node(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        node: roxmltree::Node,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: Some(Position::of(node)),
            rule: Some(rule),
            params,
            element_path: Some(crate::xmlext::node_path(node)),
            violation_kind: None,
        });
    }

    /// Like [`Report::push_node`], but the finding is about a run of text;
    /// the element path pins the text run (`…/text()[n]`) rather than
    /// resolving to its containing element.
    #[allow(clippy::too_many_arguments)]
    pub fn push_node_text(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        node: roxmltree::Node,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.push_full_path(
            id,
            severity,
            text,
            location,
            Position::of(node),
            crate::xmlext::node_path_text(node),
            rule,
            params,
        );
    }

    /// Like `push_node`, but the finding is about a specific `attr` of `node`:
    /// the `element_path` ends in an `/@name` step pinning that attribute
    /// (issue #18) and the `position` points at the attribute itself
    /// (see [`Position::of_attr`]).
    #[allow(clippy::too_many_arguments)]
    pub fn push_node_attr(
        &mut self,
        id: &'static str,
        severity: Severity,
        text: impl Into<String>,
        location: impl Into<String>,
        node: roxmltree::Node,
        attr: roxmltree::Attribute,
        rule: &'static str,
        params: Vec<String>,
    ) {
        self.messages.push(Message {
            id,
            severity,
            text: text.into(),
            location: Some(location.into()),
            position: Some(Position::of_attr(node, attr)),
            rule: Some(rule),
            params,
            element_path: Some(crate::xmlext::node_path_attr(node, attr)),
            violation_kind: None,
        });
    }

    /// Attach a [`ViolationKind`] to the message that was **just pushed**.
    ///
    /// Deliberately not a parameter on the three `push_node*` helpers: they have
    /// well over a hundred call sites between them and all but the schema
    /// violations would pass `None`, which is a lot of noise to carry a value
    /// exactly one caller can supply. `push_blame` is that caller, it already
    /// owns the three-way routing, and it calls this on the line after the push
    /// — so the pairing is local enough to read in one glance and has nowhere
    /// to drift to.
    ///
    /// A no-op on an empty report rather than a panic: the only way to reach
    /// that is a caller that pushed nothing, and taking down an embedder over a
    /// missing diagnostic annotation is the wrong trade.
    pub(crate) fn attach_violation_kind(&mut self, kind: ViolationKind) {
        if let Some(m) = self.messages.last_mut() {
            m.violation_kind = Some(kind);
        }
    }

    fn count(&self, sev: Severity) -> usize {
        self.messages.iter().filter(|m| m.severity == sev).count()
    }

    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    /// Count of `Fatal`-severity findings — a defect that stopped processing.
    /// Kept separate from [`errors`](Self::errors): a consumer reading
    /// `{errors: N}` sees the same number it always did, and asks for fatals by
    /// name. Both count toward [`is_valid`](Self::is_valid).
    pub fn fatals(&self) -> usize {
        self.count(Severity::Fatal)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    /// Valid = no `error`- or `fatal`-severity findings (conventions v0.4 §6's
    /// verifier threshold). Warnings, info and usage findings are reported but
    /// never make a book invalid.
    pub fn is_valid(&self) -> bool {
        self.errors() == 0 && self.fatals() == 0
    }

    /// The verdict line the CLI closes a report with. No trailing newline.
    ///
    /// ```text
    /// — 0 error(s), 1 warning(s): VALID
    /// ```
    ///
    /// The fatal count leads only when there is one, so a fatal-only book does
    /// not read as "0 error(s) … INVALID".
    ///
    /// Separate from [`render_human`](Self::render_human) because a consumer
    /// that prints our findings under its own verdict wants the lines without
    /// this, and one that mirrors the CLI wants both.
    pub fn render_summary(&self) -> String {
        let fatals = self.fatals();
        let head = if fatals > 0 {
            format!("{fatals} fatal, ")
        } else {
            String::new()
        };
        format!(
            "— {}{} error(s), {} warning(s): {}",
            head,
            self.errors(),
            self.warnings(),
            if self.is_valid() { "VALID" } else { "INVALID" }
        )
    }

    /// Every finding, one per line, then the verdict — the body of what the
    /// CLI prints for a single book. No trailing newline.
    ///
    /// Built on [`Message::render_human`], which is the primitive; see its
    /// note for why that direction matters to consumers that group findings
    /// rather than listing them flat.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        for m in &self.messages {
            out.push_str(&m.render_human());
            out.push('\n');
        }
        out.push_str(&self.render_summary());
        out
    }

    /// Order findings the way epubcheck does — by document position — so a book
    /// reads top-to-bottom instead of in check-execution order. Checks run in
    /// passes (grammar, then Schematron, then hand-coded, then CSS, …) and each
    /// walks the document its own way, so without this the same error scattered
    /// down a file comes out interleaved by *which check* found it, not *where*
    /// — and any check that iterates a hash container adds a nondeterministic
    /// shuffle on top (MobileRead #111 / issue #32).
    ///
    /// Files keep their existing first-seen order — the spine/processing order
    /// the validator already emits them in — so only the ordering *within* each
    /// file changes, sorted by `(line, column)`. The sort is stable, so findings
    /// at the same spot (and file-level findings with no position, which sort to
    /// the front of their file) keep their original relative order. Called once
    /// at the end of validation, so every consumer (CLI, wasm, JSON) sees it.
    pub fn sort_by_document_order(&mut self) {
        let mut file_order: std::collections::HashMap<Option<String>, usize> =
            std::collections::HashMap::new();
        for m in &self.messages {
            if !file_order.contains_key(&m.location) {
                file_order.insert(m.location.clone(), file_order.len());
            }
        }
        self.messages.sort_by_key(|m| {
            (
                file_order[&m.location],
                m.position.map(|p| (p.line, p.column)),
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::ViolationKind as K;

    /// The six machine spellings are a contract: a consumer groups on them and
    /// the json envelope carries them, so changing one is a breaking change
    /// rather than a rewording. Spelled out here so that editing `as_str` has
    /// to be deliberate.
    #[test]
    fn the_kind_spellings_are_fixed() {
        assert_eq!(K::ElementNotAllowed.as_str(), "element_not_allowed");
        assert_eq!(K::IncompleteContent.as_str(), "incomplete_content");
        assert_eq!(K::MissingAttribute.as_str(), "missing_attribute");
        assert_eq!(K::StrayText.as_str(), "stray_text");
        assert_eq!(K::AttributeNotAllowed.as_str(), "attribute_not_allowed");
        assert_eq!(K::InvalidAttributeValue.as_str(), "invalid_attribute_value");
        assert_eq!(K::StrayText.to_string(), "stray_text");
    }

    /// `ALL` really lists every kind, once.
    ///
    /// **Be honest about what this can and cannot catch.** A `const` array is
    /// perfectly happy to be short, so no compiler and no test can prove `ALL`
    /// is complete from `ALL` alone — the list below is hand-written and shares
    /// the omission it is checking for. What makes the pair work is that adding
    /// a seventh variant is a **compile error** in `as_str` and in
    /// `Blame::kind`, and the `match` below is a third site that fails the same
    /// way; whoever fixes those three arms is standing next to `ALL` each time.
    /// The real backstop is a consumer's own `ALL` test, which is the reason
    /// this const exists (epubsana, 2026-08-22).
    #[test]
    fn all_lists_every_kind_exactly_once() {
        let witnesses = [
            K::ElementNotAllowed,
            K::IncompleteContent,
            K::MissingAttribute,
            K::StrayText,
            K::AttributeNotAllowed,
            K::InvalidAttributeValue,
        ];
        for w in witnesses {
            // Wildcard-free: a seventh variant stops this compiling.
            match w {
                K::ElementNotAllowed
                | K::IncompleteContent
                | K::MissingAttribute
                | K::StrayText
                | K::AttributeNotAllowed
                | K::InvalidAttributeValue => {}
            }
            assert!(K::ALL.contains(&w), "{w} is missing from ALL");
        }
        assert_eq!(K::ALL.len(), witnesses.len(), "ALL has an extra entry");

        let mut spellings: Vec<_> = K::ALL.iter().map(|k| k.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), K::ALL.len(), "two kinds share a spelling");
    }

    use super::*;

    // The migration trap (conventions v0.4 §6, unfold note): the valid/invalid
    // line is error-AND-above. A warning-only book is valid; the moment a fatal
    // appears the book is invalid — and a fatal is never miscounted as an error.
    #[test]
    fn fatal_and_error_invalidate_a_book_but_a_warning_does_not() {
        let mut r = Report::new();
        // Real constants, not literals: `"HTM-060"` stood here, a spelling
        // epubcheck never emits (it has only `HTM_060a`/`b`).
        r.push(crate::ids::HTM_060A, Severity::Warning, "a warning");
        r.push(crate::ids::OPF_090, Severity::Usage, "an advisory");
        r.push(crate::ids::HTM_055, Severity::Info, "a note");
        assert!(
            r.is_valid(),
            "warning/info/usage alone must stay valid (exit 0)"
        );

        r.push("PKG-006", Severity::Fatal, "the container stopped us");
        assert!(!r.is_valid(), "a fatal must make the book invalid (exit 1)");
        assert_eq!(r.errors(), 0, "a fatal is not counted as an error");
        assert_eq!(r.fatals(), 1);
        assert_eq!(r.warnings(), 1);
    }

    #[test]
    fn of_offset_first_line_first_column() {
        assert_eq!(
            Position::of_offset("<a/>", 0),
            Position { line: 1, column: 1 }
        );
    }

    #[test]
    fn of_offset_advances_line_and_resets_column_after_newline() {
        let text = "line one\nline two\nline three";
        // Offset of the 'l' starting "line three".
        let offset = text.find("line three").unwrap();
        assert_eq!(
            Position::of_offset(text, offset),
            Position { line: 3, column: 1 }
        );
    }

    #[test]
    fn of_offset_counts_chars_not_bytes_for_multibyte_utf8() {
        // "café" has 4 chars but 5 bytes (é is 2 bytes) - the offset right
        // after it must report column 5 (char count), not 6 (byte count),
        // to stay consistent with `Position::of`'s roxmltree-backed,
        // char-based column counting.
        let text = "café<br/>";
        let offset = text.find("<br/>").unwrap();
        assert_eq!(
            Position::of_offset(text, offset),
            Position { line: 1, column: 5 }
        );
    }

    #[test]
    fn of_matches_of_offset_for_the_same_node_position() {
        // A node preceded by multi-byte UTF-8 text on an earlier line -
        // `Position::of` (via roxmltree) and `Position::of_offset` (the
        // hand-rolled equivalent used for raw byte/text scans) must agree,
        // since both are surfaced through the same `Message.position`
        // field and consumers shouldn't see the counting convention change
        // depending on which check produced a given finding.
        let xml = "<root><a>café</a>\n<child/></root>";
        let doc = crate::ocf::parse_xml(xml).unwrap();
        let child = doc
            .descendants()
            .find(|n| n.tag_name().name() == "child")
            .unwrap();
        let via_node = Position::of(child);
        let offset = xml.rfind("<child/>").unwrap();
        let via_offset = Position::of_offset(xml, offset);
        assert_eq!(via_node, via_offset);
    }

    #[test]
    fn sort_orders_within_file_by_position_keeping_first_seen_file_order() {
        let mut r = Report::new();
        let pos = |line, column| Position { line, column };
        // File B is seen first, its findings pushed out of position order (as
        // separate check passes would); then a B file-level finding with no
        // position; then file A. Only within-file order should change.
        r.push_at_pos("RSC-005", Severity::Error, "b:20", "B.xhtml", pos(20, 1));
        r.push_at_pos("RSC-005", Severity::Error, "b:5:9", "B.xhtml", pos(5, 9));
        r.push_at_pos("RSC-005", Severity::Error, "b:5:2", "B.xhtml", pos(5, 2));
        r.push_at("RSC-005", Severity::Error, "b:file-level", "B.xhtml");
        r.push_at_pos("RSC-005", Severity::Error, "a:3", "A.xhtml", pos(3, 1));

        r.sort_by_document_order();

        let order: Vec<&str> = r.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "b:file-level", // no position sorts to the front of its file
                "b:5:2",        // (5,2) before (5,9) before (20,1)
                "b:5:9",
                "b:20",
                "a:3", // file A stays after B (first-seen file order preserved)
            ]
        );
    }

    #[test]
    fn sort_is_stable_for_findings_at_the_same_position() {
        let mut r = Report::new();
        let pos = Position { line: 9, column: 1 };
        // Two findings at the exact same spot (e.g. two bad attributes on one
        // element) must keep the order they were pushed in.
        r.push_at_pos("RSC-005", Severity::Error, "first", "c.xhtml", pos);
        r.push_at_pos("RSC-005", Severity::Error, "second", "c.xhtml", pos);
        r.sort_by_document_order();
        let order: Vec<&str> = r.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(order, vec!["first", "second"]);
    }
}
