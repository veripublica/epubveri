//! Minimal XSD / RELAX NG built-in datatype support for `<data>` and `<value>`.
//!
//! We model the datatypes EPUB schemas actually use and validate their lexical
//! space (after XSD whiteSpace processing). Unrecognized types are accepted
//! leniently. Facets (minLength, pattern, enumerations via `<param>`) are not
//! yet supported — that comes with the schemas that need them.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Datatype {
    String,
    Token,
    NormalizedString,
    NmToken,
    NmTokens,
    Name,
    NCName,
    Id,
    /// HTML5's `id`: one or more characters, none of them XML whitespace
    /// (`xsd:string { pattern = '[^\\s]+' }` in epubcheck's `datatypes.rnc`,
    /// where it is called `datatype.html5.token`). Not an XSD built-in -
    /// there is no standard type with these bounds - so it carries a name of
    /// our own, used only by our own schemas.
    Html5Token,
    /// `<meta http-equiv>`'s value: one of HTML5's five pragma directives,
    /// matched case-insensitively. epubcheck spells this as five separate
    /// `xsd:string { pattern = "[rR][eE][fF]..." }` alternatives, one per
    /// directive, in `mod/epub-xhtml-inc.rnc`'s override of `meta.rnc` — the
    /// comment there calls the letter-class spelling "an ugly hack" for
    /// case-insensitiveness. Not an XSD built-in, so it carries a name of our
    /// own, like `html5Token` above.
    HttpEquiv,
    /// `<meta charset>`'s value in the XML serialization: `utf-8`, matched
    /// case-insensitively (`meta.rnc`'s `xsd:string { pattern = "[uU][tT][fF]-8" }`
    /// under `XMLonly`; the unrestricted `string` alternative beside it is
    /// `HTMLonly`, which epubcheck sets to `notAllowed`).
    MetaCharset,
    /// An EPUB 2 `<style type>`: OPS 2.0.1 § 2.3.8, "must be given the value of
    /// text/css or the deprecated text/x-oeb1-css". Compared as a media type:
    /// the essence before any `;` parameter, trimmed, case-blind, so only a
    /// value that names another type fails. Named by us; epubcheck takes any
    /// string here.
    Epub2StyleType,
    /// An EPUB 3 `<style type>`: HTML § 16.1, "If the attribute is present, its
    /// value must be an ASCII case-insensitive match for "text/css"". No
    /// trimming and no parameters, as for an `xsd:string` pattern. Named by
    /// us, as the loader reads no `<param>` facets.
    HtmlStyleType,
    /// An EPUB 3 `<script charset>`: HTML § 16.1, "If the attribute is
    /// present, its value must be an ASCII case-insensitive match for
    /// "utf-8"". Exact, like [`Datatype::HtmlStyleType`].
    HtmlScriptCharset,
    /// A media type as the container schema reads one: `datatype.mimetype`,
    /// `xsd:string { pattern = '[a-zA-Z0-9!#$&+\-\^_]+/[a-zA-Z0-9!#$&+\-\^_]+.*' }`.
    /// An `xsd:string` keeps its whitespace, so a padded value fails. Named
    /// by us, as the pattern facet is not something the loader reads.
    MediaType,
    /// `rendition:media`'s value: the container schema's media-query pattern
    /// (Multiple-Rendition Publications 1.0), on the raw `xsd:string`.
    RenditionMedia,
    IdRef,
    IdRefs,
    Language,
    AnyUri,
    Boolean,
    Integer,
    NonNegativeInteger,
    PositiveInteger,
    Decimal,
    /// `xsd:float`, which is **not** `xsd:decimal` and must not be folded into
    /// it: its lexical space adds an exponent and the three special values
    /// `INF`, `-INF` and `NaN`. Measured against epubcheck 5.4.0 one value per
    /// run, on `aria-valuenow` with `role="progressbar"` so the per-element
    /// pairing could not mask the answer (2026-09-21): `1e3`, `-2.5E-3`,
    /// `1.0e+3`, `.5`, `INF` and `NaN` are all accepted there, and every one of
    /// them is rejected by `is_decimal`. Mapping `float` onto `Decimal` would
    /// therefore have invented an error on six valid shapes.
    Float,
    DateTime,
    Date,
    Time,
    /// Recognized library, unknown type — validated leniently (any value).
    Unknown(String),
}

