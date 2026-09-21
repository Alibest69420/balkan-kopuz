//! The sheet `?` opens: every key in the app, read straight off the registry.

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{App, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div, px};
use widgets::format::t;
use widgets::style;

const SHEET_WIDTH: Pixels = px(720.);
const SHEET_HEIGHT: Pixels = px(520.);
const COLUMNS: usize = 2;

/// Open it, or bring the one that is already open back to the front.
pub(crate) fn open(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |plate, _, cx| {
        plate
            .title(crate::title())
            .w(SHEET_WIDTH)
            .overlay_closable(true)
            .child(body(cx))
    });
}

fn body(cx: &App) -> impl IntoElement {
    let groups = crate::groups(cx);
    let mut columns: Vec<Vec<gpui_kit::AnyElement>> = (0..COLUMNS).map(|_| Vec::new()).collect();
    for (index, (title, entries)) in groups.into_iter().enumerate() {
        let block = v_flex()
            .w_full()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .child(t(title)),
            )
            .children(entries.into_iter().map(|entry| line(entry, cx)))
            .into_any_element();
        columns[index % COLUMNS].push(block);
    }
    v_flex().w_full().max_h(SHEET_HEIGHT).child(
        h_flex().w_full().gap_8().items_start().children(
            columns
                .into_iter()
                .map(|column| v_flex().flex_1().min_w_0().gap_5().children(column)),
        ),
    )
}

fn line(entry: crate::Entry, cx: &App) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_3()
        .py_0p5()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(entry.label),
        )
        .children(
            entry
                .keystroke
                .map(|keystroke| keycap(&keystroke, cx).into_any_element()),
        )
}

/// One keystroke, drawn as the keycap the palette's footer already uses.
pub(crate) fn keycap(keystroke: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .px_1p5()
        .rounded(style::radius(cx))
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .bg(style::white(style::RAISED, cx))
        .text_size(style::text_10())
        .text_color(style::white(style::TEXT_SECONDARY, cx))
        .child(commands::keycap(keystroke))
}
