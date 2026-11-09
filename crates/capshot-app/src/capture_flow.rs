//! Capture actions: hide the editor, grab pixels off the UI thread, then show
//! the result.

use crate::main_view::MainView;
use crate::window_picker::WindowPicker;
use capshot_platform::capture;
use gpui_kit::*;
use image::RgbaImage;
use std::path::PathBuf;
use std::time::Duration;

/// Time for the editor window to get out of the way before capturing.
const HIDE_DELAY: Duration = Duration::from_millis(400);

type CaptureFn = Box<dyn FnOnce() -> Result<RgbaImage, String> + Send>;

/// Minimise the editor, run `capture` in the background, then restore the
/// editor with the result.
fn hide_and_capture(label: &'static str, capture: CaptureFn, window: &mut Window, cx: &mut Context<MainView>) {
    window.minimize_window();
    cx.spawn_in(window, async move |this, cx| {
        cx.background_executor().timer(HIDE_DELAY).await;
        let result = cx.background_spawn(async move { capture() }).await;
        let _ = this.update_in(cx, |this, window, cx| {
            window.activate_window();
            match result {
                Ok(img) => this.load_image(img, label, window, cx),
                Err(e) => this.set_status(format!("{label} capture failed: {e}"), cx),
            }
        });
    })
    .detach();
}

pub fn fullscreen(window: &mut Window, cx: &mut Context<MainView>) {
    hide_and_capture(
        "Fullscreen",
        Box::new(|| capture::capture_virtual_screen().map_err(|e| e.to_string())),
        window,
        cx,
    );
}

pub fn capture_window_by_id(id: u32, window: &mut Window, cx: &mut Context<MainView>) {
    hide_and_capture("Window", Box::new(move || capture::capture_window(id).map_err(|e| e.to_string())), window, cx);
}

/// Region: freeze every display, then let the user drag a rectangle on an
/// overlay window per display.
pub fn region(window: &mut Window, cx: &mut Context<MainView>) {
    window.minimize_window();
    let main_window = window.window_handle();
    cx.spawn_in(window, async move |this, cx| {
        cx.background_executor().timer(HIDE_DELAY).await;
        let shots = cx.background_spawn(async move { capture::capture_all_displays() }).await;
        let _ = this.update_in(cx, |this, window, cx| match shots {
            Ok(shots) => crate::overlay::open(shots, cx.entity().downgrade(), main_window, cx),
            Err(e) => {
                window.activate_window();
                this.set_status(format!("Region capture failed: {e}"), cx);
            }
        });
    })
    .detach();
}

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
