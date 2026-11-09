//! "Update available" prompt with release notes.

use crate::main_view::MainView;
use capshot_cloud::updater::{self, Release};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;

pub struct UpdateDialog {
    main: WeakEntity<MainView>,
    release: Release,
}

/// Check in the background; show the dialog when a newer, unskipped release exists.
pub fn check_on_startup(window: &mut Window, cx: &mut Context<MainView>) {
    let Some(repo) = updater::REPO else { return };
    cx.spawn_in(window, async move |this, cx| {
        let result = cx.background_spawn(async move { updater::latest(repo, "https://api.github.com") }).await;
        let Ok(release) = result else { return };
        let _ = this.update(cx, |m, cx| {
            if updater::should_offer(updater::CURRENT_VERSION, &release, &m.config.update.skipped_version) {
                let main = cx.entity().downgrade();
                m.update_dialog = Some(cx.new(|_| UpdateDialog { main, release }));
                cx.notify();
            }
        });
    })
    .detach();
}

impl UpdateDialog {
    fn close(&self, cx: &mut App) {
        let _ = self.main.update(cx, |m, cx| {
            m.update_dialog = None;
            cx.notify();
        });
    }
}

impl Render for UpdateDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted) = (theme.popover, theme.border, theme.muted_foreground);
        let r = self.release.clone();
        div().absolute().inset_0().bg(hsla(0., 0., 0., 0.55)).flex().items_center().justify_center().occlude().child(
            div()
                .w(px(560.))
                .bg(bg)
                .rounded_lg()
                .border_1()
                .border_color(border)
                .flex()
                .flex_col()
                .child(
                    div()
                        .px_4()
                        .pt_3()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("capshot {} is available", r.version)),
                )
                .child(div().px_4().text_xs().text_color(muted).child(format!("You have {}", updater::CURRENT_VERSION)))
                .child(
                    div()
                        .id("notes")
                        .mx_4()
                        .my_3()
                        .max_h(px(300.))
                        .overflow_y_scroll()
                        .child(TextView::markdown("release-notes", r.notes.clone())),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(border)
                        .child(Button::new("upd-skip").label("Skip this version").small().ghost().on_click(
                            cx.listener(move |this, _, _, cx| {
                                let tag = this.release.tag.clone();
                                let _ = this.main.update(cx, |m, _| {
                                    m.config.update.skipped_version = tag;
                                    m.save_config();
                                });
                                this.close(cx);
                            }),
                        ))
                        .child(
                            Button::new("upd-later")
                                .label("Later")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        )
                        .child(Button::new("upd-download").label("Download").small().primary().on_click(cx.listener(
                            move |this, _, _, cx| {
                                cx.open_url(&this.release.url);
                                this.close(cx);
                            },
                        ))),
                ),
        )
    }
}
