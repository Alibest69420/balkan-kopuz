//! The navigation rail (PARITY §3.3) and the source switcher above it (§3.4).

mod items;
mod switcher;

use std::time::{Duration, Instant};

use api::{SourceKind, SourceState};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, ElementId, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, px,
};
use link::Link;
use store::{ConfigModel, Navigator, Route, SourcesModel};
use widgets::format::t;
use widgets::{icons, motion, style};

use switcher::{SourceSwitcher, Status};

// Measured geometry, not theme scale: the widths the Dioxus rail snaps between.
const DEFAULT_WIDTH: Pixels = px(240.);
const RAIL_WIDTH: Pixels = px(72.);
const COLLAPSE_BELOW: Pixels = px(150.);
const EXPAND_ABOVE: Pixels = px(180.);
const MAX_WIDTH: Pixels = px(450.);
const HANDLE_WIDTH: Pixels = px(8.);
const HANDLE_LINE: Pixels = px(1.);
const INDICATOR: Pixels = px(2.);
const INDICATOR_HEIGHT: Pixels = px(24.);
const INDICATOR_HOVER: Pixels = px(16.);
const ITEM_ICON: Pixels = px(18.);
const ARROW_ICON: Pixels = px(9.);
const DIVIDER: Pixels = px(1.);

// `opacity-70` on an inactive label, full on the row the pointer is over.
const LABEL_IDLE: f32 = 0.70;
// `bg-white/40`: the sliver of an indicator an inactive row grows on hover.
const INDICATOR_IDLE: f32 = 0.40;

/// A motion channel for one nav row, derived from the item's key rather than its place in the list.
fn channel(name: &'static str, key: &'static str) -> ElementId {
    ElementId::from((ElementId::from(name), key))
}

// TODO(ui-style): the Vaxry variant — grouped "discover"/"library" sections, no reorder, 200 px default.

struct ResizeDrag;

struct ResizeGhost;

