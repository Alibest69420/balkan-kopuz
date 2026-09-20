//! One artist: the tracks showcase, or their albums.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use api::{
    AlbumInfo, AlbumPage, ArtistPage, ArtworkTarget, ArtworkUpload, JobKind, PlaylistCapability,
    SourceCapabilities, Table, TrackInfo, TrackPage,
};
use config::{AlbumSortField, AlbumViewMode, ArtistViewOrder, SortCriterion, SortDirection};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, Pixels, Rems, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div,
    px, rems,
};
use store::{
    ConfigModel, DownloadsModel, JobsModel, Link, Navigator, Notices, PlayerModel, Query, Route,
    SourcesModel, actions,
};
use widgets::format::{t, t_with, total_duration};
use widgets::{
    Cover, EmptyState, Loading, TrackActions, TrackList, TrackListEvent, TrackListOptions, columns,
    icons, motion, playlist_picker, style,
};

use crate::controls::{self, ViewModeKey};
use crate::names;

/// `w-64 h-64`: the showcase photo.
const PHOTO: f32 = 256.;
/// `minmax(180px, 1fr)`: one album card.
const CARD: f32 = 180.;
/// `w-14 h-14`: the round shuffle and play buttons.
const HERO_BUTTON: Rems = rems(3.5);
/// A `Button` sizes its glyph at 0.75× itself, and the showcase's is `text-xl`.
const HERO_GLYPH: Pixels = px(20. / 0.75);

/// A sortable column of the tracks view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Title,
    Artist,
    Album,
    Duration,
}

impl Column {
    fn label_key(self) -> &'static str {
        match self {
            Self::Title => "sort_field_title",
            Self::Artist => "sort_field_artist",
            Self::Album => "sort_field_album",
            Self::Duration => "sort_field_duration",
        }
    }
}

/// One artist from the library: tracks and albums.
pub struct ArtistDetail {
    name: SharedString,
    tracks: Entity<Query<TrackPage>>,
    artists: Entity<Query<ArtistPage>>,
    albums: Entity<Query<AlbumPage>>,
    list: Entity<TrackList>,
    column: Option<(Column, SortDirection)>,
    scroll: ScrollHandle,
    _upload: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ArtistDetail {
    pub fn new(name: SharedString, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let artist = name.to_string();
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
        let artists = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |api| async move { api.artists(store::all()).await },
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
        let list = cx.new(TrackList::new);
        let subscriptions = vec![
            cx.observe(&tracks, |this, _, cx| this.sync_rows(cx)),
            cx.observe(&artists, |_, _, cx| cx.notify()),
            cx.observe(&albums, |_, _, cx| cx.notify()),
            cx.observe(&ConfigModel::global(cx), |this, _, cx| this.sync_rows(cx)),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| this.sync_rows(cx)),
            cx.observe(&DownloadsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&JobsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&PlayerModel::global(cx), |_, _, cx| cx.notify()),
            cx.subscribe(&list, Self::on_list_event),
        ];
        Self {
            name,
            tracks,
            artists,
            albums,
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
            TrackListEvent::Delete(keys) => {
                let from_disk = capabilities(cx).delete_from_disk;
                let keys = keys.clone();
                actions::run(cx, move |api| async move {
                    api.delete_tracks(keys, from_disk).await
                });
            }
            _ => cx.notify(),
        }
    }

    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        let options = TrackListOptions {
            show_index: true,
            show_cover: controls::config(cx).show_row_images,
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
        let artists = self.artists.read(cx).value()?;
        names::photos_by_key(&artists.artists).remove(&names::normalize(&self.name))
    }

    fn artist_albums(&self, cx: &App) -> Vec<AlbumInfo> {
        let Some(page) = self.albums.read(cx).value() else {
            return Vec::new();
        };
        let wanted = self.name.to_lowercase();
        let mut albums: Vec<AlbumInfo> = page
            .albums
            .iter()
            .filter(|album| album.artist.to_lowercase() == wanted)
            .cloned()
            .collect();
        let criteria: Vec<SortCriterion<AlbumSortField>> = controls::criteria(cx);
        sort_albums(&mut albums, &criteria);
        let mut seen = HashSet::new();
        albums.retain(|album| seen.insert(album.title.trim().to_lowercase()));
        albums
    }

