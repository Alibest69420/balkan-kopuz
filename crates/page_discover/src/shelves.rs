//! Catalog shelves: the carousels, tiles and list rows Discover and the remote artist page share.

use std::collections::HashSet;

use api::{ApiError, CatalogDetailRequest, CatalogItem, CatalogItemKind, CatalogShelf, ErrorCode};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, point, px,
};
use store::{Link, Navigator, Notices, PlayerModel, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{Tile, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons};

const TILE: Pixels = px(176.);
// One ‹ or › press, matching the Dioxus frontend's 800 px nudge.
const STEP: Pixels = px(800.);
// The shared track list's row height, which a list shelf has to reserve up front.
const ROW: Pixels = px(60.);

/// What a failure says; a source nobody signed into is the one case worth wording ourselves.
pub(crate) fn failure_text(error: &ApiError) -> SharedString {
    if error.code == ErrorCode::SourceAuthExpired {
        return t("source_anon_discover");
    }
    t_with("discover_failed", &[("error", error.to_string())])
}

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
                    kind,
                    id: id.clone(),
                    continuation: cursor.clone(),
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
                                Notices::error(failure_text(&error), cx);
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
        let tiles: Vec<AnyElement> = view
            .shelf
            .items
            .iter()
            .map(|item| self.tile(index, item, cx))
            .collect();
        v_flex()
            .w_full()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .child(
                        v_flex()
                            .min_w_0()
                            .when_some(view.shelf.strapline.clone(), |column, strapline| {
                                column.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(strapline),
                                )
                            })
                            .child(
                                div()
                                    .truncate()
                                    .text_xl()
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .child(view.shelf.title.clone()),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .child(nudge(index, -STEP, &view.scroll))
                            .child(nudge(index, STEP, &view.scroll)),
                    ),
            )
            .child(
                h_flex()
                    .id(("shelf", index))
                    .w_full()
                    .items_start()
                    .gap_4()
                    .pb_2()
                    .overflow_x_scroll()
                    .track_scroll(&view.scroll)
                    .children(tiles),
            )
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
        v_flex()
            .w_full()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .truncate()
                            .text_xl()
                            .font_weight(gpui_kit::FontWeight::BOLD)
                            .child(title.clone()),
                    )
                    .when_some(view.shelf.more_ref.clone(), |row, more| {
                        row.child(
                            Button::new(("show-all", index))
                                .ghost()
                                .small()
                                .label(t("discover_show_all"))
                                .on_click(move |_, _, cx| {
                                    Navigator::navigate(
                                        Route::DiscoverPlaylist {
                                            id: more.clone().into(),
                                            title: title.clone(),
                                        },
                                        cx,
                                    )
                                }),
                        )
                    }),
            )
            .child(div().w_full().h(ROW * rows as f32).child(list))
            .into_any_element()
    }

    fn tile(&self, shelf: usize, item: &CatalogItem, cx: &mut Context<Self>) -> AnyElement {
        match item.kind {
            CatalogItemKind::Track => self.song_tile(shelf, item, cx),
            CatalogItemKind::Album | CatalogItemKind::Playlist => {
                self.playable_tile(shelf, item, cx)
            }
            CatalogItemKind::Artist => self.artist_tile(shelf, item),
            CatalogItemKind::Mood | CatalogItemKind::Unknown => self.inert_tile(shelf, item),
        }
    }

    fn playable_tile(
        &self,
        shelf: usize,
        item: &CatalogItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = item.id.clone();
        let kind = item.kind;
        let route = if kind == CatalogItemKind::Album {
            Route::Album(id.clone().into())
        } else {
            Route::DiscoverPlaylist {
                id: id.clone().into(),
                title: item.title.clone().into(),
            }
        };
        let overlay = self.play_overlay(shelf, item, kind, cx);
        div()
            .group(GROUP)
            .flex_shrink_0()
            .child(
                Tile::new(element_id(shelf, &id), item.title.clone())
                    .subtitle(item.subtitle.clone().unwrap_or_default())
                    .artwork(item.artwork.as_ref())
                    .placeholder(if kind == CatalogItemKind::Album {
                        icons::DISC
                    } else {
                        icons::LIST
                    })
                    .size(TILE)
                    .route(route)
                    .overlay(overlay),
            )
            .into_any_element()
    }

    fn artist_tile(&self, shelf: usize, item: &CatalogItem) -> AnyElement {
        div()
            .flex_shrink_0()
            .child(
                Tile::new(element_id(shelf, &item.id), item.title.clone())
                    .artwork(item.artwork.as_ref())
                    .placeholder(icons::MIC)
                    .size(TILE)
                    .round(true)
                    .route(Route::DiscoverArtist {
                        id: item.id.clone().into(),
                        name: item.title.clone().into(),
                    }),
            )
            .into_any_element()
    }

    fn inert_tile(&self, shelf: usize, item: &CatalogItem) -> AnyElement {
        div()
            .flex_shrink_0()
            .child(
                Tile::new(element_id(shelf, &item.id), item.title.clone())
                    .artwork(item.artwork.as_ref())
                    .size(TILE),
            )
            .into_any_element()
    }

    fn song_tile(&self, shelf: usize, item: &CatalogItem, cx: &mut Context<Self>) -> AnyElement {
        let Some(track) = item.track.as_ref() else {
            return div().into_any_element();
        };
        let key = track.key.clone();
        let radio = SourcesModel::global(cx).read(cx).capabilities().track_radio;
        let overlay = self.song_overlay(&key, cx);
        div()
            .id(SharedString::from(format!("song-{shelf}-{key}")))
            .group(GROUP)
            .flex_shrink_0()
            .on_click(cx.listener(move |this, _, _, cx| {
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
            }))
            .child(
                Tile::new(element_id(shelf, &item.id), item.title.clone())
                    .subtitle(item.subtitle.clone().unwrap_or_default())
                    .artwork(item.artwork.as_ref())
                    .size(TILE)
                    .overlay(overlay),
            )
            .into_any_element()
    }

    fn play_overlay(
        &self,
        shelf: usize,
        item: &CatalogItem,
        kind: CatalogItemKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = item.id.clone();
        let loading = self.pending.as_deref() == Some(id.as_str());
        let icon = if self.shows_pause(&id, cx) {
            icons::PAUSE
        } else {
            icons::PLAY
        };
        div()
            .absolute()
            .right_2()
            .bottom_2()
            .opacity(0.)
            .group_hover(GROUP, |style| style.opacity(1.))
            .child(
                Button::new(SharedString::from(format!("play-{shelf}-{id}")))
                    .primary()
                    .small()
                    .loading(loading)
                    .icon(icons::icon(icon))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_or_play(kind, id.clone(), cx);
                    })),
            )
            .into_any_element()
    }

    fn song_overlay(&self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let icon = if self.shows_pause(key, cx) {
            icons::PAUSE
        } else {
            icons::PLAY
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded(cx.theme().radius)
            .bg(cx.theme().background.opacity(0.5))
            .opacity(0.)
            .group_hover(GROUP, |style| style.opacity(1.))
            .child(icons::icon(icon).size(px(28.)))
            .into_any_element()
    }

    fn shows_pause(&self, id: &str, cx: &Context<Self>) -> bool {
        self.now_playing.as_deref() == Some(id)
            && self.pending.is_none()
            && PlayerModel::global(cx).read(cx).is_playing()
    }
}

impl Render for Shelves {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sections = Vec::with_capacity(self.views.len());
        for index in 0..self.views.len() {
            sections.push(self.section(index, cx));
        }
        v_flex().w_full().gap_10().children(sections)
    }
}

// One hover group for every tile; tiles never nest, so the nearest one is always the cursor's.
const GROUP: &str = "discover-tile";

/// A catalog id is unique only inside its shelf, and shelves only ever append.
fn element_id(shelf: usize, id: &str) -> SharedString {
    SharedString::from(format!("{shelf}-{id}"))
}

fn nudge(shelf: usize, delta: Pixels, scroll: &ScrollHandle) -> Button {
    let icon = if delta < px(0.) {
        icons::CHEVRON_LEFT
    } else {
        icons::CHEVRON_RIGHT
    };
    let scroll = scroll.clone();
    Button::new((if delta < px(0.) { "left" } else { "right" }, shelf))
        .ghost()
        .small()
        .icon(icons::icon(icon))
        .on_click(move |_, window, _| {
            let offset = scroll.offset();
            let max = scroll.max_offset().x.max(px(0.));
            scroll.set_offset(point((offset.x + delta).clamp(-max, px(0.)), offset.y));
            window.refresh();
        })
}
