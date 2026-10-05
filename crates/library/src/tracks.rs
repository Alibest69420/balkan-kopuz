//! The tracks facet: the shared table with moveable, resizable, hideable columns, groups, and paging.

use std::rc::Rc;

use api::{QueueContext, TrackInfo};
use components::columns::{self as table, Layout};
use components::format::t;
use components::track_header::{ColumnMove, ColumnResize};
use components::track_list::TrackDrag;
use components::track_row::RowMenu;
use components::{TrackHeader, TrackRow, menus, style};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, ParentElement as _, Pixels,
    SharedString, Styled as _, WeakEntity, Window, div, uniform_list,
};
use store::layout::{Column, Density};
use store::{Target, actions};

use crate::columns::{self, MAX_WIDTH, MIN_WIDTH};
use crate::filters;
use crate::{Library, Line};

impl Library {
    /// The lines the table draws: the loaded rows, with a heading wherever a group starts.
    pub(crate) fn rebuild_lines(&mut self) {
        self.lines.clear();
        let Some(field) = self.group_by().map(str::to_string) else {
            return;
        };
        let mut current: Option<String> = None;
        for (index, row) in self.rows.iter().enumerate() {
            let group = self.group_of(&field, row);
            if current.as_deref() != Some(group.as_str()) {
                self.lines
                    .push(Line::Group(SharedString::from(group.clone())));
                current = Some(group);
            }
            self.lines.push(Line::Track(index));
        }
    }

    /// What group a row falls in. A year is the album's, since a track row carries none.
    fn group_of(&self, field: &str, row: &TrackInfo) -> String {
        match field {
            "artist" => non_empty(&row.artist),
            "year" => self
                .albums_meta
                .get(&row.album_id)
                .map(|meta| meta.year)
                .filter(|year| *year > 0)
                .map(|year| year.to_string())
                .unwrap_or_else(|| t("library_group_unknown").to_string()),
            _ => non_empty(&row.album),
        }
    }

    /// Start playing at `index`, over whatever the filter set describes.
    pub(crate) fn play_from(&mut self, index: usize, cx: &mut Context<Self>) {
        // The daemon materializes a filter itself, so a 40,000-row queue never crosses the wire.
        if !self.whole && self.filters().folder.is_none() {
            let filter = self.filters().track_filter(self.sort());
            actions::play_context(QueueContext::Filter { filter }, Some(index as u32), cx);
            return;
        }
        let keys: Vec<String> = self.rows.iter().map(|row| row.key.clone()).collect();
        actions::play_tracks(keys, Some(index as u32), cx);
    }