    /// Every album key fully downloaded, so a card knows which download entry to offer.
    fn downloaded_albums(&self, cx: &App) -> HashSet<String> {
        let downloads = DownloadsModel::global(cx);
        let downloads = downloads.read(cx);
        let Some(page) = self.tracks.read(cx).value() else {
            return HashSet::new();
        };
        let mut by_album: HashMap<String, bool> = HashMap::new();
        for track in &page.items {
            let entry = by_album.entry(track.album_id.clone()).or_insert(true);
            *entry &= downloads.is_stored(&track.key);
        }
        by_album
            .into_iter()
            .filter(|(_, all)| *all)
            .map(|(id, _)| id)
            .collect()
    }

    /// Replace the artist's photo; the daemon stores the bytes, so a path would not do.
    fn pick_photo(&mut self, cx: &mut Context<Self>) {
        let name = self.name.to_string();
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
                        target: ArtworkTarget::Artist(name),
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
        widgets::scroll::smooth("artist-detail-scroll", &self.scroll, window, cx);
        let config = controls::config(cx);
        let albums_view = config.artist_view_order == ArtistViewOrder::Albums;
        let body = if albums_view {
            self.albums_view(window, cx)
        } else {
            self.tracks_view(window, cx)
        };
        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .gap_6()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(
                        Button::new("artist-back")
                            .ghost()
                            .icon(icons::icon(icons::CHEVRON_LEFT))
                            .on_click(|_, _, cx| {
                                Navigator::global(cx).update(cx, |nav, cx| nav.go_back(cx))
                            }),
                    )
                    .child(controls::view_order_toggle(
                        &config.artist_view_order,
                        window,
                        cx,
                    )),
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
            Loading.into_any_element()
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
        window: &mut Window,
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
        let (photo_hot, photo_hover) = controls::hovered("artist-photo-hover", window, cx);
        let scrim = motion::amount(
            "artist-photo-scrim",
            if photo_hot { 1. } else { 0. },
            motion::FAST,
            window,
            cx,
        );
        let (shuffle_hot, shuffle_hover) = controls::hovered("artist-shuffle-hover", window, cx);
        let shuffle_fill = motion::color(
            "artist-shuffle-fill",
            style::white(if shuffle_hot { style::HOVER } else { 0. }, cx),
            window,
            cx,
        );
        let shuffle_text = motion::color(
            "artist-shuffle-text",
            match (shuffle, shuffle_hot) {
                (true, _) => cx.theme().primary,
                (false, true) => cx.theme().foreground,
                (false, false) => style::slate_400(cx),
            },
            window,
            cx,
        );
        // A disabled Button registers no hover listener, so its flag would stay stuck where it was.
        let (play_hot, play_hover) = controls::hovered("artist-play-hover", window, cx);
        let play_hot = play_hot && playable;
        let play_fill = motion::color(
            "artist-play-fill",
            if play_hot {
                cx.theme().primary_hover
            } else {
                cx.theme().primary
            },
            window,
            cx,
        );
        let play_text = cx.theme().primary_foreground;
        // These buttons carry no border; `tweened` still wants a colour for the edge it would paint.
        let clear = style::white(0., cx);
        let (download_hot, download_hover) = controls::hovered("artist-download-hover", window, cx);
        let download_hot = download_hot && downloadable;
        let download_fill = motion::color(
            "artist-download-fill",
            style::white(if download_hot { style::HOVER } else { 0. }, cx),
            window,
            cx,
        );
        let download_edge = motion::color(
            "artist-download-edge",
            style::white(if download_hot { 0.40 } else { style::ACTIVE }, cx),
            window,
            cx,
        );
        let download_text = motion::color(
            "artist-download-text",
            style::white(if download_hot { 1. } else { 0.70 }, cx),
            window,
            cx,
        );

        h_flex()
            .gap_8()
            .items_end()
            .child(
                div()
                    .id("artist-photo")
                    .relative()
                    .flex_shrink_0()
                    .child(
                        Cover::new(artwork.as_ref())
                            .size(px(PHOTO))
                            .hq(true)
                            .rounded(px(12.))
                            .placeholder(icons::MIC),
                    )
                    .when(capabilities.edit_tags, |photo| {
                        photo
                            .cursor_pointer()
                            // `opacity-0 hover:opacity-100 transition-opacity` over the whole photo.
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(12.))
                                    .bg(style::black(0.5, cx))
                                    .opacity(scrim)
                                    .child(
                                        icons::icon(icons::EDIT)
                                            .size(px(30.))
                                            .text_color(cx.theme().foreground),
                                    ),
                            )
                            .tooltip(|window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(t("change_photo"))
                                    .build(window, cx)
                            })
                            .on_hover(controls::track(photo_hover))
                            .on_click(cx.listener(|this, _, _, cx| this.pick_photo(cx)))
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_6()
                    .child(
                        div()
                            .text_size(rems(4.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_clamp(3)
                            .child(self.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap_6()
                            .text_color(style::slate_400(cx))
                            .child(t_with(
                                "showcase_song_count",
                                &[("count", keys.len().to_string())],
                            ))
                            .child("•")
                            .child(total_duration(total)),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .flex_shrink_0()
                    .child(controls::tweened(
                        round_button("artist-shuffle", icons::SHUFFLE)
                            .selected(shuffle)
                            .on_hover(controls::track(shuffle_hover))
                            .tooltip(if shuffle {
                                t("shuffle_on")
                            } else {
                                t("shuffle_off")
                            })
                            .on_click(move |_, _, cx| {
                                PlayerModel::global(cx)
                                    .update(cx, |player, cx| player.set_shuffle(!shuffle, cx))
                            }),
                        shuffle_fill,
                        shuffle_text,
                        clear,
                        cx,
                    ))
                    .child({
                        let play = round_button("artist-play", icons::PLAY)
                            .on_hover(controls::track(play_hover))
                            .tooltip(t("play"))
                            .disabled(!playable)
                            .on_click({
                                let keys = keys.clone();
                                move |_, _, cx| actions::play_all(keys.clone(), cx)
                            });
                        if playable {
                            controls::tweened(play, play_fill, play_text, clear, cx)
                        } else {
                            play.primary()
                        }
                    })
                    .when(capabilities.downloads, |row| {
                        let download = Button::new("artist-download-all")
                            .size_12()
                            .rounded_full()
                            .border_1()
                            .on_hover(controls::track(download_hover))
                            .icon(icons::icon(if stored {
                                icons::TRASH
                            } else {
                                icons::DOWNLOAD
                            }))
                            .tooltip(if stored {
                                t("remove_download")
                            } else {
                                t("download_offline")
                            })
                            .disabled(!downloadable)
                            .on_click({
                                let keys = keys.clone();
                                move |_, _, cx| set_downloaded(keys.clone(), !stored, cx)
                            });
                        row.child(if downloadable {
                            controls::tweened(
                                download,
                                download_fill,
                                download_text,
                                download_edge,
                                cx,
                            )
                        } else {
                            download
                                .ghost()
                                .border_color(style::white(style::ACTIVE, cx))
                                .text_color(style::white(0.70, cx))
                        })
                    }),
            )
            .into_any_element()
    }

    fn column_header(&mut self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let all_selected = count > 0 && count == self.list.read(cx).selected_keys().len();
        // Built from the row's own cells, so each sort label sits over its column.
        columns::frame()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                columns::fixed(columns::INDEX).child(
                    Checkbox::new("artist-select-all")
                        .checked(all_selected)
                        .accessibility_label(t("select_all"))
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            this.list.update(cx, |list, cx| {
                                if *checked {
                                    list.select_all(cx);
                                } else {
                                    list.clear_selection(cx);
                                }
                            });
                        })),
                ),
            )
            .child(
                columns::flexible(columns::TITLE_MIN, 2.)
                    .child(self.column_label(Column::Title, cx)),
            )
            .child(
                columns::flexible(columns::TEXT_MIN, 1.)
                    .child(self.column_label(Column::Artist, cx)),
            )
            .child(
                columns::flexible(columns::TEXT_MIN, 1.)
                    .child(self.column_label(Column::Album, cx)),
            )
            .child(
                columns::fixed(columns::DURATION)
                    .justify_end()
                    .child(self.column_label(Column::Duration, cx)),
            )
            .child(columns::fixed(columns::MENU))
            .into_any_element()
    }

    fn column_label(&self, column: Column, cx: &mut Context<Self>) -> AnyElement {
        let marker = match self.column {
            Some((current, SortDirection::Asc)) if current == column => Some(icons::CHEVRON_UP),
            Some((current, SortDirection::Desc)) if current == column => Some(icons::CHEVRON_DOWN),
            _ => None,
        };
        h_flex()
            .id(SharedString::from(format!("column-{column:?}")))
            .gap_1()
            .items_center()
            .hover(|label| label.text_color(cx.theme().foreground))
            .child(t(column.label_key()))
            .children(marker.map(icons::icon))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_column(column, cx)))
            .into_any_element()
    }

    fn selection_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let capabilities = capabilities(cx);
        let selected = self.list.read(cx).selected_keys();
        let count = selected.len();
        h_flex()
            .absolute()
            .bottom_24()
            .left_0()
            .right_0()
            .justify_center()
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .px_6()
                    .py_2p5()
                    .rounded_full()
                    .shadow_lg()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().primary_foreground)
                    // TODO(i18n): the Dioxus frontend had no key for this either.
                    .child(SharedString::from(format!("{count} selected")))
                    .child(
                        Button::new("selection-queue")
                            .ghost()
                            .icon(icons::icon(icons::QUEUE_ADD))
                            .label(t("add_to_queue"))
                            .on_click({
                                let selected = selected.clone();
                                move |_, _, cx| actions::append(selected.clone(), cx)
                            }),
                    )
                    .when(capabilities.playlists != PlaylistCapability::None, |bar| {
                        bar.child(
                            Button::new("selection-playlist")
                                .ghost()
                                .icon(icons::icon(icons::PLUS))
                                .label(t("add_to_playlist"))
                                .on_click({
                                    let selected = selected.clone();
                                    move |_, window, cx| {
                                        playlist_picker::open(selected.clone(), window, cx)
                                    }
                                }),
                        )
                    })
                    .when(capabilities.delete_from_disk, |bar| {
                        bar.child(
                            Button::new("selection-delete")
                                .ghost()
                                .icon(icons::icon(icons::TRASH))
                                .label(t("delete"))
                                .on_click(cx.listener({
                                    let selected = selected.clone();
                                    move |this, _, _, cx| {
                                        let keys = selected.clone();
                                        actions::run(cx, move |api| async move {
                                            api.delete_tracks(keys, true).await
                                        });
                                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                                    }
                                })),
                        )
                    })
                    .child(
                        Button::new("selection-cancel")
                            .ghost()
                            .icon(icons::icon(icons::CLOSE))
                            .tooltip(t("cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.list.update(cx, |list, cx| list.clear_selection(cx))
                            })),
                    ),
            )
            .into_any_element()
    }

    fn albums_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let albums = self.artist_albums(cx);
        let downloaded = self.downloaded_albums(cx);
        let mode = controls::config(cx).artist_album_view_mode;
        let settled = self.albums.read(cx).is_settled();
        let fields = available_fields(&albums);

        let body = if !settled && albums.is_empty() {
            Loading.into_any_element()
        } else if albums.is_empty() {
            div()
                .text_color(style::slate_500(cx))
                .child(t("no_albums_found"))
                .into_any_element()
        } else {
            let cards: Vec<AnyElement> = albums
                .iter()
                .map(|album| {
                    let stored = downloaded.contains(&album.id);
                    match mode {
                        AlbumViewMode::Grid => album_card(album, stored, window, cx),
                        AlbumViewMode::List => album_row(album, stored, cx),
                    }
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
                    .child(controls::view_mode_toggle(
                        ViewModeKey::ArtistAlbums,
                        window,
                        cx,
                    ))
                    .child(controls::sort_control("artist-album-sort", fields, cx)),
            )
            .child(body)
            .into_any_element()
    }
}

