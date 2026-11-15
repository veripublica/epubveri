//! Does a reference spell its file the way the file's own URL is written?
//!
//! epubveri finds the file a reference names by percent-decoding the reference
//! and looking the result up among the container's entry names. epubcheck does
//! the same. So `Agatha%27nin.xhtml` finds `Agatha'nin.xhtml`, and nothing is
//! said. A reading system that compares the two URLs instead finds two files:
//! a Kobo counts such a chapter twice (kepubverto, MobileRead 375668 #29).
//! ADV-025 says so, behind `--advisory`, and changes no verdict.
//!
//! **Why an advisory and not an error** (owner, 2026-11-15; docs/DECISIONS.md):
//!
//! - **EPUB 3.3 § 4.2.5** reads as a rule: "all relative-URL-with-fragment
//!   strings MUST, after parsing, be equal to the content URL of an existing
//!   file", and URL equality is the URL Standard's (§ 4.6). The URL parser
//!   percent-encodes only the path percent-encode set and never decodes or
//!   changes the case of a hex digit, so `%27` for `'`, `%41` for `A` and `%c4`
//!   for `%C4` would all be unequal. But epubcheck's own test for that clause
//!   (`epub3/04-ocf/ocf.feature`, "Allow percent-encoded URLs",
//!   `@xref:sec-container-iri`) calls `content%26001.xhtml` for the file
//!   `content&001.xhtml` valid, and the editors read it the same way: in
//!   w3c/epub-specs#2077 (2022) a file named `A%2FB` has the URL `A%252FB`,
//!   "the parsing step does that, per the URL spec". It does not (the parser
//!   leaves `%` alone), but the intent is the decoding reading: a file name is
//!   data and its URL is its percent-encoding. So epubcheck's verdict stands.
//! - **EPUB 2**: OCF 2.0.1 § 3.2 names relative IRI references, and RFC 3986
//!   § 2.2 makes a URI that percent-encodes a *reserved* character a different
//!   URI. No EPUB 2 clause asks a reference to equal a file name. Hex case and
//!   encoded unreserved characters are equivalent under RFC 3986 (§ 2.1,
//!   § 2.3), so at EPUB 2 only a reserved character is reported.
//!
//! The comparison models the URL Standard's path parsing: the reference is
//! resolved against its document's directory, dot segments (`.`, `%2e`, …)
//! apply, empty segments stay, and each segment is put in the form the parser
//! serializes it to. **Unicode normalization is left out on purpose** (owner,
//! 2026-11-15): containers written on macOS hold decomposed names, and that
//! case has not been measured, so both sides are NFC-normalized first, as every
//! other lookup here does.
//!
//! The literal reading has a second half this module does not act on: a file
//! *named* `a%27b.xhtml` would be correctly referenced as `a%27b.xhtml`, which
//! both tools report as missing (RSC-001). Under the editors' reading that
//! RSC-001 is right: the reference for that file is `a%2527b.xhtml`.

use crate::report::Severity;

/// In the URL Standard's path percent-encode set: the parser writes this
/// ASCII byte as `%XX` when it finds it in a path.
fn in_path_set(b: u8) -> bool {
    !(0x20..=0x7e).contains(&b)
        || matches!(
            b,
            b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'^' | b'`' | b'{' | b'}'
        )
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// One path segment as the URL parser serializes it, put in a form two
/// spellings of the same URL share **except for Unicode normalization**,
/// which is left out (see the module note).
///
/// The parser encodes each byte of the path percent-encode set as `%XX` in
/// uppercase and leaves everything else, an existing `%XX` included, as it is.
/// Here a non-ASCII byte is kept raw, and an uppercase escape of a non-ASCII
/// byte is decoded back. That is exactly what the parser would have produced
/// from that raw byte. The result is then NFC-normalized. A lowercase escape
/// stays an escape, so `%c4` and `ğ` stay unequal, as they are as URLs.
fn compare_form(seg: &str) -> String {
    let b = seg.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            let v = h * 16 + l;
            let upper = !b[i + 1].is_ascii_lowercase() && !b[i + 2].is_ascii_lowercase();
            if v >= 0x80 && upper {
                out.push(v);
            } else {
                out.extend_from_slice(&b[i..i + 3]);
            }
            i += 3;
            continue;
        }
        let c = b[i];
        if c < 0x80 && in_path_set(c) {
            out.extend_from_slice(format!("%{c:02X}").as_bytes());
        } else {
            out.push(c);
        }
        i += 1;
    }
    crate::opf::nfc(&String::from_utf8_lossy(&out))
}

