//! Discover: the browsing destination. The source's catalog, the library's own genres, and what a query turned up.

mod detail;
mod genres;
mod results;
mod shelves;
mod states;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use api::{AlbumInfo, AlbumPage, ApiError, KopuzApi, SearchResults, Table};
use components::format::{t, t_with};
use components::{
    Chip, ChipSize, EmptyState, GridMode, Heading, Loading, PageHeader, SearchTrigger,
    SectionHeading, TileGrid, TrackList, TrackListEvent, TriggerSize, heading_link, icons,
};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    Keystroke, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, px,
};
use store::{Chrome, Link, PlayerModel, Query, SourcesModel, actions, all};

use crate::genres::{GenreCard, GenreTile, genres_of};
use crate::shelves::Shelves;
use store::Playing;

pub use detail::{CatalogArtist, CatalogPlaylist};
pub use genres::Genre;

// The Dioxus sentinel's root margin: how close to the end a scroll asks for the next page.
const PAGE_MARGIN: Pixels = px(600.);
/// How many genres the landing shows before it offers the lot.
const LANDING_GENRES: usize = 12;
/// Results that answer the previous query stay up at this opacity rather than blanking.
const FADED: f32 = 0.55;

type Answer = Pin<Box<dyn Future<Output = Result<SearchResults, ApiError>> + Send>>;

/// The read behind one query; a blank one answers with nothing rather than asking the daemon.
fn search_fetch(query: String) -> impl Fn(Arc<dyn KopuzApi>) -> Answer + 'static {
    move |api| {
        let query = query.clone();
        Box::pin(async move {
            if query.is_empty() {
                return Ok(SearchResults::default());
            }
            api.search(query).await
        })
    }
}

/// The browsing destination: the catalog and the genres when nothing is asked for, results when something is.
pub struct Discover {
    // The source's own feed.
    shelves: Entity<Shelves>,
    continuation: Option<String>,
    loading: bool,
    loading_more: bool,
    settled: bool,
    catalog_error: Option<SharedString>,
    scroll: ScrollHandle,

    // The library's genres, folded out of its album rows.
    albums: Entity<Query<AlbumPage>>,
    genres: Vec<GenreTile>,
    all_genres: bool,
    genre_bands: TileGrid,

    // What the palette put in the box, and what came back for it.
    query: SharedString,
    results: Entity<Query<SearchResults>>,
    tracks: Entity<TrackList>,
    result_albums: Vec<AlbumInfo>,
    result_artists: Vec<(String, SharedString)>,
    playing: Playing,