/// `w-14 h-14 rounded-full` with a `text-xl` glyph: the showcase's shuffle and play.
fn round_button(id: &'static str, icon: &'static str) -> Button {
    Button::new(id)
        .with_size(Size::Size(HERO_GLYPH))
        .size(HERO_BUTTON)
        .rounded_full()
        .icon(icons::icon(icon))
}

fn album_card(album: &AlbumInfo, stored: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let menu = album_menu(album.id.clone(), stored);
    let route = Route::Album(album.id.clone().into());
    let group = SharedString::from(format!("album-card-{}", album.id));
    let (hot, hover) = controls::hovered(
        SharedString::from(format!("album-card-hover-{}", album.id)),
        window,
        cx,
    );
    let fill = motion::color(
        SharedString::from(format!("album-card-fill-{}", album.id)),
        style::white(if hot { style::HOVER } else { style::RAISED }, cx),
        window,
        cx,
    );
    let cover = motion::zoom(
        SharedString::from(format!("album-card-cover-{}", album.id)),
        px(CARD - 32.),
        hot,
        window,
        cx,
    );
    div()
        .id(SharedString::from(format!("album-card-{}", album.id)))
        .group(group.clone())
        .relative()
        .w(px(CARD))
        .p_4()
        .rounded(style::radius_xl())
        .bg(fill)
        .cursor_pointer()
        .on_hover(controls::track(hover))
        .child(
            v_flex()
                .id(SharedString::from(format!("album-open-{}", album.id)))
                .gap_3()
                .child(
                    // `group-hover:scale-105` reads as the cover growing inside a frame that clips it.
                    div()
                        .size(px(CARD - 32.))
                        .rounded(px(8.))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Cover::new(album.artwork.as_ref())
                                .size(cover)
                                .rounded(px(8.))
                                .placeholder(icons::DISC),
                        ),
                )
                .child(
                    v_flex()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .child(album.title.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .text_color(style::slate_400(cx))
                                .child(album.artist.clone()),
                        ),
                )
                .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx)),
        )
        .child(
            div()
                .absolute()
                .bottom_3()
                .right_3()
                .invisible()
                .group_hover(group, |menu| menu.visible())
                .child(
                    Button::new(SharedString::from(format!("album-menu-{}", album.id)))
                        .ghost()
                        .xsmall()
                        .bg(style::black(0.4, cx))
                        .icon(icons::icon(icons::MORE))
                        .dropdown_menu(menu.clone()),
                ),
        )
        .context_menu(menu)
        .into_any_element()
}

