//! One album: the standard detail, and the catalog-styled one a remote source presents.

use api::{
    AlbumInfo, AlbumPage, AlbumPresentation, ArtworkRef, CatalogDetail, CatalogDetailRequest,
    CatalogItemKind, PlaylistCapability, SourceCapabilities, Table, TrackInfo,
};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, px,
};
use store::{DownloadsModel, Navigator, PlayerModel, Query, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{
    Cover, EmptyState, Loading, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons,
    motion, style,
};

use crate::clear_variant;
use crate::ops;

// TODO(offline): filtering the listing to downloaded tracks waits on an offline seam the checklist flags as wrong.

// One virtualized row, so a disc's list can be sized to its contents inside the page's own scroll.
const ROW_HEIGHT: Pixels = px(60.);
// `w-64 h-64` on the showcase, `max-w-[300px]` on the catalog-styled page.
const HERO_COVER: Pixels = px(256.);
const REMOTE_COVER: Pixels = px(300.);
// `w-14`/`w-16`/`w-12`/`w-11`, with the glyph sizes the Dioxus buttons set on them.
const HERO_ACTION: Pixels = px(56.);
const HERO_SECONDARY: Pixels = px(48.);
const REMOTE_ACTION: Pixels = px(64.);
const REMOTE_SECONDARY: Pixels = px(44.);
// The cover's upload scrim tracks hover alongside the action buttons.
const COVER: &str = "album-cover";

/// What a round hero action looks like: its ring alphas, its resting ink, and whether it washes on hover.
#[derive(Clone, Copy)]
struct Chrome {
    ring: Option<(f32, f32)>,
    ink: Hsla,
    wash: bool,
    // A control already in its "on" state paints the hovered look and stays there.
    lit: bool,
}

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
    // Which round action or the cover the pointer is over; it drives their tweens.
    hovered: Option<&'static str>,
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
            hovered: None,
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

    /// The circular chrome both heroes hang their actions on.
    fn round_button(
        id: &'static str,
        icon: &'static str,
        box_size: Pixels,
        glyph: Pixels,
        cx: &Context<Self>,
    ) -> Button {
        Button::new(id)
            .ghost()
            .icon(icons::icon(icon))
            .with_size(glyph / 0.75)
            .rounded(cx.theme().radius_full())
            .w(box_size)
            .h(box_size)
            .flex_shrink_0()
    }

    /// `transition-colors` on a round action: the frame owns the ring and wash, the button its glyph.
    fn ringed(
        &self,
        id: &'static str,
        button: Button,
        box_size: Pixels,
        chrome: Chrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = chrome.lit || self.hovered == Some(id);
        let ring = chrome.ring.map(|(idle, hovered)| {
            let alpha = if active { hovered } else { idle };
            motion::color(motion::id(id, "ring"), style::white(alpha, cx), window, cx)
        });
        let wash = style::white(
            if active && chrome.wash {
                style::HOVER
            } else {
                0.
            },
            cx,
        );
        let background = motion::color(motion::id(id, "bg"), wash, window, cx);
        let target = if chrome.lit || !active {
            chrome.ink
        } else {
            cx.theme().foreground
        };
        let ink = motion::color(motion::id(id, "ink"), target, window, cx);
        div()
            .id(id)
            .size(box_size)
            .flex_shrink_0()
            .rounded_full()
            .bg(background)
            .when_some(ring, |frame, colour| frame.border_1().border_color(colour))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.track_hover(id, *hovered, cx)),
            )
            .child(button.size_full().custom(clear_variant(ink, cx)))
    }

    /// A leave for a control that is no longer the hovered one would clear its successor.
    fn track_hover(&mut self, id: &'static str, hovered: bool, cx: &mut Context<Self>) {
        if !hovered && self.hovered != Some(id) {
            return;
        }
        self.hovered = hovered.then_some(id);
        cx.notify();
    }

    /// `bg-indigo-500 hover:bg-indigo-400 text-black`, the one filled action on the page.
    fn play_button(&self, box_size: Pixels, cx: &Context<Self>) -> impl IntoElement {
        let keys = self.keys();
        let accent = cx.theme().primary_hover;
        Self::round_button("album-play", icons::PLAY, box_size, box_size * 0.36, cx)
            // A Button paints its own hover, so the fill comes from a variant.
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(cx.theme().primary)
                    .hover(accent)
                    .active(accent)
                    .foreground(cx.theme().primary_foreground),
            )
            .tooltip(t("play"))
            .disabled(keys.is_empty())
            .on_click(move |_, _, cx| actions::play_all(keys.clone(), cx))
    }

    /// The shuffle toggle: accent-coloured while on, muted while off.
    fn shuffle_button(
        &self,
        box_size: Pixels,
        outlined: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shuffled = PlayerModel::global(cx).read(cx).shuffle();
        let button = Self::round_button(
            "album-shuffle",
            icons::SHUFFLE,
            box_size,
            box_size * 0.36,
            cx,
        )
        .tooltip(t("shuffle"))
        .on_click(move |_, _, cx| {
            PlayerModel::global(cx).update(cx, |player, cx| player.set_shuffle(!shuffled, cx))
        });
        let chrome = Chrome {
            ring: outlined.then_some((0.15, 0.3)),
            ink: if shuffled {
                cx.theme().primary
            } else {
                style::slate_400(cx)
            },
            wash: true,
            lit: shuffled,
        };
        self.ringed("album-shuffle-frame", button, box_size, chrome, window, cx)
    }

    /// Download the whole album, or give its offline copies back; disabled while a download runs.
    fn download_all_button(
        &self,
        box_size: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
        let button = Self::round_button(
            "album-download-all",
            if stored {
                icons::TRASH
            } else {
                icons::DOWNLOAD
            },
            box_size,
            box_size * 0.33,
            cx,
        )
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
        });
        let chrome = Chrome {
            ring: Some((0.2, 0.4)),
            ink: style::white(0.7, cx),
            wash: true,
            lit: false,
        };
        Some(
            self.ringed(
                "album-download-all-frame",
                button,
                box_size,
                chrome,
                window,
                cx,
            )
            .into_any_element(),
        )
    }

    fn cover(
        &self,
        album: &AlbumInfo,
        size: Pixels,
        radius: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let editable = self.capabilities.edit_tags;
        let id = album.id.clone();
        let remove_id = album.id.clone();
        let scrim = style::black(0.5, cx);
        let veil = motion::amount(
            motion::id(COVER, "scrim"),
            if self.hovered == Some(COVER) { 1. } else { 0. },
            motion::FAST,
            window,
            cx,
        );
        div()
            .id(COVER)
            .relative()
            .flex_shrink_0()
            .on_hover(
                cx.listener(|this, hovered: &bool, _, cx| this.track_hover(COVER, *hovered, cx)),
            )
            .child(
                Cover::new(album.artwork.as_ref())
                    .size(size)
                    .hq(true)
                    .rounded(radius)
                    .placeholder(icons::DISC),
            )
            .when(editable, |block| {
                block.child(
                    div()
                        .id("album-cover-pick")
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(radius)
                        .bg(scrim)
                        .opacity(veil)
                        .cursor_pointer()
                        .on_click(move |_, _, cx| ops::pick_album_cover(id.clone(), cx))
                        .child(icons::icon(icons::EDIT).size(px(30.))),
                )
            })
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

    /// `text-sm font-bold text-white/60 hover:text-white hover:underline transition-colors`.
    fn artist_link(
        &self,
        artist: &str,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name = SharedString::from(artist.to_string());
        let route = Route::Artist {
            name: name.clone(),
            channel: None,
        };
        let target = if self.hovered == Some(id) {
            cx.theme().foreground
        } else {
            style::white(style::TEXT_SECONDARY, cx)
        };
        let ink = motion::color(motion::id(id, "ink"), target, window, cx);
        div()
            .id(id)
            .truncate()
            .max_w_full()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
            .text_color(ink)
            // The rule has nothing to tween, so it stays a plain hover refinement.
            .hover(|link| link.underline())
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.track_hover(id, *hovered, cx)),
            )
            .cursor_pointer()
            .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
            .child(name)
    }

    fn tracks_body(&self, cx: &Context<Self>) -> AnyElement {
        if self.rows.is_empty() {
            return v_flex()
                .w_full()
                .py_12()
                .items_center()
                .justify_center()
                .text_color(style::slate_500(cx))
                .child(icons::icon(icons::MUSIC).size(px(36.)).into_any_element())
                .child(div().mt_4().text_lg().child(t("no_tracks_found")))
                .into_any_element();
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
                            h_flex()
                                .id(("album-disc", disc as usize))
                                .w_full()
                                .gap_3()
                                .items_center()
                                .p_2()
                                .rounded(style::radius_lg())
                                .hover(move |heading| heading.bg(hover))
                                .text_color(cx.theme().muted_foreground)
                                .child(icons::icon(icons::DISC))
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

    fn selection_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_selecting(cx) {
            return None;
        }
        let fade = motion::enter("album-selection-bar", motion::MEDIUM, window, cx);
        let keys = self.selected_keys(cx);
        let playlists = self.capabilities.playlists != PlaylistCapability::None;
        let deletable = self.capabilities.delete_from_disk;
        let ink = cx.theme().primary_foreground;
        let rule = style::black(0.2, cx);
        let divider = move || div().w(px(1.)).h_5().flex_shrink_0().bg(rule);
        let entry = move |id: &'static str, icon: &'static str, label: SharedString| {
            Button::new(id)
                .text()
                .icon(icons::icon(icon))
                .label(label)
                .text_color(ink)
                .font_weight(gpui_kit::FontWeight::MEDIUM)
        };
        Some(
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
                            div()
                                .pl_2()
                                .text_lg()
                                .font_weight(gpui_kit::FontWeight::BOLD)
                                .child(t_with(
                                    "showcase_song_count",
                                    &[("count", keys.len().to_string())],
                                )),
                        )
                        .child(divider())
                        .child(
                            h_flex()
                                .gap_4()
                                .items_center()
                                .child(
                                    entry(
                                        "album-selection-queue",
                                        icons::QUEUE_ADD,
                                        t("add_to_queue"),
                                    )
                                    .on_click({
                                        let keys = keys.clone();
                                        move |_, _, cx| actions::append(keys.clone(), cx)
                                    }),
                                )
                                .when(playlists, |bar| {
                                    bar.child(
                                        entry(
                                            "album-selection-playlist",
                                            icons::PLUS,
                                            t("add_to_playlist"),
                                        )
                                        .on_click({
                                            let keys = keys.clone();
                                            move |_, window, cx| {
                                                widgets::playlist_picker::open(
                                                    keys.clone(),
                                                    window,
                                                    cx,
                                                )
                                            }
                                        }),
                                    )
                                })
                                .when(deletable, |bar| {
                                    bar.child(
                                        entry("album-selection-delete", icons::TRASH, t("delete"))
                                            .on_click({
                                                let keys = keys.clone();
                                                move |_, _, cx| ops::delete_tracks(keys.clone(), cx)
                                            }),
                                    )
                                }),
                        )
                        .child(divider())
                        .child(
                            Button::new("album-selection-cancel")
                                .text()
                                .icon(icons::icon(icons::CLOSE))
                                .tooltip(t("cancel"))
                                .text_color(ink)
                                .pr_2()
                                .text_lg()
                                .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    fn standard_detail(
        &self,
        album: AlbumInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = t_with(
            "showcase_song_count",
            &[("count", self.rows.len().to_string())],
        );
        let dot = || div().child("•");
        v_flex()
            .size_full()
            .relative()
            .p_8()
            .overflow_hidden()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .mb_8()
                    .items_center()
                    .child(self.back_button()),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_8()
                    .items_end()
                    .mb_12()
                    .child({
                        let radius = cx.theme().radius * 3.;
                        self.cover(&album, HERO_COVER, radius, window, cx)
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().mb_2().child(self.artist_link(
                                &album.artist,
                                "album-artist",
                                window,
                                cx,
                            )))
                            .child(
                                div()
                                    .mb_6()
                                    .line_clamp(3)
                                    .text_size(gpui_kit::rems(4.5))
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child(SharedString::from(album.title.clone())),
                            )
                            .child(
                                h_flex()
                                    .gap_6()
                                    .items_center()
                                    .text_color(style::slate_400(cx))
                                    .child(div().child(count))
                                    .child(dot())
                                    .child(div().child(format!(
                                        "{} {}",
                                        self.total_ms() / 60_000,
                                        t("min")
                                    )))
                                    .when(album.year > 0, |line| {
                                        line.child(dot()).child(div().child(album.year.to_string()))
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_4()
                            .items_center()
                            .child(self.shuffle_button(HERO_ACTION, false, window, cx))
                            .child(self.play_button(HERO_ACTION, cx))
                            .child({
                                let button = Self::round_button(
                                    "album-select-all",
                                    icons::CHECK,
                                    HERO_SECONDARY,
                                    HERO_SECONDARY * 0.33,
                                    cx,
                                )
                                .tooltip(t("select_all"))
                                .on_click(cx.listener(|this, _, _, cx| this.select_all(cx)));
                                let chrome = Chrome {
                                    ring: Some((0.2, 0.4)),
                                    ink: style::white(0.7, cx),
                                    wash: true,
                                    lit: false,
                                };
                                self.ringed(
                                    "album-select-all-frame",
                                    button,
                                    HERO_SECONDARY,
                                    chrome,
                                    window,
                                    cx,
                                )
                            })
                            .children(self.download_all_button(HERO_SECONDARY, window, cx)),
                    ),
            )
            .child(self.tracks_body(cx))
            .children(self.selection_bar(window, cx))
            .into_any_element()
    }

    fn remote_detail(
        &self,
        view: RemoteView,
        window: &mut Window,
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
        let artist_route = Route::Artist {
            name: view.artist.clone(),
            channel: None,
        };
        let browse_id = view.browse_id.clone();
        let first_key = self.rows.first().map(|row| row.key.clone());

        let outlined = Chrome {
            ring: Some((0.15, 0.3)),
            ink: style::slate_400(cx),
            wash: false,
            lit: false,
        };

        v_flex()
            .size_full()
            .relative()
            .p_8()
            .overflow_hidden()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .mb_8()
                    .items_center()
                    .child(self.back_button()),
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
                                self.cover(&cover, REMOTE_COVER, radius, window, cx)
                            }))
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap_2()
                                    .child(self.artist_link(
                                        &view.artist,
                                        "album-remote-artist",
                                        window,
                                        cx,
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
                                    .children(self.download_all_button(
                                        REMOTE_SECONDARY,
                                        window,
                                        cx,
                                    ))
                                    .child({
                                        let button = Self::round_button(
                                            "album-remote-artist-button",
                                            icons::MIC,
                                            REMOTE_SECONDARY,
                                            REMOTE_SECONDARY * 0.36,
                                            cx,
                                        )
                                        // TODO(i18n): en.ftl has no go-to-artist key.
                                        .tooltip("Go to artist")
                                        .on_click(move |_, _, cx| {
                                            Navigator::navigate(artist_route.clone(), cx)
                                        });
                                        self.ringed(
                                            "album-remote-artist-frame",
                                            button,
                                            REMOTE_SECONDARY,
                                            outlined,
                                            window,
                                            cx,
                                        )
                                    })
                                    .child(self.play_button(REMOTE_ACTION, cx))
                                    .child(self.shuffle_button(REMOTE_SECONDARY, true, window, cx))
                                    .child({
                                        let button = Self::round_button(
                                            "album-remote-share",
                                            icons::SHARE,
                                            REMOTE_SECONDARY,
                                            REMOTE_SECONDARY * 0.36,
                                            cx,
                                        )
                                        // TODO(i18n): en.ftl has no generic share key.
                                        .tooltip("Share")
                                        .on_click(move |_, _, cx| {
                                            ops::share_album(
                                                browse_id.clone(),
                                                first_key.clone(),
                                                cx,
                                            )
                                        });
                                        self.ringed(
                                            "album-remote-share-frame",
                                            button,
                                            REMOTE_SECONDARY,
                                            outlined,
                                            window,
                                            cx,
                                        )
                                    }),
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
