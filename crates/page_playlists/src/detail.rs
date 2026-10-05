//! One playlist's tracks, its cover, and what the source lets you do to it.

use std::path::Path;

use api::{ArtworkTarget, ArtworkUpload, PlaylistCapability, PlaylistReorder, Table, TrackInfo};
use components::format::{t, t_with};
use components::track_list::{TrackActions, TrackList, TrackListEvent};
use components::{
    DetailHeader, EmptyState, TrackHeader, detail_header_skeleton, icons, motion, style,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, Size, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    PathPromptOptions, Pixels, Rems, Render, SharedString, Styled as _, Subscription, Window, div,
    px, rems,
};
use link::Link;
use store::{
    DownloadsModel, Navigator, Notices, PlaylistsModel, Query, Route, SourcesModel, actions,
};

use crate::cards::tweened;

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
                let removed = self.list.read(cx).keys().get(index as usize).cloned();
                let last = (self.list.read(cx).keys().len() as u32).saturating_sub(1);
                actions::run(cx, {
                    let id = id.clone();
                    move |api| async move { api.remove_playlist_track(id, index).await }
                });
                let Some(key) = removed else { return };
                // Re-adding appends, so the undo puts the row back where it was as well.
                store::Notices::undo(
                    components::format::t("track_removed_from_playlist"),
                    move |cx| {
                        let (id, key) = (id.clone(), key.clone());
                        actions::run(cx, move |api| async move {
                            api.add_playlist_tracks(id.clone(), vec![key]).await?;
                            api.reorder_playlist(
                                id,
                                PlaylistReorder {
                                    from: last,
                                    to: index,
                                },
                            )
                            .await
                        });
                    },
                    cx,
                );
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
                // Reading the picked file, not a daemon call: its own message is the one to show.
                Err(error) => cx.update(|cx| Notices::error(error.to_string(), cx)),
            }
        })
        .detach();
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>, name: SharedString) {
        let id = self.id.to_string();
        components::dialogs::confirm_delete(
            t("delete_playlist"),
            name,
            move |_, cx| {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let playlists = PlaylistsModel::global(cx);
        let loaded = playlists.read(cx).is_loaded();
        let playlist = playlists.read(cx).playlist(&self.id).cloned();
        let Some(playlist) = playlist else {
            // The page used to draw nothing at all while the catalog was still being read.
            return v_flex().size_full().px_8().pt_8().child(if loaded {
                EmptyState::new(t("playlist_not_found"))
                    .icon(icons::LIST)
                    .into_any_element()
            } else {
                detail_header_skeleton(HERO, false, cx).into_any_element()
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
        // These buttons carry no border; `tweened` still wants a colour for the edge it would paint.
        let clear = style::white(0., cx);
        let (play_hot, play_hover) = motion::hovered("play-playlist-hover", window, cx);
        let play_fill = motion::color(
            "play-playlist-fill",
            if play_hot {
                cx.theme().primary_hover
            } else {
                cx.theme().primary
            },
            window,
            cx,
        );
        let play_text = cx.theme().primary_foreground;
        let (radio_hot, radio_hover) = motion::hovered("playlist-radio-hover", window, cx);
        let radio_fill = motion::color(
            "playlist-radio-fill",
            style::white(if radio_hot { style::HOVER } else { 0. }, cx),
            window,
            cx,
        );
        let radio_text = motion::color(
            "playlist-radio-text",
            if radio_hot {
                cx.theme().foreground
            } else {
                style::slate_400(cx)
            },
            window,
            cx,
        );
        // The frame still tracks the pointer while the download button is loading.
        let (download_hot, download_hover) = motion::hovered("playlist-download-hover", window, cx);
        let download_hot = download_hot && !active;
        let (delete_hot, delete_hover) = motion::hovered("playlist-delete-hover", window, cx);
        let outline = |hot: bool, cx: &mut Context<Self>| {
            (
                style::white(if hot { style::HOVER } else { 0. }, cx),
                style::white(if hot { 1. } else { 0.70 }, cx),
                style::white(if hot { 0.40 } else { style::ACTIVE }, cx),
            )
        };
        let (download_fill, download_text, download_edge) = outline(download_hot, cx);
        let download_fill = motion::color("playlist-download-fill", download_fill, window, cx);
        let download_text = motion::color("playlist-download-text", download_text, window, cx);
        let download_edge = motion::color("playlist-download-edge", download_edge, window, cx);
        let (delete_fill, delete_text, delete_edge) = outline(delete_hot, cx);
        let delete_fill = motion::color("playlist-delete-fill", delete_fill, window, cx);
        let delete_text = motion::color("playlist-delete-text", delete_text, window, cx);
        let delete_edge = motion::color("playlist-delete-edge", delete_edge, window, cx);

        let page = cx.entity().downgrade();
        let mut header = DetailHeader::new(name.clone())
            .artwork(playlist.artwork.as_ref())
            .placeholder(icons::LIST)
            .on_cover("playlist-cover", move |_, cx| {
                page.update(cx, |this, cx| this.upload_cover(cx)).ok();
            })
            .meta(t_with(
                "playlist_track_count",
                &[("count", keys.len().to_string())],
            ))
            .action(tweened(
                "play-playlist",
                round_button("play-playlist", icons::PLAY)
                    .tooltip(t("play"))
                    .on_click({
                        let keys = keys.clone();
                        move |_, _, cx| actions::play_all(keys.clone(), cx)
                    }),
                play_hover,
                play_fill,
                play_text,
                clear,
                cx,
            ));
        if capabilities.playlist_radio {
            let id = self.id.to_string();
            header = header.action(tweened(
                "playlist-radio",
                round_button("playlist-radio", icons::RADIO)
                    .tooltip(t("start_radio"))
                    .on_click(move |_, _, cx| actions::start_playlist_radio(id.clone(), cx)),
                radio_hover,
                radio_fill,
                radio_text,
                clear,
                cx,
            ));
        }
        if capabilities.downloads {
            let keys = keys.clone();
            header = header.action(tweened(
                "playlist-download",
                Button::new("playlist-download")
                    .size_12()
                    .rounded_full()
                    .border_1()
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
                        DownloadsModel::global(cx).update(cx, |downloads, cx| {
                            if stored {
                                for key in keys {
                                    downloads.remove(key, cx);
                                }
                            } else {
                                downloads.download(keys, cx);
                            }
                        });
                    }),
                download_hover,
                download_fill,
                download_text,
                download_edge,
                cx,
            ));
        }
        if capabilities.playlists != PlaylistCapability::None {
            let name = name.clone();
            header = header.action(tweened(
                "playlist-delete",
                Button::new("playlist-delete")
                    .size_12()
                    .rounded_full()
                    .border_1()
                    .icon(icons::icon(icons::TRASH))
                    .tooltip(t("delete_playlist"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.delete(window, cx, name.clone())
                    })),
                delete_hover,
                delete_fill,
                delete_text,
                delete_edge,
                cx,
            ));
        }

        let body = self.body(keys.len(), window, cx);
        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .gap_8()
            .child(
                Button::new("playlist-back")
                    .custom(style::quiet_button(style::slate_400(cx), cx))
                    .icon(icons::icon(icons::CHEVRON_LEFT))
                    .on_click(|_, _, cx| {
                        Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx))
                    }),
            )
            .child(header)
            .child(body)
    }
}