fn album_row(album: &AlbumInfo, stored: bool, cx: &App) -> AnyElement {
    let menu = album_menu(album.id.clone(), stored);
    let route = Route::Album(album.id.clone().into());
    h_flex()
        .id(SharedString::from(format!("album-row-{}", album.id)))
        .w_full()
        .gap_3()
        .px_3()
        .py_2()
        .rounded(style::radius_lg())
        .bg(style::white(style::RAISED, cx))
        .cursor_pointer()
        .hover(|row| row.bg(style::white(style::HOVER, cx)))
        .child(
            Cover::new(album.artwork.as_ref())
                .size(px(48.))
                .rounded(px(6.))
                .placeholder(icons::DISC),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(album.title.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .text_color(style::slate_400(cx))
                        .child(album.artist.clone()),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("album-row-menu-{}", album.id)))
                .ghost()
                .xsmall()
                .icon(icons::icon(icons::MORE))
                .dropdown_menu(menu.clone()),
        )
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .context_menu(menu)
        .into_any_element()
}

/// The album card menu, gated on what the active source can do.
fn album_menu(
    id: String,
    stored: bool,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + Clone + 'static {
    move |menu, _, cx| {
        let capabilities = capabilities(cx);
        let mut menu = menu.item(
            PopupMenuItem::new(t("add_all_to_queue"))
                .icon(icons::icon(icons::QUEUE_ADD))
                .on_click({
                    let id = id.clone();
                    move |_, _, cx| with_album_keys(id.clone(), cx, actions::append)
                }),
        );
        if capabilities.playlists != PlaylistCapability::None {
            menu = menu.item(
                PopupMenuItem::new(t("add_all_to_playlist"))
                    .icon(icons::icon(icons::PLUS))
                    .on_click({
                        let id = id.clone();
                        move |_, window, cx| {
                            let handle = window.window_handle();
                            with_album_keys(id.clone(), cx, move |keys, cx| {
                                handle
                                    .update(cx, |_, window, cx| {
                                        playlist_picker::open(keys, window, cx)
                                    })
                                    .ok();
                            })
                        }
                    }),
            );
        }
        if capabilities.delete_from_disk {
            menu = menu.item(
                PopupMenuItem::new(t("delete_album"))
                    .icon(icons::icon(icons::TRASH))
                    .on_click({
                        let id = id.clone();
                        move |_, _, cx| {
                            let id = id.clone();
                            actions::run(
                                cx,
                                move |api| async move { api.delete_album(id, true).await },
                            );
                        }
                    }),
            );
        }
        if capabilities.downloads {
            menu = menu.item(
                PopupMenuItem::new(if stored {
                    t("remove_download")
                } else {
                    t("download_offline")
                })
                .icon(icons::icon(if stored {
                    icons::TRASH
                } else {
                    icons::DOWNLOAD
                }))
                .on_click({
                    let id = id.clone();
                    move |_, _, cx| {
                        with_album_keys(id.clone(), cx, move |keys, cx| {
                            set_downloaded(keys, !stored, cx)
                        })
                    }
                }),
            );
        }
        menu
    }
}

/// Read an album's tracks in play order, then hand their keys to `then`.
fn with_album_keys(id: String, cx: &mut App, then: impl FnOnce(Vec<String>, &mut App) + 'static) {
    let call =
        Link::global(cx).call(move |api| async move { api.album_tracks(id, store::all()).await });
    cx.spawn(async move |cx| match call.await {
        Ok(page) => {
            let mut tracks = page.items;
            tracks.sort_by(|left, right| {
                left.track_number
                    .cmp(&right.track_number)
                    .then_with(|| left.title.cmp(&right.title))
            });
            let keys = tracks.into_iter().map(|track| track.key).collect();
            cx.update(|cx| then(keys, cx));
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
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
