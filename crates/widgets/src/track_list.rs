use std::collections::HashSet;
use std::rc::Rc;

use api::TrackInfo;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, EventEmitter, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled, UniformListScrollHandle,
    WeakEntity, Window, div, px, uniform_list,
};
use store::{DownloadsModel, FavoritesModel, Navigator, PlayerModel, Route, SourcesModel, actions};

use crate::cover::Cover;
use crate::dialogs;
use crate::format::{duration, t};
use crate::icons;
use crate::motion;
use crate::playlist_picker;
use crate::style;

// The row is the hover group, so the number can step aside for the play glyph.
const ROW_GROUP: &str = "track-row";

/// The desktop row's column template, so a page's own column header lines up with the rows.
///
/// `20px | minmax(200px, 2fr) | minmax(150px, 1fr) | minmax(150px, 1fr) | 64px | 40px`,
/// laid out on flex because GPUI has no grid.
pub mod columns {
    use gpui_kit::{Div, Pixels, Styled as _, div, px};

    /// The index column: the number, the play glyph, or the selection box.
    pub const INDEX: Pixels = px(20.);
    /// `w-10 h-10`: the cover inside the title column, not a column of its own.
    pub const COVER: Pixels = px(40.);
    /// The duration column.
    pub const DURATION: Pixels = px(64.);
    /// The row-menu column, which the rows keep clear for a page's own control.
    pub const MENU: Pixels = px(40.);
    /// `minmax(200px, 2fr)`: what the title column never goes below.
    pub const TITLE_MIN: Pixels = px(200.);
    /// `minmax(150px, 1fr)`: what the artist and album columns never go below.
    pub const TEXT_MIN: Pixels = px(150.);

    /// The frame a row and a header share: `items-center p-2` with the `1.5rem` column gap.
    pub fn frame() -> Div {
        div().flex().w_full().items_center().p_2().gap_6()
    }

    /// A fixed column: the index, the duration, the menu.
    pub fn fixed(width: Pixels) -> Div {
        div().flex().items_center().w(width).flex_shrink_0()
    }