fn is_single_dot(seg: &str) -> bool {
    seg == "." || seg.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(seg: &str) -> bool {
    matches!(seg, "..")
        || seg.eq_ignore_ascii_case(".%2e")
        || seg.eq_ignore_ascii_case("%2e.")
        || seg.eq_ignore_ascii_case("%2e%2e")
}

/// Whether `href`, written in a document in `base_dir`, parses to the content
/// URL of the entry `entry`, the URL Standard's equality with fragments
/// excluded and NFC left out.
///
/// `None` when the question is not this check's to answer: an absolute URL, a
/// path-absolute reference (RSC-026), a query (RSC-033), a backslash, or an
/// `https:` URL with no slashes (RSC-020). Those have their own findings, and
/// the comparison below models only a path.
fn parses_to(base_dir: &str, href: &str, entry: &str) -> Option<bool> {
    let href = crate::url::trim_url(href);
    if crate::url::is_absolute(href)
        || crate::url::https_relative(href).is_some()
        || href.starts_with('/')
        || href.contains('\\')
    {
        return None;
    }
    let path = href.split('#').next().unwrap_or(href);
    if path.contains('?') || path.is_empty() {
        return None;
    }
    let mut segs: Vec<String> = base_dir
        .split('/')
        .filter(|s| !s.is_empty())
        .map(compare_form)
        .collect();
    let parts: Vec<&str> = path.split('/').collect();
    for (i, p) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if is_double_dot(p) {
            segs.pop();
            if last {
                segs.push(String::new());
            }
        } else if is_single_dot(p) {
            if last {
                segs.push(String::new());
            }
        } else {
            segs.push(compare_form(p));
        }
    }
    let want: Vec<String> = entry.split('/').map(compare_form).collect();
    Some(segs == want)
}

/// The `%XX` escapes in `href` that encode an RFC 3986 reserved character
/// (`gen-delims` and `sub-delims`, § 2.2), as written.
fn reserved_escapes(href: &str) -> Vec<&str> {
    let path = href.split('#').next().unwrap_or(href);
    let b = path.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            let v = h * 16 + l;
            if b":/?#[]@!$&'()*+,;=".contains(&v) {
                out.push(&path[i..i + 3]);
            }
            i += 3;
            continue;
        }
        i += 1;
    }
    out
}

/// A misspelled reference, ready for the site to push at its own anchor.
pub(crate) struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    pub text: String,
    pub rule: &'static str,
    /// `[href as written, the entry name it was found under]`, both as the
    /// files hold them.
    pub params: Vec<String>,
}

