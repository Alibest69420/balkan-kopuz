//! The tracks facet: a real table. Sortable headers, columns you can move, resize and hide,
//! two densities, group headings, and only the rows on screen ever read.

use api::{QueueContext, TrackInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Div, DragMoveEvent, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px, uniform_list,
};
use store::layout::{Column, Density};
use store::{Navigator, Route, Target, actions};
use widgets::format::{duration, t};
use widgets::track_list::{TrackDrag, columns as frame};
use widgets::{Cover, icons, menus, motion, style};

use crate::columns::{self, MAX_WIDTH, MIN_WIDTH, Spec};
use crate::filters;
use crate::{Library, Line};

// The row is the hover group, so the number can step aside for the play glyph.
const ROW_GROUP: &str = "library-row";

/// A column being dragged onto another, so the header can be reordered by hand.
#[derive(Clone)]
pub(crate) struct ColumnDrag(pub SharedString);

impl gpui_kit::Render for ColumnDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_xs()
            .child(self.0.clone())
    }
}

/// A column edge being dragged, which resizes rather than reorders.
#[derive(Clone)]
pub(crate) struct ColumnResize;

impl gpui_kit::Render for ColumnResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

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
        widgets::scroll::smooth_list("library-table", &self.scroll, window, cx);
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
            .child(self.header(window, cx))
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
                            range
                                .map(|index| this.line(index, grouped, height, window, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                ),
            )
            .into_any_element()
    }

    /// The heading the topmost visible row belongs to, pinned above the list.
    ///
    /// At the very top the list's own first heading is already on screen, so nothing is pinned.
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
        &mut self,
        index: usize,
        grouped: bool,
        height: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !grouped {
            return self.row(index, index, height, window, cx);
        }
        match self.lines.get(index).cloned() {
            Some(Line::Group(name)) => h_flex()
                .w_full()
                .h(height)
                .items_center()
                .px_2()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .child(name)
                .into_any_element(),
            Some(Line::Track(row)) => self.row(row, index, height, window, cx),
            None => div().h(height).into_any_element(),
        }
    }

    /// One row, or the shape of one while its window is still being read.
    fn row(
        &mut self,
        index: usize,
        slot: usize,
        height: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(track) = self.track_at(index).cloned() else {
            let shown = columns::visible(&self.columns).len();
            let covers = self.density == Density::Comfortable;
            return crate::skeleton::row(slot, height, covers, shown, cx).into_any_element();
        };
        profile::elements::built();
        let key = track.key.clone();
        let playing = self.is_playing(&key);
        let selected = self.selection.contains(&key);
        let picking = self.selection.is_active();
        let cursor = self.selection.cursor() == Some(index);
        let accent = cx.theme().primary;
        let mark = px(12.);
        let covers = self.density == Density::Comfortable;
        let cover = px(40.);
        let menu_open = self.menu_open.as_deref() == Some(key.as_str());
        let wash = motion::color(
            motion::row_id("library-row-wash", slot),
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
        let target = self.row_target(&track);
        let menu_target = target.clone();
        let drag_keys: Vec<String> = match selected && !self.selection.is_everything() {
            true => self.selection.picked().to_vec(),
            false => vec![key.clone()],
        };
        let drag_label = match drag_keys.len() {
            0 | 1 => SharedString::from(track.title.clone()),
            many => SharedString::from(format!("{} +{}", track.title, many - 1)),
        };

        let mut row = frame::frame()
            .id(("library-row", slot))
            .group(ROW_GROUP)
            .h(height)
            .rounded(style::radius_lg())
            .bg(wash)
            .on_drag(
                TrackDrag {
                    keys: drag_keys,
                    label: drag_label,
                },
                |drag, _, _, cx| cx.new(|_| drag.clone()),
            )
            .when(!playing && !selected, |row| {
                row.hover(|row| row.bg(style::white(style::RAISED, cx)))
            })
            .when(cursor, |row| row.border_1().border_color(cx.theme().ring))
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
            .child(
                frame::fixed(frame::INDEX)
                    .relative()
                    .justify_center()
                    .map(|cell| {
                        if picking {
                            cell.child(
                                Checkbox::new(("library-pick", slot))
                                    .checked(selected)
                                    .on_click(cx.listener({
                                        let key = key.clone();
                                        move |this, _: &bool, _, cx| {
                                            this.selection.toggle(&key, index);
                                            cx.notify();
                                        }
                                    })),
                            )
                        } else if playing {
                            cell.child(icons::icon(icons::VOLUME).size(mark).text_color(accent))
                        } else {
                            cell.child(
                                div()
                                    .text_xs()
                                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                                    .group_hover(ROW_GROUP, |number| number.invisible())
                                    .child((index + 1).to_string()),
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
                        }
                    }),
            );

        for (column, spec) in columns::visible(&self.columns) {
            row = row.child(self.cell(
                column, spec, &track, slot, playing, covers, cover, window, cx,
            ));
        }

        row.child(
            frame::fixed(frame::MENU)
                .id(("library-row-menu", slot))
                .justify_end()
                .on_click(|_, _, cx| cx.stop_propagation())
                .when(!menu_open, |cell| {
                    cell.invisible()
                        .group_hover(ROW_GROUP, |dots| dots.visible())
                })
                .child(
                    Button::new(("library-row-dots", slot))
                        .ghost()
                        .xsmall()
                        .icon(icons::icon(icons::MORE))
                        .dropdown_menu(menus::builder(menu_target))
                        .on_open_change(cx.listener({
                            let key = key.clone();
                            move |this: &mut Library, open: &bool, _, cx| {
                                this.menu_open = open.then(|| SharedString::from(key.clone()));
                                cx.notify();
                            }
                        })),
                ),
        )
        .context_menu(menus::builder(target))
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
        let label = match keys.len() {
            0 | 1 => SharedString::from(track.title.clone()),
            many => SharedString::from(format!("{} +{}", track.title, many - 1)),
        };
        Target::tracks(keys, label)
    }

    #[allow(clippy::too_many_arguments)]
    fn cell(
        &self,
        column: &Column,
        spec: &'static Spec,
        track: &TrackInfo,
        slot: usize,
        playing: bool,
        covers: bool,
        cover: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cell = sized(column, spec).when(spec.align_end, |cell| cell.justify_end());
        match spec.id {
            "title" => {
                let title = SharedString::from(track.title.clone());
                let ink = motion::color(
                    motion::row_id("library-row-title", slot),
                    match playing {
                        true => cx.theme().primary,
                        false => style::white(style::TEXT_STRONG, cx),
                    },
                    window,
                    cx,
                );
                cell.when(covers, |cell| {
                    cell.child(
                        div()
                            .flex_shrink_0()
                            .mr_4()
                            .child(Cover::new(track.artwork.as_ref()).size(cover)),
                    )
                })
                .child(
                    div()
                        .id(("library-title", slot))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(ink)
                        .tooltip({
                            let title = title.clone();
                            move |window, cx| Tooltip::new(title.clone()).build(window, cx)
                        })
                        .child(title),
                )
                .into_any_element()
            }
            "artist" => text_cell(
                cell,
                slot,
                "library-artist",
                &track.artist,
                style::TEXT_SUBTLE,
                Route::artist_of(track),
                cx,
            ),
            "album" => text_cell(
                cell,
                slot,
                "library-album",
                &track.album,
                style::TEXT_DIM,
                (!track.album_id.is_empty()).then(|| Route::Album(track.album_id.clone().into())),
                cx,
            ),
            "plays" => cell
                .text_xs()
                .text_color(style::white(style::TEXT_DIM, cx))
                .child(self.plays_for(track).to_string())
                .into_any_element(),
            "format" => cell
                .text_xs()
                .text_color(style::white(style::TEXT_DIM, cx))
                .child(track.format.clone().unwrap_or_default())
                .into_any_element(),
            _ => cell
                .text_xs()
                .font_family(SharedString::new_static("monospace"))
                .text_color(style::white(style::TEXT_DIM, cx))
                .child(
                    track
                        .duration_secs()
                        .map(|secs| duration(secs * 1000))
                        .unwrap_or_else(|| "--:--".into()),
                )
                .into_any_element(),
        }
    }

    /// The header: a sort control per column, a grip between them, and a menu to hide them.
    fn header(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let sort = self.sort().clone();
        let all_picked = self.selection.is_everything();
        let held = self.columns.to_vec();
        let page = cx.entity().downgrade();
        let mut header = frame::frame()
            .id("library-header")
            .border_b_1()
            .border_color(style::white(style::BORDER, cx))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(style::white(style::TEXT_MUTED, cx))
            .child(frame::fixed(frame::INDEX).justify_center().map(|cell| {
                match self.selection.is_active() {
                    true => cell.child(
                        Checkbox::new("library-select-all")
                            .checked(all_picked)
                            .accessibility_label(t("select_all_tracks"))
                            .on_click(cx.listener(move |this, _: &bool, _, cx| {
                                match this.selection.is_everything() {
                                    true => this.selection.clear(),
                                    false => this.selection.select_everything(),
                                }
                                cx.notify();
                            })),
                    ),
                    false => cell.child("#"),
                }
            }));
        for (column, spec) in columns::visible(&self.columns) {
            header = header.child(self.header_cell(column, spec, &sort, window, cx));
        }
        header
            .child(frame::fixed(frame::MENU))
            .context_menu(move |menu, window, cx| {
                columns_menu(page.clone(), &held, menu, window, cx)
            })
            .into_any_element()
    }

    fn header_cell(
        &self,
        column: &Column,
        spec: &'static Spec,
        sort: &filters::Sort,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(spec.id);
        let sortable = filters::is_server_sortable(spec.id) || spec.id == "format";
        let active = sort.field == spec.id;
        let lit = active || self.header_hover.as_deref() == Some(spec.id);
        let ink = motion::color(
            motion::id("library-header", spec.id),
            match lit {
                true => cx.theme().foreground,
                false => style::white(style::TEXT_MUTED, cx),
            },
            window,
            cx,
        );
        // The active column says which way; any other shows its hint only under the pointer.
        let glyph = match (active, sort.descending) {
            (true, false) => Some(icons::CHEVRON_UP),
            (true, true) => Some(icons::CHEVRON_DOWN),
            (false, _) => lit.then_some(icons::SORT),
        };
        let label = match spec.id {
            "duration" => icons::icon(icons::CLOCK).xsmall().into_any_element(),
            _ => div().child(t(spec.label_key)).into_any_element(),
        };
        sized(column, spec)
            .id(SharedString::from(format!("library-header-{}", spec.id)))
            .relative()
            .when(spec.align_end, |cell| cell.justify_end())
            .text_color(ink)
            .when(sortable, |cell| cell.cursor_pointer())
            .on_hover(cx.listener({
                let id = id.clone();
                move |this, hovered: &bool, _, cx| {
                    // A leave for a column that is no longer hovered would clear its successor.
                    if !*hovered && this.header_hover.as_deref() != Some(id.as_ref()) {
                        return;
                    }
                    this.header_hover = hovered.then(|| id.to_string());
                    cx.notify();
                }
            }))
            .when(sortable, |cell| {
                cell.on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, _, cx| {
                        let next = this.sort().toggled(id.as_ref()).to_layout();
                        this.write_facet(cx, move |facet| facet.sort = next);
                    }
                }))
            })
            // A column is moved by dragging its heading onto the one it should sit before.
            .on_drag(ColumnDrag(id.clone()), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .drag_over::<ColumnDrag>(|cell, _, _, cx| cell.bg(style::white(style::HOVER, cx)))
            .on_drop(cx.listener({
                let id = id.clone();
                move |this, dragged: &ColumnDrag, _, cx| {
                    let mut held = this.columns.clone();
                    columns::reorder(&mut held, dragged.0.as_ref(), id.as_ref());
                    this.columns = held.clone();
                    this.write_facet(cx, move |facet| facet.columns = held);
                }
            }))
            .child(label)
            .when_some(glyph.filter(|_| sortable), |cell, glyph| {
                cell.child(
                    div()
                        .ml_1()
                        .child(icons::icon(glyph).size(style::text_10())),
                )
            })
            .child(self.grip(column, spec, cx))
            .into_any_element()
    }

    /// The edge between two headings: drag it and the column takes the width it is left at.
    fn grip(&self, column: &Column, spec: &'static Spec, cx: &mut Context<Self>) -> AnyElement {
        let id = SharedString::from(spec.id);
        let held = column.width.unwrap_or(spec.base);
        div()
            .id(SharedString::from(format!("library-grip-{}", spec.id)))
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-3.))
            .w(px(6.))
            .cursor_col_resize()
            .hover(|grip| grip.bg(style::white(style::HOVER, cx)))
            .on_drag(ColumnResize, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener({
                    let id = id.clone();
                    move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                        this.resizing = Some((id.to_string(), event.position.x, held));
                        cx.stop_propagation();
                    }
                }),
            )
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ColumnResize>, _, cx| {
                    this.resize_column(event.event.position.x, cx)
                }),
            )
            .into_any_element()
    }

    fn resize_column(&mut self, at: Pixels, cx: &mut Context<Self>) {
        let Some((id, from, base)) = self.resizing.clone() else {
            return;
        };
        let width = (base + f32::from(at - from)).clamp(MIN_WIDTH, MAX_WIDTH);
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
    mut menu: PopupMenu,
    _: &mut Window,
    _: &mut Context<PopupMenu>,
) -> PopupMenu {
    for column in held {
        let Some(spec) = columns::spec(&column.id) else {
            continue;
        };
        if spec.required {
            continue;
        }
        let id = column.id.clone();
        let page = page.clone();
        menu = menu.item(
            PopupMenuItem::new(t(spec.label_key))
                .checked(column.visible)
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    page.update(cx, |this, cx| {
                        let mut columns = this.columns.clone();
                        if let Some(column) = columns.iter_mut().find(|column| column.id == id) {
                            column.visible = !column.visible;
                        }
                        this.columns = columns.clone();
                        this.write_facet(cx, move |facet| facet.columns = columns);
                    })
                    .ok();
                }),
        );
    }
    menu
}

