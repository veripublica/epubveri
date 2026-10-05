//! XML names, checked the way epubcheck checks them.
//!
//! epubcheck decides "is this an XML name" with two different libraries, and
//! they disagree, so this module has two rules.
//!
//! - **Its schemas (Jing).** Every `xsd:ID`, `IDREF`, `NCName`, `Name` and
//!   `NMTOKEN` in a grammar — EPUB 2 content-document ids, NCX ids, SVG ids,
//!   OPF ids — goes through Jing's XSD datatype library, which uses the
//!   character classes of XML 1.0 Appendix B (`BaseChar`, `Ideographic`,
//!   `CombiningChar`, `Digit`, `Extender`). That is [`is_ncname`],
//!   [`is_name`] and [`is_nmtoken`].
//! - **Its Java code (Saxon's `NameChecker.isValidNCName`).** The `prefix`
//!   attribute parser (OPF-004b) and fragment-identifier checks use XML 1.0
//!   Fifth Edition's much wider `NameStartChar`/`NameChar` ranges. That is
//!   [`is_ncname_fifth_edition`].
//!
//! The two differ in both directions. `a‿b` (U+203F) passes the Fifth Edition
//! and fails Appendix B; Appendix B stops at the BMP.
//!
//! **Neither is Rust's `char::is_alphanumeric`**, which this crate used
//! before 0.21.2. That rule rejected combining marks (a decomposed `ş` is `s`
//! followed by U+0327) and the middle dot `·`, both valid in an XML name, so an
//! EPUB 2 book with such an id failed here and passed epubcheck. It also
//! accepted `²` and other non-decimal digits, which epubcheck rejects.
//!
//! **The Appendix B tables are generated, not transcribed.** They were dumped
//! on 2026-10-05 from epubcheck 5.4.0's own `jing-20181222.jar`, by asking
//! `DatatypeLibrary("http://www.w3.org/2001/XMLSchema-datatypes")
//! .createDatatype("NCName").isValid(…)` about every code point: alone (a
//! start character) and after an `a` (a name character, minus the four XML
//! whitespace characters, which `isValid` accepts only because the type
//! collapses them away). `ID` gave the identical table, and `Name` and
//! `NMTOKEN` are the same tables plus `:`. To regenerate, run the same loop
//! against a newer jar and replace the two constants.

/// Is `c` a character the type's `collapse` whitespace facet removes?
///
/// XSD means exactly the four XML whitespace characters. Jing does not strip
/// NO-BREAK SPACE: `"a\u{a0}"` is not an `NCName` there.
fn is_xml_space(c: char) -> bool {
    crate::xmlext::is_xml_space(c)
}

fn in_table(c: char, table: &[(char, char)]) -> bool {
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// `xsd:NCName` (and `ID`, `IDREF`) as epubcheck's schemas read it: an XML
/// 1.0 Appendix B name with no colon. `s` is the value after the whitespace
/// facet; see [`is_xsd_id`] for a raw attribute value.
pub(crate) fn is_ncname(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if in_table(c, XML10_NAME_START) => it.all(|c| in_table(c, XML10_NAME_CHAR)),
        _ => false,
    }
}

/// `xsd:Name`: as [`is_ncname`], with `:` allowed anywhere.
pub(crate) fn is_name(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if c == ':' || in_table(c, XML10_NAME_START) => {
            it.all(|c| c == ':' || in_table(c, XML10_NAME_CHAR))
        }
        _ => false,
    }
}

/// `xsd:NMTOKEN`: one or more name characters, `:` included.
pub(crate) fn is_nmtoken(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c == ':' || in_table(c, XML10_NAME_CHAR))
}

/// A raw attribute value typed `xsd:ID` in epubcheck's grammar: the
/// `collapse` facet (XML whitespace only) and then [`is_ncname`]. `" a "`
/// is a valid ID there; `"a\u{a0}"` is not.
pub(crate) fn is_xsd_id(raw: &str) -> bool {
    is_ncname(raw.trim_matches(is_xml_space))
}

/// An NCName as Saxon's `NameChecker.isValidNCName` reads it: XML 1.0 Fifth
/// Edition `NameStartChar`/`NameChar`, no colon, no whitespace handling.
pub(crate) fn is_ncname_fifth_edition(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if is_name_start_fifth(c) => it.all(is_name_char_fifth),
        _ => false,
    }
}