    /// The table: its header and the rows under it, of which only a screenful is ever built.
    pub(crate) fn table(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        components::scroll::smooth_list("library-table", &self.scroll, window, cx);
        let grouped = self.group_by().is_some();
        let count = match grouped {
            true => self.lines.len(),
            false => self.total(),
        };
        let height = style::density_row(self.density);
        let sticky = grouped.then(|| self.sticky_group()).flatten();
        v_flex()
            .size_full()
            .gap_2()
            .child(self.header(table::visible(&self.columns), cx))
            .children(sticky.map(|name| {
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded(style::radius_lg())
                    .bg(style::white(style::RAISED, cx))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                    .child(name)
            }))
            .child(
                div().flex_1().min_h_0().child(
                    uniform_list(
                        "library-table",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                            this.note_visible(&range, cx);
                            this.ensure_window(&range, window, cx);
                            let layout = table::visible(&this.columns);
                            range
                                .map(|index| this.line(index, grouped, height, &layout, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                ),
            )
            .into_any_element()
    }

    /// The heading the topmost visible row belongs to, pinned above the list once its own has scrolled off.
    fn sticky_group(&self) -> Option<SharedString> {
        self.lines[..self.visible_from.min(self.lines.len())]
            .iter()
            .rev()
            .find_map(|line| match line {
                Line::Group(name) => Some(name.clone()),
                Line::Track(_) => None,
            })
    }

    /// Where the viewport starts, for the pinned heading. Only a change of group costs a frame.
    fn note_visible(&mut self, range: &std::ops::Range<usize>, cx: &mut Context<Self>) {
        if self.visible_from == range.start {
            return;
        }
        let before = self.sticky_group();
        self.visible_from = range.start;
        if self.group_by().is_some() && before != self.sticky_group() {
            cx.notify();
        }
    }

    fn line(
        &self,
        index: usize,
        grouped: bool,
        height: Pixels,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !grouped {
            return self.row(index, index, height, layout, cx);
        }
        match self.lines.get(index) {
            Some(Line::Group(name)) => h_flex()
                .w_full()
                .h(height)
                .items_center()
                .px_2()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .child(name.clone())
                .into_any_element(),
            Some(Line::Track(row)) => self.row(*row, index, height, layout, cx),
            None => div().h(height).into_any_element(),
        }
    }

    /// One row, or the shape of one while its window is still being read.
    fn row(
        &self,
        index: usize,
        slot: usize,
        height: Pixels,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let covers = self.density == Density::Comfortable
            && store::ConfigModel::global(cx).read(cx).row_images();
        let Some(track) = self.track_at(index) else {
            return components::skeleton::row(slot, height, covers, layout.len(), cx)
                .into_any_element();
        };
        let key = track.key.clone();
        let selected = self.selection.contains(&key);
        let drag_keys: Vec<String> = match selected && !self.selection.is_everything() {
            true => self.selection.picked().to_vec(),
            false => vec![key.clone()],
        };
        let drag_label = match drag_keys.len() {
            0 | 1 => SharedString::from(track.title.clone()),
            many => SharedString::from(format!("{} +{}", track.title, many - 1)),
        };
        let menu: RowMenu = Rc::new(menus::builder(self.row_target(track)));
        TrackRow::new(track, slot)
            .columns(layout.clone())
            .height(height)
            .lead(Some(SharedString::from((index + 1).to_string())))
            .cover(covers)
            .stored(store::DownloadsModel::global(cx).read(cx).is_stored(&key))
            .plays(self.plays_for(track))
            .playing(self.is_playing(&key))
            .selected(selected)
            .picking(self.selection.is_active())
            .cursor(self.selection.cursor() == Some(index))
            .drag(TrackDrag {
                keys: drag_keys,
                label: drag_label,
            })
            .on_click(cx.listener({
                let key = key.clone();
                move |this, event: &ClickEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    let modifiers = event.modifiers();
                    if modifiers.secondary() {
                        this.selection.toggle(&key, index);
                    } else if modifiers.shift && this.selection.is_active() {
                        let offset = this.pager.offset();
                        let whole = this.whole;
                        let rows = this.rows.clone();
                        this.selection.extend_to(index, move |slot| {
                            let at = match whole {
                                true => Some(slot),
                                false => slot.checked_sub(offset),
                            }?;
                            rows.get(at).map(|row| row.key.clone())
                        });
                    } else if this.selection.is_active() {
                        this.selection.toggle(&key, index);
                    } else {
                        this.selection.set_cursor(Some(index));
                        this.play_from(index, cx);
                    }
                    cx.notify();
                }
            }))
            .on_pick(cx.listener(move |this, _: &bool, _, cx| {
                this.selection.toggle(&key, index);
                cx.notify();
            }))
            .menu(menu)
            .into_any_element()
    }

    /// What a row's menu acts on: the selection when the row is in it, else the row alone.
    fn row_target(&self, track: &TrackInfo) -> Target {
        if !self.selection.contains(&track.key) || self.selection.is_everything() {
            return Target::track(track.key.clone(), track.title.clone());
        }
        let keys: Vec<SharedString> = self
            .selection
            .picked()
            .iter()
            .map(|key| SharedString::from(key.clone()))
            .collect();
        Target::selection(keys, track.title.clone())
    }

    /// The header: a sort control per column, a grip between them, and a menu to hide them.
    fn header(&self, layout: Layout, cx: &mut Context<Self>) -> AnyElement {
        let sort = self.sort();
        let page = cx.entity().downgrade();
        let held = self.columns.clone();
        let header = TrackHeader::new("library-header", layout)
            .sort(
                Some((sort.field.as_str(), sort.descending)),
                |id| filters::is_server_sortable(id) || id == "format",
                cx.listener(|this, id: &str, _, cx| {
                    let next = this.sort().toggled(id).to_layout();
                    this.write_facet(cx, move |facet| facet.sort = next);
                }),
            )
            .reorder(cx.listener(|this, moved: &ColumnMove, _, cx| {
                let mut held = this.columns.clone();
                columns::reorder(&mut held, moved.moved, moved.before);
                this.columns = held.clone();
                this.write_facet(cx, move |facet| facet.columns = held);
            }))
            .resize(cx.listener(|this, resize: &ColumnResize, _, cx| {
                this.resize_column(resize.id, resize.width, cx)
            }))
            .menu(move |menu, window, cx| columns_menu(page.clone(), &held, menu, window, cx));
        match self.selection.is_active() {
            true => header
                .select_all(
                    self.selection.is_everything(),
                    cx.listener(|this, _: &bool, _, cx| {
                        match this.selection.is_everything() {
                            true => this.selection.clear(),
                            false => this.selection.select_everything(),
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            false => header.into_any_element(),
        }
    }

    fn resize_column(&mut self, id: &str, width: f32, cx: &mut Context<Self>) {
        let width = width.clamp(MIN_WIDTH, MAX_WIDTH);
        let mut held = self.columns.clone();
        let Some(column) = held.iter_mut().find(|column| column.id == id) else {
            return;
        };
        if column.width == Some(width) {
            return;
        }
        column.width = Some(width);
        self.columns = held.clone();
        self.write_facet(cx, move |facet| facet.columns = held);
    }
}

/// The show-and-hide menu, reached from the header's right-click and from the toolbar alike.
pub(crate) fn columns_menu(
    page: WeakEntity<Library>,
    held: &[Column],
    menu: PopupMenu,
    _: &mut Window,
    _: &mut Context<PopupMenu>,
) -> PopupMenu {
    let toggle = move |id: &str, cx: &mut App| {
        let id = id.to_string();
        page.update(cx, |this, cx| {
            let mut columns = this.columns.clone();
            if let Some(column) = columns.iter_mut().find(|column| column.id == id) {
                column.visible = !column.visible;
            }
            this.columns = columns.clone();
            this.write_facet(cx, move |facet| facet.columns = columns);
        })
        .ok();
    };
    table::hide_menu(held, Rc::new(toggle), menu)
}

fn non_empty(value: &str) -> String {
    match value.trim().is_empty() {
        true => t("library_group_unknown").to_string(),
        false => value.trim().to_string(),
    }
}
