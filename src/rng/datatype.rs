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
    IdRef,
    IdRefs,
    Language,
    AnyUri,
    Boolean,
    Integer,
    NonNegativeInteger,
    PositiveInteger,
    Decimal,
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
            "IDREF" => IdRef,
            "IDREFS" => IdRefs,
            "language" => Language,
            "anyURI" => AnyUri,
            "boolean" => Boolean,
            "integer" | "long" | "int" => Integer,
            "nonNegativeInteger" | "unsignedLong" | "unsignedInt" => NonNegativeInteger,
            "positiveInteger" => PositiveInteger,
            "decimal" | "double" | "float" => Decimal,
            "dateTime" => DateTime,
            "date" => Date,
            "time" => Time,
            other => Unknown(other.to_string()),
        }
    }

    /// Apply the XSD whiteSpace facet for this type.
    fn normalize(&self, s: &str) -> String {
        match self {
            Datatype::String => s.to_string(),
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
            // every other built-in has whiteSpace="collapse"
            _ => s.split_whitespace().collect::<Vec<_>>().join(" "),
        }
    }

    /// Is `raw` a valid lexical value of this type?
    pub fn allows(&self, raw: &str) -> bool {
        let s = self.normalize(raw);
        match self {
            Datatype::String
            | Datatype::NormalizedString
            | Datatype::Token
            | Datatype::AnyUri
            | Datatype::Unknown(_) => true,
            Datatype::NmToken => is_nmtoken(&s),
            Datatype::NmTokens => !s.is_empty() && s.split(' ').all(is_nmtoken),
            Datatype::Name => is_name(&s),
            Datatype::NCName | Datatype::Id | Datatype::IdRef => is_ncname(&s),
            Datatype::IdRefs => !s.is_empty() && s.split(' ').all(is_ncname),
            Datatype::Language => is_language(&s),
            Datatype::Boolean => matches!(s.as_str(), "true" | "false" | "0" | "1"),
            Datatype::Integer => is_integer(&s),
            Datatype::NonNegativeInteger => s.parse::<i128>().is_ok_and(|n| n >= 0),
            Datatype::PositiveInteger => s.parse::<i128>().is_ok_and(|n| n > 0),
            Datatype::Decimal => is_decimal(&s),
            Datatype::DateTime => is_datetime(&s),
            Datatype::Date => strip_tz(&s).map(is_date_core).unwrap_or(false),
            Datatype::Time => strip_tz(&s).map(is_time_core).unwrap_or(false),
        }
    }

    /// Value-space equality, used by `<value>`.
    pub fn equal(&self, a: &str, b: &str) -> bool {
        match self {
            Datatype::Integer | Datatype::NonNegativeInteger | Datatype::PositiveInteger => {
                match (a.trim().parse::<i128>(), b.trim().parse::<i128>()) {
                    (Ok(x), Ok(y)) => x == y,
                    _ => self.normalize(a) == self.normalize(b),
                }
            }
            Datatype::Boolean => bool_val(&self.normalize(a)) == bool_val(&self.normalize(b)),
            _ => self.normalize(a) == self.normalize(b),
        }
    }
}

fn bool_val(s: &str) -> Option<bool> {
    match s {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '-' | '_')
}

fn is_ncname(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if is_name_start(c) => it.all(is_name_char),
        _ => false,
    }
}
fn is_name(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if is_name_start(c) || c == ':' => it.all(|c| is_name_char(c) || c == ':'),
        _ => false,
    }
}
fn is_nmtoken(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| is_name_char(c) || c == ':')
}

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

    #[test]
    fn value_equality() {
        assert!(D::Integer.equal("01", "1"));
        assert!(D::Boolean.equal("true", "1"));
        assert!(D::Token.equal("  a   b ", "a b"));
        assert!(!D::String.equal("a b", "a  b"));
    }
}
