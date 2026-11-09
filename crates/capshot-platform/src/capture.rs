//! Screen, display and window capture on top of `xcap`.

use crate::{PlatformError, Result};
use image::RgbaImage;

fn err(e: impl std::fmt::Display) -> PlatformError {
    PlatformError::Capture(e.to_string())
}

/// One display. Position and size are in the OS's desktop coordinates
/// (physical pixels on Windows and X11, points on macOS).
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

fn info(m: &xcap::Monitor) -> Result<DisplayInfo> {
    Ok(DisplayInfo {
        id: m.id().map_err(err)?,
        name: m.friendly_name().or_else(|_| m.name()).unwrap_or_default(),
        x: m.x().map_err(err)?,
        y: m.y().map_err(err)?,
        width: m.width().map_err(err)?,
        height: m.height().map_err(err)?,
        scale_factor: m.scale_factor().unwrap_or(1.0),
        is_primary: m.is_primary().unwrap_or(false),
    })
}

pub fn displays() -> Result<Vec<DisplayInfo>> {
    xcap::Monitor::all().map_err(err)?.iter().map(info).collect()
}

/// A display and a frozen capture of it (used by the region overlay).
pub struct DisplayCapture {
    pub display: DisplayInfo,
    pub image: RgbaImage,
}

/// Capture every display.
pub fn capture_all_displays() -> Result<Vec<DisplayCapture>> {
    xcap::Monitor::all()
        .map_err(err)?
        .iter()
        .map(|m| Ok(DisplayCapture { display: info(m)?, image: m.capture_image().map_err(err)? }))
        .collect()
}

pub fn capture_display(id: u32) -> Result<RgbaImage> {
    let monitors = xcap::Monitor::all().map_err(err)?;
    let m = monitors
        .iter()
        .find(|m| m.id().ok() == Some(id))
        .ok_or_else(|| PlatformError::NotFound(format!("display {id}")))?;
    m.capture_image().map_err(err)
}

/// The display under the mouse pointer, falling back to the primary one.
pub fn display_at(x: i32, y: i32) -> Result<DisplayInfo> {
    match xcap::Monitor::from_point(x, y) {
        Ok(m) => info(&m),
        Err(_) => {
            displays()?.into_iter().find(|d| d.is_primary).ok_or_else(|| PlatformError::NotFound("no display".into()))
        }
    }
}

/// All displays stitched into one image, laid out like the desktop
/// (winshot's "Fullscreen" mode).
pub fn capture_virtual_screen() -> Result<RgbaImage> {
    stitch(capture_all_displays()?)
}

/// Lay captures out by their desktop position. Desktop coordinates are
/// converted with one common density (the highest), and lower-density captures
/// are scaled up to it, so mixed-DPI setups (Retina + 1x on macOS) neither
/// overlap nor leave gaps. On Windows and X11 positions are already pixels and
/// every density is 1.
pub fn stitch(parts: Vec<DisplayCapture>) -> Result<RgbaImage> {
    if parts.is_empty() {
        return Err(PlatformError::NotFound("no display".into()));
    }
    if parts.len() == 1 {
        return Ok(parts.into_iter().next().unwrap().image);
    }
    let density = |p: &DisplayCapture| p.image.width() as f32 / p.display.width.max(1) as f32;
    let common = parts.iter().map(density).fold(1.0f32, f32::max);
    let parts: Vec<DisplayCapture> = parts
        .into_iter()
        .map(|mut p| {
            let s = common / density(&p);
            if (s - 1.0).abs() > 0.01 {
                let (w, h) =
                    ((p.image.width() as f32 * s).round() as u32, (p.image.height() as f32 * s).round() as u32);
                p.image = image::imageops::resize(&p.image, w, h, image::imageops::FilterType::Triangle);
            }
            p
        })
        .collect();
    let place = |p: &DisplayCapture| {
        ((p.display.x as f32 * common).round() as i64, (p.display.y as f32 * common).round() as i64)
    };
    let min_x = parts.iter().map(|p| place(p).0).min().unwrap();
    let min_y = parts.iter().map(|p| place(p).1).min().unwrap();
    let max_x = parts.iter().map(|p| place(p).0 + p.image.width() as i64).max().unwrap();
    let max_y = parts.iter().map(|p| place(p).1 + p.image.height() as i64).max().unwrap();
    let mut out = RgbaImage::from_pixel((max_x - min_x) as u32, (max_y - min_y) as u32, image::Rgba([0, 0, 0, 255]));
    for p in &parts {
        let (x, y) = place(p);
        image::imageops::replace(&mut out, &p.image, x - min_x, y - min_y);
    }
    Ok(out)
}

