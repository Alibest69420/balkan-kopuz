//! The settings card and the rows inside it, as `docs/STYLING.md` maps `.settings-section`.

use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Rems, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window,
    div, px, rems,
};

use crate::format::t;
use crate::{icons, motion, style};

/// `w-55`, the width every enumerated setting's trigger shares.
pub fn select_width() -> Rems {
    rems(13.75)
}

/// One control shape for every enumerated setting: a button that drops its options down.
pub fn choice<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    current: &T,
    options: Vec<(T, SharedString)>,
    change: impl Fn(T, &mut App) + Clone + 'static,
) -> AnyElement {
    let label = options
        .iter()
        .find(|(value, _)| value == current)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| t("choose"));
    Button::new(id)
        .outline()
        .w(select_width())
        .justify_between()
        .icon(icons::icon(icons::CHEVRON_DOWN))
        .label(label)
        .dropdown_menu(move |mut menu, _, _| {
            for (value, label) in &options {
                let value = value.clone();
                let change = change.clone();
                menu = menu.item(PopupMenuItem::new(label.clone()).on_click(move |_, _, cx| {
                    change(value.clone(), cx);
                }));
            }
            menu
        })
        .into_any_element()
}

/// `.settings-section`: `border-white/11` over `bg-black/78`.
const CARD_BORDER: f32 = 0.11;
const CARD_BG: f32 = 0.78;
/// `.settings-section-header`: `bg-black/58`, lifting to `bg-white/7` under the pointer.
const HEADER_BG: f32 = 0.58;
const HEADER_HOVER: f32 = 0.07;
/// `divide-white/[0.07]` between rows.
const DIVIDER: f32 = 0.07;
/// `bg-white/2.5` on every second row.
const STRIPE: f32 = 0.025;
/// `text-white/65` on the section heading.
const HEADING: f32 = 0.65;
/// `text-white/40` on the lock a managed row is marked with.
const LOCK: f32 = 0.40;
/// `hover:text-slate-300` on a toggle's idle half, one step brighter than the muted token.
const TOGGLE_IDLE_HOVER: f32 = 0.70;
/// `opacity-50` on a locked row's control.
const LOCKED_OPACITY: f32 = 0.50;

/// `min-h-[3.25rem]`, the height a settings row never goes under.
pub fn row_height() -> Rems {
    rems(3.25)
}

/// `prefix` over a domain key: one stable channel per row, per section, per toggle half.
fn scoped(prefix: &'static str, key: &SharedString) -> ElementId {
    (ElementId::from(prefix), key.clone()).into()
}

/// Hover as tracked state, because a tween needs the value a `.hover()` refinement never exposes.
fn hover_state(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Entity<bool> {
    window.use_keyed_state(id, cx, |_, _| false)
}

/// The listener that keeps one of those in step.
fn track_hover(state: Entity<bool>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |over, _, cx| {
        let over = *over;
        state.update(cx, |held, cx| {
            if *held != over {
                *held = over;
                cx.notify();
            }
        });
    }
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let last = self.rows.len().saturating_sub(1);
        let hovered = hover_state(scoped("section-hover", &self.title), window, cx);
        let header_target = if *hovered.read(cx) {
            style::white(HEADER_HOVER, cx)
        } else {
            style::black(HEADER_BG, cx)
        };
        let header_bg = motion::color(
            scoped("section-header", &self.title),
            header_target,
            window,
            cx,
        );
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
                    .id(scoped("section", &self.title))
                    .w_full()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .justify_between()
                    .items_center()
                    .bg(header_bg)
                    .on_hover(track_hover(hovered))
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hovered = hover_state(scoped("row-hover", &self.title), window, cx);
        let lift = if *hovered.read(cx) { style::RAISED } else { 0. };
        let surface = motion::color(
            scoped("row-bg", &self.title),
            style::white(lift, cx),
            window,
            cx,
        );
        let held = motion::amount(
            scoped("row-lock", &self.title),
            if self.locked { LOCKED_OPACITY } else { 1. },
            motion::FAST,
            window,
            cx,
        );
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
                            .child(self.title.clone()),
                    )
                    .when(self.locked, |heading| {
                        heading.child(
                            icons::icon(icons::LOCK)
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
                .when(held < 1., |fading| fading.opacity(held))
                .child(control)
        });
        if self.stacked {
            v_flex()
                .id(scoped("row", &self.title))
                .w_full()
                .min_h(row_height())
                .px_5()
                .py_4()
                .bg(surface)
                .on_hover(track_hover(hovered))
                .child(div().mb_3().child(label))
                .children(control)
                .into_any_element()
        } else {
            h_flex()
                .id(scoped("row", &self.title))
                .w_full()
                .min_h(row_height())
                .gap_5()
                .px_5()
                .py_2p5()
                .justify_between()
                .items_center()
                .bg(surface)
                .on_hover(track_hover(hovered))
                .child(label)
                .children(control)
                .into_any_element()
        }
    }
}

