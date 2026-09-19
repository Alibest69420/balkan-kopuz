//! The albums grid: every album the library holds, deduped by title.

use std::collections::HashMap;
use std::collections::HashSet;

use api::{AlbumInfo, AlbumPage, JobKind, PlaylistCapability, Table};
use config::{AlbumSortField, AlbumViewMode, SortCriterion, SortDirection};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window, div, px,
};
use store::{ConfigModel, JobsModel, Navigator, Query, Route, SourcesModel, actions};
use widgets::format::t;
use widgets::{Cover, EmptyState, Loading, PageHeader, album_tile, icons, tile_grid};

use crate::ops;
use crate::sort;

// TODO(offline): filtering the grid to downloaded albums waits on an offline seam the checklist flags as wrong.

// Tiles are a touch larger than the shared default, the way the Dioxus grid drew them.
const TILE: gpui_kit::Pixels = px(180.);

/// Every album, as tiles or as rows.
pub struct Albums {
    albums: Entity<Query<AlbumPage>>,
    scroll: ScrollHandle,
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
                cx.notify();
            }),
            cx.observe(&sources, |this, _, cx| {
                this.sync_empty_library(cx);
                cx.notify();
            }),
            cx.observe(&config, |_, _, cx| cx.notify()),
        ];
        Self {
            albums,
            scroll: ScrollHandle::new(),
            synced: false,
            _subscriptions: subscriptions,
        }
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

    fn view_toggle(&self, cx: &App) -> impl IntoElement {
        let mode = view_mode(cx);
        h_flex()
            .gap_1()
            .child(
                Button::new("albums-view-grid")
                    .ghost()
                    .icon(icons::icon(icons::GRID))
                    .tooltip(t("view_grid"))
                    .selected(mode == AlbumViewMode::Grid)
                    .on_click(|_, _, cx| set_view_mode(AlbumViewMode::Grid, cx)),
            )
            .child(
                Button::new("albums-view-list")
                    .ghost()
                    .icon(icons::icon(icons::ROWS))
                    .tooltip(t("view_list"))
                    .selected(mode == AlbumViewMode::List)
                    .on_click(|_, _, cx| set_view_mode(AlbumViewMode::List, cx)),
            )
    }

    fn sort_control(&self, albums: &[AlbumInfo], cx: &App) -> impl IntoElement {
        let available = sort::available_fields(albums);
        let primary = criteria(cx).first().copied();
        let label = match primary {
            Some(criterion) => SharedString::from(format!(
                "{}: {}",
                t("sort_by"),
                sort::label(criterion.field)
            )),
            None => t("sort_by"),
        };
        Button::new("albums-sort")
            .ghost()
            .icon(icons::icon(icons::SORT))
            .label(label)
            .dropdown_menu(move |mut menu, _, _| {
                for field in available.iter().copied() {
                    menu = menu.item(
                        PopupMenuItem::new(sort::label(field))
                            .checked(primary.is_some_and(|criterion| criterion.field == field))
                            .on_click(move |_, _, cx| set_sort_field(field, cx)),
                    );
                }
                menu = menu.separator();
                for direction in [SortDirection::Asc, SortDirection::Desc] {
                    menu = menu.item(
                        PopupMenuItem::new(sort::direction_label(direction))
                            .checked(
                                primary.is_some_and(|criterion| criterion.direction == direction),
                            )
                            .on_click(move |_, _, cx| set_sort_direction(direction, cx)),
                    );
                }
                menu
            })
    }
}

impl Render for Albums {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let all = self.all_albums(cx);
        let settled = self.albums.read(cx).is_settled();
        let grid = view_mode(cx) == AlbumViewMode::Grid;

        // A server splits one release across same-titled albums, so the grid shows each title once.
        let mut same_title: HashMap<String, Vec<String>> = HashMap::new();
        for album in &all {
            same_title
                .entry(title_key(album))
                .or_default()
                .push(album.id.clone());
        }
        let mut seen = HashSet::new();
        let mut visible: Vec<AlbumInfo> = all
            .iter()
            .filter(|album| seen.insert(title_key(album)))
            .cloned()
            .collect();
        sort::sort(&mut visible, &criteria(cx));

        let cards: Vec<AnyElement> = visible
            .iter()
            .map(|album| {
                let target = MenuTarget {
                    id: album.id.clone(),
                    same_title: same_title
                        .get(&title_key(album))
                        .cloned()
                        .unwrap_or_default(),
                };
                match grid {
                    true => card(album, target).into_any_element(),
                    false => row(album, target, cx).into_any_element(),
                }
            })
            .collect();

        v_flex()
            .size_full()
            .p_4()
            .child(
                PageHeader::new(t("all_albums"))
                    .action(self.view_toggle(cx))
                    .action(self.sort_control(&all, cx)),
            )
            .child(
                div()
                    .id("albums-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .map(|body| match (cards.is_empty(), settled) {
                        (true, false) => body.child(Loading),
                        (true, true) => {
                            body.child(EmptyState::new(t("no_albums_found")).icon(icons::DISC))
                        }
                        (false, _) if grid => body.child(tile_grid().children(cards)),
                        (false, _) => body.child(v_flex().w_full().children(cards)),
                    }),
            )
    }
}

fn card(album: &AlbumInfo, target: MenuTarget) -> impl IntoElement {
    let menu_target = target.clone();
    div()
        .id(SharedString::from(format!("album-card-{}", album.id)))
        .child(
            album_tile(album).size(TILE).overlay(
                div()
                    .id(SharedString::from(format!("album-menu-slot-{}", album.id)))
                    .absolute()
                    .bottom_2()
                    .right_2()
                    // The menu button sits inside the tile, whose own click opens the album.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        Button::new(SharedString::from(format!("album-menu-{}", album.id)))
                            .ghost()
                            .icon(icons::icon(icons::MORE))
                            .dropdown_menu(move |menu, window, cx| {
                                album_menu(menu, &menu_target, window, cx)
                            }),
                    ),
            ),
        )
        .context_menu(move |menu, window, cx| album_menu(menu, &target, window, cx))
}

fn row(album: &AlbumInfo, target: MenuTarget, cx: &App) -> impl IntoElement {
    let menu_target = target.clone();
    let route = Route::Album(album.id.clone().into());
    h_flex()
        .id(SharedString::from(format!("album-row-{}", album.id)))
        .w_full()
        .gap_3()
        .p_2()
        .rounded(cx.theme().radius)
        .hover(|row| row.bg(cx.theme().list_hover))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .child(
            Cover::new(album.artwork.as_ref())
                .size(px(48.))
                .placeholder(icons::DISC),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
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
        PopupMenuItem::new(delete_label)
            .icon(icons::icon(icons::TRASH))
            .on_click({
                let id = target.id.clone();
                let same_title = target.same_title.clone();
                move |_, _, cx| {
                    if capabilities.delete_from_disk {
                        ops::delete_album(id.clone(), true, cx);
                    } else {
                        for cached in &same_title {
                            ops::delete_album(cached.clone(), false, cx);
                        }
                    }
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
