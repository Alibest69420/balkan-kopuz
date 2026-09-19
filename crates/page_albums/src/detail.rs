//! One album: the standard detail, and the catalog-styled one a remote source presents.

use api::{
    AlbumInfo, AlbumPage, AlbumPresentation, ArtworkRef, CatalogDetail, CatalogDetailRequest,
    CatalogItemKind, PlaylistCapability, SourceCapabilities, Table, TrackInfo,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, px,
};
use store::{DownloadsModel, Navigator, PlayerModel, Query, Route, SourcesModel, actions};
use widgets::format::{t, t_with, total_duration};
use widgets::{
    Cover, EmptyState, Loading, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons,
};

use crate::ops;

// TODO(offline): filtering the listing to downloaded tracks waits on an offline seam the checklist flags as wrong.

// One virtualized row, so a disc's list can be sized to its contents inside the page's own scroll.
const ROW_HEIGHT: Pixels = px(60.);
const HERO_COVER: Pixels = px(200.);
const REMOTE_COVER: Pixels = px(280.);

/// One disc's rows; its list reports a play by the album-wide index its subscription carries.
struct DiscGroup {
    disc: Option<u32>,
    count: usize,
    list: Entity<TrackList>,
}

pub struct AlbumDetail {
    id: SharedString,
    album: Entity<Query<Option<AlbumInfo>>>,
    albums: Entity<Query<AlbumPage>>,
    tracks: Entity<Query<Vec<TrackInfo>>>,
    remote: Entity<Query<Option<CatalogDetail>>>,
    rows: Vec<TrackInfo>,
    groups: Vec<DiscGroup>,
    // The album ids the track query is pointed at, and the id the remote one is.
    aggregated: Vec<String>,
    remote_id: Option<String>,
    capabilities: SourceCapabilities,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    _lists: Vec<Subscription>,
}

