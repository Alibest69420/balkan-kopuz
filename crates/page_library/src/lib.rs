//! The Library page: the source's whole track listing, its stat cards, the stacked sort control and multi-select.

mod sort;
mod stats;

use std::collections::HashSet;

use api::{AlbumPage, ArtistPage, JobKind, Table, TrackFilter, TrackPage, TrackSort};
use config::{SortCriterion, TrackSortField};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Render,
    SharedString, Styled, Subscription, Window, div, px,
};
use store::{ConfigModel, JobsModel, PlaylistsModel, Query, SourcesModel, actions};
use widgets::format::t;
use widgets::track_list::{TrackActions, TrackList, TrackListEvent, TrackListOptions};
use widgets::{EmptyState, Loading, PageHeader, icons, playlist_picker};

use crate::stats::StatCard;

pub struct Library {
    tracks: Entity<Query<TrackPage>>,
    albums: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    list: Entity<TrackList>,
    sort: Vec<SortCriterion<TrackSortField>>,
    // The empty-library pull runs once per visit, like the Dioxus page's `has_fetched`.
    auto_synced: bool,
    _subscriptions: Vec<Subscription>,
}

fn filter(sort: &[SortCriterion<TrackSortField>]) -> TrackFilter {
    TrackFilter {
        sort: TrackSort::Fields(sort.to_vec()),
        ..Default::default()
    }
}

impl Library {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = window;
        let config = ConfigModel::global(cx);
        let sources = SourcesModel::global(cx);
        let jobs = JobsModel::global(cx);
        let playlists = PlaylistsModel::global(cx);
        let sort = config.read(cx).config().library_sort.clone();

        // TODO(paging): §2.6 wants the visible Page{offset,limit} only; the shared TrackList owns its scroll, so the page holds the whole listing.
        let listing = filter(&sort);
        let tracks = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                move |api| {
                    let listing = listing.clone();
                    async move { api.tracks(listing, store::all()).await }
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
        let artists = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |api| async move { api.artists(store::all()).await },
                cx,
            )
        });

        let list = cx.new(TrackList::new);
        let subscriptions = vec![
            cx.observe(&tracks, |this, _, cx| this.adopt_tracks(cx)),
            cx.observe(&albums, |_, _, cx| cx.notify()),
            cx.observe(&artists, |_, _, cx| cx.notify()),
            cx.observe(&playlists, |_, _, cx| cx.notify()),
            cx.observe(&jobs, |_, _, cx| cx.notify()),
            cx.observe(&config, |this, _, cx| this.adopt_config(cx)),
            cx.observe(&sources, |this, _, cx| {
                this.adopt_capabilities(cx);
                this.auto_sync(cx);
            }),
            cx.subscribe(&list, |this, _, event: &TrackListEvent, cx| {
                this.on_list_event(event, cx)
            }),
        ];

        let mut page = Self {
            tracks,
            albums,
            artists,
            list,
            sort,
            auto_synced: false,
            _subscriptions: subscriptions,
        };
        page.adopt_capabilities(cx);
        page
    }

    fn total(&self, cx: &Context<Self>) -> Option<u32> {
        self.tracks.read(cx).value().map(|page| page.total)
    }

    fn adopt_tracks(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .tracks
            .read(cx)
            .value()
            .map(|page| page.items.clone())
            .unwrap_or_default();
        self.list.update(cx, |list, cx| list.set_rows(rows, cx));
        self.auto_sync(cx);
        cx.notify();
    }

    /// The sort control writes `cfg.library_sort`; the listing follows it from here.
    fn adopt_config(&mut self, cx: &mut Context<Self>) {
        self.adopt_capabilities(cx);
        let sort = ConfigModel::global(cx)
            .read(cx)
            .config()
            .library_sort
            .clone();
        if sort == self.sort {
            return;
        }
        self.sort = sort;
        let listing = filter(&self.sort);
        self.tracks.update(cx, |query, cx| {
            query.set_fetch(
                move |api| {
                    let listing = listing.clone();
                    async move { api.tracks(listing, store::all()).await }
                },
                cx,
            )
        });
    }

    fn adopt_capabilities(&mut self, cx: &mut Context<Self>) {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let options = TrackListOptions {
            show_index: true,
            show_cover: covers,
            show_album: true,
            track_numbers: false,
            selectable: true,
        };
        let row_actions = TrackActions {
            remove_from_playlist: false,
            delete: capabilities.delete_from_disk,
            reorder: false,
        };
        self.list.update(cx, |list, cx| {
            list.set_options(options, cx);
            list.set_actions(row_actions, cx);
        });
        cx.notify();
    }

    /// A server library that answers "no tracks" on the first visit is pulled once.
    fn auto_sync(&mut self, cx: &mut Context<Self>) {
        if self.auto_synced || !SourcesModel::global(cx).read(cx).capabilities().sync {
            return;
        }
        let Some(total) = self.total(cx) else { return };
        self.auto_synced = true;
        if total == 0 {
            JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(JobKind::LibrarySync, cx));
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let kind = if capabilities.scan_folders {
            JobKind::Scan
        } else if capabilities.sync {
            JobKind::LibrarySync
        } else {
            return;
        };
        JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(kind, cx));
    }

    fn on_list_event(&mut self, event: &TrackListEvent, cx: &mut Context<Self>) {
        match event {
            // The held rows are the whole filtered listing, so the clicked index is the queue's.
            TrackListEvent::Play(index) => {
                let keys = self.list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
            TrackListEvent::Delete(keys) => self.delete(keys.clone(), cx),
            _ => cx.notify(),
        }
    }

    fn delete(&mut self, keys: Vec<String>, cx: &mut Context<Self>) {
        if keys.is_empty() {
            return;
        }
        let from_disk = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        actions::run(cx, move |api| async move {
            api.delete_tracks(keys, from_disk).await
        });
        self.list.update(cx, |list, cx| list.clear_selection(cx));
    }

    fn toggle_select_all(&mut self, all_selected: bool, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            if all_selected {
                list.clear_selection(cx);
            } else {
                list.select_all(cx);
            }
        });
    }

    fn selection_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let deletable = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        h_flex()
            .w_full()
            .mb_3()
            .gap_3()
            .items_center()
            .px_4()
            .py_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().accent)
            .child(
                // TODO(i18n): the Dioxus selection bar spells this count in English too.
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(format!("{count} selected"))),
            )
            .child(
                Button::new("selection-queue")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::QUEUE_ADD))
                    .label(t("add_to_queue"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let keys = this.list.read(cx).selected_keys();
                        actions::append(keys, cx);
                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                    })),
            )
            .child(
                Button::new("selection-playlist")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::PLUS))
                    .label(t("add_to_playlist"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let keys = this.list.read(cx).selected_keys();
                        playlist_picker::open(keys, window, cx);
                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                    })),
            )
            .when(deletable, |bar| {
                bar.child(
                    Button::new("selection-delete")
                        .ghost()
                        .small()
                        .icon(icons::icon(icons::TRASH))
                        .label(t("delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let keys = this.list.read(cx).selected_keys();
                            this.delete(keys, cx);
                        })),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("selection-cancel")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::CLOSE))
                    .label(t("cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                    })),
            )
    }

    fn column_header(&self, covers: bool, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_3()
            .px_2()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(div().w(px(28.)).flex_shrink_0().child(t("title")))
            .when(covers, |row| row.child(div().w(px(40.)).flex_shrink_0()))
            .child(div().flex_1().min_w_0().child(t("artist")))
            .child(div().w(px(200.)).flex_shrink_0().child(t("album")))
            .child(div().w(px(56.)).flex_shrink_0().child(t("duration")))
    }
}

