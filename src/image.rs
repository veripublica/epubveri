//! Byte-level image format sniffing (magic-number signatures only, no
//! decoding) - used to cross-check a manifest item's declared media-type
//! against its actual file content for the four raster Core Media Types
//! (JPEG/PNG/GIF/WebP). SVG isn't sniffable this way (it's XML, already
//! validated as such elsewhere) and isn't included here.

/// Sniffs an image's real format from its leading bytes, or `None` if the
/// bytes don't match any recognized signature (including empty/truncated
/// files - confirmed via a real corpus fixture using a 0-byte file
/// declared as `image/jpeg`).
pub(crate) fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.len() >= 6
        && &bytes[..3] == b"GIF"
        && (&bytes[3..6] == b"87a" || &bytes[3..6] == b"89a")
    {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The conventional file extensions for a sniffed raster Core Media Type,
/// for the PKG-022 (wrong extension) check.
pub(crate) fn conventional_extensions(mt: &str) -> &'static [&'static str] {
    match mt {
        "image/jpeg" => &["jpg", "jpeg"],
        "image/png" => &["png"],
        "image/gif" => &["gif"],
        "image/webp" => &["webp"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_known_signatures() {
        assert_eq!(
            sniff_image_type(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            sniff_image_type(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(sniff_image_type(b"GIF89a..."), Some("image/gif"));
        assert_eq!(sniff_image_type(b"GIF87a..."), Some("image/gif"));
        assert_eq!(sniff_image_type(b"RIFF\0\0\0\0WEBP..."), Some("image/webp"));
    }

    #[test]
    fn rejects_empty_or_unknown() {
        assert_eq!(sniff_image_type(&[]), None);
        assert_eq!(sniff_image_type(b"not an image"), None);
    }
}
