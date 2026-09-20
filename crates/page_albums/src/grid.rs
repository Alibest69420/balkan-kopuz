//! The albums grid: every album the library holds, deduped by title.

use std::collections::HashMap;
use std::collections::HashSet;

use api::{AlbumInfo, AlbumPage, JobKind, PlaylistCapability, Table};
use config::{AlbumSortField, AlbumViewMode, SortCriterion, SortDirection};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use store::{ConfigModel, JobsModel, Navigator, Query, Route, SourcesModel, actions};
use widgets::format::t;
use widgets::{Cover, Loading, PageHeader, dialogs, icons, motion, style};

use crate::clear_variant;
use crate::ops;
use crate::sort;

// TODO(offline): filtering the grid to downloaded albums waits on an offline seam the checklist flags as wrong.

// `grid-cols-[repeat(auto-fill,minmax(180px,1fr))]`: GPUI wraps flex children instead, at the column's minimum.
const TILE: gpui_kit::Pixels = px(180.);
// The tile's cover fills it inside `p-4`.
const TILE_COVER: gpui_kit::Pixels = px(148.);
const ROW_COVER: gpui_kit::Pixels = px(48.);
// `gap-6` between tiles, so a column costs this much width.
const TILE_GAP: gpui_kit::Pixels = px(24.);
// Until the list has been laid out once there is no measured width to divide.
const TILE_COLUMNS_FALLBACK: usize = 4;

// The two chrome controls track their own hover, since a Button cannot carry a tween.
const SORT_CONTROL: &str = "albums-sort-frame";

/// Every album, as tiles or as rows.
pub struct Albums {
    albums: Entity<Query<AlbumPage>>,
    // Deduped and sorted once per change. Render used to rebuild all of this every frame, and a
    // card hover notifies, so moving the pointer across the grid re-sorted the whole library.
    visible: Vec<AlbumInfo>,
    same_title: HashMap<String, Vec<String>>,
    sort_fields: Vec<AlbumSortField>,
    scroll: UniformListScrollHandle,
    // Which card the pointer is over, and whose menu is open: both drive tweens.
    hovered: Option<String>,
    menu_open: Option<String>,
    control: Option<&'static str>,
    // A library sync is offered once per visit, not once per refetch.
    synced: bool,
    _subscriptions: Vec<Subscription>,
}

