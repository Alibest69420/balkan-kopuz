//! The navigation bar: history, the destination tabs, the palette field and the right-hand cluster.

mod bounds;
mod settings;
mod switcher;
mod tabs;

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, FocusHandle, Hsla,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    NavigationDirection, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, px,
};
use store::layout::NavPosition;
use store::{Chrome, LayoutModel, Navigator, Route, SourcesModel};
use widgets::focus::{Axis, Roving, Step};
use widgets::format::t;
use widgets::{icons, motion, style};

pub use bounds::{remember as remember_bounds, saved_bounds};
pub use settings::register as register_settings;
pub use switcher::{SourceProbe, SourceSwitcher, Status};

/// How tall the bar is across the top; the rail is this wide per row instead.
pub const BAR_HEIGHT: Pixels = px(44.);
/// The left rail's width, when the bar is stood on its side.
pub const RAIL_WIDTH: Pixels = px(200.);
/// Below this the tab labels go, leaving the glyphs.
pub const COMPACT_BELOW: Pixels = px(700.);
/// The width past which a table stops stretching and a grid starts centring.
pub const CONTENT_MAX: Pixels = px(1400.);

const GLYPH: Pixels = px(16.);
const CHROME_GLYPH: Pixels = px(15.);
const CHROME_BUTTON: Pixels = px(30.);
const SEARCH_MAX: Pixels = px(440.);
const SEARCH_HEIGHT: Pixels = px(30.);
const UNDERLINE: Pixels = px(2.);
const RAIL_MARKER: Pixels = px(3.);
// `bg-black/40` and `border-white/5`: the chrome's own scrim, as the rail wore it.
const SCRIM: f32 = 0.40;

/// The frame a page draws inside, so a table stops stretching and a grid centres.
pub fn content_frame() -> gpui_kit::Div {
    div().w_full().max_w(CONTENT_MAX).mx_auto()
}

/// Mouse buttons 4 and 5, for the root of whatever window wants history on them.
pub fn mouse_history(root: gpui_kit::Div) -> gpui_kit::Div {
    root.on_mouse_down(
        MouseButton::Navigate(NavigationDirection::Back),
        |_, _, cx| {
            Navigator::global(cx).update(cx, |navigator, cx| navigator.go_back(cx));
        },
    )
    .on_mouse_down(
        MouseButton::Navigate(NavigationDirection::Forward),
        |_: &MouseDownEvent, _, cx| {
            Navigator::global(cx).update(cx, |navigator, cx| navigator.go_forward(cx));
        },
    )
}

/// The bar. Same entity either way up: `layout.nav.position` picks the axis.
pub struct TopBar {
    destinations: Vec<&'static str>,
    position: NavPosition,
    activity: Option<AnyView>,
    probe: gpui_kit::Entity<SourceProbe>,
    tabs: Roving,
    search_focus: FocusHandle,
    hovered: Option<&'static str>,
    compact: bool,
    _subscriptions: Vec<Subscription>,
}

