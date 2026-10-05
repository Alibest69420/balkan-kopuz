//! A guided flow: the numbered rail, the frame a step sits in, and the big cards a step offers.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    px, rems,
};

use crate::format::t;
use crate::{SectionHeading, icons, motion, style};

/// Lucide paths this flow needs that `icons` has no name for yet.
const SPARKLES: &str = "icons/sparkles.svg";
const ARROW_LEFT: &str = "icons/arrow-left.svg";

/// `bg-white/8` over `bg-white/5`, the lift a choice card takes under the pointer.
const CARD_HOVER: f32 = 0.08;
/// `bg-<accent>/15` behind a glyph.
const BADGE_BG: f32 = 0.15;

/// The "1 Source · 2 Connect · 3 Scan" rail; each step is lit while it is the one being answered.
#[derive(IntoElement)]
pub struct Stepper {
    labels: Vec<SharedString>,
    current: usize,
}

impl Stepper {
    pub fn new(labels: impl IntoIterator<Item = SharedString>) -> Self {
        Self {
            labels: labels.into_iter().collect(),
            current: 0,
        }
    }

    /// The zero-based step being answered.
    pub fn current(mut self, current: usize) -> Self {
        self.current = current;
        self
    }
}

impl RenderOnce for Stepper {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let current = self.current;
        h_flex()
            .gap_4()
            .items_center()
            .children(self.labels.into_iter().enumerate().map(|(index, label)| {
                let (done, here) = (index < current, index == current);
                let color = if here {
                    cx.theme().primary
                } else if done {
                    style::white(style::TEXT_SECONDARY, cx)
                } else {
                    style::white(style::TEXT_DIM, cx)
                };
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_5()
                            .rounded_full()
                            .bg(if here {
                                style::accent(BADGE_BG, cx)
                            } else {
                                style::white(style::RAISED, cx)
                            })
                            .text_size(style::text_10())
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(SharedString::from((index + 1).to_string())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(color)
                            .child(label),
                    )
            }))
    }
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// `max-w-2xl mx-auto`: the column a guided flow is set in.
pub fn column_width() -> gpui_kit::Rems {
    rems(40.)
}

/// One step of a flow: the greeting, the rail, the step's own body, and the way back or out.
#[derive(IntoElement)]
pub struct StepFrame {
    title: SharedString,
    subtitle: SharedString,
    stepper: Option<Stepper>,
    body: Vec<AnyElement>,
    on_back: Option<OnClick>,
    on_skip: Option<OnClick>,
}

impl StepFrame {
    pub fn new(title: impl Into<SharedString>, subtitle: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            stepper: None,
            body: Vec::new(),
            on_back: None,
            on_skip: None,
        }
    }

    pub fn stepper(mut self, stepper: Stepper) -> Self {
        self.stepper = Some(stepper);
        self
    }

    /// Without it there is no Back button, as on a flow's first step.
    pub fn on_back(mut self, back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Rc::new(back));
        self
    }

    pub fn on_skip(mut self, skip: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_skip = Some(Rc::new(skip));
        self
    }
}

impl ParentElement for StepFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for StepFrame {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let quiet = style::bare_button(style::slate_400(cx), cx);
        let (back, skip) = (self.on_back, self.on_skip);
        v_flex()
            .w_full()
            .max_w(column_width())
            .mx_auto()
            .px_8()
            .py_12()
            .gap_6()
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_10()
                            .rounded(style::radius_xl())
                            .bg(style::accent(BADGE_BG, cx))
                            .child(
                                icons::icon(SPARKLES)
                                    .size(px(18.))
                                    .text_color(cx.theme().primary),
                            ),
                    )
                    .child(SectionHeading::new(self.title))
                    .child(
                        div()
                            .text_sm()
                            .text_color(style::slate_400(cx))
                            .child(self.subtitle),
                    ),
            )
            .children(self.stepper)
            .children(self.body)
            .child(
                h_flex()
                    .w_full()
                    .pt_2()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(div().children(back.map(|back| {
                        Button::new("step-back")
                            .custom(quiet)
                            .small()
                            .icon(icons::icon(ARROW_LEFT))
                            .label(t("first_run_back"))
                            .on_click(move |event, window, cx| back(event, window, cx))
                    })))
                    .children(skip.map(|skip| {
                        Button::new("step-skip")
                            .custom(quiet)
                            .small()
                            .label(t("first_run_skip"))
                            .on_click(move |event, window, cx| skip(event, window, cx))
                    })),
            )
    }
}

/// A big pressable card: a tinted glyph, a title, a line saying what it gets you.
#[derive(IntoElement)]
pub struct ChoiceCard {
    id: SharedString,
    glyph: Option<AnyElement>,
    accent: Option<Hsla>,
    title: SharedString,
    detail: Option<SharedString>,
    on_click: Option<OnClick>,
}

impl ChoiceCard {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            glyph: None,
            accent: None,
            title: title.into(),
            detail: None,
            on_click: None,
        }
    }

    pub fn glyph(mut self, glyph: impl IntoElement) -> Self {
        self.glyph = Some(glyph.into_any_element());
        self
    }

    /// The colour the glyph's square is tinted with; the app's accent by default.
    pub fn accent(mut self, accent: Hsla) -> Self {
        self.accent = Some(accent);
        self
    }

    pub fn detail(mut self, detail: Option<SharedString>) -> Self {
        self.detail = detail;
        self
    }

    pub fn on_click(
        mut self,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(click));
        self
    }
}

impl RenderOnce for ChoiceCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (hovered, state) = motion::hovered(motion::id("first-run-card", &self.id), window, cx);
        let surface = motion::color(
            motion::id("first-run-card-bg", &self.id),
            style::white(if hovered { CARD_HOVER } else { style::RAISED }, cx),
            window,
            cx,
        );
        let accent = self.accent.unwrap_or(cx.theme().primary);
        let click = self.on_click;
        h_flex()
            .id(self.id)
            .w_full()
            .gap_4()
            .items_center()
            .p_4()
            .cursor_pointer()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .bg(surface)
            .on_hover(motion::track(state))
            .on_click(move |event, window, cx| {
                if let Some(click) = &click {
                    click(event, window, cx);
                }
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(rems(2.75))
                    .rounded(style::radius_lg())
                    .bg(accent.opacity(BADGE_BG))
                    .children(self.glyph),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.title),
                    )
                    .when_some(self.detail, |column, detail| {
                        column.child(
                            div()
                                .text_sm()
                                .text_color(style::slate_400(cx))
                                .child(detail),
                        )
                    }),
            )
            .child(
                icons::icon(icons::CHEVRON_RIGHT)
                    .size(px(16.))
                    .flex_shrink_0()
                    .text_color(style::white(style::TEXT_DIM, cx)),
            )
    }
}
