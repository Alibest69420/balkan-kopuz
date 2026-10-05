use std::time::Duration;

use api::{ApiEvent, ConfigView};
use config::AppConfig;
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

use crate::notices::Notices;

// Slider drags and typing settle before a write; the Dioxus app used the same delay.
const SAVE_DELAY: Duration = Duration::from_millis(600);

// How long a settings row says "Saved" before going quiet again.
const SAVED_FOR: Duration = Duration::from_millis(2200);

/// The daemon's settings surface, edited locally and written back debounced.
pub struct ConfigModel {
    view: ConfigView,
    loaded: bool,
    // Bumped per local edit, so a save's answer never clobbers a newer edit.
    edits: u64,
    // The keys edited since the last write went out, then the ones that write carried.
    pending: Vec<&'static str>,
    saved: Vec<&'static str>,
    _save: Option<Task<()>>,
    _tick: Option<Task<()>>,
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
            pending: Vec::new(),
            saved: Vec::new(),
            _save: None,
            _tick: None,
            _fetch: None,
            _subscription: subscription,
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.view.config
    }

    /// Whether track rows carry their cover, which every list of tracks asks.
    pub fn row_images(&self) -> bool {
        self.view.config.show_row_images
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

    /// Whether a write for `key` has just landed, which is what a row's "Saved" tick reads.
    pub fn just_saved(&self, key: &str) -> bool {
        self.saved.contains(&key)
    }

    /// A write is on its way out or in flight.
    pub fn is_saving(&self) -> bool {
        self._save.is_some()
    }

    /// Change settings from anywhere holding an `App`.
    pub fn edit(cx: &mut App, edit: impl FnOnce(&mut AppConfig)) {
        Self::global(cx).update(cx, |model, cx| model.update(cx, edit));
    }

    /// The same, naming the key it touches so that key's row can confirm the write.
    pub fn edit_key(key: &'static str, cx: &mut App, edit: impl FnOnce(&mut AppConfig)) {
        Self::global(cx).update(cx, |model, cx| model.update_key(key, cx, edit));
    }

    /// What a settings control calls with the value it picked, written under `key`.
    pub fn writer<T: 'static>(
        key: &'static str,
        set: impl Fn(&mut AppConfig, T) + 'static,
    ) -> impl Fn(T, &mut App) + 'static {
        move |value, cx| Self::edit_key(key, cx, |config| set(config, value))
    }

    /// The same edit, naming the key it touches so that row can confirm the write.
    pub fn update_key(
        &mut self,
        key: &'static str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut AppConfig),
    ) {
        if !self.pending.contains(&key) {
            self.pending.push(key);
        }
        self.saved.retain(|held| *held != key);
        self.update(cx, edit);
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
        crate::notify(cx);
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
                    Ok(view) if this.edits == edits => {
                        this.adopt(view, cx);
                        this.confirm(cx);
                    }
                    Ok(_) => this.confirm(cx),
                    // The control is showing an edit the daemon never took; put the truth back.
                    Err(error) => {
                        Notices::error(crate::human(&error), cx);
                        this.pending.clear();
                        this.reload(cx);
                    }
                }
            })
            .ok();
        }));
    }

    /// Mark what the write carried as saved, and take the mark off again a moment later.
    fn confirm(&mut self, cx: &mut Context<Self>) {
        self.saved = std::mem::take(&mut self.pending);
        if self.saved.is_empty() {
            return;
        }
        crate::notify(cx);
        self._tick = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVED_FOR).await;
            this.update(cx, |this, cx| {
                this.saved.clear();
                crate::notify(cx);
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
        crate::notify(cx);
    }
}
