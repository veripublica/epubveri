//! A small, hand-written absolute-URL syntax validator for `<a href>`
//! values - no new dependency, same "no new dependency for a narrow
//! grammar" style as `smil.rs`'s clock-value parser and `htm.rs`'s
//! datetime grammar. Scope confirmed via the real corpus's dedicated
//! fixtures: an invalid character in the host (a comma or a space), and a
//! scheme not immediately followed by "//", are RSC-020; an unregistered
//! URL scheme is HTM-025.
//!
//! Deliberately NOT flagged: a space (or other stray character) in the
//! path/query/fragment, or leading/trailing whitespace around the whole
//! URL. EPUB references the WHATWG URL Standard, whose parser strips
//! leading/trailing spaces and percent-encodes an interior path/query
//! space - so such a URL is a *valid URL string* in practice, which is why
//! epubcheck accepts it (`url-valid.xhtml`'s "Whitespace around" case and
//! its `%20`-in-query case). Only a space that breaks the *host* (where it
//! genuinely can't be parsed, `url-invalid-error.xhtml`) is an error.
//! Reported by patrik on the MobileRead forum: a trailing space in a
//! youtube query was wrongly drawing RSC-020 while epubcheck stayed
//! silent.

/// Real, commonly-registered IANA URL schemes - anything else is
/// HTM-025. Includes every scheme `is_external`/`is_remote_url` already
/// treat specially, plus a few other common ones.
const REGISTERED_SCHEMES: &[&str] = &[
    "http", "https", "ftp", "ftps", "mailto", "tel", "data", "urn", "file", "ws", "wss", "irc",
];

/// Only meaningful on absolute URLs (a scheme followed by `:`) - relative
/// and fragment-only hrefs are untouched by both checks.
pub(crate) fn is_absolute(href: &str) -> bool {
    href.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty() && scheme.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// RSC-020: the URL doesn't conform to basic URL syntax. The "must have
/// `//` after the scheme" and "host must be sane" rules are scoped to
/// http/https specifically (both real corpus fixtures only ever exercise
/// those two schemes) - other schemes (`mailto:`, `data:`, `tel:`, `urn:`)
/// are legitimately non-hierarchical and never have `//` at all, so
/// applying that rule to them uniformly would be a real false positive
/// (confirmed via `a-href-valid.xhtml`'s `mailto:` link).
pub(crate) fn has_syntax_error(href: &str) -> bool {
    let Some((scheme, rest)) = href.split_once(':') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    if !rest.starts_with("//") {
        return true;
    }
    let after_slashes = &rest[2..];
    let host = after_slashes
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_slashes);
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    // Percent-decode *after* stripping userinfo, never before: `%40` decodes
    // to `@`, and decoding first would let the userinfo strip eat the host
    // and hide the very error being looked for.
    //
    // epubcheck (galimatias, following WHATWG) decodes the host and then
    // applies the forbidden-domain code points, so `http://a%40b.com` is an
    // error and `http://ex%C3%BCample.com` - which decodes to a real IDN
    // label - is not. Ours allowed `%` outright, on the reasoning that
    // percent-encoded octets are legitimate; they are, but only when what
    // they decode to is. Measured one URL per shape against 5.3.0: `%40` and
    // `%20` are RSC-020, `%C3%BC` and a real `user@` are clean.
    let host = percent_decode_lossy(host);
    let host = host.as_str();
    // Internationalized domain names (real Unicode host labels),
    // percent-encoded octets, and an underscore (non-standard but
    // accepted by most browsers, confirmed via `url-valid.xhtml`) are all
    // legitimate - only a stray ASCII character outside that alphabet
    // (a comma, or a space) is a real syntax error. The space check is
    // scoped to the *host* on purpose: a space in the path/query is
    // normalized by the WHATWG parser EPUB references (percent-encoded, or
    // stripped when leading/trailing), so it isn't an error there - see
    // the module note.
    host.chars().any(|c| {
        c.is_ascii()
            && !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']' | '%' | '_'))
    })
}

