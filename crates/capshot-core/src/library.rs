//! Screenshot library: list images in the quick-save folder, newest first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MAX_FILES: usize = 500;
pub const THUMB_MAX: (u32, u32) = (160, 120);
const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub file_name: String,
    pub modified: SystemTime,
}

/// List supported images in `dir`, newest first, at most [`MAX_FILES`].
/// Creates `dir` when it does not exist (winshot does the same).
pub fn scan(dir: &Path) -> std::io::Result<Vec<LibraryEntry>> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        return Ok(Vec::new());
    }
    let mut out: Vec<LibraryEntry> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let path = e.path();
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            EXTENSIONS.contains(&ext.as_str()).then(|| LibraryEntry {
                file_name: e.file_name().to_string_lossy().into_owned(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                path,
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.file_name.cmp(&b.file_name)));
    out.truncate(MAX_FILES);
    Ok(out)
}

/// Fit `orig` inside `max` keeping aspect ratio; never upscales, never below 1 px.
pub fn thumbnail_size(orig: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let (w, h) = orig;
    if w <= max.0 && h <= max.1 {
        return orig;
    }
    let ratio = (max.0 as f64 / w as f64).min(max.1 as f64 / h as f64);
    (((w as f64 * ratio) as u32).max(1), ((h as f64 * ratio) as u32).max(1))
}

/// Decode `path` and return (thumbnail, original width, original height).
pub fn thumbnail(path: &Path) -> image::ImageResult<(image::RgbaImage, u32, u32)> {
    let img = image::open(path)?;
    let (w, h) = (img.width(), img.height());
    let (tw, th) = thumbnail_size((w, h), THUMB_MAX);
    Ok((img.thumbnail(tw, th).to_rgba8(), w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_size_matches_winshot() {
        assert_eq!(thumbnail_size((100, 50), (160, 120)), (100, 50));
        assert_eq!(thumbnail_size((1920, 1080), (160, 120)), (160, 90));
        assert_eq!(thumbnail_size((1080, 1920), (160, 120)), (67, 120));
        assert_eq!(thumbnail_size((10000, 1), (160, 120)), (160, 1));
    }

    #[test]
    fn scan_filters_and_sorts_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, secs: u64| {
            let p = dir.path().join(name);
            std::fs::write(&p, b"x").unwrap();
            let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        };
        write("old.png", 1_000);
        write("new.JPG", 2_000);
        write("notes.txt", 3_000);
        std::fs::create_dir(dir.path().join("sub.png")).unwrap();
        let names: Vec<_> = scan(dir.path()).unwrap().into_iter().map(|e| e.file_name).collect();
        assert_eq!(names, ["new.JPG", "old.png"]);
    }

    #[test]
    fn scan_creates_missing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        assert!(scan(&sub).unwrap().is_empty());
        assert!(sub.is_dir());
    }
}
