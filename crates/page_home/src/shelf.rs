//! A shelf: its heading, the arrows the pointer reveals, the fades at its edges, and its glide.

use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, linear_color_stop, linear_gradient, px,
};
use widgets::format::t;
use widgets::{icons, motion, scroll, style};

use crate::Home;
use crate::cards::{card, step};
use crate::data::{Form, Shelf};

/// The shelf's own channels, keyed off its name.
const HOVER: u64 = 0;
const ARROWS: u64 = 1;
const LEFT: u64 = 2;
const RIGHT: u64 = 3;
const BACK: u64 = 4;
const FORWARD: u64 = 5;

/// How wide the fade at each end is.
const FADE: f32 = 48.;

pub(crate) fn shelf(
    shelf: &Shelf,
    handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let name = SharedString::new_static(shelf.name);
    let lead = shelf.form == Form::Mosaic;
    let stride = step();
    scroll::smooth_x(shelf.name, handle, stride, window, cx);
    let (over, pointer) = motion::hovered(motion::channel(&name, HOVER), window, cx);
    let (back, forward) = scroll::room_x(handle);
    let (first, last) = window_of(handle, stride, shelf.tiles.len());

    let mut cards: Vec<AnyElement> = Vec::with_capacity(last - first + 2);
    // The tiles out of view stand as one empty box each way, so the row keeps its true width.
    cards.push(spacer(stride * first as f32));
    for tile in &shelf.tiles[first..last] {
        cards.push(card(tile, cx));
    }
    cards.push(spacer(stride * (shelf.tiles.len() - last) as f32));

    let arrows = motion::amount(
        motion::channel(&name, ARROWS),
        match over {
            true => 1.,
            false => 0.,
        },
        motion::FAST,
        window,
        cx,
    );
    let page = page_step(handle, stride);
    // The whole shelf is the hover, so reaching up for an arrow does not fade it away.
    v_flex()
        .id(ElementId::NamedInteger(name.clone(), 99))
        .w_full()
        .gap_6()
        .on_hover(motion::track(pointer))
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .map(|heading| match lead {
                            true => heading.text_3xl(),
                            false => heading.text_2xl(),
                        })
                        .child(t(title(shelf.name))),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .opacity(arrows)
                        .child(arrow(
                            &name,
                            BACK,
                            icons::CHEVRON_LEFT,
                            handle,
                            -page,
                            back,
                            window,
                            cx,
                        ))
                        .child(arrow(
                            &name,
                            FORWARD,
                            icons::CHEVRON_RIGHT,
                            handle,
                            page,
                            forward,
                            window,
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .relative()
                .w_full()
                .id(ElementId::NamedInteger(name.clone(), 100))
                .child(
                    h_flex()
                        .id(ElementId::NamedInteger(name.clone(), 101))
                        .w_full()
                        .pt_2()
                        .pb_6()
                        .items_start()
                        .overflow_x_scroll()
                        // Without this a bare wheel notch would scroll the page and slide the row at once.
                        .restrict_scroll_to_axis()
                        .on_scroll_wheel(wheel(handle.clone()))
                        .track_scroll(handle)
                        .children(cards),
                )
                .child(fade(&name, LEFT, true, back, window, cx))
                .child(fade(&name, RIGHT, false, forward, window, cx)),
        )
        .into_any_element()
}

/// The i18n key a shelf is titled by.
pub(crate) fn title(name: &str) -> &'static str {
    match name {
        "jump_back_in" => "jump_back_in",
        "recently_added" => "recently_added",
        "made_for_you" => "made_for_you",
        "top_artists" => "top_artists",
        "your_playlists" => "playlists",
        _ => "recently_played",
    }
}

/// Which tiles this row can actually show; the rest are two empty boxes it never builds.
fn window_of(handle: &ScrollHandle, stride: Pixels, count: usize) -> (usize, usize) {
    let width = handle.bounds().size.width;
    // Before the first layout there is no viewport to measure, so nothing is left out.
    if width <= px(0.) || stride <= px(0.) {
        return (0, count);
    }
    let stride = stride.as_f32();
    let first = ((-handle.offset().x.as_f32() / stride).floor().max(0.) as usize).min(count);
    let reach = (width.as_f32() / stride).ceil() as usize + 2;
    (first, (first + reach).min(count))
}

/// Shift and the wheel drives the row sideways, since a bare notch belongs to the page.
fn wheel(handle: ScrollHandle) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        let delta = event.delta.pixel_delta(window.line_height());
        if !event.modifiers.shift || delta.y == px(0.) {
            return;
        }
        scroll::nudge_x(&handle, -delta.y);
        cx.stop_propagation();
    }
}

/// One arrow's nudge: a viewport's worth, so a click lands the next screenful of tiles.
fn page_step(handle: &ScrollHandle, stride: Pixels) -> Pixels {
    let width = handle.bounds().size.width;
    match width > stride {
        true => px((width.as_f32() / stride.as_f32()).floor() * stride.as_f32()),
        false => stride,
    }
}

fn spacer(width: Pixels) -> AnyElement {
    div().w(width).flex_shrink_0().into_any_element()
}

/// `w-9 h-9 rounded-full bg-white/5 hover:bg-white/10`: one carousel arrow.
#[allow(clippy::too_many_arguments)]
fn arrow(
    name: &SharedString,
    channel: u64,
    icon: &'static str,
    handle: &ScrollHandle,
    by: Pixels,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let (over, pointer) = motion::hovered(motion::channel(name, channel + 10), window, cx);
    let background = motion::color(
        motion::channel(name, channel),
        style::white(
            match (enabled, over) {
                (false, _) => style::BORDER_SOFT,
                (true, true) => style::HOVER,
                (true, false) => style::RAISED,
            },
            cx,
        ),
        window,
        cx,
    );
    let handle = handle.clone();
    div()
        .id(ElementId::NamedInteger(name.clone(), channel))
        .flex()
        .items_center()
        .justify_center()
        .size(style::icon_button())
        .rounded_full()
        .bg(background)
        .text_color(style::white(
            match enabled {
                true => style::TEXT_STRONG,
                false => style::TEXT_DIM,
            },
            cx,
        ))
        .when(enabled, |arrow| arrow.cursor_pointer())
        .on_hover(motion::track(pointer))
        .child(Icon::default().path(icon).size(px(14.)))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if enabled {
                scroll::nudge_x(&handle, by);
            }
        })
        .into_any_element()
}

/// The page colour bled over the tiles at an end that has more behind it.
fn fade(
    name: &SharedString,
    channel: u64,
    left: bool,
    shown: bool,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let amount = motion::amount(
        motion::channel(name, channel),
        match shown {
            true => 1.,
            false => 0.,
        },
        motion::FAST,
        window,
        cx,
    );
    let background = cx.theme().background;
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(FADE))
        .map(|edge| match left {
            true => edge.left_0(),
            false => edge.right_0(),
        })
        .opacity(amount)
        .bg(linear_gradient(
            match left {
                true => 90.,
                false => 270.,
            },
            linear_color_stop(background, 0.),
            linear_color_stop(background.opacity(0.), 1.),
        ))
        .into_any_element()
}
