//! The cards the Home rows are made of, and the play button they all share.

use api::{AlbumInfo, PlaylistInfo, QueueContext, TrackInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Rems, SharedString, Stateful, StatefulInteractiveElement as _, Styled, div, px, rems,
};
use store::{Navigator, PlayerModel, QueueModel, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{Cover, icons, style};

use crate::data::Recent;

/// `w-48`: the album and playlist cards.
pub(crate) fn album_size() -> Pixels {
    px(192.)
}

/// `w-40`: the artist circle and the Listen Now card.
pub(crate) fn artist_size() -> Pixels {
    px(160.)
}

/// `w-44`: the Continue Listening card.
fn recent_size() -> Pixels {
    px(176.)
}

/// `rounded-xl` in the pixels a [`Cover`] wants.
fn card_radius() -> Pixels {
    px(12.)
}

/// How a card paints its play button; the album rows use the white one.
#[derive(Clone, Copy)]
pub(crate) enum PlayLook {
    /// `bg-indigo-500`, on a song or Listen Now card.
    Accent,
    /// `bg-white text-black`, on an album card.
    Light,
    /// `bg-white/10`, on the Listen Now list row.
    Subtle,
}

/// Play/pause for an album card: pause while this album is the one playing, and a toggle then.
pub(crate) fn play_button(
    scope: &str,
    album_id: &str,
    size: Rems,
    look: PlayLook,
    cx: &mut App,
) -> AnyElement {
    let current = QueueModel::global(cx)
        .read(cx)
        .current()
        .map(|track| track.album_id.clone())
        .unwrap_or_default();
    let is_current = !album_id.is_empty() && current == album_id;
    let playing = is_current && PlayerModel::global(cx).read(cx).is_playing();
    let id = album_id.to_string();
    let (background, foreground, hovered) = match look {
        PlayLook::Accent => (
            cx.theme().primary,
            cx.theme().primary_foreground,
            cx.theme().primary_hover,
        ),
        PlayLook::Light => (
            cx.theme().foreground,
            cx.theme().background,
            style::white(style::TEXT_STRONG, cx),
        ),
        PlayLook::Subtle => (
            style::white(style::HOVER, cx),
            style::white(0.8, cx),
            style::white(style::ACTIVE, cx),
        ),
    };
    div()
        .id(SharedString::from(format!("{scope}-play-{album_id}")))
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .rounded_full()
        .bg(background)
        .text_color(foreground)
        .hover(move |button| button.bg(hovered))
        .cursor_pointer()
        .child(
            Icon::default()
                .path(if playing { icons::PAUSE } else { icons::PLAY })
                .size(rems(size.0 * 0.35)),
        )
        .on_click(move |_, _, cx| {
            if is_current {
                PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
            } else if !id.is_empty() {
                actions::play_context(QueueContext::Album { id: id.clone() }, Some(0), cx);
            }
        })
        .into_any_element()
}

/// The play button and the menu a card reveals while the pointer is over it.
fn overlay(
    group: &SharedString,
    inset: Rems,
    bottom: Option<AnyElement>,
    top: Option<AnyElement>,
) -> Div {
    div()
        .absolute()
        .inset_0()
        .when_some(bottom, |frame, bottom| {
            frame.child(
                div()
                    .absolute()
                    .right(inset)
                    .bottom(inset)
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(bottom),
            )
        })
        .when_some(top, |frame, top| {
            frame.child(
                div()
                    .absolute()
                    .right_1()
                    .top_1()
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(top),
            )
        })
}

/// `flex-none w-N group cursor-pointer`: the frame every home card sits in.
fn frame(id: String, group: &SharedString, width: Pixels, route: Option<Route>) -> Stateful<Div> {
    v_flex()
        .id(SharedString::from(id))
        .group(group.clone())
        .flex_none()
        .w(width)
        .cursor_pointer()
        .when_some(route, |card, route| {
            card.on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        })
}

/// `text-white font-bold truncate text-sm md:text-base px-1`.
fn card_title(title: SharedString) -> Div {
    div()
        .truncate()
        .px_1()
        .text_base()
        .font_weight(FontWeight::BOLD)
        .child(title)
}

/// `text-xs md:text-sm text-white/50 truncate px-1 font-semibold mt-1`.
fn card_subtitle(subtitle: SharedString, alpha: f32, cx: &App) -> Div {
    div()
        .truncate()
        .px_1()
        .mt_1()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(style::white(alpha, cx))
        .child(subtitle)
}

pub(crate) fn album_card(scope: &str, album: &AlbumInfo, cx: &mut App) -> AnyElement {
    let size = album_size();
    let group = SharedString::from(format!("{scope}-{}", album.id));
    let play = play_button(scope, &album.id, rems(2.5), PlayLook::Light, cx);
    frame(
        format!("{scope}-{}", album.id),
        &group,
        size,
        Some(Route::Album(album.id.clone().into())),
    )
    .child(
        div()
            .relative()
            .mb_4()
            .child(
                Cover::new(album.artwork.as_ref())
                    .size(size)
                    .rounded(card_radius())
                    .placeholder(icons::DISC),
            )
            .child(overlay(&group, rems(0.75), Some(play), None)),
    )
    .child(card_title(album.title.clone().into()))
    .child(card_subtitle(album.artist.clone().into(), 0.5, cx))
    .into_any_element()
}

/// The Listen Now card: the same shape one size down, with the accent play button.
pub(crate) fn listen_now_card(album: &AlbumInfo, cx: &mut App) -> AnyElement {
    let scope = "home-listen-now";
    let size = artist_size();
    let group = SharedString::from(format!("{scope}-{}", album.id));
    let play = play_button(scope, &album.id, rems(2.25), PlayLook::Accent, cx);
    frame(
        format!("{scope}-{}", album.id),
        &group,
        size,
        Some(Route::Album(album.id.clone().into())),
    )
    .child(
        div()
            .relative()
            .mb_2()
            .child(
                Cover::new(album.artwork.as_ref())
                    .size(size)
                    .rounded(card_radius())
                    .placeholder(icons::DISC),
            )
            .child(overlay(&group, rems(0.5), Some(play), None)),
    )
    .child(
        div()
            .truncate()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .child(album.title.clone()),
    )
    .child(
        div()
            .truncate()
            .mt_0p5()
            .text_xs()
            .text_color(style::white(style::TEXT_SUBTLE, cx))
            .child(album.artist.clone()),
    )
    .into_any_element()
}

/// A recently played row: the track's own cover, the album behind it, and the song menu.
pub(crate) fn recent_card(recent: &Recent, cx: &mut App) -> AnyElement {
    let scope = "home-continue";
    let size = recent_size();
    // A recent row without a resolved album has no id to key its button on, so the uid does it.
    let button_scope = format!("{scope}-{}", recent.track.uid);
    let album_id = recent.album.as_ref().map(|album| album.id.clone());
    let album_title = recent
        .album
        .as_ref()
        .map(|album| album.title.clone())
        .unwrap_or_else(|| recent.track.album.clone());
    let group = SharedString::from(format!("{scope}-{}", recent.track.uid));
    let play = play_button(
        &button_scope,
        album_id.as_deref().unwrap_or_default(),
        rems(2.5),
        PlayLook::Accent,
        cx,
    );
    frame(
        format!("{scope}-{}", recent.track.uid),
        &group,
        size,
        album_id.map(|id| Route::Album(id.into())),
    )
    .child(
        div()
            .relative()
            .mb_3()
            .child(
                Cover::new(recent.track.artwork.as_ref())
                    .size(size)
                    .rounded(card_radius()),
            )
            .child(overlay(
                &group,
                rems(0.5),
                Some(play),
                Some(track_menu(&recent.track)),
            )),
    )
    .child(
        div()
            .truncate()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .child(recent.track.title.clone()),
    )
    .child(
        div()
            .truncate()
            .mt_0p5()
            .text_xs()
            .text_color(style::white(0.5, cx))
            .child(format!("{} — {album_title}", recent.track.artist)),
    )
    .into_any_element()
}

pub(crate) fn artist_card(name: &str, artwork: Option<&api::ArtworkRef>, cx: &App) -> AnyElement {
    let size = artist_size();
    let group = SharedString::from(format!("home-artist-{name}"));
    let accent = cx.theme().primary_hover;
    frame(
        format!("home-artist-{name}"),
        &group,
        size,
        Some(Route::Artist {
            name: name.to_string().into(),
            channel: None,
        }),
    )
    .child(
        div().mb_4().child(
            Cover::new(artwork)
                .size(size)
                .rounded(size * 0.5)
                .placeholder(icons::MIC),
        ),
    )
    .child(
        div()
            .truncate()
            .px_2()
            .text_center()
            .text_base()
            .font_weight(FontWeight::BOLD)
            .group_hover(group.clone(), move |name| name.text_color(accent))
            .child(name.to_string()),
    )
    .into_any_element()
}

pub(crate) fn playlist_card(playlist: &PlaylistInfo, cx: &mut App) -> AnyElement {
    let size = album_size();
    let group = SharedString::from(format!("home-playlist-{}", playlist.id));
    let accent = cx.theme().primary_hover;
    let radio = SourcesModel::global(cx)
        .read(cx)
        .capabilities()
        .playlist_radio;
    let menu = radio.then(|| playlist_menu(&playlist.id));
    frame(
        format!("home-playlist-{}", playlist.id),
        &group,
        size,
        Some(Route::Playlist(playlist.id.clone().into())),
    )
    .child(
        div()
            .relative()
            .mb_4()
            .child(
                Cover::new(playlist.artwork.as_ref())
                    .size(size)
                    .rounded(card_radius())
                    .placeholder(icons::LIST),
            )
            .child(overlay(&group, rems(0.5), None, menu)),
    )
    .child(
        card_title(playlist.name.clone().into())
            .group_hover(group.clone(), move |name| name.text_color(accent)),
    )
    .child(card_subtitle(
        t_with(
            "music_playlist_count",
            &[("count", playlist.track_keys.len().to_string())],
        ),
        0.4,
        cx,
    ))
    .into_any_element()
}

/// The Listen Now list row: a wide tile rather than a card.
pub(crate) fn album_row(album: &AlbumInfo, cx: &mut App) -> AnyElement {
    let scope = "home-listen-now-list";
    let group = SharedString::from(format!("{scope}-{}", album.id));
    let id = album.id.clone();
    let play = play_button(scope, &album.id, rems(2.), PlayLook::Subtle, cx);
    h_flex()
        .id(SharedString::from(format!("{scope}-row-{}", album.id)))
        .group(group.clone())
        .w(px(350.))
        .flex_grow(1.)
        .items_center()
        .overflow_hidden()
        .pr_4()
        .rounded(style::radius_xl())
        .border_1()
        .border_color(style::white(style::BORDER_SOFT, cx))
        .bg(style::white(style::RAISED, cx))
        .hover(|row| row.bg(style::white(style::HOVER, cx)))
        .cursor_pointer()
        .on_click(move |_, _, cx| Navigator::navigate(Route::Album(id.clone().into()), cx))
        .child(
            Cover::new(album.artwork.as_ref())
                .size(px(80.))
                .rounded(px(0.))
                .placeholder(icons::DISC),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .truncate()
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .child(album.title.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .mt_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(style::white(0.5, cx))
                        .child(album.artist.clone()),
                ),
        )
        .child(
            div()
                .invisible()
                .group_hover(group, |button| button.visible())
                .child(play),
        )
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
