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

use crate::ids::{HTM_058, RSC_016, RSC_027, RSC_028};
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
    match declared_name(ascii_header(head)) {
        None => Sniffed::Utf8,
        Some(name) if name.eq_ignore_ascii_case("UTF-8") => Sniffed::Utf8,
        Some(name) if name.eq_ignore_ascii_case("UTF-16") => Sniffed::Utf16,
        Some(name) => Sniffed::Declared(name.to_owned()),
    }
}

/// Whether epubcheck's XML parser (Xerces) takes the encoding a resource
/// declares, or stops at its declaration with a fatal RSC-016.
///
/// It only matters when the sniff is not UTF-8: then epubcheck hands Xerces
/// the bytes and Xerces reads the declaration itself, while on the UTF-8
/// branch it hands over decoded characters and the declared name is never
/// looked up. Xerces takes a name, case-blind, only if it is one of the
/// [`PARSER_ACCEPTED`] names; anything else is fatal, Java's own aliases
/// (`SJIS`, `Cp1252`, `UTF8`) and `UTF-32` included, and so is a UTF-16 or
/// EBCDIC name declared over bytes that are neither.
///
/// A UTF-16, UCS-4 or EBCDIC resource found by its byte signature is treated
/// as accepted here; a declaration that contradicts the signature is a
/// separate question this does not answer.
pub fn parser_accepts(bytes: &[u8]) -> bool {
    match sniff(bytes) {
        Sniffed::Utf8 | Sniffed::Ucs4 | Sniffed::Ebcdic => true,
        // Only a declaration over 8-bit bytes reaches the table.
        Sniffed::Utf16 => {
            let head = &bytes[..bytes.len().min(SNIFF_LEN)];
            UTF16_MAGIC.iter().any(|m| head.starts_with(m))
        }
        Sniffed::Declared(name) => PARSER_ACCEPTED
            .binary_search(&name.to_ascii_uppercase().as_str())
            .is_ok(),
    }
}

