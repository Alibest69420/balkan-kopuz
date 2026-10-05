//! What the daemon is working on: every running job, its progress, and how to stop it.

use api::{JobKind, JobProgress};
use components::format::t;
use components::{Badge, BadgeTone, Chip, ChipTone, icons, style};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div, px,
};
use store::{Finished, JobsModel};

use crate::{DONE, FAILED, ROW_GLYPH, empty, header, row};

const BAR: Pixels = px(3.);

/// The activity popover's body.
pub struct ActivityList {
    _subscriptions: Vec<Subscription>,
}

impl ActivityList {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let jobs = JobsModel::global(cx);
        Self {
            _subscriptions: vec![cx.observe(&jobs, |_, _, cx| cx.notify())],
        }
    }

    fn job(&self, progress: &JobProgress, cx: &mut Context<Self>) -> AnyElement {
        let jobs = JobsModel::global(cx);
        let stopping = jobs.read(cx).is_stopping(&progress.id);
        let done = progress.current.unwrap_or(0);
        let total = progress.total.unwrap_or(0);
        let percent = match total {
            0 => None,
            total => Some(done as f32 / total as f32 * 100.),
        };
        let detail = progress
            .message
            .clone()
            .filter(|message| !message.is_empty())
            .or_else(|| (!progress.phase.is_empty()).then(|| progress.phase.clone()))
            .map(SharedString::from);
        let counted = (total > 0).then(|| SharedString::from(format!("{done} / {total}")));
        let id = progress.id.clone();
        let danger = cx.theme().danger;
        v_flex()
            .w_full()
            .gap_1()
            .px_3()
            .py_2()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(store::kind_label(progress.kind)),
                    )
                    .children(counted.map(|counted| {
                        div()
                            .flex_shrink_0()
                            .text_size(style::text_10())
                            .text_color(style::white(style::TEXT_MUTED, cx))
                            .child(counted)
                            .into_any_element()
                    }))
                    .child(match stopping {
                        true => Badge::new(t("stopping"))
                            .tone(BadgeTone::Muted)
                            .into_any_element(),
                        false => Chip::new("stop-job")
                            .label(t("stop"))
                            .tone(ChipTone::Ink(danger))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                JobsModel::global(cx).update(cx, |jobs, cx| jobs.cancel(id, cx));
                            })
                            .into_any_element(),
                    }),
            )
            .child(
                Progress::new(SharedString::from(format!("job-progress-{}", progress.id)))
                    .xsmall()
                    .h(BAR)
                    .map(|bar| match percent {
                        Some(percent) => bar.value(percent),
                        None => bar.loading(true),
                    })
                    .color(cx.theme().primary),
            )
            .children(detail.map(|detail| {
                div()
                    .w_full()
                    .truncate()
                    .text_size(style::text_10())
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .child(detail)
                    .into_any_element()
            }))
            .into_any_element()
    }

    /// A job that has just stopped, with what it ended as.
    fn outcome(&self, finished: &Finished, cx: &mut Context<Self>) -> AnyElement {
        let (glyph, ink) = match finished.ok {
            true => (DONE, cx.theme().success),
            false => (FAILED, cx.theme().danger),
        };
        let detail = finished
            .message
            .clone()
            .unwrap_or_else(|| match finished.ok {
                true => t("job_done"),
                false => t("job_stopped"),
            });
        row()
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
                            .child(store::kind_label(finished.kind)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(style::text_10())
                            .text_color(style::white(style::TEXT_MUTED, cx))
                            .child(detail),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ActivityList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let jobs = JobsModel::global(cx);
        let (running, recent) = {
            let jobs = jobs.read(cx);
            (jobs.running().to_vec(), jobs.recent().to_vec())
        };
        let scan_running = running.iter().any(|job| job.kind == JobKind::Scan);
        let mut rows: Vec<AnyElement> = running
            .iter()
            .map(|progress| self.job(progress, cx))
            .collect();
        rows.extend(recent.iter().map(|finished| self.outcome(finished, cx)));
        v_flex()
            .w_full()
            .child(header(t("activity"), None, cx))
            .when(rows.is_empty(), |body| {
                body.child(empty(t("activity_idle"), cx))
            })
            .when(!rows.is_empty(), |body| {
                body.child(
                    v_flex()
                        .id("activity-rows")
                        .w_full()
                        .py_1()
                        .max_h(crate::LIST_HEIGHT)
                        .overflow_y_scroll()
                        .children(rows),
                )
            })
            .when(!scan_running, |body| {
                body.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(style::white(style::BORDER_SOFT, cx))
                        .child(
                            Chip::new("start-scan")
                                .label(t("rescan_library"))
                                .tone(ChipTone::Ink(cx.theme().primary))
                                .on_click(|_, _, cx| {
                                    JobsModel::global(cx)
                                        .update(cx, |jobs, cx| jobs.start(JobKind::Scan, cx));
                                }),
                        ),
                )
            })
    }
}
