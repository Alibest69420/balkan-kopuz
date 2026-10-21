//! Settings → Interface → Home: which shelves the page draws, and in what order.

use components::format::t;
use components::settings::{order_row, section};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled, Window,
};
use store::LayoutModel;
use store::layout::{SHELVES, Section as Group};

/// Registered by the app at startup, beside the other Interface sections.
pub fn register(cx: &mut App) {
    settings_interface::register(
        "home",
        "customize_home",
        3,
        Some(Group::Home),
        |window, cx| cx.new(|cx| Shelves::new(window, cx)).into(),
        cx,
    );
}

struct Shelves;

impl Shelves {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&LayoutModel::global(cx), |_, _, cx| cx.notify())
            .detach();
        Self
    }
}

impl Render for Shelves {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let home = &store::layout(cx).home;
        let rows = SHELVES
            .iter()
            .filter_map(|name| {
                order_row(
                    "home",
                    name,
                    t(crate::shelf::title(name)),
                    home,
                    |layout| &mut layout.home,
                    cx,
                )
                .shown(cx)
            })
            .collect::<Vec<_>>();
        v_flex()
            .w_full()
            .child(section(t("home")).children(rows))
            .into_any_element()
    }
}
