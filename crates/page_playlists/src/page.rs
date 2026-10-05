//! The playlists page: folders and playlists, or the flat remote layout.

use std::collections::HashMap;
use std::rc::Rc;

use api::{AlbumPresentation, JobKind, PlaylistInfo, SourceCapabilities};
use components::format::t;
use components::{
    DownloadToggle, EmptyState, Heading, IconButton, Loading, PageHeader, SectionHeading,
    ToggleSize, icons, style,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, px,
};
use store::{DownloadsModel, JobsModel, Offline, PlaylistsModel, SourcesModel, actions};

use crate::cards::{folder_card, playlist_card};
use crate::dialogs;
use components::{Tile, tile_grid as card_grid};

pub struct Playlists {
    requested_sync: bool,
    // Each playlist's keys and what the downloads say of them, rebuilt on a change, not per card per frame.
    offline: HashMap<String, (Rc<[String]>, Offline)>,
    _subscriptions: Vec<Subscription>,
}

impl Playlists {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe(&PlaylistsModel::global(cx), |this, _, cx| {
                this.rekey_offline(cx);
                cx.notify();
            }),
            cx.observe(&JobsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&DownloadsModel::global(cx), |this, _, cx| {
                this.restate_offline(cx);
                cx.notify();
            }),
            cx.observe(&SourcesModel::global(cx), |this: &mut Self, _, cx| {
                this.request_sync(cx);
                this.rekey_offline(cx);
                cx.notify();
            }),
        ];
        let mut page = Self {
            requested_sync: false,
            offline: HashMap::new(),
            _subscriptions: subscriptions,
        };
        page.request_sync(cx);
        page.rekey_offline(cx);
        page
    }

    /// The playlists or the source changed: take every playlist's keys afresh.
    fn rekey_offline(&mut self, cx: &App) {
        self.offline.clear();
        if !SourcesModel::global(cx).read(cx).capabilities().downloads {
            return;
        }
        let downloads = DownloadsModel::global(cx).read(cx);
        for playlist in PlaylistsModel::global(cx).read(cx).playlists() {
            let offline = downloads.offline(&playlist.track_keys);
            self.offline.insert(
                playlist.id.clone(),
                (playlist.track_keys.clone().into(), offline),
            );
        }
    }

    /// A download moved: the keys stand, only what the downloads say of them is asked again.
    fn restate_offline(&mut self, cx: &App) {
        let downloads = DownloadsModel::global(cx).read(cx);
        for (keys, offline) in self.offline.values_mut() {
            *offline = downloads.offline(keys);
        }
    }

    /// The pull runs in the daemon, single-flight, so this only has to ask once per visit.
    fn request_sync(&mut self, cx: &mut Context<Self>) {
        if self.requested_sync || !SourcesModel::global(cx).read(cx).capabilities().sync {
            return;
        }
        self.requested_sync = true;
        JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(JobKind::PlaylistSync, cx));
    }

    fn header(&self, capabilities: SourceCapabilities, cx: &mut Context<Self>) -> PageHeader {
        let add = cx.listener(|_, _: &ClickEvent, window, cx| {
            dialogs::open_add_playlist(window, cx);
        });
        PageHeader::new(t("playlists"))
            .when(capabilities.folders, |header| {
                header.action(
                    IconButton::new("new-folder", icons::FOLDER)
                        .tooltip(t("new_folder"))
                        .on_click(|_, _, cx| {
                            let name = t("new_folder").to_string();
                            actions::run(cx, move |api| async move {
                                api.create_playlist_folder(name).await
                            });
                        }),
                )
            })
            .action(
                IconButton::new("add-playlist", icons::PLUS)
                    .tooltip(t("add_playlist"))
                    .on_click(add),
            )
    }

    fn folders_layout(&self, cx: &mut Context<Self>) -> AnyElement {
        let model = PlaylistsModel::global(cx);
        let folders = model.read(cx).folders().to_vec();
        let playlists = model.read(cx).playlists().to_vec();
        let root: Vec<PlaylistInfo> = playlists
            .iter()
            .filter(|playlist| {
                !folders
                    .iter()
                    .any(|folder| folder.playlist_ids.contains(&playlist.id))
            })
            .cloned()
            .collect();

        if folders.is_empty() && root.is_empty() {
            return EmptyState::new(t("no_playlists_yet"))
                .icon(icons::FOLDER)
                .into_any_element();
        }
        v_flex()
            .gap_8()
            .when(!folders.is_empty(), |column| {
                column.child(
                    card_grid()
                        .children(folders.iter().map(|folder| folder_card(folder, &playlists))),
                )
            })
            .when(!root.is_empty(), |column| {
                column.child(
                    v_flex()
                        .gap_4()
                        .when(!folders.is_empty(), |section| {
                            section
                                .child(SectionHeading::new(t("playlists")).heading(Heading::Group))
                        })
                        .child(card_grid().children(root.iter().map(playlist_card))),
                )
            })
            .into_any_element()
    }

    fn flat_layout(&self, capabilities: SourceCapabilities, cx: &mut Context<Self>) -> AnyElement {
        let playlists = PlaylistsModel::global(cx).read(cx).playlists().to_vec();
        let anonymous = SourcesModel::global(cx)
            .read(cx)
            .active()
            .is_some_and(|source| source.anonymous);
        let remote = capabilities.albums == AlbumPresentation::Remote;

        v_flex()
            .gap_3()
            .when(remote, |column| {
                column.child(self.sync_row(playlists.len(), cx))
            })
            .child(if playlists.is_empty() {
                if anonymous {
                    EmptyState::new(t("source_anon_playlists"))
                        .icon(icons::LIST)
                        .into_any_element()
                } else {
                    EmptyState::new(t("no_playlists_found"))
                        .icon(icons::FOLDER)
                        .into_any_element()
                }
            } else {
                let mut cards = Vec::with_capacity(playlists.len());
                for playlist in &playlists {
                    cards.push(self.remote_card(playlist, capabilities));
                }
                card_grid().children(cards).into_any_element()
            })
            .into_any_element()
    }

    fn remote_card(&self, playlist: &PlaylistInfo, capabilities: SourceCapabilities) -> Tile {
        let corner = capabilities
            .downloads
            .then(|| self.offline.get(&playlist.id))
            .flatten()
            .map(|(keys, offline)| {
                DownloadToggle::new(format!("download-{}", playlist.id), keys.clone())
                    .size(ToggleSize::Corner)
                    .offline(*offline)
            });
        playlist_card(playlist)
            .subtitle(format!(
                "{} • {}",
                t("server_playlist"),
                components::format::t_with(
                    "playlist_track_count",
                    &[("count", playlist.track_keys.len().to_string())],
                )
            ))
            .corner(corner)
    }

    fn sync_row(&self, total: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = JobsModel::global(cx)
            .read(cx)
            .progress(JobKind::PlaylistSync)
            .cloned();
        let syncing = progress.is_some();
        let done = progress.and_then(|job| job.current).unwrap_or(0) as usize;
        let remaining = total.saturating_sub(done);
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_3()
            .px_2()
            .text_xs()
            .text_color(style::slate_400(cx))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .when(syncing, |row| {
                        row.child(Spinner::new().xsmall().color(cx.theme().primary_hover))
                            .child(
                                // TODO(i18n): the Dioxus status line is English-only too.
                                SharedString::from(format!(
                                    "Loading tracks — {done} / {total} playlists ({remaining} left)"
                                )),
                            )
                    })
                    .when(!syncing && total > 0, |row| {
                        row.child(icons::icon(icons::CHECK).text_color(cx.theme().success))
                            // TODO(i18n): the Dioxus status line is English-only too.
                            .child(SharedString::from(format!("{total} playlists synced")))
                    }),
            )
            .child(
                Button::new("refresh-playlists")
                    .custom(style::soft_button(style::white(0.80, cx), cx))
                    .xsmall()
                    .px_3()
                    .py_1()
                    // `Button::rounded` is in pixels, so the Rems helper cannot travel here.
                    .rounded(px(8.))
                    .icon(icons::icon(icons::REFRESH))
                    .label(t("refresh"))
                    .disabled(syncing)
                    .on_click(|_, _, cx| {
                        JobsModel::global(cx)
                            .update(cx, |jobs, cx| jobs.start(JobKind::PlaylistSync, cx));
                    }),
            )
    }
}

impl Render for Playlists {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let loaded = PlaylistsModel::global(cx).read(cx).is_loaded();
        let header = self.header(capabilities, cx);
        let body = if !loaded {
            Loading::new().into_any_element()
        } else if capabilities.folders {
            self.folders_layout(cx)
        } else {
            self.flat_layout(capabilities, cx)
        };
        components::page().child(header).child(
            v_flex()
                .id("playlists-body")
                .flex_1()
                .min_h_0()
                .pb_20()
                .overflow_y_scroll()
                .child(body),
        )
    }
}