impl Albums {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |api| async move { api.albums(store::all()).await },
                cx,
            )
        });
        let sources = SourcesModel::global(cx);
        let config = ConfigModel::global(cx);
        let subscriptions = vec![
            cx.observe(&albums, |this, _, cx| {
                this.sync_empty_library(cx);
                this.rebuild(cx);
            }),
            cx.observe(&sources, |this, _, cx| {
                this.sync_empty_library(cx);
                cx.notify();
            }),
            // The sort lives in config, so a change there is a change to the order.
            cx.observe(&config, |this, _, cx| this.rebuild(cx)),
        ];
        Self {
            albums,
            visible: Vec::new(),
            same_title: HashMap::new(),
            sort_fields: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            hovered: None,
            menu_open: None,
            control: None,
            synced: false,
            _subscriptions: subscriptions,
        }
    }

    /// Dedupe by title, sort, and note which fields are worth offering — on change, not per frame.
    ///
    /// A server splits one release across same-titled albums, so the grid shows each title once
    /// and the menu acts on every album behind it.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let all = self.all_albums(cx);
        self.same_title.clear();
        for album in &all {
            self.same_title
                .entry(title_key(album))
                .or_default()
                .push(album.id.clone());
        }
        let mut seen = HashSet::with_capacity(all.len());
        self.visible = all
            .iter()
            .filter(|album| seen.insert(title_key(album)))
            .cloned()
            .collect();
        sort::sort(&mut self.visible, &criteria(cx));
        self.sort_fields = sort::available_fields(&all);
        cx.notify();
    }

    /// A source that syncs and holds nothing yet gets one pull, the way a first visit should.
    fn sync_empty_library(&mut self, cx: &mut Context<Self>) {
        if self.synced || !SourcesModel::global(cx).read(cx).capabilities().sync {
            return;
        }
        let empty = match self.albums.read(cx).value() {
            Some(page) => page.albums.is_empty(),
            None => return,
        };
        self.synced = true;
        if empty {
            JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(JobKind::LibrarySync, cx));
        }
    }

    fn all_albums(&self, cx: &App) -> Vec<AlbumInfo> {
        self.albums
            .read(cx)
            .value()
            .map(|page| page.albums.clone())
            .unwrap_or_default()
    }

    fn view_toggle(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_0p5()
            .p_0p5()
            .rounded(style::radius_lg())
            .bg(style::white(style::RAISED, cx))
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(self.view_segment(
                "albums-view-grid",
                icons::GRID,
                t("view_grid"),
                AlbumViewMode::Grid,
                window,
                cx,
            ))
            .child(self.view_segment(
                "albums-view-list",
                icons::ROWS,
                t("view_list"),
                AlbumViewMode::List,
                window,
                cx,
            ))
    }

    /// `transition-all` on the switch: the frame carries the wash and the glyph its colour.
    fn view_segment(
        &self,
        id: &'static str,
        icon: &'static str,
        label: SharedString,
        mode: AlbumViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = view_mode(cx) == mode;
        let hovered = self.control == Some(id);
        let wash = style::white(if active { style::HOVER } else { 0. }, cx);
        let target = match (active, hovered) {
            (true, _) => cx.theme().foreground,
            (false, true) => style::white(0.8, cx),
            (false, false) => style::white(0.4, cx),
        };
        let background = motion::color(motion::id(id, "bg"), wash, window, cx);
        let ink = motion::color(motion::id(id, "ink"), target, window, cx);
        div()
            .id(id)
            .w_7()
            .h_6()
            .flex()
            .items_center()
            .justify_center()
            .rounded(style::radius_md())
            .bg(background)
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| {
                    this.track_control(id, *hovered, cx)
                }),
            )
            .child(
                Button::new(SharedString::from(format!("{id}-button")))
                    .xsmall()
                    .icon(icons::icon(icon))
                    .tooltip(label)
                    .size_full()
                    // A Button paints its own hover, so the tween reaches it as a variant.
                    .custom(clear_variant(ink, cx))
                    .on_click(move |_, _, cx| set_view_mode(mode, cx)),
            )
    }

    /// A leave for a control that is no longer the hovered one would clear its successor.
    fn track_control(&mut self, id: &'static str, hovered: bool, cx: &mut Context<Self>) {
        if !hovered && self.control != Some(id) {
            return;
        }
        self.control = hovered.then_some(id);
        cx.notify();
    }

    fn sort_control(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let available = self.sort_fields.clone();
        let primary = criteria(cx).first().copied();
        let label = match primary {
            Some(criterion) => SharedString::from(format!(
                "{}: {}",
                t("sort_by"),
                sort::label(criterion.field)
            )),
            None => t("sort_by"),
        };
        let hovered = self.control == Some(SORT_CONTROL);
        let wash = style::white(if hovered { style::HOVER } else { style::RAISED }, cx);
        let target = if hovered {
            cx.theme().foreground
        } else {
            style::white(0.7, cx)
        };
        let background = motion::color(motion::id(SORT_CONTROL, "bg"), wash, window, cx);
        let ink = motion::color(motion::id(SORT_CONTROL, "ink"), target, window, cx);
        div()
            .id(SORT_CONTROL)
            .flex_shrink_0()
            .rounded(style::radius_lg())
            .bg(background)
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.track_control(SORT_CONTROL, *hovered, cx)
            }))
            .child(
                Button::new("albums-sort")
                    // A Button paints its own hover, so the frame carries the wash and the variant the ink.
                    .custom(clear_variant(ink, cx))
                    .icon(icons::icon(icons::SORT))
                    .label(label)
                    .h_7()
                    .px_3()
                    .text_xs()
                    .dropdown_menu(move |mut menu, _, _| {
                        for field in available.iter().copied() {
                            menu = menu.item(
                                PopupMenuItem::new(sort::label(field))
                                    .checked(
                                        primary.is_some_and(|criterion| criterion.field == field),
                                    )
                                    .on_click(move |_, _, cx| set_sort_field(field, cx)),
                            );
                        }
                        menu = menu.separator();
                        for direction in [SortDirection::Asc, SortDirection::Desc] {
                            menu = menu.item(
                                PopupMenuItem::new(sort::direction_label(direction))
                                    .checked(
                                        primary.is_some_and(|criterion| {
                                            criterion.direction == direction
                                        }),
                                    )
                                    .on_click(move |_, _, cx| set_sort_direction(direction, cx)),
                            );
                        }
                        menu
                    }),
            )
    }
}

