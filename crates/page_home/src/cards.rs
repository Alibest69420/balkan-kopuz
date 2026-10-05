//! One card: its cover, its hover, and the play button the pointer reveals.

use api::QueueContext;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled, Window, div, px, rems,
};
use store::{Navigator, PlayerModel, QueueModel, actions};
use widgets::{Cover, icons, menus, motion, style};

use crate::Home;
use crate::data::{Play, Tile};

/// A card's element and the channels it tweens on, keyed off its own id rather than a `format!`.
const ROOT: u64 = 0;
const HOVER: u64 = 1;
const COVER: u64 = 2;
const TITLE: u64 = 3;
const BUTTON: u64 = 4;
const BUTTON_HOVER: u64 = 5;
const WASH: u64 = 6;

/// `gap-5` between cards, as the pixels the shelf's snap and its virtual window both count in.
pub(crate) const GAP: f32 = 20.;

/// `md:w-48`, which every shelf upstream settles on at desktop width.
pub(crate) const TILE: Pixels = px(192.);

/// What one card occupies, which is what a row snaps to and steps by.
pub(crate) fn step() -> Pixels {
    TILE + px(GAP)
}

pub(crate) fn card(tile: &Tile, window: &mut Window, cx: &mut Context<Home>) -> AnyElement {
    profile::elements::built();
    let size = TILE;
    let radius = match tile.round {
        true => size / 2.,
        false => style::to_px(style::radius_xl(), window),
    };
    let (over, pointer) = motion::hovered(motion::channel(&tile.key, HOVER), window, cx);
    // Upstream only artists and playlists take the accent; an album title stays white.
    let tints = tile.round || matches!(tile.play, Play::Playlist(_));
    let title = motion::color(
        motion::channel(&tile.key, TITLE),
        match over && tints {
            true => cx.theme().primary_hover,
            false => cx.theme().foreground,
        },
        window,
        cx,
    );
    let wash = motion::color_over(
        motion::channel(&tile.key, WASH),
        style::black(if over { 0.2 } else { 0. }, cx),
        motion::SLOW,
        window,
        cx,
    );
    let focus = widgets::focus::handle(format!("home-card:{}", tile.key), window, cx);
    let focused = focus.is_focused(window);
    // Only the card under the pointer or the keyboard builds a button; the rest are cover and lines.
    let play = (over || focused)
        .then(|| play_button(tile, window, cx))
        .flatten();
    let route = tile.route.clone();
    let opens = tile.route.clone();
    widgets::focus::stop(
        v_flex().id(motion::channel(&tile.key, ROOT)),
        &focus,
        move |_, cx| {
            if let Some(route) = opens.clone() {
                Navigator::navigate(route, cx)
            }
        },
    )
    .relative()
    .w(size)
    .mr(px(GAP))
    .flex_shrink_0()
    .gap_4()
    .cursor_pointer()
    .on_hover(motion::track(pointer))
    .on_click(move |_, _, cx| {
        if let Some(route) = route.clone() {
            Navigator::navigate(route, cx)
        }
    })
    .child(
        div()
            .relative()
            .child(
                Cover::new(tile.artwork.as_ref())
                    .size(size)
                    .rounded(radius)
                    .placeholder(tile.placeholder)
                    .zoom(motion::channel(&tile.key, COVER), over),
            )
            .child(div().absolute().inset_0().rounded(radius).bg(wash))
            .children(play),
    )
    .child(
        v_flex()
            .w(size)
            .px_1()
            .when(tile.round, |lines| lines.items_center())
            .child(
                div()
                    .truncate()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(title)
                    .child(tile.title.clone()),
            )
            .when(!tile.subtitle.is_empty(), |lines| {
                lines.child(
                    div()
                        .mt_1()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(style::white(style::TEXT_MUTED, cx))
                        .child(tile.subtitle.clone()),
                )
            }),
    )
    .when(focused, |card| {
        card.child(widgets::focus::ring(style::radius_xl(), cx))
    })
    .context_menu(menus::builder(tile.target.clone()))
    .into_any_element()
}

/// `bg-indigo-500` over the cover's bottom-right corner: play, or pause what this card is playing.
fn play_button(tile: &Tile, window: &mut Window, cx: &mut Context<Home>) -> Option<AnyElement> {
    if matches!(tile.play, Play::None) {
        return None;
    }
    let current = QueueModel::global(cx)
        .read(cx)
        .current()
        .map(|track| (track.album_id.clone(), track.key.clone()));
    let is_current = match (&tile.play, &current) {
        (Play::Album(id), Some((album, _))) => !album.is_empty() && album == id,
        (Play::Track(key), Some((_, playing))) => !playing.is_empty() && playing == key,
        _ => false,
    };
    let playing = is_current && PlayerModel::global(cx).read(cx).is_playing();
    let (over, pointer) = motion::hovered(motion::channel(&tile.key, BUTTON_HOVER), window, cx);
    let tint = motion::color(
        motion::channel(&tile.key, BUTTON),
        match over {
            true => cx.theme().primary_hover,
            false => cx.theme().primary,
        },
        window,
        cx,
    );
    let play = tile.play.clone();
    Some(
        div()
            .id(motion::channel(&tile.key, BUTTON))
            .absolute()
            .right_2()
            .bottom_2()
            .flex()
            .items_center()
            .justify_center()
            .size(rems(2.5))
            .rounded_full()
            .bg(tint)
            .text_color(cx.theme().primary_foreground)
            .shadow_md()
            .on_hover(motion::track(pointer))
            .child(
                Icon::default()
                    .path(match playing {
                        true => icons::PAUSE,
                        false => icons::PLAY,
                    })
                    .size(px(14.)),
            )
            .on_click(move |_, _, cx| {
                // The button sits inside the card, whose own click opens what it names.
                cx.stop_propagation();
                if is_current {
                    PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                    return;
                }
                match &play {
                    Play::Album(id) => {
                        actions::play_context(QueueContext::Album { id: id.clone() }, Some(0), cx)
                    }
                    Play::Playlist(id) => actions::play_context(
                        QueueContext::Playlist { id: id.clone() },
                        Some(0),
                        cx,
                    ),
                    Play::Track(key) => actions::play_tracks(vec![key.clone()], Some(0), cx),
                    Play::None => {}
                }
            })
            .into_any_element(),
    )
}
