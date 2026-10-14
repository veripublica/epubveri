//! The encoding an XML resource is in, as the validator decides it before
//! any parse: [`sniff`] reads the bytes, [`Sniffed::finding`] says what that
//! costs a resource of a given media type.
//!
//! **A port of epubcheck's `XMLEncodingSniffer`, not a reading of the XML
//! specification.** epubcheck grades encoding in `XMLParser.process()`, which
//! every XML resource it opens passes through — the package document, XHTML
//! and SVG, the NCX, Media Overlays, search key maps and the `META-INF` files,
//! in EPUB 2 and EPUB 3 alike — and it grades it from this sniff alone. What
//! the parser later makes of the bytes is a separate question, answered (or
//! not) by RSC-016. So the classification follows the sniffer exactly, edges
//! included:
//!
//! - only the first 256 bytes are read, and fewer than 4 is UTF-8;
//! - the UTF-16 signatures are tried before the UCS-4 ones, so a UTF-32LE
//!   byte order mark (`FF FE 00 00`) is UTF-16;
//! - a declaration is found by the literal `encoding=` in the leading ASCII
//!   bytes — with a space before the `=`, or no quote after it, there is no
//!   declaration and the resource is UTF-8;
//! - the declared name is compared case-blind with `UTF-8` and `UTF-16` and
//!   nothing else, so `utf8` and `UTF-16LE` are encodings other than UTF-8.
//!
//! Public because a transformer that leaves a document untouched for its
//! encoding reports it with these codes (kepubverto, 2026-10-14), and a second
//! copy of the sniffer is a second oracle that drifts.

use crate::ids::{HTM_058, RSC_027, RSC_028};
use crate::report::{Position, Report, Severity};

/// What [`sniff`] found. Non-exhaustive: epubcheck's sniffer could grow a
/// signature, and this enum follows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Sniffed {
    /// UTF-8: a UTF-8 byte order mark, a declaration of `UTF-8`, no
    /// declaration at all, or fewer than four bytes.
    Utf8,
    /// UTF-16, from a byte order mark, from `<?` in either 16-bit order, or
    /// from a declaration of `UTF-16`.
    Utf16,
    /// UCS-4 / UTF-32, from a byte order mark or from `<` in any 32-bit order.
    Ucs4,
    /// EBCDIC, from `<?xm` in EBCDIC.
    Ebcdic,
    /// An ASCII-compatible resource that declares some other encoding. The
    /// name is as the file spells it.
    Declared(String),
}

impl Sniffed {
    /// The name epubcheck puts in RSC-028's message: `UCS-4`, `EBCDIC`, or
    /// the declared name upper-cased. `None` for UTF-8 and UTF-16, which
    /// RSC-028 is never about.
    pub fn name(&self) -> Option<String> {
        match self {
            Sniffed::Utf8 | Sniffed::Utf16 => None,
            Sniffed::Ucs4 => Some("UCS-4".to_owned()),
            Sniffed::Ebcdic => Some("EBCDIC".to_owned()),
            Sniffed::Declared(name) => Some(name.to_ascii_uppercase()),
        }
    }

    /// The finding a resource of `media_type` draws for this encoding, as
    /// epubcheck grades it: UTF-16 is HTM-058 (error) on
    /// `application/xhtml+xml` and RSC-027 (warning) on anything else; every
    /// other encoding but UTF-8 is RSC-028 (error). `None` for UTF-8.
    ///
    /// The media type is the one the resource is checked as — the manifest's
    /// for a publication resource, `application/xml` for a `META-INF` file.
    pub fn finding(&self, media_type: &str) -> Option<(&'static str, Severity)> {
        match self {
            Sniffed::Utf8 => None,
            Sniffed::Utf16 if media_type == "application/xhtml+xml" => {
                Some((HTM_058, Severity::Error))
            }
            Sniffed::Utf16 => Some((RSC_027, Severity::Warning)),
            _ => Some((RSC_028, Severity::Error)),
        }
    }
}

const UTF16_MAGIC: [&[u8]; 4] = [
    &[0xFE, 0xFF],
    &[0xFF, 0xFE],
    &[0, 0x3C, 0, 0x3F],
    &[0x3C, 0, 0x3F, 0],
];
const UCS4_MAGIC: [&[u8]; 8] = [
    &[0, 0, 0xFE, 0xFF],
    &[0xFF, 0xFE, 0, 0],
    &[0, 0, 0xFF, 0xFE],
    &[0xFE, 0xFF, 0, 0],
    &[0, 0, 0, 0x3C],
    &[0, 0, 0x3C, 0],
    &[0, 0x3C, 0, 0],
    &[0x3C, 0, 0, 0],
];
const UTF8_MAGIC: &[u8] = &[0xEF, 0xBB, 0xBF];
const EBCDIC_MAGIC: &[u8] = &[0x4C, 0x6F, 0xA7, 0x94];

