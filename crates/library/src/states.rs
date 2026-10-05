//! What the page shows instead of rows, and the one button that gets you out of it.

use api::ApiError;
use components::format::t;
use components::{icons, motion, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div, px};
use store::{Navigator, Route, SourcesModel};

/// The shared icon set has no sign-in glyph, which is the one an anonymous source wants.
pub const SIGN_IN: &str = "icons/log-in.svg";

/// A state with a way out of it: a line, a reason, and the button that does the obvious thing.
pub struct State {
    icon: &'static str,
    title: SharedString,
    detail: Option<SharedString>,
    action: Option<(SharedString, Act)>,
    danger: bool,
}

type Act = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

impl State {
    pub fn new(icon: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            icon,
            title: title.into(),
            detail: None,
            action: None,
            danger: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        act: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), std::rc::Rc::new(act)));
        self
    }

    pub fn render(self, window: &mut Window, cx: &mut App) -> gpui_kit::AnyElement {
        let fade = motion::enter(
            motion::id("library-state", &self.title),
            motion::MEDIUM,
            window,
            cx,
        );
        let glyph = match self.danger {
            true => cx.theme().danger,
            false => style::white(style::TEXT_FAINT, cx),
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .px_6()
            .py_12()
            .opacity(fade)
            .text_center()
            .text_color(style::white(style::TEXT_SUBTLE, cx))
            .child(
                Icon::default()
                    .path(self.icon)
                    .size(px(36.))
                    .mb_4()
                    .text_color(glyph),
            )
            .child(div().text_base().child(self.title))
            .when_some(self.detail, |column, detail| {
                column.child(
                    div()
                        .text_sm()
                        .text_color(style::white(style::TEXT_DIM, cx))
                        .child(detail),
                )
            })
            .when_some(self.action, |column, (label, act)| {
                column.child(
                    div().mt_5().child(
                        Button::new("library-state-action")
                            .primary()
                            .label(label)
                            .on_click(move |_, window, cx| act(window, cx)),
                    ),
                )
            })
            .into_any_element()
    }
}

/// A read that failed says so, and offers the one thing worth trying: asking again.
pub fn failed(error: &ApiError, retry: impl Fn(&mut Window, &mut App) + 'static) -> State {
    State::new(icons::WARNING, t("library_read_failed"))
        .detail(store::human(error))
        .danger()
        .action(t("retry_now"), retry)
}

/// A library with nothing in it: the button is whatever would put something there.
///
/// A source nobody is signed into needs an account; one that scans folders needs a folder; one
/// that syncs needs a pull. Each is a different sentence and a different button.
pub fn empty_library(cx: &App) -> State {
    let sources = SourcesModel::global(cx);
    let sources = sources.read(cx);
    let capabilities = sources.capabilities();
    let source = sources.active().cloned();
    let anonymous = source.as_ref().is_some_and(|source| source.anonymous);
    let signed_out = source
        .as_ref()
        .is_some_and(|source| !source.authenticated && !source.anonymous);

    if signed_out || anonymous {
        let Some(source) = source else {
            return State::new(icons::LIBRARY, t("no_tracks_found"));
        };
        return State::new(SIGN_IN, t("library_empty_signed_out"))
            .detail(SharedString::from(source.name.clone()))
            .action(t("library_sign_in"), move |window, cx| {
                store::start_sign_in(&source, window, cx)
            });
    }
    if capabilities.scan_folders {
        return State::new(icons::FOLDER, t("library_empty_no_folder"))
            .detail(t("library_empty_no_folder_detail"))
            .action(t("library_add_folder"), |_, cx| {
                Navigator::navigate(
                    Route::Settings {
                        section: Some("library".into()),
                    },
                    cx,
                )
            });
    }
    if capabilities.sync {
        return State::new(icons::REFRESH, t("library_empty_not_synced"))
            .detail(t("library_empty_not_synced_detail"))
            .action(t("refresh_music_library"), |_, cx| {
                store::JobsModel::global(cx)
                    .update(cx, |jobs, cx| jobs.start(api::JobKind::LibrarySync, cx));
            });
    }
    State::new(icons::LIBRARY, t("no_tracks_found"))
}

/// Everything is there, the chips simply exclude it: the button takes the chips off.
pub fn no_matches(clear: impl Fn(&mut Window, &mut App) + 'static) -> State {
    State::new(icons::SEARCH, t("library_no_matches"))
        .detail(t("library_no_matches_detail"))
        .action(t("clear"), clear)
}
