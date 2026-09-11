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
    // EPUB 3.4 (CR draft, 2026-07-21) adds two image types, each with a
    // dated entry in the spec's own change log and a "3.4" row in the core
    // media types table: `image/avif` (06-Oct-2025, w3c/epub-specs#2794) and
    // `image/jxl` (23-Jan-2026, #2896). epubcheck has an open issue for AVIF
    // (w3c/epubcheck#1642, "ready for implementation") and none for JXL, so
    // JXL was checked against the table directly rather than taken from the
    // issue list.
    //
    // Shipping these before epubcheck does makes us *more* permissive than it
    // is today: a 3.3-targeting book using AVIF draws a fallback error there
    // and none here. That is a false negative against epubcheck-as-it-is and
    // a true negative against the spec as it will be - the deliberate cost of
    // being first, and the safe direction, since it cannot invent an error on
    // a valid book.
    "image/avif",
    "image/jxl",
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
pub(crate) const NON_PREFERRED: &[&str] = &[
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

/// What a font resource's own bytes say it is, read from the four-byte
/// signature RFC 8081 registers as each font media type's magic number.
///
/// The bytes rule spellings **out**, not in, and that asymmetry is the whole
/// reason this type exists. `font/ttf`'s magic-number list is `0x00010000`
/// alone, so an `OTTO` file is `font/ttf` under no reading; but `font/otf`'s
/// own list carries *both* `0x00010000` and `OTTO`, and the OpenType
/// specification says an OpenType font containing TrueType outlines "should
/// use the value of 0x00010000 for sfntVersion". So a `glyf`-outline font
/// declared `application/vnd.ms-opentype` is not mislabelled — EPUB 3.3 puts
/// that spelling on the OpenType row and nowhere else, and names `font/otf`
/// as that row's preferred type. See issue #135, where the opposite was
/// argued and measured: all 47 shelf fonts it names carry `OS/2` (required by
/// OpenType, not by Apple's TrueType) and 35 of them `GSUB`+`GPOS`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FontSignature {
    /// sfnt with TrueType (`glyf`) outlines: `0x00010000`, or Apple's `true`.
    Glyf,
    /// sfnt with CFF outlines: `OTTO`.
    Cff,
    /// WOFF 1.0: `wOFF`.
    Woff1,
    /// WOFF 2.0: `wOF2`.
    Woff2,
    /// An sfnt collection: `ttcf`. RFC 8081 gives it `font/collection`, which
    /// EPUB's Core Media Types table does not list at all — so no preferred
    /// spelling we could name admits it.
    Collection,
}

/// The font signature of `bytes`, if they carry one.
pub(crate) fn font_signature(bytes: &[u8]) -> Option<FontSignature> {
    match bytes.get(..4)? {
        [0x00, 0x01, 0x00, 0x00] | b"true" => Some(FontSignature::Glyf),
        b"OTTO" => Some(FontSignature::Cff),
        b"wOFF" => Some(FontSignature::Woff1),
        b"wOF2" => Some(FontSignature::Woff2),
        b"ttcf" => Some(FontSignature::Collection),
        _ => None,
    }
}

/// Whether RFC 8081's magic-number list for `mt` admits `sig`.
fn signature_admits(mt: &str, sig: FontSignature) -> bool {
    use FontSignature::*;
    match mt {
        "font/ttf" => sig == Glyf,
        "font/otf" => sig == Glyf || sig == Cff,
        "font/woff" => sig == Woff1,
        "font/woff2" => sig == Woff2,
        // Not a font type; its file's signature has no bearing.
        _ => true,
    }
}

/// Whether the resource's own bytes can bear on [`preferred_media_type`]'s
/// answer for `mt`. Keeps the two script rows from reading a file for
/// nothing — the caller pays a container read for every `true` here.
pub(crate) fn signature_can_decide(mt: &str) -> bool {
    matches!(
        base_media_type(mt),
        "application/font-sfnt"
            | "application/vnd.ms-opentype"
            | "application/font-woff"
            | "application/x-font-ttf"
    )
}