/// How many leading bytes [`sniff`] looks at; passing more is harmless.
pub const SNIFF_LEN: usize = 256;

/// Classify an XML resource's encoding from its first bytes, exactly as
/// epubcheck's sniffer does (see the module note for its edges).
pub fn sniff(bytes: &[u8]) -> Sniffed {
    let head = &bytes[..bytes.len().min(SNIFF_LEN)];
    if head.len() < 4 {
        return Sniffed::Utf8;
    }
    if UTF16_MAGIC.iter().any(|m| head.starts_with(m)) {
        return Sniffed::Utf16;
    }
    if UCS4_MAGIC.iter().any(|m| head.starts_with(m)) {
        return Sniffed::Ucs4;
    }
    if head.starts_with(UTF8_MAGIC) {
        return Sniffed::Utf8;
    }
    if head.starts_with(EBCDIC_MAGIC) {
        return Sniffed::Ebcdic;
    }
    let ascii_len = head
        .iter()
        .position(|&b| b == 0 || b > 0x7F)
        .unwrap_or(head.len());
    // All ASCII, so the conversion cannot fail.
    let header = std::str::from_utf8(&head[..ascii_len]).unwrap_or("");
    match declared_name(header) {
        None => Sniffed::Utf8,
        Some(name) if name.eq_ignore_ascii_case("UTF-8") => Sniffed::Utf8,
        Some(name) if name.eq_ignore_ascii_case("UTF-16") => Sniffed::Utf16,
        Some(name) => Sniffed::Declared(name.to_owned()),
    }
}

