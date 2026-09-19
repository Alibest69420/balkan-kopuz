//! One playlist's tracks, its cover, and what the source lets you do to it.

use std::path::Path;

use api::{ArtworkTarget, ArtworkUpload, PlaylistCapability, PlaylistReorder, Table, TrackInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, Size, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Pixels, Rems, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px, rems,
};
use link::Link;
use store::{
    DownloadsModel, Navigator, Notices, PlaylistsModel, Query, Route, SourcesModel, actions,
};
use widgets::format::{t, t_with};
use widgets::track_list::{TrackActions, TrackList, TrackListEvent};
use widgets::{Cover, EmptyState, Loading, icons, style};

/// `w-64 h-64`: the showcase cover.
const HERO: Pixels = px(256.);
/// `w-14 h-14`: the round play and radio buttons beside it.
const HERO_BUTTON: Rems = rems(3.5);
/// A `Button` sizes its glyph at 0.75× itself, and the showcase's is `text-xl`.
const HERO_GLYPH: Pixels = px(20. / 0.75);

pub struct PlaylistDetail {
    id: SharedString,
    keys: Vec<String>,
    tracks: Entity<Query<Vec<TrackInfo>>>,
    list: Entity<TrackList>,
    refreshed: bool,
    _subscriptions: Vec<Subscription>,
}

impl PlaylistDetail {
    pub fn new(id: SharedString, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let playlists = PlaylistsModel::global(cx);
        let keys = playlists
            .read(cx)
            .playlist(&id)
            .map(|playlist| playlist.track_keys.clone())
            .unwrap_or_default();
        let tracks = cx.new(|cx| {
            let keys = keys.clone();
            Query::new(
                &[Table::Tracks, Table::Playlists],
                move |api| {
                    let keys = keys.clone();
                    async move { api.tracks_by_keys(keys).await }
                },
                cx,
            )
        });
        let list = cx.new(TrackList::new);
        let sources = SourcesModel::global(cx);
        let subscriptions = vec![
            cx.observe(&tracks, |this, _, cx| this.adopt_rows(cx)),
            cx.subscribe(&list, Self::on_list_event),
            cx.observe(&playlists, |this, _, cx| {
                this.sync_keys(cx);
                cx.notify();
            }),
            cx.observe(&sources, |this, _, cx| {
                this.apply_capabilities(cx);
                this.request_refresh(cx);
                cx.notify();
            }),
            cx.observe(&DownloadsModel::global(cx), |_, _, cx| cx.notify()),
        ];
        let mut detail = Self {
            id,
            keys,
            tracks,
            list,
            refreshed: false,
            _subscriptions: subscriptions,
        };
        detail.apply_capabilities(cx);
        detail.request_refresh(cx);
        detail.adopt_rows(cx);
        detail
    }

    /// A server playlist's contents are filled in by the daemon; this only asks, once.
    fn request_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshed || !SourcesModel::global(cx).read(cx).capabilities().sync {
            return;
        }
        self.refreshed = true;
        let id = self.id.to_string();
        Link::global(cx).fire(cx, "refresh playlist", move |api| async move {
            api.refresh_playlist(id).await
        });
    }

    fn apply_capabilities(&mut self, cx: &mut Context<Self>) {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        self.list.update(cx, |list, cx| {
            list.set_actions(
                TrackActions {
                    remove_from_playlist: capabilities.playlists != PlaylistCapability::None,
                    delete: true,
                    reorder: capabilities.playlists == PlaylistCapability::Reorder,
                },
                cx,
            )
        });
    }

    fn sync_keys(&mut self, cx: &mut Context<Self>) {
        let playlists = PlaylistsModel::global(cx);
        let Some(keys) = playlists
            .read(cx)
            .playlist(&self.id)
            .map(|playlist| playlist.track_keys.clone())
        else {
            return;
        };
        if keys == self.keys {
            return;
        }
        self.keys = keys.clone();
        self.tracks.update(cx, |query, cx| {
            query.set_fetch(
                move |api| {
                    let keys = keys.clone();
                    async move { api.tracks_by_keys(keys).await }
                },
                cx,
            )
        });
    }

    fn adopt_rows(&mut self, cx: &mut Context<Self>) {
        let rows = self.tracks.read(cx).value().cloned().unwrap_or_default();
        self.list.update(cx, |list, cx| list.set_rows(rows, cx));
        cx.notify();
    }

    fn on_list_event(
        &mut self,
        _: Entity<TrackList>,
        event: &TrackListEvent,
        cx: &mut Context<Self>,
    ) {
        let id = self.id.to_string();
        match event {
            TrackListEvent::Play(index) => {
                let keys = self.list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
            TrackListEvent::RemoveFromPlaylist(index) => {
                let index = *index as u32;
                actions::run(cx, move |api| async move {
                    api.remove_playlist_track(id, index).await
                });
            }
            TrackListEvent::Delete(keys) => {
                let keys = keys.clone();
                let from_disk = SourcesModel::global(cx)
                    .read(cx)
                    .capabilities()
                    .delete_from_disk;
                actions::run(cx, move |api| async move {
                    api.delete_tracks(keys, from_disk).await
                });
            }
            TrackListEvent::Reorder { from, to } => {
                let reorder = PlaylistReorder {
                    from: *from as u32,
                    to: *to as u32,
                };
                actions::run(cx, move |api| async move {
                    api.reorder_playlist(id, reorder).await
                });
            }
            TrackListEvent::SelectionChanged => cx.notify(),
        }
    }

    /// The daemon decides what setting a cover means, so only the bytes travel.
    fn upload_cover(&mut self, cx: &mut Context<Self>) {
        let id = self.id.to_string();
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let content_type = content_type_for(&path);
            let read = cx
                .background_executor()
                .spawn(async move { std::fs::read(path) })
                .await;
            match read {
                Ok(bytes) => cx.update(|cx| {
                    actions::run(cx, move |api| async move {
                        api.upload_artwork(ArtworkUpload {
                            target: ArtworkTarget::Playlist(id),
                            content_type,
                            bytes,
                        })
                        .await
                    })
                }),
                Err(error) => cx.update(|cx| Notices::error(error.to_string(), cx)),
            }
        })
        .detach();
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>, name: SharedString) {
        let id = self.id.to_string();
        crate::dialogs::confirm_delete(
            t("delete_playlist"),
            name,
            move |cx| {
                let id = id.clone();
                actions::run(cx, move |api| async move { api.delete_playlist(id).await });
                Navigator::navigate(Route::Playlists, cx);
            },
            window,
            cx,
        );
    }
}