/// What a toggle answers with; both halves share one handler, so it is counted, not moved.
type Answer = Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

/// `w-48`, the track the thumb slides along.
fn toggle_width() -> Rems {
    rems(12.)
}

/// `inset-inline-start: 4px`, the gap the thumb keeps from the track's ends.
const THUMB_INSET: f32 = 4.;
/// `border`, which the thumb's `50%` resolves against the padding box without.
const TOGGLE_BORDER: f32 = 1.;

/// `ToggleSetting`: the segmented Enabled/Disabled control every boolean setting answers with.
#[derive(IntoElement)]
pub struct Toggle {
    id: SharedString,
    enabled: bool,
    disabled: bool,
    on_change: Option<Answer>,
}

impl Toggle {
    /// `id` is the row's config key, so the two halves keep stable, domain-derived ids.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            enabled: false,
            disabled: false,
            on_change: None,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// One half of the segmented control: the thumb says which is the answer, so a half only colors.
    fn half(
        &self,
        value: bool,
        label: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement + use<> {
        let id = SharedString::from(format!("{}-{}", self.id, if value { "on" } else { "off" }));
        let selected = value == self.enabled;
        let hovered = hover_state(scoped("toggle-half-hover", &id), window, cx);
        let target = if selected {
            style::white(1., cx)
        } else if !self.disabled && *hovered.read(cx) {
            style::white(TOGGLE_IDLE_HOVER, cx)
        } else {
            style::slate_500(cx)
        };
        let text = motion::color_over(scoped("toggle-half", &id), target, motion::SLOW, window, cx);
        let half = h_flex()
            .id(id)
            .flex_1()
            .h_8()
            .items_center()
            .justify_center()
            .rounded(style::radius_lg())
            .text_size(style::text_11())
            .font_weight(FontWeight::BOLD)
            .text_color(text)
            .child(label);
        if self.disabled {
            return half;
        }
        let half = half.cursor_pointer().on_hover(track_hover(hovered));
        match self.on_change.clone() {
            Some(handler) => half.on_click(move |_, window, cx| handler(&value, window, cx)),
            None => half,
        }
    }
}

impl RenderOnce for Toggle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let half = (style::to_px(toggle_width(), window) - px(TOGGLE_BORDER * 2.)) / 2.;
        let resting = if self.enabled {
            px(THUMB_INSET)
        } else {
            half + px(THUMB_INSET / 2.)
        };
        let offset = motion::length(
            scoped("toggle-thumb", &self.id),
            resting,
            motion::SLOW,
            window,
            cx,
        );
        let thumb = div()
            .absolute()
            .left(offset)
            .h_8()
            .w(half - px(THUMB_INSET))
            .rounded(style::radius_lg())
            .bg(style::white(style::HOVER, cx));
        let on = self.half(true, t("enabled"), window, cx);
        let off = self.half(false, t("disabled"), window, cx);
        h_flex()
            .relative()
            .w_48()
            .h_10()
            .p_1()
            .items_center()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::white(style::RAISED, cx))
            .child(thumb)
            .child(on)
            .child(off)
    }
}
