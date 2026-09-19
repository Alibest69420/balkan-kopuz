//! Search: the genre browser, what a query turns up, and one genre's tracks.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use api::{AlbumInfo, AlbumPage, ApiError, ArtworkRef, KopuzApi, SearchResults, Table, TrackPage};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FontWeight, InteractiveElement as _,
    IntoElement, KeyBinding, NoAction, ObjectFit, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Subscription, Task, Window, div,
    img, linear_color_stop, linear_gradient, px, rems,
};
use store::{Navigator, PlayerModel, Query, Route, SourcesModel, actions, all};
use widgets::format::{t, t_with};
use widgets::{
    Cover, EmptyState, Loading, PageHeader, TrackActions, TrackList, TrackListEvent,
    TrackListOptions, icons, style,
};

const DEBOUNCE: Duration = Duration::from_millis(150);

// The shell binds bare keys such as space; a page that takes text has to unbind them for itself.
const INPUT_CONTEXT: &str = "KopuzSearchInput";

fn unbind_typing_keys(cx: &mut App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED
        .call_once(|| cx.bind_keys([KeyBinding::new("space", NoAction, Some(INPUT_CONTEXT))]));
}

type SearchAnswer = Pin<Box<dyn Future<Output = Result<SearchResults, ApiError>> + Send>>;

/// The read for one query; a blank one answers with nothing rather than asking the daemon.
fn search_fetch(query: String) -> impl Fn(Arc<dyn KopuzApi>) -> SearchAnswer + 'static {
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

/// Delete gated on the active source, where the Dioxus frontend always asked for the disk.
fn delete_tracks(keys: Vec<String>, cx: &mut App) {
    let from_disk = SourcesModel::global(cx)
        .read(cx)
        .capabilities()
        .delete_from_disk;
    actions::run(cx, move |api| async move {
        api.delete_tracks(keys, from_disk).await
    });
}

fn row_actions(cx: &App) -> TrackActions {
    TrackActions {
        delete: SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk,
        ..Default::default()
    }
}

/// `text-xl font-semibold text-white/80 mb-4`: the band above a result list.
fn section(title: SharedString, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .mb_4()
        .text_xl()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(style::white(0.8, cx))
        .child(title)
}

/// `aspect-video bg-white/5 border border-white/10 rounded-xl p-4`: one genre tile.
fn genre_tile(genre: &GenreTile, cx: &App) -> AnyElement {
    let name = genre.name.clone();
    let route = Route::Genre(name.clone());
    let hovered = style::white(style::HOVER, cx);
    let scrim = style::black(0.8, cx);
    div()
        .id(SharedString::from(format!("genre-{name}")))
        .relative()
        .overflow_hidden()
        .flex()
        .items_end()
        .aspect_ratio(16. / 9.)
        .p_4()
        .rounded(style::radius_xl())
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .bg(style::white(style::RAISED, cx))
        .hover(move |tile| tile.bg(hovered))
        .cursor_pointer()
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .when_some(genre.artwork.as_ref(), |tile, artwork| {
            tile.child(
                img(store::artwork_source(artwork, false))
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .child(div().absolute().inset_0().size_full().bg(linear_gradient(
                0.,
                linear_color_stop(scrim, 0.),
                linear_color_stop(scrim.opacity(0.), 0.5),
            )))
        })
        .child(
            div()
                .relative()
                .text_lg()
                .font_weight(FontWeight::BOLD)
                .child(name),
        )
        .into_any_element()
}

/// `p-4 bg-white/5 rounded-xl hover:bg-white/10`: one album a search turned up.
fn album_card(album: &AlbumInfo, cx: &App) -> AnyElement {
    let route = Route::Album(album.id.clone().into());
    let hovered = style::white(style::HOVER, cx);
    v_flex()
        .id(SharedString::from(format!("search-album-{}", album.id)))
        .w(px(180.))
        .p_4()
        .rounded(style::radius_xl())
        .bg(style::white(style::RAISED, cx))
        .hover(move |card| card.bg(hovered))
        .cursor_pointer()
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .child(
            div().mb_3().child(
                Cover::new(album.artwork.as_ref())
                    .size(px(148.))
                    .rounded(px(8.))
                    .placeholder(icons::DISC),
            ),
        )
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(album.title.clone()),
        )
        .child(
            div()
                .truncate()
                .text_sm()
                .text_color(style::slate_400(cx))
                .child(album.artist.clone()),
        )
        .into_any_element()
}

fn track_count(count: usize) -> SharedString {
    if count == 1 {
        t("track_count_singular")
    } else {
        t_with("track_count", &[("count", count.to_string())])
    }
}

/// One genre tile: the name, and the cover of the first album filed under it.
struct GenreTile {
    name: SharedString,
    artwork: Option<ArtworkRef>,
}

