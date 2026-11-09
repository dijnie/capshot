//! Keyboard actions and their default bindings (winshot's shortcuts; `secondary`
//! is Cmd on macOS and Ctrl elsewhere).

use gpui_kit::{App, KeyBinding};

gpui_kit::actions!(
    capshot,
    [
        Undo,
        Redo,
        DeleteSelected,
        Escape,
        ImportImage,
        PasteImage,
        QuickSave,
        SaveAs,
        CopyImage,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToolSelect,
        ToolRectangle,
        ToolEllipse,
        ToolArrow,
        ToolLine,
        ToolText,
        ToolNumber,
        ToolSpotlight,
        ToolCrop,
        ApplyCrop,
        Quit,
    ]
);

pub const APP_CONTEXT: &str = "CapshotApp";
pub const CANVAS_CONTEXT: &str = "CapshotCanvas";

/// `cmd-<keys>` on macOS, `ctrl-<keys>` elsewhere.
macro_rules! primary {
    ($keys:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $keys)
        } else {
            concat!("ctrl-", $keys)
        }
    };
}

pub fn bind_keys(cx: &mut App) {
    let app = Some(APP_CONTEXT);
    let canvas = Some(CANVAS_CONTEXT);
    cx.bind_keys([
        KeyBinding::new(primary!("z"), Undo, app),
        KeyBinding::new(primary!("shift-z"), Redo, app),
        KeyBinding::new(primary!("y"), Redo, app),
        KeyBinding::new(primary!("o"), ImportImage, app),
        KeyBinding::new(primary!("v"), PasteImage, app),
        KeyBinding::new(primary!("s"), QuickSave, app),
        KeyBinding::new(primary!("shift-s"), SaveAs, app),
        KeyBinding::new(primary!("c"), CopyImage, app),
        KeyBinding::new(primary!("="), ZoomIn, app),
        KeyBinding::new(primary!("+"), ZoomIn, app),
        KeyBinding::new(primary!("-"), ZoomOut, app),
        KeyBinding::new(primary!("0"), ZoomReset, app),
        KeyBinding::new(primary!("q"), Quit, app),
        KeyBinding::new("delete", DeleteSelected, canvas),
        KeyBinding::new("backspace", DeleteSelected, canvas),
        KeyBinding::new("escape", Escape, canvas),
        KeyBinding::new("enter", ApplyCrop, canvas),
        KeyBinding::new("v", ToolSelect, canvas),
        KeyBinding::new("r", ToolRectangle, canvas),
        KeyBinding::new("e", ToolEllipse, canvas),
        KeyBinding::new("a", ToolArrow, canvas),
        KeyBinding::new("l", ToolLine, canvas),
        KeyBinding::new("t", ToolText, canvas),
        KeyBinding::new("n", ToolNumber, canvas),
        KeyBinding::new("s", ToolSpotlight, canvas),
        KeyBinding::new("c", ToolCrop, canvas),
    ]);
}
