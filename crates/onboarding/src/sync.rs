//! Step three: the first scan or sync, with whatever the daemon is reporting about it.

use api::JobProgress;
use components::format::{t, t_with};
use components::{Heading, ProgressBar, SectionHeading, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled, Window, div,
};
use store::JobsModel;

use crate::{FirstRun, active_source, ready_to_sync};

/// `h-1.5`, the bar under the heading.
const BAR: gpui_kit::Pixels = gpui_kit::px(6.);

impl FirstRun {
    pub(crate) fn sync_step(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let source = active_source(cx);
        let waiting = source.as_ref().is_none_or(|source| !ready_to_sync(source));
        // Whichever library job the daemon started for the source, scan or sync alike.
        let progress = JobsModel::global(cx)
            .read(cx)
            .running()
            .iter()
            .find(|job| matches!(job.kind, api::JobKind::Scan | api::JobKind::LibrarySync))
            .cloned();
        let share = progress.as_ref().and_then(components::job::fraction);
        v_flex()
            .w_full()
            .gap_3()
            .child(SectionHeading::new(t("first_run_sync_title")).heading(Heading::Step))
            .when(waiting, |step| {
                step.child(crate::ui::hint(t("first_run_waiting_sign_in"), cx))
                    .child(
                        Button::new("first-run-sign-in")
                            .outline()
                            .small()
                            .label(t("login"))
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                this.sign_in_again(window, cx)
                            }))
                            .self_start(),
                    )
            })
            .when(!waiting, |step| {
                step.child(crate::ui::hint(t("first_run_sync_hint"), cx))
                    .child(ProgressBar::new("first-run-progress", share).height(BAR))
            })
            .when_some(progress.filter(|_| !waiting), |step, progress| {
                step.child(line(&progress, cx))
            })
            .child(
                Button::new("first-run-done")
                    .primary()
                    .label(t("first_run_go_to_library"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.finish(cx)))
                    .self_start(),
            )
            .into_any_element()
    }

    /// Ask for the sign-in again, which is the daemon's browser flow or the registered form.
    fn sign_in_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = active_source(cx) else {
            return;
        };
        store::start_sign_in(&source, window, cx);
    }
}

/// What the job is on, and how far through it is where it counts.
fn line(progress: &JobProgress, cx: &Context<FirstRun>) -> impl IntoElement {
    let phase = SharedString::from(progress.phase.clone());
    let message = progress
        .message
        .clone()
        .filter(|message| !message.is_empty())
        .map(SharedString::from);
    let counted = match (progress.current, progress.total) {
        (Some(current), Some(total)) if total > 0 => Some(t_with(
            "first_run_progress",
            &[
                ("current", current.to_string()),
                ("total", total.to_string()),
            ],
        )),
        _ => None,
    };
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(style::white(style::TEXT_SECONDARY, cx))
        .when(!phase.is_empty(), |row| {
            row.child(div().flex_shrink_0().child(phase))
        })
        .when_some(message, |row, message| {
            row.child(div().flex_1().min_w_0().truncate().child(message))
        })
        .when_some(counted, |row, counted| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_color(style::slate_500(cx))
                    .child(counted),
            )
        })
}
