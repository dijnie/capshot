//! The editable document: original capture, non-destructive crop, annotations
//! and the look settings.

use crate::annotation::Annotation;
use crate::config::EditorConfig;
use crate::geometry::{Layout, Point};
use image::RgbaImage;
use std::sync::Arc;

pub type EditorSettings = EditorConfig;

/// Crop rectangle in original-image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The part of the document that undo/redo tracks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Content {
    /// In original-image pixels, so cropping never moves or deletes them.
    pub annotations: Vec<Annotation>,
    pub crop: Option<PixelRect>,
}

#[derive(Debug, Clone)]
pub struct Document {
    original: Arc<RgbaImage>,
    /// `original` cut to `content.crop`, kept so rendering never re-crops.
    visible: Arc<RgbaImage>,
    pub content: Content,
    pub settings: EditorSettings,
}

impl Document {
    /// New document. When auto background is on, the background and inset
    /// colour follow the capture's dominant edge colour, as winshot does.
    pub fn new(image: RgbaImage, mut settings: EditorSettings) -> Self {
        if settings.auto_background {
            let c = crate::edge_color::dominant_edge_color(&image).to_hex();
            settings.background_color = c.clone();
            settings.inset_background_color = Some(c);
        }
        let original = Arc::new(image);
        Self { visible: original.clone(), original, content: Content::default(), settings }
    }

    pub fn original(&self) -> &Arc<RgbaImage> {
        &self.original
    }

    /// The image as currently shown (cropped when a crop is applied).
    pub fn image(&self) -> &Arc<RgbaImage> {
        &self.visible
    }

    /// Offset of the visible image inside the original.
    pub fn crop_origin(&self) -> Point {
        self.content.crop.map_or(Point::default(), |c| Point::new(c.x as f32, c.y as f32))
    }

    /// Annotations that are drawn: with a crop applied, the ones entirely
    /// outside it are hidden (winshot dropped them; capshot keeps them for a
    /// later re-crop).
    pub fn visible_annotations(&self) -> impl Iterator<Item = &crate::annotation::Annotation> {
        let visible = self
            .content
            .crop
            .map(|c| crate::geometry::Rect::new(c.x as f32, c.y as f32, c.width as f32, c.height as f32));
        self.content.annotations.iter().filter(move |a| visible.is_none_or(|v| a.bounds().intersects(&v)))
    }

    pub fn layout(&self) -> Layout {
        let s = &self.settings;
        let padding = if s.show_background { s.padding as f32 } else { 0.0 };
        let inset = if s.show_background { s.inset as f32 } else { 0.0 };
        Layout::compute(self.visible.width(), self.visible.height(), padding, s.output_ratio, inset)
    }

    /// Replace the tracked content (used by undo/redo and crop changes).
    pub fn set_content(&mut self, content: Content) {
        let crop_changed = content.crop != self.content.crop;
        self.content = content;
        if crop_changed {
            self.refresh_visible();
        }
    }

    pub fn set_crop(&mut self, crop: Option<PixelRect>) {
        let mut c = self.content.clone();
        c.crop = crop.and_then(|r| clamp_crop(r, self.original.width(), self.original.height()));
        self.set_content(c);
    }

    fn refresh_visible(&mut self) {
        self.visible = match self.content.crop {
            None => self.original.clone(),
            Some(r) => Arc::new(image::imageops::crop_imm(&*self.original, r.x, r.y, r.width, r.height).to_image()),
        };
    }

    /// Map a point on the output canvas to original-image pixels.
    pub fn canvas_to_original(&self, p: Point) -> Point {
        let o = self.crop_origin();
        let q = self.layout().canvas_to_image(p);
        Point::new(q.x + o.x, q.y + o.y)
    }

    /// Map original-image pixels to a point on the output canvas.
    pub fn original_to_canvas(&self, p: Point) -> Point {
        let o = self.crop_origin();
        self.layout().image_to_canvas(Point::new(p.x - o.x, p.y - o.y))
    }
}

fn clamp_crop(r: PixelRect, w: u32, h: u32) -> Option<PixelRect> {
    let x = r.x.min(w.saturating_sub(1));
    let y = r.y.min(h.saturating_sub(1));
    let width = r.width.min(w - x);
    let height = r.height.min(h - y);
    (width > 0 && height > 0).then_some(PixelRect { x, y, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(w: u32, h: u32) -> Document {
        let settings = EditorSettings { auto_background: false, ..EditorSettings::default() };
        Document::new(RgbaImage::new(w, h), settings)
    }

    #[test]
    fn crop_changes_visible_image_but_keeps_original() {
        let mut d = doc(100, 80);
        d.set_crop(Some(PixelRect { x: 10, y: 20, width: 50, height: 30 }));
        assert_eq!(d.image().dimensions(), (50, 30));
        assert_eq!(d.original().dimensions(), (100, 80));
        d.set_crop(None);
        assert_eq!(d.image().dimensions(), (100, 80));
    }

    #[test]
    fn crop_is_clamped_to_the_image() {
        let mut d = doc(100, 80);
        d.set_crop(Some(PixelRect { x: 90, y: 70, width: 50, height: 50 }));
        assert_eq!(d.content.crop, Some(PixelRect { x: 90, y: 70, width: 10, height: 10 }));
        d.set_crop(Some(PixelRect { x: 0, y: 0, width: 0, height: 10 }));
        assert_eq!(d.content.crop, None);
    }

    #[test]
    fn canvas_mapping_accounts_for_crop_and_padding() {
        let mut d = doc(100, 80);
        d.settings.padding = 40;
        d.set_crop(Some(PixelRect { x: 10, y: 20, width: 50, height: 30 }));
        let p = Point::new(15.0, 25.0);
        let c = d.original_to_canvas(p);
        assert_eq!(c, Point::new(45.0, 45.0));
        assert_eq!(d.canvas_to_original(c), p);
    }

    #[test]
    fn auto_background_uses_edge_colour() {
        let img = RgbaImage::from_pixel(20, 20, image::Rgba([255, 255, 255, 255]));
        let d = Document::new(img, EditorSettings::default());
        assert_eq!(d.settings.background_color, "#ffffff");
        assert_eq!(d.settings.inset_background_color.as_deref(), Some("#ffffff"));
    }
}
