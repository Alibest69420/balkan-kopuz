use gpui_kit::component::Sizable as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled, Window, div, px,
};

use crate::icons;
use crate::motion;
use crate::style;

/// What a page shows instead of rows: nothing found, nothing configured, nothing yet.
#[derive(IntoElement)]
pub struct EmptyState {
    title: SharedString,
    detail: Option<SharedString>,
    icon: &'static str,
}

impl EmptyState {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            icon: icons::MUSIC,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fade = motion::enter(motion::id("empty", &self.title), motion::MEDIUM, window, cx);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .px_6()
            .opacity(fade)
            .text_center()
            .text_color(style::white(style::TEXT_SUBTLE, cx))
            .child(
                Icon::default()
                    .path(self.icon)
                    .size(px(36.))
                    .mb_4()
                    .text_color(style::white(style::TEXT_FAINT, cx)),
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
    }
}

/// The whole-page spinner, for a read that has not answered yet.
#[derive(IntoElement)]
pub struct Loading;

impl RenderOnce for Loading {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fade = motion::enter("loading", motion::MEDIUM, window, cx);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .py_12()
            .opacity(fade)
            .child(
                Spinner::new()
                    .large()
                    .color(style::white(style::TEXT_FAINT, cx)),
            )
    }
}