/// Visible, capturable top-level windows, excluding capshot's own.
pub fn windows() -> Result<Vec<WindowInfo>> {
    let own = std::process::id();
    let mut out = Vec::new();
    for w in xcap::Window::all().map_err(err)? {
        if w.pid().ok() == Some(own) || w.is_minimized().unwrap_or(false) {
            continue;
        }
        let (Ok(width), Ok(height)) = (w.width(), w.height()) else { continue };
        let title = w.title().unwrap_or_default();
        if width < 20 || height < 20 || title.trim().is_empty() {
            continue;
        }
        out.push(WindowInfo {
            id: w.id().map_err(err)?,
            title,
            app_name: w.app_name().unwrap_or_default(),
            x: w.x().unwrap_or(0),
            y: w.y().unwrap_or(0),
            width,
            height,
        });
    }
    Ok(out)
}

pub fn capture_window(id: u32) -> Result<RgbaImage> {
    let all = xcap::Window::all().map_err(err)?;
    let w =
        all.iter().find(|w| w.id().ok() == Some(id)).ok_or_else(|| PlatformError::NotFound(format!("window {id}")))?;
    w.capture_image().map_err(err)
}

/// Small preview for the window picker, at most `max` on its longer side.
pub fn window_thumbnail(id: u32, max: u32) -> Result<RgbaImage> {
    let img = capture_window(id)?;
    Ok(image::DynamicImage::ImageRgba8(img).thumbnail(max, max).to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(x: i32, y: i32, w: u32, h: u32, px_w: u32, px_h: u32, color: u8) -> DisplayCapture {
        DisplayCapture {
            display: DisplayInfo {
                id: 0,
                name: String::new(),
                x,
                y,
                width: w,
                height: h,
                scale_factor: 1.0,
                is_primary: false,
            },
            image: RgbaImage::from_pixel(px_w, px_h, image::Rgba([color, 0, 0, 255])),
        }
    }

    #[test]
    fn stitch_side_by_side() {
        let out = stitch(vec![part(0, 0, 100, 50, 100, 50, 10), part(100, 0, 100, 80, 100, 80, 20)]).unwrap();
        assert_eq!(out.dimensions(), (200, 80));
        assert_eq!(out.get_pixel(50, 10).0[0], 10);
        assert_eq!(out.get_pixel(150, 70).0[0], 20);
        assert_eq!(out.get_pixel(50, 70).0, [0, 0, 0, 255], "gap below the shorter display is black");
    }

    #[test]
    fn stitch_handles_negative_origin() {
        let out = stitch(vec![part(-100, 0, 100, 50, 100, 50, 10), part(0, 0, 100, 50, 100, 50, 20)]).unwrap();
        assert_eq!(out.dimensions(), (200, 50));
        assert_eq!(out.get_pixel(10, 10).0[0], 10);
    }

    #[test]
    fn mixed_density_displays_do_not_overlap() {
        // A 2x display (100 points wide, 200 px) left of a 1x display at x = 100 points.
        let out = stitch(vec![part(0, 0, 100, 50, 200, 100, 10), part(100, 0, 100, 50, 100, 50, 20)]).unwrap();
        assert_eq!(out.dimensions(), (400, 100));
        assert_eq!(out.get_pixel(150, 50).0[0], 10, "built-in display is intact");
        assert_eq!(out.get_pixel(350, 50).0[0], 20, "1x display is scaled up beside it");
    }

    #[test]
    fn single_display_is_returned_as_is() {
        let out = stitch(vec![part(0, 0, 10, 10, 20, 20, 7)]).unwrap();
        assert_eq!(out.dimensions(), (20, 20));
    }
}
