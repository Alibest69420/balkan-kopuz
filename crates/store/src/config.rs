use std::time::Duration;

use api::{ApiEvent, ConfigField, ConfigView, DAEMON_OWNED_CONFIG_KEYS};
use config::AppConfig;
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;
use serde_json::{Map, Value};

use crate::notices::Notices;

// Slider drags and typing settle before a write; the Dioxus app used the same delay.
const SAVE_DELAY: Duration = Duration::from_millis(600);

// How long a settings row says "Saved" before going quiet again.
const SAVED_FOR: Duration = Duration::from_millis(2200);

/// The daemon's settings surface, edited locally and patched back debounced, key by key.
pub struct ConfigModel {
    view: ConfigView,
    /// The daemon's config as last adopted, per top-level key; a local key that differs is unsent.
    baseline: Map<String, Value>,
    loaded: bool,
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
            ApiEvent::ConfigChanged { revision, .. } if *revision > this.view.revision => {
                this.reload(cx)
            }
            _ => {}
        });
        Self {
            view: ConfigView::default(),
            baseline: Map::new(),
            loaded: false,
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
        self._fetch = Some(cx.spawn(async move |this, cx| match call.await {
            Ok(view) => {
                this.update(cx, |this, cx| this.adopt(view, cx)).ok();
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

    /// Change settings; the keys that moved reach the daemon once edits settle.
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
        crate::notify(cx);
        self.schedule_save(cx);
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(call) = this.update(cx, |this, cx| {
                let fields = patch_of(&this.baseline, &this.view.config);
                (!fields.is_empty()).then(|| {
                    Link::global(cx).call(move |api| async move { api.patch_config(fields).await })
                })
            }) else {
                return;
            };
            let result = match call {
                Some(call) => Some(call.await),
                None => None,
            };
            this.update(cx, |this, cx| {
                this._save = None;
                match result {
                    Some(Ok(view)) => {
                        this.adopt(view, cx);
                        this.confirm(cx);
                    }
                    None => this.confirm(cx),
                    // The control is showing an edit the daemon never took; put the truth back.
                    Some(Err(error)) => {
                        Notices::error(crate::human(&error), cx);
                        this.pending.clear();
                        this.discard_unsent();
                        crate::notify(cx);
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

    /// Go back to what the daemon last said.
    fn discard_unsent(&mut self) {
        if let Ok(config) = serde_json::from_value(Value::Object(self.baseline.clone())) {
            self.view.config = config;
        }
    }

    /// Take the daemon's view, unless an older one lost the race, keeping local edits not yet sent.
    fn adopt(&mut self, view: ConfigView, cx: &mut Context<Self>) {
        if self.loaded && view.revision < self.view.revision {
            return;
        }
        let remote = fields_of(&view.config);
        let mut config = view.config;
        if self.loaded {
            let unsent = unsent(&self.baseline, &self.view.config);
            if !unsent.is_empty() {
                let mut merged = remote.clone();
                merged.extend(unsent);
                if let Ok(kept) = serde_json::from_value(Value::Object(merged)) {
                    config = kept;
                }
            }
        }
        if !self.loaded || config.language != self.view.config.language {
            i18n::set_locale(&config.language);
        }
        self.baseline = remote;
        self.view = ConfigView {
            config,
            locked_keys: view.locked_keys,
            revision: view.revision,
        };
        self.loaded = true;
        crate::notify(cx);
    }
}

fn fields_of(config: &AppConfig) -> Map<String, Value> {
    match serde_json::to_value(config) {
        Ok(Value::Object(object)) => object,
        _ => Map::new(),
    }
}

/// The caller-writable keys whose local value differs from what the daemon last said.
fn unsent(baseline: &Map<String, Value>, local: &AppConfig) -> Map<String, Value> {
    fields_of(local)
        .into_iter()
        .filter(|(key, value)| {
            !DAEMON_OWNED_CONFIG_KEYS.contains(&key.as_str()) && baseline.get(key) != Some(value)
        })
        .collect()
}

fn patch_of(baseline: &Map<String, Value>, local: &AppConfig) -> Vec<ConfigField> {
    unsent(baseline, local)
        .into_iter()
        .map(|(key, value)| ConfigField {
            key,
            json: value.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_keys_that_moved_are_sent() {
        let before = AppConfig::default();
        let mut after = before.clone();
        after.show_row_images = !before.show_row_images;
        let fields = patch_of(&fields_of(&before), &after);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "show_row_images");
    }

    #[test]
    fn a_key_the_daemon_owns_is_never_sent() {
        let before = AppConfig::default();
        let mut after = before.clone();
        after.volume = 0.1;
        after
            .pinned_stations
            .push("https://example.invalid/station.json".into());
        assert!(patch_of(&fields_of(&before), &after).is_empty());
    }
}
