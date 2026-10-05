//! The albums grid: every album the library holds; two albums sharing a title are two tiles.

use api::{AlbumInfo, AlbumPage, JobKind, Table};
use components::format::t;
use components::{
    EmptyState, Frame, GridMode, Loading, PageHeader, SortControl, TileGrid, album_row, album_tile,
    icons,
};
use config::{AlbumSortField, AlbumViewMode, SortCriterion};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled,
    Subscription, Window, div, px,
};
use store::{ConfigModel, JobsModel, Query, SourcesModel};

use crate::sort;

// TODO(offline): filtering the grid to downloaded albums waits on an offline seam the checklist flags as wrong.

// `grid-cols-[repeat(auto-fill,minmax(180px,1fr))]`: GPUI wraps flex children instead, at the column's minimum.
const TILE: gpui_kit::Pixels = px(180.);
// The tile's cover fills it inside `p-4`.
const TILE_COVER: gpui_kit::Pixels = px(148.);

/// Every album, as tiles or as rows.
pub struct Albums {
    albums: Entity<Query<AlbumPage>>,
    // Sorted once per change, since a card hover notifies and render would re-sort the library.
    visible: Vec<AlbumInfo>,
    sort_fields: Vec<AlbumSortField>,
    grid: TileGrid,
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
            sort_fields: Vec::new(),
            grid: TileGrid::new("albums-scroll"),
            synced: false,
            _subscriptions: subscriptions,
        }
    }

    /// Sort, and note which fields are worth offering — on change, not per frame.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let all = self.all_albums(cx);
        self.visible = all.clone();
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

    fn view_toggle(&self, cx: &App) -> impl IntoElement {
        components::view_toggle(
            "albums-view",
            view_mode(cx) == AlbumViewMode::Grid,
            |grid, cx| {
                let mode = match grid {
                    true => AlbumViewMode::Grid,
                    false => AlbumViewMode::List,
                };
                ConfigModel::edit(cx, |config| config.album_view_mode = mode);
            },
        )
    }

    fn sort_control(&self, cx: &App) -> impl IntoElement {
        SortControl::new("albums-sort", criteria(cx))
            .fields(
                self.sort_fields
                    .iter()
                    .map(|field| (*field, sort::label(*field))),
            )
            .on_change(|sorted, cx| ConfigModel::edit(cx, |config| config.album_sort = sorted))
    }
}

impl Render for Albums {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("albums");
        let settled = self.albums.read(cx).is_settled();
        let mode = match view_mode(cx) {
            AlbumViewMode::Grid => GridMode::Tiles,
            AlbumViewMode::List => GridMode::Rows,
        };
        let empty = self.visible.is_empty();
        let bands = (!empty).then(|| {
            self.grid
                .bands(
                    self.visible.len(),
                    TILE,
                    mode,
                    move |this: &mut Albums, index, _, cx| {
                        let album = &this.visible[index];
                        match mode {
                            GridMode::Tiles => album_tile(album)
                                .size(TILE_COVER)
                                .frame(Frame::Card)
                                .into_any_element(),
                            GridMode::Rows => album_row(album)
                                .thumb(store::ConfigModel::global(cx).read(cx).row_images())
                                .into_any_element(),
                        }
                    },
                )
                .build(window, cx)
        });

        components::page()
            .child(PageHeader::new(t("all_albums")))
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .mb_4()
                    .child(self.view_toggle(cx))
                    .child(self.sort_control(cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pb_8()
                    .map(|body| match (bands, settled) {
                        (Some(bands), _) => body.child(bands),
                        (None, false) => body.child(Loading::new()),
                        (None, true) => {
                            body.child(EmptyState::new(t("no_albums_found")).icon(icons::DISC))
                        }
                    }),
            )
    }
}

fn view_mode(cx: &App) -> AlbumViewMode {
    ConfigModel::global(cx).read(cx).config().album_view_mode
}

fn criteria(cx: &App) -> Vec<SortCriterion<AlbumSortField>> {
    ConfigModel::global(cx).read(cx).config().album_sort.clone()
}
