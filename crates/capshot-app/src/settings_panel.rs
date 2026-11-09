//! Right-hand panel: canvas look (background, padding, corners, shadow,
//! inset, border, output ratio).

use crate::main_view::{to_hsla, MainView};
use capshot_core::color::Rgba;
use capshot_core::config::{BorderType, EditorConfig};
use capshot_core::geometry::OutputRatio;
use capshot_core::presets::{ANNOTATION_COLORS, GRADIENT_PRESETS};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Selectable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Padding,
    CornerRadius,
    Shadow,
    Inset,
    BorderWeight,
    BorderOpacity,
}

impl Field {
    const ALL: [Field; 6] =
        [Self::Padding, Self::CornerRadius, Self::Shadow, Self::Inset, Self::BorderWeight, Self::BorderOpacity];

    fn range(self) -> (f32, f32) {
        match self {
            Self::Padding => (0.0, 200.0),
            Self::CornerRadius => (0.0, 50.0),
            Self::Shadow => (0.0, 30.0),
            Self::Inset => (0.0, 50.0),
            Self::BorderWeight => (1.0, 50.0),
            Self::BorderOpacity => (0.0, 100.0),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Padding => "Padding",
            Self::CornerRadius => "Corner radius",
            Self::Shadow => "Shadow",
            Self::Inset => "Inset",
            Self::BorderWeight => "Weight",
            Self::BorderOpacity => "Opacity",
        }
    }

    fn get(self, s: &EditorConfig) -> u32 {
        match self {
            Self::Padding => s.padding,
            Self::CornerRadius => s.corner_radius,
            Self::Shadow => s.shadow_size,
            Self::Inset => s.inset,
            Self::BorderWeight => s.border_weight,
            Self::BorderOpacity => s.border_opacity,
        }
    }

    fn set(self, s: &mut EditorConfig, v: u32) {
        match self {
            Self::Padding => s.padding = v,
            Self::CornerRadius => s.corner_radius = v,
            Self::Shadow => s.shadow_size = v,
            Self::Inset => s.inset = v,
            Self::BorderWeight => s.border_weight = v,
            Self::BorderOpacity => s.border_opacity = v,
        }
    }
}

pub struct Sliders(Vec<(Field, Entity<SliderState>)>);

impl Sliders {
    pub fn new(s: &EditorConfig, cx: &mut App) -> Self {
        Self(
            Field::ALL
                .iter()
                .map(|&f| {
                    let (min, max) = f.range();
                    (f, cx.new(|_| SliderState::new().min(min).max(max).step(1.0).default_value(f.get(s) as f32)))
                })
                .collect(),
        )
    }

    pub fn subscribe(&self, cx: &mut Context<MainView>) -> Vec<Subscription> {
        self.0
            .iter()
            .map(|(field, state)| {
                let field = *field;
                cx.subscribe(state, move |this: &mut MainView, _, ev: &SliderEvent, cx| {
                    let (v, release) = match ev {
                        SliderEvent::Change(v) => (v.end(), false),
                        SliderEvent::Release(v) => (v.end(), true),
                    };
                    if let Some(e) = this.editor_mut() {
                        field.set(&mut e.doc.settings, v.round() as u32);
                        e.touch();
                    }
                    if release {
                        this.save_config();
                    }
                    cx.notify();
                })
            })
            .collect()
    }

    /// Move the sliders to a document's values (after loading a capture).
    pub fn sync(&self, s: &EditorConfig, window: &mut Window, cx: &mut App) {
        for (f, state) in &self.0 {
            let v = f.get(s) as f32;
            state.update(cx, |st, cx| st.set_value(v, window, cx));
        }
    }

    fn get(&self, f: Field) -> &Entity<SliderState> {
        &self.0.iter().find(|(x, _)| *x == f).unwrap().1
    }
}

fn section(title: &'static str, muted: Hsla) -> Div {
    div().mt_3().mb_1().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(muted).child(title.to_uppercase())
}

fn slider_row(view: &MainView, f: Field, value: u32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .mb_2()
        .child(div().flex().justify_between().text_xs().child(f.label()).child(format!("{value}")))
        .child(Slider::new(view.sliders.get(f)))
}

fn edit_settings(this: &mut MainView, f: impl FnOnce(&mut EditorConfig)) {
    if let Some(e) = this.editor_mut() {
        f(&mut e.doc.settings);
        e.touch();
    }
    this.save_config();
}

