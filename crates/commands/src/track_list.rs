//! The verbs a track list asks its owner for, answered once for every page that draws one.

use components::track_list::{TrackList, TrackListEvent};
use gpui_kit::{App, Entity};
use store::{ConfigModel, PlayerModel, Target};

use crate::ops;

/// Wire a list to the player, the config and the queue/favorite verbs; the wiring lives as long as the list.
pub fn bind(list: &Entity<TrackList>, cx: &mut App) {
    let player = PlayerModel::global(cx);
    let config = ConfigModel::global(cx);
    let playing = playing_key(&player, cx);
    let row_images = config.read(cx).row_images();
    list.update(cx, |list, cx| {
        list.set_playing(playing, cx);
        list.set_row_images(row_images, cx);
    });
    let weak = list.downgrade();
    let on_player = cx.observe(&player, {
        let weak = weak.clone();
        move |player, cx| {
            let playing = playing_key(&player, cx);
            weak.update(cx, |list, cx| list.set_playing(playing, cx))
                .ok();
        }
    });
    let on_config = cx.observe(&config, move |config, cx| {
        let row_images = config.read(cx).row_images();
        weak.update(cx, |list, cx| list.set_row_images(row_images, cx))
            .ok();
    });
    let on_event = cx.subscribe(list, |_, event: &TrackListEvent, cx| perform(event, cx));
    list.update(cx, |list, _| list.hold([on_player, on_config, on_event]));
}

fn playing_key(player: &Entity<PlayerModel>, cx: &App) -> Option<String> {
    player.read(cx).track().map(|track| track.key.clone())
}

fn perform(event: &TrackListEvent, cx: &mut App) {
    match event {
        TrackListEvent::PlayNext(keys) => store::actions::play_next(keys.clone(), cx),
        TrackListEvent::Queue(keys) => store::actions::append(keys.clone(), cx),
        TrackListEvent::ToggleFavorite(keys) => {
            let keys = keys.iter().cloned().map(Into::into).collect();
            ops::toggle_favorite(&Target::tracks(keys, ""), cx);
        }
        _ => {}
    }
}
