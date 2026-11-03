//! Images a kernel sends: base64 `image/png` or `image/jpeg`, decoded and
//! checked before anything is written or shown.
//!
//! A kernel is not trusted to send sane images, so [`decode`] refuses data
//! whose base64 text or decoded bytes pass [`MAX_IMAGE_BYTES`], data that is
//! not the image type it claims (the PNG signature or the JPEG start
//! marker), and an image whose header gives a side over [`MAX_SIDE`] or
//! more than [`MAX_PIXELS`] pixels in all. The size is read from the PNG
//! `IHDR` chunk or the JPEG start-of-frame segment, so a "decompression
//! bomb" is refused before any pixel is decoded.

use base64::Engine as _;
use sha2::{Digest, Sha256};

/// The most bytes a decoded image may have (16 MiB).
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// The longest side an image may have, in pixels.
pub const MAX_SIDE: u32 = 16_384;
/// The most pixels an image may have in all (16 megapixels, 64 MiB as
/// RGBA once decoded).
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;

/// Why an image was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageError {
    /// The data is over [`MAX_IMAGE_BYTES`].
    TooLarge,
    /// The image's width or height is over [`MAX_SIDE`], or its pixel
    /// count over [`MAX_PIXELS`].
    TooManyPixels {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// Not base64, not the claimed type, or no readable size.
    Invalid,
    /// A MIME type other than `image/png` or `image/jpeg`.
    Unsupported,
}

impl std::fmt::Display for ImageError {
    /// Renders a fixed human-readable message for the error.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "image over {MAX_IMAGE_BYTES} bytes"),
            Self::TooManyPixels { width, height } => {
                write!(f, "image of {width}×{height} pixels is too large")
            }
            Self::Invalid => f.write_str("not valid image data"),
            Self::Unsupported => f.write_str("unsupported image type"),
        }
    }
}

impl std::error::Error for ImageError {}

/// A decoded, checked image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// `image/png` or `image/jpeg`.
    pub mime: &'static str,
    /// The file bytes.
    pub bytes: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Image {
    /// The file extension for its type, without the dot.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        if self.mime == "image/jpeg" {
            "jpg"
        } else {
            "png"
        }
    }

    /// A stable file name from the content: the first 16 hex digits of its
    /// SHA-256, then the extension.
    #[must_use]
    pub fn file_name(&self) -> String {
        let digest = Sha256::digest(&self.bytes);
        let mut name = digest[..8]
            .iter()
            .fold(String::with_capacity(20), |mut out, b| {
                use std::fmt::Write as _;
                let _ = write!(out, "{b:02x}");
                out
            });
        name.push('.');
        name.push_str(self.extension());
        name
    }
}

/// Decode and check base64 image data of type `mime`.
///
/// # Errors
/// An [`ImageError`] when the data is too large, invalid, or of an
/// unsupported type.
pub fn decode(mime: &str, base64_text: &str) -> Result<Image, ImageError> {
    let mime: &'static str = match mime {
        "image/png" => "image/png",
        "image/jpeg" => "image/jpeg",
        _ => return Err(ImageError::Unsupported),
    };
    // Four base64 characters carry three bytes: refuse before decoding.
    if base64_text.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return Err(ImageError::TooLarge);
    }
    let compact: String = base64_text
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|_| ImageError::Invalid)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge);
    }
    let (width, height) = if mime == "image/png" {
        png_size(&bytes)
    } else {
        jpeg_size(&bytes)
    }
    .ok_or(ImageError::Invalid)?;
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(ImageError::TooManyPixels { width, height });
    }
    Ok(Image {
        mime,
        bytes,
        width,
        height,
    })
}

/// Big-endian `u32` at `at`, or `None` if the slice is too short.
fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Big-endian `u16` at `at`, or `None` if the slice is too short.
fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

/// Width and height from a PNG's signature and `IHDR` chunk.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    /// The 8-byte PNG file signature.
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.get(..8)? != SIGNATURE || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be_u32(bytes, 16)?, be_u32(bytes, 20)?))
}

/// Width and height from a JPEG's first start-of-frame segment.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut i = 2;
    loop {
        if *bytes.get(i)? != 0xff {
            return None;
        }
        let mut marker = *bytes.get(i + 1)?;
        i += 1;
        while marker == 0xff {
            i += 1;
            marker = *bytes.get(i)?;
        }
        i += 1;
        // Markers without a length: TEM, RST0–7.
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        // End of image or start of scan before any frame header.
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        let len = usize::from(be_u16(bytes, i)?);
        let is_sof = (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if is_sof {
            let height = be_u16(bytes, i + 3)?;
            let width = be_u16(bytes, i + 5)?;
            return Some((u32::from(width), u32::from(height)));
        }
        if len < 2 {
            return None;
        }
        i += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    /// A PNG signature and `IHDR` for a `w`×`h` image (no pixel data).
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&w.to_be_bytes());
        out.extend_from_slice(&h.to_be_bytes());
        out.extend_from_slice(&[8, 6, 0, 0, 0]);
        out
    }

    #[test]
    fn png_size_and_name() {
        let image = decode("image/png", &b64(&png_header(3, 2))).unwrap();
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(image.extension(), "png");
        let name = image.file_name();
        assert_eq!(name.len(), 20);
        assert_eq!(name.rsplit_once('.').map(|(_, e)| e), Some("png"));
    }

    #[test]
    fn jpeg_size_skips_segments() {
        // SOI, APP0 (length 4: two data bytes), padding 0xff, SOF0.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xff, 0xc0, 0x00, 0x11, 0x08,
            0x00, 0x20, 0x00, 0x40, 0x03,
        ];
        let image = decode("image/jpeg", &b64(&jpeg)).unwrap();
        assert_eq!((image.width, image.height), (64, 32));
        assert_eq!(image.extension(), "jpg");
    }

    #[test]
    fn refuses_bombs_lies_and_junk() {
        assert_eq!(
            decode("image/png", &b64(&png_header(100_000, 10))),
            Err(ImageError::TooManyPixels {
                width: 100_000,
                height: 10
            })
        );
        assert_eq!(
            decode("image/png", &b64(&png_header(10_000, 10_000))),
            Err(ImageError::TooManyPixels {
                width: 10_000,
                height: 10_000
            })
        );
        assert_eq!(
            decode("image/png", &b64(&png_header(0, 5))),
            Err(ImageError::TooManyPixels {
                width: 0,
                height: 5
            })
        );
        // A JPEG labelled as PNG, junk, and not base64.
        assert_eq!(
            decode("image/png", &b64(&[0xff, 0xd8, 0xff])),
            Err(ImageError::Invalid)
        );
        assert_eq!(
            decode("image/jpeg", &b64(b"hello")),
            Err(ImageError::Invalid)
        );
        assert_eq!(decode("image/png", "!!!"), Err(ImageError::Invalid));
        assert_eq!(decode("image/gif", ""), Err(ImageError::Unsupported));
    }

    #[test]
    fn refuses_oversized_data_before_decoding() {
        let huge = "A".repeat((MAX_IMAGE_BYTES / 3 + 2) * 4);
        assert_eq!(decode("image/png", &huge), Err(ImageError::TooLarge));
    }
}
