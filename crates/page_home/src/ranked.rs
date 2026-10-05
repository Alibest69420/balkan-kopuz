//! A ranking reads as a list, not a row of equals: it is numbered, and it says what it counted.

use components::TileRow;
use components::format::t;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, FontWeight, IntoElement, ParentElement as _, Styled, div};

use crate::data::Shelf;

/// Two columns: eight names read as a list at a glance, sixteen as a wall of text.
const COLUMNS: usize = 2;

pub(crate) fn ranked(shelf: &Shelf) -> AnyElement {
    let rows: Vec<AnyElement> = shelf
        .tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| row(index, tile))
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

fn row(index: usize, tile: &crate::data::Tile) -> AnyElement {
    let row = TileRow::new(tile.key.clone(), tile.title.clone())
        .subtitle(tile.subtitle.clone())
        .artwork(tile.artwork.as_ref())
        .round(true)
        .placeholder(tile.placeholder)
        .lead((index + 1).to_string())
        .menu(Some(tile.target.clone()));
    match tile.route.clone() {
        Some(route) => row.route(route).into_any_element(),
        None => row.into_any_element(),
    }
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
