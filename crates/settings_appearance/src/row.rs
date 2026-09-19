//! The settings card and the rows inside it, as `docs/STYLING.md` maps `.settings-section`.

// Copied verbatim into the sibling settings crates until this shape moves into `widgets`.
#![allow(dead_code)]

use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Rems,
    RenderOnce, SharedString, Styled, Window, div, rems,
};
use widgets::icons;
use widgets::style;

/// `.settings-section`: `border-white/11` over `bg-black/78`.
const CARD_BORDER: f32 = 0.11;
const CARD_BG: f32 = 0.78;
/// `.settings-section-header`: `bg-black/58`.
const HEADER_BG: f32 = 0.58;
/// `divide-white/[0.07]` between rows.
const DIVIDER: f32 = 0.07;
/// `bg-white/2.5` on every second row.
const STRIPE: f32 = 0.025;
/// `text-white/65` on the section heading.
const HEADING: f32 = 0.65;
/// `text-white/40` on the lock a managed row is marked with.
const LOCK: f32 = 0.40;
/// The lock glyph; `widgets::icons` has no constant for it yet.
const LOCK_ICON: &str = "icons/lock.svg";

/// `min-h-[3.25rem]`, the height a settings row never goes under.
pub fn row_height() -> Rems {
    rems(3.25)
}

/// A titled group of rows: the card the Dioxus settings page draws one category on.
pub fn section(title: impl Into<SharedString>) -> Section {
    Section {
        title: title.into(),
        rows: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct Section {
    title: SharedString,
    rows: Vec<AnyElement>,
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for Section {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let last = self.rows.len().saturating_sub(1);
        v_flex()
            .w_full()
            .overflow_hidden()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(CARD_BORDER, cx))
            .bg(style::black(CARD_BG, cx))
            .shadow_lg()
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .justify_between()
                    .items_center()
                    .bg(style::black(HEADER_BG, cx))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(style::white(HEADING, cx))
                            .child(SharedString::from(self.title.to_uppercase())),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .children(self.rows.into_iter().enumerate().map(|(index, row)| {
                        div()
                            .w_full()
                            .when(index % 2 == 1, |band| band.bg(style::white(STRIPE, cx)))
                            .when(index < last, |band| {
                                band.border_b_1().border_color(style::white(DIVIDER, cx))
                            })
                            .child(row)
                    })),
            )
    }
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
            .gap_0p5()
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(style::white(style::TEXT_STRONG, cx))
                            .child(self.title),
                    )
                    .when(self.locked, |heading| {
                        heading.child(
                            icons::icon(LOCK_ICON)
                                .with_size(style::text_10())
                                .text_color(style::white(LOCK, cx)),
                        )
                    }),
            )
            .when_some(self.description, |column, description| {
                column.child(
                    div()
                        .text_xs()
                        .text_color(style::slate_400(cx))
                        .child(description),
                )
            });
        let control = self.control.map(|control| {
            div()
                .when(self.locked, |held| held.opacity(0.5))
                .child(control)
        });
        if self.stacked {
            v_flex()
                .w_full()
                .min_h(row_height())
                .px_5()
                .py_4()
                .hover(|row| row.bg(style::white(style::RAISED, cx)))
                .child(div().mb_3().child(label))
                .children(control)
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .min_h(row_height())
                .gap_5()
                .px_5()
                .py_2p5()
                .justify_between()
                .items_center()
                .hover(|row| row.bg(style::white(style::RAISED, cx)))
                .child(label)
                .children(control)
                .into_any_element()
        }
    }
}
