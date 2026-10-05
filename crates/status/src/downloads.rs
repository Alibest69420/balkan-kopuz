//! The download queue, and the history of everything fetched from a URL.

use std::ops::Range;

use api::schema::ChoiceOption;
use api::{DownloadHistoryEntry, DownloadState, JobKind};
use components::format::{t, t_with, text};
use components::{focus, icons, style};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use store::{DownloadItem, DownloadsModel, ItemState, JobEvent, JobsModel, Link, Notices, Query};

use crate::{DONE, FAILED, ROW_GLYPH, chip, empty, header, quiet_chip, row};

/// The same one-line list row the playlists panel draws, so a skin moves both together.
fn row_height() -> Pixels {
    style::list_row()
}
const BAR: Pixels = px(3.);
/// `LIST_HEIGHT` as a number, so a short queue can size its list to itself.
const LIST: f32 = 260.;
/// The history shares the popover with the queue, so it gets the shorter half.
const HISTORY: f32 = 170.;

/// The popover asking to stand down, because what it opened wants the screen.
pub(crate) struct Dismiss;

/// The downloads popover's body.
pub struct DownloadsList {
    scroll: UniformListScrollHandle,
    past_scroll: UniformListScrollHandle,
    history: Entity<Query<Vec<DownloadHistoryEntry>>>,
    formats: Entity<Query<Vec<ChoiceOption>>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Dismiss> for DownloadsList {}

impl DownloadsList {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let downloads = DownloadsModel::global(cx);
        let jobs = JobsModel::global(cx);
        let history =
            cx.new(|cx| Query::new(&[], |api| async move { api.downloader_history().await }, cx));
        let formats =
            cx.new(|cx| Query::new(&[], |api| async move { api.download_formats().await }, cx));
        Self {
            scroll: UniformListScrollHandle::new(),
            past_scroll: UniformListScrollHandle::new(),
            history: history.clone(),
            formats,
            _subscriptions: vec![
                cx.observe(&downloads, |_, _, cx| cx.notify()),
                cx.observe(&jobs, |_, _, cx| cx.notify()),
                cx.observe(&history, |_, _, cx| cx.notify()),
                // The history gains a row the moment a URL download stops running.
                cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                    if event.kind == JobKind::UrlDownload {
                        this.history.update(cx, |history, cx| history.refresh(cx));
                    }
                }),
            ],
        }
    }

    /// The popover was opened: what the daemon fetched while it was shut is worth re-reading.
    pub(crate) fn opened(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |history, cx| history.refresh(cx));
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.clear_downloader_history().await });
        cx.spawn(async move |this, cx| match call.await {
            Ok(()) => {
                this.update(cx, |this, cx| {
                    this.history.update(cx, |history, cx| history.refresh(cx));
                })
                .ok();
            }
            Err(error) => {
                tracing::warn!(%error, "clearing the download history failed");
                this.update(cx, |_, cx| Notices::error(store::human(&error), cx))
                    .ok();
            }
        })
        .detach();
    }

    /// The chip that opens the URL box; a tab stop, because a popover is reachable without a mouse.
    fn add_url(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let handle = focus::handle("downloads-add-url", window, cx);
        let focused = handle.is_focused(window);
        let ink = cx.theme().primary;
        let this = cx.entity();
        let asked = this.clone();
        focus::stop(div().id("downloads-add-url"), &handle, move |window, cx| {
            this.update(cx, |_, cx| cx.emit(Dismiss));
            components::dialogs::open_url_download(window, cx);
        })
        .relative()
        .px_2()
        .py_0p5()
        .flex_shrink_0()
        .rounded(style::radius(cx))
        .cursor_pointer()
        .text_xs()
        .bg(ink.opacity(style::HOVER))
        .text_color(ink)
        .hover(|chip| chip.bg(ink.opacity(style::ACTIVE)))
        .child(t("downloader_add_url"))
        .on_click(move |_, window, cx| {
            asked.update(cx, |_, cx| cx.emit(Dismiss));
            components::dialogs::open_url_download(window, cx);
        })
        .when(focused, |chip| {
            chip.child(focus::ring(style::radius(cx), cx))
        })
        .into_any_element()
    }

    fn past_row(
        &self,
        entry: &DownloadHistoryEntry,
        formats: &[ChoiceOption],
        cx: &Context<Self>,
    ) -> AnyElement {
        let failed = entry.state == DownloadState::Failed;
        let status: SharedString = match (failed, entry.error.clone()) {
            (true, Some(error)) => error.into(),
            (true, None) => t("downloader_status_failed"),
            (false, _) => t("downloader_status_completed"),
        };
        let format = formats
            .iter()
            .find(|option| option.value == entry.format)
            .map(|option| text(&option.label))
            .unwrap_or_default();
        let title: SharedString = match entry.title == entry.url {
            true => shorten(&entry.url),
            false => entry.title.clone().into(),
        };
        let (glyph, ink) = match failed {
            true => (FAILED, cx.theme().danger),
            false => (DONE, cx.theme().success),
        };
        let status_ink = match failed {
            true => cx.theme().danger,
            false => style::slate_500(cx),
        };

        row()
            .h(row_height())
            .child(
                icons::icon(glyph)
                    .size(ROW_GLYPH)
                    .flex_shrink_0()
                    .text_color(ink),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(style::text_10())
                                    .text_color(style::slate_500(cx))
                                    .child(format),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(style::text_10())
                            .text_color(status_ink)
                            .child(status),
                    ),
            )
            .into_any_element()
    }

    fn row(&self, item: &DownloadItem, cx: &mut Context<Self>) -> AnyElement {
        let (glyph, ink) = match item.state {
            ItemState::Downloading => (icons::DOWNLOAD, cx.theme().primary_hover),
            ItemState::Queued => (icons::CLOCK, style::slate_500(cx)),
            ItemState::Failed => (FAILED, cx.theme().danger),
            ItemState::Done => (DONE, cx.theme().success),
        };
        let title_ink = match item.state {
            ItemState::Failed => cx.theme().danger,
            ItemState::Queued => style::slate_500(cx),
            _ => cx.theme().foreground,
        };
        row()
            .h(row_height())
            .child(
                icons::icon(glyph)
                    .size(ROW_GLYPH)
                    .flex_shrink_0()
                    .text_color(ink),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(title_ink)
                            .child(item.title.clone()),
                    )
                    .when(!item.artist.is_empty(), |cell| {
                        cell.child(
                            div()
                                .truncate()
                                .text_size(style::text_10())
                                .text_color(style::slate_500(cx))
                                .child(item.artist.clone()),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl Render for DownloadsList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let downloads = DownloadsModel::global(cx);
        let (items, batch) = {
            let downloads = downloads.read(cx);
            (downloads.queue().to_vec(), downloads.batch())
        };
        let running = JobsModel::global(cx)
            .read(cx)
            .progress(JobKind::Download)
            .cloned();
        let failed = items
            .iter()
            .filter(|item| item.state == ItemState::Failed)
            .count();
        let count = items.len();
        let stopping = running
            .as_ref()
            .is_some_and(|job| JobsModel::global(cx).read(cx).is_stopping(&job.id));
        let job_id = running.as_ref().map(|job| job.id.clone());
        let percent = running.as_ref().and_then(|job| {
            let total = job.total.unwrap_or(0);
            (total > 0).then(|| job.current.unwrap_or(0) as f32 / total as f32 * 100.)
        });
        let danger = cx.theme().danger;

        let action = match (running.is_some(), stopping, count > 0) {
            (true, true, _) => Some(quiet_chip(t("stopping"), cx)),
            (true, false, _) => Some(
                chip("stop-downloads", t("stop"), danger, cx)
                    .on_click(move |_, _, cx| {
                        let Some(id) = job_id.clone() else { return };
                        JobsModel::global(cx).update(cx, |jobs, cx| jobs.cancel(id, cx));
                    })
                    .into_any_element(),
            ),
            (false, _, true) => Some(
                chip("clear-downloads", t("clear"), style::slate_400(cx), cx)
                    .on_click(|_, _, cx| {
                        DownloadsModel::global(cx)
                            .update(cx, |downloads, cx| downloads.clear_queue(cx));
                    })
                    .into_any_element(),
            ),
            _ => None,
        };
        let past = self.history.read(cx).value().cloned().unwrap_or_default();
        let formats = self.formats.read(cx).value().cloned().unwrap_or_default();
        let fetched = past.len();
        let heading = h_flex()
            .gap_1p5()
            .items_center()
            .children(action)
            .child(self.add_url(window, cx))
            .into_any_element();

        v_flex()
            .w_full()
            .child(header(t("downloads"), Some(heading), cx))
            .when_some(running.as_ref(), |body, _| {
                body.child(
                    Progress::new("downloads-progress")
                        .xsmall()
                        .h(BAR)
                        .map(|bar| match percent {
                            Some(percent) => bar.value(percent),
                            None => bar.loading(true),
                        })
                        .color(cx.theme().primary),
                )
            })
            .when_some(batch, |body, batch| {
                body.child(
                    div()
                        .w_full()
                        .px_3()
                        .py_1p5()
                        .text_size(style::text_10())
                        .text_color(style::white(style::TEXT_SECONDARY, cx))
                        .child(t_with(
                            "downloads_summary",
                            &[
                                ("done", batch.downloaded.to_string()),
                                ("failed", batch.failed.to_string()),
                            ],
                        )),
                )
            })
            .when(count == 0, |body| {
                body.child(empty(t("downloads_idle"), cx))
            })
            .when(count > 0, |body| {
                body.child(
                    uniform_list(
                        "download-rows",
                        count,
                        cx.processor(move |this, range: Range<usize>, _, cx| {
                            range
                                .filter_map(|index| items.get(index).map(|item| this.row(item, cx)))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .h(px(LIST.min(row_height().to_f64() as f32 * count as f32))),
                )
            })
            .when(failed > 0 && running.is_none(), |body| {
                body.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(style::white(style::BORDER_SOFT, cx))
                        .child(
                            chip("retry-downloads", t("retry_failed"), cx.theme().primary, cx)
                                .on_click(|_, _, cx| {
                                    DownloadsModel::global(cx)
                                        .update(cx, |downloads, cx| downloads.retry_failed(cx));
                                }),
                        ),
                )
            })
            .when(fetched > 0, |body| {
                body.child(
                    div()
                        .w_full()
                        .border_t_1()
                        .border_color(style::white(style::BORDER_SOFT, cx))
                        .child(header(
                            t("downloader_history"),
                            Some(
                                chip("clear-url-history", t("clear"), style::slate_400(cx), cx)
                                    .on_click(cx.listener(|this, _, _, cx| this.clear_history(cx)))
                                    .into_any_element(),
                            ),
                            cx,
                        )),
                )
                .child(
                    uniform_list(
                        "url-download-rows",
                        fetched,
                        cx.processor(move |this, range: Range<usize>, _, cx| {
                            range
                                .filter_map(|index| {
                                    past.get(index)
                                        .map(|entry| this.past_row(entry, &formats, cx))
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.past_scroll)
                    .h(px(
                        HISTORY.min(row_height().to_f64() as f32 * fetched as f32)
                    )),
                )
            })
    }
}

/// A URL is what a row shows until the downloader names the file it wrote.
fn shorten(url: &str) -> SharedString {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .take(48)
        .collect::<String>()
        .into()
}
