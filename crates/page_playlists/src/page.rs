//! The playlists page: folders and playlists, or the flat remote layout.

use api::{AlbumPresentation, JobKind, PlaylistInfo, SourceCapabilities};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    px,
};
use store::{DownloadsModel, JobsModel, PlaylistsModel, SourcesModel, actions};
use widgets::format::t;
use widgets::{EmptyState, Loading, PageHeader, icons, style};

use crate::cards::{Card, card_grid, folder_card, playlist_card};
use crate::dialogs;

pub struct Playlists {
    requested_sync: bool,
    _subscriptions: Vec<Subscription>,
}

/// `w-10 h-10 rounded-full text-white/60`: the two buttons beside the page title.
fn round_action(id: &'static str, cx: &App) -> Button {
    Button::new(id)
        .ghost()
        .size_10()
        .rounded_full()
        .text_color(style::white(style::TEXT_SECONDARY, cx))
}

impl Playlists {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe(&PlaylistsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&JobsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&DownloadsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&SourcesModel::global(cx), |this: &mut Self, _, cx| {
                this.request_sync(cx);
                cx.notify();
            }),
        ];
        let mut page = Self {
            requested_sync: false,
            _subscriptions: subscriptions,
        };
        page.request_sync(cx);
        page
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
        PageHeader::new(t("playlists"))
            .when(capabilities.folders, |header| {
                header.action(
                    round_action("new-folder", cx)
                        .icon(icons::icon(icons::FOLDER))
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
                round_action("add-playlist", cx)
                    .icon(icons::icon(icons::PLUS))
                    .tooltip(t("add_playlist"))
                    .on_click(cx.listener(|_, _, window, cx| {
                        dialogs::open_add_playlist(window, cx);
                    })),
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
                            section.child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(style::white(0.40, cx))
                                    .child(t("playlists")),
                            )
                        })
                        .child(
                            card_grid().children(
                                root.iter().map(|playlist| playlist_card(playlist, None)),
                            ),
                        ),
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
                    cards.push(self.remote_card(playlist, capabilities, cx));
                }
                card_grid().children(cards).into_any_element()
            })
            .into_any_element()
    }

    fn remote_card(
        &self,
        playlist: &PlaylistInfo,
        capabilities: SourceCapabilities,
        cx: &mut Context<Self>,
    ) -> Card {
        let downloads = DownloadsModel::global(cx);
        let keys = playlist.track_keys.clone();
        let active = keys.iter().any(|key| downloads.read(cx).is_active(key));
        let stored = !keys.is_empty() && keys.iter().all(|key| downloads.read(cx).is_stored(key));
        let corner = capabilities.downloads.then(|| {
            Button::new(SharedString::from(format!("download-{}", playlist.id)))
                .ghost()
                .xsmall()
                .size_8()
                .rounded_full()
                .bg(style::black(0.4, cx))
                .border_1()
                .border_color(style::white(style::BORDER, cx))
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .icon(icons::icon(if stored {
                    icons::TRASH
                } else {
                    icons::DOWNLOAD
                }))
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
                })
                .into_any_element()
        });
        crate::cards::playlist_card(playlist, None)
            .subtitle(format!(
                "{} • {}",
                t("server_playlist"),
                widgets::format::t_with(
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
                        row.child(
                            Spinner::new()
                                .xsmall()
                                .text_color(cx.theme().primary_hover),
                        )
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
                    .ghost()
                    .xsmall()
                    .px_3()
                    .py_1()
                    // `Button::rounded` is in pixels, so the Rems helper cannot travel here.
                    .rounded(px(8.))
                    .bg(style::white(style::HOVER, cx))
                    .text_color(style::white(0.80, cx))
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
        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .child(self.header(capabilities, cx))
            .child(
                v_flex()
                    .id("playlists-body")
                    .flex_1()
                    .min_h_0()
                    .pb_20()
                    .overflow_y_scroll()
                    .child(if !loaded {
                        Loading.into_any_element()
                    } else if capabilities.folders {
                        self.folders_layout(cx)
                    } else {
                        self.flat_layout(capabilities, cx)
                    }),
            )
    }
}
