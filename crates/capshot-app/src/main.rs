//! capshot: capture, annotate and export screenshots.

mod actions;
mod assets;
mod capture_flow;
mod cli;
mod cloud_settings;
mod editor;
mod library_window;
mod main_view;
mod overlay;
mod preview;
mod settings_dialog;
mod settings_panel;
mod shell;
mod update_dialog;
mod window_picker;

use capshot_core::config::{Config, LoadSource};
use gpui_kit::*;

fn main() {
    let path = Config::default_path().unwrap_or_else(|_| std::path::PathBuf::from("capshot-config.json"));
    let winshot = Config::winshot_path();
    let (config, source) = match Config::load_or_init(&path, winshot.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("capshot: could not load settings ({e}); using defaults");
            (Config::default(), LoadSource::Defaults)
        }
    };

    let cli = cli::parse(std::env::args_os().skip(1));
    // One capshot per user: later launches forward their arguments and exit.
    let (instance_tx, instance_rx) = std::sync::mpsc::channel();
    // Paths are made absolute: the running instance has its own working directory.
    let raw_args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .map(|a| {
            if a.starts_with("--") {
                a
            } else {
                std::path::absolute(&a).map(|p| p.to_string_lossy().into_owned()).unwrap_or(a)
            }
        })
        .collect();
    match capshot_platform::single_instance::acquire(&raw_args, instance_tx) {
        Ok(capshot_platform::single_instance::Instance::Forwarded) => return,
        Ok(_) => {}
        Err(e) => eprintln!("capshot: single-instance check failed ({e}); continuing"),
    }

    // `capshot <image>` opens a file at start ("Open with" support).
    let initial = cli.open.clone().and_then(|p| match image::open(&p) {
        Ok(img) => Some((img.to_rgba8(), p)),
        Err(e) => {
            eprintln!("capshot: could not open {}: {e}", p.display());
            None
        }
    });

    let start_hidden = config.startup.minimize_to_tray && cli.capture.is_none() && initial.is_none();
    gpui_kit::application().with_assets(assets::Assets).with_quit_mode(QuitMode::Explicit).run(move |cx| {
        gpui_kit::init(cx);
        actions::bind_keys(cx);
        let view = cx.new(|cx| {
            let mut view = main_view::MainView::new(config, path, cx);
            match source {
                LoadSource::ImportedFromWinshot(p) => {
                    view.status = format!("Imported settings from {}", p.display()).into()
                }
                LoadSource::InvalidFile(e) => {
                    view.status = format!("Settings file unreadable ({e}); using defaults").into()
                }
                _ => {}
            }
            view
        });
        shell::start(cx, view, !start_hidden, instance_rx);
        if let Some(mode) = cli.capture {
            shell::capture(mode, cx);
        }
        if let Some((img, p)) = initial {
            shell::with_view(cx, move |view, window, cx| {
                view.load_image(img, "Opened", window, cx);
                view.set_status(format!("Opened {}", p.display()), cx);
            });
        }
    });
}
