use std::collections::HashSet;
use std::rc::Rc;

use api::TrackInfo;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled,
    UniformListScrollHandle, WeakEntity, Window, actions, div, uniform_list,
};
use store::{ConfigModel, FavoritesModel, PlayerModel, Target};

use crate::columns::{self, Layout};
use crate::format::t;
use crate::menus;
use crate::style;
use crate::track_row::TrackRow;

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
    // The one setting a row reads, held so another config change does not redraw every list.
    row_images: bool,
    _config: gpui_kit::Subscription,
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
            row_images: ConfigModel::global(cx).read(cx).row_images(),
            _config: cx.observe(&ConfigModel::global(cx), |this, config, cx| {
                let row_images = config.read(cx).row_images();
                if this.row_images != row_images {
                    this.row_images = row_images;
                    cx.notify();
                }
            }),
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
        // Selection mode outliving a re-sort's emptied selection would leave a "0 selected" pill eating clicks.
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
        Target::selection(keys, row.title.clone())
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
        crate::dialogs::confirm(
            crate::dialogs::ConfirmKind::Delete,
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

    /// How many rows are selected, without the per-row allocation `selected_keys` pays.
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

    /// The columns the rows draw, for a page's header to sit over them.
    pub fn layout(&self) -> Layout {
        columns::standard(self.options.show_album)
    }

    fn row(&self, index: usize, columns: &Layout, cx: &mut Context<Self>) -> AnyElement {
        let Some(track) = self.rows.get(index) else {
            return div().into_any_element();
        };
        let key = track.key.clone();
        let playing = PlayerModel::global(cx)
            .read(cx)
            .track()
            .is_some_and(|now| now.key == key);
        let selected = self.selected.contains(&key);
        let lead = self.options.show_index.then(|| {
            let number = match self.options.track_numbers {
                true => track.track_number.unwrap_or(index as u32 + 1),
                false => index as u32 + 1,
            };
            SharedString::from(number.to_string())
        });
        // A row inside a selection drags the selection; the queue is where it lands.
        let keys = match selected {
            true => self.selected_keys(),
            false => vec![key.clone()],
        };
        // "Title +3" rather than a count, which would need a plural form per locale.
        let label = match keys.len() {
            0 | 1 => SharedString::from(track.title.clone()),
            many => SharedString::from(format!("{} +{}", track.title, many - 1)),
        };
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
        TrackRow::new(track, index)
            .columns(columns.clone())
            .height(style::row_height())
            .lead(lead)
            .cover(self.options.show_cover && self.row_images)
            .playing(playing)
            .selected(selected)
            .picking(self.options.selectable && self.selecting)
            .cursor(self.cursor == Some(index))
            .drag(TrackDrag { keys, label })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // A click is also where the arrows should carry on from.
                this.cursor = Some(index);
                window.focus(&this.focus, cx);
                let modifiers = event.modifiers();
                // Ctrl/cmd picks one, shift extends from the last pick.
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
            .on_pick(cx.listener({
                let key = track.key.clone();
                move |this, _: &bool, _, cx| this.toggle_selected(key.clone(), cx)
            }))
            .menu(menu)
            .into_any_element()
    }
}

impl Render for TrackList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();
        crate::scroll::smooth_list("track-list", &self.scroll, window, cx);
        v_flex()
            .size_full()
            // Without a handle here no list took arrows, Enter, select-all or Escape.
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
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let columns = this.layout();
                        range.map(|index| this.row(index, &columns, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .size_full(),
            )
    }
}
