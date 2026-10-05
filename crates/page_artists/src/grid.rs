//! The artists grid: every credited name in the library, with the photo the daemon found for it.

use std::collections::HashSet;
use std::ops::Range;
use std::time::Duration;

use api::{AlbumPage, ArtistInfo, ArtistPage, Table};
use components::format::t;
use components::{EmptyState, GridMode, Loading, PageHeader, TileGrid, icons};
use config::{AlbumViewMode, ArtistSortField};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, div, px,
};
use store::{ConfigModel, Link, Query};

use crate::controls::{self, ViewModeKey};
use crate::names;

const TILE: f32 = 160.;
// Bands land one after another while a list scrolls; their missing photos travel as one request.
const PHOTO_BATCH: Duration = Duration::from_millis(150);

/// The artists grid.
pub struct Artists {
    albums: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    grid: TileGrid,
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
            // `gap-8` between round tiles.
            grid: TileGrid::new("artists-scroll").gap(px(32.)).fallback(5),
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

    /// Queue the photos the bands on screen lack; only what is seen is ever looked up, and once.
    fn want_photos(&mut self, shown: Range<usize>, cx: &mut Context<Self>) {
        let Some(entries) = self.entries.get(shown) else {
            return;
        };
        let missing: Vec<String> = entries
            .iter()
            .filter(|artist| artist.artwork.is_none())
            .filter(|artist| self.asked.insert(artist.key.clone()))
            .map(|artist| artist.key.clone())
            .collect();
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
        let settled = self.is_settled(cx);
        let mode = match controls::config(cx).artists_view_mode {
            AlbumViewMode::Grid => GridMode::Tiles,
            AlbumViewMode::List => GridMode::Rows,
        };
        let empty = self.entries.is_empty();

        let body = match (settled, empty) {
            (false, true) => Loading::new().into_any_element(),
            (true, true) => EmptyState::new(t("artists"))
                .icon(icons::MIC)
                .detail(t("no_tracks_found"))
                .into_any_element(),
            _ => div()
                .flex_1()
                .min_h_0()
                .pb_20()
                .child(
                    self.grid
                        .bands(
                            self.entries.len(),
                            px(TILE),
                            mode,
                            move |this: &mut Artists, index, _, _| {
                                let artist = &this.entries[index];
                                match mode {
                                    GridMode::Tiles => components::artist_tile(artist)
                                        .size(px(TILE))
                                        .into_any_element(),
                                    GridMode::Rows => {
                                        components::artist_row(artist).into_any_element()
                                    }
                                }
                            },
                        )
                        .visit(|this: &mut Artists, shown, _, cx| this.want_photos(shown, cx))
                        .build(window, cx),
                )
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .child(
                PageHeader::new(t("artists"))
                    .action(controls::view_mode_toggle(ViewModeKey::Artists, cx))
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
            .child(body)
    }
}
