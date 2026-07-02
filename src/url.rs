//! A small, hand-written absolute-URL syntax validator for `<a href>`
//! values - no new dependency, same "no new dependency for a narrow
//! grammar" style as `smil.rs`'s clock-value parser and `htm.rs`'s
//! datetime grammar. Scope confirmed via the real corpus's two dedicated
//! fixtures: literal spaces anywhere in the URL, an invalid character in
//! the host (a comma), and a scheme not immediately followed by "//" are
//! all RSC-020; an unregistered URL scheme is HTM-025.

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
    if href.contains(' ') {
        return true;
    }
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
    // Internationalized domain names (real Unicode host labels),
    // percent-encoded octets, and an underscore (non-standard but
    // accepted by most browsers, confirmed via `url-valid.xhtml`) are all
    // legitimate - only a stray ASCII character outside that alphabet
    // (e.g. a comma) is a real syntax error.
    host.chars().any(|c| {
        c.is_ascii()
            && !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']' | '%' | '_'))
    })
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

    #[test]
    fn detects_space_in_url() {
        assert!(has_syntax_error("https://www.example .com"));
        assert!(has_syntax_error("http://  www.example.com"));
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
