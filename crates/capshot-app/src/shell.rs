//! App shell: main window lifecycle, tray, global hotkeys, single-instance
//! messages and autostart.

use crate::cli::{self, CaptureMode};
use crate::main_view::MainView;
use capshot_platform::hotkeys::{HotkeyAction, HotkeyService};
use capshot_platform::tray::{self, Tray, TrayCommand};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::*;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub struct Shell {
    pub view: Entity<MainView>,
    window: Option<AnyWindowHandle>,
    hotkeys: Option<HotkeyService>,
    _tray: Option<Tray>,
}

impl Global for Shell {}

/// How often OS events (tray, hotkeys, second launches) are drained.
const POLL: Duration = Duration::from_millis(80);

pub fn start(cx: &mut App, view: Entity<MainView>, show_window: bool, instance_rx: Receiver<Vec<String>>) {
    Theme::change(ThemeMode::Dark, None, cx);
    let (tray_tx, tray_rx) = channel();
    let tray = match tray::spawn(tray_tx) {
        Ok(t) => Some(t),
        Err(e) => {
            log::warn!("tray unavailable: {e}");
            None
        }
    };
    let (hk_tx, hk_rx) = channel();
    let hotkeys = match HotkeyService::new(hk_tx) {
        Ok(h) => Some(h),
        Err(e) => {
            log::warn!("global hotkeys unavailable: {e}");
            None
        }
    };
    cx.set_global(Shell { view: view.clone(), window: None, hotkeys, _tray: tray });
    apply_system_settings(cx);
    view.update(cx, |v, _| v.refresh_cloud_state());
    if show_window {
        show_main_window(cx);
    }
    if show_window && view.read(cx).config.update.check_on_startup {
        with_view(cx, |_, window, cx| crate::update_dialog::check_on_startup(window, cx));
    }

    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(POLL).await;
        let mut commands = Vec::new();
        commands.extend(tray_rx.try_iter().map(Command::Tray));
        commands.extend(hk_rx.try_iter().map(Command::Hotkey));
        commands.extend(instance_rx.try_iter().map(Command::Instance));
        if commands.is_empty() {
            continue;
        }
        cx.update(|cx| commands.into_iter().for_each(|c| dispatch(c, cx)));
    })
    .detach();
}

enum Command {
    Tray(TrayCommand),
    Hotkey(HotkeyAction),
    Instance(Vec<String>),
}

fn dispatch(cmd: Command, cx: &mut App) {
    match cmd {
        Command::Tray(TrayCommand::Quit) => cx.quit(),
        Command::Tray(TrayCommand::Show) => {
            show_main_window(cx);
        }
        Command::Tray(TrayCommand::Fullscreen) | Command::Hotkey(HotkeyAction::Fullscreen) => {
            capture(CaptureMode::Fullscreen, cx)
        }
        Command::Tray(TrayCommand::Region) | Command::Hotkey(HotkeyAction::Region) => capture(CaptureMode::Region, cx),
        Command::Tray(TrayCommand::Window) | Command::Hotkey(HotkeyAction::Window) => capture(CaptureMode::Window, cx),
        Command::Tray(TrayCommand::Settings) => with_view(cx, |view, window, cx| view.open_settings(window, cx)),
        Command::Tray(TrayCommand::Library) => with_view(cx, |view, window, cx| view.open_library(window, cx)),
        Command::Instance(args) => {
            let parsed = cli::parse(args.into_iter().map(Into::into));
            match (parsed.capture, parsed.open) {
                (Some(mode), _) => capture(mode, cx),
                (None, Some(path)) => {
                    with_view(cx, move |view, window, cx| crate::capture_flow::open_path(view, path, window, cx))
                }
                (None, None) => {
                    show_main_window(cx);
                }
            }
        }
    }
}

/// What to grab.
#[derive(Clone, Copy)]
pub enum CaptureJob {
    Fullscreen,
    Region,
    Window(u32),
}

/// Time for the editor window to get out of the way before capturing.
const HIDE_DELAY: Duration = Duration::from_millis(400);

/// Start a capture from a hotkey, the tray, the command line or a button.
/// Only the window picker needs the editor window.
pub fn capture(mode: CaptureMode, cx: &mut App) {
    match mode {
        CaptureMode::Fullscreen => start_capture(CaptureJob::Fullscreen, cx),
        CaptureMode::Region => start_capture(CaptureJob::Region, cx),
        CaptureMode::Window => with_view(cx, crate::capture_flow::open_window_picker),
    }
}

