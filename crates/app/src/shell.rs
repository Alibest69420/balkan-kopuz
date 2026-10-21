//! The window: the top bar, the routed page, the panels, the player bar, and everything over them.

use gpui_kit::component::{ActiveTheme as _, Root, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled,
    Subscription, Task, Window, div,
};
use gpui_kit::{FocusHandle, InteractiveElement as _, actions};
use link::Connection;
use store::{Chrome, ConfigModel, Navigator, PlayerModel, SourcesModel};

use crate::router::Router;

/// The gap a toast keeps above whatever runs along the window's bottom edge.
const TOAST_GAP: gpui_kit::Pixels = gpui_kit::px(16.);

/// Toasts rise from the bottom edge, so they clear the player bar only while it sits there.
fn place_toasts(cx: &mut gpui_kit::App) {
    let at_top = ConfigModel::global(cx)
        .read(cx)
        .config()
        .ui
        .player_bar_position
        == config::PlayerBarPosition::Top;
    let clearance = match at_top {
        true => TOAST_GAP,
        false => now_playing::bar_height(cx) + TOAST_GAP,
    };
    theme::place_toasts(clearance, cx);
}

type AppliedTheme = (String, Option<config::CustomTheme>);

/// Render what the daemon's settings say, whichever layer set them; `applied` keeps a repaint from reapplying the theme.
fn follow_config(applied: &mut AppliedTheme, window: &mut Window, cx: &mut gpui_kit::App) {
    let settings = ConfigModel::global(cx).read(cx).config().clone();
    // Every transition in the app asks GPUI whether motion is reduced.
    let reduced = settings.ui.reduce_animations;
    if reduced != cx.reduce_motion() {
        cx.set_reduce_motion(reduced);
    }
    // Re-editing a custom theme keeps its name, so the colors are the key.
    let wanted = (
        settings.ui.theme.clone(),
        settings.ui.custom_themes.get(&settings.ui.theme).cloned(),
    );
    if wanted != *applied {
        *applied = wanted;
        theme::apply_config(&settings.ui, Some(window), cx);
    }
    place_toasts(cx);
}

actions!(
    kopuz,
    [
        TogglePlayback,
        QuickSearch,
        Escape,
        Back,
        Forward,
        TogglePlaylistsPanel
    ]
);

