use gpui_kit::component::Sizable as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled, Window, div, px,
};

use crate::icons;

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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .p_8()
            .text_color(cx.theme().muted_foreground)
            .child(Icon::default().path(self.icon).size(px(32.)))
            .child(div().text_lg().child(self.title))
            .when_some(self.detail, |column, detail| {
                column.child(div().text_sm().child(detail))
            })
    }
}

/// The whole-page spinner, for a read that has not answered yet.
#[derive(IntoElement)]
pub struct Loading;

impl RenderOnce for Loading {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(Spinner::new().large())
    }
}