impl Albums {
    /// How many tiles fit across, from the width the list reported at its last layout.
    fn tile_columns(&self) -> usize {
        let width: Pixels = self.scroll.0.borrow().base_handle.bounds().size.width;
        if width <= px(0.) {
            return TILE_COLUMNS_FALLBACK;
        }
        // The last tile in a band needs no gap after it.
        (((width + TILE_GAP) / (TILE + TILE_GAP)) as usize).max(1)
    }

    /// One virtualized band: a row of tiles in grid mode, a single album in list mode.
    fn band(
        &self,
        index: usize,
        columns: usize,
        grid: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first = index * columns;
        let last = (first + columns).min(self.visible.len());
        let mut cards: Vec<AnyElement> = Vec::with_capacity(last.saturating_sub(first));
        for album in &self.visible[first..last] {
            let target = MenuTarget {
                id: album.id.clone(),
                title: album.title.clone(),
                same_title: self
                    .same_title
                    .get(&title_key(album))
                    .cloned()
                    .unwrap_or_default(),
            };
            let hovered = self.hovered.as_deref() == Some(album.id.as_str());
            cards.push(match grid {
                true => {
                    let open = self.menu_open.as_deref() == Some(album.id.as_str());
                    card(album, target, hovered, open, window, cx).into_any_element()
                }
                false => row(album, target, hovered, window, cx).into_any_element(),
            });
        }
        match grid {
            true => h_flex()
                .w_full()
                .items_start()
                .gap_6()
                .pb_6()
                .children(cards)
                .into_any_element(),
            false => v_flex().w_full().pb_2().children(cards).into_any_element(),
        }
    }
}

impl Render for Albums {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("albums");
        let settled = self.albums.read(cx).is_settled();
        let grid = view_mode(cx) == AlbumViewMode::Grid;
        widgets::scroll::smooth_list("albums-scroll", &self.scroll, window, cx);
        let empty = self.visible.is_empty();
        // A tile row is the virtualized unit, so the grid builds a screenful instead of the library.
        let columns = match grid {
            true => self.tile_columns(),
            false => 1,
        };
        let rows = self.visible.len().div_ceil(columns.max(1));

        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .child(PageHeader::new(t("all_albums")))
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .mb_4()
                    .child(self.view_toggle(window, cx))
                    .child(self.sort_control(window, cx)),
            )
            .child(div().flex_1().min_h_0().pb_8().map(|body| {
                match (empty, settled) {
                    (true, false) => body.child(Loading),
                    (true, true) => body.child(
                        div()
                            .text_color(style::slate_500(cx))
                            .child(t("no_albums_found")),
                    ),
                    (false, _) => body.child(
                        uniform_list(
                            "albums-scroll",
                            rows,
                            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                                range
                                    .map(|index| this.band(index, columns, grid, window, cx))
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .size_full(),
                    ),
                }
            }))
    }
}

