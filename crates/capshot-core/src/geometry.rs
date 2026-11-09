//! Plain geometry types and the layout math shared by preview and export.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Rectangle spanning two corner points in any order.
    pub fn from_points(a: Point, b: Point) -> Self {
        Self { x: a.x.min(b.x), y: a.y.min(b.y), width: (a.x - b.x).abs(), height: (a.y - b.y).abs() }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.right() > other.x && self.x < other.right() && self.bottom() > other.y && self.y < other.bottom()
    }

    /// Positive-size copy of a rectangle that may have negative width or height.
    pub fn normalized(&self) -> Self {
        Self::from_points(Point::new(self.x, self.y), Point::new(self.right(), self.bottom()))
    }

    pub fn inflate(&self, d: f32) -> Self {
        Self::new(self.x - d, self.y - d, self.width + 2.0 * d, self.height + 2.0 * d)
    }
}

/// Final canvas aspect ratio. `Auto` follows the screenshot plus padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputRatio {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "1:1")]
    R1x1,
    #[serde(rename = "4:3")]
    R4x3,
    #[serde(rename = "3:2")]
    R3x2,
    #[serde(rename = "16:9")]
    R16x9,
    #[serde(rename = "5:3")]
    R5x3,
    #[serde(rename = "9:16")]
    R9x16,
    #[serde(rename = "3:4")]
    R3x4,
    #[serde(rename = "2:3")]
    R2x3,
}

impl OutputRatio {
    pub const ALL: [OutputRatio; 9] =
        [Self::Auto, Self::R1x1, Self::R4x3, Self::R3x2, Self::R16x9, Self::R5x3, Self::R9x16, Self::R3x4, Self::R2x3];

    pub fn ratio(self) -> Option<f32> {
        let (w, h) = match self {
            Self::Auto => return None,
            Self::R1x1 => (1.0, 1.0),
            Self::R4x3 => (4.0, 3.0),
            Self::R3x2 => (3.0, 2.0),
            Self::R16x9 => (16.0, 9.0),
            Self::R5x3 => (5.0, 3.0),
            Self::R9x16 => (9.0, 16.0),
            Self::R3x4 => (3.0, 4.0),
            Self::R2x3 => (2.0, 3.0),
        };
        Some(w / h)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::R1x1 => "1:1",
            Self::R4x3 => "4:3",
            Self::R3x2 => "3:2",
            Self::R16x9 => "16:9",
            Self::R5x3 => "5:3",
            Self::R9x16 => "9:16",
            Self::R3x4 => "3:4",
            Self::R2x3 => "2:3",
        }
    }
}

/// Where everything sits on the output canvas, in output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Full output size.
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// Area the screenshot occupies before inset is applied.
    pub frame: Rect,
    /// Area the screenshot occupies after inset scaling (what is actually drawn).
    pub image: Rect,
    /// Scale from screenshot pixels to output pixels (includes inset).
    pub image_scale: f32,
}

impl Layout {
    /// Same output-size rules as winshot's `calculateOutputDimensions`, plus the
    /// centred fit and inset scaling from its editor canvas.
    pub fn compute(image_w: u32, image_h: u32, padding: f32, ratio: OutputRatio, inset_percent: f32) -> Self {
        let sw = image_w.max(1) as f32;
        let sh = image_h.max(1) as f32;
        let padding = padding.max(0.0);
        let min_w = sw + padding * 2.0;
        let min_h = sh + padding * 2.0;

        let (total_w, total_h) = match ratio.ratio() {
            None => (min_w, min_h),
            Some(target) => {
                if target > min_w / min_h {
                    ((min_h * target).round(), min_h)
                } else {
                    (min_w, (min_w / target).round())
                }
            }
        };

        let avail_w = total_w - padding * 2.0;
        let avail_h = total_h - padding * 2.0;
        let aspect = sw / sh;
        let (inner_w, inner_h) =
            if aspect > avail_w / avail_h { (avail_w, avail_w / aspect) } else { (avail_h * aspect, avail_h) };
        let frame = Rect::new((total_w - inner_w) / 2.0, (total_h - inner_h) / 2.0, inner_w, inner_h);

        let inset_scale = 1.0 - inset_percent.clamp(0.0, 50.0) / 100.0;
        let image = Rect::new(
            frame.x + inner_w * (1.0 - inset_scale) / 2.0,
            frame.y + inner_h * (1.0 - inset_scale) / 2.0,
            inner_w * inset_scale,
            inner_h * inset_scale,
        );

        Self {
            canvas_width: total_w.round().max(1.0) as u32,
            canvas_height: total_h.round().max(1.0) as u32,
            frame,
            image,
            image_scale: inner_w / sw * inset_scale,
        }
    }

