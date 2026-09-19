//! The horizontal rows: their heading, their arrows, and the scroller they share.

use api::AlbumInfo;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, div, point, px,
};
use widgets::icons;

use crate::cards::album_card;

// The nudge the Dioxus arrows scrolled by.
const STEP: f32 = 300.;

/// Scroll a row by one step; `direction` is -1 for left and 1 for right.
pub(crate) fn nudge(handle: &ScrollHandle, direction: f32) {
    let offset = handle.offset();
    let x = (offset.x - px(STEP * direction)).clamp(-handle.max_offset().x, px(0.));
    handle.set_offset(point(x, offset.y));
}

/// A row heading with its arrows; the eyebrow is the Vaxry skin's small label above it.
pub(crate) fn heading(
    id: &str,
    title: SharedString,
    eyebrow: Option<SharedString>,
    handle: Option<&ScrollHandle>,
    cx: &mut App,
) -> AnyElement {
    let arrows = handle.map(|handle| {
        h_flex()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("{id}-left")))
                    .ghost()
                    .icon(icons::icon(icons::CHEVRON_LEFT))
                    .on_click({
                        let handle = handle.clone();
                        move |_, _, _| nudge(&handle, -1.)
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("{id}-right")))
                    .ghost()
                    .icon(icons::icon(icons::CHEVRON_RIGHT))
                    .on_click({
                        let handle = handle.clone();
                        move |_, _, _| nudge(&handle, 1.)
                    }),
            )
    });
    h_flex()
        .w_full()
        .justify_between()
        .items_end()
        .pb_2()
        .child(
            v_flex()
                .when_some(eyebrow, |column, eyebrow| {
                    column.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(eyebrow),
                    )
                })
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
        )
        .children(arrows)
        .into_any_element()
}

/// The horizontal strip every row lays its cards out in.
pub(crate) fn scroller(id: &str, handle: &ScrollHandle, cards: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .id(SharedString::from(id.to_string()))
        .w_full()
        .gap_4()
        .pt_1()
        .pb_2()
        .overflow_x_scroll()
        .track_scroll(handle)
        .children(cards)
        .into_any_element()
}

/// One album row: heading, arrows, cards. An empty row renders nothing.
pub(crate) fn album_row(
    id: &'static str,
    title: SharedString,
    eyebrow: Option<SharedString>,
    albums: &[AlbumInfo],
    handle: &ScrollHandle,
    cx: &mut App,
) -> AnyElement {
    if albums.is_empty() {
        return div().into_any_element();
    }
    let cards: Vec<AnyElement> = albums
        .iter()
        .map(|album| album_card(id, album, cx))
        .collect();
    v_flex()
        .w_full()
        .child(heading(id, title, eyebrow, Some(handle), cx))
        .child(scroller(id, handle, cards))
        .into_any_element()
}