impl AlbumDetail {
    pub fn new(id: SharedString, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let album_id = id.to_string();
        let album = cx.new(|cx| {
            let album_id = album_id.clone();
            Query::new(
                &[Table::Albums],
                move |api| {
                    let id = album_id.clone();
                    async move { api.album(id).await }
                },
                cx,
            )
        });
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |api| async move { api.albums(store::all()).await },
                cx,
            )
        });
        let tracks = cx.new(|cx| {
            let ids = vec![album_id.clone()];
            Query::new(
                &[Table::Tracks],
                move |api| {
                    let ids = ids.clone();
                    async move { Ok(collect_album_tracks(api, ids).await) }
                },
                cx,
            )
        });
        let remote = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |_| async move { Ok::<Option<CatalogDetail>, api::ApiError>(None) },
                cx,
            )
        });

        let sources = SourcesModel::global(cx);
        let downloads = DownloadsModel::global(cx);
        let player = PlayerModel::global(cx);
        let subscriptions = vec![
            cx.observe(&album, |this, _, cx| this.sync(cx)),
            cx.observe(&albums, |this, _, cx| this.sync(cx)),
            cx.observe(&sources, |this, _, cx| this.sync(cx)),
            cx.observe(&tracks, |this, _, cx| this.refresh_rows(cx)),
            cx.observe(&remote, |this, _, cx| this.refresh_rows(cx)),
            cx.observe(&downloads, |_, _, cx| cx.notify()),
            cx.observe(&player, |_, _, cx| cx.notify()),
        ];

        Self {
            id,
            album,
            albums,
            tracks,
            remote,
            rows: Vec::new(),
            groups: Vec::new(),
            aggregated: vec![album_id],
            remote_id: None,
            // Sources usually loaded before the page opened, so their observer would not fire again.
            capabilities: SourcesModel::global(cx).read(cx).capabilities(),
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
            _lists: Vec::new(),
        }
    }

    /// Point the track and catalog reads at what the album turned out to be.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let album = self.album.read(cx).value().cloned().flatten();
        let settled = self.album.read(cx).is_settled();

        // The grid dedupes by title, so the detail aggregates every same-titled album's tracks.
        let ids = match &album {
            Some(album) => {
                let title = album.title.trim().to_lowercase();
                let mut ids: Vec<String> = self
                    .albums
                    .read(cx)
                    .value()
                    .map(|page| {
                        page.albums
                            .iter()
                            .filter(|other| other.title.trim().to_lowercase() == title)
                            .map(|other| other.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if !ids.contains(&album.id) {
                    ids.push(album.id.clone());
                }
                ids
            }
            None => vec![self.id.to_string()],
        };
        if ids != self.aggregated {
            self.aggregated = ids.clone();
            point_tracks(&self.tracks, ids, cx);
        }

        // A remote source holds only the saved subset locally; a DB miss has nothing local at all.
        let remote_id = if capabilities.albums == AlbumPresentation::Remote {
            Some(
                album
                    .as_ref()
                    .map(|album| album.id.clone())
                    .unwrap_or_else(|| self.id.to_string()),
            )
        } else if settled && album.is_none() {
            Some(self.id.to_string())
        } else {
            None
        };
        if remote_id != self.remote_id {
            self.remote_id = remote_id.clone();
            point_remote(&self.remote, remote_id, cx);
        }

        if capabilities != self.capabilities {
            self.capabilities = capabilities;
            self.rebuild_groups(cx);
        }
        cx.notify();
    }

    fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        let mut rows = match self.remote.read(cx).value().cloned().flatten() {
            Some(detail) if !detail.tracks.is_empty() => detail.tracks,
            _ => self.tracks.read(cx).value().cloned().unwrap_or_default(),
        };
        ops::in_album_order(&mut rows);
        if rows != self.rows {
            self.rows = rows;
            self.rebuild_groups(cx);
        }
        cx.notify();
    }

    fn rebuild_groups(&mut self, cx: &mut Context<Self>) {
        let remote = self.capabilities.albums == AlbumPresentation::Remote;
        let options = TrackListOptions {
            show_index: true,
            show_cover: false,
            show_album: false,
            track_numbers: true,
            selectable: !remote,
        };
        let row_actions = TrackActions {
            remove_from_playlist: false,
            delete: self.capabilities.delete_from_disk && !remote,
            reorder: false,
        };

        let mut groups = Vec::new();
        let mut subscriptions = Vec::new();
        let mut offset = 0;
        for (disc, chunk) in by_disc(&self.rows) {
            let list = cx.new(|cx| {
                TrackList::new(cx)
                    .with_options(options)
                    .with_actions(row_actions)
            });
            let count = chunk.len();
            list.update(cx, |list, cx| list.set_rows(chunk, cx));
            subscriptions.push(
                cx.subscribe(&list, move |this, _, event: &TrackListEvent, cx| {
                    this.on_list_event(offset, event, cx)
                }),
            );
            groups.push(DiscGroup { disc, count, list });
            offset += count;
        }
        self.groups = groups;
        self._lists = subscriptions;
    }

    fn on_list_event(&mut self, offset: usize, event: &TrackListEvent, cx: &mut Context<Self>) {
        match event {
            TrackListEvent::Play(index) => {
                let keys: Vec<String> = self.rows.iter().map(|row| row.key.clone()).collect();
                actions::play_tracks(keys, Some((offset + index) as u32), cx);
            }
            TrackListEvent::Delete(keys) => ops::delete_tracks(keys.clone(), cx),
            _ => cx.notify(),
        }
    }

    fn selected_keys(&self, cx: &App) -> Vec<String> {
        self.groups
            .iter()
            .flat_map(|group| group.list.read(cx).selected_keys())
            .collect()
    }

    fn is_selecting(&self, cx: &App) -> bool {
        self.groups
            .iter()
            .any(|group| group.list.read(cx).is_selecting())
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        for group in &self.groups {
            group.list.update(cx, |list, cx| list.select_all(cx));
        }
    }

    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        for group in &self.groups {
            group.list.update(cx, |list, cx| list.clear_selection(cx));
        }
    }

    fn keys(&self) -> Vec<String> {
        ops::keys_of(&self.rows)
    }

    fn total_ms(&self) -> u64 {
        self.rows
            .iter()
            .filter_map(|row| row.duration_secs())
            .sum::<u64>()
            * 1000
    }

    fn back_button(&self) -> impl IntoElement {
        Button::new("album-back")
            .ghost()
            .icon(icons::icon(icons::CHEVRON_LEFT))
            .on_click(|_, _, cx| {
                Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx))
            })
    }

    fn play_button(&self, cx: &Context<Self>) -> impl IntoElement {
        let keys = self.keys();
        let shuffled = PlayerModel::global(cx).read(cx).shuffle();
        h_flex()
            .gap_2()
            .child(
                Button::new("album-play")
                    .primary()
                    .icon(icons::icon(icons::PLAY))
                    .label(t("play"))
                    .disabled(keys.is_empty())
                    .on_click(move |_, _, cx| actions::play_all(keys.clone(), cx)),
            )
            .child(
                Button::new("album-shuffle")
                    .ghost()
                    .icon(icons::icon(icons::SHUFFLE))
                    .tooltip(t("shuffle"))
                    .selected(shuffled)
                    .on_click(move |_, _, cx| {
                        PlayerModel::global(cx)
                            .update(cx, |player, cx| player.set_shuffle(!shuffled, cx))
                    }),
            )
    }

    /// Download the whole album, or give its offline copies back; disabled while a download runs.
    fn download_all_button(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.capabilities.downloads {
            return None;
        }
        let keys = self.keys();
        if keys.is_empty() {
            return None;
        }
        let downloads = DownloadsModel::global(cx).read(cx);
        let stored = keys.iter().all(|key| downloads.is_stored(key));
        let busy = keys.iter().any(|key| downloads.is_active(key));
        // TODO(i18n): en.ftl has no key for the album-wide download toggle.
        let label = if stored {
            "Remove downloads"
        } else {
            "Download all"
        };
        Some(
            Button::new("album-download-all")
                .ghost()
                .icon(icons::icon(if stored {
                    icons::TRASH
                } else {
                    icons::DOWNLOAD
                }))
                .label(label)
                .disabled(busy)
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
                    })
                })
                .into_any_element(),
        )
    }

    fn cover(&self, album: &AlbumInfo, size: Pixels, cx: &Context<Self>) -> impl IntoElement {
        let editable = self.capabilities.edit_tags;
        let id = album.id.clone();
        let remove_id = album.id.clone();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                div()
                    .id("album-cover")
                    .when(editable, |cover| {
                        cover.cursor_pointer().on_click(move |_, _, cx| {
                            ops::pick_album_cover(id.clone(), cx);
                        })
                    })
                    .child(
                        Cover::new(album.artwork.as_ref())
                            .size(size)
                            .hq(true)
                            .rounded(cx.theme().radius)
                            .placeholder(icons::DISC),
                    ),
            )
            .when(editable && album.artwork.is_some(), |block| {
                block.child(
                    div().absolute().bottom_1().right_1().child(
                        Button::new("album-cover-remove")
                            .ghost()
                            .icon(icons::icon(icons::TRASH))
                            .tooltip(t("remove_cover"))
                            .on_click(move |_, _, cx| {
                                ops::remove_album_cover(remove_id.clone(), cx)
                            }),
                    ),
                )
            })
    }

    fn artist_link(&self, artist: &str, id: &'static str, cx: &Context<Self>) -> impl IntoElement {
        let name = SharedString::from(artist.to_string());
        let route = Route::Artist {
            name: name.clone(),
            channel: None,
        };
        div()
            .id(id)
            .truncate()
            .text_color(cx.theme().muted_foreground)
            .hover(|link| link.text_color(cx.theme().foreground))
            .cursor_pointer()
            .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
            .child(name)
    }

    fn tracks_body(&self, cx: &Context<Self>) -> AnyElement {
        if self.rows.is_empty() {
            return EmptyState::new(t("no_tracks_found"))
                .icon(icons::MUSIC)
                .into_any_element();
        }
        div()
            .id("album-tracks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(self.groups.iter().map(|group| {
                v_flex()
                    .w_full()
                    .when_some(group.disc, |column, disc| {
                        column.child(
                            div()
                                .px_2()
                                .py_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                // TODO(i18n): en.ftl has no disc-heading key.
                                .child(format!("Disc {disc}")),
                        )
                    })
                    .child(
                        div()
                            .w_full()
                            .h(ROW_HEIGHT * group.count as f32)
                            .child(group.list.clone()),
                    )
            }))
            .into_any_element()
    }

    fn selection_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.is_selecting(cx) {
            return None;
        }
        let keys = self.selected_keys(cx);
        let playlists = self.capabilities.playlists != PlaylistCapability::None;
        let deletable = self.capabilities.delete_from_disk;
        Some(
            h_flex()
                .w_full()
                .gap_2()
                .mt_2()
                .p_2()
                .rounded(cx.theme().radius)
                .bg(cx.theme().secondary)
                .child(div().flex_1().text_sm().child(t_with(
                    "showcase_song_count",
                    &[("count", keys.len().to_string())],
                )))
                .child(
                    Button::new("album-selection-queue")
                        .ghost()
                        .icon(icons::icon(icons::QUEUE_ADD))
                        .label(t("add_to_queue"))
                        .on_click({
                            let keys = keys.clone();
                            move |_, _, cx| actions::append(keys.clone(), cx)
                        }),
                )
                .when(playlists, |bar| {
                    bar.child(
                        Button::new("album-selection-playlist")
                            .ghost()
                            .icon(icons::icon(icons::PLUS))
                            .label(t("add_to_playlist"))
                            .on_click({
                                let keys = keys.clone();
                                move |_, window, cx| {
                                    widgets::playlist_picker::open(keys.clone(), window, cx)
                                }
                            }),
                    )
                })
                .when(deletable, |bar| {
                    bar.child(
                        Button::new("album-selection-delete")
                            .ghost()
                            .icon(icons::icon(icons::TRASH))
                            .label(t("delete"))
                            .on_click({
                                let keys = keys.clone();
                                move |_, _, cx| ops::delete_tracks(keys.clone(), cx)
                            }),
                    )
                })
                .child(
                    Button::new("album-selection-cancel")
                        .ghost()
                        .icon(icons::icon(icons::CLOSE))
                        .label(t("cancel"))
                        .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx))),
                )
                .into_any_element(),
        )
    }

    fn standard_detail(&self, album: AlbumInfo, cx: &Context<Self>) -> AnyElement {
        let meta = {
            let count = t_with(
                "showcase_song_count",
                &[("count", self.rows.len().to_string())],
            );
            match album.year > 0 {
                true => format!(
                    "{} • {} • {}",
                    album.year,
                    count,
                    total_duration(self.total_ms())
                ),
                false => format!("{} • {}", count, total_duration(self.total_ms())),
            }
        };
        v_flex()
            .size_full()
            .p_4()
            .child(h_flex().gap_2().items_center().child(self.back_button()))
            .child(
                h_flex()
                    .gap_4()
                    .items_end()
                    .py_4()
                    .child(self.cover(&album, HERO_COVER, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .truncate()
                                    .text_2xl()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child(SharedString::from(album.title.clone())),
                            )
                            .child(self.artist_link(&album.artist, "album-artist", cx))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(meta),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(self.play_button(cx))
                                    .child(
                                        Button::new("album-select-all")
                                            .ghost()
                                            .icon(icons::icon(icons::CHECK))
                                            .label(t("select_all"))
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.select_all(cx)),
                                            ),
                                    )
                                    .children(self.download_all_button(cx)),
                            ),
                    ),
            )
            .child(self.tracks_body(cx))
            .children(self.selection_bar(cx))
            .into_any_element()
    }

    fn remote_detail(&self, view: RemoteView, cx: &Context<Self>) -> AnyElement {
        let minutes = self.total_ms() / 60_000;
        let count = t_with(
            "showcase_song_count",
            &[("count", self.rows.len().to_string())],
        );
        let mut meta = Vec::new();
        if view.year.is_some() {
            meta.push(t("album").to_string());
        }
        meta.push(count.to_string());
        meta.push(format!("{minutes} {}", t("min")));
        if let Some(year) = &view.year {
            meta.push(year.to_string());
        }
        let cover = AlbumInfo {
            id: view.id.clone(),
            title: view.title.to_string(),
            artist: view.artist.to_string(),
            artwork: view.artwork.clone(),
            ..AlbumInfo::default()
        };
        let artist_route = Route::Artist {
            name: view.artist.clone(),
            channel: None,
        };
        let browse_id = view.browse_id.clone();
        let first_key = self.rows.first().map(|row| row.key.clone());

        v_flex()
            .size_full()
            .p_4()
            .child(h_flex().gap_2().items_center().child(self.back_button()))
            .child(
                h_flex()
                    .size_full()
                    .min_h_0()
                    .gap_6()
                    .pt_4()
                    .child(
                        v_flex()
                            .w(px(320.))
                            .flex_shrink_0()
                            .gap_4()
                            .child(self.cover(&cover, REMOTE_COVER, cx))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(self.artist_link(
                                        &view.artist,
                                        "album-remote-artist",
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                            .child(view.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(meta.join(" • ")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .children(self.download_all_button(cx))
                                    .child(
                                        Button::new("album-remote-artist-button")
                                            .ghost()
                                            .icon(icons::icon(icons::MIC))
                                            // TODO(i18n): en.ftl has no go-to-artist key.
                                            .tooltip("Go to artist")
                                            .on_click(move |_, _, cx| {
                                                Navigator::navigate(artist_route.clone(), cx)
                                            }),
                                    )
                                    .child(self.play_button(cx))
                                    .child(
                                        Button::new("album-remote-share")
                                            .ghost()
                                            .icon(icons::icon(icons::SHARE))
                                            // TODO(i18n): en.ftl has no generic share key.
                                            .tooltip("Share")
                                            .on_click(move |_, _, cx| {
                                                ops::share_album(
                                                    browse_id.clone(),
                                                    first_key.clone(),
                                                    cx,
                                                )
                                            }),
                                    ),
                            ),
                    )
                    .child(v_flex().flex_1().min_w_0().child(self.tracks_body(cx))),
            )
            .into_any_element()
    }
}

impl Render for AlbumDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let album = self.album.read(cx).value().cloned().flatten();
        let remote = self.remote.read(cx).value().cloned().flatten();

        if self.capabilities.albums == AlbumPresentation::Remote {
            if let Some(view) = RemoteView::of(album.as_ref(), remote.as_ref()) {
                return self.remote_detail(view, cx);
            }
        } else if let Some(album) = album.clone() {
            return self.standard_detail(album, cx);
        } else if let Some(view) = RemoteView::of(None, remote.as_ref()) {
            return self.remote_detail(view, cx);
        }

        match self.album.read(cx).is_settled() && self.remote.read(cx).is_settled() {
            true => EmptyState::new(t("album_not_found"))
                .icon(icons::DISC)
                .into_any_element(),
            false => Loading.into_any_element(),
        }
    }
}

