//! The EPUB Structural Semantics Vocabulary (SSV) - the `epub:type`
//! terms, and the three facts about them the checks need: whether a term
//! exists at all, whether it is deprecated, and whether it may appear in an
//! HTML content document.
//!
//! These live together deliberately. They used to be split across modules,
//! and drifted: seven deprecated terms were missing from the "known" list,
//! so `sidebar` was reported as *both* "not in the default vocabulary"
//! (OPF-088) and "deprecated" (OPF-086b) - claims that contradict each
//! other, since knowing a term is deprecated means knowing the term
//! (reported by Doitsu on the MobileRead forum). `is_default_vocab_type`
//! now derives from both tables rather than repeating them, so that
//! particular contradiction cannot be stated.

/// Non-deprecated SSV terms. Generously inclusive: every finding built on
/// this is usage-level, so missing a real term (staying quiet) is far safer
/// than flagging a legitimate one, hence biased toward inclusion.
///
/// Deprecated terms are deliberately *not* repeated here - see
/// [`DEPRECATED`] and [`is_default_vocab_type`].
const KNOWN: &[&str] = &[
    "abstract",
    "acknowledgments",
    "afterword",
    "answer",
    "answers",
    "appendix",
    "aside",
    "assessment",
    "assessments",
    "backlink",
    "backmatter",
    "bibliography",
    "bodymatter",
    "chapter",
    "colophon",
    "concludingsentence",
    "conclusion",
    "contributors",
    "copyright-page",
    "cover",
    "credit",
    "credits",
    "dedication",
    "division",
    "endnotes",
    "epigraph",
    "epilogue",
    "errata",
    "example",
    "footnote",
    "footnotes",
    "foreword",
    "frontmatter",
    "figure",
    "fulltitlepage",
    "glossary",
    "glossdef",
    "glossref",
    "glossterm",
    "halftitlepage",
    "imprimatur",
    "imprint",
    "index",
    "index-editor-note",
    "index-entry",
    "index-entry-list",
    "index-group",
    "index-headnotes",
    "index-legend",
    "index-locator",
    "index-locator-list",
    "index-locator-range",
    "index-term",
    "index-term-categories",
    "index-term-category",
    "index-xref-preferred",
    "index-xref-related",
    "introduction",
    "keyword",
    "landmarks",
    "learning-objective",
    "learning-objectives",
    "learning-outcome",
    "learning-outcomes",
    "learning-resource",
    "learning-resources",
    "learning-standard",
    "learning-standards",
    "list",
    "list-item",
    "loa",
    "loi",
    "lot",
    "lov",
    "noteref",
    "notice",
    "ordinal",
    "other-credits",
    "page-list",
    "pagebreak",
    "part",
    "practice",
    "practice-answer",
    "preamble",
    "preface",
    "prologue",
    "pullquote",
    "qna",
    "question",
    "region-based",
    "revision-history",
    "seriespage",
    "subtitle",
    "table",
    "table-cell",
    "table-row",
    "tip",
    "title",
    "titlepage",
    "toc",
    "toc-brief",
    "topic-sentence",
    "translator-note",
    "volume",
];

/// SSV terms the vocabulary deprecates, each with what to use instead -
/// `None` where the spec names nothing, so there is nothing honest to
/// suggest. Complete as of EPUB SSV 1.1 (appendix A), which names a
/// replacement for 5 of the 13.
///
/// The replacement is a phrase, not a bare term, because they are not all
/// the same kind of thing: four name another SSV semantic, while `sidebar`
/// is replaced by an HTML element rather than by any `epub:type` value at
/// all. It slots into "consider {replacement} instead".
pub(crate) const DEPRECATED: &[(&str, Option<&str>)] = &[
    ("annoref", None),
    ("annotation", None),
    ("biblioentry", None),
    ("bridgehead", None),
    ("endnote", None),
    ("help", Some("the \"tip\" semantic")),
    ("marginalia", None),
    ("note", Some("the \"footnote\" semantic")),
    ("rearnote", None),
    ("rearnotes", Some("the \"endnotes\" semantic")),
    ("sidebar", Some("a bare HTML \"aside\" element")),
    ("subchapter", None),
    ("warning", Some("the \"notice\" semantic")),
];

/// SSV terms whose HTML usage context the vocabulary gives as "Not
/// Allowed": they carry a media-overlay meaning only, identifying a `seq`
/// or `par` as escapable/skippable structure, and have no meaning on an
/// HTML element at all.
pub(crate) const MEDIA_OVERLAY_ONLY: &[&str] = &[
    "table",
    "table-row",
    "table-cell",
    "list",
    "list-item",
    "figure",
    "aside",
];

/// Whether `token` is a term of the default vocabulary - deprecated or not.
///
/// Deprecation is a statement *about* a term the vocabulary defines, so a
/// deprecated term is in the vocabulary. Deriving this from both tables,
/// rather than keeping a third hand-maintained list, is what keeps
/// OPF-088 and OPF-086b from contradicting each other on the same value.
pub(crate) fn is_default_vocab_type(token: &str) -> bool {
    KNOWN.contains(&token) || DEPRECATED.iter().any(|(t, _)| *t == token)
}

/// Whether `token` may not appear on an HTML element (see
/// [`MEDIA_OVERLAY_ONLY`]).
pub(crate) fn is_media_overlay_only(token: &str) -> bool {
    MEDIA_OVERLAY_ONLY.contains(&token)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defect this module exists to prevent: a term cannot be both
    /// unknown and known-to-be-deprecated. Enforced over the whole table,
    /// not just the seven that had drifted.
    #[test]
    fn every_deprecated_term_is_in_the_vocabulary() {
        for (t, _) in DEPRECATED {
            assert!(
                is_default_vocab_type(t),
                "'{t}' is deprecated, so it must be a known term - reporting \
                 OPF-088 'not in the default vocabulary' alongside OPF-086b \
                 'is deprecated' contradicts itself"
            );
        }
    }

    /// The two tables must stay disjoint: `KNOWN` holding a deprecated term
    /// as well would make `DEPRECATED` no longer the single answer to "is
    /// this deprecated".
    #[test]
    fn known_does_not_repeat_deprecated_terms() {
        for (t, _) in DEPRECATED {
            assert!(!KNOWN.contains(t), "'{t}' is listed twice");
        }
    }

    /// Media-overlay-only terms are real vocabulary terms - they just have
    /// no HTML usage context. They must not also draw an OPF-088.
    #[test]
    fn media_overlay_only_terms_are_known() {
        for t in MEDIA_OVERLAY_ONLY {
            assert!(is_default_vocab_type(t), "'{t}' must be a known term");
        }
    }
}
