use std::collections::HashMap;

use api::{ApiEvent, SourceCapabilities, SourceInfo, SourceState, Table};
use gpui_kit::{App, Context, Entity, Subscription, Task, Window};
use link::Link;

use crate::config::ConfigModel;
use crate::invalidation::{Invalidation, TablesChanged};
use crate::notices::Notices;

/// What a source's row says about the link to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Connected,
    Connecting,
    Disconnected,
}

impl SourceStatus {
    /// The daemon's probe answer as a status; no answer yet reads as connecting.
    pub fn of(state: Option<SourceState>) -> Self {
        match state {
            Some(SourceState::Online) => Self::Connected,
            Some(SourceState::AuthExpired | SourceState::Offline) => Self::Disconnected,
            Some(SourceState::Checking) | None => Self::Connecting,
        }
    }
}

/// Configured sources and the active one's capabilities, which gate every affordance.
pub struct SourcesModel {
    sources: Vec<SourceInfo>,
    /// The daemon's latest probe answers, laid over each fetch: a fetch that left before an answer carries the older one.
    heard: HashMap<String, SourceState>,
    loaded: bool,
    _fetch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl SourcesModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let invalidation = Invalidation::global(cx);
        let subscriptions = vec![
            cx.subscribe(&invalidation, |this, _, event: &TablesChanged, cx| {
                if event.touches(&[Table::Servers]) {
                    this.refetch(cx);
                }
            }),
            cx.subscribe(&Link::connection(cx), |this, _, event: &ApiEvent, cx| {
                match event {
                    ApiEvent::SourceStatus { source, state } => {
                        this.heard.insert(source.clone(), *state);
                        if let Some(row) = this.sources.iter_mut().find(|row| &row.id == source) {
                            row.state = Some(*state);
                            crate::notify(cx);
                        }
                    }
                    // Answers may have been lost; the refetch the resync starts is trusted over them.
                    ApiEvent::Resync => this.heard.clear(),
                    _ => {}
                }
            }),
        ];
        Self {
            sources: Vec::new(),
            heard: HashMap::new(),
            loaded: false,
            _fetch: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn refetch(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.sources().await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let Ok(sources) = call.await else { return };
            this.update(cx, |this, cx| this.adopt(sources, cx)).ok();
        }));
    }

    fn adopt(&mut self, mut sources: Vec<SourceInfo>, cx: &mut Context<Self>) {
        let before = self.active().map(|source| source.id.clone());
        for row in &mut sources {
            if let Some(state) = self.heard.get(&row.id) {
                row.state = Some(*state);
            }
        }
        self.sources = sources;
        let after = self.active().map(|source| source.id.clone());
        let switched = self.loaded && before != after;
        self.loaded = true;
        crate::notify(cx);
        if switched {
            Invalidation::global(cx).update(cx, |invalidation, cx| invalidation.invalidate_all(cx));
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn sources(&self) -> &[SourceInfo] {
        &self.sources
    }

    pub fn active(&self) -> Option<&SourceInfo> {
        self.sources.iter().find(|source| source.active)
    }

    /// The active source's link as the daemon's probe last found it.
    pub fn active_status(&self) -> SourceStatus {
        SourceStatus::of(self.active().and_then(|source| source.state))
    }

    /// The active source's capabilities; all off until sources load.
    pub fn capabilities(&self) -> SourceCapabilities {
        self.active()
            .map(|source| source.capabilities)
            .unwrap_or_default()
    }

    /// Make `id` active, and start whichever sign-in the daemon says it still needs.
    pub fn switch(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.switch_source(id).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(source) => {
                    this.refetch(cx);
                    ConfigModel::global(cx).update(cx, |config, cx| config.reload(cx));
                    crate::signin::start_sign_in(&source, window, cx);
                }
                Err(error) => Notices::error(crate::human(&error), cx),
            })
            .ok();
        })
        .detach();
    }
}
