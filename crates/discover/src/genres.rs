//! Genres: the tiles the library's own tags make, and the page one of them opens.

use std::collections::HashMap;
use std::rc::Rc;

use api::{AlbumInfo, ArtworkRef, Table, TrackPage};
use components::format::{t, t_with};
use components::{
    DetailActions, EmptyState, Loading, Look, PageHeader, TrackActions, TrackList, TrackListEvent,
    TrackListOptions, icons,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Subscription, Window, div,
};
use store::{Query, SourcesModel, actions, all};

use crate::states;

/// One genre tile: the name, and the cover of the first album filed under it.
pub struct GenreTile {
    pub name: SharedString,
    pub artwork: Option<ArtworkRef>,
}

/// The card for one tile.
pub fn genre_card(tile: &GenreTile) -> components::GenreCard {
    components::GenreCard::new(tile.name.clone(), tile.artwork.clone())
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

fn track_count(count: usize) -> SharedString {
    t_with("track_count", &[("count", count.into())])
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
    // The rows' keys, taken once per answer rather than per frame.
    keys: Rc<[String]>,
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
        commands::bind_track_list(&list, cx);
        let sources = SourcesModel::global(cx);

        let subscriptions = vec![
            cx.observe(&tracks, |this, tracks, cx| {
                let rows = tracks
                    .read(cx)
                    .value()
                    .map(|page| page.items.clone())
                    .unwrap_or_default();
                this.keys = rows.iter().map(|row| row.key.clone()).collect();
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
            keys: Rc::from([]),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Genre {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("genre");
        let count = self.keys.len();
        let failure = self.tracks.read(cx).error().map(states::read_failure);
        let settled = self.tracks.read(cx).is_settled();
        let body = if let Some(message) = failure {
            EmptyState::failed(
                message,
                cx.listener(|this, _, _, cx| {
                    this.tracks.update(cx, |tracks, cx| tracks.refresh(cx));
                }),
            )
            .into_any_element()
        } else if !settled {
            Loading::new().into_any_element()
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

        components::page()
            .pb_8()
            .child(
                PageHeader::new(self.name.clone())
                    .subtitle(track_count(count))
                    .when(count > 0, |header| {
                        header.action(
                            DetailActions::new("genre", self.keys.clone())
                                .look(Look::Compact)
                                .play(commands::play_all)
                                .shuffle_play(commands::shuffle_play),
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
