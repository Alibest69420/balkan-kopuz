use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};
use std::rc::Rc;

use api::{ApiEvent, NoticeLevel};
use gpui_kit::{App, Context, Entity, EventEmitter, SharedString, Subscription};
use link::Link;

/// How many past messages the notification centre keeps.
const HISTORY_MAX: usize = 100;

/// What a toast offers to put back, and how.
#[derive(Clone)]
pub struct Undo {
    pub label: SharedString,
    action: Rc<dyn Fn(&mut App)>,
}

impl Undo {
    pub fn new(label: impl Into<SharedString>, action: impl Fn(&mut App) + 'static) -> Self {
        Self {
            label: label.into(),
            action: Rc::new(action),
        }
    }

    pub fn run(&self, cx: &mut App) {
        (self.action)(cx);
    }
}

impl std::fmt::Debug for Undo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Undo").field("label", &self.label).finish()
    }
}

/// A transient message for the user; the chrome turns each into a toast and a centre entry.
#[derive(Debug, Clone)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: SharedString,
    /// The job kind it came from, which `layout.notifications.jobs` can silence.
    pub kind: Option<SharedString>,
    pub undo: Option<Undo>,
}

impl Notice {
    pub fn new(level: NoticeLevel, message: impl Into<SharedString>) -> Self {
        Self {
            level,
            message: message.into(),
            kind: None,
            undo: None,
        }
    }

    pub fn kind(mut self, kind: impl Into<SharedString>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn with_undo(mut self, undo: Undo) -> Self {
        self.undo = Some(undo);
        self
    }

    /// The same sentence is the same notification, so a repeat replaces instead of stacking.
    pub fn id(&self) -> SharedString {
        let mut hasher = DefaultHasher::new();
        self.message.hash(&mut hasher);
        SharedString::from(format!("notice-{:016x}", hasher.finish()))
    }
}

/// One line of the notification centre.
#[derive(Debug, Clone)]
pub struct Record {
    pub id: SharedString,
    pub level: NoticeLevel,
    pub message: SharedString,
    /// How many times this exact message has arrived.
    pub repeats: usize,
}

/// The toast channel and the history behind the bell: anything can push, the chrome renders.
pub struct Notices {
    history: Vec<Record>,
    unseen: usize,
    _subscription: Subscription,
}

impl EventEmitter<Notice> for Notices {}

impl Notices {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| {
            if let ApiEvent::Notice {
                level,
                code,
                message,
            } = event
            {
                let text = message.clone().unwrap_or_else(|| i18n::t(code));
                this.say(Notice::new(*level, text), cx);
            }
        });
        Self {
            history: Vec::new(),
            unseen: 0,
            _subscription: subscription,
        }
    }

    pub fn history(&self) -> &[Record] {
        &self.history
    }

    pub fn unseen(&self) -> usize {
        self.unseen
    }

    pub fn mark_seen(&mut self, cx: &mut Context<Self>) {
        if self.unseen > 0 {
            self.unseen = 0;
            crate::notify(cx);
        }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if !self.history.is_empty() || self.unseen > 0 {
            self.history.clear();
            self.unseen = 0;
            crate::notify(cx);
        }
    }

    pub fn dismiss(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.history.len();
        self.history.retain(|record| record.id != id);
        if self.history.len() != before {
            self.unseen = self.unseen.min(self.history.len());
            crate::notify(cx);
        }
    }

    /// Record it, then hand it to whoever draws toasts.
    pub fn say(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.record(&notice, cx);
        cx.emit(notice);
    }

    fn record(&mut self, notice: &Notice, cx: &mut Context<Self>) {
        let id = notice.id();
        let repeats = match self.history.iter().position(|record| record.id == id) {
            Some(index) => self.history.remove(index).repeats + 1,
            None => 1,
        };
        self.history.insert(
            0,
            Record {
                id,
                level: notice.level,
                message: notice.message.clone(),
                repeats,
            },
        );
        self.history.truncate(HISTORY_MAX);
        self.unseen = (self.unseen + 1).min(self.history.len());
        crate::notify(cx);
    }

    pub fn info(message: impl Into<SharedString>, cx: &mut App) {
        Self::push(Notice::new(NoticeLevel::Info, message), cx);
    }

    pub fn error(message: impl Into<SharedString>, cx: &mut App) {
        Self::push(Notice::new(NoticeLevel::Error, message), cx);
    }

    /// An action that can be taken back for a few seconds, rather than confirmed beforehand.
    pub fn undo(
        message: impl Into<SharedString>,
        action: impl Fn(&mut App) + 'static,
        cx: &mut App,
    ) {
        let undo = Undo::new(i18n::t("undo"), action);
        Self::push(Notice::new(NoticeLevel::Info, message).with_undo(undo), cx);
    }

    /// Push a notice built elsewhere, from anywhere holding an `App`.
    pub fn push(notice: Notice, cx: &mut App) {
        Self::global(cx).update(cx, |this, cx| this.say(notice, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_sentence_is_the_same_notification() {
        let one = Notice::new(NoticeLevel::Error, "could not reach the daemon");
        let two = Notice::new(NoticeLevel::Info, "could not reach the daemon");
        let other = Notice::new(NoticeLevel::Error, "something else");
        assert_eq!(one.id(), two.id());
        assert_ne!(one.id(), other.id());
    }
}