/// Genres are no listing of their own: they are the album rows' `genre` field, split and folded.
fn genres_of(albums: &[AlbumInfo]) -> Vec<GenreTile> {
    let mut covers: HashMap<&str, Option<&ArtworkRef>> = HashMap::new();
    for album in albums {
        for genre in album.genre.split(['/', ';', ',']) {
            let genre = genre.trim();
            if genre.is_empty() {
                continue;
            }
            let entry = covers.entry(genre).or_default();
            if entry.is_none() {
                *entry = album.artwork.as_ref();
            }
        }
    }
    let mut tiles: Vec<GenreTile> = covers
        .into_iter()
        .map(|(name, artwork)| GenreTile {
            name: name.to_string().into(),
            artwork: artwork.cloned(),
        })
        .collect();
    tiles.sort_by(|left, right| left.name.cmp(&right.name));
    tiles
}

/// Search: the genre browser when the query is blank, results when it is not.
pub struct Search {
    input: Entity<InputState>,
    results: Entity<Query<SearchResults>>,
    albums: Entity<Query<AlbumPage>>,
    tracks: Entity<TrackList>,
    query: SharedString,
    genres: Vec<GenreTile>,
    _debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Search {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        unbind_typing_keys(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t("search_placeholder")));
        let results = cx.new(|cx| {
            Query::new(
                &[Table::Tracks, Table::Albums],
                search_fetch(String::new()),
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
                .with_options(TrackListOptions::default())
                .with_actions(row_actions(cx))
        });
        let sources = SourcesModel::global(cx);

        let subscriptions = vec![
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.schedule(cx),
                InputEvent::PressEnter { .. } => this.apply(cx),
                _ => {}
            }),
            cx.observe(&results, |this, results, cx| {
                let rows = results
                    .read(cx)
                    .value()
                    .map(|results| results.tracks.clone())
                    .unwrap_or_default();
                this.tracks.update(cx, |list, cx| list.set_rows(rows, cx));
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
            cx.subscribe(&tracks, |this, list, event: &TrackListEvent, cx| {
                this.on_track_event(&list, event, cx)
            }),
            cx.observe(&sources, |this, _, cx| {
                let actions = row_actions(cx);
                this.tracks
                    .update(cx, |list, cx| list.set_actions(actions, cx));
            }),
        ];

        Self {
            input,
            results,
            albums,
            tracks,
            query: SharedString::default(),
            genres: Vec::new(),
            _debounce: None,
            _subscriptions: subscriptions,
        }
    }

    /// Typing restarts the timer; the task it replaces is dropped, which cancels it.
    fn schedule(&mut self, cx: &mut Context<Self>) {
        self._debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            this.update(cx, |this, cx| this.apply(cx)).ok();
        }));
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        self._debounce = None;
        let query = self.input.read(cx).value().trim().to_string();
        if query == self.query.as_ref() {
            return;
        }
        self.query = query.clone().into();
        let fetch = search_fetch(query.to_lowercase());
        self.results
            .update(cx, |results, cx| results.set_fetch(fetch, cx));
        cx.notify();
    }

    fn on_track_event(
        &mut self,
        list: &Entity<TrackList>,
        event: &TrackListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TrackListEvent::Play(index) => {
                let keys = list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
            TrackListEvent::Delete(keys) => delete_tracks(keys.clone(), cx),
            _ => {}
        }
    }

    fn bar(&self, cx: &App) -> impl IntoElement {
        div()
            .key_context(INPUT_CONTEXT)
            .flex_shrink_0()
            .w_full()
            .max_w(rems(42.))
            .mb_8()
            .child(
                Input::new(&self.input)
                    .prefix(icons::icon(icons::SEARCH).text_color(style::slate_500(cx)))
                    .cleanable(true)
                    .h_12()
                    .px_4()
                    .rounded_full()
                    .bg(style::white(style::HOVER, cx))
                    .border_1()
                    .border_color(style::white(style::BORDER, cx)),
            )
    }

    fn genre_browser(&self, cx: &Context<Self>) -> AnyElement {
        if self.genres.is_empty() {
            return if self.albums.read(cx).is_settled() {
                EmptyState::new(t("no_genres_found"))
                    .icon(icons::DISC)
                    .into_any_element()
            } else {
                Loading.into_any_element()
            };
        }
        let tiles: Vec<AnyElement> = self
            .genres
            .iter()
            .map(|genre| genre_tile(genre, cx))
            .collect();
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .mt_12()
            .child(section(t("browse_genres"), cx))
            .child(
                div()
                    .id("search-genres")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(div().grid().grid_cols(4).gap_4().children(tiles)),
            )
            .into_any_element()
    }

    fn results(&self, cx: &Context<Self>) -> AnyElement {
        let results = self.results.read(cx);
        let settled = results.is_settled();
        let has_tracks = results
            .value()
            .is_some_and(|value| !value.tracks.is_empty());
        let albums: Vec<AlbumInfo> = results
            .value()
            .map(|value| value.albums.clone())
            .unwrap_or_default();

        if !settled {
            return Loading.into_any_element();
        }
        if !has_tracks && albums.is_empty() {
            return EmptyState::new(t_with(
                "no_results_found",
                &[("query", self.query.to_string())],
            ))
            .icon(icons::SEARCH)
            .into_any_element();
        }

        let cards: Vec<AnyElement> = albums.iter().map(|album| album_card(album, cx)).collect();
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .mt_8()
            .when(has_tracks, |body| {
                body.child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .child(section(t("tracks"), cx))
                        .child(div().flex_1().min_h_0().child(self.tracks.clone())),
                )
            })
            .when(!albums.is_empty(), |body| {
                body.child(
                    v_flex()
                        // Under a track list the grid is a band of its own; alone it is the page.
                        .map(|grid| {
                            if has_tracks {
                                grid.flex_shrink_0().mt_12().max_h(px(320.))
                            } else {
                                grid.flex_1().min_h_0()
                            }
                        })
                        .child(section(t("albums"), cx))
                        .child(
                            div()
                                .id("search-albums")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .child(div().flex().flex_wrap().gap_4().children(cards)),
                        ),
                )
            })
            .into_any_element()
    }
}

