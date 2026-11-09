//! The compositor. One function draws a [`Document`] at any scale; the editor
//! preview and the exported file both come from it.

pub mod blur;
pub mod shapes;
pub mod text;

use crate::annotation::{AnnotationKind, DEFAULT_DIM_OPACITY};
use crate::background::Background;
use crate::color::Rgba;
use crate::config::BorderType;
use crate::document::Document;
use crate::geometry::{Layout, OutputRatio, Point, Rect};
use image::RgbaImage;
use shapes::{paint, rounded_rect};
use std::sync::{Arc, Mutex, Weak};
use tiny_skia::{FillRule, FilterQuality, Mask, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// Output pixels per document pixel. 1.0 for export, smaller for previews.
    pub scale: f32,
    /// When false, only the (cropped) screenshot and its annotations are drawn:
    /// no padding, background, inset or shadow.
    pub include_background: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { scale: 1.0, include_background: true }
    }
}

/// Layout used for these options (the bare layout ignores padding and inset).
pub fn layout_for(doc: &Document, opts: &RenderOptions) -> Layout {
    if opts.include_background && doc.settings.show_background {
        doc.layout()
    } else {
        let img = doc.image();
        Layout::compute(img.width(), img.height(), 0.0, OutputRatio::Auto, 0.0)
    }
}

pub fn render(doc: &Document, opts: RenderOptions) -> Pixmap {
    let s = &doc.settings;
    let with_bg = opts.include_background && s.show_background;
    let layout = layout_for(doc, &opts);
    let k = opts.scale.max(0.01);
    let w = ((layout.canvas_width as f32 * k).round() as u32).max(1);
    let h = ((layout.canvas_height as f32 * k).round() as u32).max(1);
    let mut pm = Pixmap::new(w, h).expect("canvas size is non-zero");
    let id = Transform::identity();
    let sc = |r: Rect| Rect::new(r.x * k, r.y * k, r.width * k, r.height * k);

    if with_bg {
        draw_background(&mut pm, &Background::parse(&s.background_color));
    }

    // Everything in winshot's screenshot group is scaled by the inset factor.
    let inset_scale = if layout.frame.width > 0.0 { layout.image.width / layout.frame.width } else { 1.0 };
    let frame = sc(layout.frame);
    let img_rect = sc(layout.image);
    let radius = s.corner_radius as f32 * k * inset_scale;

    if with_bg && s.inset > 0 {
        if let Some(c) = s.inset_background_color.as_deref().and_then(Rgba::parse) {
            if let Some(p) = rounded_rect(frame, s.corner_radius as f32 * k) {
                pm.fill_path(&p, &paint(c), FillRule::Winding, id, None);
            }
        }
    }

    if with_bg && s.shadow_size > 0 {
        let size = s.shadow_size as f32 * k * inset_scale;
        draw_blurred_shadow(
            &mut pm,
            &|pb: &mut PathBuilder, dx: f32, dy: f32| {
                push_rounded(pb, Rect { x: img_rect.x + dx, y: img_rect.y + dy, ..img_rect }, radius)
            },
            img_rect,
            Point::new(0.0, size / 4.0),
            size / 2.0,
            Rgba::BLACK.with_alpha_f(0.5),
            Some(img_rect.inflate(-radius)),
        );
        if let Some(p) = rounded_rect(img_rect, radius) {
            pm.fill_path(&p, &paint(Rgba::BLACK), FillRule::Winding, id, None);
        }
    }

    draw_image(&mut pm, doc.image(), img_rect, radius);

    if s.border_enabled && s.border_weight > 0 {
        let weight = s.border_weight as f32 * k * inset_scale;
        let mult = match s.border_type {
            BorderType::Outside => -1.0,
            BorderType::Center => 0.0,
            BorderType::Inside => 1.0,
        };
        let off = weight / 2.0 * mult;
        let r = Rect::new(img_rect.x + off, img_rect.y + off, img_rect.width - 2.0 * off, img_rect.height - 2.0 * off);
        if let (Some(p), Some(c)) = (rounded_rect(r, (radius - off).max(0.0)), Rgba::parse(&s.border_color)) {
            let c = c.with_alpha_f(s.border_opacity.min(100) as f32 / 100.0);
            pm.stroke_path(&p, &paint(c), &Stroke { width: weight, ..Stroke::default() }, id, None);
        }
    }

    // Annotations live in original-image pixels.
    let origin = doc.crop_origin();
    let scale = layout.image_scale * k;
    let map = |p: Point| Point::new(img_rect.x + (p.x - origin.x) * scale, img_rect.y + (p.y - origin.y) * scale);
    let shown: Vec<_> = doc.visible_annotations().collect();
    for a in &shown {
        shapes::draw_annotation(&mut pm, a, &map, scale);
    }

    // Spotlights: one dimming layer with a hole per spotlight (winshot uses the
    // first spotlight's opacity for the whole layer).
    let spots: Vec<(Rect, f32)> = shown
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Spotlight { rect, dim_opacity } => Some((rect.normalized(), *dim_opacity)),
            _ => None,
        })
        .collect();
    if let Some((_, opacity)) = spots.first() {
        let mut pb = PathBuilder::new();
        pb.push_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap());
        for (r, _) in &spots {
            let p = map(Point::new(r.x, r.y));
            if let Some(rr) = tiny_skia::Rect::from_xywh(p.x, p.y, r.width * scale, r.height * scale) {
                pb.push_rect(rr);
            }
        }
        if let Some(path) = pb.finish() {
            let o = if opacity.is_finite() { *opacity } else { DEFAULT_DIM_OPACITY };
            pm.fill_path(&path, &paint(Rgba::BLACK.with_alpha_f(o)), FillRule::EvenOdd, id, None);
        }
    }
    pm
}

