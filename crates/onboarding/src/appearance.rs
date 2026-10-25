//! How Kopuz looks: its theme, chosen from examples drawn in each theme's own colours, and the cover-art background.

use components::SettingRow;
use components::format::t;
use config::{BackgroundEdit, SettingsEdit};
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled};
use store::ConfigModel;

use crate::FirstRun;
use crate::ui::toggle;

impl FirstRun {
    pub(crate) fn appearance_step(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = ConfigModel::global(cx).read(cx).config().clone();
        v_flex()
            .w_full()
            .child(
                crate::ui::card(cx)
                    .child(
                        SettingRow::new(t("appearance"))
                            .description(t("theme_description"))
                            .stacked()
                            .setting(settings_appearance::theme_gallery(
                                "onboarding-theme",
                                &settings,
                            ))
                            .bind("theme", cx),
                    )
                    .child(toggle(
                        "cover_art_background",
                        "cover_art_background",
                        "cover_art_background_description",
                        settings.background.cover_art_background,
                        |on| SettingsEdit::Background(BackgroundEdit::CoverArtBackground(on)),
                        cx,
                    )),
            )
            .into_any_element()
    }
}
