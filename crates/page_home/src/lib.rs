//! Home: the queue to pick back up, and the shelves the layout puts under it.

mod cards;
mod data;
mod resume;
mod settings;
mod shelf;

pub use settings::register;

use std::collections::HashMap;

use api::{
    AlbumPage, ArtistPage, JobKind, Page, StatsView, Table, TrackFilter, TrackPage, TrackSort,
};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use store::{JobsModel, PlaylistsModel, Query, SourcesModel};
use widgets::format::t;
use widgets::{EmptyState, icons};

use data::{RECENT_TRACKS_WINDOW, RECENTLY_ADDED_WINDOW, Shelf};

pub struct Home {
    albums: Entity<Query<AlbumPage>>,
    recently_added: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    recents: Entity<Query<TrackPage>>,
    // A ranking needs both the counts and the tracks they name.
    stats: Entity<Query<StatsView>>,
    played: Entity<Query<TrackPage>>,
    genre: Entity<Query<Option<String>>>,
    shelves: Vec<Shelf>,
    // One scroller per shelf, kept across rebuilds so new data does not throw a row back to its start.
    scroll: HashMap<&'static str, ScrollHandle>,
    page: ScrollHandle,
    seed: u64,
    // The empty-library sync is offered once per visit, not once per refetch.
    synced: bool,
    _subscriptions: Vec<Subscription>,
}

impl Home {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |client| async move { client.albums(store::all()).await },
                cx,
            )
        });
        let recently_added = cx.new(|cx| {
            Query::new(
                &[Table::Albums, Table::Tracks],
                |client| async move {
                    client
                        .albums_recently_added(page(RECENTLY_ADDED_WINDOW))
                        .await
                },
                cx,
            )
        });
        let artists = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |client| async move { client.artists(store::all()).await },
                cx,
            )
        });
        let recents = cx.new(|cx| {
            Query::new(
                &[Table::Recents],
                |client| async move { client.recent_tracks(page(RECENT_TRACKS_WINDOW)).await },
                cx,
            )
        });
        let stats = cx.new(|cx| {
            Query::new(
                &[Table::Recents],
                |client| async move { client.stats().await },
                cx,
            )
        });
        // The daemon can order by play count, so the most-played tracks are one bounded read.
        let played = cx.new(|cx| {
            Query::new(
                &[Table::Recents, Table::Tracks],
                |client| async move {
                    let filter = TrackFilter {
                        sort: TrackSort::PlayCount,
                        ..Default::default()
                    };
                    client.tracks(filter, page(PLAYED_WINDOW)).await
                },
                cx,
            )
        });
        let genre = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |client| async move { client.top_genre().await },
                cx,
            )
        });

        // Only what the shelves are derived from is observed: any notify at all redraws the page.
        let subscriptions = vec![
            cx.observe(&albums, |this, _, cx| this.rebuild(cx)),
            cx.observe(&recently_added, |this, _, cx| this.rebuild(cx)),
            cx.observe(&artists, |this, _, cx| this.rebuild(cx)),
            cx.observe(&recents, |this, _, cx| this.rebuild(cx)),
            cx.observe(&stats, |this, _, cx| this.rebuild(cx)),
            cx.observe(&played, |this, _, cx| this.rebuild(cx)),
            cx.observe(&genre, |this, _, cx| this.rebuild(cx)),
            cx.observe(&PlaylistsModel::global(cx), |this, _, cx| this.rebuild(cx)),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| this.rebuild(cx)),
        ];

        Self {
            albums,
            recently_added,
            artists,
            recents,
            stats,
            played,
            genre,
            shelves: Vec::new(),
            scroll: store::layout::SHELVES
                .iter()
                .map(|name| (*name, ScrollHandle::default()))
                .collect(),
            page: ScrollHandle::default(),
            seed: seed(),
            synced: false,
            _subscriptions: subscriptions,
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let playlists = PlaylistsModel::global(cx).read(cx).playlists().to_vec();
        self.shelves = data::build(data::Inputs {
            albums: self.albums.read(cx).value(),
            recently_added: self.recently_added.read(cx).value(),
            artists: self.artists.read(cx).value(),
            recents: self.recents.read(cx).value(),
            played: self.played.read(cx).value(),
            plays: self
                .stats
                .read(cx)
                .value()
                .map(|stats| &stats.listen_counts),
            genre: self.genre.read(cx).value().and_then(|genre| genre.as_ref()),
            playlists: &playlists,
            seed: self.seed,
        });
        self.start_empty_library_sync(cx);
        cx.notify();
    }

    /// A server fills an empty cache by syncing; a local library is populated by the scan instead.
    fn start_empty_library_sync(&mut self, cx: &mut Context<Self>) {
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

    /// The shelves the layout asks for, in its order, skipping the ones with nothing in them.
    fn drawn(&self, cx: &Context<Self>) -> Vec<usize> {
        store::layout(cx)
            .home
            .visible()
            .into_iter()
            .filter_map(|name| self.shelves.iter().position(|shelf| shelf.name == name))
            .filter(|index| !self.shelves[*index].tiles.is_empty())
            .collect()
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("home");
        widgets::scroll::smooth("home-scroll", &self.page, window, cx);
        let banner = resume::banner(window, cx);
        let drawn = self.drawn(cx);
        let mut rows: Vec<AnyElement> = Vec::with_capacity(drawn.len());
        for index in drawn {
            let shelf = &self.shelves[index];
            let Some(handle) = self.scroll.get(shelf.name).cloned() else {
                continue;
            };
            rows.push(shelf::shelf(shelf, &handle, window, cx));
        }
        // Nothing to show is only worth saying once the library has actually answered.
        let bare = banner.is_none() && rows.is_empty() && self.albums.read(cx).is_settled();
        v_flex()
            .id("home")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.page)
            .p_8()
            .pb_32()
            .gap_12()
            .children(banner)
            .children(rows)
            .when(bare, |page| {
                page.child(EmptyState::new(t("no_albums_found")).icon(icons::DISC))
            })
    }
}

/// How many of the most-played tracks are resolved to name their artists.
const PLAYED_WINDOW: u32 = 200;

fn page(limit: u32) -> Page {
    Page { offset: 0, limit }
}

/// A per-visit seed, so the shuffled rows keep their order while the page is open.
fn seed() -> u64 {
    use std::hash::{BuildHasher as _, Hasher as _};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}
