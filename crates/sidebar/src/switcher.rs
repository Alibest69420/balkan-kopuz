use std::time::Duration;

use api::{SourceInfo, SourceKind};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::popover::{Popover, PopoverState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, Entity, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, RenderOnce, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, pulsating_between, px, svg,
};
use store::{Navigator, Route, SourcesModel};
use widgets::format::{t, text};
use widgets::{icons, style};

const GLYPH: Pixels = px(16.);
const DOT: Pixels = px(6.);
const CHEVRON: Pixels = px(10.);
const CHECK: Pixels = px(11.);
const ARROW: Pixels = px(9.);
const POPOVER_WIDTH: Pixels = px(218.);
const ROWS_HEIGHT: Pixels = px(320.);
const RAIL_TILE: Pixels = px(40.);
const HEAD_TOP: Pixels = px(10.);
const SUBLINE_GAP: Pixels = px(2.);

// The switcher's own dimmed tones, which are off the named alpha scale.
const HEAD_COUNT: f32 = 0.38;
const ROW_NAME: f32 = 0.85;

const LOCAL_ICON: &str = "icons/hard-drive.svg";
const LIBRARY_ICON: &str = "icons/folder-tree.svg";
const SERVICE_ICON: &str = "icons/server.svg";
const CLOUD_ICON: &str = "icons/cloud.svg";
const MANAGE_ICON: &str = "icons/sliders-horizontal.svg";
const FORWARD_ICON: &str = "icons/arrow-right.svg";

/// Whether the active source answered the daemon's probe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Connecting,
    Online,
    Offline,
}

/// How a source is drawn: the daemon's own SVG path, or a bundled icon standing in for its class.
#[derive(Clone)]
enum Mark {
    Path(String),
    Bundled(&'static str),
}

/// The active source, and a popover to pick another.
#[derive(IntoElement)]
pub(crate) struct SourceSwitcher {
    sources: Vec<SourceInfo>,
    status: Status,
    collapsed: bool,
}

impl SourceSwitcher {
    pub(crate) fn new(sources: Vec<SourceInfo>, status: Status) -> Self {
        Self {
            sources,
            status,
            collapsed: false,
        }
    }

    pub(crate) fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl RenderOnce for SourceSwitcher {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let active = self.sources.iter().find(|source| source.active);
        let label: SharedString = active
            .map(|source| SharedString::from(source.name.clone()))
            .unwrap_or_else(|| t("local"));
        let mark = active.map_or(Mark::Bundled(LOCAL_ICON), mark_of);
        let accent = accent_of(active, cx);
        let collapsed = self.collapsed;
        let count = self.sources.len();
        let sources = self.sources;

        div()
            .border_b_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .map(|frame| {
                if collapsed {
                    frame.flex().justify_center().py_3()
                } else {
                    frame.px_3().pt_3().pb_2()
                }
            })
            .child(
                Popover::new("source-switcher")
                    .anchor(Anchor::TopLeft)
                    .p_0()
                    .w(POPOVER_WIDTH)
                    .rounded(style::radius_lg())
                    .border_1()
                    .border_color(style::white(style::BORDER, cx))
                    .overflow_hidden()
                    .trigger(SwitcherTrigger {
                        label,
                        mark,
                        accent,
                        status: self.status,
                        collapsed,
                        selected: false,
                    })
                    .content(move |_, _, cx| {
                        let state = cx.entity();
                        v_flex()
                            .w_full()
                            .child(header(count, cx))
                            .child(
                                v_flex()
                                    .id("source-rows")
                                    .p_1()
                                    .max_h(ROWS_HEIGHT)
                                    .overflow_y_scroll()
                                    .children(
                                        sources
                                            .iter()
                                            .map(|source| row(source, &state, cx))
                                            .collect::<Vec<_>>(),
                                    ),
                            )
                            .child(footer(&state, cx))
                    }),
            )
    }
}

fn header(count: usize, cx: &App) -> impl IntoElement {
    h_flex()
        .justify_between()
        .pt(HEAD_TOP)
        .px_3()
        .pb_2()
        .border_b_1()
        .border_color(style::white(style::BORDER, cx))
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .child(t("sources")),
        )
        .child(
            div()
                .text_xs()
                .text_color(style::white(HEAD_COUNT, cx))
                .child(SharedString::from(count.to_string())),
        )
}