/// The `.vcard` grid tile: a padded raised card whose menu appears on hover.
fn card(
    album: &AlbumInfo,
    target: MenuTarget,
    hovered: bool,
    menu_open: bool,
    window: &mut Window,
    cx: &mut Context<Albums>,
) -> impl IntoElement {
    let menu_target = target.clone();
    let route = Route::Album(album.id.clone().into());
    let wash = style::white(if hovered { style::HOVER } else { style::RAISED }, cx);
    let background = motion::color(motion::id("album-card-bg", &album.id), wash, window, cx);
    let dots = motion::amount(
        motion::id("album-card-menu", &album.id),
        if hovered || menu_open { 1. } else { 0. },
        motion::FAST,
        window,
        cx,
    );
    div()
        .id(SharedString::from(format!("album-card-{}", album.id)))
        .relative()
        .w(TILE)
        .p_4()
        .rounded(style::radius_xl())
        .bg(background)
        .on_hover(cx.listener({
            let id = album.id.clone();
            move |this, hovered: &bool, _, cx| {
                // A leave for a card that is no longer the hovered one would clear its successor.
                if !*hovered && this.hovered.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.hovered = hovered.then(|| id.clone());
                cx.notify();
            }
        }))
        .child(
            div()
                .id(SharedString::from(format!("album-card-open-{}", album.id)))
                .cursor_pointer()
                .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
                .child(
                    Cover::new(album.artwork.as_ref())
                        .size(TILE_COVER)
                        .rounded(cx.theme().radius * 2.)
                        .zoom(motion::id("album-card-cover", &album.id), hovered)
                        .placeholder(icons::DISC),
                )
                .child(
                    div()
                        .mt_3()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(album.title.clone())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(album.artist.clone())),
                        ),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("album-menu-slot-{}", album.id)))
                .absolute()
                .bottom_3()
                .right_3()
                // A hidden element takes its own open popup with it, so the fade stops at an open menu.
                .opacity(dots)
                // The menu button sits inside the card, whose own click opens the album.
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    Button::new(SharedString::from(format!("album-menu-{}", album.id)))
                        .custom(style::button(
                            style::black(0.4, cx),
                            style::white(style::HOVER, cx),
                            cx.theme().secondary_foreground,
                            cx,
                        ))
                        .icon(icons::icon(icons::MORE))
                        .rounded(cx.theme().radius_full())
                        .bg(style::black(0.4, cx))
                        .dropdown_menu(move |menu, window, cx| {
                            album_menu(menu, &menu_target, window, cx)
                        })
                        .on_open_change(cx.listener({
                            let id = album.id.clone();
                            move |this: &mut Albums, open: &bool, _, cx| {
                                this.menu_open = open.then(|| id.clone());
                                cx.notify();
                            }
                        })),
                ),
        )
        .context_menu(move |menu, window, cx| album_menu(menu, &target, window, cx))
}

/// The same card as `.view-list` restyles it: a compact row.
fn row(
    album: &AlbumInfo,
    target: MenuTarget,
    hovered: bool,
    window: &mut Window,
    cx: &mut Context<Albums>,
) -> impl IntoElement {
    let menu_target = target.clone();
    let route = Route::Album(album.id.clone().into());
    let wash = style::white(if hovered { style::HOVER } else { style::RAISED }, cx);
    let background = motion::color(motion::id("album-row-bg", &album.id), wash, window, cx);
    h_flex()
        .id(SharedString::from(format!("album-row-{}", album.id)))
        .w_full()
        .gap_3()
        .items_center()
        .px_3()
        .py_2()
        .rounded(style::radius_lg())
        .bg(background)
        .on_hover(cx.listener({
            let id = album.id.clone();
            move |this, hovered: &bool, _, cx| {
                // A leave for a row that is no longer the hovered one would clear its successor.
                if !*hovered && this.hovered.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.hovered = hovered.then(|| id.clone());
                cx.notify();
            }
        }))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .child(
            Cover::new(album.artwork.as_ref())
                .size(ROW_COVER)
                .rounded(cx.theme().radius * 1.5)
                .zoom(motion::id("album-row-cover", &album.id), hovered)
                .placeholder(icons::DISC),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(album.title.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(album.artist.clone())),
                ),
        )
        .when(album.year > 0, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(album.year.to_string()),
            )
        })
        .child(
            div()
                .id(SharedString::from(format!(
                    "album-row-menu-slot-{}",
                    album.id
                )))
                .ml_auto()
                .flex_shrink_0()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    Button::new(SharedString::from(format!("album-row-menu-{}", album.id)))
                        .ghost()
                        .icon(icons::icon(icons::MORE))
                        .dropdown_menu(move |menu, window, cx| {
                            album_menu(menu, &menu_target, window, cx)
                        }),
                ),
        )
        .context_menu(move |menu, window, cx| album_menu(menu, &target, window, cx))
}