    /// Convert a point in screenshot pixels to output pixels.
    pub fn image_to_canvas(&self, p: Point) -> Point {
        Point::new(self.image.x + p.x * self.image_scale, self.image.y + p.y * self.image_scale)
    }

    /// Convert a point in output pixels to screenshot pixels.
    pub fn canvas_to_image(&self, p: Point) -> Point {
        Point::new((p.x - self.image.x) / self.image_scale, (p.y - self.image.y) / self.image_scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_ratio_adds_padding_on_every_side() {
        let l = Layout::compute(800, 600, 40.0, OutputRatio::Auto, 0.0);
        assert_eq!((l.canvas_width, l.canvas_height), (880, 680));
        assert_eq!(l.image, Rect::new(40.0, 40.0, 800.0, 600.0));
        assert_eq!(l.image_scale, 1.0);
    }

    #[test]
    fn auto_ratio_without_padding_is_the_screenshot_size() {
        let l = Layout::compute(1920, 1080, 0.0, OutputRatio::Auto, 0.0);
        assert_eq!((l.canvas_width, l.canvas_height), (1920, 1080));
    }

    #[test]
    fn wider_target_ratio_expands_width() {
        // 800x600 + 40 padding = 880x680; 16:9 of height 680 = 1209 (rounded)
        let l = Layout::compute(800, 600, 40.0, OutputRatio::R16x9, 0.0);
        assert_eq!((l.canvas_width, l.canvas_height), (1209, 680));
        // Screenshot keeps its own size and is centred horizontally.
        assert!((l.image.width - 800.0).abs() < 0.01);
        assert!((l.image.x - (1209.0 - 800.0) / 2.0).abs() < 0.01);
    }

    #[test]
    fn taller_target_ratio_expands_height() {
        let l = Layout::compute(800, 600, 40.0, OutputRatio::R9x16, 0.0);
        assert_eq!(l.canvas_width, 880);
        assert_eq!(l.canvas_height, (880.0f32 / (9.0 / 16.0)).round() as u32);
    }

    #[test]
    fn inset_shrinks_the_image_around_its_centre() {
        let l = Layout::compute(1000, 500, 0.0, OutputRatio::Auto, 20.0);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(near(l.image.x, 100.0) && near(l.image.y, 50.0), "{:?}", l.image);
        assert!(near(l.image.width, 800.0) && near(l.image.height, 400.0));
        assert!(near(l.image_scale, 0.8));
        let p = l.image_to_canvas(Point::new(1000.0, 500.0));
        assert!(near(p.x, 900.0) && near(p.y, 450.0));
        let back = l.canvas_to_image(p);
        assert!(near(back.x, 1000.0) && near(back.y, 500.0));
    }

    #[test]
    fn inset_is_clamped_to_fifty_percent() {
        let l = Layout::compute(100, 100, 0.0, OutputRatio::Auto, 90.0);
        assert!((l.image_scale - 0.5).abs() < 1e-6);
    }

    #[test]
    fn ratio_labels_round_trip_through_serde() {
        for r in OutputRatio::ALL {
            let s = serde_json::to_string(&r).unwrap();
            assert_eq!(serde_json::from_str::<OutputRatio>(&s).unwrap(), r);
        }
        assert_eq!(serde_json::to_string(&OutputRatio::R16x9).unwrap(), "\"16:9\"");
    }
}
