//! Global hotkeys. Strings use winshot's format ("Ctrl+Shift+PrintScreen"),
//! so hotkeys saved by winshot keep working.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    Fullscreen,
    Region,
    Window,
}

/// A parsed hotkey: modifiers plus one key, independent of any backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: Code,
}

const KEYS: &[(&str, Code)] = &[
    ("PRINTSCREEN", Code::PrintScreen),
    ("PRTSC", Code::PrintScreen),
    ("SNAPSHOT", Code::PrintScreen),
    ("F1", Code::F1),
    ("F2", Code::F2),
    ("F3", Code::F3),
    ("F4", Code::F4),
    ("F5", Code::F5),
    ("F6", Code::F6),
    ("F7", Code::F7),
    ("F8", Code::F8),
    ("F9", Code::F9),
    ("F10", Code::F10),
    ("F11", Code::F11),
    ("F12", Code::F12),
    ("A", Code::KeyA),
    ("B", Code::KeyB),
    ("C", Code::KeyC),
    ("D", Code::KeyD),
    ("E", Code::KeyE),
    ("F", Code::KeyF),
    ("G", Code::KeyG),
    ("H", Code::KeyH),
    ("I", Code::KeyI),
    ("J", Code::KeyJ),
    ("K", Code::KeyK),
    ("L", Code::KeyL),
    ("M", Code::KeyM),
    ("N", Code::KeyN),
    ("O", Code::KeyO),
    ("P", Code::KeyP),
    ("Q", Code::KeyQ),
    ("R", Code::KeyR),
    ("S", Code::KeyS),
    ("T", Code::KeyT),
    ("U", Code::KeyU),
    ("V", Code::KeyV),
    ("W", Code::KeyW),
    ("X", Code::KeyX),
    ("Y", Code::KeyY),
    ("Z", Code::KeyZ),
    ("0", Code::Digit0),
    ("1", Code::Digit1),
    ("2", Code::Digit2),
    ("3", Code::Digit3),
    ("4", Code::Digit4),
    ("5", Code::Digit5),
    ("6", Code::Digit6),
    ("7", Code::Digit7),
    ("8", Code::Digit8),
    ("9", Code::Digit9),
    ("SPACE", Code::Space),
    ("ENTER", Code::Enter),
    ("TAB", Code::Tab),
    ("ESCAPE", Code::Escape),
    ("ESC", Code::Escape),
    ("BACKSPACE", Code::Backspace),
    ("DELETE", Code::Delete),
    ("INSERT", Code::Insert),
    ("HOME", Code::Home),
    ("END", Code::End),
    ("PAGEUP", Code::PageUp),
    ("PAGEDOWN", Code::PageDown),
    ("UP", Code::ArrowUp),
    ("DOWN", Code::ArrowDown),
    ("LEFT", Code::ArrowLeft),
    ("RIGHT", Code::ArrowRight),
];

/// Display names used when formatting (the first alias of each key).
fn key_name(code: Code) -> Option<&'static str> {
    let name = KEYS.iter().find(|(_, c)| *c == code)?.0;
    Some(match name {
        "PRINTSCREEN" => "PrintScreen",
        "SPACE" => "Space",
        "ENTER" => "Enter",
        "TAB" => "Tab",
        "ESCAPE" => "Escape",
        "BACKSPACE" => "Backspace",
        "DELETE" => "Delete",
        "INSERT" => "Insert",
        "HOME" => "Home",
        "END" => "End",
        "PAGEUP" => "PageUp",
        "PAGEDOWN" => "PageDown",
        "UP" => "Up",
        "DOWN" => "Down",
        "LEFT" => "Left",
        "RIGHT" => "Right",
        other => other,
    })
}

/// Parse "Ctrl+Shift+PrintScreen" (case and spaces ignored). Exactly one
/// non-modifier key is required, as in winshot.
pub fn parse(s: &str) -> Option<Combo> {
    let norm = s.replace(' ', "").to_ascii_uppercase();
    if norm.is_empty() {
        return None;
    }
    let mut c = Combo { ctrl: false, alt: false, shift: false, meta: false, key: Code::PrintScreen };
    let mut key = None;
    for part in norm.split('+') {
        match part {
            "CTRL" | "CONTROL" => c.ctrl = true,
            "ALT" => c.alt = true,
            "SHIFT" => c.shift = true,
            "WIN" | "WINDOWS" | "META" | "SUPER" | "CMD" => c.meta = true,
            k => key = Some(KEYS.iter().find(|(n, _)| *n == k)?.1),
        }
    }
    c.key = key?;
    Some(c)
}

