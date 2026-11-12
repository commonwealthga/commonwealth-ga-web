//! Accepting an image without trusting it.
//!
//! The rule this file exists to enforce: **the bytes a player uploads are never
//! stored and never served.** An upload is decoded, scaled and re-encoded into a
//! fresh file, and only that derivative reaches the disk. Everything the original
//! carried — EXIF, XMP, ICC, text chunks, data appended after the image's end
//! marker, any structure that made the file two things at once — is gone,
//! because none of it survives a decode.
//!
//! Belt and braces beyond that:
//!
//! * the format is sniffed from the bytes, not trusted from a `Content-Type` or
//!   a file name;
//! * only PNG, JPEG and WebP are accepted. SVG is refused by omission: it is a
//!   document that can carry script, not an image;
//! * dimensions and total allocation are capped *before* decoding, so a
//!   decompression bomb cannot take the process down;
//! * the output is content-addressed, so identical screenshots share one file,
//!   and served with `nosniff` and an immutable cache.
//!
//! Decoding is pure Rust (`image`), not a subprocess: the boundary is a library
//! with limits rather than a second process to supervise.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};

/// What we will read. A screenshot is a few hundred kilobytes; four megabytes is
/// already generous for a 4K capture.
pub const MAX_UPLOAD_BYTES: usize = 4 * 1024 * 1024;
/// Refuse anything with a side longer than this before allocating pixels.
pub const MAX_DIMENSION: u32 = 4_000;
/// Builds are shown in a tile and at most ~800 px wide on the detail page, so
/// twice that keeps them crisp on a retina screen.
pub const MAX_WIDTH: u32 = 1_600;
/// A stored image that is still bigger than this after re-encoding is not a
/// screenshot; step the width down until it fits.
pub const MAX_STORED_BYTES: usize = 3 * 1024 * 1024;
/// The list tiles show a build at roughly 220 px wide, so this is its @2x. A
/// 1.5 MB full-size screenshot in a thumbnail is most of a phone's data budget
/// for one screen.
pub const TILE_WIDTH: u32 = 480;
const WIDTHS: [u32; 3] = [MAX_WIDTH, 1_280, 1_024];

/// Uploads allowed per account per window. Generous for a person posting builds,
/// small enough that the disk is not free real estate.
pub const UPLOAD_LIMIT: i64 = 30;
pub const UPLOAD_WINDOW_SECS: i64 = 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    Empty,
    TooLarge,
    Unsupported,
    Undecodable,
    TooLargeAfterEncode,
}

impl ImageError {
    pub fn message(self) -> &'static str {
        match self {
            ImageError::Empty => "That file is empty.",
            ImageError::TooLarge => "That image is too large — 4 MB at most.",
            ImageError::Unsupported => "Only PNG, JPEG and WebP images are accepted.",
            ImageError::Undecodable => "That file could not be read as an image.",
            ImageError::TooLargeAfterEncode => {
                "That image is too big to store — try a smaller crop."
            }
        }
    }
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ImageError {}

/// A re-encoded image, ready to store. The name is the hash of the bytes, so the
/// same screenshot uploaded twice costs one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitised {
    pub hash: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Sanitised {
    /// `<hash>.webp` — the full-size derivative.
    pub fn file_name(&self) -> String {
        format!("{}.webp", self.hash)
    }

    /// `<hash>.tile.webp` — the same image at tile width, for lists.
    pub fn tile_file_name(&self) -> String {
        format!("{}.tile.webp", self.hash)
    }
}

