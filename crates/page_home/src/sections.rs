//! The horizontal rows: their heading, their arrows, and the scroller they share.

use api::AlbumInfo;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, div, point, px, relative,
};
use widgets::{icons, style};

use crate::cards::album_card;

// The nudge the Dioxus arrows scrolled by.
const STEP: f32 = 300.;

/// Scroll a row by one step; `direction` is -1 for left and 1 for right.
pub(crate) fn nudge(handle: &ScrollHandle, direction: f32) {
    let offset = handle.offset();
    let x = (offset.x - px(STEP * direction)).clamp(-handle.max_offset().x, px(0.));
    handle.set_offset(point(x, offset.y));
}

/// `w-9 h-9 rounded-full bg-white/5 hover:bg-white/10`: one carousel arrow.
fn arrow(
    id: String,
    icon: &'static str,
    handle: &ScrollHandle,
    direction: f32,
    cx: &App,
) -> AnyElement {
    let handle = handle.clone();
    let hovered = style::white(style::HOVER, cx);
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .justify_center()
        .size(style::icon_button())
        .rounded_full()
        .bg(style::white(style::RAISED, cx))
        .text_color(cx.theme().foreground)
        .hover(move |button| button.bg(hovered))
        .cursor_pointer()
        .child(Icon::default().path(icon).size(px(14.)))
        .on_click(move |_, _, _| nudge(&handle, direction))
        .into_any_element()
}

/// A row heading with its arrows; the eyebrow is the Vaxry skin's small label above it.
pub(crate) fn heading(
    id: &str,
    title: SharedString,
    eyebrow: Option<SharedString>,
    handle: Option<&ScrollHandle>,
    cx: &mut App,
) -> AnyElement {
    heading_sized(id, title, eyebrow, handle, false, cx)
}

/// The Listen Now heading, which the normal skin sets a size larger than the rest.
pub(crate) fn heading_lead(
    id: &str,
    title: SharedString,
    eyebrow: Option<SharedString>,
    handle: Option<&ScrollHandle>,
    cx: &mut App,
) -> AnyElement {
    heading_sized(id, title, eyebrow, handle, true, cx)
}

fn heading_sized(
    id: &str,
    title: SharedString,
    eyebrow: Option<SharedString>,
    handle: Option<&ScrollHandle>,
    lead: bool,
    cx: &mut App,
) -> AnyElement {
    let arrows = handle.map(|handle| {
        h_flex()
            .gap_2()
            .child(arrow(
                format!("{id}-left"),
                icons::CHEVRON_LEFT,
                handle,
                -1.,
                cx,
            ))
            .child(arrow(
                format!("{id}-right"),
                icons::CHEVRON_RIGHT,
                handle,
                1.,
                cx,
            ))
    });
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .mb_6()
        .child(
            v_flex()
                .when_some(eyebrow, |column, eyebrow| {
                    column.child(
                        div()
                            .mb_0p5()
                            .text_size(style::text_10())
                            .font_weight(FontWeight::BOLD)
                            .text_color(style::white(0.35, cx))
                            .child(eyebrow),
                    )
                })
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .map(|title| {
                            if lead {
                                title.text_3xl().line_height(relative(1.))
                            } else {
                                title.text_2xl()
                            }
                        })
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
        .gap_5()
        .pt_2()
        .pb_6()
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
