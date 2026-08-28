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
    // An empty host for a special scheme: `http://` and `http:///x` both
    // parse to no authority at all, which epubcheck reports as
    // "Invalid host: empty host". A real book carries two.
    if host.is_empty() {
        return true;
    }
    // **A denylist of what epubcheck actually rejects, not an allowlist of
    // what looks like a hostname.** This used to accept only
    // alphanumerics plus `. - : [ ] % _` and call everything else a syntax
    // error, which made us stricter than epubcheck on **seventeen** printable
    // ASCII characters. Measured one URL per character against 5.3.0, all
    // seventeen clean there and an error here:
    //
    //     , ! $ & ' ( ) * + ; = ~ ^ | { } `
    //
    // The comma is the one the corpus names. `url-host-unparseable-warning`
    // carries `https://w,w.example.com` with the comment "Host contains an
    // invalid character (see issue #1034)" — epubcheck's own fixture records
    // that it *should* flag this and does not, and w3c/epubcheck#1034 is
    // still open. Reporting it anyway is a restrictive divergence, which to
    // anyone diffing the two tools is indistinguishable from a false
    // positive; the project's rule sends those behind `--advisory`, and with
    // a shelf population of zero there is nothing to send.
    //
    // What epubcheck *does* reject, same measurement: `\`, and — after the
    // percent-decode above — `@` and a space (`%40`, `%20`). Those three are
    // the whole list here.
    //
    // Known gaps, left as gaps deliberately (false negatives, which harm
    // nobody who has them): a non-numeric port after `:`, and an unmatched
    // `[`/`]`, both of which epubcheck reports. Closing them means modelling
    // galimatias's port and IPv6 parsing, which is the inference this module
    // already refuses to make — see the RSC-020 note in CLAUDE.md.
    if host.chars().any(|c| matches!(c, '\\' | '@' | ' ')) {
        return true;
    }
    // A space anywhere in an absolute URL, not only in the host. The comment
    // above used to scope this to the host on the reasoning that the WHATWG
    // parser normalizes a space in the path; it does, but epubcheck parses
    // every URL a second time through galimatias with a strict handler that
    // turns those recoverable warnings into errors - so a space in the path
    // is an error there, and one real book carries twenty of them
    // ("http://www.ted.com/talks/richard_branson_s_life_at_30_000 _feet.html").
    // Measured against 5.3.0 one URL per book.
    // *Interior* space only: leading and trailing whitespace is stripped by
    // the URL parser and is valid, which the corpus says outright - the
    // fixture is called `content-model-a-with-leading-trailing-spaces-valid`
    // and it caught this the moment the rule was written without the trim.
    href.trim().contains(' ')
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

