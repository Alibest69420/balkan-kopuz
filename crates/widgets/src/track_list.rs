use std::collections::HashSet;

use api::TrackInfo;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Render, SharedString, StatefulInteractiveElement as _, Styled, UniformListScrollHandle,
    WeakEntity, Window, div, px, uniform_list,
};
use store::{DownloadsModel, FavoritesModel, Navigator, PlayerModel, Route, SourcesModel, actions};

use crate::cover::Cover;
use crate::dialogs;
use crate::format::{duration, t};
use crate::icons;
use crate::playlist_picker;

// Virtualized rows need one resolved height; this is the Dioxus frontend's.
const ROW_HEIGHT: Pixels = px(60.);

/// Which columns a list draws; an album view numbers its rows and hides the album column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackListOptions {
    pub show_index: bool,
    pub show_cover: bool,
    pub show_album: bool,
    pub track_numbers: bool,
    pub selectable: bool,
}

impl Default for TrackListOptions {
    fn default() -> Self {
        Self {
            show_index: true,
            show_cover: true,
            show_album: true,
            track_numbers: false,
            selectable: true,
        }
    }
}

/// Row-menu entries only the owning page can carry out.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TrackActions {
    pub remove_from_playlist: bool,
    pub delete: bool,
    pub reorder: bool,
}

/// What the page has to answer for; everything else the list does itself.
#[derive(Debug, Clone)]
pub enum TrackListEvent {
    /// A row was activated; the page decides what queue that starts.
    Play(usize),
    RemoveFromPlaylist(usize),
    Delete(Vec<String>),
    Reorder {
        from: usize,
        to: usize,
    },
    SelectionChanged,
}

/// Tracks dragged out of a list, for a drop target such as the queue.
#[derive(Debug, Clone)]
pub struct TrackDrag {
    pub keys: Vec<String>,
    pub label: SharedString,
}

/// The shared track list: virtualized rows, selection, and the row menu.
pub struct TrackList {
    rows: Vec<TrackInfo>,
    options: TrackListOptions,
    row_actions: TrackActions,
    selected: HashSet<String>,
    selecting: bool,
    scroll: UniformListScrollHandle,
}

impl EventEmitter<TrackListEvent> for TrackList {}