/// What OPF-090 should say about a non-preferred Core Media Type.
///
/// Two fields because they answer to different readers. `text` goes into the
/// message a person reads; `media_type` is the machine-readable `params[1]`
/// a repairer acts on, and it is `None` whenever there is no single media
/// type to name.
pub(crate) struct Preferred {
    /// What the message names. May be the two-way hint `font/(ttf|otf)`,
    /// which is not a media type.
    pub(crate) text: &'static str,
    /// The one media type a tool may write, when the answer is one type.
    pub(crate) media_type: Option<&'static str>,
}

/// The preferred spelling of a non-preferred Core Media Type, for OPF-090.
///
/// Requested by Doitsu on MobileRead: epubcheck names the replacement
/// (`It is encouraged to use MIME media type "font/otf" instead of
/// "application/vnd.ms-opentype"`) and we only said the type was
/// non-preferred, which tells the reader they have a problem and not what to
/// do about it.
///
/// The table is EPUB 3.3's §3.2 Core Media Types table read directly — each
/// non-preferred spelling appears on exactly one row, and "the first one is
/// the preferred media type" — which is also what
/// `OPFChecker30.getPreferredMediaType` implements, so parity and
/// conformance agree on every row it has. Two places they do not:
///
/// - **`application/font-sfnt` sits on *both* the TrueType and the OpenType
///   row**, so the table names no single answer and epubcheck guesses from
///   the file extension. We keep that guess, but let the file's own bytes
///   overrule it where RFC 8081 settles the question: only `font/otf` admits
///   `OTTO`.
/// - **`application/x-font-ttf` is in no EPUB table at all** (0 occurrences in
///   the spec text). It is epubcheck's own extension to the Core Media Type
///   set, in the permissive direction, and we follow it.
///
/// Whatever the row says, a spelling the resource's own signature rules out
/// is never named — a wrong machine instruction is worse than none.
pub(crate) fn preferred_media_type(
    mt: &str,
    href: &str,
    sig: Option<FontSignature>,
) -> Option<Preferred> {
    let single = |m: &'static str| Preferred {
        text: m,
        media_type: Some(m),
    };
    let candidate = match base_media_type(mt) {
        "application/font-sfnt" => match sig {
            // The one case where the bytes decide a row the spec leaves open.
            Some(FontSignature::Cff) => single("font/otf"),
            // Both rows admit `glyf`, so we are back to epubcheck's guess.
            Some(FontSignature::Glyf) | None => {
                if href.ends_with(".ttf") {
                    single("font/ttf")
                } else if href.ends_with(".otf") {
                    single("font/otf")
                } else {
                    Preferred {
                        text: "font/(ttf|otf)",
                        media_type: None,
                    }
                }
            }
            // WOFF, WOFF2 and collections: neither row's type admits them.
            Some(_) => return None,
        },
        "application/vnd.ms-opentype" => single("font/otf"),
        "application/font-woff" => single("font/woff"),
        "application/x-font-ttf" => single("font/ttf"),
        "text/javascript" | "application/ecmascript" => single("application/javascript"),
        _ => return None,
    };
    if let (Some(m), Some(s)) = (candidate.media_type, sig)
        && !signature_admits(m, s)
    {
        return None;
    }
    Some(candidate)
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

    /// EPUB 3.4's core media type additions. The audio ones need no entry of
    /// their own — `audio/mp4` is already here and `base_media_type` strips
    /// the parameter — but they are asserted so that a future change to
    /// either cannot silently drop them (spec change log, 14-Apr-2026:
    /// Opus in MP4, plus a codec-bearing type for AAC LC).
    #[test]
    fn epub34_core_media_types() {
        assert!(is_core_media_type("image/avif"));
        assert!(is_core_media_type("image/jxl"));
        assert!(is_core_media_type("audio/mp4; codecs=opus"));
        assert!(is_core_media_type("audio/mp4; codecs=mp4a.40.2"));
        // Additions, not reclassifications: both are preferred types.
        assert!(!is_non_preferred_core_media_type("image/avif"));
        assert!(!is_non_preferred_core_media_type("image/jxl"));
        // And nothing image-shaped came along for the ride.
        assert!(!is_core_media_type("image/heic"));
        assert!(!is_core_media_type("image/tiff"));
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
