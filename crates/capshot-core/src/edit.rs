//! Editing rules shared by every front end: which tool makes which shape, how
//! a drag shapes it, and how its grips resize it. Points are in original-image
//! pixels.

use crate::annotation::{
    arrow_control_point, Annotation, AnnotationKind, FontStyle, TextAlign, DEFAULT_DIM_OPACITY, DEFAULT_FONT_FAMILY,
    MIN_SHAPE_SIZE,
};
use crate::color::Rgba;
use crate::crop::{self, CropAspect, Handle};
use crate::geometry::{Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Crop,
    Rectangle,
    Ellipse,
    Arrow,
    Line,
    Text,
    Number,
    Spotlight,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Self::Select,
        Self::Rectangle,
        Self::Ellipse,
        Self::Arrow,
        Self::Line,
        Self::Text,
        Self::Number,
        Self::Spotlight,
        Self::Crop,
    ];

    /// Single-key shortcut (winshot's editor shortcuts).
    pub fn shortcut(self) -> char {
        match self {
            Self::Select => 'v',
            Self::Rectangle => 'r',
            Self::Ellipse => 'e',
            Self::Arrow => 'a',
            Self::Line => 'l',
            Self::Text => 't',
            Self::Number => 'n',
            Self::Spotlight => 's',
            Self::Crop => 'c',
        }
    }

    pub fn from_shortcut(c: char) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.shortcut() == c.to_ascii_lowercase())
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Arrow => "Arrow",
            Self::Line => "Line",
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Spotlight => "Spotlight",
            Self::Crop => "Crop",
        }
    }

    /// Tools that make a shape by dragging.
    pub fn is_drag_shape(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse | Self::Arrow | Self::Line | Self::Spotlight)
    }
}

/// The current drawing style from the annotation toolbar.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub stroke: Option<Rgba>,
    pub fill: Option<Rgba>,
    pub stroke_width: f32,
    pub corner_radius: f32,
    pub font_size: f32,
    pub font_family: String,
    pub font_style: FontStyle,
    pub text_align: TextAlign,
    pub dim_opacity: f32,
    pub curved_arrows: bool,
}

/// Winshot's default red, used when a tool needs a colour but none is set.
pub const DEFAULT_RED: Rgba = Rgba::rgb(0xef, 0x44, 0x44);

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke: Some(DEFAULT_RED),
            fill: None,
            stroke_width: 4.0,
            corner_radius: 0.0,
            font_size: 48.0,
            font_family: DEFAULT_FONT_FAMILY.into(),
            font_style: FontStyle::Normal,
            text_align: TextAlign::Left,
            dim_opacity: DEFAULT_DIM_OPACITY,
            curved_arrows: false,
        }
    }
}

/// Start a new annotation for `tool` at `p`. Text and number markers are
/// complete immediately; drag shapes start at zero size.
pub fn begin(tool: Tool, p: Point, style: &Style, id: u64, existing: &[Annotation]) -> Option<Annotation> {
    let colored = style.stroke.or(Some(DEFAULT_RED));
    let boxy = matches!(tool, Tool::Rectangle | Tool::Ellipse);
    let kind = match tool {
        Tool::Select | Tool::Crop => return None,
        Tool::Rectangle => {
            AnnotationKind::Rectangle { rect: Rect::new(p.x, p.y, 0.0, 0.0), corner_radius: style.corner_radius }
        }
        Tool::Ellipse => AnnotationKind::Ellipse { rect: Rect::new(p.x, p.y, 0.0, 0.0) },
        Tool::Spotlight => {
            AnnotationKind::Spotlight { rect: Rect::new(p.x, p.y, 0.0, 0.0), dim_opacity: style.dim_opacity }
        }
        Tool::Arrow => AnnotationKind::Arrow { start: p, end: p, curved: style.curved_arrows, curve_offset: None },
        Tool::Line => AnnotationKind::Line { start: p, end: p },
        Tool::Text => AnnotationKind::Text {
            origin: p,
            text: String::new(),
            font_size: style.font_size,
            font_family: style.font_family.clone(),
            font_style: style.font_style,
            align: style.text_align,
        },
        Tool::Number => AnnotationKind::Number { center: p, value: crate::annotation::next_number(existing) },
    };
    Some(Annotation {
        id,
        kind,
        // Boxes may have no outline; everything else needs a colour.
        stroke: if boxy { style.stroke } else { colored },
        stroke_width: style.stroke_width,
        fill: if boxy { style.fill } else { None },
    })
}

