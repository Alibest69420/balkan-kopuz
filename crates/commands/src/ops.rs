//! What the registry's `run` bodies do, over `store`'s verbs and models.

use api::{QueueContext, SourceCapabilities, TrackInfo};
use gpui_kit::{App, Window};
use link::Link;
use store::{
    Chrome, DownloadsModel, FavoritesModel, Navigator, Notices, PlayerModel, PlaylistsModel,
    QueueModel, Route, SidePanel, SourcesModel, Target, actions,
};

/// How far one seek step moves, and how much one volume step is worth.
pub const SEEK_STEP_MS: u64 = 5_000;
pub const VOLUME_STEP: f32 = 0.05;

pub fn caps(cx: &App) -> SourceCapabilities {
    SourcesModel::global(cx).read(cx).capabilities()
}

/// The track keys a target stands for; a queue row is one track like any other.
pub fn track_keys(target: &Target) -> Vec<String> {
    match target {
        Target::Tracks { keys, .. } => keys.iter().map(|key| key.to_string()).collect(),
        Target::QueueItem { key, .. } => vec![key.to_string()],
        _ => Vec::new(),
    }
}

pub fn first_key(target: &Target) -> Option<String> {
    track_keys(target).into_iter().next()
}

pub fn album_id(target: &Target) -> Option<String> {
    match target {
        Target::Album { id, .. } => Some(id.to_string()),
        _ => None,
    }
}

pub fn playlist_id(target: &Target) -> Option<String> {
    match target {
        Target::Playlist { id, .. } => Some(id.to_string()),
        _ => None,
    }
}

pub fn folder_id(target: &Target) -> Option<String> {
    match target {
        Target::Folder { id, .. } => Some(id.to_string()),
        _ => None,
    }
}

pub fn queue_index(target: &Target) -> Option<u32> {
    match target {
        Target::QueueItem { index, .. } => Some(*index),
        _ => None,
    }
}

pub fn all_favorite(keys: &[String], cx: &App) -> bool {
    let favorites = FavoritesModel::global(cx).read(cx);
    !keys.is_empty() && keys.iter().all(|key| favorites.is_favorite(key))
}

pub fn all_stored(keys: &[String], cx: &App) -> bool {
    let downloads = DownloadsModel::global(cx).read(cx);
    !keys.is_empty() && keys.iter().all(|key| downloads.is_stored(key))
}

pub fn toggle_favorite(target: &Target, cx: &mut App) {
    let keys = track_keys(target);
    let favorite = !all_favorite(&keys, cx);
    store::actions::set_favorites(keys, favorite, cx);
}

pub fn toggle_download(target: &Target, cx: &mut App) {
    let keys = track_keys(target);
    let stored = all_stored(&keys, cx);
    DownloadsModel::global(cx).update(cx, |downloads, cx| {
        if stored {
            for key in keys {
                downloads.remove(key, cx);
            }
        } else {
            downloads.download(keys, cx);
        }
    });
}

/// Album order: by disc, then by track number.
fn in_album_order(tracks: &mut [TrackInfo]) {
    tracks.sort_by(|left, right| {
        left.disc_number
            .unwrap_or(1)
            .cmp(&right.disc_number.unwrap_or(1))
            .then_with(|| {
                left.track_number
                    .unwrap_or(0)
                    .cmp(&right.track_number.unwrap_or(0))
            })
    });
}

fn keys_of(tracks: &[TrackInfo]) -> Vec<String> {
    tracks
        .iter()
        .filter(|track| !track.key.is_empty())
        .map(|track| track.key.clone())
        .collect()
}

