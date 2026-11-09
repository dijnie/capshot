//! Gaussian blur approximated by three box blurs (accurate to a few percent,
//! linear time in the blur radius). Used for drop shadows.

/// Box widths whose three-pass result approximates a Gaussian of `sigma`.
fn box_sizes(sigma: f32) -> [usize; 3] {
    let n = 3.0;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f32;
    let m_ideal = (12.0 * sigma * sigma - n * wlf * wlf - 4.0 * n * wlf - 3.0 * n) / (-4.0 * wlf - 4.0);
    let m = m_ideal.round() as i64;
    let mut out = [0usize; 3];
    for (i, o) in out.iter_mut().enumerate() {
        *o = if (i as i64) < m { wl as usize } else { wu as usize };
    }
    out
}

/// Blur an 8-bit single-channel image in place. Pixels outside count as 0.
pub fn blur_alpha(data: &mut [u8], width: usize, height: usize, sigma: f32) {
    if sigma < 0.5 || width == 0 || height == 0 {
        return;
    }
    let mut tmp = vec![0u8; data.len()];
    for size in box_sizes(sigma) {
        let r = (size - 1) / 2;
        if r == 0 {
            continue;
        }
        box_pass(data, &mut tmp, width, height, r, true);
        box_pass(&tmp, data, width, height, r, false);
    }
}

fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len, step_line, step_px) = if horizontal { (h, w, w, 1) } else { (w, h, 1, w) };
    let div = (2 * r + 1) as u32;
    for line in 0..lines {
        let base = line * step_line;
        let px = |i: usize| src[base + i * step_px] as u32;
        let mut sum: u32 = (0..=r.min(len - 1)).map(px).sum();
        for i in 0..len {
            dst[base + i * step_px] = ((sum + div / 2) / div) as u8;
            if i + r + 1 < len {
                sum += px(i + r + 1);
            }
            if i >= r {
                sum -= px(i - r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sigma_is_a_no_op() {
        let mut d = vec![0, 255, 0];
        blur_alpha(&mut d, 3, 1, 0.0);
        assert_eq!(d, [0, 255, 0]);
    }

    #[test]
    fn blur_spreads_and_conserves_mass() {
        let (w, h) = (41, 41);
        let mut d = vec![0u8; w * h];
        for y in 15..26 {
            for x in 15..26 {
                d[y * w + x] = 255;
            }
        }
        let before: u32 = d.iter().map(|&v| v as u32).sum();
        blur_alpha(&mut d, w, h, 3.0);
        let after: u32 = d.iter().map(|&v| v as u32).sum();
        assert!((before as f32 - after as f32).abs() / (before as f32) < 0.03);
        assert!(d[20 * w + 12] > 0, "blur should reach outside the square");
        assert!(d[20 * w + 20] > 200, "centre stays nearly opaque");
    }

    #[test]
    fn box_sizes_are_odd() {
        for s in [1.0, 2.5, 10.0, 40.0] {
            assert!(box_sizes(s).iter().all(|b| b % 2 == 1));
        }
    }
}