    _load: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Discover {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = Chrome::global(cx).read(cx).search().clone();
        let results = cx.new(|cx| {
            Query::new(
                &[Table::Tracks, Table::Albums],
                search_fetch(query.trim().to_lowercase()),
                cx,
            )
        });
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |api| async move { api.albums(all()).await },
                cx,
            )
        });
        let tracks = cx.new(|cx| {
            TrackList::new(cx)
                .with_options(components::TrackListOptions::default())
                .with_actions(genres::row_actions(cx))
        });
        commands::bind_track_list(&tracks, cx);
        let chrome = Chrome::global(cx);
        let sources = SourcesModel::global(cx);
        let player = PlayerModel::global(cx);

        let subscriptions = vec![
            cx.observe(&chrome, |this, chrome, cx| {
                let asked = chrome.read(cx).search().clone();
                if asked == this.query {
                    return;
                }
                this.query = asked;
                let fetch = search_fetch(this.query.trim().to_lowercase());
                this.results
                    .update(cx, |results, cx| results.set_fetch(fetch, cx));
                cx.notify();
            }),
            cx.observe(&results, |this, results, cx| {
                let (tracks, albums) = match results.read(cx).value() {
                    Some(found) => (found.tracks.clone(), found.albums.clone()),
                    None => (Vec::new(), Vec::new()),
                };
                this.result_artists = results::artists_of(&tracks, &albums);
                this.result_albums = albums;
                this.tracks.update(cx, |list, cx| list.set_rows(tracks, cx));
                cx.notify();
            }),
            cx.observe(&albums, |this, albums, cx| {
                this.genres = albums
                    .read(cx)
                    .value()
                    .map(|page| genres_of(&page.albums))
                    .unwrap_or_default();
                cx.notify();
            }),
            cx.subscribe(&tracks, |_, list, event: &TrackListEvent, cx| match event {
                TrackListEvent::Play(index) => {
                    let keys = list.read(cx).keys();
                    actions::play_tracks(keys, Some(*index as u32), cx);
                }
                TrackListEvent::Delete(keys) => genres::delete_tracks(keys.clone(), cx),
                _ => {}
            }),
            // Capabilities arrive with the source list, so the first catalog load follows that answer.
            cx.observe(&sources, |this, _, cx| {
                let actions = genres::row_actions(cx);
                this.tracks
                    .update(cx, |list, cx| list.set_actions(actions, cx));
                this.start(cx);
            }),
            // The player notifies on every tick, so only a change of what is marked is worth a frame.
            cx.observe(&player, |this, player, cx| {
                let marked = player.read(cx).track().map(Playing::of);
                let marked = marked.unwrap_or_default();
                if marked == this.playing {
                    return;
                }
                this.playing = marked;
                cx.notify();
            }),
        ];

        let mut page = Self {
            shelves: cx.new(Shelves::new),
            continuation: None,
            loading: false,
            loading_more: false,
            settled: false,
            catalog_error: None,
            scroll: ScrollHandle::new(),
            albums,
            genres: Vec::new(),
            all_genres: false,
            genre_bands: TileGrid::new("discover-genre-grid").gap(genres::TILE_GAP),
            query,
            results,
            tracks,
            result_albums: Vec::new(),
            result_artists: Vec::new(),
            playing: Playing::default(),
            _load: None,
            _subscriptions: subscriptions,
        };
        page.start(cx);
        page
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let supported = SourcesModel::global(cx).read(cx).capabilities().discover;
        if self.loading || self.settled || !supported {
            return;
        }
        self.load(None, cx);
    }

    /// What Retry does: forget the failed page and ask for the feed from the top.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.catalog_error = None;
        self.continuation = None;
        self.settled = false;
        self.shelves.update(cx, |shelves, cx| shelves.clear(cx));
        self.start(cx);
    }

    fn load(&mut self, continuation: Option<String>, cx: &mut Context<Self>) {
        if continuation.is_some() {
            self.loading_more = true;
        } else {
            self.loading = true;
        }
        cx.notify();
        let call = Link::global(cx).call(move |api| async move { api.catalog(continuation).await });
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                this.loading_more = false;
                this.settled = true;
                match result {
                    Ok(page) => {
                        this.continuation = page.continuation;
                        this.catalog_error = None;
                        let shelves = page.shelves;
                        this.shelves.update(cx, |held, cx| held.extend(shelves, cx));
                    }
                    Err(error) => this.catalog_error = Some(states::catalog_failure(&error)),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Ask for the next page once the end of the feed is within reach.
    fn page_on_scroll(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.loading_more {
            return;
        }
        let Some(continuation) = self.continuation.clone() else {
            return;
        };
        let scrolled = -self.scroll.offset().y;
        if self.scroll.max_offset().y - scrolled > PAGE_MARGIN {
            return;
        }
        self.load(Some(continuation), cx);
    }

    fn show_all_genres(&mut self, all: bool, cx: &mut Context<Self>) {
        self.all_genres = all;
        cx.notify();
    }

    fn clear_search(&mut self, cx: &mut Context<Self>) {
        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_search("", cx));
    }
}

impl Discover {
    /// The chip that says where typing goes: the same palette the top bar's box opens.
    fn search_hint(&self, cx: &App) -> impl IntoElement {
        let shortcut = commands::keystroke_for("app.palette", cx)
            .and_then(|held| Keystroke::parse(&held).ok());
        SearchTrigger::new("discover-search-hint", t("discover_search_hint"))
            .size(TriggerSize::Compact)
            .shortcut(shortcut)
    }

    /// The genres the landing offers, and the link to the rest of them.
    fn genre_shelf(&self, cx: &mut Context<Self>) -> AnyElement {
        let more = self.genres.len() > LANDING_GENRES;
        let cards: Vec<AnyElement> = self
            .genres
            .iter()
            .take(LANDING_GENRES)
            .map(|tile| GenreCard::new(tile).into_any_element())
            .collect();
        let mut heading = SectionHeading::new(t("browse_genres")).heading(Heading::Band);
        if more {
            heading = heading.trailing(
                heading_link("discover-all-genres", t("discover_show_all"))
                    .on_click(cx.listener(|this, _, _, cx| this.show_all_genres(true, cx))),
            );
        }
        v_flex()
            .w_full()
            .gap_3()
            .child(heading)
            .child(div().flex().flex_wrap().gap_4().children(cards))
            .into_any_element()
    }

    /// The landing: what this source offers to browse, and what the library files its albums under.
    fn landing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        components::scroll::smooth("discover-scroll", &self.scroll, window, cx);
        let sources = SourcesModel::global(cx);
        let loaded = sources.read(cx).is_loaded();
        let supported = sources.read(cx).capabilities().discover;
        let genres = !self.genres.is_empty();
        let (settled, read_failed) = {
            let held = self.albums.read(cx);
            (held.is_settled(), held.error().map(states::read_failure))
        };
        let waiting = !loaded || (self.loading && self.shelves.read(cx).is_empty());