impl TopBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let probe = cx.new(SourceProbe::new);
        let layout = LayoutModel::global(cx);
        let subscriptions = vec![
            // Derived here rather than in render: the bar redraws on every notify in the app.
            cx.observe(&layout, |this: &mut Self, _, cx| {
                this.reread(cx);
                cx.notify();
            }),
            cx.observe(&Navigator::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&SourcesModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&probe, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |this, window, cx| {
                bounds::remember(window, cx);
                let compact = window.viewport_size().width < COMPACT_BELOW;
                if compact != this.compact {
                    this.compact = compact;
                    cx.notify();
                }
            }),
        ];
        let mut this = Self {
            destinations: Vec::new(),
            position: NavPosition::Top,
            activity: None,
            probe,
            tabs: Roving::new(Axis::Row, cx),
            search_focus: widgets::focus::stop_handle(cx),
            hovered: None,
            compact: window.viewport_size().width < COMPACT_BELOW,
            _subscriptions: subscriptions,
        };
        this.reread(cx);
        this
    }

    /// Put a view in the slot left of the source switcher; `None` empties it again.
    pub fn set_activity(&mut self, view: Option<AnyView>, cx: &mut Context<Self>) {
        self.activity = view;
        cx.notify();
    }

    fn reread(&mut self, cx: &App) {
        let nav = &store::layout(cx).nav;
        self.destinations = nav
            .visible()
            .into_iter()
            .filter_map(|name| {
                store::layout::DESTINATIONS
                    .iter()
                    .copied()
                    .find(|d| *d == name)
            })
            .collect();
        self.position = nav.position;
        self.tabs.set_len(self.destinations.len());
    }

    fn open(&self, index: usize, cx: &mut App) {
        let Some(name) = self.destinations.get(index) else {
            return;
        };
        if let Some(route) = Route::destination(name) {
            Navigator::navigate(route, cx);
        }
    }

    /// Left and right walk the strip; enter opens what the walk landed on.
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match self.tabs.walk(event) {
            Step::Moved(_) => cx.notify(),
            Step::Activate(at) => self.open(at, cx),
            Step::Ignored => cx.propagate(),
        }
    }

    fn ink(&self, active: bool, hovered: bool, cx: &App) -> Hsla {
        if active {
            cx.theme().foreground
        } else if hovered {
            style::white(style::TEXT_STRONG, cx)
        } else {
            style::slate_400(cx)
        }
    }

    fn tab(
        &self,
        index: usize,
        name: &'static str,
        active: bool,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let vertical = self.position == NavPosition::Left;
        let hovered = self.hovered == Some(name);
        let ink = motion::color_over(
            ElementId::from((ElementId::from("nav-tab-ink"), name)),
            self.ink(active, hovered, cx),
            motion::FAST,
            window,
            cx,
        );
        let wash = motion::color_over(
            ElementId::from((ElementId::from("nav-tab-wash"), name)),
            match (active, hovered) {
                (true, _) => style::white(style::RAISED, cx),
                (false, true) => style::white(style::RAISED, cx),
                (false, false) => style::white(0., cx),
            },
            motion::FAST,
            window,
            cx,
        );
        let label = t(tabs::label(name));
        let tooltip = commands::tooltip(tabs::command(name), label.clone(), cx);
        h_flex()
            .id(ElementId::from((ElementId::from("nav-tab"), name)))
            .relative()
            .gap_2()
            .px_3()
            .rounded(style::radius_lg())
            .cursor_pointer()
            .bg(wash)
            .text_color(ink)
            .map(|tab| match vertical {
                true => tab.w_full().py_2(),
                false => tab.h_full().py_1(),
            })
            .when(self.compact && !vertical, |tab| {
                tab.px_2()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            })
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                let held = over.then_some(name);
                if this.hovered != held && (*over || this.hovered == Some(name)) {
                    this.hovered = held;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tabs.set_cursor(index);
                this.open(index, cx);
            }))
            .child(
                Icon::default()
                    .path(tabs::icon(name))
                    .size(GLYPH)
                    .flex_shrink_0(),
            )
            .when(vertical || !self.compact, |tab| {
                tab.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_medium()
                        .child(label),
                )
            })
            .when(active, |tab| tab.child(marker(vertical, cx)))
            .when(focused, |tab| {
                tab.child(widgets::focus::ring(style::radius_lg(), cx))
            })
            .into_any_element()
    }

    fn strip(
        &self,
        current: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let vertical = self.position == NavPosition::Left;
        let tabs = self
            .destinations
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let active = current == Some(name);
                let ringed = self.tabs.is_active(index, window);
                self.tab(index, name, active, ringed, window, cx)
            })
            .collect::<Vec<_>>();
        let strip = match vertical {
            true => v_flex().w_full().gap_1(),
            false => h_flex().h_full().gap_1(),
        };
        self.tabs
            .group(strip.id("nav-strip"), "NavBar")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key(event, cx)))
            .children(tabs)
            .into_any_element()
    }

    /// The palette's doorway: it looks like a field, and clicking it opens the palette.
    fn search(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.search_focus.is_focused(window);
        widgets::focus::stop(h_flex().id("nav-search"), &self.search_focus, |_, cx| {
            Chrome::global(cx).update(cx, |chrome, cx| chrome.set_quick_search(true, cx));
        })
        .relative()
        .w_full()
        .max_w(SEARCH_MAX)
        .h(SEARCH_HEIGHT)
        .px_2()
        .gap_2()
        .rounded(style::radius_lg())
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .bg(style::white(style::RAISED, cx))
        .cursor_pointer()
        .hover(|field| field.bg(style::white(style::HOVER, cx)))
        .on_click(cx.listener(|_, _, _, cx| {
            Chrome::global(cx).update(cx, |chrome, cx| chrome.set_quick_search(true, cx));
        }))
        .child(
            Icon::default()
                .path(icons::SEARCH)
                .size(px(14.))
                .flex_shrink_0()
                .text_color(style::white(style::TEXT_MUTED, cx)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(style::white(style::TEXT_MUTED, cx))
                .child(t("search_placeholder")),
        )
        .child(hint(cx))
        .when(focused, |field| {
            field.child(widgets::focus::ring(style::radius_lg(), cx))
        })
        .into_any_element()
    }

    fn history(&self, cx: &mut Context<Self>) -> AnyElement {
        let navigator = Navigator::global(cx);
        let (back, forward) = {
            let navigator = navigator.read(cx);
            (navigator.can_go_back(), navigator.can_go_forward())
        };
        h_flex()
            .gap_0p5()
            .flex_shrink_0()
            .child(
                chrome_button("nav-back", icons::CHEVRON_LEFT, cx)
                    .disabled(!back)
                    .tooltip(commands::tooltip("nav.back", t("go_back"), cx))
                    .on_click(|_, _, cx| {
                        Navigator::global(cx).update(cx, |this, cx| this.go_back(cx));
                    }),
            )
            .child(
                chrome_button("nav-forward", icons::CHEVRON_RIGHT, cx)
                    .disabled(!forward)
                    .tooltip(commands::tooltip("nav.forward", t("go_forward"), cx))
                    .on_click(|_, _, cx| {
                        Navigator::global(cx).update(cx, |this, cx| this.go_forward(cx));
                    }),
            )
            .into_any_element()
    }

    fn cluster(&self, cx: &mut Context<Self>) -> AnyElement {
        let sources = SourcesModel::global(cx).read(cx).sources().to_vec();
        let status = self.probe.read(cx).status();
        let compact = self.compact && self.position == NavPosition::Top;
        h_flex()
            .gap_1()
            .flex_shrink_0()
            .items_center()
            .children(self.activity.clone())
            .child(
                div().when(!compact, |slot| slot.w(px(180.))).child(
                    SourceSwitcher::new(sources, status)
                        .collapsed(compact)
                        .bare(true),
                ),
            )
            .child(
                chrome_button("nav-settings", icons::SETTINGS, cx)
                    .tooltip(commands::tooltip("app.settings", t("settings"), cx))
                    .on_click(|_, _, cx| {
                        Navigator::navigate(Route::Settings { section: None }, cx);
                    }),
            )
            .into_any_element()
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("topbar");
        let current = Navigator::global(cx).read(cx).current().destination_of();
        let strip = self.strip(current, window, cx);
        let search = self.search(window, cx);
        let history = self.history(cx);
        let cluster = self.cluster(cx);

        match self.position {
            NavPosition::Top => h_flex()
                .w_full()
                .h(BAR_HEIGHT)
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .px_2()
                .bg(style::black(SCRIM, cx))
                .border_b_1()
                .border_color(style::white(style::BORDER_SOFT, cx))
                .child(
                    h_flex()
                        .h_full()
                        .items_center()
                        .gap_2()
                        .child(history)
                        .child(strip),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .justify_center()
                        .px_2()
                        .child(search),
                )
                .child(cluster)
                .into_any_element(),
            NavPosition::Left => v_flex()
                .h_full()
                .w(RAIL_WIDTH)
                .flex_shrink_0()
                .gap_2()
                .p_2()
                .bg(style::black(SCRIM, cx))
                .border_r_1()
                .border_color(style::white(style::BORDER_SOFT, cx))
                .child(history)
                .child(search)
                .child(strip)
                .child(div().flex_1().min_h_0())
                .child(v_flex().w_full().gap_1().child(cluster))
                .into_any_element(),
        }
    }
}

/// The accent under the active tab, or down its leading edge in the rail.
fn marker(vertical: bool, cx: &App) -> AnyElement {
    let bar = div().absolute().rounded_full().bg(cx.theme().primary);
    match vertical {
        true => bar.left_0().top_1().bottom_1().w(RAIL_MARKER),
        false => bar.left_2().right_2().bottom_0().h(UNDERLINE),
    }
    .into_any_element()
}

/// `ctrl-k`, said the way a keycap says it.
fn hint(cx: &App) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_1()
        .rounded(style::radius(cx))
        .bg(style::white(style::HOVER, cx))
        .text_size(style::text_10())
        .text_color(style::white(style::TEXT_MUTED, cx))
        .child(SharedString::new_static("Ctrl K"))
        .into_any_element()
}

/// The bar's own icon buttons: no fill of their own, a wash under the pointer.
fn chrome_button(id: &'static str, glyph: &'static str, cx: &App) -> Button {
    Button::new(id)
        .tab_index(0)
        .custom(style::quiet_button(style::slate_400(cx), cx))
        .with_size(CHROME_GLYPH * (4. / 3.))
        .size(CHROME_BUTTON)
        .rounded(CHROME_BUTTON / 4.)
        .icon(icons::icon(glyph))
}
