//! Screenshot library: images in the quick-save folder, newest first, with
//! open, reveal and delete (to the trash).

use crate::main_view::MainView;
use capshot_core::library::{self, LibraryEntry};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct LibraryView {
    main: WeakEntity<MainView>,
    folder: PathBuf,
    entries: Option<Result<Vec<LibraryEntry>, String>>,
    thumbs: HashMap<PathBuf, (Arc<RenderImage>, u32, u32)>,
    confirm_delete: Option<PathBuf>,
}

pub fn open(view: &mut MainView, window: &mut Window, cx: &mut Context<MainView>) {
    let main = cx.entity().downgrade();
    let folder = PathBuf::from(&view.config.quick_save.folder);
    view.library = Some(cx.new(|cx| LibraryView::new(main, folder, window, cx)));
    cx.notify();
}

fn to_render_image(mut img: image::RgbaImage) -> Arc<RenderImage> {
    for p in img.pixels_mut() {
        p.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(img)]))
}

impl LibraryView {
    fn new(main: WeakEntity<MainView>, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self { main, folder, entries: None, thumbs: HashMap::new(), confirm_delete: None };
        this.reload(window, cx);
        this
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.folder.clone();
        cx.spawn_in(window, async move |this, cx| {
            let scan = cx.background_spawn(async move { library::scan(&folder).map_err(|e| e.to_string()) }).await;
            let paths: Vec<PathBuf> =
                scan.as_ref().map(|v| v.iter().map(|e| e.path.clone()).collect()).unwrap_or_default();
            if this
                .update(cx, |this, cx| {
                    this.entries = Some(scan);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            // Thumbnails fill in progressively, a batch at a time off the UI thread.
            for chunk in paths.chunks(8) {
                let batch = chunk.to_vec();
                let thumbs = cx
                    .background_spawn(async move {
                        batch
                            .into_iter()
                            .filter_map(|p| library::thumbnail(&p).ok().map(|t| (p, t)))
                            .collect::<Vec<_>>()
                    })
                    .await;
                let ok = this.update(cx, |this, cx| {
                    for (p, (img, w, h)) in thumbs {
                        this.thumbs.entry(p).or_insert_with(|| (to_render_image(img), w, h));
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn close(&self, cx: &mut App) {
        let _ = self.main.update(cx, |m, cx| {
            m.library = None;
            cx.notify();
        });
    }

    fn delete(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match capshot_platform::files::move_to_trash(&path) {
            Ok(()) => {
                if let Some(Ok(list)) = &mut self.entries {
                    list.retain(|e| e.path != path);
                }
                self.thumbs.remove(&path);
            }
            Err(e) => {
                let _ = self.main.update(cx, |m, cx| m.set_status(format!("Could not delete: {e}"), cx));
            }
        }
        self.confirm_delete = None;
        cx.notify();
    }
}

impl Render for LibraryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, hover) = (theme.popover, theme.border, theme.muted_foreground, theme.accent);
        let body: AnyElement = match &self.entries {
            None => div().p_4().text_color(muted).child("Loading…").into_any_element(),
            Some(Err(e)) => {
                div().p_4().child(format!("Could not read {}: {e}", self.folder.display())).into_any_element()
            }
            Some(Ok(list)) if list.is_empty() => {
                div().p_4().text_color(muted).child("No screenshots yet. Quick Save puts them here.").into_any_element()
            }
            Some(Ok(list)) => {
                let mut grid = div().flex().flex_wrap().gap_3().p_3();
                for (i, e) in list.iter().enumerate() {
                    let path = e.path.clone();
                    let thumb = self.thumbs.get(&path).cloned();
                    let date: chrono::DateTime<chrono::Local> = e.modified.into();
                    let confirming = self.confirm_delete.as_ref() == Some(&path);
                    let (p_open, p_reveal, p_del, p_confirm) = (path.clone(), path.clone(), path.clone(), path.clone());
                    grid = grid.child(
                        div()
                            .id(("lib", i))
                            .w(px(184.))
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .hover(|s| s.bg(hover))
                            .child(
                                div()
                                    .id(("lib-open", i))
                                    .h(px(120.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(match &thumb {
                                        Some((t, _, _)) => {
                                            img(t.clone()).max_h(px(120.)).max_w(px(168.)).into_any_element()
                                        }
                                        None => div().text_xs().text_color(muted).child("…").into_any_element(),
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let p = p_open.clone();
                                        let main = this.main.clone();
                                        this.close(cx);
                                        let _ =
                                            main.update(cx, |m, cx| crate::capture_flow::open_path(m, p, window, cx));
                                    })),
                            )
                            .child(div().mt_1().text_xs().truncate().child(e.file_name.clone()))
                            .child(div().text_xs().text_color(muted).child(format!(
                                "{}{}",
                                date.format("%Y-%m-%d %H:%M"),
                                thumb.map(|(_, w, h)| format!(" · {w}×{h}")).unwrap_or_default()
                            )))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .mt_1()
                                    .child(
                                        Button::new(("lib-reveal", i))
                                            .label("Show")
                                            .xsmall()
                                            .ghost()
                                            .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&p_reveal))),
                                    )
                                    .child(if confirming {
                                        Button::new(("lib-confirm", i))
                                            .label("Confirm delete")
                                            .xsmall()
                                            .danger()
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| this.delete(p_confirm.clone(), cx)),
                                            )
                                    } else {
                                        Button::new(("lib-del", i)).label("Delete").xsmall().ghost().on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.confirm_delete = Some(p_del.clone());
                                                cx.notify();
                                            }),
                                        )
                                    }),
                            ),
                    );
                }
                grid.into_any_element()
            }
        };
        div().absolute().inset_0().bg(hsla(0., 0., 0., 0.55)).flex().items_center().justify_center().occlude().child(
            div()
                .w(px(940.))
                .max_h(px(680.))
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
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(border)
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Library"))
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(self.folder.display().to_string()),
                        )
                        .child(Button::new("lib-refresh").label("Refresh").small().ghost().on_click(cx.listener(
                            |this, _, window, cx| {
                                this.thumbs.clear();
                                this.reload(window, cx);
                            },
                        )))
                        .child(
                            Button::new("lib-close")
                                .label("Close")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        ),
                )
                .child(div().id("lib-scroll").overflow_y_scroll().child(body)),
        )
    }
}
