use std::collections::HashSet;
use std::rc::Rc;

use api::TrackInfo;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    Global, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, UniformListScrollHandle, WeakEntity, Window, actions,
    div, px, uniform_list,
};
use store::{DownloadsModel, FavoritesModel, Navigator, PlayerModel, Route, Target};

use crate::cover::Cover;
use crate::format::{duration, t};
use crate::icons;
use crate::menus;
use crate::motion;
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

/// What follows the pointer during the drag: a chip naming what is being carried.
impl Render for TrackDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_sm()
            .child(self.label.clone())
    }
}

/// The row a menu is being built for, so a list-scoped command can emit back through the list.
#[derive(Clone)]
pub struct RowScope {
    pub list: WeakEntity<TrackList>,
    pub index: usize,
    pub rows: usize,
    pub actions: TrackActions,
}

#[derive(Default)]
struct OpenRow(Option<RowScope>);

impl Global for OpenRow {}

fn set_row_scope(scope: RowScope, cx: &mut App) {
    cx.set_global(OpenRow(Some(scope)));
}

/// The row whose menu was built last; `None` when the menu belongs to something that is not a row.
pub fn row_scope(cx: &App) -> Option<RowScope> {
    cx.try_global::<OpenRow>().and_then(|open| open.0.clone())
}

/// Forget the row scope, so a menu opened elsewhere cannot inherit the last row's.
pub fn clear_row_scope(cx: &mut App) {
    cx.set_global(OpenRow(None));
}

impl RowScope {
    pub fn reorder(&self, to: usize, cx: &mut App) {
        let from = self.index;
        self.list
            .update(cx, |_, cx| cx.emit(TrackListEvent::Reorder { from, to }))
            .ok();
    }

    pub fn remove_from_playlist(&self, cx: &mut App) {
        let index = self.index;
        self.list
            .update(cx, |_, cx| {
                cx.emit(TrackListEvent::RemoveFromPlaylist(index))
            })
            .ok();
    }

    pub fn delete(&self, keys: Vec<String>, cx: &mut App) {
        self.list
            .update(cx, |_, cx| cx.emit(TrackListEvent::Delete(keys)))
            .ok();
    }
}

/// The shared track list: virtualized rows, selection, and the row menu.
pub struct TrackList {
    rows: Vec<TrackInfo>,
    options: TrackListOptions,
    row_actions: TrackActions,
    selected: HashSet<String>,
    selecting: bool,
    // Where a shift-click extends from: the row picked last.
    anchor: Option<usize>,
    // The row the arrow keys are on, drawn with a ring so it can be seen.
    cursor: Option<usize>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    // A hidden trigger would take its own open menu down with it, so the row it belongs to shows it.
    menu_open: Option<String>,
}

actions!(
    track_list,
    [
        CursorUp,
        CursorDown,
        ActivateRow,
        SelectAll,
        ClearSelection,
        PlayNextSelection,
        QueueSelection,
        FavoriteSelection,
        RemoveSelection
    ]
);

impl EventEmitter<TrackListEvent> for TrackList {}

impl Focusable for TrackList {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TrackList {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            rows: Vec::new(),
            options: TrackListOptions::default(),
            row_actions: TrackActions::default(),
            selected: HashSet::new(),
            selecting: false,
            anchor: None,
            cursor: None,
            focus: cx.focus_handle(),
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

    /// What a row's menu acts on: the whole selection when the row is in it, else the row alone.
    fn row_target(&self, index: usize) -> Target {
        let Some(row) = self.rows.get(index) else {
            return Target::None;
        };
        if !self.selected.contains(&row.key) {
            return Target::track(row.key.clone(), row.title.clone());
        }
        let keys: Vec<SharedString> = self
            .rows
            .iter()
            .filter(|candidate| self.selected.contains(&candidate.key))
            .map(|candidate| SharedString::from(candidate.key.clone()))
            .collect();
        // "Title +3" rather than a count, which would need a plural form per locale.
        let label = match keys.len() {
            0 | 1 => SharedString::from(row.title.clone()),
            many => SharedString::from(format!("{} +{}", row.title, many - 1)),
        };
        Target::tracks(keys, label)
    }

