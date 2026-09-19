//! The frames the Library and Connectivity panels share: sections, cards, folder rows, the host-access warning.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement as _, SharedString,
    Styled, Window, div,
};
use widgets::format::t;
use widgets::icons;

// Flathub refuses `org.freedesktop.Flatpak` in the manifest, so the user grants it by hand.
const HOST_ACCESS_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak moe.kopuz.kopuz";

/// A titled block; the caller appends the rows.
pub fn section(title: SharedString) -> Div {
    v_flex().w_full().gap_2().child(
        div()
            .text_lg()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title),
    )
}

/// The surface one settings group sits on.
pub fn card(cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().muted)
}

/// A row inside a card, one shade away from it.
pub fn row(cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_2()
        .p_2()
        .rounded(cx.theme().radius)
        .bg(cx.theme().background)
}

pub fn label(text: SharedString) -> Div {
    div().text_sm().child(text)
}

pub fn hint(text: SharedString, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

pub fn error(text: SharedString, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().danger).child(text)
}

/// The "Active" pill on whichever source the daemon is using.
pub fn active_badge(text: SharedString, cx: &App) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded(cx.theme().radius)
        .bg(cx.theme().primary)
        .text_xs()
        .text_color(cx.theme().primary_foreground)
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
        .px_2()
        .py_1()
        .rounded(cx.theme().radius)
        .bg(cx.theme().background)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(path),
        )
        .child(
            Button::new(id)
                .danger()
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
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .outline()
        .small()
        .icon(icons::icon(icons::PLUS))
        .label(t("add_folder"))
        .disabled(locked)
        .on_click(on_click)
}

/// Said when the daemon cannot reach a browser on the host; the command that fixes it is copyable.
pub fn host_access_warning(id: impl Into<ElementId>, message: SharedString, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_1()
        .p_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().warning)
        .child(div().text_xs().child(message))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(HOST_ACCESS_COMMAND),
                )
                .child(
                    Clipboard::new(id)
                        .value(HOST_ACCESS_COMMAND)
                        .tooltip(t("copy")),
                ),
        )
}