/// What a card's menu acts on: the album itself, plus every same-titled album the cache holds.
#[derive(Clone)]
struct MenuTarget {
    id: String,
    // Named in the delete confirmation, so it says what goes rather than "this album".
    title: String,
    same_title: Vec<String>,
}

fn album_menu(
    menu: PopupMenu,
    target: &MenuTarget,
    _: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let capabilities = SourcesModel::global(cx).read(cx).capabilities();
    let mut menu = menu.item(
        PopupMenuItem::new(t("add_all_to_queue"))
            .icon(icons::icon(icons::QUEUE_ADD))
            .on_click({
                let id = target.id.clone();
                move |_, _, cx| ops::with_album_keys(id.clone(), cx, actions::append)
            }),
    );
    if capabilities.playlists != PlaylistCapability::None {
        menu = menu.item(
            PopupMenuItem::new(t("add_all_to_playlist"))
                .icon(icons::icon(icons::PLUS))
                .on_click({
                    let id = target.id.clone();
                    move |_, window, cx| ops::add_album_to_playlist(id.clone(), window, cx)
                }),
        );
    }
    let delete_label = if capabilities.delete_from_disk {
        t("delete_album")
    } else {
        t("remove_from_cache")
    };
    menu.separator().item(
        PopupMenuItem::element({
            let label = delete_label.clone();
            move |_, cx| {
                h_flex()
                    .gap_2()
                    .items_center()
                    // Erasing files should not look like the queue item above it.
                    .text_color(cx.theme().danger)
                    .child(icons::icon(icons::TRASH))
                    .child(label.clone())
                    .into_any_element()
            }
        })
        .on_click({
            let id = target.id.clone();
            let title = SharedString::from(target.title.clone());
            let same_title = target.same_title.clone();
            move |_, window, cx| {
                let id = id.clone();
                let same_title = same_title.clone();
                // Only the disk delete is irreversible; dropping cached rows re-syncs.
                if !capabilities.delete_from_disk {
                    for cached in &same_title {
                        ops::delete_album(cached.clone(), false, cx);
                    }
                    return;
                }
                dialogs::confirm_delete(
                    t("delete_album"),
                    title.clone(),
                    move |_, cx| ops::delete_album(id.clone(), true, cx),
                    window,
                    cx,
                );
            }
        }),
    )
}

fn title_key(album: &AlbumInfo) -> String {
    album.title.trim().to_lowercase()
}

fn view_mode(cx: &App) -> AlbumViewMode {
    ConfigModel::global(cx).read(cx).config().album_view_mode
}

fn set_view_mode(mode: AlbumViewMode, cx: &mut App) {
    ConfigModel::global(cx).update(cx, |model, cx| {
        model.update(cx, |config| config.album_view_mode = mode)
    });
}

fn criteria(cx: &App) -> Vec<SortCriterion<AlbumSortField>> {
    ConfigModel::global(cx).read(cx).config().album_sort.clone()
}

/// Make `field` decide the order, keeping the rest as tie-breakers.
fn set_sort_field(field: AlbumSortField, cx: &mut App) {
    let mut sorted = criteria(cx);
    let direction = sorted
        .first()
        .map(|criterion| criterion.direction)
        .unwrap_or_default();
    sorted.retain(|criterion| criterion.field != field);
    sorted.insert(0, SortCriterion::new(field, direction));
    write_criteria(sorted, cx);
}

fn set_sort_direction(direction: SortDirection, cx: &mut App) {
    let mut sorted = criteria(cx);
    match sorted.first_mut() {
        Some(criterion) => criterion.direction = direction,
        None => sorted.push(SortCriterion::new(AlbumSortField::Title, direction)),
    }
    write_criteria(sorted, cx);
}

fn write_criteria(sorted: Vec<SortCriterion<AlbumSortField>>, cx: &mut App) {
    ConfigModel::global(cx).update(cx, |model, cx| {
        model.update(cx, |config| config.album_sort = sorted)
    });
}
