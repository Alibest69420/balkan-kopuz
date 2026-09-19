use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled, Window,
    div, px,
};
use widgets::style;

/// One of the four counters the library opens with.
#[derive(IntoElement)]
pub struct StatCard {
    label: SharedString,
    value: SharedString,
    icon: &'static str,
}

impl StatCard {
    pub fn new(label: impl Into<SharedString>, value: usize, icon: &'static str) -> Self {
        Self {
            label: label.into(),
            value: value.to_string().into(),
            icon,
        }
    }
}

impl RenderOnce for StatCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .items_center()
            .p_5()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_12()
                    .flex_shrink_0()
                    .rounded(style::radius_lg())
                    .bg(style::white(style::RAISED, cx))
                    .child(
                        Icon::default()
                            .path(self.icon)
                            .size(px(18.))
                            .text_color(style::white(style::TEXT_SECONDARY, cx)),
                    ),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(style::slate_500(cx))
                            .truncate()
                            .child(self.label),
                    )
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.value),
                    ),
            )
    }
}