/// Decode, downscale, re-encode. Nothing from the input is carried over.
pub fn sanitise(input: &[u8]) -> Result<Sanitised, ImageError> {
    if input.is_empty() {
        return Err(ImageError::Empty);
    }
    if input.len() > MAX_UPLOAD_BYTES {
        return Err(ImageError::TooLarge);
    }

    let reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|_| ImageError::Undecodable)?;

    // The format comes from the bytes. A file named `.png` that is not one, or a
    // `Content-Type` that lies, gets no further than here.
    match reader.format() {
        Some(ImageFormat::Png) | Some(ImageFormat::Jpeg) | Some(ImageFormat::WebP) => {}
        _ => return Err(ImageError::Unsupported),
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    // Room for RGBA at the dimension cap, and not a byte more: a crafted header
    // claiming 4000×4000 with a bomb behind it fails here rather than in the OOM
    // killer.
    limits.max_alloc = Some(u64::from(MAX_DIMENSION) * u64::from(MAX_DIMENSION) * 4);
    let mut reader = reader;
    reader.limits(limits);

    let decoded: DynamicImage = reader.decode().map_err(|_| ImageError::Undecodable)?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err(ImageError::Undecodable);
    }

    // Step down until the encoded file fits. Skipping the first step when the
    // image is already narrow avoids upscaling a small screenshot.
    for width in WIDTHS {
        let scaled = if decoded.width() > width {
            decoded.resize(width, u32::MAX, image::imageops::FilterType::Lanczos3)
        } else {
            decoded.clone()
        };
        let bytes = encode_webp(&scaled)?;
        if bytes.len() <= MAX_STORED_BYTES {
            return Ok(Sanitised {
                hash: hex(&bytes),
                width: scaled.width(),
                height: scaled.height(),
                bytes,
            });
        }
    }

    Err(ImageError::TooLargeAfterEncode)
}

fn encode_webp(image: &DynamicImage) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
    encoder
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            ExtendedColorType::from(image.color()),
        )
        .map_err(|_| ImageError::Undecodable)?;
    Ok(out)
}

fn hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Write the full-size derivative under `<dir>/<hash>.webp`. Idempotent: the
/// same image twice finds the file already there and writes nothing.
pub async fn save(dir: &Path, image: &Sanitised) -> std::io::Result<PathBuf> {
    tokio::fs::create_dir_all(dir).await?;
    write_once(dir, &image.hash, image.file_name(), &image.bytes).await?;
    Ok(dir.join(image.file_name()))
}

async fn write_once(dir: &Path, hash: &str, name: String, bytes: &[u8]) -> std::io::Result<()> {
    let path = dir.join(name);
    if tokio::fs::metadata(&path).await.is_ok() {
        return Ok(());
    }
    // Write beside the target and rename: a half-written file must never be
    // servable, and the name is a hash of the content it is supposed to have.
    let temp = dir.join(format!("{hash}.{}.part", std::process::id()));
    tokio::fs::write(&temp, bytes).await?;
    tokio::fs::rename(&temp, &path).await?;
    Ok(())
}

/// Produce the tile derivative from one already stored, and write it.
///
/// Used at startup to fill in tiles for images stored before they existed, and
/// after a fresh upload. Returns whether anything was written.
pub fn ensure_tile(dir: &Path, hash: &str) -> std::io::Result<bool> {
    if !name_is_valid(&format!("{hash}.webp")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a stored image name",
        ));
    }
    let tile = dir.join(format!("{hash}.tile.webp"));
    if tile.exists() {
        return Ok(false);
    }

    let full = std::fs::read(dir.join(format!("{hash}.webp")))?;
    // Re-encoding a derivative of a derivative: the source is already ours, and
    // the same decoder applies.
    let decoded = match ImageReader::new(Cursor::new(&full))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.decode().ok())
    {
        Some(image) => image,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stored image is unreadable",
            ));
        }
    };
    let scaled = if decoded.width() > TILE_WIDTH {
        decoded.resize(TILE_WIDTH, u32::MAX, image::imageops::FilterType::Lanczos3)
    } else {
        decoded
    };
    let bytes = encode_webp(&scaled).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "could not encode the tile")
    })?;

    let temp = dir.join(format!("{hash}.tile.{}.part", std::process::id()));
    std::fs::write(&temp, &bytes)?;
    std::fs::rename(&temp, &tile)?;
    Ok(true)
}

