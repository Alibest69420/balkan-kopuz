use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use widgets::{motion, style};

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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // A card has no entity behind it, so its hover lives in the element's own keyed state.
        let hover =
            window.use_keyed_state(motion::id("stat-card-hover", self.icon), cx, |_, _| false);
        let lit = *hover.read(cx);
        let alpha = if lit {
            style::BORDER
        } else {
            style::BORDER_SOFT
        };
        let border = motion::color(
            motion::id("stat-card-border", self.icon),
            style::white(alpha, cx),
            window,
            cx,
        );
        h_flex()
            .id(motion::id("stat-card", self.icon))
            .flex_1()
            .min_w_0()
            .gap_4()
            .items_center()
            .p_5()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(border)
            .on_hover(move |hovered: &bool, _, cx| {
                let hovered = *hovered;
                hover.update(cx, |lit, cx| {
                    if *lit != hovered {
                        *lit = hovered;
                        cx.notify();
                    }
                })
            })
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
