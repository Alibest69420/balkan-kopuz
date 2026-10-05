//! The optional left column: the playlists and folders that are yours, as a list you can drop onto.

mod order;
mod settings;

use components::focus::{Axis, Roving, Step};
use components::format::t;
use components::{EmptyState, TrackDrag, icons, menus, motion, style};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, DragMoveEvent, ElementId,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, Render,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled, Subscription,
    UniformListScrollHandle, Window, div, px, uniform_list,
};
use store::{LayoutModel, Navigator, PlaylistsModel, Route, Target};

use order::Entry;

pub use settings::register as register_settings;

// The bounds the panel drags between, matching the right-hand queue panel's feel.
const MIN_WIDTH: Pixels = px(180.);
const MAX_WIDTH: Pixels = px(420.);
/// A window narrower than this opens with the panel closed.
pub const NARROW_WINDOW: Pixels = px(1100.);

const HANDLE_WIDTH: Pixels = px(6.);
const GLYPH: Pixels = px(14.);
const CLOSE_BUTTON: Pixels = px(24.);
const CLOSE_GLYPH: Pixels = px(12.);
// `bg-black/40`, the same scrim the other chrome wears.
const SCRIM: f32 = 0.40;

/// Open the panel, or close it if it is already open; what `ctrl-b` does.
pub fn toggle(cx: &mut App) {
    LayoutModel::edit(cx, |layout| layout.panel.shown = !layout.panel.shown);
}

/// One row being dragged to a new place in the list.
#[derive(Clone)]
struct RowDrag {
    id: SharedString,
    label: SharedString,
}

impl Render for RowDrag {
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

/// Carried by the right-edge handle; nothing is dropped, so it paints nothing.
#[derive(Clone)]
struct ResizePanel;

impl Render for ResizePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// What one row draws, derived when the catalog changes rather than every frame.
#[derive(Clone)]
struct Row {
    id: SharedString,
    name: SharedString,
    folder: bool,
    detail: SharedString,
}

impl Row {
    fn target(&self) -> Target {
        match self.folder {
            true => Target::Folder {
                id: self.id.clone(),
                name: self.name.clone(),
            },
            false => Target::Playlist {
                id: self.id.clone(),
                name: self.name.clone(),
            },
        }
    }

    fn route(&self) -> Route {
        match self.folder {
            true => Route::PlaylistFolder(self.id.clone()),
            false => Route::Playlist(self.id.clone()),
        }
    }
}

/// The panel: a list of what is yours, drag-resized and remembered across launches.
pub struct PlaylistsPanel {
    rows: Vec<Row>,
    width: Pixels,
    roving: Roving,
    hovered: Option<SharedString>,
    handle_hovered: bool,
    // The window was too narrow to spare a column at launch; asking for the panel clears it.
    suppressed: bool,
    shown: bool,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl PlaylistsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let playlists = PlaylistsModel::global(cx);
        let layout = LayoutModel::global(cx);
        let subscriptions = vec![
            cx.observe(&playlists, |this: &mut Self, _, cx| {
                this.reread(cx);
                cx.notify();
            }),
            cx.observe(&layout, |this: &mut Self, _, cx| {
                let shown = store::layout(cx).panel.shown;
                // Asking for the panel, either way, is what lifts a launch-time suppression.
                if shown != this.shown {
                    this.shown = shown;
                    this.suppressed = false;
                }
                this.reread(cx);
                cx.notify();
            }),
        ];
        // A window too narrow to spare a column opens without one, without forgetting the setting.
        let shown = store::layout(cx).panel.shown;
        let mut this = Self {
            rows: Vec::new(),
            width: px(store::layout(cx).panel.width),
            roving: Roving::new(Axis::Column, cx),
            hovered: None,
            handle_hovered: false,
            suppressed: shown && window.viewport_size().width < NARROW_WINDOW,
            shown,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        this.reread(cx);
        this
    }

    /// What the panel has been dragged to; the shell sizes the slot it puts the panel in.
    pub fn width(&self) -> Pixels {
        self.width
    }

