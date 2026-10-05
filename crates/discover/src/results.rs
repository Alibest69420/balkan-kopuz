//! What a query turned up: the strips of albums and artists over the list of matching tracks.

use api::{AlbumInfo, TrackInfo};
use components::{Tile, icons};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled, px,
};
use store::{Playing, Route, Target};

const ALBUM_TILE: Pixels = px(128.);
// An artist strip has no artwork to show off, so its tiles only have to be legible.
const ARTIST_TILE: Pixels = px(88.);
/// How many of each a strip is worth showing; the rest are one more word in the query away.
const STRIP: usize = 16;

/// Search answers with tracks and albums; the artists behind them are worth a row of their own.
pub fn artists_of(tracks: &[TrackInfo], albums: &[AlbumInfo]) -> Vec<(String, SharedString)> {
    let mut artists: Vec<(String, SharedString)> = Vec::new();
    let credits = tracks
        .iter()
        .filter_map(|track| {
            let credit = track.primary_credit()?;
            Some((credit.key.clone()?, credit.name.as_str()))
        })
        .chain(
            albums
                .iter()
                .filter_map(|album| Some((album.artist_key.clone()?, album.artist.as_str()))),
        );
    for (key, name) in credits {
        let name = name.trim();
        if name.is_empty() || artists.len() >= STRIP {
            continue;
        }
        if artists.iter().any(|(held, _)| held == &key) {
            continue;
        }
        artists.push((key, name.to_string().into()));
    }
    artists
}

/// One album a query turned up: opens the album, right-clicks to the album's own menu.
fn album_card(album: &AlbumInfo, playing: &Playing) -> AnyElement {
    Tile::new(
        SharedString::from(format!("result-album-{}", album.id)),
        album.title.clone(),
    )
    .subtitle(album.artist.clone())
    .artwork(album.artwork.as_ref())
    .placeholder(icons::DISC)
    .size(ALBUM_TILE)
    .playing(playing.is_album(&album.id))
    .route(Route::Album(album.id.clone().into()))
    .menu(Some(Target::Album {
        id: album.id.clone().into(),
        title: album.title.clone().into(),
    }))
    .into_any_element()
}

/// One artist credited on what a query turned up.
fn artist_card(key: &String, name: &SharedString, playing: &Playing) -> AnyElement {
    Tile::new(
        SharedString::from(format!("result-artist-{key}")),
        name.clone(),
    )
    .placeholder(icons::MIC)
    .size(ARTIST_TILE)
    .round(true)
    .playing(playing.is_artist(key))
    .route(Route::Artist(key.clone()))
    .menu(Some(Target::Artist {
        key: key.clone(),
        name: name.clone(),
    }))
    .into_any_element()
}

/// A band of cards that scrolls sideways, so the track list below keeps the page's one scroller.
fn strip(id: &'static str, title: SharedString, cards: Vec<AnyElement>, cx: &App) -> AnyElement {
    gpui_kit::component::v_flex()
        .w_full()
        .flex_shrink_0()
        .gap_4()
        .mb_6()
        .child(crate::band_label(title, cx))
        .child(
            h_flex()
                .id(id)
                .w_full()
                .items_start()
                .gap_4()
                .pb_2()
                .overflow_x_scroll()
                // A bare wheel notch belongs to the page, not to the strip under the pointer.
                .restrict_scroll_to_axis()
                .children(cards),
        )
        .into_any_element()
}

/// The albums a query turned up, or nothing at all when it turned up none.
pub fn album_strip(
    albums: &[AlbumInfo],
    title: SharedString,
    playing: &Playing,
    cx: &App,
) -> Option<AnyElement> {
    if albums.is_empty() {
        return None;
    }
    let cards = albums
        .iter()
        .take(STRIP)
        .map(|album| album_card(album, playing))
        .collect();
    Some(strip("result-albums", title, cards, cx))
}

/// The artists credited on what a query turned up.
pub fn artist_strip(
    artists: &[(String, SharedString)],
    title: SharedString,
    playing: &Playing,
    cx: &App,
) -> Option<AnyElement> {
    if artists.is_empty() {
        return None;
    }
    let cards = artists
        .iter()
        .map(|(key, name)| artist_card(key, name, playing))
        .collect();
    Some(strip("result-artists", title, cards, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(artist: &str, key: &str) -> TrackInfo {
        TrackInfo {
            artist: artist.to_string(),
            credits: vec![api::ArtistCredit {
                name: artist.to_string(),
                key: Some(key.to_string()),
            }],
            ..TrackInfo::default()
        }
    }

    fn names(artists: &[(String, SharedString)]) -> Vec<&str> {
        artists.iter().map(|(_, name)| name.as_ref()).collect()
    }

    #[test]
    fn an_artist_is_listed_once_per_key_whatever_its_credits_say() {
        let tracks = [
            track("Boards of Canada", "boc"),
            track("boards of canada", "boc"),
            track("Boards of Canada", "a-namesake"),
        ];
        assert_eq!(
            names(&artists_of(&tracks, &[])),
            ["Boards of Canada", "Boards of Canada"]
        );
    }

    #[test]
    fn a_track_with_no_credit_contributes_no_artist() {
        let tracks = [track("  ", "blank"), track("Autechre", "autechre")];
        assert_eq!(names(&artists_of(&tracks, &[])), ["Autechre"]);
    }

    #[test]
    fn a_strip_never_grows_past_what_it_can_show() {
        let tracks: Vec<TrackInfo> = (0..40)
            .map(|at| track(&format!("artist {at}"), &format!("key-{at}")))
            .collect();
        assert_eq!(artists_of(&tracks, &[]).len(), STRIP);
    }
}
