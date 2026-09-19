//! The row and section shapes every settings category in this crate is built from.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled, Window, div,
};
use widgets::format::t;

/// A titled group of rows, the way the Dioxus settings page grouped them.
pub fn section(title: SharedString) -> Div {
    v_flex().w_full().gap_1().pb_6().child(
        div()
            .pb_2()
            .text_lg()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title),
    )
}

/// One setting: its label on the left, its control on the right.
#[derive(IntoElement)]
pub struct SettingRow {
    title: SharedString,
    description: Option<SharedString>,
    locked: bool,
    stacked: bool,
    control: Option<AnyElement>,
}

impl SettingRow {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            locked: false,
            stacked: false,
            control: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A managed settings file pins this row's key; its control is inert and says so.
    pub fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// Put the control under the label instead of beside it, for a control that needs the width.
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = v_flex()
            .min_w_0()
            .gap_1()
            .child(div().text_sm().child(self.title))
            .when_some(self.description, |column, description| {
                column.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                )
            })
            .when(self.locked, |column| {
                column.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t("setting_managed_by_system")),
                )
            });
        if self.stacked {
            v_flex()
                .w_full()
                .gap_2()
                .py_2()
                .child(label)
                .children(self.control)
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .gap_4()
                .py_2()
                .justify_between()
                .items_center()
                .child(label)
                .children(self.control)
                .into_any_element()
        }
    }
}
