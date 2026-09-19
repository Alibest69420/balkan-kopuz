//! The album commands a card menu or a detail button issues.

use std::path::Path;

use api::{ArtworkTarget, ArtworkUpload, TrackInfo};
use gpui_kit::{App, ClipboardItem, PathPromptOptions, Window};
use link::Link;
use store::{Notices, actions};
use widgets::playlist_picker;

/// Album order: by disc, then by track number.
pub fn in_album_order(tracks: &mut [TrackInfo]) {
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

pub fn keys_of(tracks: &[TrackInfo]) -> Vec<String> {
    tracks
        .iter()
        .filter(|track| !track.key.is_empty())
        .map(|track| track.key.clone())
        .collect()
}

/// Read one album's tracks, then hand their keys in album order to `then`.
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
            cx.update(|cx| Notices::error(error.to_string(), cx));
        }
    })
    .detach();
}

/// Open the playlist picker for a whole album; the picker needs the window the click came from.
pub fn add_album_to_playlist(id: String, window: &mut Window, cx: &mut App) {
    let call =
        Link::global(cx).call(move |api| async move { api.album_tracks(id, store::all()).await });
    window
        .spawn(cx, async move |cx| match call.await {
            Ok(page) => {
                let mut tracks = page.items;
                in_album_order(&mut tracks);
                let keys = keys_of(&tracks);
                cx.update(|window, cx| playlist_picker::open(keys, window, cx))
                    .ok();
            }
            Err(error) => {
                cx.update(|_, cx| Notices::error(error.to_string(), cx))
                    .ok();
            }
        })
        .detach();
}

/// `from_disk` deletes the files; without it the album's cached rows are dropped and a re-sync restores them.
pub fn delete_album(id: String, from_disk: bool, cx: &mut App) {
    actions::run(cx, move |api| async move {
        api.delete_album(id, from_disk).await
    });
}

pub fn delete_tracks(keys: Vec<String>, cx: &mut App) {
    actions::run(
        cx,
        move |api| async move { api.delete_tracks(keys, true).await },
    );
}

/// Pick a picture and store it as the album's cover.
pub fn pick_album_cover(id: String, cx: &mut App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = picked.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        let read = cx.background_executor().spawn(async move {
            let content_type = content_type_for(&path);
            std::fs::read(&path).map(|bytes| (content_type, bytes))
        });
        match read.await {
            Ok((content_type, bytes)) => {
                cx.update(|cx| {
                    actions::run(cx, move |api| async move {
                        api.upload_artwork(ArtworkUpload {
                            target: ArtworkTarget::Album(id),
                            content_type,
                            bytes,
                        })
                        .await
                    })
                });
            }
            Err(error) => {
                cx.update(|cx| Notices::error(error.to_string(), cx));
            }
        }
    })
    .detach();
}

pub fn remove_album_cover(id: String, cx: &mut App) {
    actions::run(cx, move |api| async move {
        api.remove_artwork(ArtworkTarget::Album(id)).await
    });
}

/// The album's public page, falling back to its first track's; the daemon knows which sources have one.
pub fn share_album(browse_id: Option<String>, first_key: Option<String>, cx: &mut App) {
    let call = Link::global(cx).call(move |api| async move {
        if let Some(id) = browse_id
            && let Some(url) = api.album_web_url(id).await?
        {
            return Ok(Some(url));
        }
        match first_key {
            Some(key) => api.track_web_url(key).await,
            None => Ok(None),
        }
    });
    cx.spawn(async move |cx| {
        let url = call.await;
        cx.update(|cx| match url {
            Ok(Some(url)) => {
                cx.write_to_clipboard(ClipboardItem::new_string(url));
                // TODO(i18n): no en.ftl key for a generic share result yet.
                Notices::info("Link copied", cx);
            }
            Ok(None) => Notices::error("No public page for this album", cx),
            Err(error) => Notices::error(error.to_string(), cx),
        });
    })
    .detach();
}

fn content_type_for(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
    .to_string()
}
