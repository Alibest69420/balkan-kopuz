use std::time::Duration;

use api::{SourceInfo, SourceState};
use components::format::{t, text};
use components::source_mark::{self, SourceMark};
use components::{icons, style};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, IntoElement, ParentElement as _, Pixels,
    RenderOnce, SharedString, Styled, Window, div, pulsating_between, px,
};
use store::{Navigator, Route, SourcesModel};

const GLYPH: Pixels = px(16.);
const DOT: Pixels = px(6.);
const CHEVRON: Pixels = px(10.);
const CHECK: Pixels = px(11.);
const POPOVER_WIDTH: Pixels = px(218.);
const ROWS_HEIGHT: Pixels = px(320.);
const RAIL_TILE: Pixels = px(40.);
const SUBLINE_GAP: Pixels = px(2.);

// The switcher's own dimmed tones, which are off the named alpha scale.
const HEAD_COUNT: f32 = 0.38;
const ROW_NAME: f32 = 0.85;

const MANAGE_ICON: &str = "icons/sliders-horizontal.svg";

/// Whether the active source answered the daemon's probe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Online,
    Offline,
}

/// The active source's link, as the daemon's own probe last found it.
pub fn status(cx: &App) -> Status {
    let sources = SourcesModel::global(cx);
    let sources = sources.read(cx);
    match sources.active().and_then(|source| source.state) {
        Some(SourceState::Online) => Status::Online,
        Some(SourceState::AuthExpired | SourceState::Offline) => Status::Offline,
        Some(SourceState::Checking) | None => Status::Connecting,
    }
}

/// The active source, and a popover to pick another.
#[derive(IntoElement)]
pub struct SourceSwitcher {
    sources: Vec<SourceInfo>,
    status: Status,
    collapsed: bool,
    bare: bool,
}

impl SourceSwitcher {
    pub fn new(sources: Vec<SourceInfo>, status: Status) -> Self {
        Self {
            sources,
            status,
            collapsed: false,
            bare: false,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Drop the rail's framing: in a bar the padding and the rule below it have nowhere to go.
    pub fn bare(mut self, bare: bool) -> Self {
        self.bare = bare;
        self
    }
}

impl RenderOnce for SourceSwitcher {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let active = self.sources.iter().find(|source| source.active);
        let label: SharedString = active
            .map(|source| SharedString::from(source.name.clone()))
            .unwrap_or_default();
        let mark = active.map_or(SourceMark::none(), |source| {
            SourceMark::new(&source.service)
        });
        let collapsed = self.collapsed;
        let bare = self.bare;
        let count = self.sources.len();
        let sources = self.sources;
        let status = self.status;

        div()
            .when(!bare, |frame| {
                frame
                    .border_b_1()
                    .border_color(style::white(style::BORDER_SOFT, cx))
            })
            .map(|frame| match (bare, collapsed) {
                (true, _) => frame,
                (false, true) => frame.flex().justify_center().py_3(),
                (false, false) => frame.px_3().pt_3().pb_2(),
            })
            .child(
                trigger(label, mark, status, collapsed, cx)
                    .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, cx| {
                        build(&sources, count, menu, cx)
                    }),
            )
    }
}

/// Every source, then the way to manage them; a `PopupMenu` so escape, arrows and enter come free.
fn build(sources: &[SourceInfo], count: usize, mut menu: PopupMenu, cx: &App) -> PopupMenu {
    menu = menu
        .max_w(POPOVER_WIDTH)
        .min_w(POPOVER_WIDTH)
        .max_h(ROWS_HEIGHT)
        .scrollable(true)
        .label(SharedString::from(format!("{}  {count}", t("sources"))));
    for source in sources {
        menu = menu.item(row(source, cx));
    }
    menu.separator().item(
        PopupMenuItem::new(t("manage_sources"))
            .icon(Icon::default().path(MANAGE_ICON).size(GLYPH))
            .on_click(|_, _, cx| {
                Navigator::navigate(
                    Route::Settings {
                        section: Some("library".into()),
                    },
                    cx,
                );
            }),
    )
}

fn row(source: &SourceInfo, cx: &App) -> PopupMenuItem {
    let accent = source_mark::accent(Some(&source.service), cx);
    let active = source.active;
    let id = source.id.clone();
    let name = SharedString::from(source.name.clone());
    let subline = subline(source);
    let mark = SourceMark::new(&source.service).size(GLYPH);
    let ink = match active {
        true => cx.theme().foreground,
        false => style::white(ROW_NAME, cx),
    };
    let muted = style::white(style::TEXT_SUBTLE, cx);
    PopupMenuItem::element(move |_, _| {
        h_flex()
            .w_full()
            .gap_2p5()
            .child(mark.clone())
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(SUBLINE_GAP)
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .map(|line| match active {
                                true => line.font_semibold(),
                                false => line.font_medium(),
                            })
                            .text_color(ink)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(style::text_11())
                            .text_color(muted)
                            .child(subline.clone()),
                    ),
            )
            .when(active, |row| {
                row.child(
                    Icon::default()
                        .path(icons::CHECK)
                        .size(CHECK)
                        .text_color(accent),
                )
            })
    })
    .on_click(move |_, window, cx| {
        SourcesModel::global(cx).update(cx, |sources, cx| sources.switch(id.clone(), window, cx));
    })
}

/// The trigger tile: glyph, name, status dot and chevron, on the Button the menu hangs off.
fn trigger(
    label: SharedString,
    mark: SourceMark,
    status: Status,
    collapsed: bool,
    cx: &App,
) -> Button {
    let tooltip = label.clone();
    let tile = h_flex()
        .w_full()
        .gap_2p5()
        .child(mark)
        .when(!collapsed, |trigger| {
            trigger
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_medium()
                        .text_color(cx.theme().foreground)
                        .child(label),
                )
                .child(dot(status, cx))
                .child(
                    Icon::default()
                        .path(icons::CHEVRON_DOWN)
                        .size(CHEVRON)
                        .text_color(style::white(HEAD_COUNT, cx)),
                )
        });
    let button = style::dress(
        Button::new("source-trigger").tab_index(0),
        gpui_kit::transparent_black(),
        style::white(style::RAISED, cx),
        cx.theme().foreground,
        style::white(style::BORDER, cx),
        cx,
    )
    .rounded(px(8.))
    .map(|button| match collapsed {
        true => button.size(RAIL_TILE).tooltip(tooltip),
        false => button.w_full().h_auto().px_2p5().py_2(),
    });
    button.child(tile)
}

fn dot(status: Status, cx: &App) -> AnyElement {
    let color = match status {
        Status::Online => cx.theme().success,
        Status::Connecting => cx.theme().warning,
        Status::Offline => cx.theme().danger,
    };
    let dot = div().w(DOT).h(DOT).flex_shrink_0().rounded_full().bg(color);
    if status == Status::Connecting {
        dot.with_animation(
            "source-status",
            Animation::new(Duration::from_millis(1100))
                .repeat()
                .with_easing(pulsating_between(0.4, 1.)),
            |dot, delta| dot.opacity(delta),
        )
        .into_any_element()
    } else {
        dot.into_any_element()
    }
}

fn subline(source: &SourceInfo) -> SharedString {
    text(&source.service.name).to_uppercase().into()
}
