use api::{SourceCapabilities, SourceInfo, Table};
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

use crate::config::ConfigModel;
use crate::invalidation::{Invalidation, TablesChanged};
use crate::notices::Notices;

/// Configured sources and the active one's capabilities, which gate every affordance.
pub struct SourcesModel {
    sources: Vec<SourceInfo>,
    loaded: bool,
    _fetch: Option<Task<()>>,
    _subscription: Subscription,
}

impl SourcesModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let invalidation = Invalidation::global(cx);
        let subscription = cx.subscribe(&invalidation, |this, _, event: &TablesChanged, cx| {
            if event.touches(&[Table::Servers]) {
                this.refetch(cx);
            }
        });
        Self {
            sources: Vec::new(),
            loaded: false,
            _fetch: None,
            _subscription: subscription,
        }
    }

    pub fn refetch(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.sources().await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let Ok(sources) = call.await else { return };
            this.update(cx, |this, cx| this.adopt(sources, cx)).ok();
        }));
    }

    fn adopt(&mut self, sources: Vec<SourceInfo>, cx: &mut Context<Self>) {
        let before = self.active().map(|source| source.id.clone());
        self.sources = sources;
        let after = self.active().map(|source| source.id.clone());
        let switched = self.loaded && before != after;
        self.loaded = true;
        cx.notify();
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

    /// The active source's capabilities; all off until sources load.
    pub fn capabilities(&self) -> SourceCapabilities {
        self.active()
            .map(|source| source.capabilities)
            .unwrap_or_default()
    }

    /// Make `id` active; the refetch that follows invalidates every read.
    pub fn switch(&mut self, id: String, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.switch_source(id).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| match result {
                Ok(_) => {
                    this.refetch(cx);
                    ConfigModel::global(cx).update(cx, |config, cx| config.reload(cx));
                }
                Err(error) => Notices::error(crate::human(&error), cx),
            })
            .ok();
        })
        .detach();
    }
}
