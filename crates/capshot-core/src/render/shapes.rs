//! Path builders and annotation drawing.

use crate::annotation::{
    arrow_polygon, number_radius, text_or_placeholder, Annotation, AnnotationKind, FontStyle, TextAlign,
};
use crate::color::Rgba;
use crate::geometry::{Point, Rect};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Rounded rectangle path (circular corners clamped to half the short side).
pub fn rounded_rect(r: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let r = r.normalized();
    if r.width <= 0.0 || r.height <= 0.0 {
        return None;
    }
    let rad = radius.max(0.0).min(r.width / 2.0).min(r.height / 2.0);
    if rad <= 0.0 {
        return Some(PathBuilder::from_rect(tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height)?));
    }
    // Cubic approximation of a quarter circle.
    let k = rad * 0.552_284_8;
    let (l, t, rt, b) = (r.x, r.y, r.right(), r.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(rt - rad, t);
    pb.cubic_to(rt - rad + k, t, rt, t + rad - k, rt, t + rad);
    pb.line_to(rt, b - rad);
    pb.cubic_to(rt, b - rad + k, rt - rad + k, b, rt - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - k, b, l, b - rad + k, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - k, l + rad - k, t, l + rad, t);
    pb.close();
    pb.finish()
}

pub fn polygon(points: &[Point]) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x, first.y);
    for p in rest {
        pb.line_to(p.x, p.y);
    }
    pb.close();
    pb.finish()
}

pub fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

fn stroke(width: f32, cap: LineCap) -> Stroke {
    Stroke { width, line_cap: cap, line_join: LineJoin::Miter, ..Stroke::default() }
}

/// Draw one annotation. `map` turns original-image pixels into canvas pixels
/// and `scale` is the matching size factor for widths, radii and fonts.
pub fn draw_annotation(pm: &mut Pixmap, a: &Annotation, map: &dyn Fn(Point) -> Point, scale: f32) {
    let id = Transform::identity();
    let map_rect = |r: &Rect| {
        let p = map(Point::new(r.x, r.y));
        Rect::new(p.x, p.y, r.width * scale, r.height * scale)
    };
    let sw = a.stroke_width * scale;
    match &a.kind {
        AnnotationKind::Rectangle { rect, corner_radius } => {
            if let Some(path) = rounded_rect(map_rect(rect), corner_radius * scale) {
                if let Some(f) = a.fill {
                    pm.fill_path(&path, &paint(f), FillRule::Winding, id, None);
                }
                if let (Some(s), true) = (a.stroke, sw > 0.0) {
                    pm.stroke_path(&path, &paint(s), &stroke(sw, LineCap::Butt), id, None);
                }
            }
        }
        AnnotationKind::Ellipse { rect } => {
            let r = map_rect(&rect.normalized());
            if let Some(path) = tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height).and_then(PathBuilder::from_oval)
            {
                if let Some(f) = a.fill {
                    pm.fill_path(&path, &paint(f), FillRule::Winding, id, None);
                }
                if let (Some(s), true) = (a.stroke, sw > 0.0) {
                    pm.stroke_path(&path, &paint(s), &stroke(sw, LineCap::Butt), id, None);
                }
            }
        }
        AnnotationKind::Line { start, end } => {
            let Some(s) = a.stroke else { return };
            let (p0, p1) = (map(*start), map(*end));
            let mut pb = PathBuilder::new();
            pb.move_to(p0.x, p0.y);
            pb.line_to(p1.x, p1.y);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &paint(s), &stroke(sw.max(0.5), LineCap::Square), id, None);
            }
        }
        AnnotationKind::Arrow { start, end, curved, curve_offset } => {
            let Some(s) = a.stroke else { return };
            let poly = arrow_polygon(*start, *end, *curved, *curve_offset, a.stroke_width);
            let mapped: Vec<Point> = poly.iter().map(|p| map(*p)).collect();
            let path = if mapped.len() >= 3 {
                polygon(&mapped)
            } else {
                let c = map(*start);
                PathBuilder::from_circle(c.x, c.y, 2.0 * scale)
            };
            if let Some(path) = path {
                pm.fill_path(&path, &paint(s), FillRule::Winding, id, None);
            }
        }
        AnnotationKind::Text { origin, text, font_size, font_family, font_style, align } => {
            let p = map(*origin);
            let color = a.stroke.unwrap_or(Rgba::BLACK);
            super::text::draw(
                pm,
                p.x,
                p.y,
                text_or_placeholder(text),
                font_size * scale,
                font_family,
                *font_style,
                *align,
                color,
            );
        }
        AnnotationKind::Number { center, value } => {
            let c = map(*center);
            let r = number_radius(*value) * scale;
            // Soft drop shadow: black, blur 4, opacity 0.3, 2 px down.
            super::draw_blurred_shadow(
                pm,
                &|pb: &mut PathBuilder, dx: f32, dy: f32| pb.push_circle(c.x + dx, c.y + dy, r),
                Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r),
                Point::new(0.0, 2.0 * scale),
                2.0 * scale,
                Rgba::BLACK.with_alpha_f(0.3),
                None,
            );
            if let Some(path) = PathBuilder::from_circle(c.x, c.y, r) {
                pm.fill_path(
                    &path,
                    &paint(a.stroke.unwrap_or(Rgba::rgb(0xef, 0x44, 0x44))),
                    FillRule::Winding,
                    id,
                    None,
                );
            }
            let label = value.to_string();
            let size = r * 1.2;
            let (w, h) = super::text::measure(&label, size, "Arial", FontStyle::Bold);
            super::text::draw(
                pm,
                c.x - w / 2.0,
                c.y - h / 2.0,
                &label,
                size,
                "Arial",
                FontStyle::Bold,
                TextAlign::Left,
                Rgba::WHITE,
            );
        }
        AnnotationKind::Spotlight { .. } => {} // drawn as one dimming layer by the caller
    }
}