    /// Whether the shell should give the panel a slot at all.
    pub fn is_open(&self, cx: &App) -> bool {
        store::layout(cx).panel.shown && !self.suppressed
    }

    fn reread(&mut self, cx: &App) {
        let pinned = store::layout(cx).panel.pinned.clone();
        let model = PlaylistsModel::global(cx).read(cx);
        let mut catalog: Vec<Entry> = model
            .folders()
            .iter()
            .map(|folder| Entry {
                id: folder.id.clone(),
                folder: true,
            })
            .collect();
        catalog.extend(model.playlists().iter().map(|playlist| Entry {
            id: playlist.id.clone(),
            folder: false,
        }));
        self.rows = order::arrange(catalog, &pinned)
            .into_iter()
            .filter_map(|entry| match entry.folder {
                true => model
                    .folders()
                    .iter()
                    .find(|f| f.id == entry.id)
                    .map(|f| Row {
                        id: f.id.clone().into(),
                        name: f.name.clone().into(),
                        folder: true,
                        detail: SharedString::from(f.playlist_ids.len().to_string()),
                    }),
                false => model.playlist(&entry.id).map(|p| Row {
                    id: p.id.clone().into(),
                    name: p.name.clone().into(),
                    folder: false,
                    detail: SharedString::from(p.track_keys.len().to_string()),
                }),
            })
            .collect();
        self.width = px(store::layout(cx).panel.width);
        self.roving.set_len(self.rows.len());
    }

    fn open(&self, index: usize, cx: &mut App) {
        if let Some(row) = self.rows.get(index) {
            Navigator::navigate(row.route(), cx);
        }
    }

    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match self.roving.walk(event) {
            Step::Moved(at) => {
                self.scroll
                    .scroll_to_item(at, gpui_kit::ScrollStrategy::Nearest);
                cx.notify();
            }
            Step::Activate(at) => self.open(at, cx),
            Step::Ignored => cx.propagate(),
        }
    }

    /// The order the list is in now, so a drop writes every row down and the order sticks.
    fn pin(&self, id: &str, at: usize, cx: &mut App) {
        let entries: Vec<Entry> = self
            .rows
            .iter()
            .map(|row| Entry {
                id: row.id.to_string(),
                folder: row.folder,
            })
            .collect();
        let pinned = order::reorder(&entries, id, at);
        LayoutModel::edit(cx, move |layout| layout.panel.pinned = pinned);
    }

