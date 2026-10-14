//! Home: the queue to pick back up, and the shelves the layout puts under it.

mod cards;
mod data;
mod ranked;
mod resume;
mod settings;
mod shelf;

pub use settings::register;

use std::collections::HashMap;

use api::{AlbumPage, ArtistPage, Page, StatsView, Table, TrackFilter, TrackPage, TrackSort};
use components::format::t;
use components::{EmptyState, PageHeader, TitleSize, icons, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window,
};
use store::layout::Ordered as _;
use store::{Navigator, PlaylistsModel, Query, Route, SourcesModel};

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
    _subscriptions: Vec<Subscription>,
}

impl Home {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |client| async move { client.albums(Default::default(), store::all()).await },
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
                &[Table::Tracks, Table::Artists],
                |client| async move { client.artists(Default::default(), store::all()).await },
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
        cx.notify();
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
        components::scroll::smooth("home-scroll", &self.page, window, cx);
        let banner = resume::banner(cx);
        let drawn = self.drawn(cx);
        let mut rows: Vec<AnyElement> = Vec::with_capacity(drawn.len());
        for index in drawn {
            let shelf = &self.shelves[index];
            let Some(handle) = self.scroll.get(shelf.name).cloned() else {
                continue;
            };
            // A ranking is a list and a shelf is a row; `Form` is what says which a name gets.
            rows.push(match shelf.form {
                data::Form::Ranked => ranked::ranked(shelf, cx),
                _ => shelf::shelf(shelf, &handle),
            });
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
            .child(heading(cx))
            .children(banner)
            .children(rows)
            .when(bare, |page| {
                page.child(EmptyState::new(t("no_albums_found")).icon(icons::DISC))
            })
    }
}

/// `text-4xl font-black` "Home", and the pill that opens where the shelves are arranged.
fn heading(cx: &App) -> AnyElement {
    PageHeader::new(t("home"))
        .size(TitleSize::Display)
        .flush()
        .action(
            Button::new("home-customize")
                .custom(style::quiet_button(
                    style::white(style::TEXT_STRONG, cx),
                    cx,
                ))
                .small()
                .rounded_full()
                .border_1()
                .border_color(style::white(style::BORDER, cx))
                .icon(icons::icon(icons::SETTINGS))
                .label(t("customize_home"))
                .on_click(|_, _, cx| {
                    Navigator::navigate(
                        Route::Settings {
                            section: Some(SharedString::new_static("home")),
                        },
                        cx,
                    )
                }),
        )
        .into_any_element()
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