/// RSC-031: a remote reference that is not transport-secure.
///
/// **epubcheck's test is "not `https`", not "is `http://`"**, and the
/// difference is a real gap rather than a nicety.
/// `ResourceReferencesChecker`:382-388 reads
///
/// ```text
/// version == VERSION_3
///   && !EnumSet.of(LINK, HYPERLINK).contains(reference.type)
///   && !"https".equals(url.scheme())
///   && !"file".equals(url.scheme())
/// ```
///
/// so a `res:///system/fonts/HelveticaNeue.ttf` in a Calibre/Kobo
/// `@font-face` draws RSC-031 there and drew nothing here. Measured against
/// 5.3.0 with one book per version, which is also how the caller's `is_epub3`
/// guard was confirmed.
///
/// `file:` is excluded because it is disallowed and reported elsewhere;
/// `data:` never reaches this predicate, since [`is_remote_url`] already
/// rules it out (epubcheck's `OCFContainer.isRemote` does the same). The two
/// reference-type exemptions are structural here rather than conditional:
/// hyperlink targets live in `remote_link_refs`, a separate set, and the
/// package document's `<link>` elements are not collected into either of the
/// sets that reach RSC-031.
///
/// [`is_remote_url`]: crate::opf::is_remote_url
pub(crate) fn is_insecure_remote(href: &str) -> bool {
    let scheme = href.trim().split_once(':').map(|(s, _)| s).unwrap_or("");
    !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("file")
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

    /// A *trailing* space is stripped by the URL parser and is valid; an
    /// *interior* one is not.
    ///
    /// Two of this test's three assertions were our own stance rather than
    /// epubcheck's, and were re-measured against 5.3.0 one URL per book:
    /// only the first - patrik's actual MobileRead report, a trailing space
    /// in a query - is accepted by epubcheck. The other two it reports, and
    /// so do we now. A real book carries twenty path spaces of the same
    /// shape.
    ///
    /// The middle case is the reason the rule trims rather than tests the
    /// last character: an invisible formatting character after the space
    /// makes that space interior, and epubcheck flags it.
    #[test]
    fn a_trailing_space_is_valid_and_an_interior_one_is_not() {
        // patrik (MobileRead): must stay accepted.
        assert!(!has_syntax_error(
            "https://www.youtube.com/watch?v=1ju_N8JlXFc. "
        ));
        // The space is interior once something follows it.
        assert!(has_syntax_error(
            "https://www.youtube.com/watch?v=1ju_N8JlXFc. \u{202c}"
        ));
        assert!(has_syntax_error("https://example.com/a b/c"));
        assert!(!has_syntax_error("https://example.com/a/c"));
        // An empty host: `http://` and `http:///x` both parse to no
        // authority, which epubcheck reports as "Invalid host: empty host".
        // A real book carries two, alongside twenty path spaces.
        assert!(has_syntax_error("http://"));
        assert!(has_syntax_error("http:///x"));
        assert!(!has_syntax_error("http://x"));
    }

    #[test]
    fn detects_missing_slashes() {
        assert!(has_syntax_error("https:/www.example.com"));
        assert!(has_syntax_error("https:www.example.com"));
    }

    /// The host character set is epubcheck's, and it is a **denylist**.
    ///
    /// This test used to assert the opposite — that a comma in the host is a
    /// syntax error — which encoded our own stance rather than epubcheck's.
    /// Measured one URL per character against 5.3.0: seventeen printable
    /// ASCII characters our old allowlist rejected are clean there, the comma
    /// among them. `url-host-unparseable-warning.xhtml` carries exactly that
    /// URL and epubcheck says nothing about it; its own comment points at
    /// w3c/epubcheck#1034, still open.
    ///
    /// The three it does reject are asserted below them, so this cannot drift
    /// into accepting everything.
    #[test]
    fn host_characters_match_epubchecks_denylist() {
        for c in [
            ',', '!', '$', '&', '\'', '(', ')', '*', '+', ';', '=', '~', '^', '|', '{', '}', '`',
        ] {
            let url = format!("https://a{c}b.example.com");
            assert!(
                !has_syntax_error(&url),
                "epubcheck accepts {url} and so must we"
            );
        }
        assert!(has_syntax_error("https://a\\b.example.com"), "backslash");
        // `%40` and `%20` decode to `@` and a space, which is why the
        // percent-decode above has to run before this check.
        assert!(has_syntax_error("https://a%40b.example.com"), "encoded @");
        assert!(
            has_syntax_error("https://a%20b.example.com"),
            "encoded space"
        );
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

    /// The predicate is "not https", not "is http" — see the doc comment.
    /// Scheme comparison is case-insensitive because a URL scheme is, and
    /// `HTTPS://` reaching the insecure branch would be a false positive on
    /// a book doing nothing wrong.
    #[test]
    fn insecure_remote_is_anything_but_https() {
        for secure in [
            "https://example.com/f",
            "HTTPS://example.com/f",
            "file:///x",
        ] {
            assert!(!is_insecure_remote(secure), "{secure}");
        }
        for insecure in [
            "http://example.com/f",
            "res:///system/fonts/HelveticaNeue.ttf",
            "ftp://example.com/f",
        ] {
            assert!(is_insecure_remote(insecure), "{insecure}");
        }
    }
}