/// Update a shape being drawn from `start` to `current`. Shift keeps boxes
/// square and snaps lines to 45 degrees.
pub fn drag_to(a: &mut Annotation, start: Point, current: Point, shift: bool) {
    let mut end = current;
    match &mut a.kind {
        AnnotationKind::Rectangle { rect, .. }
        | AnnotationKind::Ellipse { rect }
        | AnnotationKind::Spotlight { rect, .. } => {
            if shift {
                let side = (end.x - start.x).abs().max((end.y - start.y).abs());
                end = Point::new(
                    start.x + side * (end.x - start.x).signum(),
                    start.y + side * (end.y - start.y).signum(),
                );
            }
            *rect = Rect::from_points(start, end);
        }
        AnnotationKind::Arrow { start: s, end: e, .. } | AnnotationKind::Line { start: s, end: e } => {
            if shift {
                end = snap_45(start, end);
            }
            *s = start;
            *e = end;
        }
        _ => {}
    }
}

/// Whether a freshly drawn shape is big enough to keep (winshot keeps shapes
/// over 5 px in either direction).
pub fn is_big_enough(a: &Annotation) -> bool {
    match &a.kind {
        AnnotationKind::Text { .. } | AnnotationKind::Number { .. } => true,
        AnnotationKind::Arrow { start, end, .. } | AnnotationKind::Line { start, end } => {
            (end.x - start.x).abs() > MIN_SHAPE_SIZE || (end.y - start.y).abs() > MIN_SHAPE_SIZE
        }
        _ => {
            let r = a.bounds();
            r.width > MIN_SHAPE_SIZE || r.height > MIN_SHAPE_SIZE
        }
    }
}

fn snap_45(from: Point, to: Point) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    let angle = dy.atan2(dx);
    let snapped = (angle / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
    Point::new(from.x + len * snapped.cos(), from.y + len * snapped.sin())
}

/// A draggable point on a selected annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    Box(Handle),
    Start,
    End,
    Control,
}

/// Grips of an annotation and where they sit.
pub fn grips(a: &Annotation) -> Vec<(Grip, Point)> {
    match &a.kind {
        AnnotationKind::Rectangle { rect, .. }
        | AnnotationKind::Ellipse { rect }
        | AnnotationKind::Spotlight { rect, .. } => {
            let r = rect.normalized();
            Handle::ALL.iter().map(|h| (Grip::Box(*h), h.position(r))).collect()
        }
        AnnotationKind::Arrow { start, end, curved, curve_offset } => {
            let mut v = vec![(Grip::Start, *start), (Grip::End, *end)];
            if *curved {
                v.push((Grip::Control, arrow_control_point(*start, *end, *curve_offset)));
            }
            v
        }
        AnnotationKind::Line { start, end } => vec![(Grip::Start, *start), (Grip::End, *end)],
        // Text and number markers move but do not resize (winshot keeps their size).
        AnnotationKind::Text { .. } | AnnotationKind::Number { .. } => Vec::new(),
    }
}

/// Grip under `p`, if any, within `radius`.
pub fn grip_at(a: &Annotation, p: Point, radius: f32) -> Option<Grip> {
    grips(a).into_iter().find(|(_, g)| (g.x - p.x).abs() <= radius && (g.y - p.y).abs() <= radius).map(|(g, _)| g)
}