/// Render at 1:1 into an `RgbaImage` (straight alpha) for saving or copying.
pub fn render_image(doc: &Document, include_background: bool) -> RgbaImage {
    pixmap_to_image(&render(doc, RenderOptions { scale: 1.0, include_background }))
}

pub fn pixmap_to_image(pm: &Pixmap) -> RgbaImage {
    let mut out = RgbaImage::new(pm.width(), pm.height());
    for (dst, src) in out.pixels_mut().zip(pm.pixels()) {
        let c = src.demultiply();
        dst.0 = [c.red(), c.green(), c.blue(), c.alpha()];
    }
    out
}

fn draw_background(pm: &mut Pixmap, bg: &Background) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let full = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h).unwrap();
    let id = Transform::identity();
    match bg {
        Background::Solid(c) => pm.fill(c.to_skia()),
        Background::Linear(stops) => {
            let stops: Vec<_> = stops.iter().map(|(p, c)| tiny_skia::GradientStop::new(*p, c.to_skia())).collect();
            let shader = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(0.0, 0.0),
                tiny_skia::Point::from_xy(w, h),
                stops,
                tiny_skia::SpreadMode::Pad,
                id,
            );
            match shader {
                Some(shader) => {
                    let p = tiny_skia::Paint { shader, anti_alias: false, ..Default::default() };
                    pm.fill_rect(full, &p, id, None);
                }
                None => pm.fill(Rgba::rgb(0x66, 0x7e, 0xea).to_skia()),
            }
        }
        Background::Image(src) => match load_background_image(src) {
            Some(img) => {
                // "cover": scale to fill, centre, crop the overflow.
                let s = (w / img.width() as f32).max(h / img.height() as f32);
                let (dw, dh) = (img.width() as f32 * s, img.height() as f32 * s);
                let t = Transform::from_row(s, 0.0, 0.0, s, (w - dw) / 2.0, (h - dh) / 2.0);
                pm.draw_pixmap(
                    0,
                    0,
                    img.as_ref().as_ref(),
                    &PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() },
                    t,
                    None,
                );
            }
            None => pm.fill(crate::edge_color::DEFAULT_EDGE_COLOR.to_skia()),
        },
    }
}

fn push_rounded(pb: &mut PathBuilder, r: Rect, radius: f32) {
    if let Some(p) = rounded_rect(r, radius) {
        pb.push_path(&p);
    }
}