/// A column's box: the width it was dragged to, or the one its kind ships with.
fn sized(column: &Column, spec: &'static Spec) -> Div {
    match (column.width, spec.grow > 0.) {
        (Some(width), _) => frame::fixed(px(width)),
        (None, true) => frame::flexible(px(spec.base), spec.grow),
        (None, false) => frame::fixed(px(spec.base)),
    }
}

fn text_cell(
    cell: Div,
    slot: usize,
    id: &'static str,
    value: &str,
    ink: f32,
    route: Option<Route>,
    cx: &App,
) -> AnyElement {
    let value = SharedString::from(value.to_string());
    cell.id((id, slot))
        .text_sm()
        .text_color(style::white(ink, cx))
        .when_some(route, |cell, route| {
            cell.cursor_pointer()
                .hover(|cell| cell.underline())
                .on_click(move |_, _, cx| {
                    // Without this the row's own click would play the track too.
                    cx.stop_propagation();
                    Navigator::navigate(route.clone(), cx);
                })
        })
        .tooltip({
            let value = value.clone();
            move |window, cx| Tooltip::new(value.clone()).build(window, cx)
        })
        .child(div().flex_1().min_w_0().truncate().child(value))
        .into_any_element()
}

fn non_empty(value: &str) -> String {
    match value.trim().is_empty() {
        true => t("library_group_unknown").to_string(),
        false => value.trim().to_string(),
    }
}
