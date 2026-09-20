//! What a view draws instead of content: a read that failed, said plainly, with the button that runs it again.

use api::{ApiError, ErrorCode};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, IntoElement, ParentElement as _, SharedString, Styled, Window,
    div, px,
};
use widgets::format::{t, t_with};
use widgets::{icons, style};

/// What a catalog failure says; a source nobody signed into is the one case worth wording ourselves.
pub fn catalog_failure(error: &ApiError) -> SharedString {
    if error.code == ErrorCode::SourceAuthExpired {
        return t("source_anon_discover");
    }
    t_with("discover_failed", &[("error", error.to_string())])
}

/// What a library read's failure says: the daemon's own sentence, not a Rust enum.
pub fn read_failure(error: &ApiError) -> SharedString {
    t_with(
        "discover_read_failed",
        &[("error", store::human(error).to_string())],
    )
}

/// A failed read with its retry, so a page that could not load is never drawn as an empty one.
pub fn failed(
    id: &'static str,
    message: SharedString,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    v_flex()
        .w_full()
        .items_center()
        .justify_center()
        .gap_4()
        .py_16()
        .px_6()
        .text_center()
        .child(
            icons::icon(icons::WARNING)
                .size(px(32.))
                .text_color(cx.theme().danger),
        )
        .child(
            div()
                .max_w(px(420.))
                .text_sm()
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .child(message),
        )
        .child(
            Button::new(id)
                .small()
                .custom(style::button(
                    style::white(style::RAISED, cx),
                    style::white(style::HOVER, cx),
                    cx.theme().foreground,
                    cx,
                ))
                .icon(icons::icon(icons::REFRESH))
                .label(t("retry_now"))
                .on_click(retry),
        )
        .into_any_element()
}