impl TrackList {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            rows: Vec::new(),
            options: TrackListOptions::default(),
            row_actions: TrackActions::default(),
            selected: HashSet::new(),
            selecting: false,
            scroll: UniformListScrollHandle::new(),
        }
    }

    pub fn with_options(mut self, options: TrackListOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_actions(mut self, actions: TrackActions) -> Self {
        self.row_actions = actions;
        self
    }

    pub fn set_options(&mut self, options: TrackListOptions, cx: &mut Context<Self>) {
        self.options = options;
        cx.notify();
    }

    pub fn set_actions(&mut self, actions: TrackActions, cx: &mut Context<Self>) {
        self.row_actions = actions;
        cx.notify();
    }

    pub fn set_rows(&mut self, rows: Vec<TrackInfo>, cx: &mut Context<Self>) {
        let keys: HashSet<&String> = rows.iter().map(|row| &row.key).collect();
        self.selected.retain(|key| keys.contains(key));
        self.rows = rows;
        cx.notify();
    }

    pub fn rows(&self) -> &[TrackInfo] {
        &self.rows
    }

    pub fn keys(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.key.clone()).collect()
    }

    pub fn selected_keys(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| self.selected.contains(&row.key))
            .map(|row| row.key.clone())
            .collect()
    }

    pub fn is_selecting(&self) -> bool {
        self.selecting
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected.clear();
        self.selecting = false;
        cx.notify();
        cx.emit(TrackListEvent::SelectionChanged);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = self.rows.iter().map(|row| row.key.clone()).collect();
        self.selecting = true;
        cx.notify();
        cx.emit(TrackListEvent::SelectionChanged);
    }

    pub fn scroll_to(&self, index: usize) {
        self.scroll
            .scroll_to_item(index, gpui_kit::ScrollStrategy::Center);
    }

    fn toggle_selected(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
        self.selecting = !self.selected.is_empty();
        cx.notify();
        cx.emit(TrackListEvent::SelectionChanged);
    }

    fn row(&self, index: usize, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(track) = self.rows.get(index) else {
            return div().into_any_element();
        };
        let key = track.key.clone();
        let playing = PlayerModel::global(cx)
            .read(cx)
            .track()
            .is_some_and(|now| now.key == key);
        let selected = self.selected.contains(&key);
        let favorite = FavoritesModel::global(cx).read(cx).is_favorite(&key);
        let stored = DownloadsModel::global(cx).read(cx).is_stored(&key);
        let lead = if self.options.track_numbers {
            track.track_number.unwrap_or(index as u32 + 1).to_string()
        } else {
            (index + 1).to_string()
        };
        let artist = SharedString::from(track.artist.clone());
        let album = SharedString::from(track.album.clone());
        let album_id = track.album_id.clone();
        let title = SharedString::from(track.title.clone());
        let length = track
            .duration_secs()
            .map(|secs| duration(secs * 1000))
            .unwrap_or_else(|| "--:--".into());

        div()
            .id(("track-row", index))
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .rounded(cx.theme().radius)
            .when(selected, |row| row.bg(cx.theme().list_active))
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.selecting {
                    this.toggle_selected(key.clone(), cx);
                } else {
                    cx.emit(TrackListEvent::Play(index));
                }
            }))
            .child(
                div()
                    .w(px(28.))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(if playing {
                        cx.theme().primary
                    } else {
                        cx.theme().muted_foreground
                    })
                    .map(|cell| {
                        if self.options.selectable && self.selecting {
                            cell.child(Checkbox::new(("select", index)).checked(selected).on_click(
                                cx.listener({
                                    let key = track.key.clone();
                                    move |this, _: &bool, _, cx| {
                                        this.toggle_selected(key.clone(), cx)
                                    }
                                }),
                            ))
                        } else if self.options.show_index {
                            cell.child(lead)
                        } else {
                            cell
                        }
                    }),
            )
            .when(self.options.show_cover, |row| {
                row.child(Cover::new(track.artwork.as_ref()).size(px(40.)))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .when(playing, |title| title.text_color(cx.theme().primary))
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(artist.clone()),
                    ),
            )
            .when(self.options.show_album, |row| {
                row.child(
                    div()
                        .id(("album", index))
                        .w(px(200.))
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .hover(|cell| cell.text_color(cx.theme().foreground))
                        .on_click({
                            let album_id = album_id.clone();
                            move |_, _, cx| {
                                if !album_id.is_empty() {
                                    Navigator::navigate(Route::Album(album_id.clone().into()), cx);
                                }
                            }
                        })
                        .child(album.clone()),
                )
            })
            .when(favorite, |row| {
                row.child(crate::icons::icon(icons::HEART).text_color(cx.theme().primary))
            })
            .when(stored, |row| {
                row.child(
                    crate::icons::icon(icons::DOWNLOAD).text_color(cx.theme().muted_foreground),
                )
            })
            .child(
                div()
                    .w(px(56.))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(length),
            )
            .context_menu({
                let track = track.clone();
                let actions = self.row_actions;
                let list = cx.entity().downgrade();
                move |menu, window, cx| {
                    row_menu(menu, &track, index, actions, list.clone(), window, cx)
                }
            })
            .into_any_element()
    }
}