impl Render for PlaylistDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let playlists = PlaylistsModel::global(cx);
        let loaded = playlists.read(cx).is_loaded();
        let playlist = playlists.read(cx).playlist(&self.id).cloned();
        let Some(playlist) = playlist else {
            return v_flex().size_full().px_8().pt_8().child(if loaded {
                EmptyState::new(t("playlist_not_found"))
                    .icon(icons::LIST)
                    .into_any_element()
            } else {
                Loading.into_any_element()
            });
        };

        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let downloads = DownloadsModel::global(cx);
        let keys = playlist.track_keys.clone();
        let active = keys
            .iter()
            .any(|key| downloads.read(cx).is_active(key.as_str()));
        let stored = !keys.is_empty()
            && keys
                .iter()
                .all(|key| downloads.read(cx).is_stored(key.as_str()));
        let name = SharedString::from(playlist.name.clone());

        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .gap_8()
            .child(
                Button::new("playlist-back")
                    .ghost()
                    .icon(icons::icon(icons::CHEVRON_LEFT))
                    .text_color(style::slate_400(cx))
                    .on_click(|_, _, cx| {
                        Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx))
                    }),
            )
            .child(
                h_flex()
                    .gap_8()
                    .items_end()
                    .child(
                        div()
                            .id("playlist-cover")
                            .flex_shrink_0()
                            .child(
                                Cover::new(playlist.artwork.as_ref())
                                    .size(HERO)
                                    .hq(true)
                                    .rounded(px(12.))
                                    .placeholder(icons::LIST),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.upload_cover(cx))),
                    )
                    .child(
                        v_flex()
                            .gap_6()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .line_clamp(3)
                                    .text_size(rems(4.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(name.clone()),
                            )
                            .child(div().text_color(style::slate_400(cx)).child(t_with(
                                "playlist_track_count",
                                &[("count", keys.len().to_string())],
                            )))
                            .child(
                                h_flex()
                                    .gap_4()
                                    .items_center()
                                    .flex_wrap()
                                    .child(
                                        round_button("play-playlist", icons::PLAY)
                                            .primary()
                                            .tooltip(t("play"))
                                            .on_click({
                                                let keys = keys.clone();
                                                move |_, _, cx| actions::play_all(keys.clone(), cx)
                                            }),
                                    )
                                    .when(capabilities.playlist_radio, |row| {
                                        let id = self.id.to_string();
                                        row.child(
                                            round_button("playlist-radio", icons::RADIO)
                                                .ghost()
                                                .text_color(style::slate_400(cx))
                                                .tooltip(t("start_radio"))
                                                .on_click(move |_, _, cx| {
                                                    actions::start_playlist_radio(id.clone(), cx)
                                                }),
                                        )
                                    })
                                    .when(capabilities.downloads, |row| {
                                        let keys = keys.clone();
                                        row.child(
                                            Button::new("playlist-download")
                                                .ghost()
                                                .size_12()
                                                .rounded_full()
                                                .border_1()
                                                .border_color(style::white(style::ACTIVE, cx))
                                                .text_color(style::white(0.70, cx))
                                                .icon(icons::icon(if stored {
                                                    icons::TRASH
                                                } else {
                                                    icons::DOWNLOAD
                                                }))
                                                .tooltip(if stored {
                                                    t("remove_from_cache")
                                                } else {
                                                    t("downloader_download")
                                                })
                                                .loading(active)
                                                .on_click(move |_, _, cx| {
                                                    let keys = keys.clone();
                                                    DownloadsModel::global(cx).update(
                                                        cx,
                                                        |downloads, cx| {
                                                            if stored {
                                                                for key in keys {
                                                                    downloads.remove(key, cx);
                                                                }
                                                            } else {
                                                                downloads.download(keys, cx);
                                                            }
                                                        },
                                                    );
                                                }),
                                        )
                                    })
                                    .when(
                                        capabilities.playlists != PlaylistCapability::None,
                                        |row| {
                                            let name = name.clone();
                                            row.child(
                                                Button::new("playlist-delete")
                                                    .ghost()
                                                    .size_12()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(style::white(style::ACTIVE, cx))
                                                    .text_color(style::white(0.70, cx))
                                                    .icon(icons::icon(icons::TRASH))
                                                    .tooltip(t("delete_playlist"))
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.delete(window, cx, name.clone())
                                                        },
                                                    )),
                                            )
                                        },
                                    ),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().pb_20().child(self.list.clone()))
    }
}

/// `w-14 h-14 rounded-full` with a `text-xl` glyph: the showcase's play and radio.
fn round_button(id: &'static str, icon: &'static str) -> Button {
    Button::new(id)
        .with_size(Size::Size(HERO_GLYPH))
        .size(HERO_BUTTON)
        .rounded_full()
        .icon(icons::icon(icon))
}

/// The upload's content type, by extension, the way §5.4 spells it out.
fn content_type_for(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
    .to_string()
}
