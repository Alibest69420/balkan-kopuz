//! Step three: the first scan or sync, with whatever the daemon is reporting about it.

use components::format::t;
use components::{Heading, JobLine, ProgressBar, SectionHeading, StatusLine, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled, Window, div};
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
                step.child(
                    StatusLine::new(store::SourceStatus::Disconnected)
                        .label(t("first_run_waiting_sign_in"))
                        .on_sign_in(cx.listener(|this: &mut Self, _, window, cx| {
                            this.sign_in_again(window, cx)
                        })),
                )
            })
            .when(!waiting, |step| {
                step.child(crate::ui::hint(t("first_run_sync_hint"), cx))
                    .child(ProgressBar::new("first-run-progress", share).height(BAR))
            })
            .when_some(progress.filter(|_| !waiting), |step, progress| {
                step.child(
                    div()
                        .text_xs()
                        .text_color(style::white(style::TEXT_SECONDARY, cx))
                        .child(JobLine::new(&progress)),
                )
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
