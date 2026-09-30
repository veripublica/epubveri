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

/// What epubcheck's `BitmapChecker` would make of an image's header: whether
/// the reader it hands the file to can get the image's width and height.
///
/// **This is the whole of epubcheck's PKG-021 for a file whose signature
/// matches.** It asks `ImageReader.getWidth` / `getHeight`, which read the
/// header and stop, and reports PKG-021 when they throw; it never decodes the
/// pixels. So a JPEG cut short *after* its SOF segment, or a PNG with no
/// `IDAT` or `IEND`, draws nothing there, and must draw nothing here: looking
/// for the end of the file would report books epubcheck passes. Measured
/// against 5.4.0 one book per shape (#138).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Header {
    /// The dimensions are there, or the bytes do not settle it for certain.
    Readable,
    /// The reader would throw: epubcheck reports PKG-021.
    Unreadable,
    /// `head` ended before the answer, and it is not the whole file.
    NeedMore,
}

/// [`Header`] for a sniffed `format`, from the file's first bytes. `complete`
/// says `head` is the whole file, so running out of bytes is the answer
/// rather than a reason to read more.
///
/// Every doubtful case answers [`Header::Readable`]: this may only ever report
/// what epubcheck reports.
pub(crate) fn header(format: &str, head: &[u8], complete: bool) -> Header {
    let short = if complete {
        Header::Unreadable
    } else {
        Header::NeedMore
    };
    match format {
        "image/jpeg" => jpeg_header(head, short),
        "image/png" => png_header(head, short),
        "image/gif" => gif_header(head, short),
        // epubcheck ships no WebP reader and reports nothing for a truncated
        // WebP; TIFF and BMP are not Core Media Types and are left alone.
        _ => Header::Readable,
    }
}

/// A complete SOF segment anywhere in the bytes. A raw scan rather than a
/// walk of the segment chain, on purpose: the reader epubcheck bundles
/// (TwelveMonkeys) skips junk a strict walk would stop at, and a raw scan can
/// only find a SOF the reader might not, never miss one it would find.
fn jpeg_header(b: &[u8], short: Header) -> Header {
    const SOF: [u8; 13] = [
        0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF,
    ];
    for i in 0..b.len().saturating_sub(3) {
        if b[i] == 0xFF && SOF.contains(&b[i + 1]) {
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            if i + 2 + len <= b.len() {
                return Header::Readable;
            }
        }
    }
    short
}

/// The IHDR chunk, checked as the JDK's PNG reader checks it before it will
/// report a size: length 13, type `IHDR`, positive width and height, a legal
/// bit depth and colour type and a legal pair of them, compression and filter
/// method 0, interlace 0 or 1. Not the CRC: that reader does not check it
/// there (measured).
fn png_header(b: &[u8], short: Header) -> Header {
    if b.len() < 29 {
        return short;
    }
    let be = |i: usize| i32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (len, ty) = (be(8), &b[12..16]);
    let (width, height) = (be(16), be(20));
    let (depth, colour, compression, filter, interlace) = (b[24], b[25], b[26], b[27], b[28]);
    let depth_ok = matches!(depth, 1 | 2 | 4 | 8 | 16);
    let colour_ok = matches!(colour, 0 | 2 | 3 | 4 | 6);
    let pair_ok = match colour {
        3 => depth != 16,
        2 | 4 | 6 => depth == 8 || depth == 16,
        _ => true,
    };
    if len != 13
        || ty != b"IHDR"
        || width <= 0
        || height <= 0
        || !depth_ok
        || !colour_ok
        || !pair_ok
        || compression != 0
        || filter != 0
        || interlace > 1
    {
        Header::Unreadable
    } else {
        Header::Readable
    }
}