        // A library read that failed is not an empty library, so it says so and offers the retry.
        if let Some(message) = read_failed.filter(|_| !genres) {
            return components::page()
                .child(PageHeader::new(t("discover")).action(self.search_hint(cx)))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .justify_center()
                        .child(EmptyState::failed(
                            message,
                            cx.listener(|this, _, _, cx| {
                                this.albums.update(cx, |albums, cx| albums.refresh(cx));
                            }),
                        )),
                )
                .into_any_element();
        }

        // Nothing to browse at all: the one place to go next is the box the hint points at.
        if loaded && settled && !supported && !genres && self.catalog_error.is_none() {
            return components::page()
                .child(PageHeader::new(t("discover")).action(self.search_hint(cx)))
                .child(
                    div().flex_1().min_h_0().child(
                        EmptyState::new(t("discover_unsupported"))
                            .detail(t("discover_search_hint"))
                            .icon(icons::COMPASS),
                    ),
                )
                .into_any_element();
        }

        let shelf = genres.then(|| self.genre_shelf(cx));
        let failure = self.catalog_error.clone();
        div()
            .id("discover")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.page_on_scroll(cx)))
            .child(
                v_flex()
                    .w_full()
                    .px_8()
                    .pt_8()
                    .pb_10()
                    .gap_6()
                    .child(PageHeader::new(t("discover")).action(self.search_hint(cx)))
                    .children(shelf)
                    .when_some(failure, |body, message| {
                        body.child(EmptyState::failed(
                            message,
                            cx.listener(|this, _, _, cx| this.reload(cx)),
                        ))
                    })
                    .when(waiting, |body| body.child(Loading::new()))
                    .child(self.shelves.clone())
                    .when(self.loading_more, |body| {
                        body.child(Loading::new().label(t("discover_more_loading")).compact())
                    }),
            )
            .into_any_element()
    }

    /// Every genre, as a grid that builds a screenful rather than the library.
    fn genre_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let bands = self
            .genre_bands
            .bands(
                self.genres.len(),
                genres::TILE,
                GridMode::Tiles,
                |this: &mut Discover, index, _, _| {
                    GenreCard::new(&this.genres[index]).into_any_element()
                },
            )
            .build(window, cx);
        components::page()
            .child(
                PageHeader::new(t("browse_genres"))
                    .subtitle(t_with(
                        "discover_genre_count",
                        &[("count", self.genres.len().to_string())],
                    ))
                    .action(
                        heading_link("discover-fewer-genres", t("discover_show_less")).on_click(
                            cx.listener(|this, _, _, cx| this.show_all_genres(false, cx)),
                        ),
                    ),
            )
            .child(div().flex_1().min_h_0().pb_8().child(bands))
            .into_any_element()
    }

    /// The chip that puts the query back where it came from.
    fn clear_chip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Chip::new("discover-clear-search")
            .icon(icons::CLOSE)
            .label(t("discover_clear_search"))
            .size(ChipSize::Medium)
            .on_click(cx.listener(|this, _, _, cx| this.clear_search(cx)))
    }

    /// What one query turned up: the strips over the list of matching tracks.
    fn results_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (failure, settled, stale) = {
            let held = self.results.read(cx);
            (
                held.error().map(states::read_failure),
                held.is_settled(),
                held.is_stale(),
            )
        };
        let found = self.tracks.read(cx).rows().len();
        let nothing = found == 0 && self.result_albums.is_empty();

        let body = if let Some(message) = failure {
            EmptyState::failed(
                message,
                cx.listener(|this, _, _, cx| {
                    this.results.update(cx, |results, cx| results.refresh(cx));
                }),
            )
            .into_any_element()
        } else if !settled {
            Loading::new().into_any_element()
        } else if nothing {
            div()
                .flex_1()
                .min_h_0()
                .child(
                    EmptyState::new(t_with(
                        "no_results_found",
                        &[("query", self.query.to_string())],
                    ))
                    .detail(t("discover_no_results_hint"))
                    .icon(icons::SEARCH),
                )
                .into_any_element()
        } else {
            let albums = results::album_strip(&self.result_albums, t("albums"), &self.playing);
            let artists = results::artist_strip(&self.result_artists, t("artists"), &self.playing);
            v_flex()
                .flex_1()
                .min_h_0()
                // The held rows answer the previous query: dim them rather than blanking the page.
                .when(stale, |body| body.opacity(FADED))
                .children(albums)
                .children(artists)
                .when(found > 0, |body| {
                    body.child(
                        div()
                            .flex_shrink_0()
                            .mb_4()
                            .child(SectionHeading::new(t("tracks")).heading(Heading::Band)),
                    )
                    .child(div().flex_1().min_h_0().pb_8().child(self.tracks.clone()))
                })
                .into_any_element()
        };

        components::page()
            .child(
                PageHeader::new(t_with(
                    "discover_results_for",
                    &[("query", self.query.to_string())],
                ))
                .action(self.clear_chip(cx)),
            )
            .child(body)
            .into_any_element()
    }
}

impl Render for Discover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("discover");
        if !self.query.trim().is_empty() {
            return self.results_view(window, cx);
        }
        if self.all_genres {
            return self.genre_grid(window, cx);
        }
        self.landing(window, cx)
    }
}
