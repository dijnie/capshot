//! Region selection: one borderless top-most window per display showing the
//! frozen capture. Drag to select, release to finish, Esc to cancel.

use capshot_platform::capture::DisplayCapture;
use gpui_kit::*;
use image::RgbaImage;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const SELECTION_BLUE: u32 = 0x3b82f6;

struct Session {
    windows: Vec<AnyWindowHandle>,
    /// The editor was open before the capture; bring it back on cancel.
    restore_on_cancel: bool,
    finished: bool,
}

type SharedSession = Rc<RefCell<Session>>;

/// Close every overlay and hand the result to the editor. `current` is the
/// overlay whose event triggered this: GPUI cannot update a window from inside
/// its own event handler, so that one is closed directly.
fn finish(session: &SharedSession, result: Option<RgbaImage>, current: Option<&mut Window>, cx: &mut App) {
    let (windows, restore) = {
        let mut s = session.borrow_mut();
        if s.finished {
            return;
        }
        s.finished = true;
        (std::mem::take(&mut s.windows), s.restore_on_cancel)
    };
    let current_handle = current.as_ref().map(|w| w.window_handle());
    for w in windows {
        if Some(w) != current_handle {
            let _ = w.update(cx, |_, window, _| window.remove_window());
        }
    }
    if let Some(window) = current {
        window.remove_window();
    }
    // After this overlay's event handler returns: a selection opens the editor;
    // a cancel leaves a tray-only capshot hidden.
    cx.defer(move |cx| match result {
        Some(img) => crate::shell::deliver(img, "Region", cx),
        None if restore => crate::shell::report("Region capture cancelled".into(), cx),
        None => {}
    });
}

/// GPUI display that matches a captured display's desktop position.
fn match_display(shot: &DisplayCapture, cx: &App) -> Option<Rc<dyn PlatformDisplay>> {
    let d = &shot.display;
    cx.displays().into_iter().min_by_key(|g| {
        let b = g.bounds();
        let (gx, gy) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let raw = (gx - d.x as f32).abs() + (gy - d.y as f32).abs();
        let scaled = (gx * d.scale_factor - d.x as f32).abs() + (gy * d.scale_factor - d.y as f32).abs();
        raw.min(scaled) as i64
    })
}

pub fn open(shots: Vec<DisplayCapture>, restore_on_cancel: bool, cx: &mut App) {
    let session = Rc::new(RefCell::new(Session { windows: Vec::new(), restore_on_cancel, finished: false }));
    for shot in shots {
        let Some(display) = match_display(&shot, cx) else { continue };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(display.bounds())),
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: true,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: Some(display.id()),
            ..Default::default()
        };
        let session2 = session.clone();
        let opened = cx.open_window(options, move |window, cx| {
            let view = cx.new(|cx| RegionOverlay::new(shot.image, session2, cx));
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            view
        });
        match opened {
            Ok(handle) => session.borrow_mut().windows.push(handle.into()),
            Err(e) => log::error!("could not open region overlay: {e}"),
        }
    }
    if session.borrow().windows.is_empty() {
        finish(&session, None, None, cx);
    }
}

pub struct RegionOverlay {
    capture: Arc<RgbaImage>,
    image: Arc<RenderImage>,
    start: Option<gpui_kit::Point<Pixels>>,
    end: Option<gpui_kit::Point<Pixels>>,
    session: SharedSession,
    focus: FocusHandle,
}

