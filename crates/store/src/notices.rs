use api::{ApiEvent, NoticeLevel};
use gpui_kit::{App, Context, Entity, EventEmitter, SharedString, Subscription};
use link::Link;

/// A transient message for the user; the window shell turns each into a toast.
#[derive(Debug, Clone)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: SharedString,
}

/// The toast channel: anything can push, only the window shell listens.
pub struct Notices {
    _subscription: Subscription,
}

impl EventEmitter<Notice> for Notices {}

impl Notices {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |_, _, event: &ApiEvent, cx| {
            if let ApiEvent::Notice {
                level,
                code,
                message,
            } = event
            {
                let text = message.clone().unwrap_or_else(|| i18n::t(code));
                cx.emit(Notice {
                    level: *level,
                    message: text.into(),
                });
            }
        });
        Self {
            _subscription: subscription,
        }
    }

    pub fn info(message: impl Into<SharedString>, cx: &mut App) {
        Self::push(NoticeLevel::Info, message.into(), cx);
    }

    pub fn error(message: impl Into<SharedString>, cx: &mut App) {
        Self::push(NoticeLevel::Error, message.into(), cx);
    }

    fn push(level: NoticeLevel, message: SharedString, cx: &mut App) {
        Self::global(cx).update(cx, |_, cx| cx.emit(Notice { level, message }));
    }
}
