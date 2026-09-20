//! The frames the Library and Connectivity panels share: cards, folder rows, the host-access warning.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, ClickEvent, Div, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Stateful, StatefulInteractiveElement as _, Styled, Window,
    div, px,
};
use widgets::format::t;
use widgets::{icons, motion, style};

pub use widgets::settings::section;

// Flathub refuses `org.freedesktop.Flatpak` in the manifest, so the user grants it by hand.
const HOST_ACCESS_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak moe.kopuz.kopuz";

/// `text-white/60`, a card's secondary lines.
const HINT: f32 = 0.60;
/// `bg-indigo-500/30`, the pill on the active source.
const BADGE: f32 = 0.30;
/// `border-white/30` around the copyable flatpak command.
const COMMAND_BORDER: f32 = 0.30;
/// `bg-neutral-900/79` behind it.
const COMMAND_BG: f32 = 0.79;

/// `bg-white/10 hover:bg-white/20`: the quiet button these cards are built from.
pub fn soft(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(style::rest(style::white(style::HOVER, cx)))
        .hover(style::white(style::ACTIVE, cx))
        .active(style::white(style::ACTIVE, cx))
        .foreground(cx.theme().foreground)
}

/// `text-red-400 hover:text-red-300`: a destructive action written as text, not a filled button.
pub fn danger_text(cx: &App) -> ButtonCustomVariant {
    let transparent = cx.theme().transparent;
    ButtonCustomVariant::new(cx)
        .color(transparent)
        .hover(cx.theme().danger.opacity(0.1))
        .active(cx.theme().danger.opacity(0.1))
        .foreground(cx.theme().danger)
}

/// The inset a settings card gives its contents, the same one a stacked row has.
pub fn card(_: &App) -> Div {
    v_flex().w_full().gap_3().px_5().py_4()
}

/// `bg-white/5 p-3 rounded`: one library, server or picker inside that card.
pub fn row(cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_2()
        .p_3()
        .rounded(style::radius(cx))
        .bg(style::white(style::RAISED, cx))
}

/// `bg-white/8` over `bg-white/5`, the card a source's own row lifts to under the pointer.
const ROW_HOVER: f32 = 0.08;

/// The same card keyed to a source, so its surface tweens instead of cutting.
pub fn hover_row(key: &str, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let id = ElementId::Name(SharedString::from(format!("source-row-{key}")));
    let hovered: Entity<bool> = window.use_keyed_state(id.clone(), cx, |_, _| false);
    let lift = if *hovered.read(cx) {
        ROW_HOVER
    } else {
        style::RAISED
    };
    let surface = motion::color(
        ElementId::Name(SharedString::from(format!("source-row-bg-{key}"))),
        style::white(lift, cx),
        window,
        cx,
    );
    v_flex()
        .id(id)
        .w_full()
        .gap_2()
        .p_3()
        .rounded(style::radius(cx))
        .bg(surface)
        .on_hover(move |over, _, cx| {
            let over = *over;
            hovered.update(cx, |held, cx| {
                if *held != over {
                    *held = over;
                    cx.notify();
                }
            });
        })
}

/// `text-sm font-medium text-white`: the name of whatever the card is about.
pub fn label(text: SharedString) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .child(text)
}

/// `text-xs text-white/60`: a line under that name.
pub fn hint(text: SharedString, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(style::white(HINT, cx))
        .child(text)
}

/// `text-xs text-slate-500 italic`: what a list says when it holds nothing.
pub fn empty(text: SharedString, cx: &App) -> Div {
    div()
        .text_xs()
        .italic()
        .text_color(style::slate_500(cx))
        .child(text)
}

/// `.popup .error`: red text in a red box, the way every dialog reports a failure.
pub fn error(text: SharedString, cx: &App) -> Div {
    let danger = cx.theme().danger;
    div()
        .w_full()
        .px_3()
        .py_2()
        .rounded(style::radius(cx))
        .border_1()
        .border_color(danger.opacity(0.3))
        .bg(danger.opacity(0.1))
        .text_sm()
        .text_color(danger)
        .child(text)
}

/// Why one field is wrong, under that field rather than at the foot of the form.
pub fn field_problem(text: SharedString, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().danger).child(text)
}

/// The "Active" pill on whichever source the daemon is using.
pub fn active_badge(text: SharedString, cx: &App) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded(style::radius(cx))
        .bg(style::accent(BADGE, cx))
        .text_size(style::text_10())
        .text_color(cx.theme().primary_hover)
        .child(text)
}

/// One folder of a library, with the button that drops it.
pub fn folder_row(
    id: impl Into<ElementId>,
    path: SharedString,
    locked: bool,
    on_remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_3()
        .items_center()
        .justify_between()
        .p_2()
        .rounded(style::radius(cx))
        .bg(style::white(style::RAISED, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(style::slate_400(cx))
                .child(path),
        )
        .child(
            Button::new(id)
                .custom(danger_text(cx))
                .xsmall()
                .label(t("remove"))
                .disabled(locked)
                .on_click(on_remove),
        )
}

/// The button that opens the platform folder picker.
pub fn add_folder_button(
    id: impl Into<ElementId>,
    locked: bool,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .custom(soft(cx))
        .small()
        .icon(icons::icon(icons::PLUS))
        .label(t("add_folder"))
        .disabled(locked)
        .on_click(on_click)
        .self_start()
}

/// Said when the daemon cannot reach a browser on the host; the command that fixes it is copyable.
pub fn host_access_warning(id: impl Into<ElementId>, message: SharedString, cx: &App) -> Div {
    let warning = cx.theme().warning;
    v_flex()
        .w_full()
        .gap_2()
        .items_center()
        .px_3()
        .py_2()
        .rounded(style::radius(cx))
        .border_1()
        .border_color(warning.opacity(0.3))
        .bg(warning.opacity(0.1))
        .child(div().text_xs().text_color(warning).child(message))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .px_1p5()
                        .py_1()
                        .rounded(style::radius(cx))
                        .border_1()
                        .border_color(style::white(COMMAND_BORDER, cx))
                        .bg(style::black(COMMAND_BG, cx))
                        .text_size(style::text_10())
                        .child(HOST_ACCESS_COMMAND),
                )
                .child(
                    Clipboard::new(id)
                        .value(HOST_ACCESS_COMMAND)
                        .tooltip(t("copy")),
                ),
        )
}

/// `.popup`: `max-width: 500px`; the dialog takes pixels, not a rem length.
pub fn dialog_width() -> gpui_kit::Pixels {
    px(500.)
}