fn is_name_start_fifth(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char_fifth(c: char) -> bool {
    is_name_start_fifth(c)
        || matches!(c,
            '-' | '.' | '0'..='9' | '\u{B7}'
            | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Generated - see the module comment. `(first, last)`, inclusive, sorted.
const XML10_NAME_START: &[(char, char)] = &[
    ('\u{41}', '\u{5A}'),
    ('\u{5F}', '\u{5F}'),
    ('\u{61}', '\u{7A}'),
    ('\u{C0}', '\u{D6}'),
    ('\u{D8}', '\u{F6}'),
    ('\u{F8}', '\u{131}'),
    ('\u{134}', '\u{13E}'),
    ('\u{141}', '\u{148}'),
    ('\u{14A}', '\u{17E}'),
    ('\u{180}', '\u{1C3}'),
    ('\u{1CD}', '\u{1F0}'),
    ('\u{1F4}', '\u{1F5}'),
    ('\u{1FA}', '\u{217}'),
    ('\u{250}', '\u{2A8}'),
    ('\u{2BB}', '\u{2C1}'),
    ('\u{386}', '\u{386}'),
    ('\u{388}', '\u{38A}'),
    ('\u{38C}', '\u{38C}'),
    ('\u{38E}', '\u{3A1}'),
    ('\u{3A3}', '\u{3CE}'),
    ('\u{3D0}', '\u{3D6}'),
    ('\u{3DA}', '\u{3DA}'),
    ('\u{3DC}', '\u{3DC}'),
    ('\u{3DE}', '\u{3DE}'),
    ('\u{3E0}', '\u{3E0}'),
    ('\u{3E2}', '\u{3F3}'),
    ('\u{401}', '\u{40C}'),
    ('\u{40E}', '\u{44F}'),
    ('\u{451}', '\u{45C}'),
    ('\u{45E}', '\u{481}'),
    ('\u{490}', '\u{4C4}'),
    ('\u{4C7}', '\u{4C8}'),
    ('\u{4CB}', '\u{4CC}'),
    ('\u{4D0}', '\u{4EB}'),
    ('\u{4EE}', '\u{4F5}'),
    ('\u{4F8}', '\u{4F9}'),
    ('\u{531}', '\u{556}'),
    ('\u{559}', '\u{559}'),
    ('\u{561}', '\u{586}'),
    ('\u{5D0}', '\u{5EA}'),
    ('\u{5F0}', '\u{5F2}'),
    ('\u{621}', '\u{63A}'),
    ('\u{641}', '\u{64A}'),
    ('\u{671}', '\u{6B7}'),
    ('\u{6BA}', '\u{6BE}'),
    ('\u{6C0}', '\u{6CE}'),
    ('\u{6D0}', '\u{6D3}'),
    ('\u{6D5}', '\u{6D5}'),
    ('\u{6E5}', '\u{6E6}'),
    ('\u{905}', '\u{939}'),
    ('\u{93D}', '\u{93D}'),
    ('\u{958}', '\u{961}'),
    ('\u{985}', '\u{98C}'),
    ('\u{98F}', '\u{990}'),
    ('\u{993}', '\u{9A8}'),
    ('\u{9AA}', '\u{9B0}'),
    ('\u{9B2}', '\u{9B2}'),
    ('\u{9B6}', '\u{9B9}'),
    ('\u{9DC}', '\u{9DD}'),
    ('\u{9DF}', '\u{9E1}'),
    ('\u{9F0}', '\u{9F1}'),
    ('\u{A05}', '\u{A0A}'),
    ('\u{A0F}', '\u{A10}'),
    ('\u{A13}', '\u{A28}'),
    ('\u{A2A}', '\u{A30}'),
    ('\u{A32}', '\u{A33}'),
    ('\u{A35}', '\u{A36}'),
    ('\u{A38}', '\u{A39}'),
    ('\u{A59}', '\u{A5C}'),
    ('\u{A5E}', '\u{A5E}'),
    ('\u{A72}', '\u{A74}'),
    ('\u{A85}', '\u{A8B}'),
    ('\u{A8D}', '\u{A8D}'),
    ('\u{A8F}', '\u{A91}'),
    ('\u{A93}', '\u{AA8}'),
    ('\u{AAA}', '\u{AB0}'),
    ('\u{AB2}', '\u{AB3}'),
    ('\u{AB5}', '\u{AB9}'),
    ('\u{ABD}', '\u{ABD}'),
    ('\u{AE0}', '\u{AE0}'),
    ('\u{B05}', '\u{B0C}'),
    ('\u{B0F}', '\u{B10}'),
    ('\u{B13}', '\u{B28}'),
    ('\u{B2A}', '\u{B30}'),
    ('\u{B32}', '\u{B33}'),
    ('\u{B36}', '\u{B39}'),
    ('\u{B3D}', '\u{B3D}'),
    ('\u{B5C}', '\u{B5D}'),
    ('\u{B5F}', '\u{B61}'),
    ('\u{B85}', '\u{B8A}'),
    ('\u{B8E}', '\u{B90}'),
    ('\u{B92}', '\u{B95}'),
    ('\u{B99}', '\u{B9A}'),
    ('\u{B9C}', '\u{B9C}'),
    ('\u{B9E}', '\u{B9F}'),
    ('\u{BA3}', '\u{BA4}'),
    ('\u{BA8}', '\u{BAA}'),
    ('\u{BAE}', '\u{BB5}'),
    ('\u{BB7}', '\u{BB9}'),
    ('\u{C05}', '\u{C0C}'),
    ('\u{C0E}', '\u{C10}'),
    ('\u{C12}', '\u{C28}'),
    ('\u{C2A}', '\u{C33}'),
    ('\u{C35}', '\u{C39}'),
    ('\u{C60}', '\u{C61}'),
    ('\u{C85}', '\u{C8C}'),
    ('\u{C8E}', '\u{C90}'),
    ('\u{C92}', '\u{CA8}'),
    ('\u{CAA}', '\u{CB3}'),
    ('\u{CB5}', '\u{CB9}'),
    ('\u{CDE}', '\u{CDE}'),
    ('\u{CE0}', '\u{CE1}'),
    ('\u{D05}', '\u{D0C}'),
    ('\u{D0E}', '\u{D10}'),
    ('\u{D12}', '\u{D28}'),
    ('\u{D2A}', '\u{D39}'),
    ('\u{D60}', '\u{D61}'),
    ('\u{E01}', '\u{E2E}'),
    ('\u{E30}', '\u{E30}'),
    ('\u{E32}', '\u{E33}'),
    ('\u{E40}', '\u{E45}'),
    ('\u{E81}', '\u{E82}'),
    ('\u{E84}', '\u{E84}'),
    ('\u{E87}', '\u{E88}'),
    ('\u{E8A}', '\u{E8A}'),
    ('\u{E8D}', '\u{E8D}'),
    ('\u{E94}', '\u{E97}'),
    ('\u{E99}', '\u{E9F}'),
    ('\u{EA1}', '\u{EA3}'),
    ('\u{EA5}', '\u{EA5}'),
    ('\u{EA7}', '\u{EA7}'),
    ('\u{EAA}', '\u{EAB}'),
    ('\u{EAD}', '\u{EAE}'),
    ('\u{EB0}', '\u{EB0}'),
    ('\u{EB2}', '\u{EB3}'),
    ('\u{EBD}', '\u{EBD}'),
    ('\u{EC0}', '\u{EC4}'),
    ('\u{F40}', '\u{F47}'),
    ('\u{F49}', '\u{F69}'),
    ('\u{10A0}', '\u{10C5}'),
    ('\u{10D0}', '\u{10F6}'),
    ('\u{1100}', '\u{1100}'),
    ('\u{1102}', '\u{1103}'),
    ('\u{1105}', '\u{1107}'),
    ('\u{1109}', '\u{1109}'),
    ('\u{110B}', '\u{110C}'),
    ('\u{110E}', '\u{1112}'),
    ('\u{113C}', '\u{113C}'),
    ('\u{113E}', '\u{113E}'),
    ('\u{1140}', '\u{1140}'),
    ('\u{114C}', '\u{114C}'),
    ('\u{114E}', '\u{114E}'),
    ('\u{1150}', '\u{1150}'),
    ('\u{1154}', '\u{1155}'),
    ('\u{1159}', '\u{1159}'),
    ('\u{115F}', '\u{1161}'),
    ('\u{1163}', '\u{1163}'),
    ('\u{1165}', '\u{1165}'),
    ('\u{1167}', '\u{1167}'),
    ('\u{1169}', '\u{1169}'),
    ('\u{116D}', '\u{116E}'),
    ('\u{1172}', '\u{1173}'),
    ('\u{1175}', '\u{1175}'),
    ('\u{119E}', '\u{119E}'),
    ('\u{11A8}', '\u{11A8}'),
    ('\u{11AB}', '\u{11AB}'),
    ('\u{11AE}', '\u{11AF}'),
    ('\u{11B7}', '\u{11B8}'),
    ('\u{11BA}', '\u{11BA}'),
    ('\u{11BC}', '\u{11C2}'),
    ('\u{11EB}', '\u{11EB}'),
    ('\u{11F0}', '\u{11F0}'),
    ('\u{11F9}', '\u{11F9}'),
    ('\u{1E00}', '\u{1E9B}'),
    ('\u{1EA0}', '\u{1EF9}'),
    ('\u{1F00}', '\u{1F15}'),
    ('\u{1F18}', '\u{1F1D}'),
    ('\u{1F20}', '\u{1F45}'),
    ('\u{1F48}', '\u{1F4D}'),
    ('\u{1F50}', '\u{1F57}'),
    ('\u{1F59}', '\u{1F59}'),
    ('\u{1F5B}', '\u{1F5B}'),
    ('\u{1F5D}', '\u{1F5D}'),
    ('\u{1F5F}', '\u{1F7D}'),
    ('\u{1F80}', '\u{1FB4}'),
    ('\u{1FB6}', '\u{1FBC}'),
    ('\u{1FBE}', '\u{1FBE}'),
    ('\u{1FC2}', '\u{1FC4}'),
    ('\u{1FC6}', '\u{1FCC}'),
    ('\u{1FD0}', '\u{1FD3}'),
    ('\u{1FD6}', '\u{1FDB}'),
    ('\u{1FE0}', '\u{1FEC}'),
    ('\u{1FF2}', '\u{1FF4}'),
    ('\u{1FF6}', '\u{1FFC}'),
    ('\u{2126}', '\u{2126}'),
    ('\u{212A}', '\u{212B}'),
    ('\u{212E}', '\u{212E}'),
    ('\u{2180}', '\u{2182}'),
    ('\u{3007}', '\u{3007}'),
    ('\u{3021}', '\u{3029}'),
    ('\u{3041}', '\u{3094}'),
    ('\u{30A1}', '\u{30FA}'),
    ('\u{3105}', '\u{312C}'),
    ('\u{4E00}', '\u{9FA5}'),
    ('\u{AC00}', '\u{D7A3}'),
];

/// Generated - see the module comment. `(first, last)`, inclusive, sorted.
const XML10_NAME_CHAR: &[(char, char)] = &[
    ('\u{2D}', '\u{2E}'),
    ('\u{30}', '\u{39}'),
    ('\u{41}', '\u{5A}'),
    ('\u{5F}', '\u{5F}'),
    ('\u{61}', '\u{7A}'),
    ('\u{B7}', '\u{B7}'),
    ('\u{C0}', '\u{D6}'),
    ('\u{D8}', '\u{F6}'),
    ('\u{F8}', '\u{131}'),
    ('\u{134}', '\u{13E}'),
    ('\u{141}', '\u{148}'),
    ('\u{14A}', '\u{17E}'),
    ('\u{180}', '\u{1C3}'),
    ('\u{1CD}', '\u{1F0}'),
    ('\u{1F4}', '\u{1F5}'),
    ('\u{1FA}', '\u{217}'),
    ('\u{250}', '\u{2A8}'),
    ('\u{2BB}', '\u{2C1}'),
    ('\u{2D0}', '\u{2D1}'),
    ('\u{300}', '\u{345}'),
    ('\u{360}', '\u{361}'),
    ('\u{386}', '\u{38A}'),
    ('\u{38C}', '\u{38C}'),
    ('\u{38E}', '\u{3A1}'),
    ('\u{3A3}', '\u{3CE}'),
    ('\u{3D0}', '\u{3D6}'),
    ('\u{3DA}', '\u{3DA}'),
    ('\u{3DC}', '\u{3DC}'),
    ('\u{3DE}', '\u{3DE}'),
    ('\u{3E0}', '\u{3E0}'),
    ('\u{3E2}', '\u{3F3}'),
    ('\u{401}', '\u{40C}'),
    ('\u{40E}', '\u{44F}'),
    ('\u{451}', '\u{45C}'),
    ('\u{45E}', '\u{481}'),
    ('\u{483}', '\u{486}'),
    ('\u{490}', '\u{4C4}'),
    ('\u{4C7}', '\u{4C8}'),
    ('\u{4CB}', '\u{4CC}'),
    ('\u{4D0}', '\u{4EB}'),
    ('\u{4EE}', '\u{4F5}'),
    ('\u{4F8}', '\u{4F9}'),
    ('\u{531}', '\u{556}'),
    ('\u{559}', '\u{559}'),
    ('\u{561}', '\u{586}'),
    ('\u{591}', '\u{5A1}'),
    ('\u{5A3}', '\u{5B9}'),
    ('\u{5BB}', '\u{5BD}'),
    ('\u{5BF}', '\u{5BF}'),
    ('\u{5C1}', '\u{5C2}'),
    ('\u{5C4}', '\u{5C4}'),
    ('\u{5D0}', '\u{5EA}'),
    ('\u{5F0}', '\u{5F2}'),
    ('\u{621}', '\u{63A}'),
    ('\u{640}', '\u{652}'),
    ('\u{660}', '\u{669}'),
    ('\u{670}', '\u{6B7}'),
    ('\u{6BA}', '\u{6BE}'),
    ('\u{6C0}', '\u{6CE}'),
    ('\u{6D0}', '\u{6D3}'),
    ('\u{6D5}', '\u{6E8}'),
    ('\u{6EA}', '\u{6ED}'),
    ('\u{6F0}', '\u{6F9}'),
    ('\u{901}', '\u{903}'),
    ('\u{905}', '\u{939}'),
    ('\u{93C}', '\u{94D}'),
    ('\u{951}', '\u{954}'),
    ('\u{958}', '\u{963}'),
    ('\u{966}', '\u{96F}'),
    ('\u{981}', '\u{983}'),
    ('\u{985}', '\u{98C}'),
    ('\u{98F}', '\u{990}'),
    ('\u{993}', '\u{9A8}'),
    ('\u{9AA}', '\u{9B0}'),
    ('\u{9B2}', '\u{9B2}'),
    ('\u{9B6}', '\u{9B9}'),
    ('\u{9BC}', '\u{9BC}'),
    ('\u{9BE}', '\u{9C4}'),
    ('\u{9C7}', '\u{9C8}'),
    ('\u{9CB}', '\u{9CD}'),
    ('\u{9D7}', '\u{9D7}'),
    ('\u{9DC}', '\u{9DD}'),
    ('\u{9DF}', '\u{9E3}'),
    ('\u{9E6}', '\u{9F1}'),
    ('\u{A02}', '\u{A02}'),
    ('\u{A05}', '\u{A0A}'),
    ('\u{A0F}', '\u{A10}'),
    ('\u{A13}', '\u{A28}'),
    ('\u{A2A}', '\u{A30}'),
    ('\u{A32}', '\u{A33}'),
    ('\u{A35}', '\u{A36}'),
    ('\u{A38}', '\u{A39}'),
    ('\u{A3C}', '\u{A3C}'),
    ('\u{A3E}', '\u{A42}'),
    ('\u{A47}', '\u{A48}'),
    ('\u{A4B}', '\u{A4D}'),
    ('\u{A59}', '\u{A5C}'),
    ('\u{A5E}', '\u{A5E}'),
    ('\u{A66}', '\u{A74}'),
    ('\u{A81}', '\u{A83}'),
    ('\u{A85}', '\u{A8B}'),
    ('\u{A8D}', '\u{A8D}'),
    ('\u{A8F}', '\u{A91}'),
    ('\u{A93}', '\u{AA8}'),
    ('\u{AAA}', '\u{AB0}'),
    ('\u{AB2}', '\u{AB3}'),
    ('\u{AB5}', '\u{AB9}'),
    ('\u{ABC}', '\u{AC5}'),
    ('\u{AC7}', '\u{AC9}'),
    ('\u{ACB}', '\u{ACD}'),
    ('\u{AE0}', '\u{AE0}'),
    ('\u{AE6}', '\u{AEF}'),
    ('\u{B01}', '\u{B03}'),
    ('\u{B05}', '\u{B0C}'),
    ('\u{B0F}', '\u{B10}'),
    ('\u{B13}', '\u{B28}'),
    ('\u{B2A}', '\u{B30}'),
    ('\u{B32}', '\u{B33}'),
    ('\u{B36}', '\u{B39}'),
    ('\u{B3C}', '\u{B43}'),
    ('\u{B47}', '\u{B48}'),
    ('\u{B4B}', '\u{B4D}'),
    ('\u{B56}', '\u{B57}'),
    ('\u{B5C}', '\u{B5D}'),
    ('\u{B5F}', '\u{B61}'),
    ('\u{B66}', '\u{B6F}'),
    ('\u{B82}', '\u{B83}'),
    ('\u{B85}', '\u{B8A}'),
    ('\u{B8E}', '\u{B90}'),
    ('\u{B92}', '\u{B95}'),
    ('\u{B99}', '\u{B9A}'),
    ('\u{B9C}', '\u{B9C}'),
    ('\u{B9E}', '\u{B9F}'),
    ('\u{BA3}', '\u{BA4}'),
    ('\u{BA8}', '\u{BAA}'),
    ('\u{BAE}', '\u{BB5}'),
    ('\u{BB7}', '\u{BB9}'),
    ('\u{BBE}', '\u{BC2}'),
    ('\u{BC6}', '\u{BC8}'),
    ('\u{BCA}', '\u{BCD}'),
    ('\u{BD7}', '\u{BD7}'),
    ('\u{BE7}', '\u{BEF}'),
    ('\u{C01}', '\u{C03}'),
    ('\u{C05}', '\u{C0C}'),
    ('\u{C0E}', '\u{C10}'),
    ('\u{C12}', '\u{C28}'),
    ('\u{C2A}', '\u{C33}'),
    ('\u{C35}', '\u{C39}'),
    ('\u{C3E}', '\u{C44}'),
    ('\u{C46}', '\u{C48}'),
    ('\u{C4A}', '\u{C4D}'),
    ('\u{C55}', '\u{C56}'),
    ('\u{C60}', '\u{C61}'),
    ('\u{C66}', '\u{C6F}'),
    ('\u{C82}', '\u{C83}'),
    ('\u{C85}', '\u{C8C}'),
    ('\u{C8E}', '\u{C90}'),
    ('\u{C92}', '\u{CA8}'),
    ('\u{CAA}', '\u{CB3}'),
    ('\u{CB5}', '\u{CB9}'),
    ('\u{CBE}', '\u{CC4}'),
    ('\u{CC6}', '\u{CC8}'),
    ('\u{CCA}', '\u{CCD}'),
    ('\u{CD5}', '\u{CD6}'),
    ('\u{CDE}', '\u{CDE}'),
    ('\u{CE0}', '\u{CE1}'),
    ('\u{CE6}', '\u{CEF}'),
    ('\u{D02}', '\u{D03}'),
    ('\u{D05}', '\u{D0C}'),
    ('\u{D0E}', '\u{D10}'),
    ('\u{D12}', '\u{D28}'),
    ('\u{D2A}', '\u{D39}'),
    ('\u{D3E}', '\u{D43}'),
    ('\u{D46}', '\u{D48}'),
    ('\u{D4A}', '\u{D4D}'),
    ('\u{D57}', '\u{D57}'),
    ('\u{D60}', '\u{D61}'),
    ('\u{D66}', '\u{D6F}'),
    ('\u{E01}', '\u{E2E}'),
    ('\u{E30}', '\u{E3A}'),
    ('\u{E40}', '\u{E4E}'),
    ('\u{E50}', '\u{E59}'),
    ('\u{E81}', '\u{E82}'),
    ('\u{E84}', '\u{E84}'),
    ('\u{E87}', '\u{E88}'),
    ('\u{E8A}', '\u{E8A}'),
    ('\u{E8D}', '\u{E8D}'),
    ('\u{E94}', '\u{E97}'),
    ('\u{E99}', '\u{E9F}'),
    ('\u{EA1}', '\u{EA3}'),
    ('\u{EA5}', '\u{EA5}'),
    ('\u{EA7}', '\u{EA7}'),
    ('\u{EAA}', '\u{EAB}'),
    ('\u{EAD}', '\u{EAE}'),
    ('\u{EB0}', '\u{EB9}'),
    ('\u{EBB}', '\u{EBD}'),
    ('\u{EC0}', '\u{EC4}'),
    ('\u{EC6}', '\u{EC6}'),
    ('\u{EC8}', '\u{ECD}'),
    ('\u{ED0}', '\u{ED9}'),
    ('\u{F18}', '\u{F19}'),
    ('\u{F20}', '\u{F29}'),
    ('\u{F35}', '\u{F35}'),
    ('\u{F37}', '\u{F37}'),
    ('\u{F39}', '\u{F39}'),
    ('\u{F3E}', '\u{F47}'),
    ('\u{F49}', '\u{F69}'),
    ('\u{F71}', '\u{F84}'),
    ('\u{F86}', '\u{F8B}'),
    ('\u{F90}', '\u{F95}'),
    ('\u{F97}', '\u{F97}'),
    ('\u{F99}', '\u{FAD}'),
    ('\u{FB1}', '\u{FB7}'),
    ('\u{FB9}', '\u{FB9}'),
    ('\u{10A0}', '\u{10C5}'),
    ('\u{10D0}', '\u{10F6}'),
    ('\u{1100}', '\u{1100}'),
    ('\u{1102}', '\u{1103}'),
    ('\u{1105}', '\u{1107}'),
    ('\u{1109}', '\u{1109}'),
    ('\u{110B}', '\u{110C}'),
    ('\u{110E}', '\u{1112}'),
    ('\u{113C}', '\u{113C}'),
    ('\u{113E}', '\u{113E}'),
    ('\u{1140}', '\u{1140}'),
    ('\u{114C}', '\u{114C}'),
    ('\u{114E}', '\u{114E}'),
    ('\u{1150}', '\u{1150}'),
    ('\u{1154}', '\u{1155}'),
    ('\u{1159}', '\u{1159}'),
    ('\u{115F}', '\u{1161}'),
    ('\u{1163}', '\u{1163}'),
    ('\u{1165}', '\u{1165}'),
    ('\u{1167}', '\u{1167}'),
    ('\u{1169}', '\u{1169}'),
    ('\u{116D}', '\u{116E}'),
    ('\u{1172}', '\u{1173}'),
    ('\u{1175}', '\u{1175}'),
    ('\u{119E}', '\u{119E}'),
    ('\u{11A8}', '\u{11A8}'),
    ('\u{11AB}', '\u{11AB}'),
    ('\u{11AE}', '\u{11AF}'),
    ('\u{11B7}', '\u{11B8}'),
    ('\u{11BA}', '\u{11BA}'),
    ('\u{11BC}', '\u{11C2}'),
    ('\u{11EB}', '\u{11EB}'),
    ('\u{11F0}', '\u{11F0}'),
    ('\u{11F9}', '\u{11F9}'),
    ('\u{1E00}', '\u{1E9B}'),
    ('\u{1EA0}', '\u{1EF9}'),
    ('\u{1F00}', '\u{1F15}'),
    ('\u{1F18}', '\u{1F1D}'),
    ('\u{1F20}', '\u{1F45}'),
    ('\u{1F48}', '\u{1F4D}'),
    ('\u{1F50}', '\u{1F57}'),
    ('\u{1F59}', '\u{1F59}'),
    ('\u{1F5B}', '\u{1F5B}'),
    ('\u{1F5D}', '\u{1F5D}'),
    ('\u{1F5F}', '\u{1F7D}'),
    ('\u{1F80}', '\u{1FB4}'),
    ('\u{1FB6}', '\u{1FBC}'),
    ('\u{1FBE}', '\u{1FBE}'),
    ('\u{1FC2}', '\u{1FC4}'),
    ('\u{1FC6}', '\u{1FCC}'),
    ('\u{1FD0}', '\u{1FD3}'),
    ('\u{1FD6}', '\u{1FDB}'),
    ('\u{1FE0}', '\u{1FEC}'),
    ('\u{1FF2}', '\u{1FF4}'),
    ('\u{1FF6}', '\u{1FFC}'),
    ('\u{20D0}', '\u{20DC}'),
    ('\u{20E1}', '\u{20E1}'),
    ('\u{2126}', '\u{2126}'),
    ('\u{212A}', '\u{212B}'),
    ('\u{212E}', '\u{212E}'),
    ('\u{2180}', '\u{2182}'),
    ('\u{3005}', '\u{3005}'),
    ('\u{3007}', '\u{3007}'),
    ('\u{3021}', '\u{302F}'),
    ('\u{3031}', '\u{3035}'),
    ('\u{3041}', '\u{3094}'),
    ('\u{3099}', '\u{309A}'),
    ('\u{309D}', '\u{309E}'),
    ('\u{30A1}', '\u{30FA}'),
    ('\u{30FC}', '\u{30FE}'),
    ('\u{3105}', '\u{312C}'),
    ('\u{4E00}', '\u{9FA5}'),
    ('\u{AC00}', '\u{D7A3}'),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes probed against epubcheck 5.4.0 on whole EPUB 2 books
    /// (2026-10-05), each one id in an XHTML 1.1 content document.
    #[test]
    fn content_document_ids_as_epubcheck_reads_them() {
        for valid in [
            "plain",
            "ba\u{73}\u{327}",
            "a\u{b7}b",
            "a\u{660}",
            "_x",
            "x-1.2",
        ] {
            assert!(is_xsd_id(valid), "{valid:?} is valid in epubcheck");
        }
        for invalid in [
            "a\u{203f}b",
            "a\u{b2}",
            "1a",
            "-a",
            ".a",
            "a:b",
            "",
            "\u{b7}",
        ] {
            assert!(!is_xsd_id(invalid), "{invalid:?} is invalid in epubcheck");
        }
    }

    /// Jing's `collapse` strips XML whitespace and nothing else (probed on
    /// its datatype library directly).
    #[test]
    fn the_whitespace_facet_is_xml_whitespace_only() {
        assert!(is_xsd_id(" a "));
        assert!(is_xsd_id("\ta\n"));
        for raw in ["a\u{a0}", "\u{a0}a", "a\u{2003}", "\u{a0}", "a b"] {
            assert!(!is_xsd_id(raw), "{raw:?}");
        }
    }

    /// The table edges, as Jing's dump has them: Appendix B ends in the BMP
    /// at the Hangul syllables, and an ideograph may start a name.
    #[test]
    fn table_edges_match_the_dump() {
        assert!(is_ncname("\u{4E00}"));
        assert!(is_ncname("\u{AC00}\u{D7A3}"));
        assert!(!is_ncname("\u{D7A4}"));
        assert!(!is_ncname("\u{10000}"));
        assert!(!is_ncname("\u{132}"), "the IJ ligature is not a BaseChar");
        assert!(is_name(":a") && is_name("a:b") && !is_name("1"));
        assert!(is_nmtoken("1") && is_nmtoken("\u{b7}") && is_nmtoken(":"));
        for w in XML10_NAME_START
            .windows(2)
            .chain(XML10_NAME_CHAR.windows(2))
        {
            assert!(w[0].1 < w[1].0, "sorted and disjoint: {w:?}");
        }
    }

    /// Saxon's rule (OPF-004b, fragments) is the Fifth Edition one.
    #[test]
    fn fifth_edition_is_wider_than_appendix_b() {
        assert!(is_ncname_fifth_edition("a\u{203f}b"));
        assert!(is_ncname_fifth_edition("\u{10000}"));
        assert!(is_ncname_fifth_edition("\u{132}"));
        assert!(!is_ncname_fifth_edition(" a"));
        assert!(!is_ncname_fifth_edition("a:b"));
        assert!(!is_ncname_fifth_edition("1a"));
    }
}
