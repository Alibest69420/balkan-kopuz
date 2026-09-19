use std::time::Duration;

use api::{ApiEvent, ConfigView};
use config::AppConfig;
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

use crate::notices::Notices;

// Slider drags and typing settle before a write; the Dioxus app used the same delay.
const SAVE_DELAY: Duration = Duration::from_millis(600);

/// The daemon's settings surface, edited locally and written back debounced.
pub struct ConfigModel {
    view: ConfigView,
    loaded: bool,
    // Bumped per local edit, so a save's answer never clobbers a newer edit.
    edits: u64,
    _save: Option<Task<()>>,
    _fetch: Option<Task<()>>,
    _subscription: Subscription,
}

impl ConfigModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| match event {
            ApiEvent::Resync => this.reload(cx),
            ApiEvent::ConfigChanged { .. } if this._save.is_none() => this.reload(cx),
            _ => {}
        });
        Self {
            view: ConfigView::default(),
            loaded: false,
            edits: 0,
            _save: None,
            _fetch: None,
            _subscription: subscription,
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.view.config
    }

    /// Nothing is saved until the daemon's config has been read, so a default is never persisted.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn locked_keys(&self) -> &[String] {
        &self.view.locked_keys
    }

    /// A managed settings file pins `key`; its control renders locked.
    pub fn is_locked(&self, key: &str) -> bool {
        self.view.locked_keys.iter().any(|locked| locked == key)
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.config().await });
        let edits = self.edits;
        self._fetch = Some(cx.spawn(async move |this, cx| match call.await {
            Ok(view) => {
                this.update(cx, |this, cx| {
                    if this.edits == edits {
                        this.adopt(view, cx);
                    }
                })
                .ok();
            }
            Err(error) => tracing::warn!(%error, "could not read the daemon config"),
        }));
    }

    /// Change settings; the write reaches the daemon once edits settle.
    pub fn update(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut AppConfig)) {
        if !self.loaded {
            tracing::warn!("config edit before the daemon config loaded; ignored");
            return;
        }
        let language = self.view.config.language.clone();
        edit(&mut self.view.config);
        if self.view.config.language != language {
            i18n::set_locale(&self.view.config.language);
        }
        self.edits += 1;
        cx.notify();
        self.schedule_save(cx);
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok((call, edits)) = this.update(cx, |this, cx| {
                let config = this.view.config.clone();
                let call =
                    Link::global(cx).call(move |api| async move { api.set_config(config).await });
                (call, this.edits)
            }) else {
                return;
            };
            let result = call.await;
            this.update(cx, |this, cx| {
                this._save = None;
                match result {
                    Ok(view) if this.edits == edits => this.adopt(view, cx),
                    Ok(_) => {}
                    Err(error) => Notices::error(error.to_string(), cx),
                }
            })
            .ok();
        }));
    }

    fn adopt(&mut self, view: ConfigView, cx: &mut Context<Self>) {
        if !self.loaded || view.config.language != self.view.config.language {
            i18n::set_locale(&view.config.language);
        }
        self.view = view;
        self.loaded = true;
        cx.notify();
    }
}
