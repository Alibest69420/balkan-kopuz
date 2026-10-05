//! One artist: the tracks showcase, or their albums.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;

use api::{
    AlbumInfo, ArtworkTarget, ArtworkUpload, JobKind, SourceCapabilities, Table, TrackInfo,
    TrackPage,
};
use components::format::{t, t_with, total_duration};
use components::{
    DetailHeader, EmptyState, IconButton, IconTone, Loading, TrackActions, TrackHeader, TrackList,
    TrackListEvent, TrackListOptions, icons,
};
use config::{AlbumSortField, AlbumViewMode, ArtistViewOrder, SortCriterion, SortDirection};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, px, rems,
};
use store::{
    ConfigModel, DownloadsModel, JobsModel, PlayerModel, Query, SourcesModel, Target, actions,
};

use crate::controls::{self, ViewModeKey};

/// `w-64 h-64`: the showcase photo.
const PHOTO: f32 = 256.;
/// `minmax(180px, 1fr)`: one album card.
const CARD: f32 = 180.;
/// `w-14 h-14`: the round shuffle and play buttons.
const HERO_BUTTON: Pixels = px(56.);
/// `w-12 h-12`: the outlined download beside them.
const HERO_SECONDARY: Pixels = px(48.);
/// `text-xl`, the showcase's glyph.
const HERO_GLYPH: Pixels = px(20.);

/// A sortable column of the tracks view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Title,
    Artist,
    Album,
    Duration,
}

impl Column {
    /// The table column it sorts, by the id the shared header names it with.
    fn id(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Duration => "duration",
        }
    }

    fn parse(id: &str) -> Option<Self> {
        [Self::Title, Self::Artist, Self::Album, Self::Duration]
            .into_iter()
            .find(|column| column.id() == id)
    }
}

/// One artist from the library: tracks and albums.
pub struct ArtistDetail {
    key: String,
    tracks: Entity<Query<TrackPage>>,
    detail: Entity<Query<api::ArtistDetail>>,
    list: Entity<TrackList>,
    column: Option<(Column, SortDirection)>,
    scroll: ScrollHandle,
    _upload: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ArtistDetail {
    pub fn new(key: String, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let artist = key.clone();
        let tracks = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                move |api| {
                    let artist = artist.clone();
                    async move { api.artist_tracks(artist, store::all()).await }
                },
                cx,
            )
        });
        let artist = key.clone();
        let detail = cx.new(|cx| {
            Query::new(
                &[Table::Tracks, Table::Albums],
                move |api| {
                    let artist = artist.clone();
                    async move { api.artist(artist).await }
                },
                cx,
            )
        });
        let list = cx.new(TrackList::new);
        commands::bind_track_list(&list, cx);
        let subscriptions = vec![
            cx.observe(&tracks, |this, _, cx| this.sync_rows(cx)),
            cx.observe(&detail, |_, _, cx| cx.notify()),
            cx.observe(&ConfigModel::global(cx), |this, _, cx| this.sync_rows(cx)),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| this.sync_rows(cx)),
            cx.observe(&DownloadsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&JobsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&PlayerModel::global(cx), |_, _, cx| cx.notify()),
            cx.subscribe(&list, Self::on_list_event),
        ];
        Self {
            key,
            tracks,
            detail,
            list,
            column: None,
            scroll: ScrollHandle::new(),
            _upload: None,
            _subscriptions: subscriptions,
        }
    }

    fn on_list_event(
        &mut self,
        list: Entity<TrackList>,
        event: &TrackListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TrackListEvent::Play(index) => {
                let keys = list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
            TrackListEvent::Delete(keys) => delete_tracks(keys.clone(), cx),
            _ => cx.notify(),
        }
    }

    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        let options = TrackListOptions {
            show_index: true,
            show_cover: true,
            show_album: true,
            track_numbers: false,
            selectable: true,
        };
        let row_actions = TrackActions {
            remove_from_playlist: false,
            delete: capabilities(cx).delete_from_disk,
            reorder: false,
        };
        let rows = self.rows(cx);
        self.list.update(cx, |list, cx| {
            list.set_options(options, cx);
            list.set_actions(row_actions, cx);
            list.set_rows(rows, cx);
        });
        cx.notify();
    }

    fn rows(&self, cx: &App) -> Vec<TrackInfo> {
        // TODO(offline): filter to downloaded tracks when offline.
        let mut rows = self
            .tracks
            .read(cx)
            .value()
            .map(|page| page.items.clone())
            .unwrap_or_default();
        if let Some((column, direction)) = self.column {
            sort_tracks(&mut rows, column, direction);
        }
        rows
    }

    fn toggle_column(&mut self, column: Column, cx: &mut Context<Self>) {
        self.column = match self.column {
            Some((current, SortDirection::Asc)) if current == column => {
                Some((column, SortDirection::Desc))
            }
            Some((current, SortDirection::Desc)) if current == column => None,
            _ => Some((column, SortDirection::Asc)),
        };
        self.sync_rows(cx);
    }

    fn artwork(&self, cx: &App) -> Option<api::ArtworkRef> {
        self.detail.read(cx).value()?.info.artwork.clone()
    }

    /// Blank until the daemon answers: the key carries no display name.
    fn name(&self, cx: &App) -> SharedString {
        self.detail
            .read(cx)
            .value()
            .map(|detail| SharedString::from(detail.info.name.clone()))
            .unwrap_or_default()
    }

    fn artist_albums(&self, cx: &App) -> Vec<AlbumInfo> {
        let Some(detail) = self.detail.read(cx).value() else {
            return Vec::new();
        };
        let mut albums = detail.albums.clone();
        let criteria: Vec<SortCriterion<AlbumSortField>> = controls::criteria(cx);
        sort_albums(&mut albums, &criteria);
        let mut seen = HashSet::new();
        albums.retain(|album| seen.insert(album.title.trim().to_lowercase()));
        albums
    }

    fn pick_photo(&mut self, cx: &mut Context<Self>) {
        let artist = self.key.clone();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        self._upload = Some(cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let content_type = content_type_for(&path);
            let read = cx
                .background_executor()
                .spawn(async move { std::fs::read(&path) });
            let bytes = match read.await {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::warn!(%error, "could not read the picked picture");
                    return;
                }
            };
            cx.update(|cx| {
                actions::run(cx, move |api| async move {
                    api.upload_artwork(ArtworkUpload {
                        target: ArtworkTarget::Artist(artist),
                        content_type,
                        bytes,
                    })
                    .await
                })
            });
        }));
    }
}

