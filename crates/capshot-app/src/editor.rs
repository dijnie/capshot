//! Editor state and pointer logic, independent of GPUI. Coordinates named
//! `*_orig` are original-image pixels; `screen` points are logical window
//! pixels.

use capshot_core::annotation::{Annotation, AnnotationKind};
use capshot_core::crop::{self, CropAspect, Handle};
use capshot_core::document::{Content, PixelRect};
use capshot_core::edit::{self, Grip, Style, Tool};
use capshot_core::geometry::{Point, Rect};
use capshot_core::history::History;
use capshot_core::{Document, EditorSettings};
use image::RgbaImage;

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 5.0;
pub const ZOOM_STEP: f32 = 0.1;
/// Empty space kept around the canvas when fitting it (winshot uses 40 px a side).
pub const FIT_MARGIN: f32 = 80.0;
/// Grip size in screen pixels.
pub const GRIP_SIZE: f32 = 10.0;

/// Maps canvas pixels to screen pixels: `screen = origin + canvas * scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    pub origin: Point,
    pub scale: f32,
}

impl ViewTransform {
    pub fn fit(bounds: Rect, canvas: (u32, u32), zoom: f32, pan: Point) -> Self {
        let (cw, ch) = (canvas.0.max(1) as f32, canvas.1.max(1) as f32);
        let base = ((bounds.width - FIT_MARGIN) / cw).min((bounds.height - FIT_MARGIN) / ch).clamp(0.01, 1.0);
        let scale = base * zoom;
        let origin = Point::new(
            bounds.x + (bounds.width - cw * scale) / 2.0 + pan.x,
            bounds.y + (bounds.height - ch * scale) / 2.0 + pan.y,
        );
        Self { origin, scale }
    }

    pub fn to_screen(self, p: Point) -> Point {
        Point::new(self.origin.x + p.x * self.scale, self.origin.y + p.y * self.scale)
    }

