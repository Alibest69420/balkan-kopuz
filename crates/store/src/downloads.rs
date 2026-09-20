use std::collections::HashMap;
use std::collections::HashSet;

use api::{DownloadItemState, JobKind, Table};
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

use crate::invalidation::{Invalidation, TablesChanged};
use crate::jobs::{JobEvent, JobsModel};
use crate::notices::Notices;

/// Which tracks have an offline copy, and what the running download batch is doing.
pub struct DownloadsModel {
    stored: HashSet<String>,
    active: HashMap<String, DownloadItemState>,
    _fetch: Option<Task<()>>,
    _statuses: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl DownloadsModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let invalidation = Invalidation::global(cx);
        let jobs = JobsModel::global(cx);
        let subscriptions = vec![
            cx.subscribe(&invalidation, |this, _, event: &TablesChanged, cx| {
                if event.touches(&[Table::Tracks]) {
                    this.refetch(cx);
                }
            }),
            cx.observe(&jobs, |this, jobs, cx| {
                if jobs.read(cx).is_running(JobKind::Download) {
                    this.refresh_statuses(cx);
                }
            }),
            cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                if event.kind == JobKind::Download {
                    this.active.clear();
                    this.refetch(cx);
                }
            }),
        ];
        Self {
            stored: HashSet::new(),
            active: HashMap::new(),
            _fetch: None,
            _statuses: None,
            _subscriptions: subscriptions,
        }
    }

    fn refetch(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.downloads().await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let Ok(keys) = call.await else { return };
            this.update(cx, |this, cx| {
                this.stored = keys.into_iter().collect();
                cx.notify();
            })
            .ok();
        }));
    }

    fn refresh_statuses(&mut self, cx: &mut Context<Self>) {
        if self._statuses.is_some() {
            return;
        }
        let call = Link::global(cx).call(|api| async move { api.download_statuses().await });
        self._statuses = Some(cx.spawn(async move |this, cx| {
            let statuses = call.await;
            this.update(cx, |this, cx| {
                this._statuses = None;
                if let Ok(statuses) = statuses {
                    this.active = statuses
                        .into_iter()
                        .map(|status| (status.key, status.state))
                        .collect();
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    pub fn is_stored(&self, key: &str) -> bool {
        self.stored.contains(key)
    }

    pub fn state(&self, key: &str) -> Option<DownloadItemState> {
        self.active.get(key).copied()
    }

    pub fn is_active(&self, key: &str) -> bool {
        matches!(
            self.state(key),
            Some(DownloadItemState::Queued | DownloadItemState::Downloading)
        )
    }

    pub fn stored(&self) -> &HashSet<String> {
        &self.stored
    }

    pub fn active(&self) -> &HashMap<String, DownloadItemState> {
        &self.active
    }

    pub fn download(&mut self, keys: Vec<String>, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.download(keys).await });
        cx.spawn(async move |_, cx| {
            if let Err(error) = call.await {
                cx.update(|cx| Notices::error(crate::human(&error), cx));
            }
        })
        .detach();
    }

    pub fn remove(&mut self, key: String, cx: &mut Context<Self>) {
        self.stored.remove(&key);
        cx.notify();
        let call = Link::global(cx).call(move |api| async move { api.remove_download(key).await });
        cx.spawn(async move |this, cx| {
            if let Err(error) = call.await {
                cx.update(|cx| Notices::error(crate::human(&error), cx));
            }
            this.update(cx, |this, cx| this.refetch(cx)).ok();
        })
        .detach();
    }
}
