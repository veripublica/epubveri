//! Validation report: a flat list of diagnostics with epubcheck-style message IDs.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "ERROR",
            Severity::Warning => "WARNING",
            Severity::Info => "INFO",
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
    pub(crate) fn of_parse_error(err: &roxmltree::Error) -> Position {
        let p = err.pos();
        Position {
            line: p.row,
            column: p.col,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    /// epubcheck-compatible message ID (e.g. "RSC-001"). See `ids.rs`.
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
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub messages: Vec<Message>,
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
        });
    }

    pub fn errors(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.severity == Severity::Warning)
            .count()
    }

    pub fn is_valid(&self) -> bool {
        self.errors() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
