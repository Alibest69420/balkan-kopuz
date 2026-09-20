//! The cards the Home rows are made of, and the play button they all share.

use api::{AlbumInfo, PlaylistInfo, QueueContext, TrackInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Rems, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled, Window, div, px, rems,
};
use store::{Navigator, PlayerModel, QueueModel, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{Cover, icons, motion, style};

use crate::data::Recent;
use crate::{Home, Hovered, channel, track_hover};

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

/// What a card's play button starts. A track whose album is not in the library still plays.
#[derive(Clone)]
pub(crate) enum PlayTarget {
    Album(String),
    Track(String),
}

impl PlayTarget {
    /// An album card keys and compares on the album; a bare track has only its own key.
    fn album(&self) -> &str {
        match self {
            PlayTarget::Album(id) => id,
            PlayTarget::Track(_) => "",
        }
    }

    fn id(&self) -> &str {
        match self {
            PlayTarget::Album(id) | PlayTarget::Track(id) => id,
        }
    }
}

/// Play/pause for a card: pause while this is the thing playing, and a toggle then.
pub(crate) fn play_button(
    scope: &str,
    target: PlayTarget,
    size: Rems,
    look: PlayLook,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let current = QueueModel::global(cx)
        .read(cx)
        .current()
        .map(|track| track.album_id.clone())
        .unwrap_or_default();
    let album_id = target.album();
    let is_current = !album_id.is_empty() && current == album_id;
    let playing = is_current && PlayerModel::global(cx).read(cx).is_playing();
    let key = SharedString::from(format!("{scope}-play-{}", target.id()));
    let (rest, foreground, hot) = match look {
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
    let tint = if hovered.contains(&key) { hot } else { rest };
    let background = motion::color(channel(key.clone(), "bg"), tint, window, cx);
    let button = div()
        .id(key.clone())
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .rounded_full()
        .bg(background)
        .text_color(foreground)
        .cursor_pointer()
        .child(
            Icon::default()
                .path(if playing { icons::PAUSE } else { icons::PLAY })
                .size(rems(size.0 * 0.35)),
        )
        .on_click(move |_, _, cx| {
            if is_current {
                PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                return;
            }
            match &target {
                PlayTarget::Album(id) => {
                    actions::play_context(QueueContext::Album { id: id.clone() }, Some(0), cx)
                }
                // A recent track whose album the library has no row for; it still plays.
                PlayTarget::Track(key) => actions::play_tracks(vec![key.clone()], Some(0), cx),
            }
        });
    track_hover(button, key, cx).into_any_element()
}

/// The cover in an `overflow-hidden` frame, so the hover zoom reads as `group-hover:scale-105`.
#[allow(clippy::too_many_arguments)]
fn cover_frame(
    key: &SharedString,
    cover: Cover,
    size: Pixels,
    radius: Pixels,
    zoom: bool,
    hovered: bool,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let grown = motion::zoom(
        channel(key.clone(), "cover"),
        size,
        zoom && hovered,
        window,
        cx,
    );
    let scrim = motion::color_over(
        channel(key.clone(), "scrim"),
        style::black(if hovered { 0.2 } else { 0. }, cx),
        motion::SLOW,
        window,
        cx,
    );
    div()
        .relative()
        .size(size)
        .rounded(radius)
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .child(cover.size(grown).rounded(radius))
        .child(div().absolute().inset_0().size_full().bg(scrim))
}

/// The play button and the menu a card reveals while the pointer is over it.
fn overlay(
    key: &SharedString,
    inset: Rems,
    bottom: Option<AnyElement>,
    top: Option<AnyElement>,
    hovered: bool,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let reveal = motion::amount(
        channel(key.clone(), "reveal"),
        if hovered { 1. } else { 0. },
        motion::SLOW,
        window,
        cx,
    );
    div()
        .absolute()
        .inset_0()
        .when_some(bottom, |frame, bottom| {
            frame.child(
                div()
                    .absolute()
                    .right(inset)
                    .bottom(inset)
                    .opacity(reveal)
                    .child(bottom),
            )
        })
        // The menu reveal stays instant: a popup trigger must not ride a tweened opacity.
        .when_some(top, |frame, top| {
            frame.child(
                div()
                    .absolute()
                    .right_1()
                    .top_1()
                    .invisible()
                    .group_hover(key.clone(), |style| style.visible())
                    .child(top),
            )
        })
}

/// `flex-none w-N group cursor-pointer`: the frame every home card sits in, hover and all.
fn frame(
    key: &SharedString,
    width: Pixels,
    route: Option<Route>,
    cx: &mut Context<Home>,
) -> Stateful<Div> {
    let card = v_flex()
        .id(key.clone())
        .group(key.clone())
        .flex_none()
        .w(width)
        .cursor_pointer()
        .when_some(route, |card, route| {
            card.on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        });
    track_hover(card, key.clone(), cx)
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

/// `group-hover:text-indigo-400 transition-colors` on a card's name.
fn title_color(key: &SharedString, hovered: bool, window: &mut Window, cx: &mut App) -> Hsla {
    let target = if hovered {
        cx.theme().primary_hover
    } else {
        cx.theme().foreground
    };
    motion::color(channel(key.clone(), "title"), target, window, cx)
}

pub(crate) fn album_card(
    scope: &str,
    album: &AlbumInfo,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let size = album_size();
    let key = SharedString::from(format!("{scope}-{}", album.id));
    let over = hovered.contains(&key);
    let play = play_button(
        scope,
        PlayTarget::Album(album.id.clone()),
        rems(2.5),
        PlayLook::Light,
        hovered,
        window,
        cx,
    );
    let cover = cover_frame(
        &key,
        Cover::new(album.artwork.as_ref()).placeholder(icons::DISC),
        size,
        card_radius(),
        true,
        over,
        window,
        cx,
    );
    frame(&key, size, Some(Route::Album(album.id.clone().into())), cx)
        .child(
            div()
                .relative()
                .mb_4()
                .size(size)
                .child(cover)
                .child(overlay(
                    &key,
                    rems(0.75),
                    Some(play),
                    None,
                    over,
                    window,
                    cx,
                )),
        )
        .child(card_title(album.title.clone().into()))
        .child(card_subtitle(album.artist.clone().into(), 0.5, cx))
        .into_any_element()
}

/// The Listen Now card: the same shape one size down, with the accent play button.
pub(crate) fn listen_now_card(
    album: &AlbumInfo,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let scope = "home-listen-now";
    let size = artist_size();
    let key = SharedString::from(format!("{scope}-{}", album.id));
    let over = hovered.contains(&key);
    let play = play_button(
        scope,
        PlayTarget::Album(album.id.clone()),
        rems(2.25),
        PlayLook::Accent,
        hovered,
        window,
        cx,
    );
    let cover = cover_frame(
        &key,
        Cover::new(album.artwork.as_ref()).placeholder(icons::DISC),
        size,
        card_radius(),
        true,
        over,
        window,
        cx,
    );
    frame(&key, size, Some(Route::Album(album.id.clone().into())), cx)
        .child(
            div()
                .relative()
                .mb_2()
                .size(size)
                .child(cover)
                .child(overlay(&key, rems(0.5), Some(play), None, over, window, cx)),
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
pub(crate) fn recent_card(
    recent: &Recent,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
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
    let key = SharedString::from(format!("{scope}-{}", recent.track.uid));
    let over = hovered.contains(&key);
    // A single, or anything the library holds no album row for, plays as the track it is.
    let target = match album_id.clone() {
        Some(id) => PlayTarget::Album(id),
        None => PlayTarget::Track(recent.track.key.clone()),
    };
    let play = play_button(
        &button_scope,
        target,
        rems(2.5),
        PlayLook::Accent,
        hovered,
        window,
        cx,
    );
    let cover = cover_frame(
        &key,
        Cover::new(recent.track.artwork.as_ref()),
        size,
        card_radius(),
        true,
        over,
        window,
        cx,
    );
    // Without an album row there is no album page to open, so the card goes to the artist.
    let route = match album_id {
        Some(id) => Route::Album(id.into()),
        None => Route::Artist {
            name: recent.track.artist.clone().into(),
            channel: None,
        },
    };
    frame(&key, size, Some(route), cx)
        .child(
            div()
                .relative()
                .mb_3()
                .size(size)
                .child(cover)
                .child(overlay(
                    &key,
                    rems(0.5),
                    Some(play),
                    Some(track_menu(&recent.track)),
                    over,
                    window,
                    cx,
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

pub(crate) fn artist_card(
    name: &str,
    artwork: Option<&api::ArtworkRef>,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let size = artist_size();
    let key = SharedString::from(format!("home-artist-{name}"));
    let over = hovered.contains(&key);
    // The artist circle darkens on hover but never zooms: upstream scales no picture here.
    let cover = cover_frame(
        &key,
        Cover::new(artwork).placeholder(icons::MIC),
        size,
        size * 0.5,
        false,
        over,
        window,
        cx,
    );
    let title = title_color(&key, over, window, cx);
    frame(
        &key,
        size,
        Some(Route::Artist {
            name: name.to_string().into(),
            channel: None,
        }),
        cx,
    )
    .child(div().mb_4().child(cover))
    .child(
        div()
            .truncate()
            .px_2()
            .text_center()
            .text_base()
            .font_weight(FontWeight::BOLD)
            .text_color(title)
            .child(name.to_string()),
    )
    .into_any_element()
}

pub(crate) fn playlist_card(
    playlist: &PlaylistInfo,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let size = album_size();
    let key = SharedString::from(format!("home-playlist-{}", playlist.id));
    let over = hovered.contains(&key);
    let radio = SourcesModel::global(cx)
        .read(cx)
        .capabilities()
        .playlist_radio;
    let menu = radio.then(|| playlist_menu(&playlist.id));
    let cover = cover_frame(
        &key,
        Cover::new(playlist.artwork.as_ref()).placeholder(icons::LIST),
        size,
        card_radius(),
        true,
        over,
        window,
        cx,
    );
    let title = title_color(&key, over, window, cx);
    frame(
        &key,
        size,
        Some(Route::Playlist(playlist.id.clone().into())),
        cx,
    )
    .child(
        div()
            .relative()
            .mb_4()
            .size(size)
            .child(cover)
            .child(overlay(&key, rems(0.5), None, menu, over, window, cx)),
    )
    .child(card_title(playlist.name.clone().into()).text_color(title))
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
pub(crate) fn album_row(
    album: &AlbumInfo,
    hovered: &Hovered,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> AnyElement {
    let scope = "home-listen-now-list";
    let key = SharedString::from(format!("{scope}-{}", album.id));
    let over = hovered.contains(&key);
    let id = album.id.clone();
    let play = play_button(
        scope,
        PlayTarget::Album(album.id.clone()),
        rems(2.),
        PlayLook::Subtle,
        hovered,
        window,
        cx,
    );
    let background = motion::color_over(
        channel(key.clone(), "bg"),
        style::white(if over { style::HOVER } else { style::RAISED }, cx),
        motion::SLOW,
        window,
        cx,
    );
    let reveal = motion::amount(
        channel(key.clone(), "reveal"),
        if over { 1. } else { 0. },
        motion::SLOW,
        window,
        cx,
    );
    let cover = cover_frame(
        &key,
        Cover::new(album.artwork.as_ref()).placeholder(icons::DISC),
        px(80.),
        px(0.),
        false,
        over,
        window,
        cx,
    );
    let row = h_flex()
        .id(SharedString::from(format!("{scope}-row-{}", album.id)))
        .w(px(350.))
        .flex_grow(1.)
        .items_center()
        .overflow_hidden()
        .pr_4()
        .rounded(style::radius_xl())
        .border_1()
        .border_color(style::white(style::BORDER_SOFT, cx))
        .bg(background)
        .cursor_pointer()
        .on_click(move |_, _, cx| Navigator::navigate(Route::Album(id.clone().into()), cx))
        .child(cover)
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
        .child(div().opacity(reveal).child(play));
    track_hover(row, key, cx).into_any_element()
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
