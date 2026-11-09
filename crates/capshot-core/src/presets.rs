//! Gradient presets, copied verbatim from winshot's settings panel so saved
//! configs keep the same backgrounds.

pub struct GradientPreset {
    pub name: &'static str,
    pub value: &'static str,
}

pub const GRADIENT_PRESETS: [GradientPreset; 24] = [
    GradientPreset { name: "Sunset", value: "linear-gradient(135deg, #f093fb 0%, #f5576c 100%)" },
    GradientPreset { name: "Ocean", value: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)" },
    GradientPreset { name: "Forest", value: "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)" },
    GradientPreset { name: "Fire", value: "linear-gradient(135deg, #f12711 0%, #f5af19 100%)" },
    GradientPreset { name: "Cool Blue", value: "linear-gradient(135deg, #2193b0 0%, #6dd5ed 100%)" },
    GradientPreset { name: "Lavender", value: "linear-gradient(135deg, #c471f5 0%, #fa71cd 100%)" },
    GradientPreset { name: "Aqua", value: "linear-gradient(135deg, #13547a 0%, #80d0c7 100%)" },
    GradientPreset { name: "Grape", value: "linear-gradient(135deg, #5f2c82 0%, #49a09d 100%)" },
    GradientPreset { name: "Peach", value: "linear-gradient(135deg, #ffecd2 0%, #fcb69f 100%)" },
    GradientPreset { name: "Sky", value: "linear-gradient(135deg, #a8edea 0%, #fed6e3 100%)" },
    GradientPreset { name: "Warm", value: "linear-gradient(135deg, #ff9a9e 0%, #fecfef 100%)" },
    GradientPreset { name: "Mint", value: "linear-gradient(135deg, #d4fc79 0%, #96e6a1 100%)" },
    GradientPreset { name: "Midnight", value: "linear-gradient(135deg, #0f0c29 0%, #302b63 50%, #24243e 100%)" },
    GradientPreset { name: "Carbon", value: "linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)" },
    GradientPreset { name: "Deep Space", value: "linear-gradient(135deg, #000428 0%, #004e92 100%)" },
    GradientPreset { name: "Noir", value: "linear-gradient(135deg, #232526 0%, #414345 100%)" },
    GradientPreset { name: "Royal", value: "linear-gradient(135deg, #141e30 0%, #243b55 100%)" },
    GradientPreset { name: "Rose Gold", value: "linear-gradient(135deg, #f4c4f3 0%, #fc67fa 100%)" },
    GradientPreset { name: "Emerald", value: "linear-gradient(135deg, #1d976c 0%, #93f9b9 100%)" },
    GradientPreset { name: "Amethyst", value: "linear-gradient(135deg, #9d50bb 0%, #6e48aa 100%)" },
    GradientPreset { name: "Neon", value: "linear-gradient(135deg, #00f260 0%, #0575e6 100%)" },
    GradientPreset { name: "Aurora", value: "linear-gradient(135deg, #00c6fb 0%, #005bea 100%)" },
    GradientPreset { name: "Candy", value: "linear-gradient(135deg, #ff6a88 0%, #ff99ac 100%)" },
    GradientPreset { name: "Clean", value: "linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%)" },
];

/// Default background for a fresh config ("Ocean").
pub const DEFAULT_BACKGROUND: &str = "linear-gradient(135deg, #667eea 0%, #764ba2 100%)";

/// Annotation colours, in winshot's toolbar order.
pub const ANNOTATION_COLORS: [&str; 12] = [
    "#ef4444", "#f97316", "#eab308", "#22c55e", "#0ea5e9", "#6366f1", "#a855f7", "#ec4899", "#ffffff", "#000000",
    "#64748b", "#1e293b",
];
pub const STROKE_WIDTHS: [f32; 5] = [2.0, 4.0, 6.0, 8.0, 10.0];
pub const CORNER_RADII: [f32; 5] = [0.0, 4.0, 8.0, 12.0, 20.0];
pub const FONT_SIZES: [f32; 7] = [16.0, 24.0, 32.0, 48.0, 64.0, 80.0, 96.0];
