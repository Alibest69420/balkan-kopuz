//! A ranking reads as a list, not a row of equals: it is numbered, and it says what it counted.

use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use store::Navigator;
use widgets::format::t;
use widgets::{Cover, focus, menus, motion, style};

use crate::Home;
use crate::data::Shelf;

/// Two columns: eight names read as a list at a glance, sixteen as a wall of text.
const COLUMNS: usize = 2;
const PHOTO: f32 = 40.;
const ROW: f32 = 56.;
/// Wide enough for a three-digit rank without the names starting at different places.
const RANK: f32 = 24.;

pub(crate) fn ranked(shelf: &Shelf, window: &mut Window, cx: &mut Context<Home>) -> AnyElement {
    let rows: Vec<AnyElement> = shelf
        .tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| row(index, tile, window, cx))
        .collect();
    // Down each column rather than across, so first and second are not side by side.
    let per = rows.len().div_ceil(COLUMNS);
    let mut columns: Vec<Vec<AnyElement>> = Vec::with_capacity(COLUMNS);
    let mut rest = rows;
    while !rest.is_empty() {
        let take = per.min(rest.len());
        columns.push(rest.drain(..take).collect());
    }
    v_flex()
        .w_full()
        .gap_4()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(t(crate::shelf::title(shelf.name))),
        )
        .child(
            h_flex().w_full().gap_8().items_start().children(
                columns
                    .into_iter()
                    .map(|column| v_flex().flex_1().min_w_0().gap_1().children(column)),
            ),
        )
        .into_any_element()
}

fn row(
    index: usize,
    tile: &crate::data::Tile,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let key = tile.key.clone();
    let handle = focus::handle(key.clone(), window, cx);
    let focused = handle.is_focused(window);
    let (over, pointer) = motion::hovered(motion::channel(&key, 0), window, cx);
    let wash = motion::color(
        motion::channel(&key, 1),
        style::white(
            match over || focused {
                true => style::RAISED,
                false => 0.,
            },
            cx,
        ),
        window,
        cx,
    );
    let route = tile.route.clone();
    let open = move |_: &mut Window, cx: &mut gpui_kit::App| {
        if let Some(route) = route.clone() {
            Navigator::navigate(route, cx)
        }
    };
    let go = tile.route.clone();
    focus::stop(div().id(key.clone()), &handle, open)
        .relative()
        .w_full()
        .h(px(ROW))
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .rounded(style::radius_lg())
        .bg(wash)
        .cursor_pointer()
        .on_hover(motion::track(pointer))
        .on_click(move |_, _, cx| {
            if let Some(route) = go.clone() {
                Navigator::navigate(route, cx)
            }
        })
        .child(
            div()
                .w(px(RANK))
                .flex_none()
                .text_right()
                // Tabular digits, so the numbers line up rather than wander by glyph width.
                .font_family(SharedString::new_static("monospace"))
                .text_xs()
                .text_color(style::white(style::TEXT_DIM, cx))
                .child(SharedString::from((index + 1).to_string())),
        )
        .child(
            Cover::new(tile.artwork.as_ref())
                .size(px(PHOTO))
                .rounded(px(PHOTO / 2.))
                .placeholder(tile.placeholder),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(tile.title.clone()),
                )
                .when(!tile.subtitle.is_empty(), |column| {
                    column.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(style::white(style::TEXT_DIM, cx))
                            .child(tile.subtitle.clone()),
                    )
                }),
        )
        .when(focused, |row| {
            row.child(focus::ring(style::radius_lg(), cx))
        })
        .context_menu(menus::builder(tile.target.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_ranking_fills_the_first_column_before_the_second() {
        // Eight names split four and four; seven split four and three, not three and four.
        assert_eq!(8_usize.div_ceil(super::COLUMNS), 4);
        assert_eq!(7_usize.div_ceil(super::COLUMNS), 4);
        assert_eq!(1_usize.div_ceil(super::COLUMNS), 1);
    }
}
