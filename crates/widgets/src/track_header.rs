//! The labels over a track table: sort, move, resize and hide its columns, or pick every row.

use std::rc::Rc;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Pixels,
    Render, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};

use crate::columns::{self, Layout, Shown, Spec};
use crate::format::t;
use crate::icons;
use crate::motion;
use crate::style;

// A column's channels, keyed off its spec id.
const CELL: u64 = 0;
const HOVER: u64 = 1;
const INK: u64 = 2;
const GRIP: u64 = 3;
// The header's own channels, keyed off its id.
const RESIZE: u64 = 0;
const SELECT_ALL: u64 = 1;

/// What a press on a sortable heading does: re-order by that column.
pub type OnSort = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// What ticking the select-all box does.
pub type OnSelectAll = Rc<dyn Fn(&bool, &mut Window, &mut App)>;
/// The header's right-click menu.
pub type HeaderMenu = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;
/// What dropping one heading on another does.
pub type OnMove = Rc<dyn Fn(&ColumnMove, &mut Window, &mut App)>;
/// What dragging a heading's edge does.
pub type OnResize = Rc<dyn Fn(&ColumnResize, &mut Window, &mut App)>;

/// A heading dropped on another, to sit where that one is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnMove {
    pub moved: &'static str,
    pub before: &'static str,
}

/// A heading's edge dragged to `width`, before any clamp the owner puts on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnResize {
    pub id: &'static str,
    pub width: f32,
}

/// A heading being dragged onto another.
#[derive(Clone)]
struct ColumnDrag {
    id: &'static str,
    label: SharedString,
}

impl Render for ColumnDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_xs()
            .child(self.label.clone())
    }
}

/// A heading's edge being dragged, which resizes rather than reorders.
#[derive(Clone)]
struct EdgeDrag;

impl Render for EdgeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Where a resize started: which column, the pointer, and the width it had.
#[derive(Clone, Copy)]
struct Resizing {
    id: &'static str,
    from: Pixels,
    base: f32,
}

#[derive(Clone)]
struct Sorting {
    active: Option<(&'static str, bool)>,
    sortable: fn(&str) -> bool,
    on_sort: OnSort,
}

/// The header over a track table, on the same columns as its rows.
#[derive(IntoElement)]
pub struct TrackHeader {
    id: SharedString,
    columns: Layout,
    sort: Option<Sorting>,
    select_all: Option<(bool, OnSelectAll)>,
    reorder: Option<OnMove>,
    resize: Option<OnResize>,
    menu: Option<HeaderMenu>,
}

impl TrackHeader {
    pub fn new(id: impl Into<SharedString>, columns: Layout) -> Self {
        Self {
            id: id.into(),
            columns,
            sort: None,
            select_all: None,
            reorder: None,
            resize: None,
            menu: None,
        }
    }

    /// Headings `sortable` accepts sort on a press; `active` is the column ordered by, and whether descending.
    pub fn sort(
        mut self,
        active: Option<(&str, bool)>,
        sortable: fn(&str) -> bool,
        on_sort: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.sort = Some(Sorting {
            active: active.and_then(|(field, descending)| {
                columns::spec(field).map(|spec| (spec.id, descending))
            }),
            sortable,
            on_sort: Rc::new(on_sort),
        });
        self
    }

    /// A box over the lead column in place of `#`, ticked when every row is picked.
    pub fn select_all(
        mut self,
        checked: bool,
        toggle: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.select_all = Some((checked, Rc::new(toggle)));
        self
    }

