//! The frames the three first-run steps share.

use components::style;
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled, div};

pub(crate) fn hint(text: SharedString, cx: &App) -> Div {
    div().text_sm().text_color(style::slate_400(cx)).child(text)
}