impl PlaylistDetail {
    /// The rows, or what stands in for them: the page used to draw an empty list either way.
    fn body(&self, expected: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.tracks.read(cx);
        let rows = self.list.read(cx).rows().len();
        if rows == 0 {
            if let Some(error) = query.error() {
                return v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        EmptyState::new(t("library_read_failed"))
                            .icon(icons::WARNING)
                            .detail(store::human(error)),
                    )
                    .child(
                        Button::new("playlist-retry")
                            .primary()
                            .label(t("retry_now"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tracks.update(cx, |query, cx| query.refresh(cx));
                            })),
                    )
                    .into_any_element();
            }
            if expected > 0 && !query.is_settled() {
                return v_flex()
                    .flex_1()
                    .min_h_0()
                    .pb_20()
                    .child(components::skeleton::rows(10, px(56.), true, 3, cx))
                    .into_any_element();
            }
            return v_flex()
                .flex_1()
                .min_h_0()
                .child(
                    EmptyState::new(t("playlist_empty"))
                        .icon(icons::LIST)
                        .detail(t("playlist_empty_detail")),
                )
                .into_any_element();
        }
        let _ = window;
        v_flex()
            .flex_1()
            .min_h_0()
            .pb_20()
            .gap_2()
            .child(TrackHeader::new(
                "playlist-header",
                self.list.read(cx).layout(),
            ))
            .child(div().flex_1().min_h_0().child(self.list.clone()))
            .into_any_element()
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
