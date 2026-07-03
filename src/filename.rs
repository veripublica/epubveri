//! OCF §4.1.3 file-name conformance: forbidden characters (PKG-009), a
//! trailing full stop (PKG-011), non-ASCII usage (PKG-012, Info), and (via
//! `canonical_fold_key`, used by the OPF-060 duplicate-detection in
//! `ocf.rs`) case-folding/NFC-normalization equivalence between two names.
//!
//! Applied both to real container entry names (`ocf.rs`, a full-
//! publication check) and to a manifest item's declared `href` (`opf.rs`)
//! - real epubcheck's single-package-document check mode has no actual
//! container to inspect, so it validates the declared href string itself
//! (confirmed via a real fixture pair testing the identical defect once as
//! a real file name and once as a bare `.opf`'s manifest href).

use unicode_normalization::UnicodeNormalization;

/// True for a codepoint the OCF spec forbids in a file name. Assembled
/// from the real corpus's own `filename-checker.feature` table (not
/// guessed) - control characters (C0/DEL/C1), the ASCII punctuation set
/// `" * : < > ? \ |`, the replacement character, Unicode noncharacters,
/// the "language tag" block (except the reinstated `E007F` cancel tag),
/// and the private-use areas.
fn is_forbidden(c: char) -> bool {
    let cp = c as u32;
    matches!(c, '"' | '*' | ':' | '<' | '>' | '?' | '\\' | '|')
        || cp <= 0x1F
        || cp == 0x7F
        || (0x80..=0x9F).contains(&cp)
        || c == '\u{FFFD}'
        || (0xFDD0..=0xFDEF).contains(&cp)
        || (cp & 0xFFFE) == 0xFFFE
        // Only the literal LANGUAGE TAG start character is forbidden, not
        // the whole E0000-E007F "Tags" block - that block's tag *letters*
        // (E0020-E007A) and the CANCEL TAG (E007F) are legitimately used
        // by real Unicode emoji tag sequences (e.g. a flag emoji), and a
        // real corpus fixture uses exactly that in a *valid* file name.
        || cp == 0xE0001
        || (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
}

/// PKG-009: does this name contain any forbidden character at all -
/// reported once per name regardless of how many/which (confirmed via a
/// real fixture, "disallowed characters are reported only once").
pub(crate) fn has_forbidden_char(name: &str) -> bool {
    name.chars().any(is_forbidden)
}

/// PKG-011: a name ending in a literal full stop.
pub(crate) fn ends_with_full_stop(name: &str) -> bool {
    name.ends_with('.')
}

/// PKG-012 (usage): any non-ASCII character present.
pub(crate) fn has_non_ascii(name: &str) -> bool {
    !name.is_ascii()
}

/// The key OPF-060 duplicate-detection groups names by: NFC-normalize
/// first (so canonically-equivalent forms collide - confirmed via a real
/// precomposed-vs-decomposed-Á fixture pair - but merely *compatibility*-
/// equivalent forms, like a math double-struck ℍ vs plain H, must NOT
/// collide, confirmed via a real "-valid" fixture using exactly that
/// pair), then full-case-folds each character. Rust's `char::to_lowercase`
/// performs Unicode *simple*/*special* lowercase mapping, not full case
/// *folding* - the one real difference the corpus exercises is German
/// sharp s (ß, U+00DF), whose full case fold is "ss" but whose lowercase
/// mapping is itself unchanged; a hardcoded special case covers exactly
/// that without needing the complete CaseFolding.txt table.
pub(crate) fn canonical_fold_key(name: &str) -> String {
    let nfc: String = name.nfc().collect();
    nfc.chars()
        .flat_map(|c| {
            if c == '\u{00DF}' {
                vec!['s', 's']
            } else {
                c.to_lowercase().collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_chars_detected() {
        assert!(has_forbidden_char("a*name"));
        assert!(has_forbidden_char("content_|.xhtml"));
        assert!(has_forbidden_char("file_<>.txt"));
        assert!(!has_forbidden_char("only-ascii.xhtml"));
        assert!(!has_forbidden_char("not-ascii-é.xhtml"));
    }

    #[test]
    fn full_stop_detected() {
        assert!(ends_with_full_stop("aname."));
        assert!(!ends_with_full_stop("aname"));
    }

    #[test]
    fn non_ascii_detected() {
        assert!(has_non_ascii("nav_Ф.xhtml"));
        assert!(!has_non_ascii("nav.xhtml"));
    }

    #[test]
    fn case_fold_matches_common_and_full_folding() {
        assert_eq!(
            canonical_fold_key("CONTENT_001.xhtml"),
            canonical_fold_key("content_001.xhtml")
        );
        assert_eq!(
            canonical_fold_key("content_ss.xhtml"),
            canonical_fold_key("content_\u{00DF}.xhtml")
        );
    }

    #[test]
    fn case_fold_matches_canonical_not_compatibility_normalization() {
        // precomposed Á (U+00C1) vs decomposed A + combining acute (U+0301)
        let precomposed = "content_\u{00C1}.xhtml";
        let decomposed = "content_A\u{0301}.xhtml";
        assert_eq!(
            canonical_fold_key(precomposed),
            canonical_fold_key(decomposed)
        );

        // double-struck ℍ (U+210D) is only *compatibility*-equivalent to H
        let double_struck = "content_\u{210D}.xhtml";
        let plain_h = "content_H.xhtml";
        assert_ne!(
            canonical_fold_key(double_struck),
            canonical_fold_key(plain_h)
        );
    }
}
