//! The level control, wired to the player: the paint is `components::VolumeControl`.

use components::VolumeControl;
use components::volume_control::to_wire;
use gpui_kit::Context;
use store::{ConfigModel, PlayerModel};

use crate::NowPlaying;

impl NowPlaying {
    /// The mute glyph and the level beside it, sharing one wheel.
    pub(crate) fn volume_control(&self, slider: bool, cx: &Context<Self>) -> VolumeControl {
        let level = PlayerModel::global(cx).read(cx).volume();
        let step = ConfigModel::global(cx)
            .read(cx)
            .config()
            .volume_scroll_step
            .max(0.);
        let state = self.volume.clone();
        VolumeControl::new("volume", &self.volume)
            .level(level)
            .slider(slider)
            .wide(self.presentation == crate::Presentation::Sheet)
            .held(self.volume_held)
            .scroll_step(step)
            .on_mute(|_, cx| {
                PlayerModel::global(cx).update(cx, |player, cx| player.toggle_mute(cx))
            })
            .on_step(move |next, window, cx| {
                state.update(cx, |state, cx| state.set_value(next, window, cx));
                PlayerModel::global(cx)
                    .update(cx, |player, cx| player.set_volume_saved(to_wire(next), cx));
            })
    }
}