/// What, if anything, to report about `href` in a document in `base_dir`,
/// which the lenient lookup found as the entry `entry`.
///
/// Call it only where the lookup succeeded. A reference that names no file is
/// RSC-007's business, not this one's. Nothing is reported without
/// `advisory`.
///
/// - EPUB 3: whenever the reference does not parse to the entry's URL under
///   the URL Standard.
/// - EPUB 2: only when that is because the reference percent-encodes a
///   reserved character the name holds as it is.
pub(crate) fn check(
    is_epub3: bool,
    advisory: bool,
    base_dir: &str,
    href: &str,
    entry: &str,
) -> Option<Finding> {
    if !advisory || parses_to(base_dir, href, entry)? {
        return None;
    }
    let params = vec![href.to_string(), entry.to_string()];
    if is_epub3 {
        return Some(Finding {
            id: crate::ids::ADV_025,
            severity: Severity::Usage,
            text: format!(
                "the reference '{href}' names the file '{entry}' only once it is percent-decoded; \
                 under the URL Standard they are different URLs, and a reading system that \
                 compares them treats them as two files"
            ),
            rule: "url.reference.content_url_mismatch",
            params,
        });
    }
    let escapes = reserved_escapes(href);
    let first = escapes.first()?;
    Some(Finding {
        id: crate::ids::ADV_025,
        severity: Severity::Usage,
        text: format!(
            "the reference '{href}' writes '{first}' where the file name '{entry}' has the \
             character itself; under RFC 3986 a percent-encoded reserved character makes a \
             different URI, and a reading system that compares them treats them as two files"
        ),
        rule: "url.reference.reserved_character_encoded",
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(base: &str, href: &str, entry: &str) -> Option<bool> {
        parses_to(base, href, entry)
    }

    #[test]
    fn the_parser_leaves_sub_delims_alone_so_an_escape_of_one_is_a_different_url() {
        assert_eq!(
            eq("OEBPS", "Agatha'nin.xhtml", "OEBPS/Agatha'nin.xhtml"),
            Some(true)
        );
        assert_eq!(
            eq("OEBPS", "Agatha%27nin.xhtml", "OEBPS/Agatha'nin.xhtml"),
            Some(false)
        );
        for c in "!$&'()*+,;=".chars() {
            let entry = format!("a{c}b.xhtml");
            let href = format!("a%{:02X}b.xhtml", c as u8);
            assert_eq!(eq("", &href, &entry), Some(false), "{href}");
            assert_eq!(eq("", &entry, &entry), Some(true), "{entry}");
        }
    }

    #[test]
    fn what_the_parser_encodes_itself_is_equal_written_either_way() {
        assert_eq!(eq("", "a%20b.xhtml", "a b.xhtml"), Some(true));
        assert_eq!(eq("", "a b.xhtml", "a b.xhtml"), Some(true));
        assert_eq!(eq("", "a%C4%9Fa%C3%A7.xhtml", "ağaç.xhtml"), Some(true));
        assert_eq!(eq("", "ağaç.xhtml", "ağaç.xhtml"), Some(true));
        // and the base document's own directory is a file name too
        assert_eq!(eq("Bölüm 1", "a.xhtml", "Bölüm 1/a.xhtml"), Some(true));
    }

    #[test]
    fn lowercase_hex_and_encoded_unreserved_are_different_urls() {
        assert_eq!(eq("", "a%c4%9fa%c3%a7.xhtml", "ağaç.xhtml"), Some(false));
        assert_eq!(eq("", "%41.xhtml", "A.xhtml"), Some(false));
        assert_eq!(eq("", "a%7eb.xhtml", "a~b.xhtml"), Some(false));
    }

    #[test]
    fn a_literal_escape_in_a_file_name_is_matched_by_the_same_escape() {
        assert_eq!(eq("", "a%27b.xhtml", "a%27b.xhtml"), Some(true));
        assert_eq!(eq("", "a'b.xhtml", "a%27b.xhtml"), Some(false));
        // a bare % is kept by the parser, and %25 is not it
        assert_eq!(eq("", "a%b.xhtml", "a%b.xhtml"), Some(true));
        assert_eq!(eq("", "a%25b.xhtml", "a%b.xhtml"), Some(false));
    }

    #[test]
    fn dot_segments_resolve_and_empty_segments_stay() {
        assert_eq!(
            eq("OEBPS/Text", "../Images/a.png", "OEBPS/Images/a.png"),
            Some(true)
        );
        assert_eq!(
            eq("OEBPS/Text", "%2e%2E/Images/a.png", "OEBPS/Images/a.png"),
            Some(true)
        );
        assert_eq!(eq("OEBPS", "./a.xhtml", "OEBPS/a.xhtml"), Some(true));
        assert_eq!(
            eq("OEBPS", "Text//a.xhtml", "OEBPS/Text/a.xhtml"),
            Some(false)
        );
    }

    #[test]
    fn a_fragment_is_excluded_and_other_shapes_are_not_this_checks() {
        assert_eq!(
            eq("", "Agatha'nin.xhtml#p3", "Agatha'nin.xhtml"),
            Some(true)
        );
        assert_eq!(
            eq("", "Agatha%27nin.xhtml#p3", "Agatha'nin.xhtml"),
            Some(false)
        );
        assert_eq!(eq("", "a.xhtml?x", "a.xhtml"), None);
        assert_eq!(eq("", "/a.xhtml", "a.xhtml"), None);
        assert_eq!(eq("", "http://x/a.xhtml", "a.xhtml"), None);
        assert_eq!(eq("", "#top", "a.xhtml"), None);
    }

    #[test]
    fn unicode_normalization_is_left_out() {
        let nfd = "a\u{0308}.xhtml"; // ä decomposed
        assert_eq!(eq("", nfd, "ä.xhtml"), Some(true));
        assert_eq!(eq("", "a%CC%88.xhtml", "ä.xhtml"), Some(true));
    }

    #[test]
    fn only_behind_advisory_and_epub2_only_for_a_reserved_escape() {
        assert!(check(true, false, "", "Agatha%27nin.xhtml", "Agatha'nin.xhtml").is_none());
        let f = check(true, true, "", "Agatha%27nin.xhtml", "Agatha'nin.xhtml").unwrap();
        assert_eq!((f.id, f.severity), (crate::ids::ADV_025, Severity::Usage));
        assert_eq!(f.params, vec!["Agatha%27nin.xhtml", "Agatha'nin.xhtml"]);
        assert!(check(true, true, "", "a%c4%9f.xhtml", "ağ.xhtml").is_some());
        assert!(check(true, true, "", "Agatha'nin.xhtml", "Agatha'nin.xhtml").is_none());

        assert!(check(false, false, "", "Agatha%27nin.xhtml", "Agatha'nin.xhtml").is_none());
        let f = check(false, true, "", "Agatha%27nin.xhtml", "Agatha'nin.xhtml").unwrap();
        assert_eq!((f.id, f.severity), (crate::ids::ADV_025, Severity::Usage));
        // equivalent under RFC 3986: nothing at EPUB 2
        assert!(check(false, true, "", "a%c4%9f.xhtml", "ağ.xhtml").is_none());
        assert!(check(false, true, "", "%41.xhtml", "A.xhtml").is_none());
    }
}