/// Format back to winshot's string form ("Ctrl+Alt+Shift+Win+Key").
pub fn format(c: &Combo) -> String {
    let mut parts = Vec::new();
    if c.ctrl {
        parts.push("Ctrl");
    }
    if c.alt {
        parts.push("Alt");
    }
    if c.shift {
        parts.push("Shift");
    }
    if c.meta {
        parts.push(if cfg!(target_os = "macos") { "Cmd" } else { "Win" });
    }
    if let Some(k) = key_name(c.key) {
        parts.push(k);
    }
    parts.join("+")
}

fn to_hotkey(c: &Combo) -> HotKey {
    let mut m = Modifiers::empty();
    m.set(Modifiers::CONTROL, c.ctrl);
    m.set(Modifiers::ALT, c.alt);
    m.set(Modifiers::SHIFT, c.shift);
    m.set(Modifiers::META, c.meta);
    HotKey::new(if m.is_empty() { None } else { Some(m) }, c.key)
}

/// Owns the OS registrations. Create it on the UI thread after the app's event
/// loop has started (Windows delivers hotkeys through that thread's messages).
pub struct HotkeyService {
    manager: GlobalHotKeyManager,
    registered: HashMap<HotkeyAction, HotKey>,
    by_id: std::sync::Arc<std::sync::Mutex<HashMap<u32, HotkeyAction>>>,
}

impl HotkeyService {
    pub fn new(tx: Sender<HotkeyAction>) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let by_id: std::sync::Arc<std::sync::Mutex<HashMap<u32, HotkeyAction>>> = Default::default();
        let ids = by_id.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |ev: GlobalHotKeyEvent| {
            if ev.state == HotKeyState::Pressed {
                if let Some(action) = ids.lock().ok().and_then(|m| m.get(&ev.id).copied()) {
                    let _ = tx.send(action);
                }
            }
        }));
        Ok(Self { manager, registered: HashMap::new(), by_id })
    }

    fn unregister(&mut self, action: HotkeyAction) {
        if let Some(old) = self.registered.remove(&action) {
            let _ = self.manager.unregister(old);
            if let Ok(mut m) = self.by_id.lock() {
                m.remove(&old.id());
            }
        }
    }

    fn register(&mut self, action: HotkeyAction, hk: HotKey) -> Result<(), String> {
        self.manager.register(hk).map_err(|e| e.to_string())?;
        if let Ok(mut m) = self.by_id.lock() {
            m.insert(hk.id(), action);
        }
        self.registered.insert(action, hk);
        Ok(())
    }

    /// Replace all bindings at once (so swapping two hotkeys works). Invalid
    /// strings are rejected before anything changes; when the OS refuses a new
    /// binding, that action's previous binding is restored. Returns one message
    /// per problem.
    pub fn set_all(&mut self, bindings: &[(HotkeyAction, &str)]) -> Vec<String> {
        let mut problems = Vec::new();
        let mut wanted = Vec::new();
        for (action, combo) in bindings {
            if combo.trim().is_empty() {
                wanted.push((*action, None));
                continue;
            }
            match parse(combo) {
                Some(c) => wanted.push((*action, Some((to_hotkey(&c), combo.to_string())))),
                None => problems.push(format!("\"{combo}\" is not a valid hotkey")),
            }
        }
        if !problems.is_empty() {
            return problems;
        }
        let previous: Vec<(HotkeyAction, HotKey)> = self.registered.iter().map(|(a, h)| (*a, *h)).collect();
        for (action, _) in &wanted {
            self.unregister(*action);
        }
        for (action, hk) in wanted {
            let Some((hk, combo)) = hk else { continue };
            if let Err(e) = self.register(action, hk) {
                problems.push(format!("{combo}: {e} (another app may be using it)"));
                if let Some((_, old)) = previous.iter().find(|(a, _)| *a == action) {
                    let _ = self.register(action, *old);
                }
            }
        }
        problems
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        for hk in self.registered.values() {
            let _ = self.manager.unregister(*hk);
        }
        GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_winshot_defaults() {
        let c = parse("Ctrl+Shift+PrintScreen").unwrap();
        assert!(c.ctrl && c.shift && !c.alt && !c.meta);
        assert_eq!(c.key, Code::PrintScreen);
        assert_eq!(parse("PrintScreen").unwrap().key, Code::PrintScreen);
        assert_eq!(parse("ctrl + prtsc").unwrap().key, Code::PrintScreen);
    }

    #[test]
    fn rejects_bad_strings() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("Ctrl+Shift"), None);
        assert_eq!(parse("Ctrl+Banana"), None);
    }

    #[test]
    fn format_round_trips() {
        for s in ["Ctrl+PrintScreen", "Ctrl+Alt+Shift+F12", "Alt+A", "Shift+PageDown", "Ctrl+1"] {
            assert_eq!(format(&parse(s).unwrap()), s);
        }
        assert_eq!(format(&parse("control+esc").unwrap()), "Ctrl+Escape");
    }
}
