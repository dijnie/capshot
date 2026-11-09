//! CSS-style colour strings as stored in winshot configs (`#rgb`, `#rrggbb`,
//! `#rrggbbaa`, `rgb()`, `rgba()`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);
    pub const TRANSPARENT: Rgba = Rgba { r: 0, g: 0, b: 0, a: 0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn with_alpha_f(self, alpha: f32) -> Self {
        Self { a: (self.a as f32 * alpha.clamp(0.0, 1.0)).round() as u8, ..self }
    }

    /// Parse a CSS colour string. Returns `None` for anything unsupported.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            return Self::parse_hex(hex);
        }
        let lower = s.to_ascii_lowercase();
        let inner = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb("))?.strip_suffix(')')?;
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() < 3 || parts.len() > 4 {
            return None;
        }
        let ch = |p: &str| p.parse::<f32>().ok().map(|v| v.clamp(0.0, 255.0).round() as u8);
        let a = match parts.get(3) {
            Some(p) => (p.parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8,
            None => 255,
        };
        Some(Self { r: ch(parts[0])?, g: ch(parts[1])?, b: ch(parts[2])?, a })
    }

    fn parse_hex(hex: &str) -> Option<Self> {
        let digit = |i: usize, len: usize| u8::from_str_radix(hex.get(i..i + len)?, 16).ok();
        match hex.len() {
            3 => {
                let x = |i| digit(i, 1).map(|v| v * 17);
                Some(Self::rgb(x(0)?, x(1)?, x(2)?))
            }
            6 => Some(Self::rgb(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            8 => Some(Self { r: digit(0, 2)?, g: digit(2, 2)?, b: digit(4, 2)?, a: digit(6, 2)? }),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::parse("#667eea"), Some(Rgba::rgb(0x66, 0x7e, 0xea)));
        assert_eq!(Rgba::parse("#00000080").unwrap().a, 0x80);
        assert_eq!(Rgba::parse("#12"), None);
    }

    #[test]
    fn parses_rgb_functions() {
        assert_eq!(Rgba::parse("rgb(1, 2, 3)"), Some(Rgba::rgb(1, 2, 3)));
        assert_eq!(Rgba::parse("rgba(0,0,0,0.5)").unwrap().a, 128);
        assert_eq!(Rgba::parse("hsl(0,0%,0%)"), None);
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(Rgba::parse("#1a1a2e").unwrap().to_hex(), "#1a1a2e");
    }
}