/// The first image descriptor, reached the way the JDK's GIF reader reaches
/// it: past the logical screen descriptor, the global colour table and any
/// extension blocks. A trailer before any image is unreadable; a byte that is
/// none of the three block introducers is left to the reader.
fn gif_header(b: &[u8], short: Header) -> Header {
    if b.len() < 13 {
        return short;
    }
    let mut pos = 13;
    if b[10] & 0x80 != 0 {
        pos += 3 << ((b[10] & 0x07) + 1);
    }
    loop {
        let Some(&intro) = b.get(pos) else {
            return short;
        };
        match intro {
            0x2C => {
                return if pos + 10 <= b.len() {
                    Header::Readable
                } else {
                    short
                };
            }
            0x3B => return Header::Unreadable,
            0x21 => {
                pos += 2;
                loop {
                    let Some(&size) = b.get(pos) else {
                        return short;
                    };
                    pos += 1 + usize::from(size);
                    if size == 0 {
                        break;
                    }
                }
            }
            _ => return Header::Readable,
        }
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

    /// `header` answers what epubcheck 5.4.0 answered, one probed book per
    /// row (#138): unreadable only before the dimensions, never for a file
    /// cut short after them.
    #[test]
    fn a_header_is_unreadable_only_before_the_dimensions() {
        use super::Header::{NeedMore, Readable, Unreadable};
        let h = |f: &str, b: &[u8]| header(f, b, true);

        // JPEG: SOI + APP0, cut before any SOF (the issue's 11 bytes).
        let app0 = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00";
        assert_eq!(h("image/jpeg", app0), Unreadable);
        let sof = b"\xff\xc0\x00\x11\x08\x00\x01\x00\x01\x03\x01\x11\x00\x02\x11\x01\x03\x11\x01";
        let mut cut_after_sof = b"\xff\xd8".to_vec();
        cut_after_sof.extend_from_slice(sof);
        assert_eq!(h("image/jpeg", &cut_after_sof), Readable, "no EOI is fine");
        assert_eq!(
            h("image/jpeg", &cut_after_sof[..cut_after_sof.len() - 1]),
            Unreadable
        );
        // Not the whole file: read more rather than decide.
        assert_eq!(header("image/jpeg", app0, false), NeedMore);

        // PNG: 29 bytes of signature + IHDR is enough; its fields are checked.
        let png = |w: u32, depth: u8, colour: u8| {
            let mut b = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
            b.extend_from_slice(&w.to_be_bytes());
            b.extend_from_slice(&1u32.to_be_bytes());
            b.extend_from_slice(&[depth, colour, 0, 0, 0]);
            b
        };
        assert_eq!(
            h("image/png", &png(1, 8, 2)),
            Readable,
            "IHDR only, no IEND"
        );
        assert_eq!(h("image/png", &png(1, 8, 2)[..28]), Unreadable);
        assert_eq!(h("image/png", &png(0, 8, 2)), Unreadable, "width 0");
        assert_eq!(h("image/png", &png(1, 3, 0)), Unreadable, "bit depth 3");
        assert_eq!(
            h("image/png", &png(1, 4, 2)),
            Unreadable,
            "RGB needs 8 or 16"
        );
        assert_eq!(
            h("image/png", &png(1, 16, 3)),
            Unreadable,
            "palette cannot be 16"
        );
        assert_eq!(h("image/png", &png(1, 1, 0)), Readable, "grey at 1 bit");

        // GIF: the first image descriptor must be complete.
        let screen = b"GIF89a\x01\x00\x01\x00\x00\x00\x00";
        let desc = b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00";
        assert_eq!(h("image/gif", &[&screen[..], desc].concat()), Readable);
        assert_eq!(
            h("image/gif", &[&screen[..], &desc[..5]].concat()),
            Unreadable
        );
        assert_eq!(
            h("image/gif", &[&screen[..], b"\x3b"].concat()),
            Unreadable,
            "trailer first"
        );
        assert_eq!(h("image/gif", &screen[..12]), Unreadable);
        let ext = b"\x21\xf9\x04\x00\x00\x00\x00\x00";
        assert_eq!(h("image/gif", &[&screen[..], ext, desc].concat()), Readable);

        // epubcheck has no WebP reader and reports nothing.
        assert_eq!(h("image/webp", b"RIFF\x00\x00\x00\x00WEBP"), Readable);
    }
}
