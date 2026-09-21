use std::time::Duration;

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
use crate::motion;
use crate::style;

/// The tile's own element and its three transition channels, keyed off its id rather than formatted.
const ROOT: u64 = 0;
const HOVER: u64 = 1;
const WASH: u64 = 2;
const COVER: u64 = 3;

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
    zoom_speed: Duration,
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
            zoom_speed: motion::SLOW,
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

    /// How long the cover's `group-hover:scale-105` takes, for a grid the original marks `duration-500`.
    pub fn zoom_speed(mut self, duration: Duration) -> Self {
        self.zoom_speed = duration;
        self
    }
}

impl RenderOnce for Tile {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // `rounded-xl` on a cover, `rounded-full` on an artist.
        let radius = if self.round {
            self.size / 2.
        } else {
            style::to_px(style::radius_xl(), window)
        };
        let round = self.round;
        profile::elements::built();
        // A tile is low-cardinality, so its hover is worth tracking: the wash and the zoom need it.
        let pointer = window.use_keyed_state(motion::channel(&self.id, HOVER), cx, |_, _| false);
        let hovered = *pointer.read(cx);
        let focus = crate::focus::handle(format!("tile-focus:{}", self.id), window, cx);
        let focused = focus.is_focused(window);
        let wash = motion::color(
            motion::channel(&self.id, WASH),
            style::white(
                if hovered || focused {
                    style::RAISED
                } else {
                    0.
                },
                cx,
            ),
            window,
            cx,
        );
        let opens = self.route.clone();
        crate::focus::stop(
            v_flex().id(motion::channel(&self.id, ROOT)),
            &focus,
            move |_, cx| {
                if let Some(route) = opens.clone() {
                    Navigator::navigate(route, cx);
                }
            },
        )
        .relative()
        .flex_shrink_0()
        .gap_3()
        .p_2()
        .rounded(style::radius_xl())
        .bg(wash)
        .on_hover(move |over, _, cx| {
            pointer.update(cx, |pointer, cx| {
                *pointer = *over;
                cx.notify();
            })
        })
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
                        .placeholder(self.placeholder)
                        .zoom(motion::channel(&self.id, COVER), hovered)
                        .zoom_speed(self.zoom_speed),
                )
                .children(self.overlay),
        )
        .child(
            v_flex()
                .w(self.size)
                .gap_0p5()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .when(round, |title| title.text_center())
                        .child(self.title),
                )
                .when_some(self.subtitle, |meta, subtitle| {
                    meta.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(style::white(style::TEXT_SUBTLE, cx))
                            .when(round, |line| line.text_center())
                            .child(subtitle),
                    )
                }),
        )
        .when(focused, |tile| {
            tile.child(crate::focus::ring(style::radius_xl(), cx))
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
        .zoom_speed(motion::SLOWEST)
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
        .zoom_speed(motion::SLOWEST)
        .route(Route::Playlist(playlist.id.clone().into()))
}

/// The wrapping grid every tile page lays its tiles out in.
pub fn tile_grid() -> gpui_kit::Div {
    div().flex().flex_wrap().gap_2()
}
