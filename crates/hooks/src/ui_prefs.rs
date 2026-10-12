//! This app's own UI state (sorts, view modes, layout), mirrored from the daemon's per-frontend preference store and written back once edits settle.

use std::collections::BTreeMap;
use std::time::Duration;

use dioxus::prelude::*;
use tracing::Instrument;

pub use config::UiPrefs;

/// The name this app's preferences are kept under in the daemon.
pub const FRONTEND: &str = "kopuz";

// A hero-height or sidebar drag settles before the daemon is written to.
const SAVE_SETTLE: Duration = Duration::from_millis(600);
const IDLE_POLL: Duration = Duration::from_millis(250);

type Entries = BTreeMap<String, String>;

#[derive(Clone, Copy)]
pub struct UiPrefsStore {
    prefs: Signal<UiPrefs>,
    /// What the daemon is known to hold, per key; a key whose value differs is waiting to be sent.
    synced: Signal<Entries>,
    loaded: Signal<bool>,
    generation: Signal<u64>,
}

/// Provide the store from the app root, read from the daemon now and written back debounced.
pub fn use_ui_prefs_provider() -> UiPrefsStore {
    let store = use_hook(|| {
        let defaults = UiPrefs::default();
        UiPrefsStore {
            synced: Signal::new_in_scope(defaults.entries(), ScopeId::ROOT),
            prefs: Signal::new_in_scope(defaults, ScopeId::ROOT),
            loaded: Signal::new_in_scope(false, ScopeId::ROOT),
            generation: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    });
    use_context_provider(|| store);
    use_hook(move || store.reload());

    let mut dirty = use_signal(|| 0u64);
    use_effect(move || {
        let _ = store.prefs.read();
        if *store.loaded.read() {
            dirty += 1;
        }
    });
    use_future(move || async move {
        let api = crate::api::consume_api();
        let mut flushed = 0u64;
        loop {
            let seen = *dirty.peek();
            if seen == flushed {
                utils::sleep(IDLE_POLL).await;
                continue;
            }
            utils::sleep(SAVE_SETTLE).await;
            if *dirty.peek() != seen {
                continue;
            }
            flushed = seen;
            store.save(&api).await;
        }
    });
    store
}

/// The UI state, for reading and editing; an edit reaches the daemon once edits settle.
pub fn use_ui_prefs() -> Signal<UiPrefs> {
    use_context::<UiPrefsStore>().prefs
}

/// Remember that the user picked a source themselves, which only the first pick changes.
pub fn mark_source_chosen() {
    let Some(store) = try_consume_context::<UiPrefsStore>() else {
        return;
    };
    let mut prefs = store.prefs;
    if !prefs.peek().source_explicitly_set {
        prefs.write().source_explicitly_set = true;
    }
}

impl UiPrefsStore {
    pub fn prefs(self) -> Signal<UiPrefs> {
        self.prefs
    }

    /// Re-read what the daemon holds, keeping any edit not sent yet; a later call supersedes one still in flight.
    pub fn reload(mut self) {
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);
        let api = crate::api::consume_api();
        spawn(
            async move {
                match api.frontend_prefs(FRONTEND.to_string()).await {
                    Ok(stored) if *self.generation.peek() == generation => self.adopt(stored),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "could not read the UI preferences"),
                }
            }
            .instrument(tracing::info_span!("ui_prefs.load")),
        );
    }

    fn adopt(mut self, stored: Vec<api::FrontendPref>) {
        let stored: Entries = stored
            .into_iter()
            .map(|pref| (pref.key, pref.value))
            .collect();
        let remote = UiPrefs::from_entries(stored.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let current = self.prefs.peek().entries();
        let synced = self.synced.peek().clone();
        let mut merged = remote.entries();
        for (key, value) in current {
            if synced.get(&key) != Some(&value) {
                merged.insert(key, value);
            }
        }
        self.synced.set(remote.entries());
        let next = UiPrefs::from_entries(merged.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        if next != *self.prefs.peek() {
            self.prefs.set(next);
        }
        if !*self.loaded.peek() {
            self.loaded.set(true);
        }
    }

    /// Every key the daemon does not hold yet, as the entries that would store it; none before the first read.
    pub fn unsent(self) -> Vec<api::PrefEntry> {
        if !*self.loaded.peek() {
            return Vec::new();
        }
        unsent_of(&self.prefs.peek(), &self.synced.peek())
    }

    /// Like [`Self::unsent`], but re-runs the calling effect when the state or what the daemon holds moves.
    pub fn unsent_tracked(self) -> Vec<api::PrefEntry> {
        if !*self.loaded.read() {
            return Vec::new();
        }
        unsent_of(&self.prefs.read(), &self.synced.read())
    }

    async fn save(mut self, api: &std::sync::Arc<dyn api::KopuzApi>) {
        let entries = self.unsent();
        if entries.is_empty() {
            return;
        }
        match api
            .set_frontend_prefs(FRONTEND.to_string(), entries.clone())
            .instrument(tracing::info_span!("ui_prefs.persist"))
            .await
        {
            Ok(()) => {
                let mut synced = self.synced.write();
                for entry in entries {
                    if let Some(value) = entry.value {
                        synced.insert(entry.key, value);
                    }
                }
            }
            Err(error) => tracing::error!(%error, "failed to save the UI preferences"),
        }
    }
}

fn unsent_of(prefs: &UiPrefs, synced: &Entries) -> Vec<api::PrefEntry> {
    prefs
        .entries()
        .into_iter()
        .filter(|(key, value)| synced.get(key) != Some(value))
        .map(|(key, value)| api::PrefEntry {
            key,
            value: Some(value),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_keys_that_moved_are_sent() {
        let synced = UiPrefs::default().entries();
        let prefs = UiPrefs {
            hero_height: 360,
            ..UiPrefs::default()
        };
        assert_eq!(
            unsent_of(&prefs, &synced),
            [api::PrefEntry {
                key: "hero_height".into(),
                value: Some("360".into()),
            }]
        );
        assert!(unsent_of(&UiPrefs::default(), &synced).is_empty());
    }
}