/// The encoding names Xerces 2.12.2 (the parser in epubcheck 5.4.0) accepts
/// in a declaration over 8-bit bytes, upper-cased and sorted.
///
/// **Measured, not derived.** Each of the 341 names in Xerces'
/// `EncodingMap.fIANA2JavaMap` was declared in an otherwise valid EPUB 3
/// package document and run through epubcheck 5.4.0 (2026-10-14): these 191
/// drew no RSC-016, the other 150 a fatal RSC-016 as well. Five of the 191
/// (`CCSID00924`, `CP00924`, `EBCDIC-LATIN9--EURO`, `IBM-924`, `IBM00924`)
/// drew a fatal PKG-008 instead: Xerces takes them and maps them to `CP924`,
/// which that JVM (OpenJDK 26) does not have. That is the JVM's answer, not
/// the parser's, so they stay here and draw RSC-028 alone. The 150 are
/// not a family a rule could name: the UTF-16 names and the EBCDIC code pages
/// are among them, but so are `437`, `ISO_646.IRV:1991` and the JIS X 0208 /
/// 0212 names, while `CP437`, `US-ASCII` and `SHIFT_JIS` are accepted. Names
/// outside the map (`SJIS`, `CP1252`, `UTF8`, `UTF-32`) were fatal in every
/// probe. Re-measure when epubcheck moves to another Xerces.
pub const PARSER_ACCEPTED: [&str; 191] = [
    "ANSI_X3.4-1968",
    "ANSI_X3.4-1986",
    "ARABIC",
    "ASCII",
    "ASMO-708",
    "BIG5",
    "CCSID00858",
    "CCSID00924",
    "CP-AR",
    "CP-GR",
    "CP-IS",
    "CP00858",
    "CP00924",
    "CP367",
    "CP437",
    "CP775",
    "CP819",
    "CP850",
    "CP852",
    "CP855",
    "CP857",
    "CP860",
    "CP861",
    "CP862",
    "CP863",
    "CP864",
    "CP865",
    "CP866",
    "CP868",
    "CP869",
    "CP936",
    "CSASCII",
    "CSBIG5",
    "CSEUCKR",
    "CSEUCPKDFMTJAPANESE",
    "CSGB2312",
    "CSIBM855",
    "CSIBM857",
    "CSIBM860",
    "CSIBM861",
    "CSIBM863",
    "CSIBM864",
    "CSIBM865",
    "CSIBM866",
    "CSIBM868",
    "CSIBM869",
    "CSISO13JISC6220JP",
    "CSISO2022JP",
    "CSISO2022KR",
    "CSISOLATIN1",
    "CSISOLATIN2",
    "CSISOLATIN3",
    "CSISOLATIN4",
    "CSISOLATIN5",
    "CSISOLATINARABIC",
    "CSISOLATINCYRILLIC",
    "CSISOLATINGREEK",
    "CSISOLATINHEBREW",
    "CSKOI8R",
    "CSKSC56011987",
    "CSPC775BALTIC",
    "CSPC850MULTILINGUAL",
    "CSPC862LATINHEBREW",
    "CSPC8CODEPAGE437",
    "CSPCP852",
    "CSSHIFTJIS",
    "CSWINDOWS31J",
    "CYRILLIC",
    "EBCDIC-LATIN9--EURO",
    "ECMA-114",
    "ECMA-118",
    "ELOT_928",
    "EUC-JP",
    "EUC-KR",
    "EXTENDED_UNIX_CODE_PACKED_FORMAT_FOR_JAPANESE",
    "GB18030",
    "GB2312",
    "GBK",
    "GREEK",
    "GREEK8",
    "HEBREW",
    "IBM-367",
    "IBM-437",
    "IBM-775",
    "IBM-819",
    "IBM-850",
    "IBM-852",
    "IBM-855",
    "IBM-857",
    "IBM-858",
    "IBM-860",
    "IBM-861",
    "IBM-862",
    "IBM-863",
    "IBM-864",
    "IBM-865",
    "IBM-866",
    "IBM-868",
    "IBM-869",
    "IBM-924",
    "IBM00858",
    "IBM00924",
    "IBM367",
    "IBM437",
    "IBM775",
    "IBM819",
    "IBM850",
    "IBM852",
    "IBM855",
    "IBM857",
    "IBM860",
    "IBM861",
    "IBM862",
    "IBM863",
    "IBM864",
    "IBM865",
    "IBM866",
    "IBM868",
    "IBM869",
    "ISO-2022-CN",
    "ISO-2022-JP",
    "ISO-2022-KR",
    "ISO-8859-1",
    "ISO-8859-13",
    "ISO-8859-15",
    "ISO-8859-2",
    "ISO-8859-3",
    "ISO-8859-4",
    "ISO-8859-5",
    "ISO-8859-6",
    "ISO-8859-7",
    "ISO-8859-8",
    "ISO-8859-8-I",
    "ISO-8859-9",
    "ISO-IR-100",
    "ISO-IR-101",
    "ISO-IR-109",
    "ISO-IR-110",
    "ISO-IR-126",
    "ISO-IR-127",
    "ISO-IR-138",
    "ISO-IR-144",
    "ISO-IR-148",
    "ISO-IR-149",
    "ISO-IR-6",
    "ISO646-US",
    "ISO_8859-1",
    "ISO_8859-15",
    "ISO_8859-2",
    "ISO_8859-3",
    "ISO_8859-4",
    "ISO_8859-5",
    "ISO_8859-6",
    "ISO_8859-7",
    "ISO_8859-8",
    "ISO_8859-9",
    "KOI8-R",
    "KOREAN",
    "KSC_5601",
    "KS_C_5601-1987",
    "KS_C_5601-1989",
    "L1",
    "L2",
    "L3",
    "L4",
    "L5",
    "LATIN-9",
    "LATIN1",
    "LATIN2",
    "LATIN3",
    "LATIN4",
    "LATIN5",
    "MS936",
    "MS_KANJI",
    "SHIFT_JIS",
    "TIS-620",
    "US",
    "US-ASCII",
    "UTF-8",
    "WINDOWS-1250",
    "WINDOWS-1251",
    "WINDOWS-1252",
    "WINDOWS-1253",
    "WINDOWS-1254",
    "WINDOWS-1255",
    "WINDOWS-1256",
    "WINDOWS-1257",
    "WINDOWS-1258",
    "WINDOWS-31J",
    "WINDOWS-936",
    "X0201",
];