/// Accept only the names this module produces. A path, a traversal attempt or a
/// different extension never reaches the filesystem.
pub fn name_is_valid(name: &str) -> bool {
    // `<hash>.webp` and `<hash>.tile.webp` are the only two names produced here.
    let hash = name
        .strip_suffix(".tile.webp")
        .or_else(|| name.strip_suffix(".webp"));
    let Some(hash) = hash else {
        return false;
    };
    hash.len() == 64
        && hash
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, Rgba, RgbaImage};

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
        }
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("encode png");
        out
    }

    #[test]
    fn a_png_comes_back_as_webp_and_loses_its_shape() {
        let source = png(320, 200);
        let image = sanitise(&source).expect("accepted");
        assert_eq!(image.width, 320);
        assert_eq!(image.height, 200);
        // WebP container: "RIFF" .... "WEBP"
        assert_eq!(&image.bytes[0..4], b"RIFF");
        assert_eq!(&image.bytes[8..12], b"WEBP");
        assert_eq!(image.file_name(), format!("{}.webp", image.hash));
    }

    #[test]
    fn data_appended_after_the_image_does_not_survive() {
        // The classic polyglot: a valid PNG with a payload after the end marker.
        // A browser sniffs the image and shows it; another consumer reads the
        // tail. Re-encoding is what makes that impossible.
        let mut source = png(64, 64);
        let payload = b"<?php system($_GET['c']); ?>";
        source.extend_from_slice(payload);

        let image = sanitise(&source).expect("accepted");
        assert!(
            !image
                .bytes
                .windows(payload.len())
                .any(|window| window == payload),
            "the appended payload survived re-encoding"
        );
    }

    #[test]
    fn a_wide_screenshot_is_scaled_down_to_the_tile_width() {
        let image = sanitise(&png(2_000, 900)).expect("accepted");
        assert_eq!(image.width, MAX_WIDTH);
        assert_eq!(image.height, 720);
    }

    #[test]
    fn svg_and_text_are_refused() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert_eq!(sanitise(svg), Err(ImageError::Unsupported));
        assert_eq!(
            sanitise(b"not an image at all"),
            Err(ImageError::Unsupported)
        );
        assert_eq!(sanitise(b""), Err(ImageError::Empty));
    }

    #[test]
    fn an_absurd_dimension_is_refused_before_decoding() {
        // A PNG header claiming 60000×60000 with nothing behind it: the limit
        // stops it before pixels are allocated.
        let mut source = png(8, 8);
        let huge = 60_000u32.to_be_bytes();
        source[16..20].copy_from_slice(&huge);
        source[20..24].copy_from_slice(&huge);
        assert_eq!(sanitise(&source), Err(ImageError::Undecodable));
    }

    /// Deterministic noise: unlike a gradient, it does not compress to almost
    /// nothing, so scaling it down genuinely reduces the file.
    fn noisy_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};
        let mut image = RgbaImage::new(width, height);
        let mut state: u32 = 0x9e37_79b9;
        for pixel in image.pixels_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *pixel = Rgba([
                (state & 0xff) as u8,
                ((state >> 8) & 0xff) as u8,
                ((state >> 16) & 0xff) as u8,
                255,
            ]);
        }
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("encode");
        out
    }

    #[tokio::test]
    async fn a_tile_is_written_beside_the_full_size_and_is_smaller() {
        let dir = tempfile::TempDir::new().expect("temp");
        let image = sanitise(&noisy_png(1_200, 800)).expect("accepted");
        save(dir.path(), &image).await.expect("save");
        assert!(ensure_tile(dir.path(), &image.hash).expect("tile"));

        let full = std::fs::read(dir.path().join(image.file_name())).expect("full");
        let tile = std::fs::read(dir.path().join(image.tile_file_name())).expect("tile");
        assert!(tile.len() < full.len(), "{} vs {}", tile.len(), full.len());

        // 480 px wide, keeping the aspect ratio.
        let decoded = ImageReader::new(Cursor::new(&tile))
            .with_guessed_format()
            .expect("format")
            .decode()
            .expect("decode");
        assert_eq!(decoded.width(), TILE_WIDTH);
        assert_eq!(decoded.height(), 320);

        // Idempotent, and the tile name is servable.
        assert!(!ensure_tile(dir.path(), &image.hash).expect("second run"));
        assert!(name_is_valid(&image.tile_file_name()));
        assert!(!name_is_valid(&format!("{}.tile.png", image.hash)));
    }

    #[test]
    fn only_the_names_this_module_writes_are_served() {
        let image = sanitise(&png(16, 16)).expect("accepted");
        assert!(name_is_valid(&image.file_name()));
        for bad in ["../../etc/passwd", "image.png", "abc.webp", "AABB.webp"] {
            assert!(!name_is_valid(bad), "{bad}");
        }
    }
}
