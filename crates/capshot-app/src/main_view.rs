//! The main window: capture toolbar, annotation toolbar, canvas, settings
//! panel and export bar.

use crate::actions::*;
use crate::editor::{Drag, Editor, ViewTransform, GRIP_SIZE, ZOOM_STEP};
use crate::preview::{render_preview, Preview};
use crate::{capture_flow, settings_panel};
use capshot_core::annotation::{AnnotationKind, FontStyle};
use capshot_core::color::Rgba;
use capshot_core::config::{Config, ImageFormat};
use capshot_core::crop::{CropAspect, Handle};
use capshot_core::edit::{self, Tool};
use capshot_core::geometry::{Point, Rect};
use capshot_core::presets::{ANNOTATION_COLORS, CORNER_RADII, FONT_SIZES, STROKE_WIDTHS};
use capshot_core::render::{self, RenderOptions};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme, Disableable, Selectable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub struct MainView {
    pub config: Config,
    pub config_path: PathBuf,
    pub editor: Option<Editor>,
    preview: Option<Preview>,
    canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub status: SharedString,
    focus: FocusHandle,
    canvas_focus: FocusHandle,
    text_edit: Option<(u64, Entity<TextareaState>)>,
    pub sliders: settings_panel::Sliders,
    jpeg_quality: Entity<SliderState>,
    pub window_picker: Option<Entity<crate::window_picker::WindowPicker>>,
    pub settings_dialog: Option<Entity<crate::settings_dialog::SettingsDialog>>,
    pub library: Option<Entity<crate::library_window::LibraryView>>,
    pub update_dialog: Option<Entity<crate::update_dialog::UpdateDialog>>,
    /// Whether R2 / Google Drive are set up (cached; keychain reads are slow).
    cloud_r2: bool,
    cloud_drive: bool,
    uploading: bool,
    _subscriptions: Vec<Subscription>,
}

pub fn to_hsla(c: Rgba) -> Hsla {
    gpui_kit::Rgba { r: c.r as f32 / 255.0, g: c.g as f32 / 255.0, b: c.b as f32 / 255.0, a: c.a as f32 / 255.0 }.into()
}

fn px_f(p: Pixels) -> f32 {
    f32::from(p)
}

fn to_rect(b: Bounds<Pixels>) -> Rect {
    Rect::new(px_f(b.origin.x), px_f(b.origin.y), px_f(b.size.width), px_f(b.size.height))
}

fn to_point(p: gpui_kit::Point<Pixels>) -> Point {
    Point::new(px_f(p.x), px_f(p.y))
}

fn screen_bounds(r: Rect) -> Bounds<Pixels> {
    Bounds::new(point(px(r.x), px(r.y)), size(px(r.width), px(r.height)))
}

/// Things drawn over the preview: selection, grips and the crop frame.
#[derive(Default, Clone)]
struct Chrome {
    image: Option<(Arc<RenderImage>, Rect)>,
    canvas: Rect,
    selection: Option<Rect>,
    grips: Vec<(Point, bool)>,
    crop: Option<(Rect, Rect)>,
}

impl MainView {
    pub fn new(config: Config, config_path: PathBuf, cx: &mut Context<Self>) -> Self {
        let sliders = settings_panel::Sliders::new(&config.editor, cx);
        let jpeg_quality = cx
            .new(|_| SliderState::new().min(1.0).max(100.0).step(1.0).default_value(config.export.jpeg_quality as f32));
        let mut subs = sliders.subscribe(cx);
        subs.push(cx.subscribe(&jpeg_quality, |this, _, ev: &SliderEvent, cx| {
            let v = match ev {
                SliderEvent::Change(v) | SliderEvent::Release(v) => v.end(),
            };
            this.config.export.jpeg_quality = v.round() as u8;
            if matches!(ev, SliderEvent::Release(_)) {
                this.save_config();
            }
            cx.notify();
        }));
        Self {
            config,
            config_path,
            editor: None,
            preview: None,
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            status: "Capture a screenshot to start editing".into(),
            focus: cx.focus_handle(),
            canvas_focus: cx.focus_handle(),
            text_edit: None,
            sliders,
            jpeg_quality,
            window_picker: None,
            settings_dialog: None,
            library: None,
            update_dialog: None,
            cloud_r2: false,
            cloud_drive: false,
            uploading: false,
            _subscriptions: subs,
        }
    }

    /// The window was closed to the tray and opened again: its GPU images are gone.
    pub fn window_reopened(&mut self) {
        self.preview = None;
        self.window_picker = None;
        self.library = None;
    }

    pub fn refresh_cloud_state(&mut self) {
        use capshot_cloud::credentials::Keychain;
        self.cloud_r2 = capshot_cloud::r2::R2::from_store(&self.config.cloud.r2, &Keychain).is_ok();
        self.cloud_drive = capshot_cloud::gdrive::GDrive::new(&Keychain, "").is_connected();
    }

