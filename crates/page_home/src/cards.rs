//! The cards the Home rows are made of, and the play button they all share.

use api::{AlbumInfo, PlaylistInfo, QueueContext, TrackInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled, div, px,
};
use store::{Navigator, PlayerModel, QueueModel, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{Cover, Tile, icons};

use crate::data::Recent;

pub(crate) fn album_size() -> Pixels {
    px(184.)
}

pub(crate) fn artist_size() -> Pixels {
    px(152.)
}

/// Play/pause for an album card: pause while this album is the one playing, and a toggle then.
pub(crate) fn play_button(scope: &str, album_id: &str, size: Pixels, cx: &mut App) -> AnyElement {
    let current = QueueModel::global(cx)
        .read(cx)
        .current()
        .map(|track| track.album_id.clone())
        .unwrap_or_default();
    let is_current = !album_id.is_empty() && current == album_id;
    let playing = is_current && PlayerModel::global(cx).read(cx).is_playing();
    let id = album_id.to_string();
    Button::new(SharedString::from(format!("{scope}-play-{album_id}")))
        .primary()
        .w(size)
        .h(size)
        .rounded(size * 0.5)
        .icon(icons::icon(if playing {
            icons::PAUSE
        } else {
            icons::PLAY
        }))
        .on_click(move |_, _, cx| {
            if is_current {
                PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
            } else if !id.is_empty() {
                actions::play_context(QueueContext::Album { id: id.clone() }, Some(0), cx);
            }
        })
        .into_any_element()
}

/// The overlay a card reveals while the pointer is over it.
fn on_hover(group: &SharedString, child: impl IntoElement) -> AnyElement {
    div()
        .invisible()
        .group_hover(group.clone(), |style| style.visible())
        .child(child)
        .into_any_element()
}

fn corner(group: &SharedString, bottom: impl IntoElement, top: Option<AnyElement>) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .right_2()
                .bottom_2()
                .child(on_hover(group, bottom)),
        )
        .when_some(top, |frame, menu| {
            frame.child(
                div()
                    .absolute()
                    .right_1()
                    .top_1()
                    .child(on_hover(group, menu)),
            )
        })
        .into_any_element()
}

/// Wraps a tile so its overlay can reveal itself on hover.
fn hoverable(group: SharedString, tile: impl IntoElement) -> AnyElement {
    div()
        .group(group)
        .flex_none()
        .child(tile)
        .into_any_element()
}

pub(crate) fn album_card(scope: &str, album: &AlbumInfo, cx: &mut App) -> AnyElement {
    let size = album_size();
    let group = SharedString::from(format!("{scope}-{}", album.id));
    let overlay = corner(&group, play_button(scope, &album.id, px(40.), cx), None);
    hoverable(
        group,
        Tile::new(format!("{scope}-{}", album.id), album.title.clone())
            .subtitle(album.artist.clone())
            .artwork(album.artwork.as_ref())
            .placeholder(icons::DISC)
            .size(size)
            .route(Route::Album(album.id.clone().into()))
            .overlay(overlay),
    )
}

/// A recently played row: the track's own cover, the album behind it, and the song menu.
pub(crate) fn recent_card(recent: &Recent, cx: &mut App) -> AnyElement {
    let scope = "home-continue";
    // A recent row without a resolved album has no id to key its button on, so the uid does it.
    let button_scope = format!("{scope}-{}", recent.track.uid);
    let album_id = recent.album.as_ref().map(|album| album.id.clone());
    let album_title = recent
        .album
        .as_ref()
        .map(|album| album.title.clone())
        .unwrap_or_else(|| recent.track.album.clone());
    let group = SharedString::from(format!("{scope}-{}", recent.track.uid));
    let overlay = corner(
        &group,
        play_button(
            &button_scope,
            album_id.as_deref().unwrap_or_default(),
            px(40.),
            cx,
        ),
        Some(track_menu(&recent.track)),
    );
    hoverable(
        group,
        Tile::new(
            format!("{scope}-{}", recent.track.uid),
            recent.track.title.clone(),
        )
        .subtitle(format!("{} — {album_title}", recent.track.artist))
        .artwork(recent.track.artwork.as_ref())
        .size(album_size())
        .when_some(album_id, |tile, id| tile.route(Route::Album(id.into())))
        .overlay(overlay),
    )
}

pub(crate) fn artist_card(name: &str, artwork: Option<&api::ArtworkRef>) -> AnyElement {
    Tile::new(format!("home-artist-{name}"), name.to_string())
        .artwork(artwork)
        .placeholder(icons::MIC)
        .round(true)
        .size(artist_size())
        .route(Route::Artist {
            name: name.to_string().into(),
            channel: None,
        })
        .into_any_element()
}