/// Read an album's tracks in play order, then hand their keys to `then`.
pub fn with_album_keys(
    id: String,
    cx: &mut App,
    then: impl FnOnce(Vec<String>, &mut App) + 'static,
) {
    let call =
        Link::global(cx).call(move |api| async move { api.album_tracks(id, store::all()).await });
    cx.spawn(async move |cx| match call.await {
        Ok(page) => {
            let mut tracks = page.items;
            in_album_order(&mut tracks);
            let keys = keys_of(&tracks);
            cx.update(|cx| then(keys, cx));
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

pub fn with_artist_keys(
    name: String,
    cx: &mut App,
    then: impl FnOnce(Vec<String>, &mut App) + 'static,
) {
    let call = Link::global(cx)
        .call(move |api| async move { api.artist_tracks(name, store::all()).await });
    cx.spawn(async move |cx| match call.await {
        Ok(page) => {
            let keys = keys_of(&page.items);
            cx.update(|cx| then(keys, cx));
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

/// Look one track up and hand it over; the menu knows a key, not an artist or an album.
fn with_track(key: String, cx: &mut App, then: impl FnOnce(TrackInfo, &mut App) + 'static) {
    let call = Link::global(cx).call(move |api| async move { api.tracks_by_keys(vec![key]).await });
    cx.spawn(async move |cx| match call.await {
        Ok(tracks) => {
            if let Some(track) = tracks.into_iter().next() {
                cx.update(|cx| then(track, cx));
            }
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

pub fn go_to_artist(target: &Target, cx: &mut App) {
    let Some(key) = first_key(target) else { return };
    with_track(key, cx, |track, cx| {
        if track.artist.is_empty() {
            return;
        }
        Navigator::navigate(
            Route::Artist {
                name: track.artist.into(),
                channel: None,
            },
            cx,
        );
    });
}

pub fn go_to_album(target: &Target, cx: &mut App) {
    let Some(key) = first_key(target) else { return };
    with_track(key, cx, |track, cx| {
        if track.album_id.is_empty() {
            return;
        }
        Navigator::navigate(Route::Album(track.album_id.into()), cx);
    });
}

pub fn playlist_keys(id: &str, cx: &App) -> Vec<String> {
    PlaylistsModel::global(cx)
        .read(cx)
        .playlist(id)
        .map(|playlist| playlist.track_keys.clone())
        .unwrap_or_default()
}

/// Whether a playlist currently sits inside a folder, so "remove from folder" is worth offering.
pub fn in_folder(id: &str, cx: &App) -> bool {
    PlaylistsModel::global(cx)
        .read(cx)
        .folders()
        .iter()
        .any(|folder| folder.playlist_ids.iter().any(|member| member == id))
}

pub fn play_album(target: &Target, cx: &mut App) {
    if let Some(id) = album_id(target) {
        actions::play_context(QueueContext::Album { id }, None, cx);
    }
}

pub fn play_artist(target: &Target, cx: &mut App) {
    if let Target::Artist { name, .. } = target {
        actions::play_context(
            QueueContext::Artist {
                name: name.to_string(),
            },
            None,
            cx,
        );
    }
}

pub fn play_playlist(target: &Target, cx: &mut App) {
    if let Some(id) = playlist_id(target) {
        actions::play_context(QueueContext::Playlist { id }, None, cx);
    }
}

pub fn delete_playlist(target: &Target, cx: &mut App) {
    let Some(id) = playlist_id(target) else {
        return;
    };
    actions::run(cx, move |api| async move { api.delete_playlist(id).await });
}

pub fn delete_folder(target: &Target, cx: &mut App) {
    let Some(id) = folder_id(target) else { return };
    actions::run(cx, move |api| async move {
        api.delete_playlist_folder(id).await
    });
}

pub fn rename_playlist(target: &Target, window: &mut Window, cx: &mut App) {
    let Some(id) = playlist_id(target) else {
        return;
    };
    widgets::dialogs::open_rename(
        widgets::format::t("rename_playlist"),
        target.label(),
        move |name, cx| {
            let id = id.clone();
            actions::run(
                cx,
                move |api| async move { api.rename_playlist(id, name).await },
            );
        },
        window,
        cx,
    );
}

pub fn rename_folder(target: &Target, window: &mut Window, cx: &mut App) {
    let Some(id) = folder_id(target) else { return };
    widgets::dialogs::open_rename(
        widgets::format::t("rename_folder"),
        target.label(),
        move |name, cx| {
            let id = id.clone();
            actions::run(cx, move |api| async move {
                api.rename_playlist_folder(id, name).await
            });
        },
        window,
        cx,
    );
}

pub fn remove_from_folder(target: &Target, cx: &mut App) {
    let Some(id) = playlist_id(target) else {
        return;
    };
    actions::run(
        cx,
        move |api| async move { api.move_playlist(id, None).await },
    );
}

/// `delete_from_disk` erases files; without it only the cached rows go and a re-sync restores them.
pub fn delete_album(target: &Target, cx: &mut App) {
    let Some(id) = album_id(target) else { return };
    if caps(cx).delete_from_disk {
        actions::run(
            cx,
            move |api| async move { api.delete_album(id, true).await },
        );
        return;
    }
    // A server splits one release across same-titled rows; dropping one leaves the title on screen.
    let title = title_key(&target.label());
    let call = Link::global(cx).call(|api| async move { api.albums(store::all()).await });
    cx.spawn(async move |cx| {
        let ids = match call.await {
            Ok(page) => page
                .albums
                .iter()
                .filter(|album| title_key(&album.title) == title)
                .map(|album| album.id.clone())
                .collect(),
            Err(_) => vec![id],
        };
        cx.update(|cx| {
            for id in ids {
                actions::run(
                    cx,
                    move |api| async move { api.delete_album(id, false).await },
                );
            }
        });
    })
    .detach();
}

fn title_key(title: &str) -> String {
    title.trim().to_lowercase()
}

pub fn delete_tracks(target: &Target, cx: &mut App) {
    let Some(scope) = widgets::track_list::row_scope(cx) else {
        tracing::warn!("nothing to delete: the menu did not come from a track row");
        return;
    };
    scope.delete(track_keys(target), cx);
}

pub fn queue_jump(target: &Target, cx: &mut App) {
    let Some(index) = queue_index(target) else {
        return;
    };
    QueueModel::global(cx).update(cx, |queue, cx| queue.jump(index, cx));
}

pub fn queue_move(target: &Target, step: i64, cx: &mut App) {
    let Some(index) = queue_index(target) else {
        return;
    };
    let to = index as i64 + step;
    let count = QueueModel::global(cx).read(cx).len() as i64;
    if to < 0 || to >= count {
        return;
    }
    QueueModel::global(cx).update(cx, |queue, cx| queue.move_item(index, to as u32, cx));
}

pub fn queue_remove(target: &Target, cx: &mut App) {
    let Some(index) = queue_index(target) else {
        return;
    };
    store::actions::queue_remove(index, cx);
}

pub fn seek(step_ms: i64, cx: &mut App) {
    PlayerModel::global(cx).update(cx, |player, cx| {
        let position = player.position_ms() as i64 + step_ms;
        let capped = player
            .duration_ms()
            .map_or(position, |duration| position.min(duration as i64));
        player.seek(capped.max(0) as u64, cx);
    });
}

pub fn nudge_volume(step: f32, cx: &mut App) {
    PlayerModel::global(cx).update(cx, |player, cx| player.nudge_volume(step, cx));
}

pub fn toggle_panel(panel: SidePanel, cx: &mut App) {
    Chrome::global(cx).update(cx, |chrome, cx| chrome.toggle_panel(panel, cx));
}

pub fn toggle_fullscreen(cx: &mut App) {
    Chrome::global(cx).update(cx, |chrome, cx| {
        let open = chrome.is_fullscreen();
        chrome.set_fullscreen(!open, cx);
    });
}

pub fn toggle_mini_player(cx: &mut App) {
    store::toggle_mini_player(cx);
}