    fn upload(&mut self, to_drive: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(img) = self.export_image() else { return };
        let fmt = self.config.export.default_format;
        let quality = self.config.export.jpeg_quality;
        let name = capshot_core::export::upload_name(fmt, chrono::Local::now());
        let r2 = self.config.cloud.r2.clone();
        let folder = self.config.cloud.gdrive.folder_id.clone();
        let service = if to_drive { "Google Drive" } else { "R2" };
        self.uploading = true;
        self.set_status(format!("Uploading to {service}…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    use capshot_cloud::credentials::Keychain;
                    let bytes = capshot_core::export::encode(&img, fmt, quality).map_err(|e| e.to_string())?;
                    if to_drive {
                        capshot_cloud::gdrive::GDrive::new(&Keychain, &folder).upload(&bytes, &name)
                    } else {
                        capshot_cloud::r2::R2::from_store(&r2, &Keychain).and_then(|r| r.upload(&bytes, &name))
                    }
                    .map_err(|e| e.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.uploading = false;
                match result {
                    Ok(url) => {
                        let copied = capshot_platform::clipboard::write_text(&url).is_ok();
                        this.set_status(
                            format!("Uploaded to {service}: {url}{}", if copied { " (link copied)" } else { "" }),
                            cx,
                        );
                    }
                    Err(e) => this.set_status(format!("{service} upload failed: {e}"), cx),
                }
            });
        })
        .detach();
    }

    pub fn open_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let main = cx.entity().downgrade();
        let config = self.config.clone();
        self.settings_dialog = Some(cx.new(|cx| crate::settings_dialog::SettingsDialog::new(main, config, cx)));
        cx.notify();
    }

    pub fn open_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::library_window::open(self, window, cx);
    }

    pub fn save_config(&mut self) {
        if let Some(e) = &self.editor {
            let mut s = e.doc.settings.clone();
            s.crop_aspect = e.crop_aspect;
            self.config.editor = s;
        }
        if let Err(err) = self.config.save(&self.config_path) {
            self.status = format!("Could not save settings: {err}").into();
        }
    }

    pub fn set_status(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = msg.into();
        cx.notify();
    }

    /// Start editing a new capture.
    pub fn load_image(&mut self, image: image::RgbaImage, source: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = image.dimensions();
        let mut settings = self.config.editor.clone();
        // Keep a custom background when auto background is off; follow the capture otherwise.
        if !settings.auto_background {
            settings.inset_background_color = settings.inset_background_color.clone();
        }
        let editor = Editor::new(image, settings);
        self.text_edit = None;
        self.editor = Some(editor);
        if let Some(e) = &self.editor {
            self.sliders.sync(&e.doc.settings, window, cx);
        }
        self.drop_preview(window);
        self.status = format!("{source}: {w} × {h}").into();
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    fn drop_preview(&mut self, window: &mut Window) {
        if let Some(p) = self.preview.take() {
            let _ = window.drop_image(p.image);
        }
    }

    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.editor.as_mut()
    }

    fn view_transform(&self) -> Option<ViewTransform> {
        let e = self.editor.as_ref()?;
        let layout = render::layout_for(&e.doc, &RenderOptions::default());
        Some(ViewTransform::fit(
            to_rect(self.canvas_bounds.get()),
            (layout.canvas_width, layout.canvas_height),
            e.zoom,
            e.pan,
        ))
    }

    /// Screen point → original-image pixels.
    fn to_original(&self, p: Point) -> Option<Point> {
        let t = self.view_transform()?;
        Some(self.editor.as_ref()?.doc.canvas_to_original(t.to_canvas(p)))
    }

    fn tolerance(&self) -> f32 {
        let (Some(t), Some(e)) = (self.view_transform(), self.editor.as_ref()) else { return 4.0 };
        let layout = render::layout_for(&e.doc, &RenderOptions::default());
        (GRIP_SIZE / 2.0 + 2.0) / (t.scale * layout.image_scale).max(1e-3)
    }

    fn ensure_preview(&mut self, window: &mut Window) {
        let Some(t) = self.view_transform() else { return };
        let e = self.editor.as_ref().unwrap();
        let want = t.scale * window.scale_factor();
        let fresh =
            self.preview.as_ref().is_some_and(|p| p.revision == e.revision && (p.scale - want).abs() / want < 0.02);
        if fresh {
            return;
        }
        let (image, scale) = render_preview(&e.doc, want);
        let revision = e.revision;
        self.drop_preview(window);
        self.preview = Some(Preview { image, revision, scale });
    }

    fn chrome(&self) -> Chrome {
        let mut c = Chrome::default();
        let (Some(t), Some(e)) = (self.view_transform(), self.editor.as_ref()) else { return c };
        let layout = render::layout_for(&e.doc, &RenderOptions::default());
        let canvas = Rect::new(
            t.origin.x,
            t.origin.y,
            layout.canvas_width as f32 * t.scale,
            layout.canvas_height as f32 * t.scale,
        );
        c.canvas = canvas;
        if let Some(p) = &self.preview {
            c.image = Some((p.image.clone(), canvas));
        }
        let to_screen = |p: Point| t.to_screen(e.doc.original_to_canvas(p));
        let rect_to_screen = |r: Rect| {
            let a = to_screen(Point::new(r.x, r.y));
            let b = to_screen(Point::new(r.right(), r.bottom()));
            Rect::from_points(a, b)
        };
        if let Some(a) = e.selected_annotation() {
            if self.text_edit.as_ref().is_none_or(|(id, _)| *id != a.id) {
                c.selection = Some(rect_to_screen(a.bounds()).inflate(4.0));
                c.grips =
                    edit::grips(a).into_iter().map(|(g, p)| (to_screen(p), matches!(g, edit::Grip::Box(_)))).collect();
            }
        }
        if let Some(session) = &e.crop {
            let image_rect = rect_to_screen(e.original_bounds());
            let frame = session.rect.map(rect_to_screen).unwrap_or(image_rect);
            c.crop = Some((image_rect, frame));
            if session.rect.is_some() {
                c.grips = Handle::ALL.iter().map(|h| (h.position(frame), true)).collect();
            } else {
                c.grips.clear();
            }
        }
        c
    }

    // ---- pointer handlers ----------------------------------------------------

    fn on_canvas_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.canvas_focus, cx);
        self.commit_text_edit(window, cx);
        let screen = to_point(ev.position);
        let tol = self.tolerance();
        let Some(p) = self.to_original(screen) else { return };
        if std::env::var_os("CAPSHOT_TRACE").is_some() {
            eprintln!("down screen {screen:?} orig {p:?} button {:?}", ev.button);
        }
        let Some(e) = self.editor.as_mut() else { return };
        if ev.button == MouseButton::Middle || (ev.button == MouseButton::Left && ev.modifiers.alt) {
            e.drag = Some(Drag::Pan { last: screen });
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let out = if ev.click_count >= 2 && e.tool == Tool::Select {
            e.double_click(p, tol)
        } else {
            e.pointer_down(p, ev.modifiers.shift, tol)
        };
        if let Some(id) = out.edit_text {
            self.start_text_edit(id, window, cx);
        }
        cx.notify();
    }

    fn on_canvas_move(&mut self, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let screen = to_point(ev.position);
        if std::env::var_os("CAPSHOT_TRACE").is_some() && ev.pressed_button.is_some() {
            eprintln!("move {screen:?} {:?}", ev.pressed_button);
        }
        let p = self.to_original(screen);
        let Some(e) = self.editor.as_mut() else { return };
        match e.drag {
            Some(Drag::Pan { last }) => {
                e.pan = Point::new(e.pan.x + screen.x - last.x, e.pan.y + screen.y - last.y);
                e.drag = Some(Drag::Pan { last: screen });
                e.touch();
                cx.notify();
            }
            Some(_) => {
                if let Some(p) = p {
                    e.pointer_move(p, ev.modifiers.shift);
                    cx.notify();
                }
            }
            None => {}
        }
    }

    fn on_canvas_up(&mut self, _ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var_os("CAPSHOT_TRACE").is_some() {
            eprintln!("up");
        }
        if let Some(e) = self.editor.as_mut() {
            if matches!(e.drag, Some(Drag::Pan { .. })) {
                e.drag = None;
            } else {
                e.pointer_up();
            }
            cx.notify();
        }
    }

    fn on_scroll(&mut self, ev: &ScrollEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.editor.as_mut() else { return };
        let dy = match ev.delta {
            ScrollDelta::Pixels(p) => px_f(p.y) / 40.0,
            ScrollDelta::Lines(l) => l.y,
        };
        if ev.modifiers.control || ev.modifiers.platform {
            e.zoom_by(if dy > 0.0 { ZOOM_STEP } else { -ZOOM_STEP });
        } else {
            let (dx, dy) = match ev.delta {
                ScrollDelta::Pixels(p) => (px_f(p.x), px_f(p.y)),
                ScrollDelta::Lines(l) => (l.x * 40.0, l.y * 40.0),
            };
            e.pan = Point::new(e.pan.x + dx, e.pan.y + dy);
            e.touch();
        }
        cx.notify();
    }

    // ---- text editing --------------------------------------------------------

    fn start_text_edit(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.as_ref().and_then(|e| e.text_of(id)).unwrap_or_default();
        let state = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 12).default_value(text));
        let sub = cx.subscribe_in(&state, window, move |this, state, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => {
                let value = state.read(cx).value();
                if let Some(e) = this.editor.as_mut() {
                    e.set_text_live(id, &value);
                }
                cx.notify();
            }
            InputEvent::Blur => this.commit_text_edit(window, cx),
            _ => {}
        });
        self._subscriptions.push(sub);
        state.update(cx, |s, cx| s.focus(window, cx));
        self.text_edit = Some((id, state));
        cx.notify();
    }

    fn commit_text_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, _)) = self.text_edit.take() else { return };
        if let Some(e) = self.editor.as_mut() {
            e.finish_text(id);
        }
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    // ---- export --------------------------------------------------------------

    fn export_image(&self) -> Option<image::RgbaImage> {
        let e = self.editor.as_ref()?;
        Some(render::render_image(&e.export_document(), self.config.export.include_background))
    }

    fn copy_image(&mut self, cx: &mut Context<Self>) {
        let Some(img) = self.export_image() else { return };
        let msg = match capshot_platform::clipboard::write_image(&img) {
            Ok(()) => "Copied to clipboard".to_string(),
            Err(e) => format!("Copy failed: {e}"),
        };
        self.set_status(msg, cx);
    }

    fn after_save(&mut self, path: &std::path::Path, img: &image::RgbaImage, cx: &mut Context<Self>) {
        let mut msg = format!("Saved {}", path.display());
        if self.config.export.auto_copy_to_clipboard && capshot_platform::clipboard::write_image(img).is_ok() {
            msg.push_str(" (copied)");
        }
        if self.config.startup.show_notification {
            capshot_platform::notify::show("Screenshot saved", &path.display().to_string());
        }
        self.set_status(msg, cx);
    }

    fn quick_save(&mut self, cx: &mut Context<Self>) {
        let Some(img) = self.export_image() else { return };
        let fmt = self.config.export.default_format;
        let dir = PathBuf::from(&self.config.quick_save.folder);
        let path =
            capshot_core::export::quick_save_path(&dir, self.config.quick_save.pattern, fmt, chrono::Local::now());
        match capshot_core::export::save(&img, &path, fmt, self.config.export.jpeg_quality) {
            Ok(()) => self.after_save(&path, &img, cx),
            Err(e) => self.set_status(format!("Save failed: {e}"), cx),
        }
    }

    fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_none() {
            return;
        }
        let fmt = self.config.export.default_format;
        let dir = PathBuf::from(&self.config.quick_save.folder);
        let _ = std::fs::create_dir_all(&dir);
        let name =
            capshot_core::export::quick_save_path(&dir, self.config.quick_save.pattern, fmt, chrono::Local::now())
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
        let rx = cx.prompt_for_new_path(&dir, name.as_deref());
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let _ = this.update(cx, |this, cx| {
                let Some(img) = this.export_image() else { return };
                let fmt = capshot_core::export::format_for_path(&path);
                match capshot_core::export::save(&img, &path, fmt, this.config.export.jpeg_quality) {
                    Ok(()) => this.after_save(&path, &img, cx),
                    Err(e) => this.set_status(format!("Save failed: {e}"), cx),
                }
            });
        })
        .detach();
    }

    // ---- rendering -----------------------------------------------------------

    fn tool_button(&self, tool: Tool, icon: IconName, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.editor.as_ref().is_some_and(|e| e.tool == tool);
        Button::new(SharedString::from(format!("tool-{}", tool.label())))
            .icon(icon)
            .small()
            .ghost()
            .selected(active)
            .disabled(self.editor.is_none())
            .tooltip(format!("{} ({})", tool.label(), tool.shortcut().to_ascii_uppercase()))
            .on_click(cx.listener(move |this, _, window, cx| this.select_tool(tool, window, cx)))
    }

    pub fn select_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var_os("CAPSHOT_TRACE").is_some() {
            eprintln!("tool {tool:?}");
        }
        self.commit_text_edit(window, cx);
        if let Some(e) = self.editor.as_mut() {
            e.set_tool(tool);
        }
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    fn capture_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex_bar(theme.title_bar, theme.border)
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).mr_3().child("capshot"))
            .child(
                Button::new("cap-full")
                    .icon(IconName::Monitor)
                    .label("Fullscreen")
                    .small()
                    .primary()
                    .on_click(cx.listener(|_, _, window, cx| capture_flow::fullscreen(window, cx))),
            )
            .child(
                Button::new("cap-region")
                    .icon(IconName::SquareDashed)
                    .label("Region")
                    .small()
                    .on_click(cx.listener(|_, _, window, cx| capture_flow::region(window, cx))),
            )
            .child(
                Button::new("cap-window")
                    .icon(IconName::AppWindow)
                    .label("Window")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| capture_flow::open_window_picker(this, window, cx))),
            )
            .child(
                Button::new("cap-clip")
                    .icon(IconName::Clipboard)
                    .label("Clipboard")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| capture_flow::paste(this, window, cx))),
            )
            .child(
                Button::new("cap-import")
                    .icon(IconName::FolderOpen)
                    .label("Import")
                    .small()
                    .on_click(cx.listener(|_, _, window, cx| capture_flow::import(window, cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new("open-library")
                    .icon(IconName::Images)
                    .label("Library")
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| this.open_library(window, cx))),
            )
            .child(
                Button::new("open-settings")
                    .icon(IconName::Settings)
                    .small()
                    .ghost()
                    .tooltip("Settings")
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
    }

    fn annotation_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bar_bg, border) = (theme.secondary, theme.border);
        let e = self.editor.as_ref();
        let style = e.map(|e| e.style.clone()).unwrap_or_default();
        let selected_kind = e.and_then(|e| e.selected_annotation()).map(|a| a.kind.clone());
        let tool = e.map(|e| e.tool).unwrap_or(Tool::Select);
        let boxy = matches!(tool, Tool::Rectangle | Tool::Ellipse)
            || matches!(selected_kind, Some(AnnotationKind::Rectangle { .. } | AnnotationKind::Ellipse { .. }));
        let is_rect = tool == Tool::Rectangle || matches!(selected_kind, Some(AnnotationKind::Rectangle { .. }));
        let is_text = tool == Tool::Text || matches!(selected_kind, Some(AnnotationKind::Text { .. }));
        let is_arrow = tool == Tool::Arrow || matches!(selected_kind, Some(AnnotationKind::Arrow { .. }));
        let crop_mode = tool == Tool::Crop;
        let can_undo = e.is_some_and(|e| e.history.can_undo());
        let can_redo = e.is_some_and(|e| e.history.can_redo());

        let mut bar = h_flex_bar(bar_bg, border)
            .child(self.tool_button(Tool::Select, IconName::MousePointer2, cx))
            .child(self.tool_button(Tool::Rectangle, IconName::Square, cx))
            .child(self.tool_button(Tool::Ellipse, IconName::Circle, cx))
            .child(self.tool_button(Tool::Arrow, IconName::MoveUpRight, cx))
            .child(self.tool_button(Tool::Line, IconName::Minus, cx))
            .child(self.tool_button(Tool::Text, IconName::Type, cx))
            .child(self.tool_button(Tool::Number, IconName::Hash, cx))
            .child(self.tool_button(Tool::Spotlight, IconName::Flashlight, cx))
            .child(self.tool_button(Tool::Crop, IconName::Crop, cx))
            .child(separator(border));

        if crop_mode {
            for aspect in CropAspect::ALL {
                let active = e.is_some_and(|e| e.crop_aspect == aspect);
                bar = bar.child(
                    Button::new(SharedString::from(format!("aspect-{}", aspect.label())))
                        .label(aspect.label())
                        .small()
                        .ghost()
                        .selected(active)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(e) = this.editor.as_mut() {
                                e.set_crop_aspect(aspect);
                            }
                            this.save_config();
                            cx.notify();
                        })),
                );
            }
            return bar
                .child(separator(border))
                .child(Button::new("crop-apply").icon(IconName::Check).label("Apply").small().primary().on_click(
                    cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.apply_crop();
                        }
                        cx.notify();
                    }),
                ))
                .child(Button::new("crop-cancel").icon(IconName::X).label("Cancel").small().on_click(cx.listener(
                    |this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.cancel_crop();
                        }
                        cx.notify();
                    },
                )))
                .child(Button::new("crop-reset").icon(IconName::RotateCcw).label("Reset").small().ghost().on_click(
                    cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.reset_crop();
                        }
                        cx.notify();
                    }),
                ));
        }

        // Stroke colours (and "none" for boxes).
        let selected_stroke = e.and_then(|e| e.selected_annotation()).map(|a| a.stroke).unwrap_or(style.stroke);
        if boxy {
            bar = bar.child(swatch("stroke-none", None, selected_stroke.is_none(), cx, |this, _| {
                this.update_style(|s| s.stroke = None, |a| a.stroke = None)
            }));
        }
        for (i, hex) in ANNOTATION_COLORS.iter().enumerate() {
            let c = Rgba::parse(hex);
            bar = bar.child(swatch(format!("stroke-{i}"), c, selected_stroke == c, cx, move |this, _| {
                this.update_style(move |s| s.stroke = c, move |a| a.stroke = c)
            }));
        }
        if boxy {
            let selected_fill = e.and_then(|e| e.selected_annotation()).map(|a| a.fill).unwrap_or(style.fill);
            bar = bar.child(separator(border)).child(div().text_xs().child("Fill"));
            bar = bar.child(swatch("fill-none", None, selected_fill.is_none(), cx, |this, _| {
                this.update_style(|s| s.fill = None, |a| a.fill = None)
            }));
            for (i, hex) in ANNOTATION_COLORS.iter().enumerate().take(8) {
                let c = Rgba::parse(hex).map(|c| c.with_alpha_f(0.35));
                bar = bar.child(swatch(format!("fill-{i}"), c, selected_fill == c, cx, move |this, _| {
                    this.update_style(move |s| s.fill = c, move |a| a.fill = c)
                }));
            }
        }
        bar = bar.child(separator(border));
        if is_text {
            for size in FONT_SIZES {
                bar = bar.child(
                    Button::new(SharedString::from(format!("font-{size}")))
                        .label(format!("{size}"))
                        .small()
                        .ghost()
                        .selected(style.font_size == size)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_style(
                                move |s| s.font_size = size,
                                move |a| {
                                    if let AnnotationKind::Text { font_size, .. } = &mut a.kind {
                                        *font_size = size;
                                    }
                                },
                            );
                            cx.notify();
                        })),
                );
            }
            let bold = style.font_style.is_bold();
            let italic = style.font_style.is_italic();
            bar = bar
                .child(Button::new("bold").label("B").small().ghost().selected(bold).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let fs = font_style(!bold, italic);
                        this.update_style(move |s| s.font_style = fs, move |a| set_font_style(a, fs));
                        cx.notify();
                    },
                )))
                .child(Button::new("italic").label("I").small().ghost().selected(italic).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let fs = font_style(bold, !italic);
                        this.update_style(move |s| s.font_style = fs, move |a| set_font_style(a, fs));
                        cx.notify();
                    },
                )));
        } else {
            for w in STROKE_WIDTHS {
                bar = bar.child(
                    Button::new(SharedString::from(format!("width-{w}")))
                        .label(format!("{w}"))
                        .small()
                        .ghost()
                        .selected(style.stroke_width == w)
                        .tooltip("Stroke width")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_style(move |s| s.stroke_width = w, move |a| a.stroke_width = w);
                            cx.notify();
                        })),
                );
            }
        }
        if is_rect {
            bar = bar.child(separator(border));
            for r in CORNER_RADII {
                bar = bar.child(
                    Button::new(SharedString::from(format!("radius-{r}")))
                        .label(format!("r{r}"))
                        .small()
                        .ghost()
                        .selected(style.corner_radius == r)
                        .tooltip("Corner radius")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_style(
                                move |s| s.corner_radius = r,
                                move |a| {
                                    if let AnnotationKind::Rectangle { corner_radius, .. } = &mut a.kind {
                                        *corner_radius = r;
                                    }
                                },
                            );
                            cx.notify();
                        })),
                );
            }
        }
        if is_arrow {
            let curved = style.curved_arrows;
            bar = bar.child(separator(border)).child(
                Button::new("curved")
                    .icon(IconName::Spline)
                    .small()
                    .ghost()
                    .selected(curved)
                    .tooltip("Curved arrow")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_style(
                            move |s| s.curved_arrows = !curved,
                            move |a| {
                                if let AnnotationKind::Arrow { curved: c, curve_offset, .. } = &mut a.kind {
                                    *c = !curved;
                                    *curve_offset = None;
                                }
                            },
                        );
                        cx.notify();
                    })),
            );
        }
        bar.child(div().flex_1())
            .child(
                Button::new("undo")
                    .icon(IconName::Undo2)
                    .small()
                    .ghost()
                    .disabled(!can_undo)
                    .tooltip("Undo (Ctrl+Z)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.undo();
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("redo")
                    .icon(IconName::Redo2)
                    .small()
                    .ghost()
                    .disabled(!can_redo)
                    .tooltip("Redo (Ctrl+Shift+Z)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.redo();
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("delete")
                    .icon(IconName::Trash)
                    .small()
                    .ghost()
                    .disabled(e.is_none_or(|e| e.selected.is_none()))
                    .tooltip("Delete (Del)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.delete_selected();
                        }
                        cx.notify();
                    })),
            )
    }

    /// Change the drawing style and, if something is selected, restyle it.
    fn update_style(
        &mut self,
        style: impl FnOnce(&mut edit::Style),
        annotation: impl FnOnce(&mut capshot_core::Annotation),
    ) {
        if let Some(e) = self.editor.as_mut() {
            style(&mut e.style);
            e.restyle_selected(annotation);
            e.touch();
        }
    }

    fn export_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fmt = self.config.export.default_format;
        let zoom = self.editor.as_ref().map(|e| e.zoom);
        let has = self.editor.is_some();
        let mut bar = h_flex_bar(theme.title_bar, theme.border)
            .child(div().text_xs().text_color(theme.muted_foreground).flex_1().truncate().child(self.status.clone()));
        if let Some(z) = zoom {
            bar = bar
                .child(Button::new("zoom-out").icon(IconName::ZoomOut).small().ghost().on_click(cx.listener(
                    |this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.zoom_by(-ZOOM_STEP);
                        }
                        cx.notify();
                    },
                )))
                .child(div().text_xs().w(px(40.)).text_center().child(format!("{:.0}%", z * 100.0)))
                .child(Button::new("zoom-in").icon(IconName::ZoomIn).small().ghost().on_click(cx.listener(
                    |this, _, _, cx| {
                        if let Some(e) = this.editor.as_mut() {
                            e.zoom_by(ZOOM_STEP);
                        }
                        cx.notify();
                    },
                )))
                .child(
                    Button::new("zoom-fit").icon(IconName::Maximize).small().ghost().tooltip("Fit (Ctrl+0)").on_click(
                        cx.listener(|this, _, _, cx| {
                            if let Some(e) = this.editor.as_mut() {
                                e.zoom_reset();
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(separator(theme.border));
        }
        for (f, label) in [(ImageFormat::Png, "PNG"), (ImageFormat::Jpeg, "JPEG")] {
            bar = bar.child(Button::new(label).label(label).small().ghost().selected(fmt == f).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.config.export.default_format = f;
                    this.save_config();
                    cx.notify();
                },
            )));
        }
        if fmt == ImageFormat::Jpeg {
            bar = bar
                .child(div().w(px(100.)).child(Slider::new(&self.jpeg_quality)))
                .child(div().text_xs().w(px(28.)).child(format!("{}", self.config.export.jpeg_quality)));
        }
        let (r2, drive, busy) = (self.cloud_r2, self.cloud_drive, self.uploading);
        if r2 || drive {
            bar = bar.child(separator(theme.border));
        }
        if r2 {
            bar = bar.child(
                Button::new("up-r2")
                    .label("Upload R2")
                    .small()
                    .ghost()
                    .disabled(!has || busy)
                    .on_click(cx.listener(|this, _, window, cx| this.upload(false, window, cx))),
            );
        }
        if drive {
            bar = bar.child(
                Button::new("up-drive")
                    .label("Upload Drive")
                    .small()
                    .ghost()
                    .disabled(!has || busy)
                    .on_click(cx.listener(|this, _, window, cx| this.upload(true, window, cx))),
            );
        }
        bar.child(separator(theme.border))
            .child(
                Button::new("copy")
                    .icon(IconName::Copy)
                    .label("Copy")
                    .small()
                    .disabled(!has)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_image(cx))),
            )
            .child(
                Button::new("quick-save")
                    .icon(IconName::Download)
                    .label("Quick Save")
                    .small()
                    .disabled(!has)
                    .on_click(cx.listener(|this, _, _, cx| this.quick_save(cx))),
            )
            .child(
                Button::new("save-as")
                    .icon(IconName::Save)
                    .label("Save As…")
                    .small()
                    .primary()
                    .disabled(!has)
                    .on_click(cx.listener(|this, _, window, cx| this.save_as(window, cx))),
            )
    }

    fn canvas_area(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.editor.is_some() {
            self.ensure_preview(window);
        }
        let chrome = self.chrome();
        let bounds_cell = self.canvas_bounds.clone();
        let theme = cx.theme();
        let empty_text = theme.muted_foreground;
        let accent: Hsla = theme.primary;

        let text_overlay = self.text_edit.as_ref().and_then(|(id, state)| {
            let e = self.editor.as_ref()?;
            let t = self.view_transform()?;
            let a = e.doc.content.annotations.iter().find(|a| a.id == *id)?;
            let AnnotationKind::Text { origin, font_size, .. } = &a.kind else { return None };
            let layout = render::layout_for(&e.doc, &RenderOptions::default());
            let s = t.to_screen(e.doc.original_to_canvas(*origin));
            let b = self.canvas_bounds.get();
            let size_px = (font_size * t.scale * layout.image_scale).clamp(10.0, 120.0);
            Some(
                div()
                    .absolute()
                    .left(px(s.x - px_f(b.origin.x)))
                    .top(px(s.y - px_f(b.origin.y) - size_px * 1.4))
                    .w(px(320.))
                    .child(Textarea::new(state).small())
                    .child(div().text_xs().child("Click outside to finish")),
            )
        });

        let has_editor = self.editor.is_some();
        div()
            .id("canvas-area")
            .key_context(CANVAS_CONTEXT)
            .track_focus(&self.canvas_focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_canvas_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_canvas_down))
            .on_mouse_move(cx.listener(Self::on_canvas_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_canvas_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_canvas_up))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if let Some(p) = paths.paths().iter().find(|p| capshot_platform::clipboard::is_image_path(p)) {
                    capture_flow::open_path(this, p.clone(), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToolSelect, w, cx| this.select_tool(Tool::Select, w, cx)))
            .on_action(cx.listener(|this, _: &ToolRectangle, w, cx| this.select_tool(Tool::Rectangle, w, cx)))
            .on_action(cx.listener(|this, _: &ToolEllipse, w, cx| this.select_tool(Tool::Ellipse, w, cx)))
            .on_action(cx.listener(|this, _: &ToolArrow, w, cx| this.select_tool(Tool::Arrow, w, cx)))
            .on_action(cx.listener(|this, _: &ToolLine, w, cx| this.select_tool(Tool::Line, w, cx)))
            .on_action(cx.listener(|this, _: &ToolText, w, cx| this.select_tool(Tool::Text, w, cx)))
            .on_action(cx.listener(|this, _: &ToolNumber, w, cx| this.select_tool(Tool::Number, w, cx)))
            .on_action(cx.listener(|this, _: &ToolSpotlight, w, cx| this.select_tool(Tool::Spotlight, w, cx)))
            .on_action(cx.listener(|this, _: &ToolCrop, w, cx| this.select_tool(Tool::Crop, w, cx)))
            .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.delete_selected();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.escape();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ApplyCrop, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    if e.crop.is_some() {
                        e.apply_crop();
                    }
                }
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, _, _| bounds,
                    move |bounds, _, window, _| {
                        if std::env::var_os("CAPSHOT_TRACE").is_some() {
                            eprintln!("paint bounds {bounds:?} canvas {:?}", chrome.canvas);
                        }
                        if bounds_cell.get() != bounds {
                            bounds_cell.set(bounds);
                            // Layout changed: draw again with the new size.
                            window.on_next_frame(|window, _| window.refresh());
                        }
                        paint_chrome(&chrome, accent, window);
                    },
                )
                .absolute()
                .inset_0(),
            )
            .when(!has_editor, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .text_color(empty_text)
                        .child(div().text_lg().child("No screenshot"))
                        .child(div().text_sm().child("Capture, paste (Ctrl+V), import (Ctrl+O) or drop an image here")),
                )
            })
            .children(text_overlay)
    }
}

