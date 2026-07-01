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

#[derive(Debug, Clone)]
pub struct Message {
    /// epubcheck-compatible message ID (e.g. "RSC-001"). See `ids.rs`.
    pub id: &'static str,
    pub severity: Severity,
    pub text: String,
    pub location: Option<String>,
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