/// The header the catalog-styled page draws, from the local row, the remote listing, or both.
struct RemoteView {
    id: String,
    title: SharedString,
    artist: SharedString,
    year: Option<SharedString>,
    artwork: Option<ArtworkRef>,
    browse_id: Option<String>,
}

impl RemoteView {
    fn of(album: Option<&AlbumInfo>, remote: Option<&CatalogDetail>) -> Option<Self> {
        match (album, remote) {
            (Some(album), _) => Some(Self {
                id: album.id.clone(),
                title: album.title.clone().into(),
                artist: album.artist.clone().into(),
                year: remote
                    .and_then(|detail| detail.year.clone())
                    .map(SharedString::from)
                    .or_else(|| (album.year > 0).then(|| album.year.to_string().into())),
                artwork: album
                    .artwork
                    .clone()
                    .or_else(|| remote.and_then(|detail| detail.artwork.clone())),
                browse_id: remote.map(|detail| detail.id.clone()),
            }),
            (None, Some(detail)) => Some(Self {
                id: detail.id.clone(),
                title: detail.title.clone().into(),
                artist: detail.subtitle.clone().unwrap_or_default().into(),
                year: detail.year.clone().map(SharedString::from),
                artwork: detail.artwork.clone(),
                browse_id: Some(detail.id.clone()),
            }),
            (None, None) => None,
        }
    }
}

