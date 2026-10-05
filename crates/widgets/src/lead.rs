//! A row's lead: its number, which steps aside for a play glyph while the row is under the pointer.

use gpui_kit::{
    AnyElement, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, Styled as _, div,
};

use crate::icons;

/// The number and the glyph that replaces it on hover; the cell they go in has to be `relative`.
pub fn number_or_play(
    group: &'static str,
    number: impl Into<SharedString>,
    glyph: Pixels,
    color: Hsla,
) -> [AnyElement; 2] {
    [
        div()
            .text_xs()
            .text_color(color)
            .group_hover(group, |number| number.invisible())
            .child(number.into())
            .into_any_element(),
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .invisible()
            .group_hover(group, |play| play.visible())
            .child(icons::icon(icons::PLAY).size(glyph).text_color(color))
            .into_any_element(),
    ]
}
