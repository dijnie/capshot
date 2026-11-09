//! Text layout and rasterising through cosmic-text. A bundled Inter font is
//! always available, so text renders the same on machines without the
//! requested family.

use crate::annotation::{FontStyle, TextAlign};
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const FALLBACK_FAMILY: &str = "Inter";
static INTER: &[u8] = include_bytes!("../../../../assets/fonts/Inter.ttf");
static INTER_ITALIC: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Italic.ttf");

struct Fonts {
    system: FontSystem,
    cache: SwashCache,
}

fn fonts() -> MutexGuard<'static, Fonts> {
    static FONTS: OnceLock<Mutex<Fonts>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            if std::env::var_os("CAPSHOT_NO_SYSTEM_FONTS").is_none() {
                db.load_system_fonts();
            }
            db.load_font_data(INTER.to_vec());
            db.load_font_data(INTER_ITALIC.to_vec());
            Mutex::new(Fonts {
                system: FontSystem::new_with_locale_and_db("en-US".into(), db),
                cache: SwashCache::new(),
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Font families installed on this system, sorted, for the font picker.
pub fn families() -> Vec<String> {
    let f = fonts();
    let mut v: Vec<String> =
        f.system.db().faces().filter_map(|face| face.families.first().map(|(n, _)| n.clone())).collect();
    v.sort();
    v.dedup();
    v
}

fn resolve_family<'a>(system: &FontSystem, family: &'a str) -> &'a str {
    let known = system.db().faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)));
    if known {
        family
    } else {
        FALLBACK_FAMILY
    }
}

fn layout(f: &mut Fonts, text: &str, size: f32, family: &str, style: FontStyle) -> (Buffer, f32, f32) {
    let family = resolve_family(&f.system, family).to_string();
    // Konva uses a line height equal to the font size.
    let mut buf = Buffer::new(&mut f.system, Metrics::new(size, size));
    let attrs = Attrs::new()
        .family(Family::Name(&family))
        .weight(if style.is_bold() { Weight::BOLD } else { Weight::NORMAL })
        .style(if style.is_italic() { Style::Italic } else { Style::Normal });
    buf.set_size(None, None);
    buf.set_text(text, &attrs, Shaping::Advanced, None);
    buf.shape_until_scroll(&mut f.system, false);
    let mut w: f32 = 0.0;
    let mut lines = 0usize;
    for run in buf.layout_runs() {
        w = w.max(run.line_w);
        lines += 1;
    }
    (buf, w.ceil(), lines.max(1) as f32 * size)
}

/// Size of laid-out text in pixels (width of the longest line, total height).
pub fn measure(text: &str, size: f32, family: &str, style: FontStyle) -> (f32, f32) {
    let mut f = fonts();
    let (_, w, h) = layout(&mut f, text, size, family, style);
    (w, h)
}

/// Rasterise text with its top-left corner at (`x`, `y`) on `pixmap`. Lines are
/// aligned inside the width of the longest line.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pixmap: &mut tiny_skia::Pixmap,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    family: &str,
    style: FontStyle,
    align: TextAlign,
    color: crate::color::Rgba,
) {
    if size <= 0.5 {
        return;
    }
    let mut guard = fonts();
    let f = &mut *guard;
    let (buf, block_w, _) = layout(f, text, size, family, style);
    let ccolor = cosmic_text::Color::rgba(color.r, color.g, color.b, color.a);
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let stride = pw as usize;
    let data = pixmap.data_mut();

    for run in buf.layout_runs() {
        let dx = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (block_w - run.line_w) / 2.0,
            TextAlign::Right => block_w - run.line_w,
        };
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((x + dx, y + run.line_y), 1.0);
            let Some(img) = f.cache.get_image(&mut f.system, physical.cache_key).as_ref().cloned() else { continue };
            let gx = physical.x + img.placement.left;
            let gy = physical.y - img.placement.top;
            let (gw, gh) = (img.placement.width as i32, img.placement.height as i32);
            for row in 0..gh {
                for col in 0..gw {
                    let (px, py) = (gx + col, gy + row);
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let i = (row * gw + col) as usize;
                    let (r, g, b, a) = match img.content {
                        cosmic_text::SwashContent::Mask => {
                            (ccolor.r(), ccolor.g(), ccolor.b(), mul(img.data[i], ccolor.a()))
                        }
                        cosmic_text::SwashContent::Color => {
                            let o = i * 4;
                            (img.data[o], img.data[o + 1], img.data[o + 2], img.data[o + 3])
                        }
                        cosmic_text::SwashContent::SubpixelMask => {
                            (ccolor.r(), ccolor.g(), ccolor.b(), mul(img.data[i * 4], ccolor.a()))
                        }
                    };
                    if a == 0 {
                        continue;
                    }
                    blend(&mut data[(py as usize * stride + px as usize) * 4..][..4], r, g, b, a);
                }
            }
        }
    }
}

fn mul(a: u8, b: u8) -> u8 {
    ((a as u32 * b as u32 + 127) / 255) as u8
}

/// Source-over onto premultiplied RGBA.
fn blend(dst: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    let inv = 255 - a as u32;
    let src = [mul(r, a), mul(g, a), mul(b, a), a];
    for k in 0..4 {
        dst[k] = (src[k] as u32 + (dst[k] as u32 * inv + 127) / 255).min(255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_lines_and_size() {
        let (w1, h1) = measure("Hello", 20.0, FALLBACK_FAMILY, FontStyle::Normal);
        let (w2, h2) = measure("Hello", 40.0, FALLBACK_FAMILY, FontStyle::Normal);
        let (_, h3) = measure("Hello\nWorld", 20.0, FALLBACK_FAMILY, FontStyle::Normal);
        assert!(w1 > 0.0 && w2 > w1 * 1.8);
        assert_eq!(h1, 20.0);
        assert_eq!(h2, 40.0);
        assert_eq!(h3, 40.0);
    }

    #[test]
    fn unknown_family_falls_back_to_inter() {
        let a = measure("Text", 30.0, "No Such Font 123", FontStyle::Normal);
        let b = measure("Text", 30.0, FALLBACK_FAMILY, FontStyle::Normal);
        assert_eq!(a, b);
    }

    #[test]
    fn draw_puts_ink_inside_the_measured_box() {
        let mut pm = tiny_skia::Pixmap::new(200, 60).unwrap();
        draw(
            &mut pm,
            10.0,
            10.0,
            "Hi",
            32.0,
            FALLBACK_FAMILY,
            FontStyle::Bold,
            TextAlign::Left,
            crate::color::Rgba::rgb(255, 0, 0),
        );
        let inked: Vec<(u32, u32)> = (0..60)
            .flat_map(|y| (0..200).map(move |x| (x, y)))
            .filter(|&(x, y)| pm.pixel(x, y).unwrap().alpha() > 0)
            .collect();
        assert!(!inked.is_empty());
        assert!(inked.iter().all(|&(x, y)| x >= 9 && (9..=46).contains(&y)));
    }
}