/// Fill a blurred, offset copy of a shape. Only the shape's neighbourhood is
/// processed, the blur runs at reduced resolution for large radii (a Gaussian
/// has no fine detail to lose), and pixels inside `covered` are skipped because
/// the caller paints them opaque afterwards.
pub(crate) fn draw_blurred_shadow(
    pm: &mut Pixmap,
    build: &dyn Fn(&mut PathBuilder, f32, f32),
    bounds: Rect,
    offset: Point,
    sigma: f32,
    color: Rgba,
    covered: Option<Rect>,
) {
    let margin = (sigma * 3.0).ceil() + 2.0;
    let ox = (bounds.x + offset.x - margin).floor();
    let oy = (bounds.y + offset.y - margin).floor();
    let bw = (bounds.width + 2.0 * margin).ceil() as u32 + 1;
    let bh = (bounds.height + 2.0 * margin).ceil() as u32 + 1;

    let f = (sigma / 3.0).floor().clamp(1.0, 16.0);
    let (sw, sh) = (((bw as f32) / f).ceil() as u32 + 1, ((bh as f32) / f).ceil() as u32 + 1);
    let Some(mut mask) = Mask::new(sw, sh) else { return };
    let mut pb = PathBuilder::new();
    build(&mut pb, offset.x - ox, offset.y - oy);
    let Some(path) = pb.finish() else { return };
    mask.fill_path(&path, FillRule::Winding, true, Transform::from_scale(1.0 / f, 1.0 / f));
    blur::blur_alpha(mask.data_mut(), sw as usize, sh as usize, sigma / f);

    // Clip the work area to the canvas.
    let x0 = ox.max(0.0) as u32;
    let y0 = oy.max(0.0) as u32;
    let x1 = ((ox + bw as f32) as u32).min(pm.width());
    let y1 = ((oy + bh as f32) as u32).min(pm.height());
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let skip = covered.map(|r| (r.x.ceil(), r.y.ceil(), r.right().floor(), r.bottom().floor()));
    let small = mask.data();
    let sample = |sx: f32, sy: f32| -> f32 {
        let (fx, fy) = (sx.floor(), sy.floor());
        let (ix, iy) = (fx.max(0.0) as usize, fy.max(0.0) as usize);
        let (ix1, iy1) = ((ix + 1).min(sw as usize - 1), (iy + 1).min(sh as usize - 1));
        let (tx, ty) = (sx - fx, sy - fy);
        let at = |x: usize, y: usize| small[y * sw as usize + x] as f32;
        let top = at(ix, iy) * (1.0 - tx) + at(ix1, iy) * tx;
        let bot = at(ix, iy1) * (1.0 - tx) + at(ix1, iy1) * tx;
        top * (1.0 - ty) + bot * ty
    };
    let stride = pm.width() as usize;
    let ca = color.a as f32 / 255.0;
    let pixels = pm.pixels_mut();
    for y in y0..y1 {
        let sy = ((y as f32 + 0.5 - oy) / f - 0.5).max(0.0);
        let row = y as usize * stride;
        for x in x0..x1 {
            if let Some((l, t, r, b)) = skip {
                if (x as f32) >= l && (x as f32) < r && (y as f32) >= t && (y as f32) < b {
                    continue;
                }
            }
            let sx = ((x as f32 + 0.5 - ox) / f - 0.5).max(0.0);
            let a = sample(sx, sy) / 255.0 * ca;
            if a <= 0.0 {
                continue;
            }
            let dst = &mut pixels[row + x as usize];
            let inv = 1.0 - a;
            let blend = |s: u8, d: u8| (s as f32 * a + d as f32 * inv).round().min(255.0) as u8;
            let (r, g, b) = (blend(color.r, dst.red()), blend(color.g, dst.green()), blend(color.b, dst.blue()));
            let alpha = (255.0 * a + dst.alpha() as f32 * inv).round().min(255.0) as u8;
            if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(r.min(alpha), g.min(alpha), b.min(alpha), alpha)
            {
                *dst = c;
            }
        }
    }
}

fn draw_image(pm: &mut Pixmap, img: &Arc<RgbaImage>, dest: Rect, radius: f32) {
    let target = (dest.width / img.width().max(1) as f32).min(dest.height / img.height().max(1) as f32);
    let src = image_pixmap(img, target);
    let sx = dest.width / src.width() as f32;
    let sy = dest.height / src.height() as f32;
    let t = Transform::from_row(sx, 0.0, 0.0, sy, dest.x, dest.y);
    let quality =
        if (sx - 1.0).abs() < 1e-4 && (sy - 1.0).abs() < 1e-4 && dest.x.fract() == 0.0 && dest.y.fract() == 0.0 {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bicubic
        };
    let mask = if radius > 0.0 {
        rounded_rect(dest, radius).and_then(|p| {
            let mut m = Mask::new(pm.width(), pm.height())?;
            m.fill_path(&p, FillRule::Winding, true, Transform::identity());
            Some(m)
        })
    } else {
        None
    };
    pm.draw_pixmap(0, 0, src.as_ref().as_ref(), &PixmapPaint { quality, ..Default::default() }, t, mask.as_ref());
}