impl Render for Library {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let jobs = JobsModel::global(cx);
        let jobs = jobs.read(cx);
        let syncing = jobs.is_running(JobKind::LibrarySync) || jobs.is_running(JobKind::Scan);

        let total = self.total(cx);
        let track_count = total.unwrap_or(0) as usize;
        let album_count = self
            .albums
            .read(cx)
            .value()
            .map(|page| {
                page.albums
                    .iter()
                    .map(|album| album.title.to_lowercase())
                    .collect::<HashSet<_>>()
                    .len()
            })
            .unwrap_or(0);
        let artist_count = self
            .artists
            .read(cx)
            .value()
            .map(|page| page.artists.len())
            .unwrap_or(0);
        let playlist_count = PlaylistsModel::global(cx).read(cx).playlists().len();

        let selected = self.list.read(cx).selected_keys().len();
        let selecting = self.list.read(cx).is_selecting();
        let all_selected = track_count > 0 && selected >= track_count;
        let refresh_hint = if capabilities.scan_folders {
            t("rescan_library")
        } else {
            t("refresh_music_library")
        };

        v_flex()
            .size_full()
            .p_4()
            .child(
                PageHeader::new(t("your_library")).action(
                    Button::new("library-refresh")
                        .ghost()
                        .icon(icons::icon(icons::REFRESH))
                        .tooltip(refresh_hint)
                        .disabled(!capabilities.scan_folders && !capabilities.sync)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .mb_6()
                    .child(StatCard::new(t("tracks"), track_count, icons::MUSIC))
                    .child(StatCard::new(t("albums"), album_count, icons::DISC))
                    .child(StatCard::new(t("artists"), artist_count, icons::MIC))
                    .child(StatCard::new(t("playlists"), playlist_count, icons::LIST)),
            )
            .child(
                h_flex()
                    .w_full()
                    .mb_3()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                Checkbox::new("library-select-all")
                                    .checked(all_selected)
                                    .disabled(track_count == 0)
                                    .accessibility_label(t("select_all_tracks"))
                                    .on_click(cx.listener(move |this, _: &bool, _, cx| {
                                        this.toggle_select_all(all_selected, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t("tracks")),
                            ),
                    )
                    .child(sort::control(self.sort.clone())),
            )
            .when(selecting, |page| {
                page.child(self.selection_bar(selected, cx))
            })
            .child(self.column_header(covers, cx))
            .child(v_flex().flex_1().min_h_0().map(|body| {
                match (total, track_count) {
                    (None, _) => body.child(Loading),
                    (Some(_), 0) if syncing => body.child(Loading),
                    (Some(_), 0) => body.child(EmptyState::new(t("no_tracks_found"))),
                    _ => body
                        .child(div().flex_1().min_h_0().child(self.list.clone()))
                        .when(syncing, |body| {
                            body.child(
                                h_flex()
                                    .w_full()
                                    .py_2()
                                    .justify_center()
                                    .child(Spinner::new().small()),
                            )
                        }),
                }
            }))
    }
}
