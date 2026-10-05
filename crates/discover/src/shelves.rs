//! Catalog shelves: the carousels, tiles and list rows the landing and the remote artist page share.

use std::collections::HashSet;

use api::{CatalogDetailRequest, CatalogItem, CatalogItemKind, CatalogShelf};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, point, px,
};
use store::{Link, Navigator, Notices, PlayerModel, Route, SourcesModel, Target, actions};
use widgets::format::t;
use widgets::{
    PlayState, Tile, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons, motion,
    style,
};

use crate::states::catalog_failure;

const TILE: Pixels = px(176.);
// One ‹ or › press, matching the Dioxus frontend's 800 px nudge.
const STEP: Pixels = px(800.);
// `w-8 h-8` on a carousel arrow, and its `hover:scale-105`.
const NUDGE: Pixels = px(32.);
const NUDGE_HOVER: Pixels = px(33.6);

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
    // The carousel arrow under the pointer, which its tween reacts to.
    hovered_nudge: Option<SharedString>,
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
            hovered_nudge: None,
            _play: None,
            _subscriptions: vec![cx.observe(&player, |_, _, cx| cx.notify())],
        }
    }

    fn set_nudge_hover(&mut self, id: SharedString, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.hovered_nudge = Some(id);
        } else if self.hovered_nudge.as_ref() == Some(&id) {
            self.hovered_nudge = None;
        }
        cx.notify();
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

    fn section(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = &self.views[index];
        match &view.list {
            Some(list) => self.list_section(index, list.clone(), cx),
            None => self.carousel_section(index, window, cx),
        }
    }

    fn carousel_section(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.views[index];
        let tiles: Vec<AnyElement> = view
            .shelf
            .items
            .iter()
            .map(|item| self.tile(index, item, cx))
            .collect();
        let arrows = h_flex()
            .flex_shrink_0()
            .gap_2()
            .child(self.nudge(index, -STEP, window, cx))
            .child(self.nudge(index, STEP, window, cx));
        v_flex()
            .w_full()
            .gap_5()
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
                                        .mb_0p5()
                                        .text_size(style::text_10())
                                        .font_weight(gpui_kit::FontWeight::BOLD)
                                        .text_color(style::white(0.4, cx))
                                        .child(strapline),
                                )
                            })
                            .child(
                                div()
                                    .truncate()
                                    .text_2xl()
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .child(view.shelf.title.clone()),
                            ),
                    )
                    .child(arrows),
            )
            .child(
                h_flex()
                    .id(("shelf", index))
                    .w_full()
                    .items_start()
                    .gap_5()
                    .px_2()
                    .pt_1()
                    .pb_3()
                    .overflow_x_scroll()
                    // A bare wheel notch belongs to the page; only a sideways gesture or shift slides the row.
                    .restrict_scroll_to_axis()
                    .on_scroll_wheel(cx.listener({
                        let scroll = view.scroll.clone();
                        move |_, event: &ScrollWheelEvent, window, cx| {
                            let delta = event.delta.pixel_delta(window.line_height());
                            if event.modifiers.shift && delta.y != px(0.) {
                                widgets::scroll::nudge_x(&scroll, -delta.y);
                                cx.stop_propagation();
                            }
                        }
                    }))
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
            .gap_5()
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .truncate()
                            .text_2xl()
                            .font_weight(gpui_kit::FontWeight::BOLD)
                            .child(title.clone()),
                    )
                    .when_some(view.shelf.more_ref.clone(), |row, more| {
                        row.child(
                            div()
                                .id(("show-all", index))
                                .flex_shrink_0()
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(gpui_kit::FontWeight::BOLD)
                                .text_color(style::white(style::TEXT_SECONDARY, cx))
                                .hover(|link| link.text_color(cx.theme().foreground))
                                .child(t("discover_show_all"))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sections = Vec::with_capacity(self.views.len());
        for index in 0..self.views.len() {
            sections.push(self.section(index, window, cx));
        }
        v_flex().w_full().gap_12().children(sections)
    }
}

/// A catalog id is unique only inside its shelf, and shelves only ever append.
fn element_id(shelf: usize, id: &str) -> SharedString {
    SharedString::from(format!("{shelf}-{id}"))
}

impl Shelves {
    /// `bg-white/5 hover:bg-white/10 transition-all hover:scale-105`, the scale read as a grown box.
    fn nudge(
        &self,
        shelf: usize,
        delta: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Button {
        let back = delta < px(0.);
        let icon = if back {
            icons::CHEVRON_LEFT
        } else {
            icons::CHEVRON_RIGHT
        };
        let side = if back { "back" } else { "forward" };
        let title = &self.views[shelf].shelf.title;
        let id = SharedString::from(format!("nudge-{shelf}-{title}-{side}"));
        let hovered = self.hovered_nudge.as_ref() == Some(&id);
        let target = if hovered {
            style::white(style::HOVER, cx)
        } else {
            style::white(style::RAISED, cx)
        };
        let background = motion::color(id.clone(), target, window, cx);
        let size = motion::length(
            SharedString::from(format!("{id}-size")),
            if hovered { NUDGE_HOVER } else { NUDGE },
            motion::FAST,
            window,
            cx,
        );
        let scroll = self.views[shelf].scroll.clone();
        Button::new((if back { "left" } else { "right" }, shelf))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(style::rest(background))
                    .foreground(cx.theme().foreground)
                    .hover(background)
                    .active(style::white(style::HOVER, cx)),
            )
            .rounded(px(999.))
            .with_size(size)
            .icon(icons::icon(icon).xsmall())
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_nudge_hover(id.clone(), *hovered, cx)
            }))
            .on_click(move |_, window, _| {
                let offset = scroll.offset();
                let max = scroll.max_offset().x.max(px(0.));
                scroll.set_offset(point((offset.x + delta).clamp(-max, px(0.)), offset.y));
                window.refresh();
            })
    }
}
