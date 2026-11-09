//! Times a full 4K render and a quarter-scale preview render.
//! Run with `cargo run --release -p capshot-core --example render_bench`.

use capshot_core::annotation::{Annotation, AnnotationKind, FontStyle, TextAlign};
use capshot_core::color::Rgba;
use capshot_core::geometry::{Point, Rect};
use capshot_core::render::{render, RenderOptions};
use capshot_core::{Document, EditorSettings};
use std::time::Instant;

fn main() {
    let img = image::RgbaImage::from_fn(3840, 2160, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
    let mut doc = Document::new(
        img,
        EditorSettings { shadow_size: 30, corner_radius: 16, border_enabled: true, ..Default::default() },
    );
    let red = Some(Rgba::rgb(239, 68, 68));
    let shapes = [
        AnnotationKind::Rectangle { rect: Rect::new(200.0, 200.0, 800.0, 400.0), corner_radius: 8.0 },
        AnnotationKind::Ellipse { rect: Rect::new(1200.0, 300.0, 600.0, 300.0) },
        AnnotationKind::Arrow {
            start: Point::new(100.0, 1500.0),
            end: Point::new(1500.0, 900.0),
            curved: true,
            curve_offset: None,
        },
        AnnotationKind::Line { start: Point::new(2000.0, 100.0), end: Point::new(3500.0, 900.0) },
        AnnotationKind::Text {
            origin: Point::new(400.0, 1700.0),
            text: "Release notes\nVersion 2".into(),
            font_size: 72.0,
            font_family: "Inter".into(),
            font_style: FontStyle::Bold,
            align: TextAlign::Center,
        },
        AnnotationKind::Number { center: Point::new(3000.0, 1600.0), value: 3 },
        AnnotationKind::Spotlight { rect: Rect::new(2500.0, 1200.0, 900.0, 700.0), dim_opacity: 0.6 },
    ];
    for (i, kind) in shapes.into_iter().enumerate() {
        doc.content.annotations.push(Annotation { id: i as u64, kind, stroke: red, stroke_width: 6.0, fill: None });
    }

    for (label, scale) in [("export 1.0x", 1.0), ("preview 0.25x", 0.25)] {
        let _ = render(&doc, RenderOptions { scale, include_background: true }); // warm caches
        let n = 5;
        let t = Instant::now();
        for _ in 0..n {
            std::hint::black_box(render(&doc, RenderOptions { scale, include_background: true }));
        }
        println!("{label}: {:.1} ms per render", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    }
}
