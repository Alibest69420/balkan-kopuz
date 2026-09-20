//! The Library page: the source's whole track listing, its stat cards, the stacked sort control and multi-select.

mod sort;
mod stats;

use std::collections::HashSet;

use api::{AlbumPage, ArtistPage, JobKind, Table, TrackFilter, TrackPage, TrackSort};
use config::{SortCriterion, TrackSortField};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window, div, px,
};
use store::{ConfigModel, JobsModel, PlaylistsModel, Query, SourcesModel, actions};
use widgets::format::t;
use widgets::track_list::{TrackActions, TrackList, TrackListEvent, TrackListOptions};
use widgets::{PageHeader, columns, icons, motion, playlist_picker, style};

use crate::stats::StatCard;

// The header action tracks its own hover, since a Button cannot carry a tween.
const REFRESH: &str = "library-refresh-frame";

/// A Button that paints nothing of its own, so the frame around it can own the tween.
fn clear_variant(ink: Hsla, cx: &App) -> ButtonCustomVariant {
    style::bare_button(ink, cx)
}

pub struct Library {
    tracks: Entity<Query<TrackPage>>,
    // Held so the query keeps running; the count it feeds lives in album_count.
    _albums: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    list: Entity<TrackList>,
    // Distinct album titles, counted when the albums change rather than on every render.
    album_count: usize,
    sort: Vec<SortCriterion<TrackSortField>>,
    // Which chrome control the pointer is over; it drives their tweens.
    control: Option<&'static str>,
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
            cx.observe(&albums, |this, albums, cx| {
                this.album_count = albums
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
                cx.notify();
            }),
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
            _albums: albums,
            album_count: 0,
            artists,
            list,
            sort,
            control: None,
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

    /// A leave for a control that is no longer the hovered one would clear its successor.
    fn track_control(&mut self, id: &'static str, hovered: bool, cx: &mut Context<Self>) {
        if !hovered && self.control != Some(id) {
            return;
        }
        self.control = hovered.then_some(id);
        cx.notify();
    }

