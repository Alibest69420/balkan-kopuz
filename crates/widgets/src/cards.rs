use api::{AlbumInfo, ArtistInfo, ArtworkRef, PlaylistInfo};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use store::{Navigator, Route};

use crate::cover::Cover;
use crate::icons;

/// A cover tile with a title and a subtitle: the shape of every grid in the app.
#[derive(IntoElement)]
pub struct Tile {
    id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    artwork: Option<ArtworkRef>,
    size: Pixels,
    round: bool,
    placeholder: &'static str,
    route: Option<Route>,
    overlay: Option<AnyElement>,
}

impl Tile {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            artwork: None,
            size: px(160.),
            round: false,
            placeholder: icons::MUSIC,
            route: None,
            overlay: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn artwork(mut self, artwork: Option<&ArtworkRef>) -> Self {
        self.artwork = artwork.cloned();
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Artists are round everywhere in the app.
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    pub fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }

    /// Where a click goes.
    pub fn route(mut self, route: Route) -> Self {
        self.route = Some(route);
        self
    }

    /// A play button or menu drawn over the cover.
    pub fn overlay(mut self, overlay: impl IntoElement) -> Self {
        self.overlay = Some(overlay.into_any_element());
        self
    }
}

impl RenderOnce for Tile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let radius = if self.round { self.size / 2. } else { px(8.) };
        v_flex()
            .id(SharedString::from(format!("tile-{}", self.id)))
            .w(self.size)
            .gap_2()
            .p_2()
            .rounded(cx.theme().radius)
            .hover(|tile| tile.bg(cx.theme().list_hover))
            .when_some(self.route, |tile, route| {
                tile.on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
            })
            .child(
                div()
                    .relative()
                    .child(
                        Cover::new(self.artwork.as_ref())
                            .size(self.size)
                            .rounded(radius)
                            .placeholder(self.placeholder),
                    )
                    .children(self.overlay),
            )
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .when(self.round, |title| title.text_center())
                    .child(self.title),
            )
            .when_some(self.subtitle, |tile, subtitle| {
                tile.child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .when(self.round, |line| line.text_center())
                        .child(subtitle),
                )
            })
    }
}

/// The album tile: click opens the album.
pub fn album_tile(album: &AlbumInfo) -> Tile {
    Tile::new(album.id.clone(), album.title.clone())
        .subtitle(album.artist.clone())
        .artwork(album.artwork.as_ref())
        .placeholder(icons::DISC)
        .route(Route::Album(album.id.clone().into()))
}

/// The artist tile: round, click opens the artist.
pub fn artist_tile(artist: &ArtistInfo) -> Tile {
    Tile::new(artist.name.clone(), artist.name.clone())
        .artwork(artist.artwork.as_ref())
        .placeholder(icons::MIC)
        .round(true)
        .route(Route::Artist {
            name: artist.name.clone().into(),
            channel: None,
        })
}

/// The playlist tile: click opens the playlist.
pub fn playlist_tile(playlist: &PlaylistInfo) -> Tile {
    Tile::new(playlist.id.clone(), playlist.name.clone())
        .artwork(playlist.artwork.as_ref())
        .placeholder(icons::LIST)
        .route(Route::Playlist(playlist.id.clone().into()))
}

/// The wrapping grid every tile page lays its tiles out in.
pub fn tile_grid() -> gpui_kit::Div {
    div().flex().flex_wrap().gap_2()
}