    /// Headings can be dragged onto one another.
    pub fn reorder(
        mut self,
        on_move: impl Fn(&ColumnMove, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.reorder = Some(Rc::new(on_move));
        self
    }

    /// Each heading carries a grip on its edge.
    pub fn resize(
        mut self,
        on_resize: impl Fn(&ColumnResize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.resize = Some(Rc::new(on_resize));
        self
    }

    /// The right-click menu, such as which columns are shown.
    pub fn menu(
        mut self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }
}

/// One of a column's channels; a spec id is static, so this allocates nothing.
fn key(spec: &Spec, channel: u64) -> ElementId {
    ElementId::NamedInteger(SharedString::new_static(spec.id), channel)
}

impl RenderOnce for TrackHeader {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let resizing =
            window.use_keyed_state(
                motion::channel(&self.id, RESIZE),
                cx,
                |_, _| None::<Resizing>,
            );
        let select_id = motion::channel(&self.id, SELECT_ALL);
        let mut header = columns::frame()
            .id(self.id.clone())
            .border_b_1()
            .border_color(style::white(style::BORDER, cx))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(style::white(style::TEXT_MUTED, cx))
            .child(columns::fixed(columns::INDEX).justify_center().map(|cell| {
                match self.select_all {
                    Some((checked, toggle)) => cell.child(
                        Checkbox::new(select_id)
                            .checked(checked)
                            .accessibility_label(t("select_all_tracks"))
                            .on_click(move |checked, window, cx| toggle(checked, window, cx)),
                    ),
                    None => cell.child("#"),
                }
            }));
        for shown in self.columns.iter() {
            header = header.child(heading(
                shown,
                self.sort.as_ref(),
                self.reorder.clone(),
                self.resize.clone().map(|resize| (resize, resizing.clone())),
                window,
                cx,
            ));
        }
        let header = header.child(columns::fixed(columns::MENU));
        match self.menu {
            Some(menu) => header
                .context_menu(move |popup, window, cx| menu(popup, window, cx))
                .into_any_element(),
            None => header.into_any_element(),
        }
    }
}

fn heading(
    shown: &Shown,
    sort: Option<&Sorting>,
    reorder: Option<OnMove>,
    resize: Option<(OnResize, Entity<Option<Resizing>>)>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let spec = shown.spec;
    let sortable = sort.is_some_and(|sort| (sort.sortable)(spec.id));
    let active = sort.and_then(|sort| sort.active.filter(|(id, _)| *id == spec.id));
    let (hovered, pointer) = motion::hovered(key(spec, HOVER), window, cx);
    let lit = active.is_some() || hovered;
    let ink = motion::color(
        key(spec, INK),
        match lit {
            true => cx.theme().foreground,
            false => style::white(style::TEXT_MUTED, cx),
        },
        window,
        cx,
    );
    // The active column says which way; any other shows its hint only under the pointer.
    let glyph = match active {
        Some((_, false)) => Some(icons::CHEVRON_UP),
        Some((_, true)) => Some(icons::CHEVRON_DOWN),
        None => lit.then_some(icons::SORT),
    };
    let label = t(spec.label_key);
    let shown_label = match spec.id {
        "duration" => icons::icon(icons::CLOCK).xsmall().into_any_element(),
        _ => div().child(label.clone()).into_any_element(),
    };
    let on_sort = sort.filter(|_| sortable).map(|sort| sort.on_sort.clone());
    columns::sized(shown)
        .id(key(spec, CELL))
        .relative()
        .when(spec.align_end, |cell| cell.justify_end())
        .text_color(ink)
        .on_hover(motion::track(pointer))
        .when_some(on_sort, |cell, on_sort| {
            cell.cursor_pointer()
                .on_click(move |_, window, cx| on_sort(spec.id, window, cx))
        })
        .when_some(reorder, |cell, on_move| {
            cell.on_drag(ColumnDrag { id: spec.id, label }, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .drag_over::<ColumnDrag>(|cell, _, _, cx| cell.bg(style::white(style::HOVER, cx)))
            .on_drop(move |dragged: &ColumnDrag, window, cx| {
                let moved = ColumnMove {
                    moved: dragged.id,
                    before: spec.id,
                };
                on_move(&moved, window, cx)
            })
        })
        .child(shown_label)
        .when_some(glyph.filter(|_| sortable), |cell, glyph| {
            cell.child(
                div()
                    .ml_1()
                    .child(icons::icon(glyph).size(style::text_10())),
            )
        })
        .when_some(resize, |cell, (on_resize, resizing)| {
            cell.child(grip(shown, on_resize, resizing, cx))
        })
        .into_any_element()
}

/// The edge of a heading: drag it and the column takes the width it is left at.
fn grip(
    shown: &Shown,
    on_resize: OnResize,
    resizing: Entity<Option<Resizing>>,
    cx: &App,
) -> AnyElement {
    let spec = shown.spec;
    let base = shown.width.unwrap_or(spec.base);
    let hover = style::white(style::HOVER, cx);
    div()
        .id(key(spec, GRIP))
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(-3.))
        .w(px(6.))
        .cursor_col_resize()
        .hover(move |grip| grip.bg(hover))
        .on_drag(EdgeDrag, |drag, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| drag.clone())
        })
        .on_mouse_down(MouseButton::Left, {
            let resizing = resizing.clone();
            move |event: &MouseDownEvent, _, cx| {
                let from = event.position.x;
                resizing.update(cx, |held, _| {
                    *held = Some(Resizing {
                        id: spec.id,
                        from,
                        base,
                    })
                });
                cx.stop_propagation();
            }
        })
        // Every grip hears every edge drag; only the one it started on answers.
        .on_drag_move(move |event: &DragMoveEvent<EdgeDrag>, window, cx| {
            let Some(held) = *resizing.read(cx) else {
                return;
            };
            if held.id != spec.id {
                return;
            }
            let resize = ColumnResize {
                id: spec.id,
                width: held.base + f32::from(event.event.position.x - held.from),
            };
            on_resize(&resize, window, cx)
        })
        .into_any_element()
}
