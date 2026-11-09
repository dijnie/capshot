//! System tray icon and menu. Windows and macOS use `tray-icon`; Linux uses
//! `ksni` (StatusNotifierItem over D-Bus), which needs no GTK main loop.

use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Fullscreen,
    Region,
    Window,
    Library,
    Settings,
    Quit,
}

const ITEMS: [(&str, &str, TrayCommand); 6] = [
    ("fullscreen", "Capture Fullscreen", TrayCommand::Fullscreen),
    ("region", "Capture Region", TrayCommand::Region),
    ("window", "Capture Window", TrayCommand::Window),
    ("library", "Library", TrayCommand::Library),
    ("settings", "Settings", TrayCommand::Settings),
    ("show", "Show capshot", TrayCommand::Show),
];

/// Keeps the tray alive; dropping it removes the icon.
pub struct Tray {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    _icon: tray_icon::TrayIcon,
    #[cfg(target_os = "linux")]
    _handle: ksni::blocking::Handle<LinuxTray>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn spawn(tx: Sender<TrayCommand>) -> Result<Tray, String> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let menu = Menu::new();
    for (i, (id, label, _)) in ITEMS.iter().enumerate() {
        if i == 3 || i == 5 {
            menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
        }
        menu.append(&MenuItem::with_id(*id, *label, true, None)).map_err(|e| e.to_string())?;
    }
    menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
    menu.append(&MenuItem::with_id("quit", "Quit", true, None)).map_err(|e| e.to_string())?;

    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        let id = ev.id.0.as_str();
        let cmd = if id == "quit" {
            Some(TrayCommand::Quit)
        } else {
            ITEMS.iter().find(|(i, _, _)| *i == id).map(|(_, _, c)| *c)
        };
        if let Some(cmd) = cmd {
            let _ = menu_tx.send(cmd);
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
            let _ = tx.send(TrayCommand::Show);
        }
    }));

    let icon = tray_icon::Icon::from_rgba(crate::icon::rgba(32), 32, 32).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("capshot")
        .with_icon(icon)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Tray { _icon: tray })
}

#[cfg(target_os = "linux")]
pub struct LinuxTray {
    tx: Sender<TrayCommand>,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "capshot".into()
    }

    fn title(&self) -> String {
        "capshot".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // StatusNotifierItem wants ARGB32 in network byte order.
        let mut data = crate::icon::rgba(32);
        for px in data.chunks_exact_mut(4) {
            px.rotate_right(1);
        }
        vec![ksni::Icon { width: 32, height: 32, data }]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let mut items: Vec<ksni::MenuItem<Self>> = ITEMS
            .iter()
            .map(|(_, label, cmd)| {
                let cmd = *cmd;
                StandardItem {
                    label: (*label).into(),
                    activate: Box::new(move |t: &mut Self| {
                        let _ = t.tx.send(cmd);
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        items.push(ksni::MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

#[cfg(target_os = "linux")]
pub fn spawn(tx: Sender<TrayCommand>) -> Result<Tray, String> {
    use ksni::blocking::TrayMethods;
    let handle = LinuxTray { tx }.spawn().map_err(|e| e.to_string())?;
    Ok(Tray { _handle: handle })
}