/// The value after the first `encoding=`, if it is quoted and the closing
/// quote is in `header`.
fn declared_name(header: &str) -> Option<&str> {
    let start = header.find("encoding=")? + "encoding=".len();
    let quote = header[start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = &header[start + 1..];
    Some(&value[..value.find(quote)?])
}

/// An XML resource's text, decoded so that the grading above and the parse
/// below agree on what the bytes are: UTF-16 by its BOM or by `<?` in either
/// 16-bit order, everything else as UTF-8 (lossily, as before).
///
/// epubcheck hands UTF-16 to Xerces, which decodes it; reading it as UTF-8
/// instead turned a decodable document into a fatal RSC-016 and silenced
/// every check after it (measured on 5.4.0, 2026-10-14: a UTF-16 SVG and a
/// BOM-less UTF-16 XHTML, each RSC-027 / HTM-058 alone there). UCS-4 and
/// EBCDIC are not decoded here; those are rare enough to leave to the parse.
pub(crate) fn decode_xml(bytes: &[u8]) -> String {
    use crate::css::decode_utf16;
    match bytes {
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, true),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, false),
        [0, 0x3C, 0, 0x3F, ..] => decode_utf16(bytes, true),
        [0x3C, 0, 0x3F, 0, ..] => decode_utf16(bytes, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Report the encoding finding, if any, for one resource checked as
/// `media_type`. For every XML resource except the package document, whose
/// decode needs the same answer and reports it itself.
pub(crate) fn check(bytes: &[u8], path: &str, media_type: &str, report: &mut Report) {
    let sniffed = sniff(bytes);
    let Some((id, severity)) = sniffed.finding(media_type) else {
        return;
    };
    let (text, rule, params) = match &sniffed {
        Sniffed::Utf16 => (
            "the document is UTF-16 encoded; EPUB requires UTF-8".to_owned(),
            "xml.encoding.utf16",
            Vec::new(),
        ),
        Sniffed::Declared(name) => (
            format!(
                "the document declares encoding '{name}', which is not allowed (EPUB requires UTF-8)"
            ),
            "xml.encoding.declared_non_utf8",
            vec![name.clone()],
        ),
        other => (
            format!(
                "the document is {} encoded, which is not allowed (EPUB requires UTF-8)",
                other.name().unwrap_or_default()
            ),
            "xml.encoding.non_utf8_detected",
            other.name().into_iter().collect(),
        ),
    };
    report.push_full(
        id,
        severity,
        text,
        path,
        Position { line: 1, column: 1 },
        rule,
        params,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, be: bool, bom: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if bom {
            out.extend(if be { [0xFE, 0xFF] } else { [0xFF, 0xFE] });
        }
        for u in s.encode_utf16() {
            out.extend(if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        out
    }

    #[test]
    fn utf8_in_all_its_forms() {
        for b in [
            &b"<?xml version=\"1.0\"?><a/>"[..],
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>",
            b"<?xml version=\"1.0\" encoding='utf-8'?><a/>",
            b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a/>",
            b"<a/>",
            b"<a",
        ] {
            assert_eq!(sniff(b), Sniffed::Utf8, "{:?}", String::from_utf8_lossy(b));
        }
    }

    #[test]
    fn utf16_with_and_without_a_bom_and_by_declaration() {
        let decl = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>";
        for (be, bom) in [(true, true), (false, true), (true, false), (false, false)] {
            assert_eq!(sniff(&utf16(decl, be, bom)), Sniffed::Utf16);
        }
        assert_eq!(sniff(decl.as_bytes()), Sniffed::Utf16);
        // A UTF-32LE BOM begins with the UTF-16LE one, which is tried first.
        assert_eq!(sniff(&[0xFF, 0xFE, 0, 0, b'<', 0, 0, 0]), Sniffed::Utf16);
    }

    #[test]
    fn ucs4_and_ebcdic_by_signature() {
        assert_eq!(sniff(&[0, 0, 0xFE, 0xFF, 0, 0, 0, b'<']), Sniffed::Ucs4);
        assert_eq!(sniff(&[0, 0, 0, b'<', 0, 0, 0, b'?']), Sniffed::Ucs4);
        assert_eq!(sniff(&[b'<', 0, 0, 0, b'?', 0, 0, 0]), Sniffed::Ucs4);
        assert_eq!(sniff(&[0x4C, 0x6F, 0xA7, 0x94, 0x40]), Sniffed::Ebcdic);
        assert_eq!(Sniffed::Ucs4.name().as_deref(), Some("UCS-4"));
    }

    #[test]
    fn a_declaration_is_the_literal_encoding_equals_with_a_quote() {
        let d = |s: &str| sniff(s.as_bytes());
        assert_eq!(
            d("<?xml version=\"1.0\" encoding=\"iso-8859-1\"?><a/>"),
            Sniffed::Declared("iso-8859-1".into())
        );
        // epubcheck compares with "UTF-8" exactly, so its alias is not UTF-8.
        assert_eq!(
            d("<?xml version=\"1.0\" encoding=\"utf8\"?><a/>"),
            Sniffed::Declared("utf8".into())
        );
        assert_eq!(
            d("<?xml version=\"1.0\" encoding=\"UTF-16LE\"?><a/>"),
            Sniffed::Declared("UTF-16LE".into())
        );
        // No declaration as far as the sniffer is concerned.
        assert_eq!(
            d("<?xml version=\"1.0\" encoding = \"iso-8859-1\"?><a/>"),
            Sniffed::Utf8
        );
        assert_eq!(
            d("<?xml version=\"1.0\" encoding=iso-8859-1?><a/>"),
            Sniffed::Utf8
        );
        // The search stops at the first non-ASCII byte.
        assert_eq!(d("<!--é--><?xml encoding=\"iso-8859-1\"?>"), Sniffed::Utf8);
        // And at 256 bytes.
        let late = format!("<!--{}--><?xml encoding=\"iso-8859-1\"?>", " ".repeat(260));
        assert_eq!(d(&late), Sniffed::Utf8);
        assert_eq!(
            Sniffed::Declared("iso-8859-1".into()).name().as_deref(),
            Some("ISO-8859-1")
        );
    }

    #[test]
    fn the_finding_depends_on_the_media_type_only_for_utf16() {
        assert_eq!(Sniffed::Utf8.finding("application/xhtml+xml"), None);
        assert_eq!(
            Sniffed::Utf16.finding("application/xhtml+xml"),
            Some((HTM_058, Severity::Error))
        );
        for mt in [
            "image/svg+xml",
            "text/html",
            "application/xml",
            "application/x-dtbncx+xml",
        ] {
            assert_eq!(
                Sniffed::Utf16.finding(mt),
                Some((RSC_027, Severity::Warning))
            );
        }
        for s in [
            Sniffed::Ucs4,
            Sniffed::Ebcdic,
            Sniffed::Declared("x".into()),
        ] {
            assert_eq!(
                s.finding("application/xhtml+xml"),
                Some((RSC_028, Severity::Error))
            );
            assert_eq!(s.finding("image/svg+xml"), Some((RSC_028, Severity::Error)));
        }
    }
}