impl Render for ResizeGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The navigation rail. It owns its own width, including the drag-to-resize and collapse behaviour.
pub struct Sidebar {
    width: Pixels,
    collapsed: bool,
    // The row the pointer is over, so its surface, label and indicator can tween instead of snapping.
    hovered: Option<&'static str>,
    handle_hovered: bool,
    // A drag step is the pointer's own motion; only a collapse crossing is worth tweening.
    snap_until: Option<Instant>,
    status: Status,
    probed: Option<String>,
    _probe: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = window;
        let sources = SourcesModel::global(cx);
        let subscriptions = vec![
            cx.observe(&sources, |this, sources, cx| {
                this.probe(&sources, cx);
                cx.notify();
            }),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&Navigator::global(cx), |_, _, cx| cx.notify()),
        ];
        let mut this = Self {
            width: DEFAULT_WIDTH,
            collapsed: false,
            hovered: None,
            handle_hovered: false,
            snap_until: None,
            status: Status::Connecting,
            probed: None,
            _probe: None,
            _subscriptions: subscriptions,
        };
        this.probe(&sources, cx);
        this
    }

    /// A server is asked whether it is still reachable; a local source always is.
    fn probe(&mut self, sources: &Entity<SourcesModel>, cx: &mut Context<Self>) {
        let Some(active) = sources.read(cx).active().cloned() else {
            self.probed = None;
            self.status = Status::Connecting;
            return;
        };
        if self.probed.as_deref() == Some(active.id.as_str()) {
            return;
        }
        self.probed = Some(active.id.clone());
        if active.kind != SourceKind::Server {
            self.status = Status::Online;
            self._probe = None;
            return;
        }
        self.status = Status::Connecting;
        let call =
            Link::global(cx).call(move |api| async move { api.validate_source(active.id).await });
        self._probe = Some(cx.spawn(async move |this, cx| {
            let state = call.await;
            this.update(cx, |this, cx| {
                this.status = match state {
                    Ok(SourceState::Online) => Status::Online,
                    _ => Status::Offline,
                };
                cx.notify();
            })
            .ok();
        }));
    }

    fn resize(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let was = self.collapsed;
        if self.collapsed {
            if width > EXPAND_ABOVE {
                self.collapsed = false;
                self.width = width.min(MAX_WIDTH);
            }
        } else if width < COLLAPSE_BELOW {
            self.collapsed = true;
        } else if width < MAX_WIDTH {
            self.width = width;
        }
        if self.collapsed != was {
            self.snap_until = Some(Instant::now() + motion::MEDIUM);
        }
        cx.notify();
    }

    fn reorder(key: &str, delta: isize, cx: &mut App) {
        ConfigModel::global(cx).update(cx, |model, cx| {
            model.update(cx, |config| {
                let Some(from) = config.sidebar_order.iter().position(|item| item == key) else {
                    return;
                };
                let Some(to) = from.checked_add_signed(delta) else {
                    return;
                };
                if to < config.sidebar_order.len() {
                    config.sidebar_order.swap(from, to);
                }
            });
        });
    }

    fn item(
        &self,
        key: &'static str,
        current: &Route,
        order: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(route) = items::route(key) else {
            return div().into_any_element();
        };
        let active = *current == route;
        let collapsed = self.collapsed;
        let hovered = self.hovered == Some(key);
        let label = t(key);
        let tooltip = label.clone();
        let position = order.iter().position(|item| item == key);
        let can_move_up = position.is_some_and(|index| index > 0);
        let can_move_down = position.is_some_and(|index| index + 1 < order.len());

        let surface = if active {
            style::white(style::HOVER, cx)
        } else if hovered {
            style::white(style::RAISED, cx)
        } else {
            style::white(0., cx)
        };
        let ink = if active {
            cx.theme().foreground
        } else if hovered {
            style::white(style::TEXT_STRONG, cx)
        } else {
            style::slate_400(cx)
        };
        let bar = if active {
            INDICATOR_HEIGHT
        } else if hovered {
            INDICATOR_HOVER
        } else {
            px(0.)
        };
        let bar_ink = if active {
            cx.theme().foreground
        } else {
            style::white(INDICATOR_IDLE, cx)
        };
        let surface =
            motion::color_over(channel("nav-bg", key), surface, motion::MEDIUM, window, cx);
        let ink = motion::color_over(channel("nav-ink", key), ink, motion::MEDIUM, window, cx);
        let bar = motion::length(channel("nav-indicator", key), bar, motion::SLOW, window, cx);
        let bar_ink = motion::color_over(
            channel("nav-indicator-ink", key),
            bar_ink,
            motion::SLOW,
            window,
            cx,
        );
        let label_alpha = motion::amount(
            channel("nav-label", key),
            if active || hovered { 1. } else { LABEL_IDLE },
            motion::FAST,
            window,
            cx,
        );
        let arrows_alpha = motion::amount(
            channel("nav-arrows", key),
            if hovered { 1. } else { 0. },
            motion::FAST,
            window,
            cx,
        );

        h_flex()
            .id(SharedString::from(format!("nav-{key}")))
            .w_full()
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                if *over {
                    if this.hovered != Some(key) {
                        this.hovered = Some(key);
                        cx.notify();
                    }
                } else if this.hovered == Some(key) {
                    this.hovered = None;
                    cx.notify();
                }
            }))
            .child(
                h_flex()
                    .id("link")
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .p_3()
                    .rounded(style::radius_lg())
                    .cursor_pointer()
                    .bg(surface)
                    .text_color(ink)
                    .when(collapsed, |link| {
                        link.justify_center().tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w_6()
                            .h_6()
                            .flex_shrink_0()
                            .child(Icon::default().path(items::icon(key)).size(ITEM_ICON)),
                    )
                    .when(!collapsed, |link| {
                        link.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .ml_4()
                                .truncate()
                                .text_sm()
                                .font_medium()
                                .opacity(label_alpha)
                                .child(label),
                        )
                    })
                    .child(indicator(bar, bar_ink))
                    .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx)),
            )
            .when(!collapsed && (can_move_up || can_move_down), |row| {
                row.child(
                    v_flex()
                        .pr_1()
                        .opacity(arrows_alpha)
                        .child(arrow(key, -1, "up", icons::CHEVRON_UP, can_move_up, cx))
                        .child(arrow(
                            key,
                            1,
                            "down",
                            icons::CHEVRON_DOWN,
                            can_move_down,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }
}

/// The bar down the leading edge: full height on the active row, a sliver on the hovered one.
fn indicator(height: Pixels, color: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .w(INDICATOR)
        .rounded_r_full()
        .h(height)
        .bg(color)
}

fn arrow(
    key: &'static str,
    delta: isize,
    label: &'static str,
    icon: &'static str,
    enabled: bool,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("nav-{key}-{label}")))
        .flex()
        .items_center()
        .justify_center()
        .px_1()
        .rounded_full()
        .map(|button| {
            if enabled {
                button
                    .text_color(style::slate_500(cx))
                    .cursor_pointer()
                    .hover(|button| {
                        button
                            .bg(style::white(style::HOVER, cx))
                            .text_color(cx.theme().foreground)
                    })
                    .on_click(move |_, _, cx| Sidebar::reorder(key, delta, cx))
            } else {
                button.text_color(style::white(style::TEXT_FAINT, cx))
            }
        })
        .child(Icon::default().path(icon).size(ARROW_ICON))
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (order, show_switcher) = {
            let config = ConfigModel::global(cx).read(cx);
            (
                config.config().sidebar_order.clone(),
                config.config().show_source_toggle,
            )
        };
        let (sources, discover) = {
            let model = SourcesModel::global(cx).read(cx);
            (model.sources().to_vec(), model.capabilities().discover)
        };
        let current = Navigator::global(cx).read(cx).current().root();
        let collapsed = self.collapsed;
        let snapping = self.snap_until.is_some_and(|at| at > Instant::now());
        let width = motion::length(
            "sidebar-width",
            if collapsed { RAIL_WIDTH } else { self.width },
            if snapping {
                motion::MEDIUM
            } else {
                Duration::ZERO
            },
            window,
            cx,
        );
        let handle = motion::color(
            "sidebar-handle",
            if self.handle_hovered {
                style::white(style::HOVER, cx)
            } else {
                style::white(0., cx)
            },
            window,
            cx,
        );
        let keys = items::ordered(&order, discover);
        let rows = keys
            .into_iter()
            .map(|key| self.item(key, &current, &order, window, cx))
            .collect::<Vec<_>>();
        let settings = self.item(items::SETTINGS, &current, &[], window, cx);

        div()
            .id("sidebar")
            .relative()
            .h_full()
            .w(width)
            .flex_shrink_0()
            .bg(style::black(0.40, cx))
            .text_color(style::slate_400(cx))
            .border_r_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizeDrag>, _, cx| {
                    this.resize(event.event.position.x - event.bounds.left(), cx);
                }),
            )
            .child(
                v_flex()
                    .size_full()
                    .overflow_hidden()
                    .when(show_switcher, |column| {
                        column.child(SourceSwitcher::new(sources, self.status).collapsed(collapsed))
                    })
                    .child(
                        v_flex()
                            .id("sidebar-items")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .pt_2()
                            .px_3()
                            .gap_1()
                            .children(rows)
                            .child(
                                div()
                                    .h(DIVIDER)
                                    .my_4()
                                    .mx_3()
                                    .bg(style::white(style::BORDER_SOFT, cx)),
                            )
                            .child(settings),
                    ),
            )
            .child(
                div()
                    .id("sidebar-resize")
                    .absolute()
                    .top_0()
                    .right_0()
                    .h_full()
                    .w(HANDLE_WIDTH)
                    .cursor_col_resize()
                    .on_drag(ResizeDrag, |_, _, _, cx| cx.new(|_| ResizeGhost))
                    .on_hover(cx.listener(|this, over: &bool, _, cx| {
                        if this.handle_hovered != *over {
                            this.handle_hovered = *over;
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_0()
                            .w(HANDLE_LINE)
                            .bg(handle),
                    ),
            )
    }
}
