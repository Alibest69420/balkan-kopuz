//! The frames the three first-run steps share.

use components::{icons, motion, style};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Rems, SharedString, Stateful, StatefulInteractiveElement as _, Styled,
    Window, div, px, rems,
};

/// Lucide paths this flow needs that `components::icons` has no name for yet.
pub(crate) const SERVER: &str = "icons/server.svg";
pub(crate) const CLOUD: &str = "icons/cloud.svg";
pub(crate) const SPARKLES: &str = "icons/sparkles.svg";
pub(crate) const ARROW_LEFT: &str = "icons/arrow-left.svg";

/// `max-w-2xl mx-auto`: the column the whole flow is set in.
pub(crate) fn column_width() -> Rems {
    rems(40.)
}

/// `bg-white/8` over `bg-white/5`, the lift a choice card takes under the pointer.
const CARD_HOVER: f32 = 0.08;
/// `bg-<accent>/15` behind a card's glyph.
const BADGE_BG: f32 = 0.15;

/// `w-11 h-11`, the tinted square each card wears.
fn badge() -> Rems {
    rems(2.75)
}

pub(crate) fn heading(text: SharedString, cx: &App) -> Div {
    div()
        .text_base()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(style::white(style::TEXT_STRONG, cx))
        .child(text)
}

pub(crate) fn hint(text: SharedString, cx: &App) -> Div {
    div().text_sm().text_color(style::slate_400(cx)).child(text)
}

/// One step of the rail across the top, lit while it is the one being answered.
fn pip(index: usize, label: SharedString, current: usize, cx: &App) -> impl IntoElement {
    let done = index < current;
    let here = index == current;
    let color = if here {
        cx.theme().primary
    } else if done {
        style::white(style::TEXT_SECONDARY, cx)
    } else {
        style::white(style::TEXT_DIM, cx)
    };
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size_5()
                .rounded_full()
                .bg(if here {
                    style::accent(BADGE_BG, cx)
                } else {
                    style::white(style::RAISED, cx)
                })
                .text_size(style::text_10())
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(SharedString::from((index + 1).to_string())),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(label),
        )
}

/// The "1 Source · 2 Connect · 3 Scan" rail; `current` is the zero-based step.
pub(crate) fn rail(labels: [SharedString; 3], current: usize, cx: &App) -> impl IntoElement {
    h_flex().gap_4().items_center().children(
        labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| pip(index, label, current, cx).into_any_element()),
    )
}

/// A big pressable card: a tinted glyph, a title, a line saying what it gets you.
pub(crate) fn choice_card(
    id: impl Into<ElementId>,
    glyph: &'static str,
    accent: Hsla,
    title: SharedString,
    detail: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let (hovered, state) =
        motion::hovered(motion::id("first-run-card", format!("{id:?}")), window, cx);
    let lift = if hovered { CARD_HOVER } else { style::RAISED };
    let surface = motion::color(
        motion::id("first-run-card-bg", format!("{id:?}")),
        style::white(lift, cx),
        window,
        cx,
    );
    h_flex()
        .id(id)
        .w_full()
        .gap_4()
        .items_center()
        .p_4()
        .cursor_pointer()
        .rounded(style::radius_xl())
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .bg(surface)
        .on_hover(motion::track(state))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(badge())
                .rounded(style::radius_lg())
                .bg(accent.opacity(BADGE_BG))
                .child(icons::icon(glyph).size(px(18.)).text_color(accent)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                .when_some(detail, |column, detail| column.child(hint(detail, cx))),
        )
        .child(
            icons::icon(icons::CHEVRON_RIGHT)
                .size(px(16.))
                .flex_shrink_0()
                .text_color(style::white(style::TEXT_DIM, cx)),
        )
}
