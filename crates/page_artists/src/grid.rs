//! The artists grid: every credited name in the library, with the photo the daemon found for it.

use std::collections::HashSet;
use std::time::Duration;

use api::{AlbumPage, ArtistInfo, ArtistPage, Table};
use config::{AlbumViewMode, ArtistSortField};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, UniformListScrollHandle, Window, div, px, uniform_list,
};
use store::{ConfigModel, Link, Navigator, Query, Route};
use widgets::format::t;
use widgets::{Cover, EmptyState, Loading, PageHeader, icons, motion, style};

use crate::controls::{self, ViewModeKey};
use crate::names;

const TILE: f32 = 160.;
// `gap-8` between tiles, so a column costs this much width.
const TILE_GAP: gpui_kit::Pixels = px(32.);
// Until the list has been laid out once there is no measured width to divide.
const TILE_COLUMNS_FALLBACK: usize = 5;
// Bands land one after another while a list scrolls; their missing photos travel as one request.
const PHOTO_BATCH: Duration = Duration::from_millis(150);
/// `group-hover:scale-110` on the photo.
const PHOTO_ZOOM: f32 = 1.10;
/// `duration-500`, slower than anything `widgets::motion` names.
const PHOTO_ZOOM_TIME: Duration = Duration::from_millis(500);

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
        window: &mut Window,
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
                        .map(|entry| tile(entry, window, cx).into_any_element()),
                )
                .into_any_element(),
            AlbumViewMode::List => div()
                .w_full()
                .children(
                    entries
                        .iter()
                        .map(|entry| row(entry, cx).into_any_element()),
                )
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
                            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                                range
                                    .map(|index| this.band(index, columns, mode, window, cx))
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

/// `rounded-full` photo that zooms under the pointer, and a centred label that goes accent.
fn tile(entry: &ArtistInfo, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let name = SharedString::from(entry.name.clone());
    let (hot, hover) = motion::hovered(
        SharedString::from(format!("artist-tile-hover-{}", entry.key)),
        window,
        cx,
    );
    let photo = motion::length(
        SharedString::from(format!("artist-photo-{}", entry.key)),
        px(if hot { TILE * PHOTO_ZOOM } else { TILE }),
        PHOTO_ZOOM_TIME,
        window,
        cx,
    );
    let label = motion::color(
        SharedString::from(format!("artist-label-{}", entry.key)),
        if hot {
            cx.theme().primary_hover
        } else {
            cx.theme().foreground
        },
        window,
        cx,
    );
    let route = Route::Artist(entry.key.clone());
    v_flex()
        .id(SharedString::from(format!("artist-tile-{}", entry.key)))
        .w(px(TILE))
        .gap_4()
        .items_center()
        .cursor_pointer()
        .child(
            // A transform is out of reach, so the photo grows inside a frame its own size that clips it.
            div()
                .size(px(TILE))
                .rounded_full()
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Cover::new(entry.artwork.as_ref())
                        .size(photo)
                        .rounded(photo / 2.)
                        .placeholder(icons::MIC),
                ),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .text_center()
                .font_weight(FontWeight::MEDIUM)
                .text_color(label)
                .child(name),
        )
        .on_hover(motion::track(hover))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
}

/// One artist as a row, for the list view.
fn row(entry: &ArtistInfo, cx: &App) -> impl IntoElement {
    let name = SharedString::from(entry.name.clone());
    let route = Route::Artist(entry.key.clone());
    h_flex()
        .id(SharedString::from(format!("artist-row-{}", entry.key)))
        .gap_3()
        .px_3()
        .py_2()
        .mb_2()
        .rounded(style::radius_lg())
        .bg(style::white(style::RAISED, cx))
        .hover(|row| row.bg(style::white(style::HOVER, cx)))
        .child(
            Cover::new(entry.artwork.as_ref())
                .size(px(48.))
                .rounded(px(24.))
                .placeholder(icons::MIC),
        )
        .child(div().flex_1().min_w_0().truncate().child(name.clone()))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
}