pub fn render(view: &MainView, cx: &mut Context<MainView>) -> impl IntoElement {
    let theme = cx.theme();
    let (muted, border, panel_bg, primary) = (theme.muted_foreground, theme.border, theme.sidebar, theme.primary);
    let s = view.editor.as_ref().map(|e| e.doc.settings.clone()).unwrap_or_default();

    let mut gradients = div().flex().flex_wrap().gap_1();
    for (i, p) in GRADIENT_PRESETS.iter().enumerate() {
        let value = p.value;
        let bg = capshot_core::background::Background::parse(value);
        let (a, b) = match &bg {
            capshot_core::background::Background::Linear(stops) => (stops[0].1, stops[stops.len() - 1].1),
            _ => (Rgba::BLACK, Rgba::BLACK),
        };
        let selected = s.background_color == value;
        gradients = gradients.child(
            div()
                .id(("grad", i))
                .size(px(26.))
                .rounded_md()
                .border_2()
                .border_color(if selected { primary } else { transparent_black() })
                .bg(linear_gradient(135., linear_color_stop(to_hsla(a), 0.), linear_color_stop(to_hsla(b), 1.)))
                .cursor_pointer()
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(p.name).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    edit_settings(this, |s| {
                        s.background_color = value.to_string();
                        s.auto_background = false;
                    });
                    cx.notify();
                })),
        );
    }
    let mut solids = div().flex().flex_wrap().gap_1().mt_1();
    for (i, hex) in ANNOTATION_COLORS.iter().enumerate() {
        let selected = s.background_color.eq_ignore_ascii_case(hex);
        solids = solids.child(
            div()
                .id(("solid", i))
                .size(px(18.))
                .rounded_sm()
                .border_2()
                .border_color(if selected { primary } else { border })
                .bg(to_hsla(Rgba::parse(hex).unwrap()))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    edit_settings(this, |s| {
                        s.background_color = hex.to_string();
                        s.auto_background = false;
                    });
                    cx.notify();
                })),
        );
    }

    let mut ratios = div().flex().flex_wrap().gap_1();
    for r in OutputRatio::ALL {
        ratios = ratios.child(
            Button::new(SharedString::from(format!("ratio-{}", r.label())))
                .label(r.label())
                .xsmall()
                .ghost()
                .selected(s.output_ratio == r)
                .on_click(cx.listener(move |this, _, _, cx| {
                    edit_settings(this, |s| s.output_ratio = r);
                    cx.notify();
                })),
        );
    }

    let mut border_types = div().flex().gap_1();
    for (t, label) in [(BorderType::Outside, "Outside"), (BorderType::Center, "Center"), (BorderType::Inside, "Inside")]
    {
        border_types = border_types.child(
            Button::new(SharedString::from(format!("btype-{label}")))
                .label(label)
                .xsmall()
                .ghost()
                .selected(s.border_type == t)
                .on_click(cx.listener(move |this, _, _, cx| {
                    edit_settings(this, |s| s.border_type = t);
                    cx.notify();
                })),
        );
    }
    let mut border_colors = div().flex().flex_wrap().gap_1().mb_2();
    for (i, hex) in ANNOTATION_COLORS.iter().enumerate() {
        let selected = s.border_color.eq_ignore_ascii_case(hex);
        border_colors = border_colors.child(
            div()
                .id(("bcolor", i))
                .size(px(16.))
                .rounded_full()
                .border_2()
                .border_color(if selected { primary } else { border })
                .bg(to_hsla(Rgba::parse(hex).unwrap()))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    edit_settings(this, |s| s.border_color = hex.to_string());
                    cx.notify();
                })),
        );
    }

    let show_bg = s.show_background;
    let auto_bg = s.auto_background;
    let border_on = s.border_enabled;
    div()
        .id("settings-panel")
        .w(px(260.))
        .h_full()
        .flex_none()
        .overflow_y_scroll()
        .bg(panel_bg)
        .border_l_1()
        .border_color(border)
        .px_3()
        .py_2()
        .child(Switch::new("show-bg").label("Background").checked(show_bg).small().on_click(cx.listener(
            move |this, _, _, cx| {
                edit_settings(this, |s| s.show_background = !show_bg);
                cx.notify();
            },
        )))
        .child(section("Layout", muted))
        .child(slider_row(view, Field::Padding, s.padding))
        .child(slider_row(view, Field::CornerRadius, s.corner_radius))
        .child(slider_row(view, Field::Shadow, s.shadow_size))
        .child(slider_row(view, Field::Inset, s.inset))
        .child(section("Background", muted))
        .child(Switch::new("auto-bg").label("Match screenshot edges").checked(auto_bg).small().on_click(cx.listener(
            move |this, _, _, cx| {
                let turn_on = !auto_bg;
                if let Some(e) = this.editor_mut() {
                    e.doc.settings.auto_background = turn_on;
                    if turn_on {
                        let c = capshot_core::edge_color::dominant_edge_color(e.doc.image()).to_hex();
                        e.doc.settings.background_color = c.clone();
                        e.doc.settings.inset_background_color = Some(c);
                    }
                    e.touch();
                }
                this.save_config();
                cx.notify();
            },
        )))
        .child(div().mt_2().child(gradients))
        .child(solids)
        .child(section("Output ratio", muted))
        .child(ratios)
        .child(section("Border", muted))
        .child(Switch::new("border-on").label("Show border").checked(border_on).small().on_click(cx.listener(
            move |this, _, _, cx| {
                edit_settings(this, |s| s.border_enabled = !border_on);
                cx.notify();
            },
        )))
        .when(border_on, |d| {
            d.child(div().mt_2().child(slider_row(view, Field::BorderWeight, s.border_weight)))
                .child(slider_row(view, Field::BorderOpacity, s.border_opacity))
                .child(border_colors)
                .child(border_types)
        })
}
