use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled, Window, div,
};
use store::Route;

use crate::style;

/// The title block every page opens with, with its actions on the right.
#[derive(IntoElement)]
pub struct PageHeader {
    title: SharedString,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
}

impl PageHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            actions: Vec::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Taken and ignored: the nav bar owns history, so no page draws a back chevron of its own.
    pub fn back(self, _: Route) -> Self {
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for PageHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_3()
            .pb_6()
            .child(
                h_flex().gap_3().items_center().child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(self.title),
                        )
                        .when_some(self.subtitle, |column, subtitle| {
                            column.child(
                                div()
                                    .text_sm()
                                    .text_color(style::slate_400(cx))
                                    .child(subtitle),
                            )
                        }),
                ),
            )
            .child(h_flex().gap_2().items_center().children(self.actions))
    }
}