/// Capture without bringing up the editor first. If the editor window is open
/// it is minimised out of the shot; if capshot sits in the tray nothing is
/// shown until there is a result. Must not be called from inside the editor
/// window's own event handlers (use `cx.defer`).
pub fn start_capture(job: CaptureJob, cx: &mut App) {
    let existing = cx.global::<Shell>().window;
    let was_open = existing.is_some_and(|h| h.update(cx, |_, window, _| window.minimize_window()).is_ok());
    cx.spawn(async move |cx| {
        use capshot_platform::capture as cap;
        if was_open {
            cx.background_executor().timer(HIDE_DELAY).await;
        }
        match job {
            CaptureJob::Region => {
                let shots = cx.background_spawn(async { cap::capture_all_displays() }).await;
                cx.update(|cx| match shots {
                    Ok(shots) => crate::overlay::open(shots, was_open, cx),
                    Err(e) => report(format!("Region capture failed: {e}"), cx),
                });
            }
            CaptureJob::Fullscreen | CaptureJob::Window(_) => {
                let label = if matches!(job, CaptureJob::Fullscreen) { "Fullscreen" } else { "Window" };
                let result = cx
                    .background_spawn(async move {
                        match job {
                            CaptureJob::Window(id) => cap::capture_window(id),
                            _ => cap::capture_virtual_screen(),
                        }
                    })
                    .await;
                cx.update(|cx| match result {
                    Ok(img) => deliver(img, label, cx),
                    Err(e) => report(format!("{label} capture failed: {e}"), cx),
                });
            }
        }
    })
    .detach();
}

/// Open the editor with a captured image.
pub fn deliver(img: image::RgbaImage, label: &'static str, cx: &mut App) {
    with_view(cx, move |view, window, cx| view.load_image(img, label, window, cx));
}

/// Show the editor with a status message.
pub fn report(msg: String, cx: &mut App) {
    with_view(cx, move |view, _, cx| view.set_status(msg, cx));
}

/// Show the main window, then run `f` on the view inside it.
pub fn with_view(cx: &mut App, f: impl FnOnce(&mut MainView, &mut Window, &mut Context<MainView>) + 'static) {
    let handle = show_main_window(cx);
    let view = cx.global::<Shell>().view.clone();
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, cx| view.update(cx, |v, cx| f(v, window, cx)));
    }
}

/// Bring up the main window, reopening it (with the same editor state) if it
/// was closed to the tray.
pub fn show_main_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let (existing, view) = {
        let shell = cx.global::<Shell>();
        (shell.window, shell.view.clone())
    };
    if let Some(h) = existing {
        if h.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return Some(h);
        }
    }
    let cfg = view.read(cx).config.window.clone();
    let size = size(px(cfg.width.max(800) as f32), px(cfg.height.max(560) as f32));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
        titlebar: Some(TitlebarOptions { title: Some("capshot".into()), ..Default::default() }),
        window_min_size: Some(gpui_kit::size(px(800.), px(560.))),
        app_id: Some("capshot".into()),
        ..Default::default()
    };
    let v = view.clone();
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        v.update(cx, |view, _| view.window_reopened());
        let close_view = v.clone();
        window.on_window_should_close(cx, move |window, cx| {
            let (to_tray, size) = close_view.update(cx, |view, _| {
                let b = window.bounds().size;
                view.config.window.width = f32::from(b.width).round() as u32;
                view.config.window.height = f32::from(b.height).round() as u32;
                view.save_config();
                (view.config.startup.close_to_tray, b)
            });
            let _ = size;
            if to_tray && cx.has_global::<Shell>() {
                cx.global_mut::<Shell>().window = None;
                true
            } else {
                cx.quit();
                true
            }
        });
        v.clone()
    });
    match opened {
        Ok((handle, _)) => {
            cx.global_mut::<Shell>().window = Some(handle);
            cx.activate(true);
            Some(handle)
        }
        Err(e) => {
            log::error!("could not open the main window: {e}");
            None
        }
    }
}

/// Apply hotkeys and autostart from the current config. Returns one message
/// per problem (for the settings dialog and status bar).
pub fn apply_system_settings(cx: &mut App) -> Vec<String> {
    let view = cx.global::<Shell>().view.clone();
    let config = view.read(cx).config.clone();
    let mut problems = Vec::new();
    if let Some(hk) = cx.global_mut::<Shell>().hotkeys.as_mut() {
        problems.extend(hk.set_all(&[
            (HotkeyAction::Fullscreen, config.hotkeys.fullscreen.as_str()),
            (HotkeyAction::Region, config.hotkeys.region.as_str()),
            (HotkeyAction::Window, config.hotkeys.window.as_str()),
        ]));
    } else {
        problems.push("Global hotkeys are not available on this desktop".into());
    }
    if let Err(e) = capshot_platform::autostart::set_enabled(config.startup.launch_on_startup) {
        problems.push(format!("Launch at login: {e}"));
    }
    if !problems.is_empty() {
        let msg = problems.join("; ");
        view.update(cx, |v, cx| v.set_status(msg, cx));
    }
    problems
}