impl Datatype {
    /// Resolve a `(datatypeLibrary, type-name)` pair. The two relevant
    /// libraries — the RELAX NG built-ins (`string`/`token`) and XSD — use
    /// distinct names, so we can map on the name alone.
    pub fn from(_library: &str, name: &str) -> Datatype {
        use Datatype::*;
        match name {
            "string" => String,
            "token" => Token,
            "normalizedString" => NormalizedString,
            "NMTOKEN" => NmToken,
            "NMTOKENS" => NmTokens,
            "Name" => Name,
            "NCName" => NCName,
            "ID" => Id,
            "html5Token" => Html5Token,
            "httpEquiv" => HttpEquiv,
            "metaCharset" => MetaCharset,
            "epub2StyleType" => Epub2StyleType,
            "htmlStyleType" => HtmlStyleType,
            "htmlScriptCharset" => HtmlScriptCharset,
            "mediaType" => MediaType,
            "renditionMedia" => RenditionMedia,
            "IDREF" => IdRef,
            "IDREFS" => IdRefs,
            "language" => Language,
            "anyURI" => AnyUri,
            "boolean" => Boolean,
            "integer" | "long" | "int" => Integer,
            "nonNegativeInteger" | "unsignedLong" | "unsignedInt" => NonNegativeInteger,
            "positiveInteger" => PositiveInteger,
            "decimal" => Decimal,
            "float" | "double" => Float,
            "dateTime" => DateTime,
            "date" => Date,
            "time" => Time,
            other => Unknown(other.to_string()),
        }
    }