/// Result of dragging `grip` of `original` to `to`.
pub fn drag_grip(original: &Annotation, grip: Grip, to: Point, shift: bool) -> Annotation {
    let mut a = original.clone();
    match (&mut a.kind, grip) {
        (
            AnnotationKind::Rectangle { rect, .. }
            | AnnotationKind::Ellipse { rect }
            | AnnotationKind::Spotlight { rect, .. },
            Grip::Box(h),
        ) => {
            let start = rect.normalized();
            let aspect = if shift && start.height > 0.0 { Some(start.width / start.height) } else { None };
            let unbounded = Rect::new(f32::MIN / 4.0, f32::MIN / 4.0, f32::MAX / 2.0, f32::MAX / 2.0);
            let mut r = crop::resize(start, h, to, CropAspect::Free, MIN_SHAPE_SIZE, unbounded);
            if let Some(ratio) = aspect {
                r.height = r.width / ratio;
            }
            *rect = r;
        }
        (AnnotationKind::Arrow { start, end, curve_offset, curved }, g) => {
            // Keep the curve's shape relative to the endpoints unless the control
            // point itself is dragged.
            let ctrl = arrow_control_point(*start, *end, *curve_offset);
            match g {
                Grip::Start => *start = if shift { snap_45(*end, to) } else { to },
                Grip::End => *end = if shift { snap_45(*start, to) } else { to },
                Grip::Control if *curved => {
                    let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
                    *curve_offset = Some(Point::new(to.x - mid.x, to.y - mid.y));
                    return a;
                }
                _ => {}
            }
            if matches!(g, Grip::Start | Grip::End) && curve_offset.is_some() {
                let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
                *curve_offset = Some(Point::new(ctrl.x - mid.x, ctrl.y - mid.y));
            }
        }
        (AnnotationKind::Line { start, end }, Grip::Start) => *start = if shift { snap_45(*end, to) } else { to },
        (AnnotationKind::Line { start, end }, Grip::End) => *end = if shift { snap_45(*start, to) } else { to },
        _ => {}
    }
    a
}

