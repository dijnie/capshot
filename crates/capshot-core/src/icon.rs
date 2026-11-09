//! The capshot icon, drawn in code so every platform gets the same pixels: a
//! violet rounded square with white viewfinder corners.

/// RGBA pixels of a `size`×`size` icon.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = s * 0.22;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square coverage with a one-pixel soft edge.
            let dx = (radius - fx).max(fx - (s - radius)).max(0.0);
            let dy = (radius - fy).max(fy - (s - radius)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt() - radius;
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            // Diagonal violet gradient (#8b5cf6 → #6d28d9).
            let t = (fx + fy) / (2.0 * s);
            let mix = |a: f32, b: f32| a + (b - a) * t;
            let mut c = [mix(139.0, 109.0), mix(92.0, 40.0), mix(246.0, 217.0)];
            // Viewfinder corners: L-shapes inset from each corner.
            let (m, len, w) = (s * 0.24, s * 0.18, (s * 0.07).max(1.0));
            let near = |v: f32, edge: f32| (v - edge).abs() <= w / 2.0;
            let within = |v: f32, a: f32, b: f32| v >= a.min(b) && v <= a.max(b);
            let corner = [(m, m, 1.0, 1.0), (s - m, m, -1.0, 1.0), (m, s - m, 1.0, -1.0), (s - m, s - m, -1.0, -1.0)]
                .iter()
                .any(|&(cx, cy, sx, sy)| {
                    (near(fy, cy) && within(fx, cx - sx * w / 2.0, cx + sx * len))
                        || (near(fx, cx) && within(fy, cy - sy * w / 2.0, cy + sy * len))
                });
            let dot = ((fx - s / 2.0).powi(2) + (fy - s / 2.0).powi(2)).sqrt() <= s * 0.09;
            if corner || dot {
                c = [255.0, 255.0, 255.0];
            }
            let i = ((y * size + x) * 4) as usize;
            out[i] = c[0] as u8;
            out[i + 1] = c[1] as u8;
            out[i + 2] = c[2] as u8;
            out[i + 3] = (cover * 255.0) as u8;
        }
    }
    out
}

/// Windows .ico holding PNG-compressed images at the given sizes.
pub fn ico(sizes: &[u32]) -> Vec<u8> {
    let pngs: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let img = image::RgbaImage::from_raw(s, s, rgba(s)).expect("icon buffer size");
            crate::export::encode(&img, crate::config::ImageFormat::Png, 100).expect("png encode")
        })
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, png) in sizes.iter().zip(&pngs) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        out.extend_from_slice(&png);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_has_transparent_corners_and_opaque_centre() {
        let px = super::rgba(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        assert_eq!(px[3], 0, "top-left corner is transparent");
        let centre = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(&px[centre..centre + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn ico_header_lists_every_size() {
        let ico = super::ico(&[16, 256]);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(ico[6], 16);
        assert_eq!(ico[22], 0, "256 px is stored as 0");
    }
}
