//! How the player behaves: what Back does partway through a song, and whether played songs are kept for offline.

use components::format::{self, t, t_with};
use components::settings::Toggle;
use components::{Choice, SettingRow};
use config::{BackBehavior, LibraryEdit, PlaybackEdit, SettingsEdit};
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, App, Context, IntoElement, ParentElement as _, Styled};
use store::ConfigModel;

use crate::FirstRun;

/// Keeping played songs, said with the cap they are kept up to.
fn offline_cache(settings: &config::Settings, cx: &App) -> SettingRow {
    let size = format::bytes(settings.library.offline_cache_max_bytes);
    SettingRow::new(t_with(
        "onboarding_offline_cache",
        &[("size", size.to_string().into())],
    ))
    .setting(
        Toggle::new("onboarding-offline_cache_enabled")
            .enabled(settings.library.offline_cache_enabled)
            .on_change(|on, _, cx| {
                ConfigModel::edit(
                    cx,
                    SettingsEdit::Library(LibraryEdit::OfflineCacheEnabled(*on)),
                )
            }),
    )
    .bind("offline_cache_enabled", cx)
}

impl FirstRun {
    pub(crate) fn playback_step(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = ConfigModel::global(cx).read(cx).config().clone();
        let back = Choice::new(
            "onboarding-back_behavior",
            settings.playback.back_behavior,
            vec![
                (BackBehavior::RewindThenPrev, t("back_behavior_rewind")),
                (BackBehavior::AlwaysPrev, t("back_behavior_always_prev")),
            ],
        )
        .on_change(ConfigModel::writer(|value| {
            SettingsEdit::Playback(PlaybackEdit::BackBehavior(value))
        }));
        v_flex()
            .w_full()
            .child(
                crate::ui::card(cx)
                    .child(
                        SettingRow::new(t("back_behavior"))
                            .description(t("back_behavior_description"))
                            .setting(back)
                            .bind("back_behavior", cx),
                    )
                    .child(offline_cache(&settings, cx)),
            )
            .into_any_element()
    }
}
