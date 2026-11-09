//! Canvas backgrounds. The config stores them as CSS-like strings for
//! compatibility with winshot: a colour, `linear-gradient(...)`, or `url(...)`.

use crate::color::Rgba;

#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Solid(Rgba),
    /// Colour stops at offsets 0..=1, drawn from the top-left to the
    /// bottom-right corner (winshot draws every gradient that way).
    Linear(Vec<(f32, Rgba)>),
    /// Image file drawn with "cover" scaling. Holds the path or data URL.
    Image(String),
}

const FALLBACK: [(f32, Rgba); 2] = [(0.0, Rgba::rgb(0x66, 0x7e, 0xea)), (1.0, Rgba::rgb(0x76, 0x4b, 0xa2))];

impl Background {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if let Some(inner) = t.strip_prefix("url(").and_then(|r| r.strip_suffix(')')) {
            return Self::Image(inner.trim_matches(|c| c == '"' || c == '\'').to_string());
        }
        if t.contains("gradient") {
            return Self::Linear(parse_stops(t));
        }
        Self::Solid(Rgba::parse(t).unwrap_or(Rgba::rgb(0x1a, 0x1a, 0x2e)))
    }
}

/// Same matching rules as winshot's `parseGradient`: every colour, with an
/// optional percentage; a missing first position is 0 and later ones are 1.
fn parse_stops(s: &str) -> Vec<(f32, Rgba)> {
    let mut stops = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let color_len = if let Some(hex) = rest.strip_prefix('#') {
            let n = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
            if n >= 6 {
                Some(7)
            } else if n >= 3 {
                Some(4)
            } else {
                None
            }
        } else if rest.starts_with("rgb") {
            rest.find(')').map(|e| e + 1)
        } else {
            None
        };
        let Some(len) = color_len else {
            i += 1;
            continue;
        };
        let Some(color) = Rgba::parse(&rest[..len]) else {
            i += 1;
            continue;
        };
        i += len;
        while i < s.len() && bytes[i] == b' ' {
            i += 1;
        }
        let digits = s[i..].bytes().take_while(u8::is_ascii_digit).count();
        let pos = if digits > 0 {
            let v: f32 = s[i..i + digits].parse().unwrap_or(0.0);
            i += digits;
            v / 100.0
        } else if stops.is_empty() {
            0.0
        } else {
            1.0
        };
        stops.push((pos, color));
    }
    if stops.is_empty() {
        FALLBACK.to_vec()
    } else {
        stops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::GRADIENT_PRESETS;

    #[test]
    fn parses_two_stop_gradient() {
        let bg = Background::parse("linear-gradient(135deg, #667eea 0%, #764ba2 100%)");
        assert_eq!(
            bg,
            Background::Linear(vec![(0.0, Rgba::rgb(0x66, 0x7e, 0xea)), (1.0, Rgba::rgb(0x76, 0x4b, 0xa2))])
        );
    }

    #[test]
    fn every_preset_parses_to_a_gradient() {
        for p in GRADIENT_PRESETS.iter() {
            match Background::parse(p.value) {
                Background::Linear(stops) => assert!(stops.len() >= 2, "{}", p.name),
                other => panic!("{} parsed as {other:?}", p.name),
            }
        }
        let Background::Linear(mid) = Background::parse(GRADIENT_PRESETS[12].value) else { unreachable!() };
        assert_eq!(mid[1].0, 0.5);
    }

    #[test]
    fn parses_solid_and_image() {
        assert_eq!(Background::parse("#ffffff"), Background::Solid(Rgba::WHITE));
        assert_eq!(Background::parse("url(C:/bg.png)"), Background::Image("C:/bg.png".into()));
        assert_eq!(Background::parse("url(\"a b.jpg\")"), Background::Image("a b.jpg".into()));
    }

    #[test]
    fn unparseable_gradient_falls_back() {
        assert_eq!(Background::parse("linear-gradient(red, blue)"), Background::Linear(FALLBACK.to_vec()));
    }
}