pub(crate) fn playlist_card(playlist: &PlaylistInfo, cx: &mut App) -> AnyElement {
    let group = SharedString::from(format!("home-playlist-{}", playlist.id));
    let radio = SourcesModel::global(cx)
        .read(cx)
        .capabilities()
        .playlist_radio;
    let overlay = radio.then(|| corner(&group, div(), Some(playlist_menu(&playlist.id))));
    hoverable(
        group,
        Tile::new(
            format!("home-playlist-{}", playlist.id),
            playlist.name.clone(),
        )
        .subtitle(t_with(
            "music_playlist_count",
            &[("count", playlist.track_keys.len().to_string())],
        ))
        .artwork(playlist.artwork.as_ref())
        .placeholder(icons::LIST)
        .size(album_size())
        .route(Route::Playlist(playlist.id.clone().into()))
        .when_some(overlay, |tile, overlay| tile.overlay(overlay)),
    )
}

/// The Listen Now list row: a wide tile rather than a card.
pub(crate) fn album_row(album: &AlbumInfo, cx: &mut App) -> AnyElement {
    let scope = "home-listen-now-list";
    let group = SharedString::from(format!("{scope}-{}", album.id));
    let id = album.id.clone();
    h_flex()
        .id(SharedString::from(format!("{scope}-row-{}", album.id)))
        .group(group.clone())
        .w(px(350.))
        .flex_grow(1.)
        .items_center()
        .gap_3()
        .pr_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .hover(|row| row.bg(cx.theme().list_hover))
        .cursor_pointer()
        .on_click(move |_, _, cx| Navigator::navigate(Route::Album(id.clone().into()), cx))
        .child(
            Cover::new(album.artwork.as_ref())
                .size(px(72.))
                .rounded(cx.theme().radius)
                .placeholder(icons::DISC),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .py_2()
                .child(div().truncate().text_sm().child(album.title.clone()))
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(album.artist.clone()),
                ),
        )
        .child(on_hover(&group, play_button(scope, &album.id, px(32.), cx)))
        .into_any_element()
}

/// The subset of the track row's menu a home card can carry out without a modal of its own.
fn track_menu(track: &TrackInfo) -> AnyElement {
    let key = track.key.clone();
    Button::new(SharedString::from(format!(
        "home-continue-menu-{}",
        track.uid
    )))
    .ghost()
    .xsmall()
    .icon(icons::icon(icons::MORE))
    .dropdown_menu(move |menu, _, cx| {
        let radio = SourcesModel::global(cx).read(cx).capabilities().track_radio;
        let menu = menu
            .item(
                PopupMenuItem::new(t("play_next"))
                    .icon(icons::icon(icons::QUEUE_NEXT))
                    .on_click({
                        let key = key.clone();
                        move |_, _, cx| actions::play_next(vec![key.clone()], cx)
                    }),
            )
            .item(
                PopupMenuItem::new(t("add_to_queue"))
                    .icon(icons::icon(icons::QUEUE_ADD))
                    .on_click({
                        let key = key.clone();
                        move |_, _, cx| actions::append(vec![key.clone()], cx)
                    }),
            )
            .when(radio, |menu| {
                menu.item(
                    PopupMenuItem::new(t("start_radio"))
                        .icon(icons::icon(icons::RADIO))
                        .on_click({
                            let key = key.clone();
                            move |_, _, cx| actions::start_track_radio(key.clone(), cx)
                        }),
                )
            });
        menu.item(
            PopupMenuItem::new(t("share_musicbrainz"))
                .icon(icons::icon(icons::SHARE))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::share_track(key.clone(), cx)
                }),
        )
    })
    .into_any_element()
}

/// Radio is the one playlist action a home card can offer, so the whole menu rides its gate.
fn playlist_menu(id: &str) -> AnyElement {
    let playlist = id.to_string();
    Button::new(SharedString::from(format!("home-playlist-menu-{id}")))
        .ghost()
        .xsmall()
        .icon(icons::icon(icons::MORE))
        .dropdown_menu(move |menu, _, _| {
            menu.item(
                PopupMenuItem::new(t("start_radio"))
                    .icon(icons::icon(icons::RADIO))
                    .on_click({
                        let playlist = playlist.clone();
                        move |_, _, cx| actions::start_playlist_radio(playlist.clone(), cx)
                    }),
            )
        })
        .into_any_element()
}
