//! What a query turned up: the strips of albums and artists over the list of matching tracks.

use api::{AlbumInfo, TrackInfo};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled, div, px,
};
use store::{Route, Target};
use widgets::{Tile, icons, style};

const ALBUM_TILE: Pixels = px(128.);
// An artist strip has no artwork to show off, so its tiles only have to be legible.
const ARTIST_TILE: Pixels = px(88.);
/// How many of each a strip is worth showing; the rest are one more word in the query away.
const STRIP: usize = 16;

/// What the player is on, in the terms a card can be marked by.
#[derive(Default, PartialEq)]
pub struct Playing {
    pub album: Option<String>,
    pub artist: Option<String>,
}

impl Playing {
    pub fn is_album(&self, title: &str) -> bool {
        matches!(self.album.as_deref(), Some(held) if held.eq_ignore_ascii_case(title.trim()))
    }

    pub fn is_artist(&self, name: &str) -> bool {
        matches!(self.artist.as_deref(), Some(held) if held.eq_ignore_ascii_case(name.trim()))
    }
}

/// Search answers with tracks and albums; the artists behind them are worth a row of their own.
pub fn artists_of(tracks: &[TrackInfo], albums: &[AlbumInfo]) -> Vec<SharedString> {
    let mut names: Vec<SharedString> = Vec::new();
    let credits = tracks
        .iter()
        .map(|track| track.artist.as_str())
        .chain(albums.iter().map(|album| album.artist.as_str()));
    for credit in credits {
        let credit = credit.trim();
        if credit.is_empty() || names.len() >= STRIP {
            continue;
        }
        if names.iter().any(|held| held.eq_ignore_ascii_case(credit)) {
            continue;
        }
        names.push(credit.to_string().into());
    }
    names
}

/// The badge the playing card wears, over its cover's top corner.
fn badge(cx: &App) -> AnyElement {
    div()
        .absolute()
        .top_2()
        .left_2()
        .size_5()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(cx.theme().primary)
        .text_color(cx.theme().primary_foreground)
        .child(icons::icon(icons::VOLUME).size(px(11.)))
        .into_any_element()
}

/// One album a query turned up: opens the album, right-clicks to the album's own menu.
fn album_card(album: &AlbumInfo, playing: &Playing, cx: &App) -> AnyElement {
    let target = Target::Album {
        id: album.id.clone().into(),
        title: album.title.clone().into(),
    };
    let tile = Tile::new(
        SharedString::from(format!("result-album-{}", album.id)),
        album.title.clone(),
    )
    .subtitle(album.artist.clone())
    .artwork(album.artwork.as_ref())
    .placeholder(icons::DISC)
    .size(ALBUM_TILE)
    .route(Route::Album(album.id.clone().into()));
    div()
        .id(SharedString::from(format!(
            "result-album-slot-{}",
            album.id
        )))
        .flex_shrink_0()
        .child(match playing.is_album(&album.title) {
            true => tile.overlay(badge(cx)),
            false => tile,
        })
        .context_menu(widgets::menus::builder(target))
        .into_any_element()
}

/// One artist credited on what a query turned up.
fn artist_card(name: &SharedString, playing: &Playing, cx: &App) -> AnyElement {
    let target = Target::Artist {
        name: name.clone(),
        channel: None,
    };
    let tile = Tile::new(
        SharedString::from(format!("result-artist-{name}")),
        name.clone(),
    )
    .placeholder(icons::MIC)
    .size(ARTIST_TILE)
    .round(true)
    .route(Route::Artist {
        name: name.clone(),
        channel: None,
    });
    div()
        .id(SharedString::from(format!("result-artist-slot-{name}")))
        .flex_shrink_0()
        .child(match playing.is_artist(name) {
            true => tile.overlay(badge(cx)),
            false => tile,
        })
        .context_menu(widgets::menus::builder(target))
        .into_any_element()
}

/// A band of cards that scrolls sideways, so the track list below keeps the page's one scroller.
fn strip(id: &'static str, title: SharedString, cards: Vec<AnyElement>, cx: &App) -> AnyElement {
    gpui_kit::component::v_flex()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .mb_6()
        .child(
            div()
                .text_xs()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(style::white(style::TEXT_MUTED, cx))
                .child(title),
        )
        .child(
            h_flex()
                .id(id)
                .w_full()
                .items_start()
                .gap_4()
                .pb_2()
                .overflow_x_scroll()
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
        .map(|album| album_card(album, playing, cx))
        .collect();
    Some(strip("result-albums", title, cards, cx))
}

/// The artists credited on what a query turned up.
pub fn artist_strip(
    names: &[SharedString],
    title: SharedString,
    playing: &Playing,
    cx: &App,
) -> Option<AnyElement> {
    if names.is_empty() {
        return None;
    }
    let cards = names
        .iter()
        .map(|name| artist_card(name, playing, cx))
        .collect();
    Some(strip("result-artists", title, cards, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(artist: &str) -> TrackInfo {
        TrackInfo {
            artist: artist.to_string(),
            ..TrackInfo::default()
        }
    }

    #[test]
    fn a_credit_is_only_listed_once_however_it_is_cased() {
        let tracks = [track("Boards of Canada"), track("boards of canada")];
        assert_eq!(artists_of(&tracks, &[]), ["Boards of Canada"]);
    }

    #[test]
    fn a_track_with_no_credit_contributes_no_artist() {
        let tracks = [track("  "), track("Autechre")];
        assert_eq!(artists_of(&tracks, &[]), ["Autechre"]);
    }

    #[test]
    fn a_strip_never_grows_past_what_it_can_show() {
        let tracks: Vec<TrackInfo> = (0..40).map(|at| track(&format!("artist {at}"))).collect();
        assert_eq!(artists_of(&tracks, &[]).len(), STRIP);
    }

    #[test]
    fn the_playing_mark_ignores_case_and_padding() {
        let playing = Playing {
            album: Some("Geogaddi".into()),
            artist: None,
        };
        assert!(playing.is_album(" geogaddi "));
        assert!(!playing.is_album("Amber"));
        assert!(!playing.is_artist("Geogaddi"));
    }
}
