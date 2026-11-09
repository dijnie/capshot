//! Crop frame helpers: aspect presets and the 8-handle resize rules.

use crate::geometry::{Point, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CropAspect {
    #[default]
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "16:9")]
    R16x9,
    #[serde(rename = "4:3")]
    R4x3,
    #[serde(rename = "1:1")]
    R1x1,
    #[serde(rename = "9:16")]
    R9x16,
    #[serde(rename = "3:4")]
    R3x4,
}

impl CropAspect {
    pub const ALL: [CropAspect; 6] = [Self::Free, Self::R16x9, Self::R4x3, Self::R1x1, Self::R9x16, Self::R3x4];

    pub fn ratio(self) -> Option<f32> {
        match self {
            Self::Free => None,
            Self::R16x9 => Some(16.0 / 9.0),
            Self::R4x3 => Some(4.0 / 3.0),
            Self::R1x1 => Some(1.0),
            Self::R9x16 => Some(9.0 / 16.0),
            Self::R3x4 => Some(3.0 / 4.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::R16x9 => "16:9",
            Self::R4x3 => "4:3",
            Self::R1x1 => "1:1",
            Self::R9x16 => "9:16",
            Self::R3x4 => "3:4",
        }
    }
}

/// Shrink `area` to `aspect`, keeping its top-left corner (winshot's
/// `constrainToAspectRatio`).
pub fn constrain(area: Rect, aspect: CropAspect) -> Rect {
    let Some(target) = aspect.ratio() else { return area };
    if area.width <= 0.0 || area.height <= 0.0 {
        return area;
    }
    if area.width / area.height > target {
        Rect { width: area.height * target, ..area }
    } else {
        Rect { height: area.width / target, ..area }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    /// Handle centre on a rectangle.
    pub fn position(self, r: Rect) -> Point {
        let (cx, cy) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
        match self {
            Self::TopLeft => Point::new(r.x, r.y),
            Self::Top => Point::new(cx, r.y),
            Self::TopRight => Point::new(r.right(), r.y),
            Self::Right => Point::new(r.right(), cy),
            Self::BottomRight => Point::new(r.right(), r.bottom()),
            Self::Bottom => Point::new(cx, r.bottom()),
            Self::BottomLeft => Point::new(r.x, r.bottom()),
            Self::Left => Point::new(r.x, cy),
        }
    }

    fn moves(self) -> (bool, bool, bool, bool) {
        // (left, top, right, bottom)
        match self {
            Self::TopLeft => (true, true, false, false),
            Self::Top => (false, true, false, false),
            Self::TopRight => (false, true, true, false),
            Self::Right => (false, false, true, false),
            Self::BottomRight => (false, false, true, true),
            Self::Bottom => (false, false, false, true),
            Self::BottomLeft => (true, false, false, true),
            Self::Left => (true, false, false, false),
        }
    }
}

/// Resize `start` by dragging `handle` to `to`, keeping the opposite edge fixed,
/// honouring `aspect` and a minimum size, and staying inside `bounds`.
pub fn resize(start: Rect, handle: Handle, to: Point, aspect: CropAspect, min_size: f32, bounds: Rect) -> Rect {
    let (ml, mt, mr, mb) = handle.moves();
    // The minimum can never exceed the space available, or the clamps below
    // would get an inverted range (tiny images, frames hugging an edge).
    let min_size = min_size.min(bounds.width).min(bounds.height).max(0.0);
    let clamp = |v: f32, lo: f32, hi: f32| v.max(lo).min(hi.max(lo));
    let mut l = start.x;
    let mut t = start.y;
    let mut r = start.right();
    let mut b = start.bottom();
    if ml {
        l = clamp(to.x, bounds.x, r - min_size);
    }
    if mr {
        r = clamp(to.x, (l + min_size).min(bounds.right()), bounds.right());
    }
    if mt {
        t = clamp(to.y, bounds.y, b - min_size);
    }
    if mb {
        b = clamp(to.y, (t + min_size).min(bounds.bottom()), bounds.bottom());
    }
    let mut out = Rect::new(l, t, r - l, b - t);

    if let Some(ratio) = aspect.ratio() {
        let horizontal_only = (ml || mr) && !(mt || mb);
        if horizontal_only {
            out.height = out.width / ratio;
        } else if !(ml || mr) {
            out.width = out.height * ratio;
        } else {
            out = constrain(out, aspect);
        }
        // Anchor to the fixed corner.
        if ml {
            out.x = r - out.width;
        }
        if mt {
            out.y = b - out.height;
        }
        out = fit_inside(out, bounds, ratio);
    }
    out
}

/// Move a rectangle without changing its size, keeping it inside `bounds`.
pub fn translate_within(r: Rect, dx: f32, dy: f32, bounds: Rect) -> Rect {
    Rect {
        x: (r.x + dx).clamp(bounds.x, (bounds.right() - r.width).max(bounds.x)),
        y: (r.y + dy).clamp(bounds.y, (bounds.bottom() - r.height).max(bounds.y)),
        ..r
    }
}

fn fit_inside(mut r: Rect, bounds: Rect, ratio: f32) -> Rect {
    if r.right() > bounds.right() {
        r.width = bounds.right() - r.x;
        r.height = r.width / ratio;
    }
    if r.bottom() > bounds.bottom() {
        r.height = bounds.bottom() - r.y;
        r.width = r.height * ratio;
    }
    if r.x < bounds.x {
        let right = r.right();
        r.x = bounds.x;
        r.width = right - r.x;
        r.height = r.width / ratio;
    }
    if r.y < bounds.y {
        let bottom = r.bottom();
        r.y = bounds.y;
        r.height = bottom - r.y;
        r.width = r.height * ratio;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);

    #[test]
    fn constrain_matches_winshot() {
        let r = constrain(Rect::new(0.0, 0.0, 400.0, 100.0), CropAspect::R16x9);
        assert!((r.width - 177.777_78).abs() < 0.01);
        assert_eq!(r.height, 100.0);
        let r = constrain(Rect::new(0.0, 0.0, 100.0, 400.0), CropAspect::R1x1);
        assert_eq!((r.width, r.height), (100.0, 100.0));
        let r = Rect::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(constrain(r, CropAspect::Free), r);
    }

    #[test]
    fn free_resize_keeps_opposite_corner() {
        let r = resize(
            Rect::new(100.0, 100.0, 200.0, 200.0),
            Handle::TopLeft,
            Point::new(50.0, 80.0),
            CropAspect::Free,
            10.0,
            B,
        );
        assert_eq!(r, Rect::new(50.0, 80.0, 250.0, 220.0));
    }

    #[test]
    fn resize_respects_minimum_size() {
        let r = resize(
            Rect::new(100.0, 100.0, 200.0, 200.0),
            Handle::Right,
            Point::new(0.0, 0.0),
            CropAspect::Free,
            10.0,
            B,
        );
        assert_eq!(r.width, 10.0);
    }

    #[test]
    fn aspect_resize_from_edge_keeps_ratio() {
        let r =
            resize(Rect::new(0.0, 0.0, 160.0, 90.0), Handle::Right, Point::new(320.0, 0.0), CropAspect::R16x9, 10.0, B);
        assert_eq!((r.width, r.height), (320.0, 180.0));
    }

    #[test]
    fn aspect_resize_stays_in_bounds() {
        let r = resize(
            Rect::new(800.0, 0.0, 100.0, 100.0),
            Handle::BottomRight,
            Point::new(2000.0, 2000.0),
            CropAspect::R1x1,
            10.0,
            B,
        );
        assert!(r.right() <= 1000.0 + 1e-3 && r.bottom() <= 1000.0 + 1e-3);
        assert!((r.width - r.height).abs() < 1e-3);
    }

    #[test]
    fn small_frames_and_tiny_images_do_not_panic() {
        // A 5 px frame at the right edge, dragged further right.
        let r = resize(
            Rect::new(395.0, 0.0, 5.0, 5.0),
            Handle::Right,
            Point::new(500.0, 0.0),
            CropAspect::Free,
            10.0,
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        assert!(r.right() <= 400.0);
        // An image smaller than the minimum size.
        let tiny = Rect::new(0.0, 0.0, 6.0, 6.0);
        for h in Handle::ALL {
            let r = resize(Rect::new(1.0, 1.0, 4.0, 4.0), h, Point::new(-5.0, 20.0), CropAspect::Free, 10.0, tiny);
            assert!(r.x >= 0.0 && r.right() <= 6.0 + 1e-4 && r.bottom() <= 6.0 + 1e-4, "{h:?}: {r:?}");
        }
    }

    #[test]
    fn translate_clamps_to_bounds() {
        let r = translate_within(Rect::new(900.0, 900.0, 100.0, 100.0), 50.0, -2000.0, B);
        assert_eq!(r, Rect::new(900.0, 0.0, 100.0, 100.0));
    }
}
