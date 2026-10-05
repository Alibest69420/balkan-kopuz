//! Genres: the tiles the library's own tags make, and the page one of them opens.

use std::collections::HashMap;

use api::{AlbumInfo, ArtworkRef, Table, TrackPage};
use components::format::{t, t_with};
use components::{
    Cover, EmptyState, Loading, PageHeader, Scrim, TrackActions, TrackList, TrackListEvent,
    TrackListOptions, icons, motion, style,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window, div, px,
};
use store::{Navigator, PlayerModel, Query, Route, SourcesModel, actions, all};

use crate::states;

/// One genre tile, wide enough for a name and a cover behind it.
pub const TILE: Pixels = px(240.);
pub const TILE_GAP: Pixels = px(16.);
// `aspect-video`, stated rather than derived: a virtualized band has to know its height up front.
const TILE_HEIGHT: Pixels = px(135.);

/// The card's channels, keyed off its name.
const HOVER: u64 = 1;
const WASH: u64 = 2;
const COVER: u64 = 3;

/// One genre tile: the name, and the cover of the first album filed under it.
pub struct GenreTile {
    pub name: SharedString,
    pub artwork: Option<ArtworkRef>,
}

/// Genres are no listing of their own: they are the album rows' `genre` field, split and folded.
pub fn genres_of(albums: &[AlbumInfo]) -> Vec<GenreTile> {
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

/// `aspect-video bg-white/5 border border-white/10 rounded-xl p-4`: one genre, as a card.
#[derive(IntoElement)]
pub struct GenreCard {
    name: SharedString,
    artwork: Option<ArtworkRef>,
}

impl GenreCard {
    pub fn new(tile: &GenreTile) -> Self {
        Self {
            name: tile.name.clone(),
            artwork: tile.artwork.clone(),
        }
    }
}

impl RenderOnce for GenreCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = SharedString::from(format!("genre-{}", self.name));
        let route = Route::Genre(self.name.clone());
        profile::elements::built();
        let (hovered, pointer) = motion::hovered(motion::channel(&key, HOVER), window, cx);
        let wash = motion::color(
            motion::channel(&key, WASH),
            style::white(if hovered { style::HOVER } else { style::RAISED }, cx),
            window,
            cx,
        );
        div()
            .id(key.clone())
            .relative()
            .overflow_hidden()
            .flex()
            .items_end()
            .w(TILE)
            .h(TILE_HEIGHT)
            .flex_shrink_0()
            .p_4()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .bg(wash)
            .cursor_pointer()
            .on_hover(motion::track(pointer))
            .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
            .when_some(self.artwork, |tile, artwork| {
                tile.child(
                    Cover::new(Some(&artwork))
                        .fill()
                        .zoom(motion::channel(&key, COVER), hovered)
                        .zoom_factor(motion::ZOOM_LG)
                        .scrim(Scrim {
                            foot: 0.8,
                            top: 0.,
                            reach: 0.5,
                        }),
                )
            })
            .child(
                div()
                    .relative()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .child(self.name),
            )
    }
}

fn track_count(count: usize) -> SharedString {
    if count == 1 {
        t("track_count_singular")
    } else {
        t_with("track_count", &[("count", count.to_string())])
    }
}

pub(crate) fn row_actions(cx: &App) -> TrackActions {
    TrackActions {
        delete: SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk,
        ..Default::default()
    }
}

/// Delete gated on the active source, where the Dioxus frontend always asked for the disk.
pub(crate) fn delete_tracks(keys: Vec<String>, cx: &mut App) {
    let from_disk = SourcesModel::global(cx)
        .read(cx)
        .capabilities()
        .delete_from_disk;
    actions::run(cx, move |api| async move {
        api.delete_tracks(keys, from_disk).await
    });
}

/// Every track of one genre.
pub struct Genre {
    name: SharedString,
    tracks: Entity<Query<TrackPage>>,
    list: Entity<TrackList>,
    count: usize,
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
                this.count = rows.len();
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
            count: 0,
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
        let _timing = profile::render("genre");
        let count = self.count;
        let failure = self.tracks.read(cx).error().map(states::read_failure);
        let settled = self.tracks.read(cx).is_settled();
        let body = if let Some(message) = failure {
            states::failed(
                "genre-retry",
                message,
                cx.listener(|this, _, _, cx| {
                    this.tracks.update(cx, |tracks, cx| tracks.refresh(cx));
                }),
                cx,
            )
        } else if !settled {
            Loading.into_any_element()
        } else if count == 0 {
            EmptyState::new(t("no_tracks_found"))
                .icon(icons::DISC)
                .into_any_element()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn album(genre: &str) -> AlbumInfo {
        AlbumInfo {
            genre: genre.to_string(),
            ..AlbumInfo::default()
        }
    }

    #[test]
    fn a_multi_genre_tag_becomes_one_tile_each() {
        let albums = [album("Rock / Post-Rock; Shoegaze")];
        let tiles = genres_of(&albums);
        let names: Vec<&str> = tiles.iter().map(|tile| tile.name.as_ref()).collect();
        assert_eq!(names, ["Post-Rock", "Rock", "Shoegaze"]);
    }

    #[test]
    fn the_same_genre_on_two_albums_is_one_tile() {
        let albums = [album("Jazz"), album("Jazz,"), album("")];
        assert_eq!(genres_of(&albums).len(), 1);
    }

    #[test]
    fn a_library_that_tags_nothing_has_no_genres() {
        let albums = [album(""), album("  ,  ")];
        assert!(genres_of(&albums).is_empty());
    }
}
