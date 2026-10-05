//! Catalog shelves: the carousels, tiles and list rows the landing and the remote artist page share.

use std::collections::HashSet;

use api::{CatalogDetailRequest, CatalogItem, CatalogItemKind, CatalogShelf};
use components::format::t;
use components::{
    Arrows, Carousel, PlayState, ShelfHeading, Tile, TrackActions, TrackList, TrackListEvent,
    TrackListOptions, heading_link, icons, style,
};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Task,
    Window, div, px,
};
use store::{Link, Navigator, Notices, PlayerModel, Route, SourcesModel, Target, actions};

use crate::states::catalog_failure;

const TILE: Pixels = px(176.);

struct ShelfView {
    shelf: CatalogShelf,
    scroll: ScrollHandle,
    list: Option<Entity<TrackList>>,
}

/// The shelves of one catalog page, with the tile that started playback remembered.
pub struct Shelves {
    views: Vec<ShelfView>,
    // The catalog id or track key whose tile owns the pause overlay.
    now_playing: Option<String>,
    pending: Option<String>,
    _play: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Shelves {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let player = PlayerModel::global(cx);
        Self {
            views: Vec::new(),
            now_playing: None,
            pending: None,
            _play: None,
            _subscriptions: vec![cx.observe(&player, |_, _, cx| cx.notify())],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.views.clear();
        cx.notify();
    }

    pub fn set(&mut self, shelves: Vec<CatalogShelf>, cx: &mut Context<Self>) {
        self.views.clear();
        self.extend(shelves, cx);
    }

    pub fn extend(&mut self, shelves: Vec<CatalogShelf>, cx: &mut Context<Self>) {
        for shelf in shelves {
            let list = shelf.list.then(|| self.track_list(&shelf, cx));
            self.views.push(ShelfView {
                shelf,
                scroll: ScrollHandle::new(),
                list,
            });
        }
        cx.notify();
    }

    fn track_list(&mut self, shelf: &CatalogShelf, cx: &mut Context<Self>) -> Entity<TrackList> {
        let rows = shelf
            .items
            .iter()
            .filter_map(|item| item.track.clone())
            .collect();
        let list = cx.new(|cx| {
            let mut list = TrackList::new(cx)
                .with_options(TrackListOptions {
                    show_index: true,
                    show_cover: true,
                    show_album: false,
                    track_numbers: false,
                    selectable: false,
                })
                .with_actions(TrackActions::default());
            list.set_rows(rows, cx);
            list
        });
        self._subscriptions.push(
            cx.subscribe(&list, |this, list, event: &TrackListEvent, cx| {
                if let TrackListEvent::Play(index) = event {
                    let keys = list.read(cx).keys();
                    actions::play_tracks(keys, Some(*index as u32), cx);
                    // A song preview is nobody's tile, so no card keeps the pause overlay.
                    this.now_playing = None;
                    cx.notify();
                }
            }),
        );
        list
    }

    // TODO(prefetch): §2.3's 250 ms hover prefetch cache is not ported; every play pages fresh.
    /// Play everything behind a catalog id: the first page starts the queue, the rest append.
    pub fn play(&mut self, kind: CatalogItemKind, id: String, cx: &mut Context<Self>) {
        self.now_playing = Some(id.clone());
        self.pending = Some(id.clone());
        cx.notify();
        self._play = Some(cx.spawn(async move |this, cx| {
            let mut seen = HashSet::new();
            let mut cursor: Option<String> = None;
            let mut started = false;
            loop {
                let request = CatalogDetailRequest {
                    continuation: cursor.clone(),
                    ..CatalogDetailRequest::new(kind, id.clone())
                };
                let Ok(call) = this.update(cx, |_, cx| {
                    Link::global(cx)
                        .call(move |api| async move { api.catalog_detail(request).await })
                }) else {
                    return;
                };
                let detail = match call.await {
                    Ok(detail) => detail,
                    Err(error) => {
                        this.update(cx, |this, cx| {
                            if started {
                                Notices::error(catalog_failure(&error), cx);
                            } else {
                                this.now_playing = None;
                            }
                            this.pending = None;
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                };
                let fresh: Vec<String> = detail
                    .tracks
                    .into_iter()
                    .map(|track| track.key)
                    .filter(|key| seen.insert(key.clone()))
                    .collect();
                if !fresh.is_empty() {
                    let first = !started;
                    started = true;
                    if this
                        .update(cx, |this, cx| {
                            if first {
                                actions::play_tracks(fresh, None, cx);
                                this.pending = None;
                                cx.notify();
                            } else {
                                actions::append(fresh, cx);
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                match detail.continuation {
                    // A source that hands back the cursor it was given would page forever.
                    Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
                    _ => break,
                }
            }
            this.update(cx, |this, cx| {
                this.pending = None;
                if !started {
                    this.now_playing = None;
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn toggle_or_play(&mut self, kind: CatalogItemKind, id: String, cx: &mut Context<Self>) {
        if self.pending.as_deref() == Some(id.as_str()) {
            return;
        }
        if self.now_playing.as_deref() == Some(id.as_str()) {
            PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
            return;
        }
        self.play(kind, id, cx);
    }

    fn section(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = &self.views[index];
        match &view.list {
            Some(list) => self.list_section(index, list.clone(), cx),
            None => self.carousel_section(index, cx),
        }
    }

    fn carousel_section(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = &self.views[index];
        let page = cx.entity();
        Carousel::new(
            SharedString::from(format!("catalog-shelf-{index}")),
            view.shelf.title.clone(),
            view.shelf.items.len(),
            TILE,
            // The carousel builds a tile once it is in view, after this render has handed the page back.
            move |at, _, cx| {
                page.update(cx, |this, cx| {
                    this.tile(index, &this.views[index].shelf.items[at], cx)
                })
            },
        )
        .strapline(view.shelf.strapline.clone())
        .arrows(Arrows::Always)
        .grow_arrows(true)
        .handle(view.scroll.clone())
        .into_any_element()
    }

    fn list_section(
        &self,
        index: usize,
        list: Entity<TrackList>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.views[index];
        let title = SharedString::from(view.shelf.title.clone());
        let rows = list.read(cx).rows().len();
        let mut heading = ShelfHeading::new(title.clone());
        if let Some(more) = view.shelf.more_ref.clone() {
            heading = heading.trailing(
                heading_link(("show-all", index), t("discover_show_all"), cx).on_click(
                    move |_, _, cx| {
                        Navigator::navigate(
                            Route::DiscoverPlaylist {
                                id: more.clone().into(),
                                title: title.clone(),
                            },
                            cx,
                        )
                    },
                ),
            );
        }
        v_flex()
            .w_full()
            .gap_6()
            .child(heading)
            .child(
                div()
                    .w_full()
                    .h(style::media_row() * rows as f32)
                    .child(list),
            )
            .into_any_element()
    }

    fn tile(&self, shelf: usize, item: &CatalogItem, cx: &mut Context<Self>) -> AnyElement {
        let tile = Tile::new(element_id(shelf, &item.id), item.title.clone())
            .subtitle(item.subtitle.clone().unwrap_or_default())
            .artwork(item.artwork.as_ref())
            .size(TILE);
        match item.kind {
            CatalogItemKind::Track => self.song_tile(tile, item, cx),
            CatalogItemKind::Album | CatalogItemKind::Playlist => {
                self.playable_tile(tile, item, cx)
            }
            // An artist tile's id is the key the daemon minted for it.
            CatalogItemKind::Artist => tile
                .placeholder(icons::MIC)
                .round(true)
                .tint(true)
                .route(Route::DiscoverArtist(item.id.clone()))
                .menu(Some(Target::Artist {
                    key: item.id.clone(),
                    name: item.title.clone().into(),
                }))
                .into_any_element(),
            CatalogItemKind::Mood | CatalogItemKind::Unknown => tile.into_any_element(),
        }
    }

    fn playable_tile(&self, tile: Tile, item: &CatalogItem, cx: &mut Context<Self>) -> AnyElement {
        let id = item.id.clone();
        let kind = item.kind;
        let album = kind == CatalogItemKind::Album;
        let route = match album {
            true => Route::Album(id.clone().into()),
            false => Route::DiscoverPlaylist {
                id: id.clone().into(),
                title: item.title.clone().into(),
            },
        };
        tile.placeholder(if album { icons::DISC } else { icons::LIST })
            .route(route)
            // A catalog playlist is nobody's saved list, so only an album carries the album menu.
            .menu(album.then(|| Target::Album {
                id: id.clone().into(),
                title: item.title.clone().into(),
            }))
            .play(
                self.play_state(&id, cx),
                cx.listener(move |this, _, _, cx| this.toggle_or_play(kind, id.clone(), cx)),
            )
            .into_any_element()
    }

    fn song_tile(&self, tile: Tile, item: &CatalogItem, cx: &mut Context<Self>) -> AnyElement {
        let Some(track) = item.track.as_ref() else {
            return tile.into_any_element();
        };
        let key = track.key.clone();
        let radio = SourcesModel::global(cx).read(cx).capabilities().track_radio;
        let start = move |this: &mut Self, cx: &mut Context<Self>| {
            if this.now_playing.as_deref() == Some(key.as_str()) {
                PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                return;
            }
            this.now_playing = Some(key.clone());
            if radio {
                actions::start_track_radio(key.clone(), cx);
            } else {
                actions::play_tracks(vec![key.clone()], None, cx);
            }
            cx.notify();
        };
        let press = start.clone();
        tile.menu(Some(Target::track(track.key.clone(), track.title.clone())))
            .on_click(cx.listener(move |this, _, _, cx| start(this, cx)))
            .play(
                self.play_state(&track.key, cx),
                cx.listener(move |this, _, _, cx| press(this, cx)),
            )
            .into_any_element()
    }

    fn play_state(&self, id: &str, cx: &Context<Self>) -> PlayState {
        if self.pending.as_deref() == Some(id) {
            return PlayState::Loading;
        }
        match self.now_playing.as_deref() == Some(id)
            && PlayerModel::global(cx).read(cx).is_playing()
        {
            true => PlayState::Playing,
            false => PlayState::Idle,
        }
    }
}

impl Render for Shelves {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sections = Vec::with_capacity(self.views.len());
        for index in 0..self.views.len() {
            sections.push(self.section(index, cx));
        }
        v_flex().w_full().gap_12().children(sections)
    }
}

/// A catalog id is unique only inside its shelf, and shelves only ever append.
fn element_id(shelf: usize, id: &str) -> SharedString {
    SharedString::from(format!("{shelf}-{id}"))
}
