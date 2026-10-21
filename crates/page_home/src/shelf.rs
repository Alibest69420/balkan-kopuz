//! A shelf: the shared carousel, titled by its name and filled with its cards.

use components::format::t;
use components::{Carousel, Heading};
use gpui_kit::{AnyElement, IntoElement, ScrollHandle, px};

use crate::cards::{GAP, TILE, card};
use crate::data::{Form, Shelf};

pub(crate) fn shelf(shelf: &Shelf, handle: &ScrollHandle) -> AnyElement {
    carousel(shelf, handle).into_any_element()
}

pub(crate) fn carousel(shelf: &Shelf, handle: &ScrollHandle) -> Carousel {
    let tiles = shelf.tiles.clone();
    Carousel::new(
        shelf.name,
        t(title(shelf.name)),
        tiles.len(),
        TILE,
        move |index, _, cx| card(&tiles[index], cx),
    )
    .gap(px(GAP))
    .heading(match shelf.form {
        Form::Mosaic => Heading::Lead,
        _ => Heading::Shelf,
    })
    .handle(handle.clone())
}

/// The i18n key a shelf is titled by.
pub(crate) fn title(name: &str) -> &'static str {
    match name {
        "jump_back_in" => "jump_back_in",
        "for_you" => "for_you",
        "recently_added" => "recently_added",
        "made_for_you" => "made_for_you",
        "top_artists" => "top_artists",
        "your_playlists" => "playlists",
        _ => "recently_played",
    }
}

#[cfg(test)]
mod tests {
    use components::format::t;

    #[test]
    fn the_for_you_row_has_its_own_title() {
        assert_ne!(t(super::title("for_you")), "for_you");
        assert_ne!(super::title("for_you"), super::title("made_for_you"));
    }
}
