//! One album: the standard detail, and the catalog-styled one a remote source presents.

use api::{
    AlbumInfo, AlbumPresentation, ArtworkRef, CatalogDetail, CatalogDetailRequest, CatalogItemKind,
    SourceCapabilities, Table, TrackInfo,
};
use components::format::{t, t_with};
use components::{
    DetailHeader, EditableCover, EmptyState, Heading, IconButton, IconTone, Link, Loading,
    SectionHeading, Tone, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons, style,
};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, px, rems,
};
use store::{DownloadsModel, Navigator, PlayerModel, Query, Route, SourcesModel, Target, actions};

use crate::ops;

// TODO(offline): filtering the listing to downloaded tracks waits on an offline seam the checklist flags as wrong.

// `w-64 h-64` on the showcase, `max-w-[300px]` on the catalog-styled page.
const HERO_COVER: Pixels = px(256.);
const REMOTE_COVER: Pixels = px(300.);
// `w-14`/`w-16`/`w-12`/`w-11`, with the glyph sizes the Dioxus buttons set on them.
const HERO_ACTION: Pixels = px(56.);
const HERO_SECONDARY: Pixels = px(48.);
const REMOTE_ACTION: Pixels = px(64.);
const REMOTE_SECONDARY: Pixels = px(44.);
const COVER: &str = "album-cover";
/// `border-white/15 hover:border-white/30`: the catalog-styled page's quieter ring.
const REMOTE_RING: (f32, f32) = (0.15, 0.30);

/// One disc's rows; its list reports a play by the album-wide index its subscription carries.
struct DiscGroup {
    disc: Option<u32>,
    count: usize,
    list: Entity<TrackList>,
}

