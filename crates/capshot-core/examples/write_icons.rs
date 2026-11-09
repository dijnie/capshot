//! Writes the app icon files used by installers:
//! `cargo run -p capshot-core --example write_icons -- assets/icons`.

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "assets/icons".into()));
    std::fs::create_dir_all(&dir).unwrap();
    for size in [32u32, 128, 256, 512, 1024] {
        let img = image::RgbaImage::from_raw(size, size, capshot_core::icon::rgba(size)).unwrap();
        img.save(dir.join(format!("icon-{size}.png"))).unwrap();
    }
    std::fs::write(dir.join("icon.ico"), capshot_core::icon::ico(&[16, 24, 32, 48, 64, 128, 256])).unwrap();
    println!("icons written to {}", dir.display());
}
