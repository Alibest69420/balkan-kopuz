//! The navigation rail (PARITY §3.3) and the source switcher above it (§3.4).

mod items;
mod switcher;

use api::{SourceKind, SourceState};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Task, Window, div, px,
};
use link::Link;
use store::{ConfigModel, Navigator, Route, SourcesModel};
use widgets::format::t;
use widgets::icons;

use switcher::{SourceSwitcher, Status};

// Measured geometry, not theme scale: the widths the Dioxus rail snaps between.
const DEFAULT_WIDTH: Pixels = px(240.);
const RAIL_WIDTH: Pixels = px(72.);
const COLLAPSE_BELOW: Pixels = px(150.);
const EXPAND_ABOVE: Pixels = px(180.);
const MAX_WIDTH: Pixels = px(450.);
const HANDLE_WIDTH: Pixels = px(4.);
const INDICATOR: Pixels = px(2.);
const INDICATOR_HEIGHT: Pixels = px(24.);
const ITEM_ICON: Pixels = px(18.);

// The reorder arrows only appear on the row the pointer is over.
const ITEM_GROUP: &str = "sidebar-item";

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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(route) = items::route(key) else {
            return div().into_any_element();
        };
        let active = *current == route;
        let collapsed = self.collapsed;
        let label = t(key);
        let tooltip = label.clone();
        let position = order.iter().position(|item| item == key);
        let can_move_up = position.is_some_and(|index| index > 0);
        let can_move_down = position.is_some_and(|index| index + 1 < order.len());

        h_flex()
            .id(SharedString::from(format!("nav-{key}")))
            .group(ITEM_GROUP)
            .w_full()
            .rounded(cx.theme().radius)
            .when(active, |row| {
                row.bg(cx.theme().sidebar_accent)
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
            .when(!active, |row| {
                row.hover(|row| row.bg(cx.theme().sidebar_accent.opacity(0.5)))
            })
            .child(
                div()
                    .w(INDICATOR)
                    .h(INDICATOR_HEIGHT)
                    .flex_shrink_0()
                    .rounded_r(INDICATOR)
                    .when(active, |bar| bar.bg(cx.theme().sidebar_primary)),
            )
            .child(
                h_flex()
                    .id("link")
                    .flex_1()
                    .min_w_0()
                    .gap_3()
                    .p_2()
                    .when(collapsed, |link| {
                        link.justify_center().tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .child(Icon::default().path(items::icon(key)).size(ITEM_ICON))
                    .when(!collapsed, |link| {
                        link.child(div().flex_1().min_w_0().truncate().text_sm().child(label))
                    })
                    .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx)),
            )
            .when(!collapsed && (can_move_up || can_move_down), |row| {
                row.child(
                    v_flex()
                        .pr_1()
                        .opacity(0.)
                        .group_hover(ITEM_GROUP, |arrows| arrows.opacity(1.))
                        .child(arrow(key, -1, "move_up", icons::CHEVRON_UP, can_move_up))
                        .child(arrow(
                            key,
                            1,
                            "move_down",
                            icons::CHEVRON_DOWN,
                            can_move_down,
                        )),
                )
            })
            .into_any_element()
    }
}

fn arrow(
    key: &'static str,
    delta: isize,
    label: &'static str,
    icon: &'static str,
    enabled: bool,
) -> impl IntoElement {
    Button::new(SharedString::from(format!("nav-{key}-{label}")))
        .ghost()
        .xsmall()
        .icon(Icon::default().path(icon))
        .tooltip(t(label))
        .disabled(!enabled)
        .on_click(move |_, _, cx| Sidebar::reorder(key, delta, cx))
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let width = if collapsed { RAIL_WIDTH } else { self.width };
        let keys = items::ordered(&order, discover);
        let rows = keys
            .into_iter()
            .map(|key| self.item(key, &current, &order, cx))
            .collect::<Vec<_>>();
        let settings = self.item(items::SETTINGS, &current, &[], cx);

        div()
            .id("sidebar")
            .relative()
            .h_full()
            .w(width)
            .flex_shrink_0()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
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
                            .p_2()
                            .gap_1()
                            .children(rows),
                    )
                    .child(div().h(px(1.)).mx_3().bg(cx.theme().sidebar_border))
                    .child(div().p_2().child(settings)),
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
                    .hover(|handle| handle.bg(cx.theme().ring.opacity(0.4)))
                    .on_drag(ResizeDrag, |_, _, _, cx| cx.new(|_| ResizeGhost)),
            )
    }
}
