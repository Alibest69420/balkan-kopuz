//! Commands a page or a row menu issues, each with the toast the Dioxus frontend showed.

use std::future::Future;
use std::sync::Arc;

use api::ApiError;
use api::{KopuzApi, QueueContext, QueueMode, SetQueueRequest};
use gpui_kit::App;
use link::Link;

use crate::notices::Notices;
use crate::player::PlayerModel;

/// Run a daemon call, toasting its failure; the answer is dropped.
pub fn run<T, Fut>(cx: &mut App, f: impl FnOnce(Arc<dyn KopuzApi>) -> Fut)
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
{
    let call = Link::global(cx).call(f);
    cx.spawn(async move |cx| {
        if let Err(error) = call.await {
            cx.update(|cx| Notices::error(crate::human(&error), cx));
        }
    })
    .detach();
}

fn set_queue(request: SetQueueRequest, cx: &mut App) {
    run(cx, move |api| async move { api.set_queue(request).await });
}

/// Replace the queue with `keys` and start at `start`.
pub fn play_tracks(keys: Vec<String>, start: Option<u32>, cx: &mut App) {
    crate::Chrome::global(cx).update(cx, |chrome, cx| chrome.set_origin(None, cx));
    set_queue(
        SetQueueRequest {
            mode: QueueMode::Replace,
            context: QueueContext::Tracks { keys },
            start_index: start,
            shuffle: None,
        },
        cx,
    );
}

/// Play a whole listing: from a random row while shuffle is on, from the top otherwise.
pub fn play_all(keys: Vec<String>, cx: &mut App) {
    let shuffled = PlayerModel::global(cx).read(cx).shuffle();
    let start = if shuffled && !keys.is_empty() {
        Some(pseudo_random(keys.len()))
    } else {
        None
    };
    play_tracks(keys, start, cx);
}

pub fn append(keys: Vec<String>, cx: &mut App) {
    set_queue(
        SetQueueRequest {
            mode: QueueMode::Append,
            context: QueueContext::Tracks { keys },
            start_index: None,
            shuffle: None,
        },
        cx,
    );
}

pub fn play_next(keys: Vec<String>, cx: &mut App) {
    set_queue(
        SetQueueRequest {
            mode: QueueMode::PlayNext,
            context: QueueContext::Tracks { keys },
            start_index: None,
            shuffle: None,
        },
        cx,
    );
}

/// Play something the daemon materializes itself: an album, an artist, a playlist, a station.
pub fn play_context(context: QueueContext, start: Option<u32>, cx: &mut App) {
    remember_origin(&context, cx);
    set_queue(
        SetQueueRequest {
            mode: QueueMode::Replace,
            context,
            start_index: start,
            shuffle: None,
        },
        cx,
    );
}

/// Where a replaced queue came from, so the player can offer a way back to it.
fn remember_origin(context: &QueueContext, cx: &mut App) {
    let route = match context {
        QueueContext::Album { id } => Some(crate::Route::Album(id.clone().into())),
        QueueContext::Artist { artist } => Some(crate::Route::Artist {
            name: artist.name.clone().into(),
            artist_id: artist.id.clone().map(Into::into),
        }),
        QueueContext::Genre { name } => Some(crate::Route::Genre(name.clone().into())),
        QueueContext::Playlist { id } | QueueContext::PlaylistRadio { id } => {
            Some(crate::Route::Playlist(id.clone().into()))
        }
        _ => None,
    };
    crate::Chrome::global(cx).update(cx, |chrome, cx| chrome.set_origin(route, cx));
}

/// Start the source's mix from a track; gated by `capabilities().track_radio`.
pub fn start_track_radio(key: String, cx: &mut App) {
    Notices::info(i18n::t("radio_starting"), cx);
    start_radio(QueueContext::TrackRadio { key }, cx);
}

/// Start the source's mix from a playlist; gated by `capabilities().playlist_radio`.
pub fn start_playlist_radio(id: String, cx: &mut App) {
    Notices::info(i18n::t("radio_starting"), cx);
    start_radio(QueueContext::PlaylistRadio { id }, cx);
}

fn start_radio(context: QueueContext, cx: &mut App) {
    let call = Link::global(cx).call(move |api| async move {
        api.set_queue(SetQueueRequest {
            mode: QueueMode::Replace,
            context,
            start_index: Some(0),
            shuffle: None,
        })
        .await
    });
    cx.spawn(async move |cx| {
        if call.await.is_err() {
            cx.update(|cx| Notices::error(i18n::t("radio_failed"), cx));
        }
    })
    .detach();
}

/// Copy a track's public page to the clipboard, or say the source has none.
pub fn share_track(key: String, cx: &mut App) {
    let call = Link::global(cx).call(move |api| async move { api.track_web_url(key).await });
    cx.spawn(async move |cx| {
        let url = call.await;
        cx.update(|cx| match url {
            Ok(Some(url)) => {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(url));
                Notices::info(i18n::t("share_copied"), cx);
            }
            Ok(None) => Notices::error(i18n::t("share_no_page"), cx),
            Err(error) => Notices::error(crate::human(&error), cx),
        });
    })
    .detach();
}

/// Drop a queued row, with six seconds to put it back.
pub fn queue_remove(index: u32, cx: &mut App) {
    let removed = crate::QueueModel::global(cx).update(cx, |queue, cx| queue.remove(index, cx));
    let Some(track) = removed else { return };
    let key = track.key.clone();
    Notices::undo(
        i18n::t_with("removed_from_queue", &[("title", track.title.clone())]),
        move |cx| {
            let key = key.clone();
            crate::QueueModel::global(cx)
                .update(cx, |queue, cx| queue.insert(index, vec![key], cx));
        },
        cx,
    );
}

/// Favourite or unfavourite a whole selection; taking hearts away is offered back.
pub fn set_favorites(keys: Vec<String>, favorite: bool, cx: &mut App) {
    if keys.is_empty() {
        return;
    }
    crate::FavoritesModel::global(cx).update(cx, |favorites, cx| {
        for key in keys.clone() {
            favorites.set(key, favorite, cx);
        }
    });
    if favorite {
        return;
    }
    let undone = keys.clone();
    Notices::undo(
        i18n::t_with("unfavorited_count", &[("count", keys.len().to_string())]),
        move |cx| {
            let keys = undone.clone();
            crate::FavoritesModel::global(cx).update(cx, |favorites, cx| {
                for key in keys {
                    favorites.set(key, true, cx);
                }
            });
        },
        cx,
    );
}

/// A random index below `len`, from the hasher's per-process seed.
fn pseudo_random(len: usize) -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let value = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    (value % len as u64) as u32
}