fn font_style(bold: bool, italic: bool) -> FontStyle {
    match (bold, italic) {
        (true, true) => FontStyle::BoldItalic,
        (true, false) => FontStyle::Bold,
        (false, true) => FontStyle::Italic,
        (false, false) => FontStyle::Normal,
    }
}

fn set_font_style(a: &mut capshot_core::Annotation, fs: FontStyle) {
    if let AnnotationKind::Text { font_style, .. } = &mut a.kind {
        *font_style = fs;
    }
}

fn paint_chrome(c: &Chrome, accent: Hsla, window: &mut Window) {
    if let Some((image, r)) = &c.image {
        let b = screen_bounds(*r);
        let _ = window.paint_image(b, b, Corners::default(), image.clone(), 0, false);
    }
    if let Some((image_rect, frame)) = c.crop {
        let dim = hsla(0., 0., 0., 0.5);
        let (l, t, r, b) = (image_rect.x, image_rect.y, image_rect.right(), image_rect.bottom());
        for rect in [
            Rect::new(l, t, image_rect.width, frame.y - t),
            Rect::new(l, frame.bottom(), image_rect.width, b - frame.bottom()),
            Rect::new(l, frame.y, frame.x - l, frame.height),
            Rect::new(frame.right(), frame.y, r - frame.right(), frame.height),
        ] {
            if rect.width > 0.0 && rect.height > 0.0 {
                window.paint_quad(fill(screen_bounds(rect), dim));
            }
        }
        window.paint_quad(quad(screen_bounds(frame), px(0.), transparent_black(), px(2.), white(), BorderStyle::Solid));
    }
    if let Some(sel) = c.selection {
        window.paint_quad(quad(screen_bounds(sel), px(0.), transparent_black(), px(1.), accent, BorderStyle::Dashed));
    }
    for (p, square) in &c.grips {
        let s = GRIP_SIZE;
        let r = Rect::new(p.x - s / 2.0, p.y - s / 2.0, s, s);
        let radius = if *square { px(2.) } else { px(s / 2.0) };
        window.paint_quad(quad(screen_bounds(r), radius, white(), px(1.5), accent, BorderStyle::Solid));
    }
}

