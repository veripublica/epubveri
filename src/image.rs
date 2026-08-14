//! Byte-level image format sniffing (magic-number signatures only, no
//! decoding) - used to cross-check a manifest item's declared media-type
//! against its actual file content for the four raster Core Media Types
//! (JPEG/PNG/GIF/WebP). SVG isn't sniffable this way (it's XML, already
//! validated as such elsewhere) and isn't included here.
//!
//! The table also carries formats that are *not* Core Media Types - TIFF and
//! BMP - because the question this answers is "what is this file really",
//! and the answer decides between two different messages. epubcheck reads the
//! content through `ImageIO.getImageReaders`, whose standard readers cover
//! both; a format it recognises but that disagrees with the file extension is
//! PKG-022 (wrong extension), while a format it cannot identify at all is
//! PKG-021 (corrupt). Not recognising TIFF put a valid TIFF in the second
//! bucket and called a real book's image corrupt (#75).

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
    } else if bytes.starts_with(b"II\x2a\x00") || bytes.starts_with(b"MM\x00\x2a") {
        // Both byte orders, and both are needed: a big-endian (`MM`) TIFF
        // named `.png` draws PKG-022 from epubcheck exactly as the
        // little-endian one does. BigTIFF (version 43) is deliberately not
        // matched - epubcheck's standard readers do not read it either.
        Some("image/tiff")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
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
        "image/tiff" => &["tif", "tiff"],
        "image/bmp" => &["bmp"],
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

    /// #75: a real book carried a valid little-endian TIFF named `.png` and
    /// declared `image/png`. Not recognising it meant reporting PKG-021
    /// ("corrupt") where epubcheck reports PKG-022 ("wrong extension") - an
    /// ERROR against a WARNING, about a file that is not corrupt at all.
    /// Both byte orders and BMP were confirmed against epubcheck one book at
    /// a time before being added here.
    #[test]
    fn sniffs_the_formats_epubcheck_reads_but_epub_does_not_bless() {
        assert_eq!(sniff_image_type(b"II\x2a\x00\x08"), Some("image/tiff"));
        assert_eq!(sniff_image_type(b"MM\x00\x2a\x00"), Some("image/tiff"));
        assert_eq!(sniff_image_type(b"BM\x36\x00"), Some("image/bmp"));
        // The extension tables have to move with them, or a correctly-named
        // file would draw PKG-022 instead.
        assert!(conventional_extensions("image/tiff").contains(&"tif"));
        assert!(conventional_extensions("image/tiff").contains(&"tiff"));
        assert!(conventional_extensions("image/bmp").contains(&"bmp"));
    }

    #[test]
    fn rejects_empty_or_unknown() {
        assert_eq!(sniff_image_type(&[]), None);
        assert_eq!(sniff_image_type(b"not an image"), None);
        // Still None, so the PKG-021 "corrupt" path is unchanged for content
        // that matches nothing - which is what epubcheck reports there too,
        // measured with a garbage file named `.png`.
        assert_eq!(sniff_image_type(b"IInope"), None);
        assert_eq!(sniff_image_type(b"M"), None);
    }
}
