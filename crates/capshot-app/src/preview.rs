//! Turns compositor output into a GPUI image.

use capshot_core::render::{self, RenderOptions};
use capshot_core::Document;
use gpui_kit::RenderImage;
use std::sync::Arc;

/// Largest preview we render, in pixels; above this the GPU scales it up.
const MAX_PREVIEW_PIXELS: f32 = 16_000_000.0;

pub struct Preview {
    pub image: Arc<RenderImage>,
    pub revision: u64,
    pub scale: f32,
}

/// Render `doc` at `scale` output pixels per canvas pixel (capped) as BGRA.
pub fn render_preview(doc: &Document, scale: f32) -> (Arc<RenderImage>, f32) {
    let layout = render::layout_for(doc, &RenderOptions::default());
    let area = layout.canvas_width as f32 * layout.canvas_height as f32;
    let cap = (MAX_PREVIEW_PIXELS / area.max(1.0)).sqrt();
    let scale = scale.min(cap).max(0.02);
    let pm = render::render(doc, RenderOptions { scale, include_background: true });
    let mut buf = image::RgbaImage::new(pm.width(), pm.height());
    for (dst, src) in buf.pixels_mut().zip(pm.pixels()) {
        let c = src.demultiply();
        dst.0 = [c.blue(), c.green(), c.red(), c.alpha()];
    }
    let frame = image::Frame::new(buf);
    (Arc::new(RenderImage::new(smallvec::smallvec![frame])), scale)
}
