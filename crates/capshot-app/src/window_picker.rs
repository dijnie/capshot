//! Window picker: a grid of open windows with thumbnails (winshot's
//! "Select Window" dialog).

use crate::main_view::MainView;
use capshot_platform::capture::{self, WindowInfo};
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use std::collections::HashMap;
use std::sync::Arc;

const THUMB: u32 = 240;

pub struct WindowPicker {
    main: WeakEntity<MainView>,
    windows: Option<Result<Vec<WindowInfo>, String>>,
    thumbs: HashMap<u32, Arc<RenderImage>>,
}

fn to_render_image(img: image::RgbaImage) -> Arc<RenderImage> {
    let mut img = img;
    for p in img.pixels_mut() {
        p.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(img)]))
}

impl WindowPicker {
    pub fn new(main: WeakEntity<MainView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.spawn_in(window, async move |this, cx| {
            let list = cx.background_spawn(async { capture::windows().map_err(|e| e.to_string()) }).await;
            let ids: Vec<u32> = list.as_ref().map(|l| l.iter().map(|w| w.id).collect()).unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                this.windows = Some(list);
                cx.notify();
            });
            for id in ids {
                let thumb = cx.background_spawn(async move { capture::window_thumbnail(id, THUMB).ok() }).await;
                let Some(thumb) = thumb else { continue };
                let image = to_render_image(thumb);
                if this
                    .update(cx, |this, cx| {
                        this.thumbs.insert(id, image);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self { main, windows: None, thumbs: HashMap::new() }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.main.update(cx, |m, cx| {
            m.window_picker = None;
            cx.notify();
        });
    }
}

impl Render for WindowPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, hover) = (theme.popover, theme.border, theme.muted_foreground, theme.accent);
        let body: AnyElement = match &self.windows {
            None => div().p_4().text_color(muted).child("Looking for windows…").into_any_element(),
            Some(Err(e)) => div().p_4().child(format!("Could not list windows: {e}")).into_any_element(),
            Some(Ok(list)) if list.is_empty() => {
                div().p_4().text_color(muted).child("No windows to capture").into_any_element()
            }
            Some(Ok(list)) => {
                let mut grid = div().flex().flex_wrap().gap_3().p_3();
                for w in list {
                    let id = w.id;
                    let thumb = self.thumbs.get(&id).cloned();
                    grid = grid.child(
                        div()
                            .id(("win", id as usize))
                            .w(px(200.))
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .cursor_pointer()
                            .hover(|s| s.bg(hover))
                            .child(div().h(px(120.)).flex().items_center().justify_center().child(match thumb {
                                Some(t) => img(t).max_h(px(120.)).max_w(px(184.)).into_any_element(),
                                None => div().text_xs().text_color(muted).child("…").into_any_element(),
                            }))
                            .child(div().mt_1().text_sm().truncate().child(w.title.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .truncate()
                                    .child(format!("{} · {}×{}", w.app_name, w.width, w.height)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let main = this.main.clone();
                                this.close(cx);
                                let _ =
                                    main.update(cx, |_m, cx| crate::capture_flow::capture_window_by_id(id, window, cx));
                            })),
                    );
                }
                grid.into_any_element()
            }
        };
        div().absolute().inset_0().bg(hsla(0., 0., 0., 0.55)).flex().items_center().justify_center().occlude().child(
            div()
                .w(px(920.))
                .max_h(px(640.))
                .bg(bg)
                .rounded_lg()
                .border_1()
                .border_color(border)
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(border)
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Select Window"))
                        .child(
                            Button::new("picker-close")
                                .label("Cancel")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        ),
                )
                .child(div().id("picker-scroll").overflow_y_scroll().child(body)),
        )
    }
}
