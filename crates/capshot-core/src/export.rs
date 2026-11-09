//! Encoding and quick-save file naming.

use crate::config::{FilenamePattern, ImageFormat};
use chrono::{DateTime, Local};
use image::RgbaImage;
use std::path::{Path, PathBuf};

pub const FILE_PREFIX: &str = "capshot_";

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not encode image: {0}")]
    Encode(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Encode to PNG or JPEG (`quality` 1..=100). JPEG has no alpha, so pixels are
/// composited over white first, matching a browser's JPEG export.
pub fn encode(img: &RgbaImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>, ExportError> {
    let mut out = Vec::new();
    match format {
        ImageFormat::Png => {
            image::codecs::png::PngEncoder::new(&mut out).write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
        ImageFormat::Jpeg => {
            let rgb = flatten_on_white(img);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)).encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )?;
        }
    }
    Ok(out)
}

use image::ImageEncoder;

fn flatten_on_white(img: &RgbaImage) -> image::RgbImage {
    image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0;
        let a = a as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        image::Rgb([blend(r), blend(g), blend(b)])
    })
}

/// Next free quick-save path in `dir`, using winshot's naming rules with the
/// `capshot_` prefix:
/// timestamp `capshot_2024-01-15_14-30-45.png`, date `capshot_2024-01-15.png`
/// (then `_1`, `_2`, ...), increment `capshot_001.png`, `capshot_002.png`, ...
pub fn quick_save_path(dir: &Path, pattern: FilenamePattern, format: ImageFormat, now: DateTime<Local>) -> PathBuf {
    let ext = format.extension();
    match pattern {
        FilenamePattern::Timestamp => {
            let stamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
            let first = dir.join(format!("{FILE_PREFIX}{stamp}.{ext}"));
            if !first.exists() {
                return first;
            }
            // Two saves in the same second: add a counter rather than overwrite.
            (1..).map(|n| dir.join(format!("{FILE_PREFIX}{stamp}_{n}.{ext}"))).find(|p| !p.exists()).unwrap()
        }
        FilenamePattern::Date => {
            let date = now.format("%Y-%m-%d").to_string();
            let first = dir.join(format!("{FILE_PREFIX}{date}.{ext}"));
            if !first.exists() {
                return first;
            }
            (1..).map(|n| dir.join(format!("{FILE_PREFIX}{date}_{n}.{ext}"))).find(|p| !p.exists()).unwrap()
        }
        FilenamePattern::Increment => {
            (1..).map(|n| dir.join(format!("{FILE_PREFIX}{n:03}.{ext}"))).find(|p| !p.exists()).unwrap()
        }
    }
}

/// Object name for cloud uploads: millisecond timestamp, so successive uploads
/// never overwrite each other (local naming patterns cannot see the bucket).
pub fn upload_name(format: ImageFormat, now: DateTime<Local>) -> String {
    format!("{FILE_PREFIX}{}.{}", now.format("%Y-%m-%d_%H-%M-%S-%3f"), format.extension())
}

/// Encode and write, creating the folder if needed.
pub fn save(img: &RgbaImage, path: &Path, format: ImageFormat, quality: u8) -> Result<(), ExportError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, encode(img, format, quality)?)?;
    Ok(())
}

/// Format implied by a file name chosen in a save dialog (PNG when unknown).
pub fn format_for_path(path: &Path) -> ImageFormat {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg" | "jpeg") => ImageFormat::Jpeg,
        _ => ImageFormat::Png,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2024, 1, 15, 14, 30, 45).unwrap()
    }

    #[test]
    fn timestamp_name() {
        let p = quick_save_path(Path::new("/x"), FilenamePattern::Timestamp, ImageFormat::Png, now());
        assert_eq!(p, Path::new("/x/capshot_2024-01-15_14-30-45.png"));
    }

    #[test]
    fn timestamp_name_avoids_overwrite_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = quick_save_path(dir.path(), FilenamePattern::Timestamp, ImageFormat::Png, now());
        std::fs::write(&p1, b"x").unwrap();
        let p2 = quick_save_path(dir.path(), FilenamePattern::Timestamp, ImageFormat::Png, now());
        assert_eq!(p2.file_name().unwrap(), "capshot_2024-01-15_14-30-45_1.png");
    }

    #[test]
    fn upload_names_have_milliseconds() {
        let t = Local.with_ymd_and_hms(2024, 1, 15, 14, 30, 45).unwrap() + chrono::Duration::milliseconds(7);
        assert_eq!(upload_name(ImageFormat::Jpeg, t), "capshot_2024-01-15_14-30-45-007.jpg");
    }

    #[test]
    fn date_name_avoids_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = quick_save_path(dir.path(), FilenamePattern::Date, ImageFormat::Jpeg, now());
        assert_eq!(p1.file_name().unwrap(), "capshot_2024-01-15.jpg");
        std::fs::write(&p1, b"x").unwrap();
        let p2 = quick_save_path(dir.path(), FilenamePattern::Date, ImageFormat::Jpeg, now());
        assert_eq!(p2.file_name().unwrap(), "capshot_2024-01-15_1.jpg");
    }

    #[test]
    fn increment_name_finds_first_gap() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("capshot_001.png"), b"x").unwrap();
        let p = quick_save_path(dir.path(), FilenamePattern::Increment, ImageFormat::Png, now());
        assert_eq!(p.file_name().unwrap(), "capshot_002.png");
    }

    #[test]
    fn png_and_jpeg_decode_back() {
        let img = RgbaImage::from_pixel(8, 4, image::Rgba([10, 200, 30, 255]));
        let png = encode(&img, ImageFormat::Png, 95).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8(), img);
        let jpg = encode(&img, ImageFormat::Jpeg, 95).unwrap();
        let back = image::load_from_memory(&jpg).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (8, 4));
        let p = back.get_pixel(4, 2).0;
        assert!(p[1] > 180 && p[0] < 40);
    }

    #[test]
    fn jpeg_flattens_transparency_on_white() {
        let img = RgbaImage::new(2, 2);
        assert_eq!(flatten_on_white(&img).get_pixel(0, 0).0, [255, 255, 255]);
    }

    #[test]
    fn format_from_extension() {
        assert_eq!(format_for_path(Path::new("a.JPG")), ImageFormat::Jpeg);
        assert_eq!(format_for_path(Path::new("a")), ImageFormat::Png);
    }
}