    /// A `minmax(min, Nfr)` column, sharing the free space `grow` ways.
    pub fn flexible(min: Pixels, grow: f32) -> Div {
        div()
            .flex()
            .items_center()
            .flex_grow(grow)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_w(min)
    }
}

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
    // A hidden trigger would take its own open menu down with it, so the row it belongs to shows it.
    menu_open: Option<String>,
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
            menu_open: None,
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
        let was_selecting = self.selecting;
        self.selected.retain(|key| keys.contains(key));
        // Re-sorting drops every key, and selection mode outliving the selection leaves a
        // "0 selected" pill over the list while clicks tick checkboxes instead of playing.
        self.selecting = !self.selected.is_empty();
        self.rows = rows;
        cx.notify();
        if was_selecting != self.selecting {
            cx.emit(TrackListEvent::SelectionChanged);
        }
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

    fn row(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let Some(track) = self.rows.get(index) else {
            return div().into_any_element();
        };
        let key = track.key.clone();
        let playing = PlayerModel::global(cx)
            .read(cx)
            .track()
            .is_some_and(|now| now.key == key);
        let selected = self.selected.contains(&key);
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

        let accent = cx.theme().primary;
        let mark = px(12.);
        let cover = style::to_px(style::row_cover(), window);
        let menu_open = self.menu_open.as_deref() == Some(key.as_str());
        // `transition-colors` on the row: the playing tint and the selection wash tween, hover does not.
        let wash = motion::color(
            motion::id("row-wash", &key),
            if playing {
                style::accent(style::PLAYING, cx)
            } else if selected {
                style::white(style::SELECTED, cx)
            } else {
                style::white(0., cx)
            },
            window,
            cx,
        );
        let title_color = motion::color(
            motion::id("row-title", &key),
            if playing {
                accent
            } else {
                style::white(style::TEXT_STRONG, cx)
            },
            window,
            cx,
        );
        let badge = motion::amount(
            motion::id("row-badge", &key),
            if stored && !playing { 1. } else { 0. },
            motion::FAST,
            window,
            cx,
        );
        // One builder, so the dots button and the right-click menu cannot drift apart.
        let menu = {
            let track = track.clone();
            let actions = self.row_actions;
            let list = cx.entity().downgrade();
            Rc::new(
                move |popup, window: &mut Window, cx: &mut Context<PopupMenu>| {
                    row_menu(popup, &track, index, actions, list.clone(), window, cx)
                },
            )
        };

        columns::frame()
            .id(("track-row", index))
            .group(ROW_GROUP)
            .h(style::row_height())
            .rounded(style::radius_lg())
            .bg(wash)
            // Upstream the wash is an inline style, which outranks `hover:bg-white/5`; a refinement does not.
            .when(!playing && !selected, |row| {
                row.hover(|row| row.bg(style::white(style::RAISED, cx)))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.selecting {
                    this.toggle_selected(key.clone(), cx);
                } else {
                    cx.emit(TrackListEvent::Play(index));
                }
            }))
            .child(
                columns::fixed(columns::INDEX)
                    .relative()
                    .justify_center()
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
                        } else if playing {
                            cell.child(icons::icon(icons::VOLUME).size(mark).text_color(accent))
                        } else if self.options.show_index {
                            cell.child(
                                div()
                                    .text_xs()
                                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                                    .group_hover(ROW_GROUP, |number| number.invisible())
                                    .child(lead),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .invisible()
                                    .group_hover(ROW_GROUP, |play| play.visible())
                                    .child(
                                        icons::icon(icons::PLAY)
                                            .size(mark)
                                            .text_color(style::white(style::TEXT_SECONDARY, cx)),
                                    ),
                            )
                        } else {
                            cell
                        }
                    }),
            )
            .child(
                columns::flexible(columns::TITLE_MIN, 2.)
                    .when(self.options.show_cover, |cell| {
                        cell.child(
                            div()
                                .relative()
                                .flex_shrink_0()
                                .mr_4()
                                .child(Cover::new(track.artwork.as_ref()).size(cover))
                                .when(badge > 0., |thumb| {
                                    thumb.child(
                                        div()
                                            .absolute()
                                            .bottom_0()
                                            .right_0()
                                            .size_3()
                                            .opacity(badge)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_tl(style::radius(cx))
                                            .bg(accent)
                                            .child(
                                                icons::icon(icons::CHECK)
                                                    .size(px(6.))
                                                    .text_color(cx.theme().foreground),
                                            ),
                                    )
                                }),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(title_color)
                            .child(title.clone()),
                    ),
            )
            .child(
                columns::flexible(columns::TEXT_MIN, 1.)
                    .id(("artist", index))
                    .text_sm()
                    .text_color(style::slate_500(cx))
                    .hover(|cell| cell.text_color(style::slate_400(cx)))
                    .on_click({
                        let artist = artist.clone();
                        move |_, _, cx| {
                            // Without this the row's own click would play the track too.
                            cx.stop_propagation();
                            Navigator::navigate(
                                Route::Artist {
                                    name: artist.clone(),
                                    channel: None,
                                },
                                cx,
                            )
                        }
                    })
                    .child(div().flex_1().min_w_0().truncate().child(artist.clone())),
            )
            .when(self.options.show_album, |row| {
                row.child(
                    columns::flexible(columns::TEXT_MIN, 1.)
                        .id(("album", index))
                        .text_sm()
                        .text_color(style::slate_500(cx))
                        .hover(|cell| cell.text_color(style::slate_400(cx)))
                        .on_click({
                            let album_id = album_id.clone();
                            move |_, _, cx| {
                                if !album_id.is_empty() {
                                    cx.stop_propagation();
                                    Navigator::navigate(Route::Album(album_id.clone().into()), cx);
                                }
                            }
                        })
                        .child(div().flex_1().min_w_0().truncate().child(album.clone())),
                )
            })
            .child(
                columns::fixed(columns::DURATION)
                    .justify_end()
                    .text_xs()
                    .text_color(style::white(style::TEXT_DIM, cx))
                    .child(length),
            )
            .child(
                columns::fixed(columns::MENU)
                    .id(("row-menu", index))
                    .justify_end()
                    // The popover stops the press; this stops the rest of the cell playing the row.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .when(!menu_open, |cell| {
                        cell.invisible()
                            .group_hover(ROW_GROUP, |dots| dots.visible())
                    })
                    .child(
                        Button::new(("row-menu-button", index))
                            .ghost()
                            .xsmall()
                            .icon(icons::icon(icons::MORE))
                            .dropdown_menu({
                                let menu = menu.clone();
                                move |popup, window, cx| menu(popup, window, cx)
                            })
                            .on_open_change({
                                let list = cx.entity().downgrade();
                                let key = track.key.clone();
                                move |open, _, cx| {
                                    let key = open.then(|| key.clone());
                                    list.update(cx, |this, cx| {
                                        this.menu_open = key;
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            }),
                    ),
            )
            .context_menu(move |popup, window, cx| menu(popup, window, cx))
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
            gpui_kit::component::menu::PopupMenuItem::element({
                let label = t("delete");
                move |_, cx| {
                    // Taking a file off disk should not look like the item above it.
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_color(cx.theme().danger)
                        .child(crate::icons::icon(icons::TRASH))
                        .child(label.clone())
                        .into_any_element()
                }
            })
            .on_click({
                let key = key.clone();
                let title = SharedString::from(track.title.clone());
                move |_, window, cx| {
                    let key = key.clone();
                    let list = list.clone();
                    crate::dialogs::confirm_delete(
                        t("delete"),
                        title.clone(),
                        move |_, cx| {
                            list.update(cx, |_, cx| {
                                cx.emit(TrackListEvent::Delete(vec![key.clone()]))
                            })
                            .ok();
                        },
                        window,
                        cx,
                    );
                }
            }),
        );
    }
    menu
}

impl Render for TrackList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();
        crate::scroll::smooth_list("track-list", &self.scroll, window, cx);
        v_flex().size_full().child(
            uniform_list(
                "track-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                    range
                        .map(|index| this.row(index, window, cx).into_any_element())
                        .collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .size_full(),
        )
    }
}
