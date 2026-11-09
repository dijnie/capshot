//! Clipboard images: read a bitmap or a copied image file, write PNG-able RGBA.

use crate::{PlatformError, Result};
use image::RgbaImage;
use std::path::Path;

fn err(e: impl std::fmt::Display) -> PlatformError {
    PlatformError::Clipboard(e.to_string())
}

const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"];

pub fn is_image_path(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Image on the clipboard: bitmap data first, then the first copied image file.
pub fn read_image() -> Result<RgbaImage> {
    with_clipboard(read_from)
}

fn read_from(cb: &mut arboard::Clipboard) -> Result<RgbaImage> {
    if let Ok(data) = cb.get_image() {
        return RgbaImage::from_raw(data.width as u32, data.height as u32, data.bytes.into_owned())
            .ok_or_else(|| err("clipboard image has an unexpected size"));
    }
    if let Ok(files) = cb.get().file_list() {
        if let Some(path) = files.iter().find(|p| is_image_path(p)) {
            return Ok(image::open(path)?.to_rgba8());
        }
    }
    Err(PlatformError::NotFound("No image on the clipboard".into()))
}

/// One clipboard for the whole app. On X11 and Wayland the copying process
/// serves the data, so dropping the handle right after `set` loses it.
fn with_clipboard<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<T>) -> Result<T> {
    static CB: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut guard = CB.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().map_err(err)?);
    }
    f(guard.as_mut().unwrap())
}

pub fn write_image(img: &RgbaImage) -> Result<()> {
    with_clipboard(|cb| {
        cb.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Borrowed(img.as_raw()),
        })
        .map_err(err)
    })
}

pub fn write_text(text: &str) -> Result<()> {
    with_clipboard(|cb| cb.set_text(text).map_err(err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_path_detection() {
        assert!(is_image_path(Path::new("a/b.PNG")));
        assert!(is_image_path(Path::new("x.webp")));
        assert!(!is_image_path(Path::new("notes.txt")));
        assert!(!is_image_path(Path::new("noext")));
    }
}
