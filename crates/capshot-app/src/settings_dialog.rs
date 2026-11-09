//! Settings dialog (winshot's settings modal): hotkeys, startup, quick save,
//! export. Edits a copy of the config and applies it on Save.

use crate::main_view::MainView;
use capshot_core::config::{Config, FilenamePattern, ImageFormat};
use capshot_platform::hotkeys;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Selectable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Hotkeys,
    Startup,
    QuickSave,
    Export,
    Cloud,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HotkeyField {
    Fullscreen,
    Region,
    Window,
}

pub struct SettingsDialog {
    main: WeakEntity<MainView>,
    draft: Config,
    tab: Tab,
    recording: Option<HotkeyField>,
    recorder_focus: FocusHandle,
    problems: Vec<String>,
    cloud: Option<Entity<crate::cloud_settings::CloudSettings>>,
}

impl SettingsDialog {
    pub fn new(main: WeakEntity<MainView>, config: Config, cx: &mut Context<Self>) -> Self {
        Self {
            main,
            draft: config,
            tab: Tab::Hotkeys,
            recording: None,
            recorder_focus: cx.focus_handle(),
            problems: Vec::new(),
            cloud: None,
        }
    }

    fn hotkey_mut(&mut self, f: HotkeyField) -> &mut String {
        match f {
            HotkeyField::Fullscreen => &mut self.draft.hotkeys.fullscreen,
            HotkeyField::Region => &mut self.draft.hotkeys.region,
            HotkeyField::Window => &mut self.draft.hotkeys.window,
        }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.main.update(cx, |m, cx| {
            m.settings_dialog = None;
            cx.notify();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Some(cloud) = self.cloud.clone() {
            if let Err(e) = cloud.update(cx, |c, cx| c.save(cx)) {
                self.problems = vec![e];
                cx.notify();
                return;
            }
            // The cloud view wrote its fields into the main config; keep them.
            if let Some(main) = self.main.upgrade() {
                self.draft.cloud = main.read(cx).config.cloud.clone();
            }
        }
        let draft = self.draft.clone();
        let Some(main) = self.main.upgrade() else { return };
        main.update(cx, |m, _| {
            m.config = draft;
            m.save_config();
        });
        self.problems = crate::shell::apply_system_settings(cx);
        if self.problems.is_empty() {
            self.close(cx);
        }
        cx.notify();
    }

    /// Record the next key press as a hotkey.
    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.recording else { return };
        let k = &ev.keystroke;
        if k.key == "escape" && !k.modifiers.modified() {
            self.recording = None;
            cx.notify();
            return;
        }
        let mut parts = Vec::new();
        if k.modifiers.control {
            parts.push("Ctrl".to_string());
        }
        if k.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if k.modifiers.shift {
            parts.push("Shift".to_string());
        }
        if k.modifiers.platform {
            parts.push("Win".to_string());
        }
        parts.push(k.key.clone());
        let raw = parts.join("+");
        if let Some(c) = hotkeys::parse(&raw) {
            *self.hotkey_mut(field) = hotkeys::format(&c);
            self.recording = None;
            self.problems.clear();
        } else if !matches!(k.key.as_str(), "control" | "alt" | "shift" | "platform") {
            self.problems = vec![format!("{raw} cannot be used as a hotkey")];
        }
        cx.notify();
    }

    fn hotkey_row(
        &self,
        label: &'static str,
        field: HotkeyField,
        value: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let recording = self.recording == Some(field);
        div().flex().items_center().justify_between().py_2().child(div().text_sm().child(label)).child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id(SharedString::from(format!("hk-{label}")))
                        .min_w(px(200.))
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(if recording { theme.primary } else { theme.border })
                        .bg(theme.input)
                        .text_sm()
                        .child(if recording {
                            "Press keys… (Esc to cancel)".to_string()
                        } else if value.is_empty() {
                            "Not set".into()
                        } else {
                            value.to_string()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.recording = Some(field);
                            window.focus(&this.recorder_focus, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(SharedString::from(format!("hk-clear-{label}")))
                        .label("Clear")
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.hotkey_mut(field).clear();
                            cx.notify();
                        })),
                ),
        )
    }

    fn switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        toggle: fn(&mut Config),
    ) -> impl IntoElement {
        div().py_2().child(Switch::new(id).label(label).checked(checked).small().on_click(cx.listener(
            move |this, _, _, cx| {
                toggle(&mut this.draft);
                cx.notify();
            },
        )))
    }

    fn body(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match self.tab {
            Tab::Hotkeys => div()
                .child(self.hotkey_row("Fullscreen", HotkeyField::Fullscreen, &self.draft.hotkeys.fullscreen, cx))
                .child(self.hotkey_row("Region", HotkeyField::Region, &self.draft.hotkeys.region, cx))
                .child(self.hotkey_row("Window", HotkeyField::Window, &self.draft.hotkeys.window, cx))
                .child(div().mt_2().text_xs().text_color(muted).child(
                    "Hotkeys work while capshot runs in the tray. On Wayland they need a desktop that supports the GlobalShortcuts portal.",
                ))
                .into_any_element(),
            Tab::Startup => div()
                .child(self.switch_row("launch", "Launch at login", self.draft.startup.launch_on_startup, cx, |c| {
                    c.startup.launch_on_startup = !c.startup.launch_on_startup
                }))
                .child(self.switch_row("min-tray", "Start in the tray", self.draft.startup.minimize_to_tray, cx, |c| {
                    c.startup.minimize_to_tray = !c.startup.minimize_to_tray
                }))
                .child(self.switch_row("close-tray", "Closing the window keeps capshot in the tray", self.draft.startup.close_to_tray, cx, |c| {
                    c.startup.close_to_tray = !c.startup.close_to_tray
                }))
                .child(self.switch_row("notify", "Notify when a screenshot is saved", self.draft.startup.show_notification, cx, |c| {
                    c.startup.show_notification = !c.startup.show_notification
                }))
                .child(self.switch_row("update-check", "Check for updates at startup", self.draft.update.check_on_startup, cx, |c| {
                    c.update.check_on_startup = !c.update.check_on_startup
                }))
                .into_any_element(),
            Tab::QuickSave => {
                let mut patterns = div().flex().gap_1();
                for (p, label, example) in [
                    (FilenamePattern::Timestamp, "Timestamp", "capshot_2026-01-15_14-30-45"),
                    (FilenamePattern::Date, "Date", "capshot_2026-01-15"),
                    (FilenamePattern::Increment, "Number", "capshot_001"),
                ] {
                    patterns = patterns.child(
                        Button::new(SharedString::from(format!("pat-{label}")))
                            .label(label)
                            .small()
                            .ghost()
                            .selected(self.draft.quick_save.pattern == p)
                            .tooltip(example)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.draft.quick_save.pattern = p;
                                cx.notify();
                            })),
                    );
                }
                div()
                    .child(div().text_sm().mb_1().child("Folder"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(div().flex_1().text_sm().truncate().child(self.draft.quick_save.folder.clone()))
                            .child(Button::new("choose-folder").label("Choose…").small().on_click(cx.listener(|_this, _, window, cx| {
                                let rx = cx.prompt_for_paths(PathPromptOptions {
                                    files: false,
                                    directories: true,
                                    multiple: false,
                                    prompt: Some("Choose quick save folder".into()),
                                });
                                cx.spawn_in(window, async move |this, cx| {
                                    if let Ok(Ok(Some(paths))) = rx.await {
                                        if let Some(p) = paths.into_iter().next() {
                                            let _ = this.update(cx, |this, cx| {
                                                this.draft.quick_save.folder = p.to_string_lossy().into_owned();
                                                cx.notify();
                                            });
                                        }
                                    }
                                })
                                .detach();
                            }))),
                    )
                    .child(div().text_sm().mt_3().mb_1().child("File names"))
                    .child(patterns)
                    .into_any_element()
            }
            Tab::About => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(format!("capshot {}", env!("CARGO_PKG_VERSION"))))
                .child(div().text_sm().child("Capture, annotate and beautify screenshots. Built with Rust and GPUI."))
                .child(div().text_xs().text_color(muted).child("BSD 3-Clause License."))
                .child(div().text_xs().text_color(muted).child(
                    "Derived from winshot by mrgoonie (BSD 3-Clause), github.com/mrgoonie/winshot. Inter font (SIL OFL 1.1). Lucide icons (ISC).",
                ))
                .into_any_element(),
            Tab::Cloud => match &self.cloud {
                Some(c) => c.clone().into_any_element(),
                None => div().into_any_element(),
            },
            Tab::Export => {
                let fmt = self.draft.export.default_format;
                let _ = window;
                div()
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .py_2()
                            .child(div().text_sm().mr_2().child("Default format"))
                            .child(Button::new("fmt-png").label("PNG").small().ghost().selected(fmt == ImageFormat::Png).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.draft.export.default_format = ImageFormat::Png;
                                    cx.notify();
                                },
                            )))
                            .child(Button::new("fmt-jpeg").label("JPEG").small().ghost().selected(fmt == ImageFormat::Jpeg).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.draft.export.default_format = ImageFormat::Jpeg;
                                    cx.notify();
                                },
                            ))),
                    )
                    .child(div().text_xs().text_color(muted).child(format!("JPEG quality: {} (change it in the export bar)", self.draft.export.jpeg_quality)))
                    .child(self.switch_row("incl-bg", "Include background in exports", self.draft.export.include_background, cx, |c| {
                        c.export.include_background = !c.export.include_background
                    }))
                    .child(self.switch_row("auto-copy", "Copy to clipboard after saving", self.draft.export.auto_copy_to_clipboard, cx, |c| {
                        c.export.auto_copy_to_clipboard = !c.export.auto_copy_to_clipboard
                    }))
                    .into_any_element()
            }
        }
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, danger) = (theme.popover, theme.border, theme.danger);
        let mut tabs = div().flex().gap_1().px_3().py_2().border_b_1().border_color(border);
        for (t, label) in [
            (Tab::Hotkeys, "Hotkeys"),
            (Tab::Startup, "Startup"),
            (Tab::QuickSave, "Quick Save"),
            (Tab::Export, "Export"),
            (Tab::Cloud, "Cloud"),
            (Tab::About, "About"),
        ] {
            tabs = tabs.child(Button::new(label).label(label).small().ghost().selected(self.tab == t).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.tab = t;
                    if t == Tab::Cloud && this.cloud.is_none() {
                        let (main, r2, drive) =
                            (this.main.clone(), this.draft.cloud.r2.clone(), this.draft.cloud.gdrive.clone());
                        this.cloud =
                            Some(cx.new(|cx| crate::cloud_settings::CloudSettings::new(main, &r2, &drive, window, cx)));
                    }
                    this.recording = None;
                    cx.notify();
                }),
            ));
        }
        let body = self.body(window, cx);
        let problems = self.problems.clone();
        div().absolute().inset_0().bg(hsla(0., 0., 0., 0.55)).flex().items_center().justify_center().occlude().child(
            div()
                .id("settings-dialog")
                .track_focus(&self.recorder_focus)
                .on_key_down(cx.listener(Self::on_key))
                .w(px(560.))
                .bg(bg)
                .rounded_lg()
                .border_1()
                .border_color(border)
                .flex()
                .flex_col()
                .child(div().px_3().py_2().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                .child(tabs)
                .child(div().px_4().py_3().min_h(px(240.)).child(body))
                .when(!problems.is_empty(), |d| {
                    d.child(
                        div()
                            .px_4()
                            .pb_2()
                            .text_xs()
                            .text_color(danger)
                            .children(problems.into_iter().map(|p| div().child(p))),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(border)
                        .child(
                            Button::new("settings-cancel")
                                .label("Cancel")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        )
                        .child(
                            Button::new("settings-save")
                                .label("Save")
                                .small()
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                        ),
                ),
        )
    }
}