    /// What the keyboard acts on: the selection when there is one, else the row under the cursor.
    pub fn focus_target(&self) -> Target {
        let index = self
            .rows
            .iter()
            .position(|row| self.selected.contains(&row.key))
            .or(self.cursor);
        index.map_or(Target::None, |index| self.row_target(index))
    }

    fn action_keys(&self) -> Vec<String> {
        match self.selected.is_empty() {
            false => self.selected_keys(),
            true => self
                .cursor
                .and_then(|index| self.rows.get(index))
                .map(|row| vec![row.key.clone()])
                .unwrap_or_default(),
        }
    }

    /// The Delete key: take the cursor row out of its playlist, or off disk once confirmed.
    fn remove_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.action_keys();
        if keys.is_empty() {
            cx.propagate();
            return;
        }
        if self.row_actions.remove_from_playlist {
            if let Some(index) = self.cursor {
                cx.emit(TrackListEvent::RemoveFromPlaylist(index));
            }
            return;
        }
        if !self.row_actions.delete {
            cx.propagate();
            return;
        }
        let label = self.focus_target().label();
        let list = cx.entity().downgrade();
        crate::dialogs::confirm_delete(
            t("delete"),
            label,
            move |_, cx| {
                list.update(cx, |_, cx| cx.emit(TrackListEvent::Delete(keys.clone())))
                    .ok();
            },
            window,
            cx,
        );
    }

