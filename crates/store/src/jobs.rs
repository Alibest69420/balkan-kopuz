use api::{ApiEvent, ErrorBody, ErrorCode, JobKind, JobProgress, JobState};
use gpui_kit::{App, Context, Entity, EventEmitter, Subscription, Task};
use link::Link;

use crate::notices::Notices;

/// A job reached its terminal state.
#[derive(Debug, Clone)]
pub struct JobEvent {
    pub id: String,
    pub kind: JobKind,
    pub ok: bool,
    pub error: Option<ErrorBody>,
}

/// Progress of the daemon's long-running work, by kind.
pub struct JobsModel {
    running: Vec<JobProgress>,
    _fetch: Option<Task<()>>,
    _subscription: Subscription,
}

impl EventEmitter<JobEvent> for JobsModel {}

impl JobsModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| match event {
            ApiEvent::JobProgress(progress) => {
                match this
                    .running
                    .iter_mut()
                    .find(|running| running.id == progress.id)
                {
                    Some(running) => *running = progress.clone(),
                    None => this.running.push(progress.clone()),
                }
                cx.notify();
            }
            ApiEvent::JobFinished {
                id,
                kind,
                ok,
                error,
            } => {
                this.running.retain(|running| &running.id != id);
                cx.notify();
                // A job that fails says so. Upstream drops this, which is why a sync against
                // expired credentials spins for a moment and then goes quiet.
                if !*ok && let Some(failure) = error.as_ref() {
                    Notices::error(failure.message.clone(), cx);
                }
                cx.emit(JobEvent {
                    id: id.clone(),
                    kind: *kind,
                    ok: *ok,
                    error: error.clone(),
                });
            }
            ApiEvent::Resync => this.reseed(cx),
            _ => {}
        });
        Self {
            running: Vec::new(),
            _fetch: None,
            _subscription: subscription,
        }
    }

    fn reseed(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.jobs().await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let Ok(jobs) = call.await else { return };
            this.update(cx, |this, cx| {
                this.running = jobs
                    .into_iter()
                    .filter(|job| job.state == JobState::Running)
                    .map(|job| JobProgress {
                        id: job.id,
                        kind: job.kind,
                        phase: job.phase,
                        current: job.current,
                        total: job.total,
                        message: job.message,
                    })
                    .collect();
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn progress(&self, kind: JobKind) -> Option<&JobProgress> {
        self.running.iter().find(|job| job.kind == kind)
    }

    pub fn is_running(&self, kind: JobKind) -> bool {
        self.progress(kind).is_some()
    }

    pub fn running(&self) -> &[JobProgress] {
        &self.running
    }

    /// Start a job; a kind already running answers `Conflict`, which is not an error here.
    pub fn start(&mut self, kind: JobKind, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.start_job(kind).await });
        cx.spawn(async move |_, cx| match call.await {
            Ok(_)
            | Err(api::ApiError {
                code: ErrorCode::Conflict,
                ..
            }) => {}
            Err(error) => {
                cx.update(|cx| Notices::error(crate::human(&error), cx));
            }
        })
        .detach();
    }

    pub fn cancel(&mut self, id: String, cx: &mut Context<Self>) {
        Link::global(cx).fire(cx, "cancel job", move |api| async move {
            api.cancel_job(id).await
        });
    }
}