pub struct AlbumDetail {
    id: SharedString,
    album: Entity<Query<Option<AlbumInfo>>>,
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
            cx.observe(&sources, |this, _, cx| this.sync(cx)),
            cx.observe(&tracks, |this, _, cx| this.refresh_rows(cx)),
            cx.observe(&remote, |this, _, cx| this.refresh_rows(cx)),
            cx.observe(&downloads, |_, _, cx| cx.notify()),
            cx.observe(&player, |_, _, cx| cx.notify()),
        ];

        Self {
            id,
            album,
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

        let ids = vec![match &album {
            Some(album) => album.id.clone(),
            None => self.id.to_string(),
        }];
        if ids != self.aggregated {
            self.aggregated = ids.clone();
            point_tracks(&self.tracks, ids, cx);
        }

        // A catalog-presented source stores only the saved subset; a miss means none of it is stored.
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
            commands::bind_track_list(&list, cx);
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

    /// `bg-indigo-500 hover:bg-indigo-400 text-black`, the one filled action on the page.
    fn play_button(&self, size: Pixels) -> IconButton {
        let keys = self.keys();
        IconButton::new("album-play", icons::PLAY)
            .size(size)
            .tone(IconTone::Accent)
            .tooltip(t("play"))
            .disabled(keys.is_empty())
            .on_click(move |_, _, cx| actions::play_all(keys.clone(), cx))
    }

    /// The shuffle toggle: accent-coloured while on, muted while off.
    fn shuffle_button(&self, size: Pixels, outlined: bool, cx: &App) -> IconButton {
        let shuffled = PlayerModel::global(cx).read(cx).shuffle();
        let button = IconButton::new("album-shuffle", icons::SHUFFLE)
            .size(size)
            .lit(shuffled)
            .tooltip(t("shuffle"))
            .on_click(move |_, _, cx| {
                PlayerModel::global(cx).update(cx, |player, cx| player.set_shuffle(!shuffled, cx))
            });
        match outlined {
            true => button.ring(REMOTE_RING.0, REMOTE_RING.1),
            false => button,
        }
    }

    /// Download the whole album, or give its offline copies back; disabled while a download runs.
    fn download_all_button(&self, size: Pixels, cx: &App) -> Option<IconButton> {
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
        let (icon, label) = match stored {
            true => (icons::TRASH, t("remove_download")),
            false => (icons::DOWNLOAD, t("download_offline")),
        };
        Some(
            IconButton::new("album-download-all", icon)
                .size(size)
                .glyph(size * 0.33)
                .tone(IconTone::Outline)
                .tooltip(label)
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
                }),
        )
    }

    fn cover(&self, album: &AlbumInfo, size: Pixels, radius: Pixels) -> EditableCover {
        let cover = EditableCover::new(COVER, album.artwork.as_ref())
            .size(size)
            .rounded(radius)
            .placeholder(icons::DISC);
        if !self.capabilities.edit_tags {
            return cover;
        }
        let (id, remove_id) = (album.id.clone(), album.id.clone());
        cover
            .on_pick(move |_, cx| ops::pick_album_cover(id.clone(), cx))
            .on_remove(t("remove_cover"), move |_, cx| {
                ops::remove_album_cover(remove_id.clone(), cx)
            })
    }

    /// `text-sm font-bold text-white/60 hover:text-white hover:underline transition-colors`.
    fn artist_link(artist: &str, key: Option<String>, id: &'static str) -> impl IntoElement {
        // An album that bills nobody the daemon could key opens nothing.
        Link::new(id)
            .label(artist.to_string())
            .route(key.map(Route::Artist))
            .tone(Tone::Quiet)
            .underline(true)
            .max_w_full()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
    }

    fn tracks_body(&self, cx: &Context<Self>) -> AnyElement {
        if self.rows.is_empty() {
            return EmptyState::new(t("no_tracks_found")).into_any_element();
        }
        let hover = style::white(style::RAISED, cx);
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
                                .id(("album-disc", disc as usize))
                                .w_full()
                                .p_2()
                                .rounded(style::radius_lg())
                                .hover(move |heading| heading.bg(hover))
                                .child(
                                    // TODO(i18n): en.ftl has no disc-heading key.
                                    SectionHeading::new(format!("Disc {disc}"))
                                        .heading(Heading::Group)
                                        .icon(icons::DISC),
                                ),
                        )
                    })
                    .child(
                        div()
                            .w_full()
                            .h(style::media_row() * group.count as f32)
                            .child(group.list.clone()),
                    )
            }))
            .into_any_element()
    }

    fn selection_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_selecting(cx) {
            return None;
        }
        let keys = self.selected_keys(cx);
        let first = self
            .rows
            .iter()
            .find(|row| keys.first() == Some(&row.key))
            .map(|row| row.title.clone())
            .unwrap_or_default();
        let picked = keys.iter().cloned().map(SharedString::from).collect();
        let page = cx.entity().downgrade();
        let (acted, cancelled) = (page.clone(), page);
        let bar = commands::selection_bar(
            "album-selection",
            keys.len(),
            Target::selection(picked, first),
            move |command, target, window, cx| {
                command.perform(target, window, cx);
                acted.update(cx, |this, cx| this.clear_selection(cx)).ok();
            },
            cx,
        )
        .lift(rems(6.))
        .on_cancel(move |_, cx| {
            cancelled
                .update(cx, |this, cx| this.clear_selection(cx))
                .ok();
        });
        let bar = match self.capabilities.delete_from_disk {
            true => bar.on_delete(move |_, cx| ops::delete_tracks(keys.clone(), cx)),
            false => bar,
        };
        Some(bar.into_any_element())
    }

    fn standard_detail(
        &self,
        album: AlbumInfo,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = t_with(
            "showcase_song_count",
            &[("count", self.rows.len().to_string())],
        );
        let radius = cx.theme().radius * 3.;
        let mut header = DetailHeader::new(SharedString::from(album.title.clone()))
            .cover(self.cover(&album, HERO_COVER, radius))
            .eyebrow(Self::artist_link(
                &album.artist,
                album.artist_key.clone(),
                "album-artist",
            ))
            .meta(count)
            .meta(format!("{} {}", self.total_ms() / 60_000, t("min")))
            .action(self.shuffle_button(HERO_ACTION, false, cx))
            .action(self.play_button(HERO_ACTION))
            .action(
                IconButton::new("album-select-all", icons::CHECK)
                    .size(HERO_SECONDARY)
                    .glyph(HERO_SECONDARY * 0.33)
                    .tone(IconTone::Outline)
                    .tooltip(t("select_all"))
                    .on_click(cx.listener(|this, _, _, cx| this.select_all(cx))),
            );
        if album.year > 0 {
            header = header.meta(album.year.to_string());
        }
        if let Some(download) = self.download_all_button(HERO_SECONDARY, cx) {
            header = header.action(download);
        }
        components::page()
            .relative()
            .pb_8()
            .overflow_hidden()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .mb_8()
                    .items_center()
                    .child(components::back_button("album-back", cx)),
            )
            .child(div().flex_shrink_0().mb_12().child(header))
            .child(self.tracks_body(cx))
            .children(self.selection_bar(cx))
            .into_any_element()
    }

    fn remote_detail(
        &self,
        view: RemoteView,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let minutes = self.total_ms() / 60_000;
        let count = t_with(
            "showcase_song_count",
            &[("count", self.rows.len().to_string())],
        );
        let dot = {
            let faint = style::white(0.3, cx);
            move || div().text_color(faint).child("•")
        };
        let cover = AlbumInfo {
            id: view.id.clone(),
            title: view.title.to_string(),
            artist: view.artist.to_string(),
            artwork: view.artwork.clone(),
            ..AlbumInfo::default()
        };
        let artist_route = view.artist_key.clone().map(Route::Artist);
        let browse_id = view.browse_id.clone();
        let first_key = self.rows.first().map(|row| row.key.clone());
        let outlined = |id: &'static str, icon: &'static str| {
            IconButton::new(id, icon)
                .size(REMOTE_SECONDARY)
                .ring(REMOTE_RING.0, REMOTE_RING.1)
        };

        components::page()
            .relative()
            .pb_8()
            .overflow_hidden()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .mb_8()
                    .items_center()
                    .child(components::back_button("album-back", cx)),
            )
            .child(
                h_flex()
                    .size_full()
                    .min_h_0()
                    .gap_10()
                    .items_start()
                    .child(
                        v_flex()
                            .w(px(320.))
                            .flex_shrink_0()
                            .items_start()
                            .gap_5()
                            .pt_2()
                            .child(div().shadow_2xl().child({
                                let radius = cx.theme().radius * 2.;
                                self.cover(&cover, REMOTE_COVER, radius)
                            }))
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap_2()
                                    .child(Self::artist_link(
                                        &view.artist,
                                        view.artist_key.clone(),
                                        "album-remote-artist",
                                    ))
                                    .child(
                                        div()
                                            .text_3xl()
                                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                            .child(view.title.clone()),
                                    )
                                    .child(
                                        h_flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_x_2()
                                            .text_sm()
                                            .text_color(style::slate_400(cx))
                                            .when_some(view.year.clone(), |line, _| {
                                                line.child(
                                                    div()
                                                        .text_xs()
                                                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                                        .text_color(style::white(0.4, cx))
                                                        .child(t("album")),
                                                )
                                                .child(dot())
                                            })
                                            .child(div().child(count))
                                            .child(dot())
                                            .child(div().child(format!("{minutes} {}", t("min"))))
                                            .when_some(view.year.clone(), |line, year| {
                                                line.child(dot()).child(div().child(year))
                                            }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .mt_1()
                                    .gap_3()
                                    .items_center()
                                    .flex_wrap()
                                    .children(self.download_all_button(REMOTE_SECONDARY, cx))
                                    .children(artist_route.map(|artist_route| {
                                        outlined("album-remote-artist", icons::MIC)
                                            .tooltip(t("go_to_artist"))
                                            .on_click(move |_, _, cx| {
                                                Navigator::navigate(artist_route.clone(), cx)
                                            })
                                    }))
                                    .child(self.play_button(REMOTE_ACTION))
                                    .child(self.shuffle_button(REMOTE_SECONDARY, true, cx))
                                    .child(
                                        outlined("album-remote-share", icons::SHARE)
                                            .tooltip(t("share"))
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
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .pb_24()
                            .child(self.tracks_body(cx)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for AlbumDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        components::scroll::smooth("album-detail-scroll", &self.scroll, window, cx);
        let album = self.album.read(cx).value().cloned().flatten();
        let remote = self.remote.read(cx).value().cloned().flatten();

        if self.capabilities.albums == AlbumPresentation::Remote {
            if let Some(view) = RemoteView::of(album.as_ref(), remote.as_ref()) {
                return self.remote_detail(view, window, cx);
            }
        } else if let Some(album) = album.clone() {
            return self.standard_detail(album, window, cx);
        } else if let Some(view) = RemoteView::of(None, remote.as_ref()) {
            return self.remote_detail(view, window, cx);
        }

        match self.album.read(cx).is_settled() && self.remote.read(cx).is_settled() {
            true => EmptyState::new(t("album_not_found"))
                .icon(icons::DISC)
                .into_any_element(),
            false => Loading::new().into_any_element(),
        }
    }
}

/// The header the catalog-styled page draws, from the stored row, the catalog listing, or both.
struct RemoteView {
    id: String,
    title: SharedString,
    artist: SharedString,
    year: Option<SharedString>,
    artwork: Option<ArtworkRef>,
    browse_id: Option<String>,
    artist_key: Option<String>,
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
                artist_key: remote
                    .and_then(|detail| detail.artist_key.clone())
                    .or_else(|| album.artist_key.clone()),
            }),
            (None, Some(detail)) => Some(Self {
                id: detail.id.clone(),
                title: detail.title.clone().into(),
                artist: detail.subtitle.clone().unwrap_or_default().into(),
                year: detail.year.clone().map(SharedString::from),
                artwork: detail.artwork.clone(),
                browse_id: Some(detail.id.clone()),
                artist_key: detail.artist_key.clone(),
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
                    api.catalog_detail(CatalogDetailRequest::new(CatalogItemKind::Album, id))
                        .await
                        .map(Some)
                }
            },
            cx,
        )
    });
}
