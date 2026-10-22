//! The "For you" row: the mix once it is in, pulsing tiles while it is asked for, and why it failed.

use std::time::Duration;

use components::format::t;
use components::{Carousel, SectionHeading, icons, motion, skeleton, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, IntoElement, ParentElement as _, ScrollHandle,
    SharedString, Styled, Window, div, pulsating_between, px,
};
use store::{ForYouModel, ForYouState};

use crate::cards::{GAP, TILE};
use crate::data::Shelf;

/// Enough placeholders to fill a wide window; the daemon's mix is about this long anyway.
const PLACEHOLDERS: usize = 8;

pub(crate) fn row(
    shelf: &Shelf,
    handle: &ScrollHandle,
    fade: &mut motion::Fade,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    let title = t(crate::shelf::title(shelf.name));
    let model = ForYouModel::global(cx).read(cx);
    let filled = model.shown_mix().is_some();
    let shown = fade.sample(filled, motion::MEDIUM, window, cx);
    if filled {
        let content = crate::shelf::carousel(shelf, handle).action(refresh(cx));
        // The pulsing row stays under the mix while it fades in, so the row never empties.
        let placeholder = (shown < 1.).then(|| placeholder(title.clone(), false, cx));
        return skeleton::reveal(content, placeholder, shown);
    }
    match model.state() {
        ForYouState::Failed(error) => v_flex()
            .w_full()
            .gap_4()
            .child(SectionHeading::new(title).trailing(refresh(cx)))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(store::human(error)),
            )
            .into_any_element(),
        _ => placeholder(title, true, cx),
    }
}

/// The pulsing row; `refresh` is off for the copy under a fading mix, which carries its own.
/// The pulsing row; the copy under a fading mix leaves the refresh button to the mix.
fn placeholder(title: SharedString, with_refresh: bool, cx: &App) -> AnyElement {
    let row = Carousel::new(
        "for_you_loading",
        title,
        PLACEHOLDERS,
        TILE,
        |slot, _, cx| pulsing(slot, cx),
    )
    .gap(px(GAP));
    match with_refresh {
        true => row.action(refresh(cx)).into_any_element(),
        false => row.into_any_element(),
    }
}

fn pulsing(slot: usize, cx: &App) -> AnyElement {
    div()
        .child(skeleton::shelf_tile(TILE, cx))
        .with_animation(
            SharedString::from(format!("for-you-pulse-{slot}")),
            Animation::new(Duration::from_millis(1100))
                .repeat()
                .with_easing(pulsating_between(0.4, 1.)),
            |tile, delta| tile.opacity(delta),
        )
        .into_any_element()
}

fn refresh(cx: &App) -> impl IntoElement {
    Button::new("for-you-refresh")
        .custom(style::quiet_button(
            style::white(style::TEXT_STRONG, cx),
            cx,
        ))
        .small()
        .rounded_full()
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .icon(icons::icon(icons::REFRESH))
        .label(t("refresh"))
        .on_click(|_, _, cx| {
            ForYouModel::global(cx).update(cx, |model, cx| model.refresh(cx));
        })
}