pub struct Shell {
    router: Router,
    focus: FocusHandle,
    connection: Entity<Connection>,
    topbar: Entity<topbar::TopBar>,
    playlists_panel: Entity<playlists_panel::PlaylistsPanel>,
    player_bar: Entity<now_playing::NowPlaying>,
    side_panel: Entity<now_playing::NowPlaying>,
    fullscreen: Entity<now_playing::NowPlaying>,
    quick_search: Entity<palette::Palette>,
    mini_player: Entity<now_playing::NowPlaying>,
    overlays: Entity<overlays::Overlays>,
    first_run: Entity<onboarding::FirstRun>,
    // Up until the daemon first answers and setup's state is read; never again after.
    loader: Option<Entity<status::Loader>>,
    debug_overlay: Entity<debug_overlay::DebugOverlay>,
    // The theme these were applied for, so config edits reapply only on a change.
    applied_theme: AppliedTheme,
    // On its own timer: GPUI only renders on a change, and an idle app is when covers want releasing.
    _heartbeat: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    /// Reports the heap and releases the covers no longer drawn, whether or not anything repainted.
    fn heartbeat(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            let mut heartbeat = profile::Heartbeat::new();
            loop {
                cx.background_executor().timer(profile::HEARTBEAT).await;
                let swept = this.update_in(cx, |_, window, cx| {
                    heartbeat.report();
                    store::Residents::sweep(window, cx)
                });
                if swept.is_err() {
                    return;
                }
            }
        })
    }

    pub fn new(started: std::time::Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let connection = link::Link::connection(cx);
        let navigator = Navigator::global(cx);
        let chrome = Chrome::global(cx);
        let config = ConfigModel::global(cx);
        let sources = SourcesModel::global(cx);
        let first_run = cx.new(|cx| onboarding::FirstRun::new(window, cx));
        let booted = connection.read(cx).is_connected()
            && store::SetupModel::global(cx).read(cx).is_decided();

        let subscriptions = vec![
            // Unmounted, the guide's own notify has no ancestor to dirty, so it would never appear.
            cx.observe(&first_run, |_, _, cx| cx.notify()),
            cx.observe(&store::SetupModel::global(cx), |this, _, cx| this.boot(cx)),
            cx.observe(&connection, |this, _, cx| this.boot(cx)),
            cx.observe(&navigator, |_, _, cx| cx.notify()),
            cx.observe(&chrome, |_, _, cx| cx.notify()),
            cx.observe_in(&config, window, |this, _, window, cx| {
                follow_config(&mut this.applied_theme, window, cx);
                cx.notify();
            }),
            // The skin sizes the player bar, which the toasts have to clear.
            cx.observe(&store::LayoutModel::global(cx), |_, _, cx| place_toasts(cx)),
            // A source switch makes every page's contents wrong, so none are kept.
            cx.observe(&sources, |this, _, cx| {
                this.router.clear();
                cx.notify();
            }),
        ];

        // The window opens after the config has usually loaded, so the observer above may never fire for it.
        let mut applied_theme = AppliedTheme::default();
        follow_config(&mut applied_theme, window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // Overlays owns the notices channel now, so the bar only has to hold the cluster.
        let topbar = cx.new(|cx| topbar::TopBar::new(window, cx));
        let cluster = cx.new(|cx| status::StatusCluster::new(window, cx));
        topbar.update(cx, |bar, cx| bar.set_activity(Some(cluster.into()), cx));
        Self {
            router: Router::default(),
            focus,
            connection,
            topbar,
            playlists_panel: cx.new(|cx| playlists_panel::PlaylistsPanel::new(window, cx)),
            player_bar: cx.new(|cx| now_playing::NowPlaying::bar(window, cx)),
            side_panel: cx.new(|cx| now_playing::NowPlaying::panel(window, cx)),
            fullscreen: cx.new(|cx| now_playing::NowPlaying::sheet(window, cx)),
            quick_search: cx.new(|cx| palette::Palette::new(window, cx)),
            mini_player: cx.new(|cx| now_playing::NowPlaying::mini(window, cx)),
            overlays: cx.new(|cx| overlays::Overlays::new(window, cx)),
            first_run,
            // The window often opens after both answers are in, when no observer would fire again.
            loader: (!booted).then(|| cx.new(|cx| status::Loader::new(started, cx))),
            debug_overlay: cx.new(|cx| debug_overlay::DebugOverlay::new(window, cx)),
            applied_theme,
            _heartbeat: Self::heartbeat(window, cx),
            _subscriptions: subscriptions,
        }
    }

    /// The first answer is in: the loader goes for good, and a later disconnect is the banner's.
    fn boot(&mut self, cx: &mut Context<Self>) {
        let connected = self.connection.read(cx).is_connected();
        let decided = store::SetupModel::global(cx).read(cx).is_decided();
        if self.loader.is_some() && connected && decided {
            self.loader = None;
            status::release_logo(cx);
        }
        cx.notify();
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

    /// The tail of one chain: the palette, a menu, a dialog and a selection all stop Escape first.
    pub fn escape(&mut self, cx: &mut Context<Self>) {
        // The mini window is the innermost thing the Shell can see, and it is a window, not a flag.
        if now_playing::mini_is_open(cx) {
            now_playing::close_mini(cx);
            return;
        }
        Chrome::global(cx).update(cx, |chrome, cx| {
            if chrome.is_quick_search_open() {
                chrome.set_quick_search(false, cx);
            } else if chrome.is_mini() {
                chrome.set_mini(false, cx);
            } else if chrome.is_sheet_open() {
                chrome.set_sheet(false, cx);
            } else if chrome.panel().is_some() {
                chrome.set_panel(None, cx);
            }
        });
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx));
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _rebuild = profile::Rebuild::new();
        if let Some(loader) = self.loader.clone() {
            return div()
                .size_full()
                .child(loader)
                .children(Root::render_notification_layer(window, cx))
                .into_any_element();
        }
        let connected = self.connection.read(cx).is_connected();
        let chrome = Chrome::global(cx).read(cx);
        // The sheet is a full-window overlay, not a body state: its hero cover flies out of the
        // bar's own cover slot, so the bar has to still be there underneath it.
        let sheet_open = chrome.is_sheet_open();
        // A disconnect outranks compact: a mini player with nothing behind it would keep showing
        // the last track it saw, with no way to learn the daemon had gone.
        let compact = chrome.is_compact() && connected;
        let first_run = store::SetupModel::global(cx).read(cx).showing() && connected;
        let nav_left = store::layout(cx).nav.position == store::layout::NavPosition::Left;
        let bar = Some(self.topbar.clone());
        let playlists = self.playlists_panel.read(cx);
        let playlists_open = playlists.is_open(cx) && connected;
        let playlists_width = playlists.width();
        let panel_open = chrome.panel().is_some();
        let quick_search_open = chrome.is_quick_search_open();
        // The docked column is drag-resizable, so it owns the slot's width.
        let panel_width = self.side_panel.read(cx).panel_width();
        let show_player_bar = connected && !compact;
        let player_at_top = ConfigModel::global(cx)
            .read(cx)
            .config()
            .ui
            .player_bar_position
            == config::PlayerBarPosition::Top;
        let route = Navigator::global(cx).read(cx).current().clone();
        let page = self.router.page(&route, window, cx);

        // Compact swaps the body and nothing else. It used to return early, above the key context
        // and the dialog, sheet and notification layers, so the mini player had no shortcuts, no
        // toasts and no dialogs — and, being above the `connected` check, went on showing the last
        // track forever when the daemon went away.
        let body = if compact {
            self.mini_player.clone().into_any_element()
        } else {
            v_flex()
                .size_full()
                .w_full()
                .children(bar.clone().filter(|_| !nav_left))
                .when(show_player_bar && player_at_top, |body| {
                    body.child(self.player_bar.clone())
                })
                .child(
                    h_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .items_stretch()
                        .children(bar.filter(|_| nav_left))
                        .when(playlists_open, |body| {
                            body.child(
                                div()
                                    .w(playlists_width)
                                    .flex_shrink_0()
                                    .child(self.playlists_panel.clone()),
                            )
                        })
                        .child(
                            div()
                                .h_full()
                                .flex_1()
                                .min_w_0()
                                .relative()
                                // A disconnect dims the page and banners over it rather than
                                // replacing the window, so what was on screen stays on screen.
                                .map(|content| match first_run {
                                    true => content.child(self.first_run.clone()),
                                    false => {
                                        content.child(topbar::content_frame().h_full().child(page))
                                    }
                                })
                                .child(self.overlays.clone())
                                .child(self.debug_overlay.clone()),
                        )
                        .when(panel_open && connected, |body| {
                            body.child(div().w(panel_width).child(self.side_panel.clone()))
                        }),
                )
                .when(show_player_bar && !player_at_top, |window_body| {
                    window_body.child(self.player_bar.clone())
                })
                .into_any_element()
        };

        components::focus::watch(topbar::mouse_history(div()))
            .size_full()
            .track_focus(&self.focus)
            .key_context("Shell")
            .on_action(cx.listener(|this, _: &TogglePlayback, window, cx| {
                // Space belongs to whatever has focus. A binding is dispatched before the key
                // listeners that turn space into a click, so without this a focused button, switch
                // or checkbox would toggle playback instead of activating. Declining the action
                // lets the keystroke carry on to those listeners.
                if window
                    .focused(cx)
                    .is_some_and(|focused| focused != this.focus)
                {
                    cx.propagate();
                    return;
                }
                this.toggle_playback(cx);
            }))
            .on_action(cx.listener(|this, _: &QuickSearch, _, cx| this.toggle_quick_search(cx)))
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.escape(cx)))
            .on_action(cx.listener(|this, _: &Back, _, cx| this.go_back(cx)))
            .on_action(cx.listener(|_, _: &Forward, _, cx| {
                Navigator::global(cx).update(cx, |navigator, cx| navigator.go_forward(cx));
            }))
            .on_action(cx.listener(|_, _: &TogglePlaylistsPanel, _, cx| {
                playlists_panel::toggle(cx);
            }))
            .on_action(
                cx.listener(|this, _: &debug_overlay::ToggleDebugOverlay, _, cx| {
                    this.debug_overlay
                        .update(cx, |overlay, cx| overlay.toggle(cx));
                }),
            )
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(match compact {
                true => None,
                false => crate::backdrop::layer(window, cx),
            })
            .child(body)
            .when(sheet_open && connected && !compact, |shell| {
                shell.child(div().absolute().inset_0().child(self.fullscreen.clone()))
            })
            .when(quick_search_open && !compact, |shell| {
                shell.child(self.quick_search.clone())
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .into_any_element()
    }
}