/// Topmost annotation under `p` (last drawn wins).
pub fn hit(annotations: &[Annotation], p: Point, tolerance: f32) -> Option<u64> {
    annotations.iter().rev().find(|a| a.hit_test(p, tolerance)).map(|a| a.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn shortcuts_round_trip() {
        for t in Tool::ALL {
            assert_eq!(Tool::from_shortcut(t.shortcut()), Some(t));
        }
        assert_eq!(Tool::from_shortcut('R'), Some(Tool::Rectangle));
        assert_eq!(Tool::from_shortcut('z'), None);
    }

    #[test]
    fn rectangle_drag_normalizes_and_respects_shift() {
        let s = Style::default();
        let mut a = begin(Tool::Rectangle, p(100.0, 100.0), &s, 1, &[]).unwrap();
        drag_to(&mut a, p(100.0, 100.0), p(50.0, 80.0), false);
        assert_eq!(a.bounds(), Rect::new(50.0, 80.0, 50.0, 20.0));
        drag_to(&mut a, p(100.0, 100.0), p(150.0, 120.0), true);
        assert_eq!(a.bounds(), Rect::new(100.0, 100.0, 50.0, 50.0));
    }

    #[test]
    fn boxes_keep_no_outline_but_lines_get_red() {
        let s = Style { stroke: None, fill: Some(Rgba::WHITE), ..Style::default() };
        let r = begin(Tool::Rectangle, p(0.0, 0.0), &s, 1, &[]).unwrap();
        assert_eq!((r.stroke, r.fill), (None, Some(Rgba::WHITE)));
        let l = begin(Tool::Line, p(0.0, 0.0), &s, 2, &[]).unwrap();
        assert_eq!((l.stroke, l.fill), (Some(DEFAULT_RED), None));
    }

    #[test]
    fn number_tool_uses_next_free_number() {
        let s = Style::default();
        let first = begin(Tool::Number, p(0.0, 0.0), &s, 1, &[]).unwrap();
        let second = begin(Tool::Number, p(0.0, 0.0), &s, 2, std::slice::from_ref(&first)).unwrap();
        assert!(matches!(second.kind, AnnotationKind::Number { value: 2, .. }));
    }

    #[test]
    fn tiny_shapes_are_discarded() {
        let s = Style::default();
        let mut a = begin(Tool::Ellipse, p(0.0, 0.0), &s, 1, &[]).unwrap();
        drag_to(&mut a, p(0.0, 0.0), p(3.0, 4.0), false);
        assert!(!is_big_enough(&a));
        drag_to(&mut a, p(0.0, 0.0), p(3.0, 6.0), false);
        assert!(is_big_enough(&a));
    }

    #[test]
    fn shift_snaps_lines_to_45_degrees() {
        let s = Style::default();
        let mut a = begin(Tool::Line, p(0.0, 0.0), &s, 1, &[]).unwrap();
        drag_to(&mut a, p(0.0, 0.0), p(100.0, 10.0), true);
        let AnnotationKind::Line { end, .. } = a.kind else { unreachable!() };
        assert!((end.y).abs() < 1e-3 && (end.x - 100.498).abs() < 0.01);
    }

    #[test]
    fn box_grip_resizes_from_opposite_corner() {
        let a = Annotation {
            id: 1,
            kind: AnnotationKind::Rectangle { rect: Rect::new(10.0, 10.0, 100.0, 50.0), corner_radius: 0.0 },
            stroke: None,
            stroke_width: 2.0,
            fill: None,
        };
        let b = drag_grip(&a, Grip::Box(Handle::BottomRight), p(210.0, 110.0), false);
        assert_eq!(b.bounds(), Rect::new(10.0, 10.0, 200.0, 100.0));
        let c = drag_grip(&a, Grip::Box(Handle::TopLeft), p(0.0, 0.0), false);
        assert_eq!(c.bounds(), Rect::new(0.0, 0.0, 110.0, 60.0));
        assert_eq!(grip_at(&a, p(111.0, 59.0), 4.0), Some(Grip::Box(Handle::BottomRight)));
    }

    #[test]
    fn arrow_grips_move_endpoints_and_control() {
        let a = Annotation {
            id: 1,
            kind: AnnotationKind::Arrow { start: p(0.0, 0.0), end: p(100.0, 0.0), curved: true, curve_offset: None },
            stroke: Some(DEFAULT_RED),
            stroke_width: 4.0,
            fill: None,
        };
        assert_eq!(grips(&a).len(), 3);
        let b = drag_grip(&a, Grip::Control, p(50.0, 40.0), false);
        let AnnotationKind::Arrow { curve_offset, .. } = b.kind else { unreachable!() };
        assert_eq!(curve_offset, Some(p(0.0, 40.0)));
        let c = drag_grip(&a, Grip::End, p(200.0, 0.0), false);
        let AnnotationKind::Arrow { end, .. } = c.kind else { unreachable!() };
        assert_eq!(end, p(200.0, 0.0));
    }

    #[test]
    fn hit_prefers_topmost() {
        let mk = |id, x| Annotation {
            id,
            kind: AnnotationKind::Rectangle { rect: Rect::new(x, 0.0, 50.0, 50.0), corner_radius: 0.0 },
            stroke: None,
            stroke_width: 1.0,
            fill: None,
        };
        let list = vec![mk(1, 0.0), mk(2, 25.0)];
        assert_eq!(hit(&list, p(30.0, 10.0), 0.0), Some(2));
        assert_eq!(hit(&list, p(5.0, 10.0), 0.0), Some(1));
        assert_eq!(hit(&list, p(200.0, 10.0), 0.0), None);
    }
}