/// The leading ASCII bytes of `head`, which is where the sniffer looks for a
/// declaration.
fn ascii_header(head: &[u8]) -> &str {
    let ascii_len = head
        .iter()
        .position(|&b| b == 0 || b > 0x7F)
        .unwrap_or(head.len());
    // All ASCII, so the conversion cannot fail.
    std::str::from_utf8(&head[..ascii_len]).unwrap_or("")
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

/// UTF-16 code units in either byte order, unpaired surrogates as U+FFFD.
/// XML's decoder (see [`decode_xml`]) and the package document's; stylesheets
/// decode through styloria.
pub(crate) fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    // `as_chunks` rather than `chunks_exact(2)`: it hands back fixed-size
    // arrays, so the element accesses below are checked at compile time instead
    // of indexing a slice whose length only the constant argument guarantees.
    // Same trailing behaviour — a stray odd byte is dropped, which is what a
    // truncated UTF-16 stream deserves. (clippy::chunks_exact_to_as_chunks,
    // which arrived with Rust 1.98; `as_chunks` is stable from 1.88, our MSRV.)
    let units = bytes.as_chunks::<2>().0.iter().map(|c| {
        if big_endian {
            u16::from_be_bytes(*c)
        } else {
            u16::from_le_bytes(*c)
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

/// UTF-32 code units in either byte order; a value that is no character is
/// dropped. The package document's decoder and, for a stylesheet with a
/// UTF-32BE byte order mark, the CSS one.
pub(crate) fn decode_utf32(bytes: &[u8], big_endian: bool) -> String {
    // `as_chunks` rather than `chunks_exact(4)` — see the note on
    // [`decode_utf16`]; same reasoning, same trailing behaviour.
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter_map(|c| {
            let v = if big_endian {
                u32::from_be_bytes(*c)
            } else {
                u32::from_le_bytes(*c)
            };
            char::from_u32(v)
        })
        .collect()
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
    match bytes {
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, true),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, false),
        [0, 0x3C, 0, 0x3F, ..] => decode_utf16(bytes, true),
        [0x3C, 0, 0x3F, 0, ..] => decode_utf16(bytes, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Report the encoding finding, if any, for one resource checked as
/// `media_type`, and the fatal RSC-016 when the parser would not take the
/// declared encoding (see [`parser_accepts`]). For every XML resource except
/// the package document, whose decode needs the same answer and reports it
/// itself.
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
    if !parser_accepts(bytes) {
        let declared = match &sniffed {
            Sniffed::Declared(name) => name.clone(),
            _ => "UTF-16".to_owned(),
        };
        report.push_full(
            RSC_016,
            Severity::Fatal,
            format!("the XML parser does not support the declared encoding '{declared}'"),
            path,
            Position { line: 1, column: 1 },
            "xml.encoding.unsupported",
            vec![declared],
        );
    }
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
    fn the_parser_takes_the_measured_names_and_nothing_else() {
        let decl = |n: &str| format!("<?xml version=\"1.0\" encoding=\"{n}\"?><a/>");
        for n in [
            "Shift_JIS",
            "windows-1254",
            "latin1",
            "l1",
            "csShiftJIS",
            "us-ascii",
            "UTF-8",
        ] {
            assert!(parser_accepts(decl(n).as_bytes()), "{n}");
        }
        for n in [
            "SJIS", "cp1252", "utf8", "UTF-32", "UTF-16", "UTF-16LE", "IBM037", "437", "UTF-8 ",
        ] {
            assert!(!parser_accepts(decl(n).as_bytes()), "{n}");
        }
        // UTF-16 found by its bytes is not a declaration question.
        assert!(parser_accepts(&utf16(&decl("UTF-16"), true, true)));
        assert!(parser_accepts(&utf16(&decl("UTF-16"), false, false)));
        // On the UTF-8 branch the name is never looked up.
        assert!(parser_accepts(
            b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"SJIS\"?><a/>"
        ));
        assert!(parser_accepts(
            b"<?xml version=\"1.0\" encoding = \"SJIS\"?><a/>"
        ));
        assert!(
            PARSER_ACCEPTED.windows(2).all(|w| w[0] < w[1]),
            "binary search needs order"
        );
        assert!(PARSER_ACCEPTED.iter().all(|n| *n == n.to_ascii_uppercase()));
    }

    // --- Measured against epubcheck 5.4.0 (2026-10-14) ----------------------
    //
    // Everything below is a recorded epubcheck answer, so `cargo test` keeps
    // what took a JVM to learn. The books are byte-for-byte the probes that
    // were run (veripublica/epubveri/scratchpad/enc-probe/make.py); the method
    // is in docs/DECISIONS.md, "Encoding: one sniffer, and a measured parser
    // table". Where epubcheck reports the OPF's or container.xml's encoding
    // twice (it parses them twice), the expectation lists it once.

    const MEASURED_REJECTED: [&str; 150] = [
        "437",
        "850",
        "852",
        "855",
        "857",
        "860",
        "861",
        "862",
        "863",
        "865",
        "866",
        "CCSID01140",
        "CCSID01141",
        "CCSID01142",
        "CCSID01143",
        "CCSID01144",
        "CCSID01145",
        "CCSID01146",
        "CCSID01147",
        "CCSID01148",
        "CCSID01149",
        "CP01140",
        "CP01141",
        "CP01142",
        "CP01143",
        "CP01144",
        "CP01145",
        "CP01146",
        "CP01147",
        "CP01148",
        "CP01149",
        "CP037",
        "CP1026",
        "CP1047",
        "CP273",
        "CP277",
        "CP278",
        "CP280",
        "CP284",
        "CP285",
        "CP290",
        "CP297",
        "CP420",
        "CP424",
        "CP500",
        "CP870",
        "CP871",
        "CP918",
        "CSIBM037",
        "CSIBM1026",
        "CSIBM273",
        "CSIBM277",
        "CSIBM278",
        "CSIBM280",
        "CSIBM284",
        "CSIBM285",
        "CSIBM290",
        "CSIBM297",
        "CSIBM420",
        "CSIBM424",
        "CSIBM500",
        "CSIBM870",
        "CSIBM871",
        "CSIBM918",
        "CSISO159JISX02121990",
        "CSISO87JISX0208",
        "EBCDIC-CP-AR1",
        "EBCDIC-CP-AR2",
        "EBCDIC-CP-BE",
        "EBCDIC-CP-CA",
        "EBCDIC-CP-CH",
        "EBCDIC-CP-DK",
        "EBCDIC-CP-ES",
        "EBCDIC-CP-FI",
        "EBCDIC-CP-FR",
        "EBCDIC-CP-GB",
        "EBCDIC-CP-HE",
        "EBCDIC-CP-IS",
        "EBCDIC-CP-IT",
        "EBCDIC-CP-NL",
        "EBCDIC-CP-NO",
        "EBCDIC-CP-ROECE",
        "EBCDIC-CP-SE",
        "EBCDIC-CP-US",
        "EBCDIC-CP-WT",
        "EBCDIC-CP-YU",
        "EBCDIC-JP-KANA",
        "IBM-1026",
        "IBM-1047",
        "IBM-1140",
        "IBM-1141",
        "IBM-1142",
        "IBM-1143",
        "IBM-1144",
        "IBM-1145",
        "IBM-1146",
        "IBM-1147",
        "IBM-1148",
        "IBM-1149",
        "IBM-273",
        "IBM-277",
        "IBM-278",
        "IBM-280",
        "IBM-284",
        "IBM-285",
        "IBM-290",
        "IBM-297",
        "IBM-37",
        "IBM-420",
        "IBM-424",
        "IBM-500",
        "IBM-870",
        "IBM-871",
        "IBM-918",
        "IBM01140",
        "IBM01141",
        "IBM01142",
        "IBM01143",
        "IBM01144",
        "IBM01145",
        "IBM01146",
        "IBM01147",
        "IBM01148",
        "IBM01149",
        "IBM037",
        "IBM1026",
        "IBM1047",
        "IBM273",
        "IBM277",
        "IBM278",
        "IBM280",
        "IBM284",
        "IBM285",
        "IBM290",
        "IBM297",
        "IBM420",
        "IBM424",
        "IBM500",
        "IBM870",
        "IBM871",
        "IBM918",
        "ISO-IR-159",
        "ISO-IR-87",
        "ISO_646.IRV:1991",
        "UTF-16",
        "UTF-16BE",
        "UTF-16LE",
        "X0208",
        "X0208dbiJIS_X0208-1983",
        "X0212",
    ];

    #[test]
    fn the_341_names_split_as_measured() {
        let decl = |n: &str| format!("<?xml version=\"1.0\" encoding=\"{n}\"?><a/>");
        assert_eq!(PARSER_ACCEPTED.len() + MEASURED_REJECTED.len(), 341);
        for n in MEASURED_REJECTED {
            assert!(!PARSER_ACCEPTED.contains(&n), "{n} is in both lists");
            assert!(
                !parser_accepts(decl(n).as_bytes()),
                "{n} was fatal in epubcheck"
            );
            assert!(
                !parser_accepts(decl(&n.to_ascii_lowercase()).as_bytes()),
                "{n}, lower-cased"
            );
        }
        for n in PARSER_ACCEPTED {
            assert!(
                parser_accepts(decl(n).as_bytes()),
                "{n} drew no RSC-016 in epubcheck"
            );
            assert!(
                parser_accepts(decl(&n.to_ascii_lowercase()).as_bytes()),
                "{n}, lower-cased"
            );
        }
    }

    const CONTAINER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";
    const OPF3: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language><meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta></metadata>\n<manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"c1\" href=\"c1.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"img\" href=\"i.svg\" media-type=\"image/svg+xml\"/></manifest><spine><itemref idref=\"c1\"/></spine></package>\n";
    const NAV: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>N</title></head><body><nav epub:type=\"toc\"><ol><li><a href=\"c1.xhtml\">C</a></li></ol></nav></body></html>\n";
    const SVG: &str = "<?xml version=\"1.0\" encoding=\"{enc}\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"5\" height=\"5\"/></svg>\n";

    fn chapter(enc: &str, text: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"{enc}\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>C</title></head><body><p>{text}</p><img src=\"i.svg\" alt=\"x\"/></body></html>\n"
        )
    }

    /// A zip entry: its name and its bytes.
    type Entry = (&'static str, Vec<u8>);

    /// The probe book, with `over` replacing (or adding) entries.
    fn probe(over: &[Entry]) -> Vec<u8> {
        use std::io::Write;
        let mut files: Vec<Entry> = vec![
            ("META-INF/container.xml", CONTAINER.into()),
            ("OEBPS/content.opf", OPF3.into()),
            ("OEBPS/nav.xhtml", NAV.into()),
            ("OEBPS/c1.xhtml", chapter("UTF-8", "hi").into_bytes()),
            ("OEBPS/i.svg", SVG.replace("{enc}", "UTF-8").into_bytes()),
        ];
        for (name, data) in over {
            match files.iter_mut().find(|(n, _)| n == name) {
                Some(slot) => slot.1 = data.clone(),
                None => files.push((name, data.clone())),
            }
        }
        let mut buf = Vec::new();
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        z.start_file("mimetype", stored).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        for (name, data) in files {
            z.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(&data).unwrap();
        }
        z.finish().unwrap();
        buf
    }

    fn ids(book: Vec<u8>) -> Vec<&'static str> {
        let mut ids: Vec<_> = crate::validate_bytes(book)
            .messages
            .iter()
            .map(|m| m.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn be16(s: &str, bom: bool) -> Vec<u8> {
        utf16(s, true, bom)
    }

    #[test]
    fn probe_books_get_epubcheck_s_ids() {
        let opf = |enc: &str| {
            OPF3.replace("encoding=\"UTF-8\"", &format!("encoding=\"{enc}\""))
                .into_bytes()
        };
        let cases: Vec<(&str, Vec<Entry>, Vec<&str>)> = vec![
            ("clean", vec![], vec![]),
            (
                "chapter Latin-1",
                vec![(
                    "OEBPS/c1.xhtml",
                    chapter("ISO-8859-1", "caf\u{e9}")
                        .chars()
                        .map(|c| c as u8)
                        .collect(),
                )],
                vec![RSC_028],
            ),
            (
                "chapter UTF-16, BOM",
                vec![("OEBPS/c1.xhtml", be16(&chapter("UTF-16", "hi"), true))],
                vec![HTM_058],
            ),
            (
                "chapter UTF-16, no BOM",
                vec![("OEBPS/c1.xhtml", be16(&chapter("UTF-16", "hi"), false))],
                vec![HTM_058],
            ),
            (
                "chapter with a stray byte",
                // A Latin-1 `é` (0xE9) in a document that declares UTF-8.
                vec![(
                    "OEBPS/c1.xhtml",
                    chapter("UTF-8", "h@i")
                        .replace('@', "\u{e9}")
                        .chars()
                        .map(|c| c as u8)
                        .collect(),
                )],
                vec![],
            ),
            (
                "chapter SJIS",
                vec![("OEBPS/c1.xhtml", chapter("SJIS", "hi").into_bytes())],
                vec!["OPF-097", RSC_016, RSC_028],
            ),
            (
                "chapter utf8",
                vec![("OEBPS/c1.xhtml", chapter("utf8", "hi").into_bytes())],
                vec!["OPF-097", RSC_016, RSC_028],
            ),
            (
                "chapter Shift_JIS",
                vec![("OEBPS/c1.xhtml", chapter("Shift_JIS", "hi").into_bytes())],
                vec![RSC_028],
            ),
            (
                "nav UTF-16",
                vec![(
                    "OEBPS/nav.xhtml",
                    be16(&NAV.replace("UTF-8", "UTF-16"), true),
                )],
                vec![HTM_058],
            ),
            (
                "SVG UTF-16",
                vec![("OEBPS/i.svg", be16(&SVG.replace("{enc}", "UTF-16"), true))],
                vec![RSC_027],
            ),
            (
                "SVG UTF-16, no BOM",
                vec![("OEBPS/i.svg", be16(&SVG.replace("{enc}", "UTF-16"), false))],
                vec![RSC_027],
            ),
            (
                "SVG Latin-1",
                vec![(
                    "OEBPS/i.svg",
                    SVG.replace("{enc}", "ISO-8859-1").into_bytes(),
                )],
                vec![RSC_028],
            ),
            (
                "container Latin-1",
                vec![(
                    "META-INF/container.xml",
                    CONTAINER.replace("UTF-8", "ISO-8859-1").into_bytes(),
                )],
                vec![RSC_028],
            ),
            (
                "container UTF-16",
                vec![(
                    "META-INF/container.xml",
                    be16(&CONTAINER.replace("UTF-8", "UTF-16"), true),
                )],
                vec![RSC_027],
            ),
            (
                "OPF Shift_JIS",
                vec![("OEBPS/content.opf", opf("Shift_JIS"))],
                vec![RSC_028],
            ),
            (
                "OPF windows-1254",
                vec![("OEBPS/content.opf", opf("windows-1254"))],
                vec![RSC_028],
            ),
            (
                "OPF utf8",
                vec![("OEBPS/content.opf", opf("utf8"))],
                vec!["OPF-001", RSC_016, RSC_028],
            ),
            (
                "OPF cp1252",
                vec![("OEBPS/content.opf", opf("cp1252"))],
                vec!["OPF-001", RSC_016, RSC_028],
            ),
            (
                "OPF UTF-16 over 8-bit bytes",
                vec![("OEBPS/content.opf", opf("UTF-16"))],
                vec!["OPF-001", RSC_016, RSC_027],
            ),
            (
                "OPF UTF-16, no BOM",
                vec![(
                    "OEBPS/content.opf",
                    be16(&OPF3.replace("UTF-8", "UTF-16"), false),
                )],
                vec![RSC_027],
            ),
            (
                "OPF with a space before =",
                vec![(
                    "OEBPS/content.opf",
                    OPF3.replace("encoding=\"UTF-8\"", "encoding = \"ISO-8859-1\"")
                        .into_bytes(),
                )],
                vec![],
            ),
        ];
        for (name, over, mut want) in cases {
            want.sort_unstable();
            assert_eq!(ids(probe(&over)), want, "{name}");
        }
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