    /// The floating pill the Dioxus page drops over the list while rows are picked.
    fn selection_bar(
        &self,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fade = motion::enter("library-selection-bar", motion::MEDIUM, window, cx);
        let deletable = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        let ink = cx.theme().primary_foreground;
        let rule = style::black(0.2, cx);
        let divider = move || div().w(px(1.)).h_5().flex_shrink_0().bg(rule);
        // The bar is an accent slab, so its buttons paint nothing of their own; without the
        // variant a Button's hover layer repaints the glyph in the theme's grey over the accent.
        let bare = clear_variant(ink, cx);
        let entry = |id: &'static str, icon: &'static str, label: SharedString| {
            Button::new(id)
                .icon(icons::icon(icon))
                .label(label)
                .custom(bare)
                .font_weight(FontWeight::MEDIUM)
        };
        div()
            .absolute()
            .bottom_24()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .opacity(fade)
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .px_6()
                    .py_2p5()
                    .rounded(cx.theme().radius_full())
                    .bg(cx.theme().primary)
                    .text_color(ink)
                    .shadow_2xl()
                    .child(
                        // TODO(i18n): the Dioxus selection bar spells this count in English too.
                        div()
                            .pl_2()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .child(SharedString::from(format!("{count} selected"))),
                    )
                    .child(divider())
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .child(
                                entry("selection-queue", icons::QUEUE_ADD, t("add_to_queue"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let keys = this.list.read(cx).selected_keys();
                                        actions::append(keys, cx);
                                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                                    })),
                            )
                            .child(
                                entry("selection-playlist", icons::PLUS, t("add_to_playlist"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let keys = this.list.read(cx).selected_keys();
                                        playlist_picker::open(keys, window, cx);
                                        this.list.update(cx, |list, cx| list.clear_selection(cx));
                                    })),
                            )
                            .when(deletable, |bar| {
                                bar.child(
                                    entry("selection-delete", icons::TRASH, t("delete")).on_click(
                                        cx.listener(|this, _, _, cx| {
                                            let keys = this.list.read(cx).selected_keys();
                                            this.delete(keys, cx);
                                        }),
                                    ),
                                )
                            }),
                    )
                    .child(divider())
                    .child(
                        Button::new("selection-cancel")
                            .icon(icons::icon(icons::CLOSE))
                            .tooltip(t("cancel"))
                            .custom(clear_variant(ink, cx))
                            .pr_2()
                            .text_lg()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.list.update(cx, |list, cx| list.clear_selection(cx));
                            })),
                    ),
            )
    }

    /// The column labels, laid out on the track row's own columns so they sit over them.
    // Built from the same cells as the row, so the labels sit over their columns.
    fn column_header(&self, _: bool, cx: &mut Context<Self>) -> impl IntoElement {
        columns::frame()
            .mb_2()
            .border_b_1()
            .border_color(style::white(style::BORDER, cx))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(style::white(style::TEXT_MUTED, cx))
            .child(columns::fixed(columns::INDEX).child("#"))
            .child(columns::flexible(columns::TITLE_MIN, 2.).child(t("title")))
            .child(columns::flexible(columns::TEXT_MIN, 1.).child(t("artist")))
            .child(columns::flexible(columns::TEXT_MIN, 1.).child(t("album")))
            .child(
                columns::fixed(columns::DURATION)
                    .justify_end()
                    .child(icons::icon(icons::CLOCK)),
            )
            .child(columns::fixed(columns::MENU))
    }
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let jobs = JobsModel::global(cx);
        let jobs = jobs.read(cx);
        let syncing = jobs.is_running(JobKind::LibrarySync) || jobs.is_running(JobKind::Scan);

        let total = self.total(cx);
        let track_count = total.unwrap_or(0) as usize;
        // Counted when the albums change, not here: this is one number on a stat card, and
        // building a lowercase set over 2500 titles for it ran on every render.
        let album_count = self.album_count;
        let artist_count = self
            .artists
            .read(cx)
            .value()
            .map(|page| page.artists.len())
            .unwrap_or(0);
        let playlist_count = PlaylistsModel::global(cx).read(cx).playlists().len();

        let selected = self.list.read(cx).selected_count();
        let selecting = self.list.read(cx).is_selecting();
        let all_selected = track_count > 0 && selected >= track_count;
        let refresh_hint = if capabilities.scan_folders {
            t("rescan_library")
        } else {
            t("refresh_music_library")
        };

        let faint = style::white(style::TEXT_FAINT, cx);
        let spinner = move |size: gpui_kit::Pixels| Spinner::new().color(faint).with_size(size);

        let refreshable = capabilities.scan_folders || capabilities.sync;
        let lit = self.control == Some(REFRESH);
        let refresh_wash = style::white(if lit { style::HOVER } else { 0. }, cx);
        let refresh_target = if lit {
            cx.theme().foreground
        } else {
            style::white(style::TEXT_SECONDARY, cx)
        };
        let refresh_bg = motion::color(motion::id(REFRESH, "bg"), refresh_wash, window, cx);
        let refresh_ink = motion::color(motion::id(REFRESH, "ink"), refresh_target, window, cx);

        v_flex()
            .size_full()
            .relative()
            .px_8()
            .pt_8()
            .child(
                PageHeader::new(t("your_library")).action(
                    div()
                        .id(REFRESH)
                        .size_9()
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(refresh_bg)
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            this.track_control(REFRESH, *hovered, cx)
                        }))
                        .child(
                            Button::new("library-refresh")
                                // A Button paints its own hover, so the frame carries the wash.
                                .custom(clear_variant(refresh_ink, cx))
                                .icon(icons::icon(icons::REFRESH))
                                .rounded(cx.theme().radius_full())
                                .size_full()
                                .tooltip(refresh_hint)
                                .disabled(!refreshable)
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        ),
                ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .mb_12()
                    .child(StatCard::new(t("tracks"), track_count, icons::MUSIC))
                    .child(StatCard::new(t("albums"), album_count, icons::DISC))
                    .child(StatCard::new(t("artists"), artist_count, icons::MIC))
                    .child(StatCard::new(t("playlists"), playlist_count, icons::LIST)),
            )
            .child(
                h_flex()
                    .w_full()
                    .mb_4()
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
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(style::white(0.8, cx))
                                    .child(t("tracks")),
                            ),
                    )
                    .child(sort::control(
                        self.sort.clone(),
                        self.control == Some(sort::CONTROL),
                        window,
                        cx,
                    )),
            )
            .child(self.column_header(covers, cx))
            .child(v_flex().flex_1().min_h_0().map(|body| {
                let pending = h_flex()
                    .w_full()
                    .py_12()
                    .justify_center()
                    .child(spinner(px(30.)));
                match (total, track_count) {
                    (None, _) => body.child(pending),
                    (Some(_), 0) if syncing => body.child(pending),
                    (Some(_), 0) => body.child(
                        div()
                            .italic()
                            .text_color(style::slate_500(cx))
                            .child(t("no_tracks_found")),
                    ),
                    _ => body
                        .child(div().flex_1().min_h_0().child(self.list.clone()))
                        .when(syncing, |body| {
                            body.child(
                                h_flex()
                                    .w_full()
                                    .py_4()
                                    .justify_center()
                                    .child(spinner(px(20.))),
                            )
                        }),
                }
            }))
            .when(selecting, |page| {
                page.child(self.selection_bar(selected, window, cx))
            })
    }
}