impl Render for ArtistDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        components::scroll::smooth("artist-detail-scroll", &self.scroll, window, cx);
        let config = controls::config(cx);
        let albums_view = config.artist_view_order == ArtistViewOrder::Albums;
        let failed = {
            let detail = self.detail.read(cx);
            detail
                .value()
                .is_none()
                .then(|| detail.error().cloned())
                .flatten()
        };
        let body = if let Some(error) = failed {
            let message = match error.code {
                api::ErrorCode::NotFound => t("artist_not_found"),
                _ => t_with(
                    "artist_load_failed",
                    &[("error", store::human(&error).to_string())],
                ),
            };
            EmptyState::new(message).icon(icons::MIC).into_any_element()
        } else if albums_view {
            self.albums_view(cx)
        } else {
            self.tracks_view(window, cx)
        };
        components::page()
            .gap_6()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(components::back_button("artist-back", cx))
                    .child(controls::view_order_toggle(&config.artist_view_order)),
            )
            .child(body)
    }
}

impl ArtistDetail {
    fn tracks_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (keys, total, selecting) = {
            let list = self.list.read(cx);
            let keys: Vec<String> = list.keys();
            let total: u64 = list
                .rows()
                .iter()
                .map(|row| row.duration_ms.unwrap_or_default())
                .sum();
            (keys, total, list.is_selecting())
        };
        let count = keys.len();
        let settled = self.tracks.read(cx).is_settled();
        let body = if !settled && count == 0 {
            Loading::new().into_any_element()
        } else if count == 0 {
            EmptyState::new(t("no_tracks_found"))
                .icon(icons::MUSIC)
                .into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_h_0()
                .child(self.column_header(count, cx))
                .child(div().flex_1().min_h_0().child(self.list.clone()))
                .into_any_element()
        };
        v_flex()
            .relative()
            .flex_1()
            .min_h_0()
            .gap_12()
            .child(self.hero(keys, total, window, cx))
            .child(body)
            .when(selecting, |view| view.child(self.selection_bar(cx)))
            .into_any_element()
    }

    fn hero(
        &mut self,
        keys: Vec<String>,
        total: u64,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let capabilities = capabilities(cx);
        let artwork = self.artwork(cx);
        let shuffle = PlayerModel::global(cx).read(cx).shuffle();
        let stored = {
            let downloads = DownloadsModel::global(cx);
            let downloads = downloads.read(cx);
            !keys.is_empty() && keys.iter().all(|key| downloads.is_stored(key))
        };
        let busy = JobsModel::global(cx).read(cx).is_running(JobKind::Download);
        let playable = !keys.is_empty();
        let downloadable = playable && !busy;
        let page = cx.entity().downgrade();
        let mut header = DetailHeader::new(self.name(cx))
            .artwork(artwork.as_ref())
            .placeholder(icons::MIC)
            .size(px(PHOTO))
            .when(capabilities.edit_tags, |header| {
                header.on_cover("artist-photo", Some(t("change_photo")), move |_, cx| {
                    page.update(cx, |this, cx| this.pick_photo(cx)).ok();
                })
            })
            .meta(t_with(
                "showcase_song_count",
                &[("count", keys.len().to_string())],
            ))
            .meta(total_duration(total))
            .action(
                IconButton::new("artist-shuffle", icons::SHUFFLE)
                    .size(HERO_BUTTON)
                    .glyph(HERO_GLYPH)
                    .lit(shuffle)
                    .tooltip(if shuffle {
                        t("shuffle_on")
                    } else {
                        t("shuffle_off")
                    })
                    .on_click(move |_, _, cx| {
                        PlayerModel::global(cx)
                            .update(cx, |player, cx| player.set_shuffle(!shuffle, cx))
                    }),
            )
            .action(
                IconButton::new("artist-play", icons::PLAY)
                    .size(HERO_BUTTON)
                    .glyph(HERO_GLYPH)
                    .tone(IconTone::Accent)
                    .tooltip(t("play"))
                    .disabled(!playable)
                    .on_click({
                        let keys = keys.clone();
                        move |_, _, cx| actions::play_all(keys.clone(), cx)
                    }),
            );
        if capabilities.downloads {
            header = header.action(
                IconButton::new(
                    "artist-download-all",
                    if stored {
                        icons::TRASH
                    } else {
                        icons::DOWNLOAD
                    },
                )
                .size(HERO_SECONDARY)
                .tone(IconTone::Outline)
                .tooltip(if stored {
                    t("remove_download")
                } else {
                    t("download_offline")
                })
                .disabled(!downloadable)
                .on_click({
                    let keys = keys.clone();
                    move |_, _, cx| set_downloaded(keys.clone(), !stored, cx)
                }),
            );
        }
        header.into_any_element()
    }

    fn column_header(&mut self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let list = self.list.read(cx);
        let all_selected = count > 0 && count == list.selected_count();
        let active = self
            .column
            .map(|(column, direction)| (column.id(), direction == SortDirection::Desc));
        TrackHeader::new("artist-header", list.layout())
            .sort(
                active,
                |id| Column::parse(id).is_some(),
                cx.listener(|this, id: &str, _, cx| {
                    if let Some(column) = Column::parse(id) {
                        this.toggle_column(column, cx);
                    }
                }),
            )
            .select_all(
                all_selected,
                cx.listener(move |this, checked: &bool, _, cx| {
                    this.list.update(cx, |list, cx| {
                        if *checked {
                            list.select_all(cx);
                        } else {
                            list.clear_selection(cx);
                        }
                    });
                }),
            )
            .into_any_element()
    }

    fn selection_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (keys, first) = {
            let list = self.list.read(cx);
            let keys = list.selected_keys();
            let first = list
                .rows()
                .iter()
                .find(|row| keys.first() == Some(&row.key))
                .map(|row| row.title.clone())
                .unwrap_or_default();
            (keys, first)
        };
        let picked = keys.iter().cloned().map(SharedString::from).collect();
        let (acted, cancelled) = (self.list.downgrade(), self.list.downgrade());
        let bar = commands::selection_bar(
            "artist-selection",
            keys.len(),
            Target::selection(picked, first),
            move |command, target, window, cx| {
                command.perform(target, window, cx);
                acted.update(cx, |list, cx| list.clear_selection(cx)).ok();
            },
            cx,
        )
        .lift(rems(6.))
        .on_cancel(move |_, cx| {
            cancelled
                .update(cx, |list, cx| list.clear_selection(cx))
                .ok();
        });
        let bar = match capabilities(cx).delete_from_disk {
            true => bar.on_delete(move |_, cx| delete_tracks(keys.clone(), cx)),
            false => bar,
        };
        bar.into_any_element()
    }

    fn albums_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let albums = self.artist_albums(cx);
        let mode = controls::config(cx).artist_album_view_mode;
        let settled = self.detail.read(cx).is_settled();
        let fields = available_fields(&albums);

        let body = if !settled && albums.is_empty() {
            Loading::new().into_any_element()
        } else if albums.is_empty() {
            EmptyState::new(t("no_albums_found"))
                .icon(icons::DISC)
                .into_any_element()
        } else {
            let cards: Vec<AnyElement> = albums
                .iter()
                .map(|album| match mode {
                    AlbumViewMode::Grid => components::album_tile(album)
                        .size(px(CARD - 32.))
                        .frame(components::Frame::Card)
                        .into_any_element(),
                    AlbumViewMode::List => components::album_row(album).into_any_element(),
                })
                .collect();
            div()
                .id("artist-albums-scroll")
                .flex_1()
                .min_h_0()
                .pb_20()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(match mode {
                    AlbumViewMode::Grid => div()
                        .flex()
                        .flex_wrap()
                        .gap_6()
                        .children(cards)
                        .into_any_element(),
                    AlbumViewMode::List => v_flex().gap_2().children(cards).into_any_element(),
                })
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .items_center()
                    .child(controls::view_mode_toggle(ViewModeKey::ArtistAlbums, cx))
                    .child(controls::sort_control("artist-album-sort", fields, cx)),
            )
            .child(body)
            .into_any_element()
    }
}