pub fn h_flex_bar(bg: Hsla, border: Hsla) -> Div {
    div().flex().flex_row().items_center().gap_1().px_2().py_1().bg(bg).border_b_1().border_color(border).flex_wrap()
}

pub fn separator(color: Hsla) -> Div {
    div().w(px(1.)).h(px(18.)).mx_1().bg(color)
}

/// Round colour swatch; `None` shows a red slash for "no colour".
pub fn swatch(
    id: impl Into<SharedString>,
    color: Option<Rgba>,
    selected: bool,
    cx: &mut Context<MainView>,
    on_click: impl Fn(&mut MainView, &mut Context<MainView>) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let ring = if selected { theme.primary } else { theme.border };
    div()
        .id(ElementId::Name(id.into()))
        .size(px(18.))
        .rounded_full()
        .border_2()
        .border_color(ring)
        .cursor_pointer()
        .map(|d| match color {
            Some(c) => d.bg(to_hsla(c)),
            None => d.bg(white()).child(div().absolute().w(px(18.)).h(px(2.)).top(px(7.)).bg(red())),
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
            cx.notify();
        }))
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let canvas = self.canvas_area(window, cx);
        let panel = self.editor.is_some().then(|| settings_panel::render(self, cx));
        let picker = self.window_picker.clone();
        let settings = self.settings_dialog.clone();
        let library = self.library.clone();
        let update = self.update_dialog.clone();
        div()
            .key_context(APP_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &Undo, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.undo();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Redo, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.redo();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CopyImage, _, cx| this.copy_image(cx)))
            .on_action(cx.listener(|this, _: &QuickSave, _, cx| this.quick_save(cx)))
            .on_action(cx.listener(|this, _: &SaveAs, w, cx| this.save_as(w, cx)))
            .on_action(cx.listener(|this, _: &PasteImage, w, cx| capture_flow::paste(this, w, cx)))
            .on_action(cx.listener(|_, _: &ImportImage, w, cx| capture_flow::import(w, cx)))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.zoom_by(ZOOM_STEP);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.zoom_by(-ZOOM_STEP);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| {
                if let Some(e) = this.editor.as_mut() {
                    e.zoom_reset();
                }
                cx.notify();
            }))
            .on_action(|_: &Quit, _, cx| cx.quit())
            .child(self.capture_toolbar(cx))
            .child(self.annotation_toolbar(cx))
            .child(div().flex().flex_row().flex_1().min_h_0().child(canvas).children(panel))
            .child(self.export_bar(cx))
            .children(picker)
            .children(library)
            .children(settings)
            .children(update)
    }
}
