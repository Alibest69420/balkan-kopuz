//! The window: sidebar, the routed page, the side panel, the player bar, and everything drawn over them.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Root, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled,
    Subscription, Window, div, px,
};
use gpui_kit::{FocusHandle, InteractiveElement as _, actions};
use link::{Connection, ConnectionStatus};
use store::{Chrome, ConfigModel, Navigator, Notice, Notices, PlayerModel, SourcesModel};

use crate::router::Router;

actions!(kopuz, [TogglePlayback, QuickSearch, Escape, Back]);

pub struct Shell {
    router: Router,
    focus: FocusHandle,
    connection: Entity<Connection>,
    sidebar: Entity<sidebar::Sidebar>,
    player_bar: Entity<player_bar::PlayerBar>,
    side_panel: Entity<side_panel::SidePanelView>,
    fullscreen: Entity<fullscreen::FullscreenPlayer>,
    quick_search: Entity<quick_search::QuickSearch>,
    overlays: Entity<overlays::Overlays>,
    // The theme these were applied for, so config edits reapply only on a change.
    theme_name: String,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let connection = link::Link::connection(cx);
        let navigator = Navigator::global(cx);
        let chrome = Chrome::global(cx);
        let config = ConfigModel::global(cx);
        let sources = SourcesModel::global(cx);
        let notices = Notices::global(cx);

        let subscriptions = vec![
            cx.observe(&connection, |_, _, cx| cx.notify()),
            cx.observe(&navigator, |_, _, cx| cx.notify()),
            cx.observe(&chrome, |_, _, cx| cx.notify()),
            cx.observe_in(&config, window, |this, config, window, cx| {
                let theme = config.read(cx).config().theme.clone();
                if theme != this.theme_name {
                    this.theme_name = theme.clone();
                    theme::apply_named(&theme, Some(window), cx);
                }
                cx.notify();
            }),
            // A source switch makes every page's contents wrong, so none are kept.
            cx.observe(&sources, |this, _, cx| {
                this.router.clear();
                cx.notify();
            }),
            cx.subscribe_in(&notices, window, |_, _, notice: &Notice, window, cx| {
                let toast = match notice.level {
                    api::NoticeLevel::Error => Notification::error(notice.message.clone()),
                    api::NoticeLevel::Warning => Notification::warning(notice.message.clone()),
                    _ => Notification::info(notice.message.clone()),
                };
                window.push_notification(toast, cx);
            }),
        ];

        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            router: Router::default(),
            focus,
            connection,
            sidebar: cx.new(|cx| sidebar::Sidebar::new(window, cx)),
            player_bar: cx.new(|cx| player_bar::PlayerBar::new(window, cx)),
            side_panel: cx.new(|cx| side_panel::SidePanelView::new(window, cx)),
            fullscreen: cx.new(|cx| fullscreen::FullscreenPlayer::new(window, cx)),
            quick_search: cx.new(|cx| quick_search::QuickSearch::new(window, cx)),
            overlays: cx.new(|cx| overlays::Overlays::new(window, cx)),
            theme_name: String::new(),
            _subscriptions: subscriptions,
        }
    }

    pub fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
    }

    pub fn toggle_quick_search(&mut self, cx: &mut Context<Self>) {
        Chrome::global(cx).update(cx, |chrome, cx| {
            let open = chrome.is_quick_search_open();
            chrome.set_quick_search(!open, cx);
        });
    }

    pub fn escape(&mut self, cx: &mut Context<Self>) {
        Chrome::global(cx).update(cx, |chrome, cx| {
            if chrome.is_quick_search_open() {
                chrome.set_quick_search(false, cx);
            } else if chrome.is_fullscreen() {
                chrome.set_fullscreen(false, cx);
            }
        });
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx));
    }

    /// While the daemon is unreachable there is nothing to show, so say so and offer to start it.
    fn connection_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let message = match self.connection.read(cx).status() {
            ConnectionStatus::Unreachable(error) => error.to_string(),
            _ => i18n::t("connecting"),
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_lg().child(i18n::t("daemon_unreachable")))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            )
            .child(
                Button::new("start-daemon")
                    .primary()
                    .label(i18n::t("start_daemon"))
                    .on_click(|_, _, cx| {
                        let socket = match link::Link::global(cx).address() {
                            link::Address::Socket(path) => Some(path.clone()),
                            link::Address::Tcp(_) => None,
                        };
                        if let Err(error) = link::spawn_daemon(socket) {
                            tracing::error!(%error, "could not start the daemon");
                        }
                    }),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.connection.read(cx).is_connected();
        let chrome = Chrome::global(cx).read(cx);
        let fullscreen = chrome.is_fullscreen();
        let panel_open = chrome.panel().is_some();
        let quick_search_open = chrome.is_quick_search_open();
        let route = Navigator::global(cx).read(cx).current().clone();
        let page = self.router.page(&route, window, cx);

        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("Shell")
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| this.toggle_playback(cx)))
            .on_action(cx.listener(|this, _: &QuickSearch, _, cx| this.toggle_quick_search(cx)))
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.escape(cx)))
            .on_action(cx.listener(|this, _: &Back, _, cx| this.go_back(cx)))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h_0()
                            .items_stretch()
                            .when(!fullscreen, |body| body.child(self.sidebar.clone()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .relative()
                                    .map(|content| {
                                        if !connected {
                                            content.child(self.connection_screen(cx))
                                        } else if fullscreen {
                                            content.child(self.fullscreen.clone())
                                        } else {
                                            content.child(page)
                                        }
                                    })
                                    .child(self.overlays.clone()),
                            )
                            .when(panel_open && connected, |body| {
                                body.child(div().w(px(320.)).child(self.side_panel.clone()))
                            }),
                    )
                    .when(connected, |window_body| {
                        window_body.child(self.player_bar.clone())
                    }),
            )
            .when(quick_search_open, |shell| {
                shell.child(self.quick_search.clone())
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