/// Premultiplied copy of the screenshot for drawing at `scale`. Keeps a chain
/// of half-size copies for the most recent image, and returns the smallest one
/// that is still at least `scale` of the original, so previews filter a small
/// image instead of the full capture.
fn image_pixmap(img: &Arc<RgbaImage>, scale: f32) -> Arc<Pixmap> {
    type Chain = (Weak<RgbaImage>, Vec<Arc<Pixmap>>);
    static CACHE: Mutex<Option<Chain>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let valid = cache.as_ref().is_some_and(|(weak, _)| weak.upgrade().is_some_and(|a| Arc::ptr_eq(&a, img)));
    if !valid {
        *cache = Some((Arc::downgrade(img), vec![Arc::new(image_to_pixmap(img))]));
    }
    let chain = &mut cache.as_mut().unwrap().1;
    let mut level = 0;
    while scale <= 0.5f32.powi(level as i32 + 1) {
        if chain.len() <= level + 1 {
            let Some(next) = halve(&chain[level]) else { break };
            chain.push(Arc::new(next));
        }
        level += 1;
    }
    chain[level].clone()
}

/// 2x2 box downsample of a premultiplied pixmap.
fn halve(src: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (src.width() / 2, src.height() / 2);
    let mut out = Pixmap::new(w, h)?;
    let sw = src.width() as usize;
    let s = src.pixels();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let p = [
                s[2 * y * sw + 2 * x],
                s[2 * y * sw + 2 * x + 1],
                s[(2 * y + 1) * sw + 2 * x],
                s[(2 * y + 1) * sw + 2 * x + 1],
            ];
            let avg = |f: fn(&tiny_skia::PremultipliedColorU8) -> u8| {
                ((p.iter().map(|c| f(c) as u32).sum::<u32>() + 2) / 4) as u8
            };
            let a = avg(|c| c.alpha());
            let c = tiny_skia::PremultipliedColorU8::from_rgba(
                avg(|c| c.red()).min(a),
                avg(|c| c.green()).min(a),
                avg(|c| c.blue()).min(a),
                a,
            );
            if let Some(c) = c {
                out.pixels_mut()[y * w as usize + x] = c;
            }
        }
    }
    Some(out)
}

pub fn image_to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut pm = Pixmap::new(img.width().max(1), img.height().max(1)).unwrap();
    for (dst, src) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
        let [r, g, b, a] = src.0;
        *dst = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    pm
}

fn load_background_image(src: &str) -> Option<Arc<Pixmap>> {
    static CACHE: Mutex<Vec<(String, Arc<Pixmap>)>> = Mutex::new(Vec::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, pm)) = cache.iter().find(|(k, _)| k == src) {
        return Some(pm.clone());
    }
    let img = if let Some(rest) = src.strip_prefix("data:") {
        let (_, b64) = rest.split_once(";base64,")?;
        image::load_from_memory(&decode_base64(b64)?).ok()?
    } else {
        image::open(src.strip_prefix("file://").unwrap_or(src)).ok()?
    };
    let pm = Arc::new(image_to_pixmap(&img.to_rgba8()));
    cache.insert(0, (src.to_string(), pm.clone()));
    cache.truncate(4);
    Some(pm)
}

