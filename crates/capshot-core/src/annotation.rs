//! Annotation model. Coordinates are in screenshot pixels, so annotations stay
//! attached to the image when padding, inset or output ratio change.

use crate::color::Rgba;
use crate::geometry::{Point, Rect};
use serde::{Deserialize, Serialize};

/// Smallest size a box-shaped annotation can be resized to (winshot uses 5 px).
pub const MIN_SHAPE_SIZE: f32 = 5.0;
/// Width of the clickable band around lines and arrows (winshot's hitStrokeWidth).
pub const HIT_STROKE_WIDTH: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    #[default]
    Normal,
    Bold,
    Italic,
    #[serde(rename = "bold italic")]
    BoldItalic,
}

impl FontStyle {
    pub fn is_bold(self) -> bool {
        matches!(self, Self::Bold | Self::BoldItalic)
    }
    pub fn is_italic(self) -> bool {
        matches!(self, Self::Italic | Self::BoldItalic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AnnotationKind {
    Rectangle { rect: Rect, corner_radius: f32 },
    Ellipse { rect: Rect },
    Arrow { start: Point, end: Point, curved: bool, curve_offset: Option<Point> },
    Line { start: Point, end: Point },
    Text { origin: Point, text: String, font_size: f32, font_family: String, font_style: FontStyle, align: TextAlign },
    Number { center: Point, value: u32 },
    Spotlight { rect: Rect, dim_opacity: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    pub kind: AnnotationKind,
    /// Outline colour. For arrows, text and numbers this is the main colour.
    pub stroke: Option<Rgba>,
    pub stroke_width: f32,
    pub fill: Option<Rgba>,
}

pub const DEFAULT_DIM_OPACITY: f32 = 0.7;
pub const DEFAULT_FONT_SIZE: f32 = 48.0;
pub const DEFAULT_FONT_FAMILY: &str = "Arial";

impl Annotation {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            AnnotationKind::Rectangle { .. } => "rectangle",
            AnnotationKind::Ellipse { .. } => "ellipse",
            AnnotationKind::Arrow { .. } => "arrow",
            AnnotationKind::Line { .. } => "line",
            AnnotationKind::Text { .. } => "text",
            AnnotationKind::Number { .. } => "number",
            AnnotationKind::Spotlight { .. } => "spotlight",
        }
    }

    /// Bounding box in screenshot pixels. Text uses the measured layout.
    pub fn bounds(&self) -> Rect {
        match &self.kind {
            AnnotationKind::Rectangle { rect, .. }
            | AnnotationKind::Ellipse { rect }
            | AnnotationKind::Spotlight { rect, .. } => rect.normalized(),
            AnnotationKind::Arrow { start, end, curved, curve_offset } => {
                let mut r = Rect::from_points(*start, *end);
                if *curved {
                    let c = arrow_control_point(*start, *end, *curve_offset);
                    r = union(r, Rect::new(c.x, c.y, 0.0, 0.0));
                }
                r
            }
            AnnotationKind::Line { start, end } => Rect::from_points(*start, *end),
            AnnotationKind::Text { origin, text, font_size, font_family, font_style, .. } => {
                let (w, h) =
                    crate::render::text::measure(text_or_placeholder(text), *font_size, font_family, *font_style);
                Rect::new(origin.x, origin.y, w, h)
            }
            AnnotationKind::Number { center, value } => {
                let r = number_radius(*value);
                Rect::new(center.x - r, center.y - r, r * 2.0, r * 2.0)
            }
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        let mv = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        match &mut self.kind {
            AnnotationKind::Rectangle { rect, .. }
            | AnnotationKind::Ellipse { rect }
            | AnnotationKind::Spotlight { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
            AnnotationKind::Arrow { start, end, .. } | AnnotationKind::Line { start, end } => {
                mv(start);
                mv(end);
            }
            AnnotationKind::Text { origin, .. } => mv(origin),
            AnnotationKind::Number { center, .. } => mv(center),
        }
    }

    /// Whether `p` (screenshot pixels) hits this annotation. `tolerance` widens
    /// thin shapes so they stay clickable at any zoom.
    pub fn hit_test(&self, p: Point, tolerance: f32) -> bool {
        match &self.kind {
            AnnotationKind::Arrow { start, end, curved: false, .. } | AnnotationKind::Line { start, end } => {
                distance_to_segment(p, *start, *end)
                    <= (HIT_STROKE_WIDTH / 2.0).max(self.stroke_width / 2.0) + tolerance
            }
            AnnotationKind::Arrow { start, end, curved: true, curve_offset } => {
                let c = arrow_control_point(*start, *end, *curve_offset);
                let limit = (HIT_STROKE_WIDTH / 2.0).max(self.stroke_width) + tolerance;
                (0..=20).map(|i| i as f32 / 20.0).collect::<Vec<_>>().windows(2).any(|w| {
                    distance_to_segment(p, quad_point(*start, c, *end, w[0]), quad_point(*start, c, *end, w[1]))
                        <= limit
                })
            }
            AnnotationKind::Number { center, value } => {
                let d = ((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt();
                d <= number_radius(*value) + tolerance
            }
            AnnotationKind::Ellipse { rect } => {
                let r = rect.normalized().inflate(tolerance);
                let (rx, ry) = (r.width / 2.0, r.height / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let nx = (p.x - (r.x + rx)) / rx;
                let ny = (p.y - (r.y + ry)) / ry;
                nx * nx + ny * ny <= 1.0
            }
            _ => self.bounds().inflate(tolerance).contains(p),
        }
    }
}

pub fn text_or_placeholder(text: &str) -> &str {
    if text.is_empty() {
        "Text"
    } else {
        text
    }
}

/// Number marker radius: 18 px plus 6 px per extra digit (winshot's rule).
pub fn number_radius(value: u32) -> f32 {
    let digits = value.max(1).to_string().len() as f32;
    18.0 + (digits - 1.0) * 6.0
}

/// Lowest positive integer not used by any number marker.
pub fn next_number(annotations: &[Annotation]) -> u32 {
    let used: std::collections::HashSet<u32> = annotations
        .iter()
        .filter_map(|a| match a.kind {
            AnnotationKind::Number { value, .. } => Some(value),
            _ => None,
        })
        .collect();
    (1..).find(|n| !used.contains(n)).unwrap_or(1)
}

/// Control point of a curved arrow: the stored offset from the midpoint, or
/// 20% of the length to the left of the direction of travel by default.
pub fn arrow_control_point(start: Point, end: Point, offset: Option<Point>) -> Point {
    let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    if let Some(o) = offset {
        return Point::new(mid.x + o.x, mid.y + o.y);
    }
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let amount = len * 0.2;
    Point::new(mid.x - dy / len * amount, mid.y + dx / len * amount)
}

pub fn quad_point(p0: Point, c: Point, p1: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    Point::new(mt * mt * p0.x + 2.0 * mt * t * c.x + t * t * p1.x, mt * mt * p0.y + 2.0 * mt * t * c.y + t * t * p1.y)
}

fn quad_tangent(p0: Point, c: Point, p1: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    let dx = 2.0 * mt * (c.x - p0.x) + 2.0 * t * (p1.x - c.x);
    let dy = 2.0 * mt * (c.y - p0.y) + 2.0 * t * (p1.y - c.y);
    let len = (dx * dx + dy * dy).sqrt();
    let len = if len == 0.0 { 1.0 } else { len };
    Point::new(dx / len, dy / len)
}

// Tapered arrow proportions from winshot: tail 0.5x, body 2x, head 6x long and 4x wide.
const TAIL: f32 = 0.5;
const BODY: f32 = 2.0;
const HEAD_LEN: f32 = 6.0;
const HEAD_W: f32 = 4.0;

/// Outline polygon of a tapered arrow. Returns fewer than 3 points when the
/// arrow is too short to draw (callers draw a dot instead).
pub fn arrow_polygon(
    start: Point,
    end: Point,
    curved: bool,
    curve_offset: Option<Point>,
    stroke_width: f32,
) -> Vec<Point> {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let total = (dx * dx + dy * dy).sqrt();
    if total < 1.0 {
        return vec![start];
    }
    let head_len = (stroke_width * HEAD_LEN).min(total * 0.6);
    let (tail_w, body_w, head_w) = (stroke_width * TAIL, stroke_width * BODY, stroke_width * HEAD_W);

    if !curved {
        let (ux, uy) = (dx / total, dy / total);
        let (px, py) = (-uy, ux);
        let j = Point::new(end.x - ux * head_len, end.y - uy * head_len);
        let off = |p: Point, w: f32| Point::new(p.x + px * w, p.y + py * w);
        return vec![
            off(start, tail_w),
            off(j, body_w),
            off(j, head_w),
            end,
            off(j, -head_w),
            off(j, -body_w),
            off(start, -tail_w),
        ];
    }

    let c = arrow_control_point(start, end, curve_offset);
    let t_head = (1.0 - head_len / total).max(0.5);
    let mut samples: Vec<f32> = (0..30).map(|i| i as f32 / 30.0).filter(|t| *t < t_head).collect();
    samples.push(t_head);
    let mut left = Vec::with_capacity(samples.len());
    let mut right = Vec::with_capacity(samples.len());
    for t in samples {
        let pt = quad_point(start, c, end, t);
        let tan = quad_tangent(start, c, end, t);
        let (px, py) = (-tan.y, tan.x);
        let w = tail_w + (body_w - tail_w) * (t / t_head);
        left.push(Point::new(pt.x + px * w, pt.y + py * w));
        right.push(Point::new(pt.x - px * w, pt.y - py * w));
    }
    let tip_tan = quad_tangent(start, c, end, 1.0);
    let (tpx, tpy) = (-tip_tan.y, tip_tan.x);
    let j = quad_point(start, c, end, t_head);
    let mut out = left;
    out.push(Point::new(j.x + tpx * head_w, j.y + tpy * head_w));
    out.push(end);
    out.push(Point::new(j.x - tpx * head_w, j.y - tpy * head_w));
    out.extend(right.into_iter().rev());
    out
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(x, y, a.right().max(b.right()) - x, a.bottom().max(b.bottom()) - y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(kind: AnnotationKind) -> Annotation {
        Annotation { id: 1, kind, stroke: Some(Rgba::rgb(255, 0, 0)), stroke_width: 4.0, fill: None }
    }

    #[test]
    fn next_number_fills_gaps() {
        let n = |v| ann(AnnotationKind::Number { center: Point::default(), value: v });
        assert_eq!(next_number(&[]), 1);
        assert_eq!(next_number(&[n(1), n(2)]), 3);
        assert_eq!(next_number(&[n(1), n(3)]), 2);
    }

    #[test]
    fn number_radius_grows_per_digit() {
        assert_eq!(number_radius(7), 18.0);
        assert_eq!(number_radius(42), 24.0);
        assert_eq!(number_radius(100), 30.0);
    }

    #[test]
    fn straight_arrow_has_seven_vertices_and_tip_at_end() {
        let poly = arrow_polygon(Point::new(0.0, 0.0), Point::new(100.0, 0.0), false, None, 4.0);
        assert_eq!(poly.len(), 7);
        assert_eq!(poly[3], Point::new(100.0, 0.0));
        // Head shoulders sit 24 px back from the tip, 16 px either side.
        assert_eq!(poly[2], Point::new(76.0, 16.0));
        assert_eq!(poly[4], Point::new(76.0, -16.0));
    }

    #[test]
    fn short_arrow_head_is_capped_at_sixty_percent() {
        let poly = arrow_polygon(Point::new(0.0, 0.0), Point::new(10.0, 0.0), false, None, 10.0);
        assert!((poly[2].x - 4.0).abs() < 1e-4);
    }

    #[test]
    fn degenerate_arrow_returns_single_point() {
        assert_eq!(arrow_polygon(Point::new(5.0, 5.0), Point::new(5.2, 5.0), false, None, 4.0).len(), 1);
    }

    #[test]
    fn curved_arrow_default_control_is_left_of_travel() {
        let c = arrow_control_point(Point::new(0.0, 0.0), Point::new(100.0, 0.0), None);
        assert_eq!(c, Point::new(50.0, 20.0));
        let poly = arrow_polygon(Point::new(0.0, 0.0), Point::new(100.0, 0.0), true, None, 4.0);
        assert!(poly.contains(&Point::new(100.0, 0.0)));
    }

    #[test]
    fn line_hit_test_uses_wide_band() {
        let a = ann(AnnotationKind::Line { start: Point::new(0.0, 0.0), end: Point::new(100.0, 0.0) });
        assert!(a.hit_test(Point::new(50.0, 13.0), 0.0));
        assert!(!a.hit_test(Point::new(50.0, 20.0), 0.0));
    }

    #[test]
    fn ellipse_hit_test_excludes_corners() {
        let a = ann(AnnotationKind::Ellipse { rect: Rect::new(0.0, 0.0, 100.0, 50.0) });
        assert!(a.hit_test(Point::new(50.0, 25.0), 0.0));
        assert!(!a.hit_test(Point::new(2.0, 2.0), 0.0));
    }

    #[test]
    fn translate_moves_every_kind() {
        let mut a = ann(AnnotationKind::Arrow {
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 10.0),
            curved: false,
            curve_offset: None,
        });
        a.translate(5.0, -5.0);
        assert_eq!(a.bounds(), Rect::new(5.0, -5.0, 10.0, 10.0));
    }
}