    /// Apply the XSD whiteSpace facet for this type.
    fn normalize(&self, s: &str) -> String {
        match self {
            Datatype::String | Datatype::MediaType | Datatype::RenditionMedia => s.to_string(),
            Datatype::NormalizedString => s
                .chars()
                .map(|c| {
                    if matches!(c, '\t' | '\n' | '\r') {
                        ' '
                    } else {
                        c
                    }
                })
                .collect(),
            // every other built-in has whiteSpace="collapse", which means
            // the four XML whitespace characters and nothing else: Jing keeps
            // a NO-BREAK SPACE, so `"a\u{a0}"` is not an NCName there.
            _ => s
                .split(crate::xmlext::is_xml_space)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Is `raw` a valid lexical value of this type?
    pub fn allows(&self, raw: &str) -> bool {
        let s = self.normalize(raw);
        match self {
            Datatype::String
            | Datatype::NormalizedString
            | Datatype::Token
            | Datatype::Unknown(_) => true,
            Datatype::AnyUri => Self::is_any_uri(&s),
            Datatype::MediaType => is_media_type(&s),
            Datatype::RenditionMedia => is_rendition_media(&s),
            Datatype::NmToken => is_nmtoken(&s),
            Datatype::NmTokens => !s.is_empty() && s.split(' ').all(is_nmtoken),
            Datatype::Name => is_name(&s),
            Datatype::NCName | Datatype::Id | Datatype::IdRef => is_ncname(&s),
            // XSD's `\s` is the four XML whitespace characters: an id holding a
            // NO-BREAK SPACE or an EM SPACE is valid in epubcheck.
            Datatype::Html5Token => {
                !raw.is_empty() && !raw.chars().any(crate::xmlext::is_xml_space)
            }
            Datatype::HttpEquiv => HTTP_EQUIV_DIRECTIVES
                .iter()
                .any(|d| s.eq_ignore_ascii_case(d)),
            Datatype::MetaCharset => s.eq_ignore_ascii_case("utf-8"),
            Datatype::Epub2StyleType => {
                let essence = raw.split(';').next().unwrap_or("");
                let essence = crate::xmlext::trim_xml_space(essence);
                essence.eq_ignore_ascii_case("text/css")
                    || essence.eq_ignore_ascii_case("text/x-oeb1-css")
            }
            Datatype::HtmlStyleType => raw.eq_ignore_ascii_case("text/css"),
            Datatype::HtmlScriptCharset => raw.eq_ignore_ascii_case("utf-8"),
            Datatype::IdRefs => !s.is_empty() && s.split(' ').all(is_ncname),
            Datatype::Language => is_language(&s),
            Datatype::Boolean => matches!(s.as_str(), "true" | "false" | "0" | "1"),
            Datatype::Integer => is_integer(&s),
            Datatype::NonNegativeInteger => s.parse::<i128>().is_ok_and(|n| n >= 0),
            Datatype::PositiveInteger => s.parse::<i128>().is_ok_and(|n| n > 0),
            Datatype::Decimal => is_decimal(&s),
            Datatype::Float => is_float(&s),
            Datatype::DateTime => is_datetime(&s),
            Datatype::Date => strip_tz(&s).map(is_date_core).unwrap_or(false),
            Datatype::Time => strip_tz(&s).map(is_time_core).unwrap_or(false),
        }
    }

    /// `xsd:anyURI`, kept deliberately narrow.
    ///
    /// epubcheck types `href` and its relatives as `xsd:anyURI` and Jing rejects
    /// a small set of shapes with "value of attribute … is invalid; must be a
    /// URI". The boundary was measured against 5.3.0 one value per book, because
    /// Jing is not vendored here and coding to RFC 3986 rather than to what
    /// epubcheck actually does is the mistake this project keeps having to undo:
    ///
    ///   `http://`   rejected - a special scheme with nothing after it
    ///   `%zz`       rejected - a percent sign that is not an escape
    ///   `:`         rejected - an empty scheme
    ///   `//`        rejected - a scheme-relative reference with nothing after
    ///               it (5.4.0; `///x.html` is accepted, and is RSC-020 only)
    ///   `http://x`  accepted
    ///   `a b`       accepted - a space is NOT an error here, which is what
    ///               keeps this from being a false-positive machine; the space
    ///               case belongs to RSC-020, separately and partially
    ///   `http://a b` accepted
    ///
    /// Three more rules, measured on 5.4.0 on 2026-10-14 over 70 values, each
    /// as an XHTML `<a href>` (where Jing's verdict is an ERROR) and an SVG
    /// one (a usage RSC-025):
    ///
    ///   `a#b#`, `##`         rejected - a second `#`
    ///   `1:x`, `a_b:c`, `é:x`, `-a:b`, `a:`
    ///                        rejected - a colon in the first segment makes
    ///               the part before it a scheme, which must be a letter and
    ///               then letters, digits, `+`, `-` or `.`, and must be
    ///               followed by something; `a/b:c`, `a?b:c` and `./1a:b`
    ///               have no scheme and are accepted
    ///   `a[b`, `]`, `http://h/[x]`, `http://[x]/`
    ///                        rejected - a bracket before the query or
    ///               fragment, unless it encloses an IPv6 host
    ///               (`http://[::1]:80/a`, `//[::1]/` and `?[x]`, `#[x]` are
    ///               accepted)
    ///
    /// Anything not on the measured list is still accepted, on the same
    /// reasoning RSC-020 is left partial: a guess about an unreadable
    /// implementation, applied to 39 schema sites, would invent errors on
    /// real books.
    fn is_any_uri(s: &str) -> bool {
        // A percent sign must introduce two hex digits.
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' {
                let ok = b
                    .get(i + 1..i + 3)
                    .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit));
                if !ok {
                    return false;
                }
                i += 3;
                continue;
            }
            i += 1;
        }
        // An empty scheme: a leading colon.
        if s.starts_with(':') {
            return false;
        }
        // A bare scheme-relative `//`: the same "nothing after the slashes".
        if s == "//" {
            return false;
        }
        // A scheme with nothing at all after `://`.
        if let Some(rest) = s.split_once("://").map(|(_, r)| r)
            && rest.is_empty()
        {
            return false;
        }
        // At most one fragment.
        if s.matches('#').count() > 1 {
            return false;
        }
        // A colon in the first segment: what precedes it is a scheme.
        let first_segment_end = s.find(['/', '?', '#']).unwrap_or(s.len());
        let mut after_scheme = s;
        if let Some(colon) = s[..first_segment_end].find(':') {
            let scheme = &s[..colon];
            let well_formed = scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
            if !well_formed || colon + 1 == s.len() {
                return false;
            }
            after_scheme = &s[colon + 1..];
        }
        // Brackets before the query and fragment only around an IPv6 host.
        let before_query =
            &after_scheme[..after_scheme.find(['?', '#']).unwrap_or(after_scheme.len())];
        if before_query.contains(['[', ']']) {
            let Some(rest) = before_query.strip_prefix("//") else {
                return false;
            };
            let authority = &rest[..rest.find('/').unwrap_or(rest.len())];
            let path = &rest[authority.len()..];
            let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            let ipv6 = host_port.strip_prefix('[').and_then(|h| h.split_once(']'));
            let Some((addr, port)) = ipv6 else {
                return false;
            };
            if path.contains(['[', ']'])
                || addr.parse::<std::net::Ipv6Addr>().is_err()
                || !(port.is_empty() || port.starts_with(':'))
                || port.contains(['[', ']'])
            {
                return false;
            }
        }
        true
    }

    /// Value-space equality, used by `<value>`.
    pub fn equal(&self, a: &str, b: &str) -> bool {
        match self {
            Datatype::Integer | Datatype::NonNegativeInteger | Datatype::PositiveInteger => {
                let int = |s: &str| crate::xmlext::trim_xml_space(s).parse::<i128>();
                match (int(a), int(b)) {
                    (Ok(x), Ok(y)) => x == y,
                    _ => self.normalize(a) == self.normalize(b),
                }
            }
            Datatype::Boolean => bool_val(&self.normalize(a)) == bool_val(&self.normalize(b)),
            _ => self.normalize(a) == self.normalize(b),
        }
    }
}

