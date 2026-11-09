//! Dominant edge colour of a capture, used for the automatic background.
//! Port of winshot's `extractDominantEdgeColor`.

use crate::color::Rgba;
use image::RgbaImage;

pub const DEFAULT_EDGE_COLOR: Rgba = Rgba::rgb(0x1a, 0x1a, 0x2e);
const ALPHA_THRESHOLD: u8 = 128;
const QUANTIZE_STEP: f32 = 32.0;
const MAX_DIM: u32 = 1920;
const SAMPLE_RATE: usize = 10;

pub fn dominant_edge_color(img: &RgbaImage) -> Rgba {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return DEFAULT_EDGE_COLOR;
    }
    let scale = (MAX_DIM as f32 / w.max(h) as f32).min(1.0);
    let small;
    let img = if scale < 1.0 {
        let nw = ((w as f32 * scale) as u32).max(1);
        let nh = ((h as f32 * scale) as u32).max(1);
        small = image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle);
        &small
    } else {
        img
    };
    let (w, h) = img.dimensions();

    let mut counts: Vec<(Rgba, u32)> = Vec::new();
    let mut add = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        if p[3] < ALPHA_THRESHOLD {
            return;
        }
        let q = |v: u8| ((v as f32 / QUANTIZE_STEP).round() * QUANTIZE_STEP).clamp(0.0, 255.0) as u8;
        let key = Rgba::rgb(q(p[0]), q(p[1]), q(p[2]));
        match counts.iter_mut().find(|(c, _)| *c == key) {
            Some((_, n)) => *n += 1,
            None => counts.push((key, 1)),
        }
    };

    for x in (0..w).step_by(SAMPLE_RATE) {
        add(x, 0);
        add(x, h - 1);
    }
    if h > 2 {
        for y in (1..h - 1).step_by(SAMPLE_RATE) {
            add(0, y);
            add(w - 1, y);
        }
    }

    // First colour reaching the highest count wins, matching Map iteration order.
    let mut best = (DEFAULT_EDGE_COLOR, 0);
    for (c, n) in counts {
        if n > best.1 {
            best = (c, n);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_image_returns_its_quantized_colour() {
        let img = RgbaImage::from_pixel(50, 50, image::Rgba([30, 30, 50, 255]));
        assert_eq!(dominant_edge_color(&img), Rgba::rgb(32, 32, 64));
    }

    #[test]
    fn transparent_image_returns_default() {
        let img = RgbaImage::new(50, 50);
        assert_eq!(dominant_edge_color(&img), DEFAULT_EDGE_COLOR);
    }

    #[test]
    fn border_colour_beats_centre_colour() {
        let mut img = RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        for y in 10..90 {
            for x in 10..90 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        assert_eq!(dominant_edge_color(&img), Rgba::rgb(255, 255, 255));
    }
}
