//! EPUB 3 Core Media Types (§3.2), shared by the foreign-resource-fallback
//! checks (RSC-032/MED-003/MED-007), the non-preferred-usage check
//! (OPF-090), and the exempt-font usage check (CSS-007). Assembled from the
//! real media-type strings used across the corpus's own
//! `resources-core-media-types-*.opf` fixtures, not guessed.

/// Preferred (current, IANA-registered) Core Media Types.
const PREFERRED: &[&str] = &[
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "image/webp",
    "audio/mpeg",
    "audio/mp4",
    "audio/ogg",
    "audio/opus",
    "text/css",
    "font/otf",
    "font/ttf",
    "font/woff",
    "font/woff2",
    "application/xhtml+xml",
    "application/javascript",
    "application/x-dtbncx+xml",
    "application/smil+xml",
    "application/pls+xml",
];

/// Non-preferred but still-valid legacy aliases of the types above -
/// accepted as Core Media Types, but flagged as OPF-090 usage when used.
/// Deliberately does NOT include `application/x-font-woff`: unlike the
/// other font aliases here (each confirmed via
/// `resources-core-media-types-not-preferred-valid.opf`'s own 7 tested
/// types), that one is used only by `foreign-exempt-font-valid` - a real
/// corpus fixture that expects it to be treated as a *foreign* (non-CMT)
/// font, not a non-preferred Core Media Type.
const NON_PREFERRED: &[&str] = &[
    "application/font-sfnt",
    "application/font-woff",
    "application/x-font-ttf",
    "application/vnd.ms-opentype",
    "application/ecmascript",
    "text/javascript",
];

/// Strip any `; charset=...`/`; codecs=...` parameter before comparing a
/// declared media-type against the lists above.
pub(crate) fn base_media_type(mt: &str) -> &str {
    mt.split(';').next().unwrap_or(mt).trim()
}

pub(crate) fn is_core_media_type(mt: &str) -> bool {
    let base = base_media_type(mt);
    PREFERRED.contains(&base) || NON_PREFERRED.contains(&base)
}

pub(crate) fn is_non_preferred_core_media_type(mt: &str) -> bool {
    NON_PREFERRED.contains(&base_media_type(mt))
}

/// EPUB 3 defines no Core Media Type for video at all, so any `video/*`
/// resource is exempt from the fallback requirement everywhere it's used
/// (confirmed via `foreign-exempt-xhtml-video-valid` and
/// `foreign-exempt-xhtml-video-in-img-valid`, the latter using a video
/// resource directly as an `<img src>` with no fallback).
pub(crate) fn is_exempt_video(mt: &str) -> bool {
    base_media_type(mt).starts_with("video/")
}

/// EPUB 3 §3.6 allows audio, video, and font resources to be located
/// remotely; used to decide whether a remote `<object>` (the one context
/// where the remote-restriction follows the resource's own category
/// rather than the host element - confirmed via `resources-remote-audio-
/// object-valid` vs `resources-remote-object-undeclared-error`, an
/// undeclared/unknown-category resource) is exempt.
pub(crate) fn is_audio_video_or_font(mt: &str) -> bool {
    let base = base_media_type(mt);
    base.starts_with("audio/")
        || base.starts_with("video/")
        || base.starts_with("font/")
        || base.starts_with("application/font-")
        || base.starts_with("application/x-font-")
        || base == "application/vnd.ms-opentype"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_media_types_recognized() {
        assert!(is_core_media_type("image/jpeg"));
        assert!(is_core_media_type("audio/ogg; codecs=opus"));
        assert!(is_core_media_type("font/ttf"));
        assert!(!is_core_media_type("audio/foreign"));
        assert!(!is_core_media_type("image/vnd.xyz"));
    }

    #[test]
    fn non_preferred_flagged() {
        assert!(is_non_preferred_core_media_type("application/font-sfnt"));
        assert!(is_non_preferred_core_media_type("text/javascript"));
        assert!(!is_non_preferred_core_media_type("font/ttf"));
        assert!(!is_non_preferred_core_media_type("audio/foreign"));
    }

    #[test]
    fn video_always_exempt() {
        assert!(is_exempt_video("video/avi"));
        assert!(is_exempt_video("video/webm"));
        assert!(!is_exempt_video("audio/foreign"));
    }
}
