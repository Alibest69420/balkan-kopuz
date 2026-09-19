use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled, Window, div,
};
use store::{Navigator, Route};

use crate::icons;

/// The title block every page opens with, with its actions on the right.
#[derive(IntoElement)]
pub struct PageHeader {
    title: SharedString,
    subtitle: Option<SharedString>,
    back: Option<Route>,
    actions: Vec<AnyElement>,
}

impl PageHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            back: None,
            actions: Vec::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Show a back button that returns to `route`.
    pub fn back(mut self, route: Route) -> Self {
        self.back = Some(route);
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
            .pb_4()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .when_some(self.back, |row, route| {
                        row.child(
                            Button::new("page-back")
                                .ghost()
                                .icon(crate::icons::icon(icons::CHEVRON_LEFT))
                                .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx)),
                        )
                    })
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child(self.title),
                            )
                            .when_some(self.subtitle, |column, subtitle| {
                                column.child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(subtitle),
                                )
                            }),
                    ),
            )
            .child(h_flex().gap_2().items_center().children(self.actions))
    }
}
