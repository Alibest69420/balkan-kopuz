//! The artists grid: every credited name in the library, with the photo the daemon found for it.

use std::time::Duration;

use api::{AlbumPage, ArtistPage, Table, TrackPage};
use config::{AlbumViewMode, ArtistSortField};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Window, div, px,
};
use store::{ConfigModel, Link, Navigator, Query, Route};
use widgets::format::t;
use widgets::{Cover, EmptyState, Loading, PageHeader, icons, motion, style};

use crate::controls::{self, ViewModeKey};
use crate::names::{self, ArtistEntry};

const TILE: f32 = 160.;
/// `group-hover:scale-110` on the photo.
const PHOTO_ZOOM: f32 = 1.10;
/// `duration-500`, slower than anything `widgets::motion` names.
const PHOTO_ZOOM_TIME: Duration = Duration::from_millis(500);

/// The artists grid.
pub struct Artists {
    albums: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    samples: Entity<Query<TrackPage>>,
    scroll: ScrollHandle,
    // What the last photo request asked for; the answer dirties Tracks, which would ask again forever.
    asked: Vec<String>,
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
        let samples = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |api| async move { api.artist_sample_tracks(store::all()).await },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&albums, |this, _, cx| this.reread(cx)),
            cx.observe(&artists, |this, _, cx| this.reread(cx)),
            cx.observe(&samples, |this, _, cx| this.reread(cx)),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
        ];
        Self {
            albums,
            artists,
            samples,
            scroll: ScrollHandle::new(),
            asked: Vec::new(),
            _refresh: None,
            _subscriptions: subscriptions,
        }
    }

    fn reread(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.request_photos(cx);
    }

    /// Ask the daemon to fill in the photos this grid is missing; it announces what it finds.
    fn request_photos(&mut self, cx: &mut Context<Self>) {
        let names = {
            let (Some(albums), Some(samples)) =
                (self.albums.read(cx).value(), self.samples.read(cx).value())
            else {
                return;
            };
            if albums.albums.is_empty() && samples.items.is_empty() {
                return;
            }
            names::photo_queue(&albums.albums, &samples.items)
        };
        if names.is_empty() || names == self.asked {
            return;
        }
        self.asked = names.clone();
        let call = Link::global(cx)
            .call(move |api| async move { api.refresh_artist_artwork(names).await });
        self._refresh = Some(cx.spawn(async move |_, _| {
            if let Err(error) = call.await {
                tracing::debug!(%error, "artist artwork refresh failed");
            }
        }));
    }

    fn is_settled(&self, cx: &App) -> bool {
        self.albums.read(cx).is_settled() && self.samples.read(cx).is_settled()
    }

    fn entries(&self, cx: &App) -> Vec<ArtistEntry> {
        let albums = self
            .albums
            .read(cx)
            .value()
            .map(|page| page.albums.as_slice())
            .unwrap_or_default();
        let samples = self
            .samples
            .read(cx)
            .value()
            .map(|page| page.items.as_slice())
            .unwrap_or_default();
        let artists = self
            .artists
            .read(cx)
            .value()
            .map(|page| page.artists.as_slice())
            .unwrap_or_default();
        // TODO(offline): filter to artists whose tracks are downloaded when offline.
        names::grid_entries(albums, samples, artists, &controls::criteria(cx))
    }
}

impl Render for Artists {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth("artists-scroll", &self.scroll, window, cx);
        let entries = self.entries(cx);
        let settled = self.is_settled(cx);
        let mode = controls::config(cx).artists_view_mode;

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
            .child(match (settled, entries.is_empty()) {
                (false, true) => Loading.into_any_element(),
                (true, true) => EmptyState::new(t("artists"))
                    .icon(icons::MIC)
                    .detail(t("no_tracks_found"))
                    .into_any_element(),
                _ => div()
                    .id("artists-scroll")
                    .flex_1()
                    .min_h_0()
                    .pb_20()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(match mode {
                        AlbumViewMode::Grid => grid(&entries, window, cx),
                        AlbumViewMode::List => list(&entries, cx),
                    })
                    .into_any_element(),
            })
    }
}

fn grid(entries: &[ArtistEntry], window: &mut Window, cx: &mut App) -> gpui_kit::AnyElement {
    let tiles: Vec<_> = entries
        .iter()
        .map(|entry| tile(entry, window, cx).into_any_element())
        .collect();
    div()
        .flex()
        .flex_wrap()
        .gap_8()
        .children(tiles)
        .into_any_element()
}

/// `rounded-full` photo that zooms under the pointer, and a centred label that goes accent.
fn tile(entry: &ArtistEntry, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let name = SharedString::from(entry.name.clone());
    let (hot, hover) = controls::hovered(
        SharedString::from(format!("artist-tile-hover-{}", entry.name)),
        window,
        cx,
    );
    let photo = motion::length(
        SharedString::from(format!("artist-photo-{}", entry.name)),
        px(if hot { TILE * PHOTO_ZOOM } else { TILE }),
        PHOTO_ZOOM_TIME,
        window,
        cx,
    );
    let label = motion::color(
        SharedString::from(format!("artist-label-{}", entry.name)),
        if hot {
            cx.theme().primary_hover
        } else {
            cx.theme().foreground
        },
        window,
        cx,
    );
    let route = Route::Artist {
        name: name.clone(),
        channel: None,
    };
    v_flex()
        .id(SharedString::from(format!("artist-tile-{}", entry.name)))
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
        .on_hover(controls::track(hover))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
}

fn list(entries: &[ArtistEntry], cx: &App) -> gpui_kit::AnyElement {
    v_flex()
        .gap_2()
        .children(entries.iter().map(|entry| {
            let name = SharedString::from(entry.name.clone());
            h_flex()
                .id(SharedString::from(format!("artist-row-{}", entry.name)))
                .gap_3()
                .px_3()
                .py_2()
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
                .on_click(move |_, _, cx| {
                    Navigator::navigate(
                        Route::Artist {
                            name: name.clone(),
                            channel: None,
                        },
                        cx,
                    )
                })
        }))
        .into_any_element()
}
