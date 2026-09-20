//! The download queue: what is waiting, what is going, what failed, and how to try again.

use std::ops::Range;

use api::JobKind;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled, Subscription, UniformListScrollHandle, Window, div,
    px, uniform_list,
};
use store::{DownloadItem, DownloadsModel, ItemState, JobsModel};
use widgets::format::{t, t_with};
use widgets::{icons, style};

use crate::{DONE, FAILED, ROW_GLYPH, chip, empty, header, quiet_chip, row};

const ROW: f32 = 34.;
const ROW_HEIGHT: Pixels = px(ROW);
const BAR: Pixels = px(3.);
/// `LIST_HEIGHT` as a number, so a short queue can size its list to itself.
const LIST: f32 = 260.;

/// The downloads popover's body.
pub struct DownloadsList {
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl DownloadsList {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let downloads = DownloadsModel::global(cx);
        let jobs = JobsModel::global(cx);
        Self {
            scroll: UniformListScrollHandle::new(),
            _subscriptions: vec![
                cx.observe(&downloads, |_, _, cx| cx.notify()),
                cx.observe(&jobs, |_, _, cx| cx.notify()),
            ],
        }
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
            .h(ROW_HEIGHT)
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        v_flex()
            .w_full()
            .child(header(t("downloads"), action, cx))
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
                    .h(px(LIST.min(ROW * count as f32))),
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
    }
}