impl Render for Search {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.query.trim().is_empty() {
            self.genre_browser(cx)
        } else {
            self.results(cx)
        };
        v_flex().size_full().p_8().child(self.bar(cx)).child(body)
    }
}

/// Every track of one genre.
pub struct Genre {
    name: SharedString,
    tracks: Entity<Query<TrackPage>>,
    list: Entity<TrackList>,
    _subscriptions: Vec<Subscription>,
}

impl Genre {
    pub fn new(name: SharedString, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let genre = name.to_string();
        let tracks = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                move |api| {
                    let genre = genre.clone();
                    async move { api.genre_tracks(genre, all()).await }
                },
                cx,
            )
        });
        let list = cx.new(|cx| {
            TrackList::new(cx)
                .with_options(TrackListOptions::default())
                .with_actions(row_actions(cx))
        });
        let sources = SourcesModel::global(cx);

        let subscriptions = vec![
            cx.observe(&tracks, |this, tracks, cx| {
                let rows = tracks
                    .read(cx)
                    .value()
                    .map(|page| page.items.clone())
                    .unwrap_or_default();
                this.list.update(cx, |list, cx| list.set_rows(rows, cx));
                cx.notify();
            }),
            cx.subscribe(&list, |_, list, event: &TrackListEvent, cx| match event {
                TrackListEvent::Play(index) => {
                    let keys = list.read(cx).keys();
                    actions::play_tracks(keys, Some(*index as u32), cx);
                }
                TrackListEvent::Delete(keys) => delete_tracks(keys.clone(), cx),
                _ => {}
            }),
            cx.observe(&sources, |this, _, cx| {
                let actions = row_actions(cx);
                this.list
                    .update(cx, |list, cx| list.set_actions(actions, cx));
            }),
        ];

        Self {
            name,
            tracks,
            list,
            _subscriptions: subscriptions,
        }
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        let keys = self.list.read(cx).keys();
        if keys.is_empty() {
            return;
        }
        actions::play_all(keys, cx);
    }

    /// Shuffle on, then play: `play_all` starts from a random row once it is.
    fn shuffle(&mut self, cx: &mut Context<Self>) {
        let keys = self.list.read(cx).keys();
        if keys.is_empty() {
            return;
        }
        PlayerModel::global(cx).update(cx, |player, cx| player.set_shuffle(true, cx));
        actions::play_all(keys, cx);
    }
}

impl Render for Genre {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.list.read(cx).rows().len();
        let settled = self.tracks.read(cx).is_settled();
        let body = if !settled {
            Loading.into_any_element()
        } else if count == 0 {
            EmptyState::new(t("no_tracks_found")).into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .child(self.list.clone())
                .into_any_element()
        };

        v_flex()
            .size_full()
            .p_8()
            .child(
                PageHeader::new(self.name.clone())
                    .subtitle(track_count(count))
                    .back(Route::Search)
                    .when(count > 0, |header| {
                        header
                            .action(
                                Button::new("genre-play")
                                    .primary()
                                    .icon(icons::icon(icons::PLAY))
                                    .label(t("play"))
                                    .on_click(cx.listener(|this, _, _, cx| this.play(cx))),
                            )
                            .action(
                                Button::new("genre-shuffle")
                                    .icon(icons::icon(icons::SHUFFLE))
                                    .label(t("shuffle"))
                                    .on_click(cx.listener(|this, _, _, cx| this.shuffle(cx))),
                            )
                    }),
            )
            .child(body)
    }
}