impl RegionOverlay {
    fn new(capture: RgbaImage, session: SharedSession, cx: &mut Context<Self>) -> Self {
        let mut bgra = capture.clone();
        for p in bgra.pixels_mut() {
            p.0.swap(0, 2);
        }
        Self {
            capture: Arc::new(capture),
            image: Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(bgra)])),
            start: None,
            end: None,
            session,
            focus: cx.focus_handle(),
        }
    }

    fn selection(&self) -> Option<Bounds<Pixels>> {
        let (a, b) = (self.start?, self.end?);
        Some(Bounds::from_corners(point(a.x.min(b.x), a.y.min(b.y)), point(a.x.max(b.x), a.y.max(b.y))))
    }

    /// Physical pixels of the frozen capture under a selection in window coordinates.
    fn crop(&self, sel: Bounds<Pixels>, window: &Window) -> Option<RgbaImage> {
        let win = window.bounds().size;
        let sx = self.capture.width() as f32 / f32::from(win.width).max(1.0);
        let sy = self.capture.height() as f32 / f32::from(win.height).max(1.0);
        // Round both corners, then take the difference, so a selection keeps
        // its exact physical size (rounding width separately loses a pixel).
        let edge = |v: Pixels, scale: f32, max: u32| ((f32::from(v) * scale).round().max(0.0) as u32).min(max);
        let (cw, ch) = (self.capture.width(), self.capture.height());
        let (x, y) = (edge(sel.left(), sx, cw), edge(sel.top(), sy, ch));
        let w = edge(sel.right(), sx, cw).saturating_sub(x);
        let h = edge(sel.bottom(), sy, ch).saturating_sub(y);
        (w >= 2 && h >= 2).then(|| image::imageops::crop_imm(&*self.capture, x, y, w, h).to_image())
    }

    fn physical_size(&self, sel: Bounds<Pixels>, window: &Window) -> (u32, u32) {
        let win = window.bounds().size;
        let sx = self.capture.width() as f32 / f32::from(win.width).max(1.0);
        let sy = self.capture.height() as f32 / f32::from(win.height).max(1.0);
        let px = |v: Pixels, s: f32| (f32::from(v) * s).round() as u32;
        (px(sel.right(), sx).saturating_sub(px(sel.left(), sx)), px(sel.bottom(), sy).saturating_sub(px(sel.top(), sy)))
    }
}

impl Render for RegionOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sel = self.selection();
        let image = self.image.clone();
        let label = sel.map(|s| {
            let (w, h) = self.physical_size(s, window);
            (s, format!("{w} × {h}"))
        });
        div()
            .id("region-overlay")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .cursor(CursorStyle::Crosshair)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key == "escape" {
                    finish(&this.session.clone(), None, Some(window), cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    this.start = Some(ev.position);
                    this.end = Some(ev.position);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    finish(&this.session.clone(), None, Some(window), cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.start.is_some() && ev.pressed_button == Some(MouseButton::Left) {
                    this.end = Some(ev.position);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseUpEvent, window, cx| {
                    this.end = Some(ev.position);
                    let result = this.selection().and_then(|s| this.crop(s, window));
                    if result.is_some() {
                        finish(&this.session.clone(), result, Some(window), cx);
                    } else {
                        this.start = None;
                        this.end = None;
                        cx.notify();
                    }
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
                        let dim = hsla(0., 0., 0., 0.5);
                        match sel {
                            None => window.paint_quad(fill(bounds, dim)),
                            Some(s) => {
                                let (l, t) = (bounds.origin.x, bounds.origin.y);
                                let (r, b) = (bounds.right(), bounds.bottom());
                                for rect in [
                                    Bounds::from_corners(point(l, t), point(r, s.top())),
                                    Bounds::from_corners(point(l, s.bottom()), point(r, b)),
                                    Bounds::from_corners(point(l, s.top()), point(s.left(), s.bottom())),
                                    Bounds::from_corners(point(s.right(), s.top()), point(r, s.bottom())),
                                ] {
                                    window.paint_quad(fill(rect, dim));
                                }
                                let blue: Hsla = rgb(SELECTION_BLUE).into();
                                window.paint_quad(quad(
                                    s,
                                    px(0.),
                                    transparent_black(),
                                    px(2.),
                                    blue,
                                    BorderStyle::Solid,
                                ));
                                for c in [s.origin, s.top_right(), s.bottom_left(), s.bottom_right()] {
                                    let h = Bounds::new(point(c.x - px(4.), c.y - px(4.)), size(px(8.), px(8.)));
                                    window.paint_quad(fill(h, white()));
                                    window.paint_quad(quad(
                                        h,
                                        px(0.),
                                        transparent_black(),
                                        px(1.),
                                        blue,
                                        BorderStyle::Solid,
                                    ));
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div().absolute().top(px(16.)).left_0().right_0().flex().justify_center().child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(hsla(0., 0., 0., 0.7))
                        .text_color(white())
                        .text_sm()
                        .child("Drag to select a region · Esc or right-click to cancel"),
                ),
            )
            .children(label.map(|(s, text)| {
                div()
                    .absolute()
                    .left(s.left())
                    .top((s.top() - px(26.)).max(px(0.)))
                    .px_2()
                    .py(px(2.))
                    .rounded_sm()
                    .bg(rgb(SELECTION_BLUE))
                    .text_color(white())
                    .text_xs()
                    .child(text)
            }))
    }
}