fn row(source: &SourceInfo, state: &Entity<PopoverState>, cx: &App) -> AnyElement {
    let accent = accent_of(Some(source), cx);
    let active = source.active;
    let id = source.id.clone();
    let state = state.clone();
    h_flex()
        .id(SharedString::from(format!("source-{}", source.id)))
        .w_full()
        .gap_2p5()
        .p_2()
        .rounded(style::radius_lg())
        .cursor_pointer()
        .when(active, |row| row.bg(style::white(style::RAISED, cx)))
        .hover(|row| row.bg(style::white(style::RAISED, cx)))
        .child(glyph(&mark_of(source), GLYPH, accent))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(SUBLINE_GAP)
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .map(|name| {
                            if active {
                                name.font_semibold().text_color(cx.theme().foreground)
                            } else {
                                name.font_medium().text_color(style::white(ROW_NAME, cx))
                            }
                        })
                        .child(SharedString::from(source.name.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(style::text_11())
                        .text_color(style::white(style::TEXT_SUBTLE, cx))
                        .child(subline(source)),
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
        .on_click(move |_, window, cx| {
            SourcesModel::global(cx).update(cx, |sources, cx| sources.switch(id.clone(), cx));
            state.update(cx, |popover, cx| popover.dismiss(window, cx));
        })
        .into_any_element()
}

fn footer(state: &Entity<PopoverState>, cx: &App) -> impl IntoElement {
    let state = state.clone();
    div()
        .w_full()
        .p_1()
        .border_t_1()
        .border_color(style::white(style::BORDER, cx))
        .child(
            h_flex()
                .id("manage-sources")
                .w_full()
                .gap_2p5()
                .p_2()
                .rounded(style::radius_lg())
                .cursor_pointer()
                .text_sm()
                .font_medium()
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .hover(|footer| {
                    footer
                        .bg(style::white(style::RAISED, cx))
                        .text_color(cx.theme().foreground)
                })
                .child(Icon::default().path(MANAGE_ICON).size(GLYPH))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(t("manage_sources")),
                )
                .child(Icon::default().path(FORWARD_ICON).size(ARROW).ml_auto())
                .on_click(move |_, window, cx| {
                    Navigator::navigate(
                        Route::Settings {
                            section: Some("library".into()),
                        },
                        cx,
                    );
                    state.update(cx, |popover, cx| popover.dismiss(window, cx));
                }),
        )
}

/// The trigger tile: glyph, name, status dot and chevron.
#[derive(IntoElement)]
struct SwitcherTrigger {
    label: SharedString,
    mark: Mark,
    accent: Hsla,
    status: Status,
    collapsed: bool,
    selected: bool,
}

impl Selectable for SwitcherTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for SwitcherTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = self.label.clone();
        let tooltip = self.label;
        h_flex()
            .id("source-trigger")
            .w_full()
            .gap_2p5()
            .px_2p5()
            .py_2()
            .rounded(style::radius_lg())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .cursor_pointer()
            .when(self.selected, |trigger| {
                trigger
                    .bg(style::white(style::RAISED, cx))
                    .border_color(style::white(style::ACTIVE, cx))
            })
            .hover(|trigger| trigger.bg(style::white(style::RAISED, cx)))
            .when(self.collapsed, |trigger| {
                trigger
                    .w(RAIL_TILE)
                    .h(RAIL_TILE)
                    .p_0()
                    .justify_center()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            })
            .child(glyph(&self.mark, GLYPH, self.accent))
            .when(!self.collapsed, |trigger| {
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
                    .child(dot(self.status, cx))
                    .child(
                        Icon::default()
                            .path(icons::CHEVRON_DOWN)
                            .size(CHEVRON)
                            .text_color(style::white(HEAD_COUNT, cx)),
                    )
            })
    }
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

fn glyph(mark: &Mark, size: Pixels, color: Hsla) -> AnyElement {
    match mark {
        Mark::Path(path) => svg()
            .size(size)
            .flex_shrink_0()
            .text_color(color)
            .data(document(path).as_bytes())
            .into_any_element(),
        Mark::Bundled(path) => Icon::default()
            .path(*path)
            .size(size)
            .flex_shrink_0()
            .text_color(color)
            .into_any_element(),
    }
}

// The renderer keeps only the alpha of what it rasterizes, so the fill just has to be opaque.
fn document(path: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="black"><path d="{path}"/></svg>"#
    )
}

fn mark_of(source: &SourceInfo) -> Mark {
    match (source.kind, source.service.as_ref()) {
        (SourceKind::Server, Some(service)) => match &service.icon {
            api::Icon::Svg(path) => Mark::Path(path.clone()),
            api::Icon::Class(class) => Mark::Bundled(bundled(class)),
        },
        (SourceKind::LocalLibrary, _) => Mark::Bundled(LIBRARY_ICON),
        _ => Mark::Bundled(LOCAL_ICON),
    }
}

/// A FontAwesome class cannot be drawn here, so it picks the nearest bundled Lucide icon.
fn bundled(class: &str) -> &'static str {
    if class.contains("compact-disc") {
        icons::DISC
    } else if class.contains("cloud") {
        CLOUD_ICON
    } else if class.contains("youtube") || class.contains("apple") {
        icons::MUSIC
    } else if class.contains("folder") {
        LIBRARY_ICON
    } else if class.contains("hard-drive") {
        LOCAL_ICON
    } else {
        SERVICE_ICON
    }
}

fn subline(source: &SourceInfo) -> SharedString {
    match (source.kind, source.service.as_ref()) {
        (SourceKind::Server, Some(service)) => text(&service.name).to_uppercase().into(),
        _ => t("source_on_this_device"),
    }
}

/// A service's brand colour; a local source wears the app's own accent.
fn accent_of(source: Option<&SourceInfo>, cx: &App) -> Hsla {
    source
        .and_then(|source| source.service.as_ref())
        .and_then(|service| Rgba::try_from(service.accent.as_str()).ok())
        .map(Hsla::from)
        .unwrap_or(cx.theme().primary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_class_still_draws_something() {
        assert_eq!(bundled("fa-solid fa-server"), SERVICE_ICON);
        assert_eq!(bundled("fa-brands fa-soundcloud"), CLOUD_ICON);
    }

    #[test]
    fn a_path_becomes_a_standalone_document() {
        assert!(document("M0 0").contains(r#"<path d="M0 0"/>"#));
    }
}
