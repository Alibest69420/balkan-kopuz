//! Settings → Interface: what the bar keeps in reach, and how the sheet is dressed.

use components::format::t;
use components::settings::{SettingRow, Toggle, choice, section};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled, Window,
};
use store::LayoutModel;
use store::layout::{Backdrop, Section as Group, SheetTab, Side, TileSize};

/// Contribute both sections; call once at startup, after `settings_interface::init`.
pub fn register(cx: &mut App) {
    settings_interface::register(
        "player",
        "interface_player",
        20,
        Some(Group::Player),
        |window, cx| cx.new(|cx| PlayerSettings::new(window, cx)).into(),
        cx,
    );
    settings_interface::register(
        "sheet",
        "interface_sheet",
        21,
        Some(Group::Sheet),
        |window, cx| cx.new(|cx| SheetSettings::new(window, cx)).into(),
        cx,
    );
}

struct PlayerSettings;

impl PlayerSettings {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for PlayerSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = store::layout(cx).player.clone();
        v_flex()
            .w_full()
            .child(
                section(t("interface_player"))
                    .children(
                        SettingRow::new(t("interface_player_cover"))
                            .description(t("interface_player_cover_description"))
                            .control(choice(
                                "player-cover",
                                &player.cover,
                                vec![
                                    (TileSize::Small, t("interface_player_cover_small")),
                                    (TileSize::Medium, t("interface_player_cover_medium")),
                                    (TileSize::Large, t("interface_player_cover_large")),
                                ],
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.player.cover = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(toggle_row(
                        "player-shuffle",
                        "interface_player_shuffle",
                        player.shuffle,
                        |layout, on| layout.player.shuffle = on,
                        cx,
                    ))
                    .children(toggle_row(
                        "player-repeat",
                        "interface_player_repeat",
                        player.repeat,
                        |layout, on| layout.player.repeat = on,
                        cx,
                    ))
                    .children(toggle_row(
                        "player-volume",
                        "interface_player_volume",
                        player.volume,
                        |layout, on| layout.player.volume = on,
                        cx,
                    ))
                    .children(toggle_row(
                        "player-queue",
                        "interface_player_queue",
                        player.queue,
                        |layout, on| layout.player.queue = on,
                        cx,
                    ))
                    .children(toggle_row(
                        "player-lyrics",
                        "interface_player_lyrics",
                        player.lyrics,
                        |layout, on| layout.player.lyrics = on,
                        cx,
                    ))
                    .children(toggle_row(
                        "player-favorite",
                        "interface_player_favorite",
                        player.favorite,
                        |layout, on| layout.player.favorite = on,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

struct SheetSettings;

impl SheetSettings {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SheetSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet = store::layout(cx).sheet.clone();
        v_flex()
            .w_full()
            .child(
                section(t("interface_sheet"))
                    .children(
                        SettingRow::new(t("interface_sheet_tab"))
                            .description(t("interface_sheet_tab_description"))
                            .control(choice(
                                "sheet-tab",
                                &sheet.tab,
                                vec![
                                    (SheetTab::Queue, t("up_next")),
                                    (SheetTab::Lyrics, t("lyrics")),
                                    (SheetTab::Related, t("related")),
                                ],
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.sheet.tab = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("interface_sheet_side"))
                            .description(t("interface_sheet_side_description"))
                            .control(choice(
                                "sheet-side",
                                &sheet.side,
                                vec![
                                    (Side::Right, t("interface_sheet_side_right")),
                                    (Side::Left, t("interface_sheet_side_left")),
                                ],
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.sheet.side = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("interface_sheet_backdrop"))
                            .description(t("interface_sheet_backdrop_description"))
                            .control(choice(
                                "sheet-backdrop",
                                &sheet.backdrop,
                                vec![
                                    (Backdrop::Artwork, t("interface_sheet_backdrop_artwork")),
                                    (Backdrop::Palette, t("interface_sheet_backdrop_palette")),
                                    (Backdrop::Solid, t("interface_sheet_backdrop_solid")),
                                ],
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.sheet.backdrop = value)
                                },
                            ))
                            .shown(cx),
                    ),
            )
            .into_any_element()
    }
}

/// One of the bar's optional controls, as a row that hides itself when the page is filtered.
fn toggle_row(
    id: &'static str,
    title: &'static str,
    on: bool,
    set: impl Fn(&mut store::Layout, bool) + 'static,
    cx: &App,
) -> Option<SettingRow> {
    SettingRow::new(t(title))
        .control(Toggle::new(id).enabled(on).on_change(move |on, _, cx| {
            let on = *on;
            LayoutModel::edit(cx, |layout| set(layout, on));
        }))
        .shown(cx)
}