fn row_menu(
    menu: PopupMenu,
    track: &TrackInfo,
    index: usize,
    row_actions: TrackActions,
    list: WeakEntity<TrackList>,
    _: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let capabilities = SourcesModel::global(cx).read(cx).capabilities();
    let favorite = FavoritesModel::global(cx).read(cx).is_favorite(&track.key);
    let stored = DownloadsModel::global(cx).read(cx).is_stored(&track.key);
    let key = track.key.clone();
    let artist = track.artist.clone();
    let album_id = track.album_id.clone();

    let mut menu = menu
        .item(
            gpui_kit::component::menu::PopupMenuItem::new(t("play_next"))
                .icon(crate::icons::icon(icons::QUEUE_NEXT))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::play_next(vec![key.clone()], cx)
                }),
        )
        .item(
            gpui_kit::component::menu::PopupMenuItem::new(t("add_to_queue"))
                .icon(crate::icons::icon(icons::QUEUE_ADD))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::append(vec![key.clone()], cx)
                }),
        )
        .item(
            gpui_kit::component::menu::PopupMenuItem::new(t("add_to_playlist"))
                .icon(crate::icons::icon(icons::PLUS))
                .on_click({
                    let key = key.clone();
                    move |_, window, cx| playlist_picker::open(vec![key.clone()], window, cx)
                }),
        )
        .item(
            gpui_kit::component::menu::PopupMenuItem::new(if favorite {
                t("remove_from_favorites")
            } else {
                t("add_to_favorites")
            })
            .icon(crate::icons::icon(icons::HEART))
            .on_click({
                let key = key.clone();
                move |_, _, cx| {
                    FavoritesModel::global(cx)
                        .update(cx, |favorites, cx| favorites.toggle(key.clone(), cx))
                }
            }),
        );

    if capabilities.track_radio {
        menu = menu.item(
            gpui_kit::component::menu::PopupMenuItem::new(t("start_radio"))
                .icon(crate::icons::icon(icons::RADIO))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::start_track_radio(key.clone(), cx)
                }),
        );
    }
    if capabilities.downloads {
        menu = menu.item(
            gpui_kit::component::menu::PopupMenuItem::new(if stored {
                t("remove_download")
            } else {
                t("download_offline")
            })
            .icon(crate::icons::icon(icons::DOWNLOAD))
            .on_click({
                let key = key.clone();
                move |_, _, cx| {
                    DownloadsModel::global(cx).update(cx, |downloads, cx| {
                        if stored {
                            downloads.remove(key.clone(), cx);
                        } else {
                            downloads.download(vec![key.clone()], cx);
                        }
                    })
                }
            }),
        );
    }

    menu = menu.separator().item(
        gpui_kit::component::menu::PopupMenuItem::new(t("go_to_artist"))
            .icon(crate::icons::icon(icons::MIC))
            .on_click(move |_, _, cx| {
                Navigator::navigate(
                    Route::Artist {
                        name: artist.clone().into(),
                        channel: None,
                    },
                    cx,
                )
            }),
    );
    if !album_id.is_empty() {
        menu = menu.item(
            gpui_kit::component::menu::PopupMenuItem::new(t("go_to_album"))
                .icon(crate::icons::icon(icons::DISC))
                .on_click(move |_, _, cx| {
                    Navigator::navigate(Route::Album(album_id.clone().into()), cx)
                }),
        );
    }
    menu = menu.item(
        gpui_kit::component::menu::PopupMenuItem::new(t("share"))
            .icon(crate::icons::icon(icons::SHARE))
            .on_click({
                let key = key.clone();
                move |_, _, cx| actions::share_track(key.clone(), cx)
            }),
    );
    menu = menu.item(
        gpui_kit::component::menu::PopupMenuItem::new(t("view_metadata"))
            .icon(crate::icons::icon(icons::EDIT))
            .on_click({
                let key = key.clone();
                let editable = capabilities.edit_tags;
                move |_, window, cx| dialogs::open_track_metadata(key.clone(), editable, window, cx)
            }),
    );

    if row_actions.remove_from_playlist {
        menu = menu.separator().item(
            gpui_kit::component::menu::PopupMenuItem::new(t("remove_from_playlist"))
                .icon(crate::icons::icon(icons::CLOSE))
                .on_click({
                    let list = list.clone();
                    move |_, _, cx| {
                        list.update(cx, |_, cx| {
                            cx.emit(TrackListEvent::RemoveFromPlaylist(index))
                        })
                        .ok();
                    }
                }),
        );
    }
    if row_actions.delete {
        menu = menu.item(
            gpui_kit::component::menu::PopupMenuItem::new(t("delete"))
                .icon(crate::icons::icon(icons::TRASH))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| {
                        list.update(cx, |_, cx| {
                            cx.emit(TrackListEvent::Delete(vec![key.clone()]))
                        })
                        .ok();
                    }
                }),
        );
    }
    menu
}

impl Render for TrackList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();
        v_flex().size_full().child(
            uniform_list(
                "track-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| this.row(index, cx).into_any_element())
                        .collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .size_full(),
        )
    }
}