/// Standard base64 (RFC 4648) decoder for data URLs stored by winshot.
fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        let bytes = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::Annotation;
    use crate::document::{EditorSettings, PixelRect};

    fn settings() -> EditorSettings {
        EditorSettings {
            auto_background: false,
            padding: 20,
            corner_radius: 0,
            shadow_size: 0,
            background_color: "#ff0000".into(),
            ..EditorSettings::default()
        }
    }

    fn doc_with(img: RgbaImage, s: EditorSettings) -> Document {
        Document::new(img, s)
    }

    fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = pm.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn padding_shows_background_and_image_in_the_middle() {
        let d = doc_with(RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 255, 255])), settings());
        let pm = render(&d, RenderOptions::default());
        assert_eq!((pm.width(), pm.height()), (50, 50));
        assert_eq!(px(&pm, 2, 2), [255, 0, 0, 255]);
        assert_eq!(px(&pm, 25, 25), [0, 0, 255, 255]);
    }

    #[test]
    fn bare_render_is_just_the_image() {
        let d = doc_with(RgbaImage::from_pixel(10, 8, image::Rgba([0, 0, 255, 255])), settings());
        let pm = render(&d, RenderOptions { include_background: false, ..Default::default() });
        assert_eq!((pm.width(), pm.height()), (10, 8));
        assert_eq!(px(&pm, 0, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn preview_scale_shrinks_output() {
        let d = doc_with(RgbaImage::new(200, 100), settings());
        let pm = render(&d, RenderOptions { scale: 0.5, ..Default::default() });
        assert_eq!((pm.width(), pm.height()), (120, 70));
    }

    #[test]
    fn gradient_runs_corner_to_corner() {
        let s = EditorSettings {
            background_color: "linear-gradient(135deg, #000000 0%, #ffffff 100%)".into(),
            ..settings()
        };
        let d = doc_with(RgbaImage::new(10, 10), s);
        let pm = render(&d, RenderOptions::default());
        assert!(px(&pm, 0, 0)[0] < 20);
        assert!(px(&pm, 49, 49)[0] > 235);
    }

    #[test]
    fn corner_radius_clips_image_corners() {
        let s = EditorSettings { corner_radius: 8, ..settings() };
        let d = doc_with(RgbaImage::from_pixel(40, 40, image::Rgba([0, 0, 255, 255])), s);
        let pm = render(&d, RenderOptions::default());
        assert_eq!(px(&pm, 20, 20), [255, 0, 0, 255], "corner shows background");
        assert_eq!(px(&pm, 40, 40), [0, 0, 255, 255]);
    }

    #[test]
    fn shadow_darkens_below_the_image() {
        let s = EditorSettings { shadow_size: 20, padding: 40, ..settings() };
        let d = doc_with(RgbaImage::from_pixel(40, 40, image::Rgba([0, 0, 255, 255])), s);
        let pm = render(&d, RenderOptions::default());
        let below = px(&pm, 60, 84);
        assert!(below[0] < 250 && below[0] > 60, "shadow blends into background: {below:?}");
        assert_eq!(px(&pm, 5, 5), [255, 0, 0, 255], "far corner untouched");
    }

    #[test]
    fn inside_border_paints_on_the_image_edge() {
        let s = EditorSettings {
            border_enabled: true,
            border_weight: 4,
            border_color: "#00ff00".into(),
            border_type: BorderType::Inside,
            ..settings()
        };
        let d = doc_with(RgbaImage::from_pixel(40, 40, image::Rgba([0, 0, 255, 255])), s);
        let pm = render(&d, RenderOptions::default());
        assert_eq!(px(&pm, 21, 40), [0, 255, 0, 255]);
        assert_eq!(px(&pm, 18, 40), [255, 0, 0, 255]);
    }

    #[test]
    fn spotlight_dims_outside_only() {
        let mut d = doc_with(RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255])), settings());
        d.content.annotations.push(Annotation {
            id: 1,
            kind: AnnotationKind::Spotlight { rect: Rect::new(10.0, 10.0, 30.0, 30.0), dim_opacity: 0.5 },
            stroke: None,
            stroke_width: 0.0,
            fill: None,
        });
        let pm = render(&d, RenderOptions::default());
        assert_eq!(px(&pm, 45, 45), [255, 255, 255, 255]);
        let dim = px(&pm, 100, 100);
        assert!((120..=135).contains(&dim[0]), "{dim:?}");
    }

    #[test]
    fn annotations_follow_the_crop() {
        let mut d = doc_with(
            RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255])),
            EditorSettings { padding: 0, ..settings() },
        );
        d.content.annotations.push(Annotation {
            id: 1,
            kind: AnnotationKind::Rectangle { rect: Rect::new(50.0, 50.0, 10.0, 10.0), corner_radius: 0.0 },
            stroke: None,
            stroke_width: 0.0,
            fill: Some(Rgba::rgb(0, 0, 0)),
        });
        d.set_crop(Some(PixelRect { x: 40, y: 40, width: 40, height: 40 }));
        let pm = render(&d, RenderOptions::default());
        assert_eq!((pm.width(), pm.height()), (40, 40));
        assert_eq!(px(&pm, 15, 15), [0, 0, 0, 255]);
        assert_eq!(px(&pm, 5, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGk").unwrap(), b"hi");
        assert!(decode_base64("a$").is_none());
    }

    #[test]
    fn image_round_trips_through_pixmap() {
        let img = RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        assert_eq!(pixmap_to_image(&image_to_pixmap(&img)), img);
    }
}
