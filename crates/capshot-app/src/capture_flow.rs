//! Ways to get an image into the editor that need its window: paste, import,
//! open a file, and the window picker. Screen captures live in `shell.rs`.

use crate::main_view::MainView;
use crate::window_picker::WindowPicker;
use gpui_kit::*;
use std::path::PathBuf;

pub fn paste(this: &mut MainView, window: &mut Window, cx: &mut Context<MainView>) {
    match capshot_platform::clipboard::read_image() {
        Ok(img) => this.load_image(img, "Clipboard", window, cx),
        Err(e) => this.set_status(e.to_string(), cx),
    }
}

pub fn import(window: &mut Window, cx: &mut Context<MainView>) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Open image".into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = rx.await else { return };
        let Some(path) = paths.into_iter().next() else { return };
        let _ = this.update_in(cx, |this, window, cx| open_path(this, path, window, cx));
    })
    .detach();
}

/// Load an image file into the editor (decoding off the UI thread).
pub fn open_path(_this: &mut MainView, path: PathBuf, window: &mut Window, cx: &mut Context<MainView>) {
    cx.spawn_in(window, async move |this, cx| {
        let p = path.clone();
        let result = cx.background_spawn(async move { image::open(&p).map(|i| i.to_rgba8()) }).await;
        let _ = this.update_in(cx, |this, window, cx| match result {
            Ok(img) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                this.load_image(img, "Opened", window, cx);
                this.set_status(format!("Opened {name}"), cx);
            }
            Err(e) => this.set_status(format!("Could not open {}: {e}", path.display()), cx),
        });
    })
    .detach();
}

pub fn open_window_picker(this: &mut MainView, window: &mut Window, cx: &mut Context<MainView>) {
    let main = cx.entity().downgrade();
    let picker = cx.new(|cx| WindowPicker::new(main, window, cx));
    this.window_picker = Some(picker);
    cx.notify();
}
