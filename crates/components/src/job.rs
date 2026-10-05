//! What a running job shows: how far along it is, and how to stop it.

use api::JobProgress;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, IntoElement, Pixels, RenderOnce, SharedString, Styled as _, Window, px};
use store::JobsModel;

use crate::format::t;
use crate::{Badge, BadgeTone, Chip, ChipTone};

/// How far along a job is, or nothing while it cannot say.
pub fn fraction(progress: &JobProgress) -> Option<f32> {
    let total = progress.total.filter(|total| *total > 0)?;
    let current = progress.current?.min(total);
    Some(current as f32 / total as f32)
}

/// The thin accent bar under a job: filled to `fraction`, or sweeping while there is none.
#[derive(IntoElement)]
pub struct ProgressBar {
    id: SharedString,
    fraction: Option<f32>,
    height: Pixels,
}

impl ProgressBar {
    pub fn new(id: impl Into<SharedString>, fraction: Option<f32>) -> Self {
        Self {
            id: id.into(),
            fraction,
            height: px(3.),
        }
    }

    /// The bar for a job, read off its progress.
    pub fn job(progress: &JobProgress) -> Self {
        Self::new(format!("job-progress-{}", progress.id), fraction(progress))
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        Progress::new(self.id)
            .xsmall()
            .h(self.height)
            .map(|bar| match self.fraction {
                Some(fraction) => bar.value(fraction * 100.),
                None => bar.loading(true),
            })
            .color(cx.theme().primary)
    }
}

/// A danger chip that stops a job, which reads "Stopping" once it has been asked to.
#[derive(IntoElement)]
pub struct JobStop {
    id: String,
}

impl JobStop {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl RenderOnce for JobStop {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if JobsModel::global(cx).read(cx).is_stopping(&self.id) {
            return Badge::new(t("stopping"))
                .tone(BadgeTone::Muted)
                .into_any_element();
        }
        let id = self.id;
        Chip::new(SharedString::from(format!("stop-job-{id}")))
            .label(t("stop"))
            .tone(ChipTone::Ink(cx.theme().danger))
            .on_click(move |_, _, cx| {
                let id = id.clone();
                JobsModel::global(cx).update(cx, |jobs, cx| jobs.cancel(id, cx));
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::JobKind;

    fn progress(current: Option<u64>, total: Option<u64>) -> JobProgress {
        JobProgress {
            id: "job".into(),
            kind: JobKind::Scan,
            phase: String::new(),
            current,
            total,
            message: None,
        }
    }

    #[test]
    fn a_job_without_a_count_and_a_total_has_nothing_to_fill() {
        assert_eq!(fraction(&progress(Some(3), None)), None);
        assert_eq!(fraction(&progress(None, Some(10))), None);
        assert_eq!(fraction(&progress(Some(3), Some(0))), None);
    }

    #[test]
    fn a_counted_job_fills_its_share_and_never_past_the_end() {
        assert_eq!(fraction(&progress(Some(5), Some(10))), Some(0.5));
        assert_eq!(fraction(&progress(Some(30), Some(10))), Some(1.));
    }
}