fn set_downloaded(keys: Vec<String>, download: bool, cx: &mut App) {
    DownloadsModel::global(cx).update(cx, |downloads, cx| {
        if download {
            downloads.download(keys, cx);
        } else {
            for key in keys {
                downloads.remove(key, cx);
            }
        }
    });
}

fn delete_tracks(keys: Vec<String>, cx: &mut App) {
    let from_disk = capabilities(cx).delete_from_disk;
    actions::run(cx, move |api| async move {
        api.delete_tracks(keys, from_disk).await
    });
}

fn capabilities(cx: &App) -> SourceCapabilities {
    let model = SourcesModel::global(cx);
    model.read(cx).capabilities()
}

/// Every album here shares the artist, so that field would never break a tie.
fn available_fields(albums: &[AlbumInfo]) -> Vec<AlbumSortField> {
    let mut fields = vec![AlbumSortField::Title];
    if albums.iter().any(|album| album.year > 0) {
        fields.push(AlbumSortField::Year);
    }
    if albums.iter().any(|album| !album.genre.trim().is_empty()) {
        fields.push(AlbumSortField::Genre);
    }
    fields
}

fn sort_albums(albums: &mut [AlbumInfo], criteria: &[SortCriterion<AlbumSortField>]) {
    albums.sort_by(|left, right| {
        for criterion in criteria {
            let ordering = match criterion.field {
                AlbumSortField::Title => compare_text(&left.title, &right.title),
                AlbumSortField::Artist => compare_text(&left.artist, &right.artist),
                AlbumSortField::Year => left.year.cmp(&right.year),
                AlbumSortField::Genre => compare_text(&left.genre, &right.genre),
            };
            let ordering = match criterion.direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });
}

fn sort_tracks(tracks: &mut [TrackInfo], column: Column, direction: SortDirection) {
    tracks.sort_by(|left, right| {
        let ordering = match column {
            Column::Title => compare_text(&left.title, &right.title),
            Column::Artist => compare_text(&left.artist, &right.artist),
            Column::Album => compare_text(&left.album, &right.album),
            Column::Duration => left.duration_ms.cmp(&right.duration_ms),
        };
        let ordering = match direction {
            SortDirection::Asc => ordering,
            SortDirection::Desc => ordering.reverse(),
        };
        match column {
            Column::Album => ordering
                .then_with(|| {
                    left.disc_number
                        .unwrap_or(0)
                        .cmp(&right.disc_number.unwrap_or(0))
                })
                .then_with(|| {
                    left.track_number
                        .unwrap_or(0)
                        .cmp(&right.track_number.unwrap_or(0))
                })
                .then_with(|| compare_text(&left.title, &right.title)),
            _ => ordering,
        }
    });
}

fn compare_text(left: &str, right: &str) -> Ordering {
    left.trim().to_lowercase().cmp(&right.trim().to_lowercase())
}

/// The daemon stores the bytes, so it needs to be told what they are.
fn content_type_for(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
    .to_string()
}