    /// How many rows are selected, without cloning every key to count them: `selected_keys` walks
    /// all rows and allocates a String each, and render paths only ever wanted the number.
    pub fn selected_count(&self) -> usize {
        self.selected.len()
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
            self.selected.insert(key.clone());
            self.anchor = self.rows.iter().position(|row| row.key == key);
        }
        self.selecting = !self.selected.is_empty();
        cx.notify();
        cx.emit(TrackListEvent::SelectionChanged);
    }

    /// Move the keyboard cursor, starting from the top when there is none yet.
    fn move_cursor(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() - 1;
        let next = match self.cursor {
            Some(current) => current.saturating_add_signed(step).min(last),
            None if step < 0 => last,
            None => 0,
        };
        self.cursor = Some(next);
        self.scroll_to(next);
        cx.notify();
    }

    /// Enter on the cursor row: play it, or tick it when a selection is being built.
    fn activate_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.cursor else {
            return;
        };
        let Some(row) = self.rows.get(index) else {
            return;
        };
        if self.selecting {
            let key = row.key.clone();
            self.toggle_selected(key, cx);
        } else {
            cx.emit(TrackListEvent::Play(index));
        }
    }

    /// Select every row between the last one picked and `index`, the way a shift-click reads.
    fn select_through(&mut self, index: usize, cx: &mut Context<Self>) {
        let anchor = self.anchor.unwrap_or(index);
        let (first, last) = (anchor.min(index), anchor.max(index));
        for row in self.rows.get(first..=last).unwrap_or_default() {
            self.selected.insert(row.key.clone());
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
        let artist_route = Route::artist_of(track);
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
        profile::elements::built();
        let wash = motion::color(
            motion::row_id("row-wash", index),
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
            motion::row_id("row-title", index),
            if playing {
                accent
            } else {
                style::white(style::TEXT_STRONG, cx)
            },
            window,
            cx,
        );
        let badge = motion::amount(
            motion::row_id("row-badge", index),
            if stored && !playing { 1. } else { 0. },
            motion::FAST,
            window,
            cx,
        );
        // One builder, so the dots button and the right-click menu cannot drift apart.
        let menu = {
            let target = self.row_target(index);
            let scope = RowScope {
                list: cx.entity().downgrade(),
                index,
                rows: self.rows.len(),
                actions: self.row_actions,
            };
            Rc::new(
                move |popup, window: &mut Window, cx: &mut Context<PopupMenu>| {
                    set_row_scope(scope.clone(), cx);
                    menus::build(target.clone(), popup, window, cx)
                },
            )
        };

        columns::frame()
            .id(("track-row", index))
            .group(ROW_GROUP)
            .h(style::row_height())
            .rounded(style::radius_lg())
            .bg(wash)
            // The queue registers a drop target for this and nothing ever produced one, so the
            // whole affordance was unreachable. A row inside a selection drags the selection.
            .on_drag(
                {
                    let selected_keys = match selected {
                        true => self.selected_keys(),
                        false => vec![key.clone()],
                    };
                    // "Title +3" rather than a count, which would need a plural form per locale.
                    let label = match selected_keys.len() {
                        0 | 1 => SharedString::from(track.title.clone()),
                        many => SharedString::from(format!("{} +{}", track.title, many - 1)),
                    };
                    TrackDrag {
                        keys: selected_keys,
                        label,
                    }
                },
                |drag, _, _, cx| cx.new(|_| drag.clone()),
            )
            // Upstream the wash is an inline style, which outranks `hover:bg-white/5`; a refinement does not.
            .when(!playing && !selected, |row| {
                row.hover(|row| row.bg(style::white(style::RAISED, cx)))
            })
            // The keyboard cursor, drawn so it can be followed.
            .when(self.cursor == Some(index), |row| {
                row.border_1().border_color(cx.theme().ring)
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // A click is also where the arrows should carry on from.
                this.cursor = Some(index);
                window.focus(&this.focus, cx);
                let modifiers = event.modifiers();
                // Ctrl/cmd picks one, shift extends from the last pick. Without these the only way
                // into selection was a page-owned select-all, which three pages do not have, and a
                // range had to be ticked a checkbox at a time.
                if modifiers.secondary() {
                    this.toggle_selected(key.clone(), cx);
                } else if modifiers.shift && this.selecting {
                    this.select_through(index, cx);
                } else if this.selecting {
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
                            .id(("title", index))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(title_color)
                            // Truncated text is otherwise unrecoverable: narrow the window or meet
                            // a long "Artist feat. Artist" credit and the rest is simply gone.
                            .tooltip({
                                let title = title.clone();
                                move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(title.clone())
                                        .build(window, cx)
                                }
                            })
                            .child(title.clone()),
                    ),
            )
            .child(
                columns::flexible(columns::TEXT_MIN, 1.)
                    .id(("artist", index))
                    .text_sm()
                    .text_color(style::slate_500(cx))
                    .when_some(artist_route.clone(), |cell, route| {
                        cell.hover(|cell| cell.text_color(style::slate_400(cx)))
                            .on_click(move |_, _, cx| {
                                // Without this the row's own click would play the track too.
                                cx.stop_propagation();
                                Navigator::navigate(route.clone(), cx)
                            })
                    })
                    .tooltip({
                        let artist = artist.clone();
                        move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(artist.clone())
                                .build(window, cx)
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
                        .tooltip({
                            let album = album.clone();
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(album.clone())
                                    .build(window, cx)
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

impl Render for TrackList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();
        crate::scroll::smooth_list("track-list", &self.scroll, window, cx);
        v_flex()
            .size_full()
            // Nothing outside the shell held a focus handle, so no list could be walked with the
            // keyboard: no arrows, no Enter, no select-all, and Escape never cleared a selection.
            .track_focus(&self.focus)
            .key_context("TrackList")
            .on_action(cx.listener(|this, _: &CursorDown, _, cx| this.move_cursor(1, cx)))
            .on_action(cx.listener(|this, _: &CursorUp, _, cx| this.move_cursor(-1, cx)))
            .on_action(cx.listener(|this, _: &ActivateRow, _, cx| this.activate_cursor(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &PlayNextSelection, _, cx| {
                store::actions::play_next(this.action_keys(), cx)
            }))
            .on_action(cx.listener(|this, _: &QueueSelection, _, cx| {
                store::actions::append(this.action_keys(), cx)
            }))
            .on_action(cx.listener(|this, _: &FavoriteSelection, _, cx| {
                let keys = this.action_keys();
                let favorites = FavoritesModel::global(cx);
                let all = keys.iter().all(|key| favorites.read(cx).is_favorite(key));
                store::actions::set_favorites(keys, !all, cx);
            }))
            .on_action(cx.listener(|this, _: &RemoveSelection, window, cx| {
                this.remove_selection(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ClearSelection, _, cx| {
                if this.selecting {
                    this.clear_selection(cx);
                } else {
                    cx.propagate();
                }
            }))
            .child(
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
