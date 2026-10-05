//! One shelf card: the shared tile, sized for a shelf, with what its play button starts.

use api::QueueContext;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Pixels, Styled, div, px};
use store::{PlayerModel, QueueModel, actions};
use widgets::PlayState;

use crate::data::{Play, Tile};

/// `gap-5` between cards, as the pixels the shelf's snap and its virtual window both count in.
pub(crate) const GAP: f32 = 20.;

/// `md:w-48`, which every shelf upstream settles on at desktop width.
pub(crate) const TILE: Pixels = px(192.);

/// What one card occupies, which is what a row snaps to and steps by.
pub(crate) fn step() -> Pixels {
    TILE + px(GAP)
}

pub(crate) fn card(tile: &Tile, cx: &App) -> AnyElement {
    let mut card = widgets::Tile::new(tile.key.clone(), tile.title.clone())
        .subtitle(tile.subtitle.clone())
        .artwork(tile.artwork.as_ref())
        .size(TILE)
        .round(tile.round)
        .placeholder(tile.placeholder)
        // Upstream only artists and playlists take the accent; an album title stays white.
        .tint(tile.round || matches!(tile.play, Play::Playlist(_)))
        .menu(Some(tile.target.clone()));
    if let Some(route) = tile.route.clone() {
        card = card.route(route);
    }
    if !matches!(tile.play, Play::None) {
        let current = is_current(&tile.play, cx);
        let state = match current && PlayerModel::global(cx).read(cx).is_playing() {
            true => PlayState::Playing,
            false => PlayState::Idle,
        };
        let play = tile.play.clone();
        card = card.play(state, move |_, _, cx| start(&play, current, cx));
    }
    div()
        .flex_shrink_0()
        .mr(px(GAP))
        .child(card)
        .into_any_element()
}

/// Whether the queue's current track is what `play` names.
fn is_current(play: &Play, cx: &App) -> bool {
    let queue = QueueModel::global(cx);
    let Some(track) = queue.read(cx).current() else {
        return false;
    };
    match play {
        Play::Album(id) => !track.album_id.is_empty() && &track.album_id == id,
        Play::Track(key) => !track.key.is_empty() && &track.key == key,
        _ => false,
    }
}

fn start(play: &Play, current: bool, cx: &mut App) {
    if current {
        PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
        return;
    }
    match play {
        Play::Album(id) => {
            actions::play_context(QueueContext::Album { id: id.clone() }, Some(0), cx)
        }
        Play::Playlist(id) => {
            actions::play_context(QueueContext::Playlist { id: id.clone() }, Some(0), cx)
        }
        Play::Track(key) => actions::play_tracks(vec![key.clone()], Some(0), cx),
        Play::None => {}
    }
}