    pub fn to_canvas(self, p: Point) -> Point {
        Point::new((p.x - self.origin.x) / self.scale, (p.y - self.origin.y) / self.scale)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Create { start: Point },
    Move { last: Point },
    Grip { original: Annotation, grip: Grip },
    Pan { last: Point },
    CropNew { start: Point },
    CropMove { from: Point, start: Rect },
    CropGrip { start: Rect, handle: Handle },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CropSession {
    /// Frame in original-image pixels, `None` until the user draws one.
    pub rect: Option<Rect>,
    /// Crop applied before the session started, restored on cancel.
    pub previous: Option<PixelRect>,
}

pub struct Editor {
    pub doc: Document,
    pub history: History<Content>,
    pub selected: Option<u64>,
    pub tool: Tool,
    pub style: Style,
    pub drag: Option<Drag>,
    pub crop: Option<CropSession>,
    pub crop_aspect: CropAspect,
    pub zoom: f32,
    pub pan: Point,
    /// Bumped on every visible change so the preview knows to re-render.
    pub revision: u64,
    next_id: u64,
}

/// What the view should do after a pointer release.
#[derive(Debug, Default, PartialEq)]
pub struct PointerOutcome {
    /// Start inline editing of this text annotation.
    pub edit_text: Option<u64>,
}

impl Editor {
    pub fn new(image: RgbaImage, settings: EditorSettings) -> Self {
        let crop_aspect = settings.crop_aspect;
        let doc = Document::new(image, settings);
        Self {
            history: History::new(doc.content.clone()),
            doc,
            selected: None,
            tool: Tool::Select,
            style: Style::default(),
            drag: None,
            crop: None,
            crop_aspect,
            zoom: 1.0,
            pan: Point::default(),
            revision: 0,
            next_id: 1,
        }
    }

    pub fn touch(&mut self) {
        self.revision += 1;
    }

    /// Record the current content as an undo step.
    pub fn commit(&mut self) {
        self.history.push(self.doc.content.clone());
        self.touch();
    }

    pub fn selected_annotation(&self) -> Option<&Annotation> {
        let id = self.selected?;
        self.doc.content.annotations.iter().find(|a| a.id == id)
    }

    fn selected_mut(&mut self) -> Option<&mut Annotation> {
        let id = self.selected?;
        self.doc.content.annotations.iter_mut().find(|a| a.id == id)
    }

    /// Image area in original pixels (the bounds for crop frames).
    pub fn original_bounds(&self) -> Rect {
        let o = self.doc.original();
        Rect::new(0.0, 0.0, o.width() as f32, o.height() as f32)
    }

    pub fn set_tool(&mut self, tool: Tool) {
        if tool == self.tool {
            return;
        }
        if self.tool == Tool::Crop && tool != Tool::Crop {
            self.cancel_crop();
        }
        self.tool = tool;
        self.drag = None;
        if tool == Tool::Crop {
            self.begin_crop();
        } else if tool != Tool::Select {
            self.selected = None;
        }
        self.touch();
    }

    // ---- crop -------------------------------------------------------------

    fn begin_crop(&mut self) {
        self.selected = None;
        let previous = self.doc.content.crop;
        let rect = previous.map(|c| Rect::new(c.x as f32, c.y as f32, c.width as f32, c.height as f32));
        // Show the whole original while choosing the frame.
        self.doc.set_crop(None);
        self.crop = Some(CropSession { rect, previous });
    }

    pub fn apply_crop(&mut self) {
        let Some(session) = self.crop.take() else { return };
        let crop = session.rect.map(|r| {
            let r = r.normalized();
            PixelRect {
                x: r.x.round() as u32,
                y: r.y.round() as u32,
                width: r.width.round() as u32,
                height: r.height.round() as u32,
            }
        });
        self.doc.set_crop(crop.or(session.previous));
        self.tool = Tool::Select;
        self.commit();
    }

    pub fn cancel_crop(&mut self) {
        if let Some(session) = self.crop.take() {
            self.doc.set_crop(session.previous);
        }
        if self.tool == Tool::Crop {
            self.tool = Tool::Select;
        }
        self.drag = None;
        self.touch();
    }

    pub fn reset_crop(&mut self) {
        self.crop = None;
        self.doc.set_crop(None);
        self.tool = Tool::Select;
        self.commit();
    }

    pub fn set_crop_aspect(&mut self, aspect: CropAspect) {
        self.crop_aspect = aspect;
        if let Some(CropSession { rect: Some(r), .. }) = &mut self.crop {
            *r = crop::constrain(*r, aspect);
        }
        self.touch();
    }

    // ---- pointer ----------------------------------------------------------

    /// `tolerance` is in original pixels (a few screen pixels at the current zoom).
    pub fn pointer_down(&mut self, p: Point, shift: bool, tolerance: f32) -> PointerOutcome {
        let mut out = PointerOutcome::default();
        match self.tool {
            Tool::Crop => self.crop_down(p, tolerance),
            Tool::Select => {
                if let Some(a) = self.selected_annotation() {
                    if let Some(grip) = edit::grip_at(a, p, tolerance) {
                        self.drag = Some(Drag::Grip { original: a.clone(), grip });
                        return out;
                    }
                }
                self.selected = self.hit_visible(p, tolerance);
                if self.selected.is_some() {
                    self.drag = Some(Drag::Move { last: p });
                }
                self.touch();
            }
            Tool::Text | Tool::Number => {
                let id = self.alloc_id();
                if let Some(a) = edit::begin(self.tool, p, &self.style, id, &self.doc.content.annotations) {
                    let is_text = matches!(a.kind, AnnotationKind::Text { .. });
                    self.doc.content.annotations.push(a);
                    self.selected = Some(id);
                    self.commit();
                    if is_text {
                        out.edit_text = Some(id);
                    }
                }
            }
            tool => {
                let _ = shift;
                let id = self.alloc_id();
                if let Some(a) = edit::begin(tool, p, &self.style, id, &self.doc.content.annotations) {
                    self.doc.content.annotations.push(a);
                    self.selected = Some(id);
                    self.drag = Some(Drag::Create { start: p });
                    self.touch();
                }
            }
        }
        out
    }

    fn crop_down(&mut self, p: Point, tolerance: f32) {
        let Some(session) = &self.crop else { return };
        if let Some(r) = session.rect {
            if let Some(h) = Handle::ALL.into_iter().find(|h| {
                let q = h.position(r);
                (q.x - p.x).abs() <= tolerance && (q.y - p.y).abs() <= tolerance
            }) {
                self.drag = Some(Drag::CropGrip { start: r, handle: h });
                return;
            }
            if r.contains(p) {
                self.drag = Some(Drag::CropMove { from: p, start: r });
                return;
            }
        }
        let b = self.original_bounds();
        let start = Point::new(p.x.clamp(0.0, b.width), p.y.clamp(0.0, b.height));
        self.drag = Some(Drag::CropNew { start });
    }

    pub fn pointer_move(&mut self, p: Point, shift: bool) {
        let Some(drag) = self.drag.clone() else { return };
        match drag {
            Drag::Create { start } => {
                if let Some(a) = self.selected_mut() {
                    edit::drag_to(a, start, p, shift);
                }
            }
            Drag::Move { last } => {
                let (dx, dy) = (p.x - last.x, p.y - last.y);
                if let Some(a) = self.selected_mut() {
                    a.translate(dx, dy);
                }
                self.drag = Some(Drag::Move { last: p });
            }
            Drag::Grip { original, grip } => {
                let updated = edit::drag_grip(&original, grip, p, shift);
                if let Some(a) = self.selected_mut() {
                    *a = updated;
                }
            }
            Drag::CropNew { start } => {
                let b = self.original_bounds();
                let end = Point::new(p.x.clamp(0.0, b.width), p.y.clamp(0.0, b.height));
                let mut r = Rect::from_points(start, end);
                if let Some(ratio) = self.crop_aspect.ratio() {
                    // Grow from the start corner in the drag direction.
                    let mut c =
                        crop::constrain(Rect::new(0.0, 0.0, r.width.max(1.0), r.height.max(1.0)), self.crop_aspect);
                    if c.width / c.height - ratio > 1e-3 {
                        c.width = c.height * ratio;
                    }
                    r = Rect::new(
                        if end.x < start.x { start.x - c.width } else { start.x },
                        if end.y < start.y { start.y - c.height } else { start.y },
                        c.width,
                        c.height,
                    );
                }
                if let Some(s) = &mut self.crop {
                    s.rect = Some(r);
                }
            }
            Drag::CropMove { from, start } => {
                let r = crop::translate_within(start, p.x - from.x, p.y - from.y, self.original_bounds());
                if let Some(s) = &mut self.crop {
                    s.rect = Some(r);
                }
            }
            Drag::CropGrip { start, handle } => {
                let r = crop::resize(start, handle, p, self.crop_aspect, 10.0, self.original_bounds());
                if let Some(s) = &mut self.crop {
                    s.rect = Some(r);
                }
            }
            Drag::Pan { .. } => {}
        }
        self.touch();
    }

    pub fn pointer_up(&mut self) -> PointerOutcome {
        let out = PointerOutcome::default();
        match self.drag.take() {
            Some(Drag::Create { .. }) => {
                let keep = self.selected_annotation().is_some_and(edit::is_big_enough);
                if keep {
                    // Winshot switches back to Select after drawing a line or arrow.
                    if self
                        .selected_annotation()
                        .is_some_and(|a| matches!(a.kind, AnnotationKind::Arrow { .. } | AnnotationKind::Line { .. }))
                    {
                        self.tool = Tool::Select;
                    }
                    self.commit();
                } else {
                    let id = self.selected.take();
                    self.doc.content.annotations.retain(|a| Some(a.id) != id);
                    self.touch();
                }
            }
            Some(Drag::Move { .. }) | Some(Drag::Grip { .. }) => self.commit(),
            Some(Drag::CropNew { .. }) => {
                if let Some(s) = &mut self.crop {
                    if s.rect.is_some_and(|r| r.width < 2.0 || r.height < 2.0) {
                        s.rect = None;
                    }
                }
                self.touch();
            }
            _ => {}
        }
        out
    }

    /// Double-click: edit a text annotation under the pointer.
    pub fn double_click(&mut self, p: Point, tolerance: f32) -> PointerOutcome {
        let id = self.hit_visible(p, tolerance);
        let is_text = id
            .and_then(|id| self.doc.content.annotations.iter().find(|a| a.id == id))
            .is_some_and(|a| matches!(a.kind, AnnotationKind::Text { .. }));
        if is_text {
            self.selected = id;
            self.touch();
            return PointerOutcome { edit_text: id };
        }
        PointerOutcome::default()
    }

    /// Topmost drawn annotation under `p` (cropped-out ones cannot be picked).
    fn hit_visible(&self, p: Point, tolerance: f32) -> Option<u64> {
        let shown: Vec<Annotation> = self.doc.visible_annotations().cloned().collect();
        edit::hit(&shown, p, tolerance)
    }

    /// The document as it should be exported: during a crop session the view
    /// shows the whole original, but exports keep the crop already applied.
    pub fn export_document(&self) -> std::borrow::Cow<'_, capshot_core::Document> {
        match &self.crop {
            Some(session) => {
                let mut d = self.doc.clone();
                d.set_crop(session.previous);
                std::borrow::Cow::Owned(d)
            }
            None => std::borrow::Cow::Borrowed(&self.doc),
        }
    }

    // ---- edits ------------------------------------------------------------

    fn alloc_id(&mut self) -> u64 {
        let used = self.doc.content.annotations.iter().map(|a| a.id).max().unwrap_or(0);
        self.next_id = self.next_id.max(used + 1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn text_of(&self, id: u64) -> Option<String> {
        self.doc.content.annotations.iter().find(|a| a.id == id).and_then(|a| match &a.kind {
            AnnotationKind::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
    }

    /// Set a text annotation's text while typing (no undo step).
    pub fn set_text_live(&mut self, id: u64, value: &str) {
        if let Some(AnnotationKind::Text { text, .. }) =
            self.doc.content.annotations.iter_mut().find(|a| a.id == id).map(|a| &mut a.kind)
        {
            if text != value {
                *text = value.to_string();
                self.touch();
            }
        }
    }

    /// Finish editing: empty text removes the annotation (winshot does the same).
    pub fn finish_text(&mut self, id: u64) {
        if self.text_of(id).is_some_and(|t| t.trim().is_empty()) {
            self.doc.content.annotations.retain(|a| a.id != id);
            if self.selected == Some(id) {
                self.selected = None;
            }
        }
        self.commit();
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(id) = self.selected.take() else { return false };
        self.doc.content.annotations.retain(|a| a.id != id);
        self.commit();
        true
    }

    pub fn undo(&mut self) {
        if let Some(c) = self.history.undo().cloned() {
            self.doc.set_content(c);
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        if let Some(c) = self.history.redo().cloned() {
            self.doc.set_content(c);
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        if self.selected_annotation().is_none() {
            self.selected = None;
        }
        self.crop = None;
        if self.tool == Tool::Crop {
            self.tool = Tool::Select;
        }
        self.touch();
    }

    /// Escape: cancel crop, else drop the selection.
    pub fn escape(&mut self) {
        if self.crop.is_some() {
            self.cancel_crop();
        } else if self.selected.take().is_some() {
            self.touch();
        }
    }

    /// Apply a style change to the selected annotation as one undo step.
    pub fn restyle_selected(&mut self, f: impl FnOnce(&mut Annotation)) {
        if let Some(a) = self.selected_mut() {
            let before = a.clone();
            f(a);
            if *a != before {
                self.commit();
            }
        }
    }

    pub fn zoom_by(&mut self, delta: f32) {
        self.zoom = ((self.zoom + delta) * 100.0).round() / 100.0;
        self.zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.touch();
    }

    pub fn zoom_reset(&mut self) {
        self.zoom = 1.0;
        self.pan = Point::default();
        self.touch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let settings = EditorSettings { auto_background: false, ..EditorSettings::default() };
        Editor::new(RgbaImage::new(400, 300), settings)
    }

    #[test]
    fn view_transform_round_trips_and_fits() {
        let t = ViewTransform::fit(Rect::new(0.0, 0.0, 1080.0, 880.0), (2000, 1000), 1.0, Point::default());
        assert!((t.scale - 0.5).abs() < 1e-6);
        let p = Point::new(123.0, 45.0);
        let back = t.to_canvas(t.to_screen(p));
        assert!((back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3);
        let small = ViewTransform::fit(Rect::new(0.0, 0.0, 1080.0, 880.0), (100, 100), 1.0, Point::default());
        assert_eq!(small.scale, 1.0, "never upscale to fit");
    }

    #[test]
    fn draw_rectangle_then_undo() {
        let mut e = editor();
        e.set_tool(Tool::Rectangle);
        e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        e.pointer_move(Point::new(60.0, 40.0), false);
        e.pointer_up();
        assert_eq!(e.doc.content.annotations.len(), 1);
        assert_eq!(e.doc.content.annotations[0].bounds(), Rect::new(10.0, 10.0, 50.0, 30.0));
        e.undo();
        assert!(e.doc.content.annotations.is_empty());
        e.redo();
        assert_eq!(e.doc.content.annotations.len(), 1);
    }

    #[test]
    fn click_without_drag_creates_nothing() {
        let mut e = editor();
        e.set_tool(Tool::Ellipse);
        e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        e.pointer_up();
        assert!(e.doc.content.annotations.is_empty());
        assert!(!e.history.can_undo());
    }

    #[test]
    fn drawing_an_arrow_returns_to_select() {
        let mut e = editor();
        e.set_tool(Tool::Arrow);
        e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        e.pointer_move(Point::new(100.0, 10.0), false);
        e.pointer_up();
        assert_eq!(e.tool, Tool::Select);
        assert!(e.selected.is_some());
    }

    #[test]
    fn select_and_move() {
        let mut e = editor();
        e.set_tool(Tool::Rectangle);
        e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        e.pointer_move(Point::new(60.0, 40.0), false);
        e.pointer_up();
        e.set_tool(Tool::Select);
        e.selected = None;
        e.pointer_down(Point::new(30.0, 20.0), false, 4.0);
        assert!(e.selected.is_some());
        e.pointer_move(Point::new(40.0, 30.0), false);
        e.pointer_up();
        assert_eq!(e.doc.content.annotations[0].bounds(), Rect::new(20.0, 20.0, 50.0, 30.0));
        e.undo();
        assert_eq!(e.doc.content.annotations[0].bounds(), Rect::new(10.0, 10.0, 50.0, 30.0));
    }

    #[test]
    fn text_tool_starts_editing_and_empty_text_is_removed() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        let out = e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        let id = out.edit_text.expect("editing starts");
        e.finish_text(id);
        assert!(e.doc.content.annotations.is_empty());

        let id = e.pointer_down(Point::new(10.0, 10.0), false, 4.0).edit_text.unwrap();
        e.set_text_live(id, "Hello");
        e.finish_text(id);
        assert_eq!(e.text_of(id).as_deref(), Some("Hello"));
    }

    #[test]
    fn crop_apply_cancel_and_reset() {
        let mut e = editor();
        e.set_tool(Tool::Crop);
        e.pointer_down(Point::new(50.0, 50.0), false, 4.0);
        e.pointer_move(Point::new(250.0, 150.0), false);
        e.pointer_up();
        e.apply_crop();
        assert_eq!(e.doc.image().dimensions(), (200, 100));
        assert_eq!(e.tool, Tool::Select);

        // Re-entering shows the full original with the previous frame.
        e.set_tool(Tool::Crop);
        assert_eq!(e.doc.image().dimensions(), (400, 300));
        assert_eq!(e.crop.as_ref().unwrap().rect, Some(Rect::new(50.0, 50.0, 200.0, 100.0)));
        e.cancel_crop();
        assert_eq!(e.doc.image().dimensions(), (200, 100));

        e.reset_crop();
        assert_eq!(e.doc.image().dimensions(), (400, 300));
        e.undo();
        assert_eq!(e.doc.image().dimensions(), (200, 100));
    }

    #[test]
    fn export_during_crop_session_keeps_applied_crop() {
        let mut e = editor();
        e.set_tool(Tool::Crop);
        e.pointer_down(Point::new(0.0, 0.0), false, 4.0);
        e.pointer_move(Point::new(100.0, 50.0), false);
        e.pointer_up();
        e.apply_crop();
        e.set_tool(Tool::Crop);
        assert_eq!(e.doc.image().dimensions(), (400, 300));
        assert_eq!(e.export_document().image().dimensions(), (100, 50));
    }

    #[test]
    fn cropped_out_annotations_cannot_be_selected() {
        let mut e = editor();
        e.set_tool(Tool::Rectangle);
        e.pointer_down(Point::new(300.0, 200.0), false, 4.0);
        e.pointer_move(Point::new(350.0, 250.0), false);
        e.pointer_up();
        e.set_tool(Tool::Crop);
        e.pointer_down(Point::new(0.0, 0.0), false, 4.0);
        e.pointer_move(Point::new(100.0, 100.0), false);
        e.pointer_up();
        e.apply_crop();
        e.selected = None;
        e.pointer_down(Point::new(320.0, 220.0), false, 4.0);
        assert_eq!(e.selected, None);
    }

    #[test]
    fn crop_with_aspect_keeps_ratio() {
        let mut e = editor();
        e.set_crop_aspect(CropAspect::R1x1);
        e.set_tool(Tool::Crop);
        e.pointer_down(Point::new(0.0, 0.0), false, 4.0);
        e.pointer_move(Point::new(200.0, 100.0), false);
        let r = e.crop.as_ref().unwrap().rect.unwrap();
        assert!((r.width - r.height).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn delete_and_escape() {
        let mut e = editor();
        e.set_tool(Tool::Number);
        e.pointer_down(Point::new(10.0, 10.0), false, 4.0);
        assert!(e.selected.is_some());
        e.escape();
        assert!(e.selected.is_none());
        e.selected = Some(e.doc.content.annotations[0].id);
        assert!(e.delete_selected());
        assert!(e.doc.content.annotations.is_empty());
    }

    #[test]
    fn zoom_is_clamped() {
        let mut e = editor();
        for _ in 0..100 {
            e.zoom_by(ZOOM_STEP);
        }
        assert_eq!(e.zoom, MAX_ZOOM);
        e.zoom_reset();
        assert_eq!(e.zoom, 1.0);
    }
}