    fn row(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().into_any_element();
        };
        let current = Navigator::global(cx).read(cx).current().clone();
        let active = current == row.route();
        let hovered = self.hovered.as_ref() == Some(&row.id);
        let focused = self.roving.is_active(index, window);
        let wash = motion::color(
            ElementId::from((ElementId::from("panel-row"), row.id.clone())),
            match (active, hovered) {
                (true, _) => style::white(style::HOVER, cx),
                (false, true) => style::white(style::RAISED, cx),
                (false, false) => style::white(0., cx),
            },
            window,
            cx,
        );
        let ink = match active || hovered {
            true => cx.theme().foreground,
            false => style::slate_400(cx),
        };
        let mark = row.id.clone();
        let id = row.id.to_string();
        let label = row.name.clone();
        let target = row.target();
        let route = row.route();
        h_flex()
            .id(ElementId::from((
                ElementId::from("panel-row-row"),
                row.id.clone(),
            )))
            .relative()
            .w_full()
            .h(style::list_row())
            .px_2()
            .gap_2()
            .rounded(style::radius_lg())
            .cursor_pointer()
            .bg(wash)
            .text_color(ink)
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                let held = over.then(|| mark.clone());
                if this.hovered != held && (*over || this.hovered.as_ref() == Some(&mark)) {
                    this.hovered = held;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.roving.set_cursor(index);
                Navigator::navigate(route.clone(), cx);
            }))
            .on_drag(
                RowDrag {
                    id: row.id.clone(),
                    label: label.clone(),
                },
                |drag, _, _, cx| cx.new(|_| drag.clone()),
            )
            // A track dropped on a playlist joins it; a folder holds playlists, so it takes none.
            .when(!row.folder, |element| {
                element
                    .on_drop::<TrackDrag>(move |drag, _, cx| {
                        let keys = drag.keys.clone();
                        let id = id.clone();
                        PlaylistsModel::global(cx)
                            .update(cx, move |model, cx| model.add_tracks(id, keys, cx));
                    })
                    .drag_over::<TrackDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
            })
            .child(
                Icon::default()
                    .path(match row.folder {
                        true => icons::FOLDER,
                        false => icons::LIST,
                    })
                    .size(GLYPH)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(style::text_10())
                    .text_color(style::white(style::TEXT_SUBTLE, cx))
                    .child(row.detail.clone()),
            )
            .when(focused, |element| {
                element.child(components::focus::ring(style::radius_lg(), cx))
            })
            .child(self.slot(index, 0., cx))
            .child(self.slot(index + 1, 0.5, cx))
            .context_menu(menus::builder(target))
            .into_any_element()
    }

    /// Half a row that takes a dropped row, so a drag lands above or below whatever it is over.
    fn slot(&self, at: usize, top: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(ElementId::from((
                "panel-slot",
                at as u64 + (top * 2.) as u64,
            )))
            .absolute()
            .left_0()
            .right_0()
            .top(style::list_row() * top)
            .h(style::list_row() / 2.)
            .on_drop::<RowDrag>(cx.listener(move |this, drag: &RowDrag, _, cx| {
                let id = drag.id.to_string();
                this.pin(&id, at, cx);
            }))
            .drag_over::<RowDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .flex_shrink_0()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                    .child(t("playlists")),
            )
            .child(
                Button::new("panel-playlists-close")
                    .custom(style::quiet_button(style::white(0.4, cx), cx))
                    .with_size(CLOSE_GLYPH * (4. / 3.))
                    .size(CLOSE_BUTTON)
                    .rounded(CLOSE_BUTTON / 2.)
                    .icon(icons::icon(icons::CLOSE))
                    .tooltip(t("close"))
                    .on_click(|_, _, cx| toggle(cx)),
            )
    }

    /// The right edge drags the panel between 180 and 420 px; the panel is flush with the window.
    fn resize_handle(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line = motion::color(
            "playlists-panel-handle",
            match self.handle_hovered {
                true => style::white(style::HOVER, cx),
                false => style::white(0., cx),
            },
            window,
            cx,
        );
        div()
            .id("playlists-panel-resize")
            .absolute()
            .right_0()
            .top_0()
            .h_full()
            .w(HANDLE_WIDTH)
            .occlude()
            .cursor_col_resize()
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                if this.handle_hovered != *over {
                    this.handle_hovered = *over;
                    cx.notify();
                }
            }))
            .on_drag(ResizePanel, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizePanel>, _, cx| {
                    let width = (event.event.position.x - event.bounds.left() + this.width)
                        .clamp(MIN_WIDTH, MAX_WIDTH);
                    if width != this.width {
                        this.width = width;
                        LayoutModel::edit(cx, move |layout| layout.panel.width = width.as_f32());
                        cx.notify();
                    }
                }),
            )
            .child(div().absolute().inset_0().bg(line))
    }
}

impl Render for PlaylistsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("playlists-panel");
        components::scroll::smooth_list("playlists-panel-scroll", &self.scroll, window, cx);
        let count = self.rows.len();
        let body = match count {
            0 => div()
                .flex_1()
                .min_h_0()
                .child(EmptyState::new(t("no_playlists_yet")).icon(icons::LIST))
                .into_any_element(),
            _ => uniform_list(
                "playlists-panel-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                    range.map(|index| this.row(index, window, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .px_2()
            .py_1()
            .into_any_element(),
        };

        self.roving
            .group(v_flex().id("playlists-panel"), "PlaylistsPanel")
            .relative()
            .h_full()
            .w_full()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key(event, cx)))
            .bg(style::black(SCRIM, cx))
            .border_r_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(self.header(cx))
            .child(body)
            .child(self.resize_handle(window, cx))
    }
}