/// Rows split per disc, or one unlabelled group when the album has only one.
fn by_disc(rows: &[TrackInfo]) -> Vec<(Option<u32>, Vec<TrackInfo>)> {
    let discs: Vec<u32> = {
        let mut discs: Vec<u32> = rows
            .iter()
            .map(|row| row.disc_number.unwrap_or(1))
            .collect();
        discs.dedup();
        discs
    };
    if discs.len() < 2 {
        return match rows.is_empty() {
            true => Vec::new(),
            false => vec![(None, rows.to_vec())],
        };
    }
    let mut groups: Vec<(Option<u32>, Vec<TrackInfo>)> = Vec::new();
    for row in rows {
        let disc = row.disc_number.unwrap_or(1);
        match groups.last_mut() {
            Some((Some(current), chunk)) if *current == disc => chunk.push(row.clone()),
            _ => groups.push((Some(disc), vec![row.clone()])),
        }
    }
    groups
}

async fn collect_album_tracks(
    api: std::sync::Arc<dyn api::KopuzApi>,
    ids: Vec<String>,
) -> Vec<TrackInfo> {
    let mut tracks = Vec::new();
    for id in ids {
        if let Ok(page) = api.album_tracks(id, store::all()).await {
            tracks.extend(page.items);
        }
    }
    tracks
}

fn point_tracks(query: &Entity<Query<Vec<TrackInfo>>>, ids: Vec<String>, cx: &mut App) {
    query.update(cx, |query, cx| {
        query.set_fetch(
            move |api| {
                let ids = ids.clone();
                async move { Ok(collect_album_tracks(api, ids).await) }
            },
            cx,
        )
    });
}

fn point_remote(query: &Entity<Query<Option<CatalogDetail>>>, id: Option<String>, cx: &mut App) {
    query.update(cx, |query, cx| {
        query.set_fetch(
            move |api| {
                let id = id.clone();
                async move {
                    let Some(id) = id else {
                        return Ok(None);
                    };
                    api.catalog_detail(CatalogDetailRequest {
                        kind: CatalogItemKind::Album,
                        id,
                        continuation: None,
                    })
                    .await
                    .map(Some)
                }
            },
            cx,
        )
    });
}
