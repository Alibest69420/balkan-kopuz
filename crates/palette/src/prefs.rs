//! Settings → Interface: what the palette remembers and what it puts first.

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window,
};
use store::LayoutModel;
use store::layout::Section as Group;
use widgets::format::t;
use widgets::settings::{SettingRow, Toggle, choice, section};

/// How many recents the list may hold, as the setting offers them.
const KEEP: &[usize] = &[3, 5, 10, 20];

/// Contribute the palette's own section; the app calls this once at startup.
pub fn register(cx: &mut App) {
    settings_interface::register(
        "palette",
        "interface_palette",
        10,
        Some(Group::Palette),
        |_, cx| cx.new(|_| Prefs).into(),
        cx,
    );
}

struct Prefs;

impl Render for Prefs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let held = store::layout(cx).palette.clone();
        let keep = held.recents;
        v_flex()
            .w_full()
            .child(
                section(t("interface_palette"))
                    .children(
                        SettingRow::new(t("palette_history"))
                            .description(t("palette_history_description"))
                            .control(
                                Toggle::new("palette-history")
                                    .enabled(held.history)
                                    .on_change(|value, _, cx| {
                                        LayoutModel::edit(cx, |layout| {
                                            layout.palette.history = *value;
                                            if !*value {
                                                layout.palette.recent_commands.clear();
                                                layout.palette.recent_searches.clear();
                                            }
                                        });
                                    }),
                            )
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("palette_recents"))
                            .description(t("palette_recents_description"))
                            .control(choice(
                                "palette-recents",
                                &keep,
                                KEEP.iter().map(|kept| (*kept, count(*kept))).collect(),
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.palette.recents = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("palette_current_track_first"))
                            .description(t("palette_current_track_first_description"))
                            .control(
                                Toggle::new("palette-current-track-first")
                                    .enabled(held.current_track_first)
                                    .on_change(|value, _, cx| {
                                        LayoutModel::edit(cx, |layout| {
                                            layout.palette.current_track_first = *value
                                        });
                                    }),
                            )
                            .shown(cx),
                    ),
            )
            .into_any_element()
    }
}

fn count(kept: usize) -> SharedString {
    widgets::format::t_with("palette_recents_count", &[("count", kept.to_string())])
}
