//! The artists grid: every credited name in the library, with the photo the daemon found for it.

use std::collections::HashSet;
use std::time::Duration;

use api::{AlbumPage, ArtistInfo, ArtistPage, Table};
use config::{AlbumViewMode, ArtistSortField};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels,
    Render, Styled as _, Subscription, Task, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use store::{ConfigModel, Link, Query};
use widgets::format::t;
use widgets::{EmptyState, Loading, PageHeader, icons};

use crate::controls::{self, ViewModeKey};
use crate::names;

const TILE: f32 = 160.;
// `gap-8` between tiles, so a column costs this much width.
const TILE_GAP: gpui_kit::Pixels = px(32.);
// Until the list has been laid out once there is no measured width to divide.
const TILE_COLUMNS_FALLBACK: usize = 5;
// Bands land one after another while a list scrolls; their missing photos travel as one request.
const PHOTO_BATCH: Duration = Duration::from_millis(150);

/// The artists grid.
pub struct Artists {
    albums: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    scroll: UniformListScrollHandle,
    // Built on change, not per frame: render runs on every playhead tick.
    entries: Vec<ArtistInfo>,
    // Every artist a photo was asked for, so the Tracks refresh its answer causes asks nothing again.
    asked: HashSet<String>,
    // Shown without a photo and not yet sent, waiting out the batch window.
    pending: Vec<String>,
    _refresh: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Artists {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |api| async move { api.albums(store::all()).await },
                cx,
            )
        });
        let artists = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |api| async move { api.artists(store::all()).await },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&albums, |this, _, cx| this.reread(cx)),
            cx.observe(&artists, |this, _, cx| this.reread(cx)),
            // The sort and the view mode live in config, so a change there reorders the grid.
            cx.observe(&ConfigModel::global(cx), |this, _, cx| this.reread(cx)),
        ];
        Self {
            albums,
            artists,
            scroll: UniformListScrollHandle::new(),
            entries: Vec::new(),
            asked: HashSet::new(),
            pending: Vec::new(),
            _refresh: None,
            _subscriptions: subscriptions,
        }
    }

    fn reread(&mut self, cx: &mut Context<Self>) {
        self.entries = self.build_entries(cx);
        cx.notify();
    }

    /// Queue the photos a band on screen lacks; only what is seen is ever looked up, and once.
    fn want_photos(&mut self, entries: &[ArtistInfo], cx: &mut Context<Self>) {
        let missing = entries
            .iter()
            .filter(|artist| artist.artwork.is_none())
            .filter(|artist| self.asked.insert(artist.key.clone()))
            .map(|artist| artist.key.clone());
        let before = self.pending.len();
        self.pending.extend(missing);
        if before == 0 && !self.pending.is_empty() {
            self._refresh = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(PHOTO_BATCH).await;
                let Ok(wanted) = this.update(cx, |this, _| std::mem::take(&mut this.pending))
                else {
                    return;
                };
                let call = cx.update(|cx| {
                    Link::global(cx)
                        .call(move |api| async move { api.refresh_artist_artwork(wanted).await })
                });
                if let Err(error) = call.await {
                    tracing::debug!(%error, "artist artwork refresh failed");
                }
            }));
        }
    }

    fn is_settled(&self, cx: &App) -> bool {
        self.albums.read(cx).is_settled() && self.artists.read(cx).is_settled()
    }

    /// How many tiles fit across, from the width the list reported at its last layout.
    fn tile_columns(&self) -> usize {
        let width: Pixels = self.scroll.0.borrow().base_handle.bounds().size.width;
        if width <= px(0.) {
            return TILE_COLUMNS_FALLBACK;
        }
        let pitch = px(TILE) + TILE_GAP;
        (((width + TILE_GAP) / pitch) as usize).max(1)
    }

    /// One virtualized band: a row of tiles in grid mode, a single artist in list mode.
    fn band(
        &mut self,
        index: usize,
        columns: usize,
        mode: AlbumViewMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first = index * columns;
        let last = (first + columns).min(self.entries.len());
        let entries = self.entries[first..last].to_vec();
        self.want_photos(&entries, cx);
        match mode {
            AlbumViewMode::Grid => div()
                .flex()
                .w_full()
                .items_start()
                .gap_8()
                .pb_8()
                .children(
                    entries
                        .iter()
                        .map(|entry| widgets::artist_tile(entry).size(px(TILE))),
                )
                .into_any_element(),
            AlbumViewMode::List => div()
                .w_full()
                .children(entries.iter().map(widgets::artist_row))
                .into_any_element(),
        }
    }

    fn build_entries(&self, cx: &App) -> Vec<ArtistInfo> {
        let albums = self
            .albums
            .read(cx)
            .value()
            .map(|page| page.albums.as_slice())
            .unwrap_or_default();
        let artists = self
            .artists
            .read(cx)
            .value()
            .map(|page| page.artists.as_slice())
            .unwrap_or_default();
        // TODO(offline): filter to artists whose tracks are downloaded when offline.
        names::grid_entries(albums, artists, &controls::criteria(cx))
    }
}

impl Render for Artists {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("artists");
        widgets::scroll::smooth_list("artists-scroll", &self.scroll, window, cx);
        let settled = self.is_settled(cx);
        let mode = controls::config(cx).artists_view_mode;
        let empty = self.entries.is_empty();
        // A band of tiles is the virtualized unit, so the grid builds a screenful of the library.
        let columns = match mode {
            AlbumViewMode::Grid => self.tile_columns(),
            AlbumViewMode::List => 1,
        };
        let rows = self.entries.len().div_ceil(columns.max(1));

        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .child(
                PageHeader::new(t("artists"))
                    .action(controls::view_mode_toggle(ViewModeKey::Artists, window, cx))
                    .action(controls::sort_control(
                        "artist-sort",
                        vec![
                            ArtistSortField::Name,
                            ArtistSortField::Tracks,
                            ArtistSortField::Albums,
                        ],
                        cx,
                    )),
            )
            .child(match (settled, empty) {
                (false, true) => Loading.into_any_element(),
                (true, true) => EmptyState::new(t("artists"))
                    .icon(icons::MIC)
                    .detail(t("no_tracks_found"))
                    .into_any_element(),
                _ => div()
                    .flex_1()
                    .min_h_0()
                    .pb_20()
                    .child(
                        uniform_list(
                            "artists-scroll",
                            rows,
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|index| this.band(index, columns, mode, cx))
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .size_full(),
                    )
                    .into_any_element(),
            })
    }
}
