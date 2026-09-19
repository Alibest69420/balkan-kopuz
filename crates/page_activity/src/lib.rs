//! Activity: the library's most-played tracks, with the play counts the daemon keeps.

use std::collections::HashMap;
use std::ops::Range;

use api::{Page, QueueContext, StatsView, Table, TrackFilter, TrackInfo, TrackPage, TrackSort};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use store::{Query, actions};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, Loading, PageHeader, icons};

// The Dioxus list's row height; virtualized rows need one resolved value.
const ROW_HEIGHT: Pixels = px(60.);
const COVER: Pixels = px(40.);
const INDEX_WIDTH: Pixels = px(40.);
const ALBUM_WIDTH: Pixels = px(200.);
const GENRE_WIDTH: Pixels = px(120.);
const TIME_WIDTH: Pixels = px(72.);
const PLAYS_WIDTH: Pixels = px(72.);

const BUFFER: usize = 10;
// Windows snap to this, so scrolling within one asks the daemon for nothing.
const CHUNK: usize = 50;

/// The listing this page reads, and the queue a row click starts.
fn most_played() -> TrackFilter {
    TrackFilter {
        sort: TrackSort::PlayCount,
        ..Default::default()
    }
}

/// Most-played tracks, windowed: only the visible rows are read, the counts come from `stats`.
pub struct Activity {
    rows: Vec<TrackInfo>,
    row_offset: usize,
    total: usize,
    settled: bool,
    requested: Option<Page>,
    tracks: Entity<Query<TrackPage>>,
    genres: Entity<Query<HashMap<String, String>>>,
    stats: Entity<Query<StatsView>>,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl Activity {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let first = Page {
            offset: 0,
            limit: CHUNK as u32,
        };
        let tracks = cx.new(|cx| {
            Query::new(
                &[Table::Tracks, Table::Recents],
                move |api| async move { api.tracks(most_played(), first).await },
                cx,
            )
        });
        let genres = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |api| async move {
                    api.albums(store::all()).await.map(|page| {
                        page.albums
                            .into_iter()
                            .map(|album| (album.id, album.genre))
                            .collect()
                    })
                },
                cx,
            )
        });
        let stats = cx.new(|cx| {
            Query::new(
                &[Table::Tracks, Table::Recents],
                |api| async move { api.stats().await },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&tracks, |this, _, cx| this.adopt(cx)),
            cx.observe(&genres, |_, _, cx| cx.notify()),
            cx.observe(&stats, |_, _, cx| cx.notify()),
        ];
        Self {
            rows: Vec::new(),
            row_offset: 0,
            total: 0,
            settled: false,
            requested: Some(first),
            tracks,
            genres,
            stats,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Take whatever the windowed read answered; a window in flight keeps the rows on screen.
    fn adopt(&mut self, cx: &mut Context<Self>) {
        let query = self.tracks.read(cx);
        self.settled |= query.is_settled();
        if let Some(page) = query.value() {
            self.total = page.total as usize;
            self.row_offset = page.offset as usize;
            self.rows = page.items.clone();
        }
        cx.notify();
    }

    fn window_for(&self, range: &Range<usize>) -> Page {
        let start = range.start.saturating_sub(BUFFER) / CHUNK * CHUNK;
        let end = (range.end + BUFFER)
            .div_ceil(CHUNK)
            .saturating_mul(CHUNK)
            .min(self.total.max(range.end));
        Page {
            offset: start as u32,
            limit: end.saturating_sub(start).max(1) as u32,
        }
    }

    /// Ask for the rows the viewport needs, once per window rather than once per frame.
    fn ensure_window(&mut self, range: &Range<usize>, window: &Window, cx: &mut Context<Self>) {
        let page = self.window_for(range);
        if self.requested == Some(page) {
            return;
        }
        self.requested = Some(page);
        cx.defer_in(window, move |this, _, cx| {
            this.tracks.update(cx, |query, cx| {
                query.set_fetch(
                    move |api| async move { api.tracks(most_played(), page).await },
                    cx,
                )
            });
        });
    }

    fn plays(&self, track: &TrackInfo, cx: &Context<Self>) -> u64 {
        self.stats
            .read(cx)
            .value()
            .and_then(|stats| stats.listen_counts.get(&track.uid))
            .copied()
            .unwrap_or_default()
    }

    fn genre(&self, track: &TrackInfo, cx: &Context<Self>) -> SharedString {
        self.genres
            .read(cx)
            .value()
            .and_then(|genres| genres.get(&track.album_id))
            .filter(|genre| !genre.is_empty())
            .map(|genre| SharedString::from(genre.clone()))
            .unwrap_or_else(|| "-".into())
    }

    fn row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(track) = index
            .checked_sub(self.row_offset)
            .and_then(|offset| self.rows.get(offset))
        else {
            return div().h(ROW_HEIGHT).into_any_element();
        };
        let plays = self.plays(track, cx);
        let genre = self.genre(track, cx);
        let length = track
            .duration_secs()
            .map(|secs| duration(secs * 1000))
            .unwrap_or_else(|| "--:--".into());
        let muted = cx.theme().muted_foreground;

        div()
            .id(SharedString::from(track.uid.clone()))
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .rounded(cx.theme().radius)
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(cx.listener(move |_, _, _, cx| {
                actions::play_context(
                    QueueContext::Filter {
                        filter: most_played(),
                    },
                    Some(index as u32),
                    cx,
                );
            }))
            .child(
                div()
                    .w(INDEX_WIDTH)
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .child((index + 1).to_string()),
            )
            .child(Cover::new(track.artwork.as_ref()).size(COVER))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .child(SharedString::from(track.title.clone())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(track.artist.clone())),
                    ),
            )
            .child(
                div()
                    .w(ALBUM_WIDTH)
                    .flex_shrink_0()
                    .truncate()
                    .text_sm()
                    .text_color(muted)
                    .child(SharedString::from(track.album.clone())),
            )
            .child(
                div()
                    .w(GENRE_WIDTH)
                    .flex_shrink_0()
                    .truncate()
                    .text_sm()
                    .text_color(muted)
                    .child(genre),
            )
            .child(
                div()
                    .w(TIME_WIDTH)
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .text_right()
                    .child(length),
            )
            .child(
                h_flex()
                    .w(PLAYS_WIDTH)
                    .flex_shrink_0()
                    .gap_1()
                    .justify_end()
                    .text_sm()
                    .text_color(muted)
                    .when(plays > 0, |cell| {
                        cell.text_color(cx.theme().foreground).child(
                            icons::icon(icons::ACTIVITY)
                                .size(px(12.))
                                .text_color(cx.theme().warning),
                        )
                    })
                    .child(plays.to_string()),
            )
            .into_any_element()
    }

    fn columns(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .gap_3()
            .px_2()
            .pb_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(div().w(INDEX_WIDTH).flex_shrink_0().child("#"))
            .child(div().w(COVER).flex_shrink_0())
            .child(div().flex_1().min_w_0().child(t("title")))
            .child(div().w(ALBUM_WIDTH).flex_shrink_0().child(t("album")))
            .child(div().w(GENRE_WIDTH).flex_shrink_0().child(t("genre")))
            .child(
                div()
                    .w(TIME_WIDTH)
                    .flex_shrink_0()
                    .text_right()
                    .child(t("time")),
            )
            .child(
                div()
                    .w(PLAYS_WIDTH)
                    .flex_shrink_0()
                    .text_right()
                    .child(t("plays")),
            )
    }
}

impl Render for Activity {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.total;
        v_flex()
            .size_full()
            .p_4()
            .child(PageHeader::new(t("listening_logs")).subtitle(t("most_played_tracks")))
            .child(self.columns(cx))
            .child(if count == 0 {
                if self.settled {
                    EmptyState::new(t("no_tracks_in_library"))
                        .icon(icons::ACTIVITY)
                        .into_any_element()
                } else {
                    Loading.into_any_element()
                }
            } else {
                uniform_list(
                    "activity-rows",
                    count,
                    cx.processor(move |this, range: Range<usize>, window, cx| {
                        this.ensure_window(&range, window, cx);
                        range.map(|index| this.row(index, cx)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .size_full()
                .into_any_element()
            })
    }
}