/// Percent-decode a host for validation. Invalid escapes (`%zz`, a trailing
/// `%`) are left exactly as they are rather than dropped: the point is to see
/// what the URL parser would see, and a byte sequence that is not valid UTF-8
/// after decoding is replaced rather than discarded, so nothing silently
/// disappears from the string being checked.
fn percent_decode_lossy(host: &str) -> String {
    if !host.contains('%') {
        return host.to_string();
    }
    let b = host.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(h) = b.get(i + 1..i + 3)
            && let Ok(hs) = std::str::from_utf8(h)
            && let Ok(v) = u8::from_str_radix(hs, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// HTM-025: the URL's scheme isn't a real, registered one.
pub(crate) fn has_unregistered_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    !REGISTERED_SCHEMES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host is percent-decoded before it is judged, and only after the
    /// userinfo has been stripped. `%40` decodes to `@`; decoding first would
    /// let the userinfo strip eat the host and hide the error.
    ///
    /// Found on a real book (`http://ykykultur%40ykykultur.com.tr`) that
    /// epubcheck reports and we did not. The boundary was measured one URL
    /// per shape against 5.3.0.
    #[test]
    fn a_percent_escape_in_the_host_is_judged_by_what_it_decodes_to() {
        // Decodes to a forbidden domain character.
        assert!(has_syntax_error("http://ykykultur%40ykykultur.com.tr"));
        assert!(has_syntax_error("http://exa%20mple.com/x"));
        // Decodes to a real IDN label - percent-encoding is not the problem,
        // what it decodes to is.
        assert!(!has_syntax_error("http://ex%C3%BCample.com/x"));
        // A genuine userinfo is not a host character at all.
        assert!(!has_syntax_error("http://user@example.com/x"));
        // Scoped to the host: the same escape in the path is ordinary.
        assert!(!has_syntax_error("http://example.com/a%40b"));
        assert!(!has_syntax_error("http://example.com/x"));
        // A malformed escape is left alone rather than guessed at.
        assert!(!has_syntax_error("http://exa%zzmple.com/x"));
    }

    #[test]
    fn detects_space_in_host() {
        // A space in the host genuinely breaks parsing - still an error
        // (matches epubcheck's url-invalid-error.xhtml).
        assert!(has_syntax_error("https://www.example .com"));
        assert!(has_syntax_error("http://  www.example.com"));
    }

    #[test]
    fn space_in_path_or_query_is_not_an_error() {
        // patrik (MobileRead): a trailing space in the query was wrongly
        // flagged. The WHATWG parser EPUB references normalizes a
        // path/query space (percent-encoded, or stripped when trailing), so
        // the URL is valid and epubcheck accepts it - we must not error.
        assert!(!has_syntax_error(
            "https://www.youtube.com/watch?v=1ju_N8JlXFc. "
        ));
        assert!(!has_syntax_error(
            "https://www.youtube.com/watch?v=1ju_N8JlXFc. \u{202c}"
        ));
        assert!(!has_syntax_error("https://example.com/a b/c"));
    }

    #[test]
    fn detects_missing_slashes() {
        assert!(has_syntax_error("https:/www.example.com"));
        assert!(has_syntax_error("https:www.example.com"));
    }

    #[test]
    fn detects_invalid_host_character() {
        assert!(has_syntax_error("https://w,w.example.com"));
    }

    #[test]
    fn valid_url_has_no_syntax_error() {
        assert!(!has_syntax_error("https://www.example.com/path"));
    }

    #[test]
    fn detects_unregistered_scheme() {
        assert!(has_unregistered_scheme("httpf://example.org"));
        assert!(!has_unregistered_scheme("http://example.org"));
        assert!(!has_unregistered_scheme("mailto:a@b.com"));
    }
}