/// The five HTML5 pragma directives `<meta http-equiv>` may name. `content-type`
/// is the one epubcheck adds for XHTML — HTML5 itself restricts the encoding
/// declaration state to the HTML serialization, and `mod/epub-xhtml-inc.rnc`
/// re-defines `meta.http-equiv.content-type.elem` without its `& HTMLonly` so
/// that EPUB's XHTML may carry it too. Its `@content` then has its own,
/// separate constraint, checked in `opf.rs` where epubcheck checks it (a
/// Schematron assertion, not this grammar).
const HTTP_EQUIV_DIRECTIVES: [&str; 5] = [
    "content-type",
    "refresh",
    "default-style",
    "content-security-policy",
    "x-ua-compatible",
];

fn bool_val(s: &str) -> Option<bool> {
    match s {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

use crate::xmlname::{is_name, is_ncname, is_nmtoken};

fn is_language(s: &str) -> bool {
    let mut segs = s.split('-');
    let Some(first) = segs.next() else {
        return false;
    };
    if first.is_empty() || first.len() > 8 || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    segs.all(|seg| {
        !seg.is_empty() && seg.len() <= 8 && seg.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

fn is_integer(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
fn is_decimal(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let some_digit = !int.is_empty() || !frac.is_empty();
    some_digit
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.bytes().all(|b| b.is_ascii_digit())
}

/// `xsd:float`'s lexical space: `INF`, `-INF`, `NaN`, or a decimal mantissa
/// with an optional `e`/`E` exponent whose own digits are a plain integer.
/// The mantissa rule is `is_decimal`'s, so `.5` and `5.` stay legal.
fn is_float(s: &str) -> bool {
    if matches!(s, "INF" | "+INF" | "-INF" | "NaN") {
        return true;
    }
    let (mantissa, exponent) = match s.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (s, None),
    };
    if !is_decimal(mantissa) {
        return false;
    }
    match exponent {
        None => true,
        Some(e) => {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && e.bytes().all(|b| b.is_ascii_digit())
        }
    }
}

fn n_digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}
/// Strip a trailing `Z` or `±HH:MM` timezone; returns the value without it.
fn strip_tz(s: &str) -> Option<&str> {
    if let Some(r) = s.strip_suffix('Z') {
        return Some(r);
    }
    if s.len() >= 6 {
        let (head, tz) = s.split_at(s.len() - 6);
        let tb = tz.as_bytes();
        if (tb[0] == b'+' || tb[0] == b'-')
            && n_digits(&tz[1..3], 2)
            && tb[3] == b':'
            && n_digits(&tz[4..6], 2)
        {
            return Some(head);
        }
    }
    Some(s)
}
fn is_date_core(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let p: Vec<&str> = s.split('-').collect();
    p.len() == 3
        && p[0].len() >= 4
        && p[0].bytes().all(|b| b.is_ascii_digit())
        && n_digits(p[1], 2)
        && n_digits(p[2], 2)
}
fn is_time_core(s: &str) -> bool {
    let hms = s.split_once('.').map(|(a, _)| a).unwrap_or(s);
    let p: Vec<&str> = hms.split(':').collect();
    p.len() == 3 && n_digits(p[0], 2) && n_digits(p[1], 2) && n_digits(p[2], 2)
}
fn is_datetime(s: &str) -> bool {
    match s.split_once('T') {
        Some((d, t)) => is_date_core(d) && strip_tz(t).map(is_time_core).unwrap_or(false),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Datatype as D;

    /// XSD's whitespace facet and `\s` mean the four XML whitespace
    /// characters; Jing keeps a NO-BREAK SPACE. Each value here was asked of
    /// epubcheck 5.4.0's own datatype library (2026-10-05).
    #[test]
    fn whitespace_is_xml_whitespace_only() {
        assert!(D::NCName.allows(" a "));
        assert!(D::Id.allows("\ta\n"));
        assert!(!D::NCName.allows("a\u{a0}"));
        assert!(!D::Id.allows("\u{a0}a"));
        assert!(!D::Integer.allows("1\u{a0}"));
        assert!(D::Integer.allows(" 1 "));
        assert!(!D::Language.allows("en\u{2003}"));
        assert!(D::Html5Token.allows("a\u{a0}b"));
        assert!(D::Html5Token.allows("a\u{2003}b"));
        assert!(!D::Html5Token.allows("a b"));
        assert!(!D::IdRefs.allows("a\u{a0}b"));
    }

    #[test]
    fn language() {
        assert!(D::Language.allows("en"));
        assert!(D::Language.allows("en-US"));
        assert!(D::Language.allows("zh-Hant-TW"));
        assert!(!D::Language.allows("123"));
        assert!(!D::Language.allows("toolongtag")); // first subtag > 8
        assert!(!D::Language.allows("en_US")); // underscore not allowed
    }

    #[test]
    fn ncname_and_nmtoken() {
        assert!(D::NCName.allows("chapter1"));
        assert!(!D::NCName.allows("1chapter")); // can't start with digit
        assert!(!D::NCName.allows("a:b")); // no colon in NCName
        assert!(D::NmToken.allows("a:b-c.d"));
        assert!(!D::NmToken.allows("a b"));
    }

    #[test]
    fn integers() {
        assert!(D::NonNegativeInteger.allows("0"));
        assert!(D::NonNegativeInteger.allows("42"));
        assert!(!D::NonNegativeInteger.allows("-1"));
        assert!(D::PositiveInteger.allows("1"));
        assert!(!D::PositiveInteger.allows("0"));
        assert!(D::Integer.allows("-7"));
        assert!(!D::Integer.allows("3.5"));
    }

    #[test]
    fn datetimes() {
        assert!(D::DateTime.allows("2026-06-27T12:00:00Z"));
        assert!(D::DateTime.allows("2026-06-27T12:00:00+03:00"));
        assert!(D::Date.allows("2026-06-27"));
        assert!(!D::DateTime.allows("2026-06-27")); // missing time part
        assert!(!D::Date.allows("2026/06/27"));
    }

    /// Every row measured against epubcheck 5.4.0 on 2026-09-21, one value per
    /// run, on `aria-valuenow` with `role="progressbar"`. The six marked below
    /// are the ones `is_decimal` rejects, which is why `float` no longer maps
    /// onto `Decimal`.
    #[test]
    fn floats_are_not_decimals() {
        for ok in [
            "1.5", "10", ".5", // these `is_decimal` also accepts
            "1e3", "-2.5E-3", "1.0e+3", "INF", "-INF", "NaN", // these it does not
        ] {
            assert!(D::Float.allows(ok), "float should accept {ok:?}");
        }
        for bad in ["abc", "", "1e", "e3", "1e2.5", "1.2.3", "INFINITY", "nan"] {
            assert!(!D::Float.allows(bad), "float should reject {bad:?}");
        }
        // the six that separate the two types, stated as the difference itself
        for exp in ["1e3", "-2.5E-3", "1.0e+3", "INF", "-INF", "NaN"] {
            assert!(D::Float.allows(exp) && !D::Decimal.allows(exp), "{exp:?}");
        }
    }

    /// `xsd:anyURI` as Jing judges it, every value measured on epubcheck
    /// 5.4.0 as an XHTML `<a href>` (2026-10-14; the first four rejections
    /// on 5.3.0 before that).
    #[test]
    fn any_uri_follows_jing() {
        for ok in [
            "a b",
            "a\\b",
            "a|b",
            "a^b",
            "a`b",
            "a{b}",
            "<x>",
            "a\"b",
            "%41",
            "%E9",
            "\u{e9}",
            "http://x",
            "http://\u{e9}.com/",
            "mailto:x",
            "urn:x",
            "x:y:z",
            "a+b:c",
            "A1+-.:x",
            "http:x",
            "http:/x",
            "//h",
            "a/b:c",
            "a?b:c",
            "a#b:c",
            "./1a:b",
            "#",
            "a#",
            "http://h#",
            "x y#a b",
            "http://[::1]",
            "http://[::1]:80/a",
            "http://u@[::1]/",
            "//[::1]/",
            "http://[::1]/x",
            "#[x]",
            "?[x]",
            "x?[a]",
            "http://h/p?[q]",
            "http://a:b@c",
            "http://a:99999",
            "http://a:x",
        ] {
            assert!(D::AnyUri.allows(ok), "anyURI should accept {ok:?}");
        }
        for bad in [
            "%zz",
            "%",
            "a%2",
            "http://x/%zz",
            "http://h/a#%zz",
            ":",
            "//",
            "http://",
            "c.xhtml#a#b",
            "#a#b",
            "a#b#",
            "##",
            "http://h/p#f#g",
            "1a:b",
            "1:x",
            "-a:b",
            ".a:b",
            "a_b:c",
            "\u{e9}:x",
            "a:",
            "a:%",
            "a[b",
            "a]b",
            "[x]",
            "[",
            "]",
            "a/[b]",
            "http://[bad",
            "http://x]y",
            "http://[x]/",
            "http://h/[x]",
            "//h/[x]",
        ] {
            assert!(!D::AnyUri.allows(bad), "anyURI should reject {bad:?}");
        }
    }

    #[test]
    fn value_equality() {
        assert!(D::Integer.equal("01", "1"));
        assert!(D::Boolean.equal("true", "1"));
        assert!(D::Token.equal("  a   b ", "a b"));
        assert!(!D::String.equal("a b", "a  b"));
    }
}

/// `[a-zA-Z0-9!#$&+\-\^_]+/[a-zA-Z0-9!#$&+\-\^_]+.*`, matched whole. XSD's
/// `.` is any character but a line feed or a carriage return.
fn is_media_type(s: &str) -> bool {
    let tchar = |c: char| c.is_ascii_alphanumeric() || "!#$&+-^_".contains(c);
    let Some((ty, rest)) = s.split_once('/') else {
        return false;
    };
    let sub_len = rest.chars().take_while(|&c| tchar(c)).count();
    !ty.is_empty() && ty.chars().all(tchar) && sub_len > 0 && !rest.contains(['\n', '\r'])
}

/// The Multiple-Rendition media-query pattern, matched whole:
///
/// ```text
/// \s*(((only|not)\s+)?all|\([^>)]+\))(\s+and\s+\([^>)]+\))*\s*
///     (,\s*(((only|not)\s+)?all|\([^>)]+\))(\s+and\s+\([^>)]+\))*\s*)*
/// ```
///
/// A comma-separated list of queries; each is `all`, optionally after
/// `only`/`not`, or a parenthesised expression, then any number of
/// `and (...)`. `\s` is the four XML whitespace characters.
fn is_rendition_media(s: &str) -> bool {
    let ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    // One query, from the start of `q` (which carries no comma).
    fn query(q: &str, ws: impl Fn(char) -> bool + Copy) -> bool {
        let q = q.trim_start_matches(ws).trim_end_matches(ws);
        // The head: `((only|not)\s+)?all` or `\([^>)]+\)`.
        let rest = if let Some(r) = q.strip_prefix('(') {
            let Some(end) = r.find(')') else { return false };
            let inner = &r[..end];
            if inner.is_empty() || inner.contains('>') {
                return false;
            }
            &r[end + 1..]
        } else {
            let mut r = q;
            for kw in ["only", "not"] {
                if let Some(after) = r.strip_prefix(kw)
                    && after.starts_with(ws)
                {
                    r = after.trim_start_matches(ws);
                    break;
                }
            }
            match r.strip_prefix("all") {
                Some(after) => after,
                None => return false,
            }
        };
        // `(\s+and\s+\([^>)]+\))*`
        let mut r = rest;
        while !r.is_empty() {
            if !r.starts_with(ws) {
                return false;
            }
            let t = r.trim_start_matches(ws);
            let Some(t) = t.strip_prefix("and") else {
                return false;
            };
            if !t.starts_with(ws) {
                return false;
            }
            let t = t.trim_start_matches(ws);
            let Some(t) = t.strip_prefix('(') else {
                return false;
            };
            let Some(end) = t.find(')') else { return false };
            let inner = &t[..end];
            if inner.is_empty() || inner.contains('>') {
                return false;
            }
            r = &t[end + 1..];
        }
        true
    }
    // `[^>)]` admits a comma inside parentheses, so split only on commas at
    // depth zero.
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in s.char_indices() {
        match c {
            '(' if depth == 0 => depth = 1,
            ')' if depth == 1 => depth = 0,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts.iter().all(|q| query(q, ws))
}

#[cfg(test)]
mod container_datatype_tests {
    use super::Datatype;

    /// Each value measured against epubcheck 5.4.0 as `rendition:media` on a
    /// single-rendition book: accepted or RSC-005.
    #[test]
    fn rendition_media_matches_the_specifications_pattern() {
        let dt = Datatype::from("", "renditionMedia");
        for ok in [
            "all",
            "only all",
            "not all",
            "(min-width: 600px)",
            "all and (color)",
            "(a), (b)",
            " all ",
            "all,(x)",
            "(x)  and  (y)",
            "only  all",
            "(a,b)",
            "all and (x) and (y), not all",
        ] {
            assert!(dt.allows(ok), "{ok:?} is accepted");
        }
        for bad in [
            "screen",
            "(a>b)",
            "(a),",
            "(x) and(y)",
            "()",
            "not (x)",
            "x",
            "",
        ] {
            assert!(!dt.allows(bad), "{bad:?} is RSC-005");
        }
    }

    /// `datatype.mimetype` is an `xsd:string`, so padding fails it.
    #[test]
    fn a_media_type_keeps_its_whitespace() {
        let dt = Datatype::from("", "mediaType");
        assert!(dt.allows("application/xhtml+xml"));
        assert!(dt.allows("text/html; charset=utf-8"));
        assert!(!dt.allows(" application/xhtml+xml "));
        assert!(!dt.allows("xhtml"));
        assert!(!dt.allows("/xhtml"));
    }
}
