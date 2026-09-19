//! TODO(parity): the player bar: transport, seek, volume, favourite and panel toggles (docs/PARITY.md §3.5, §3.10).

use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled, Window, div, px};
use widgets::EmptyState;

/// The bar across the bottom of the window. It owns its own height.
pub struct PlayerBar {}

impl PlayerBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for PlayerBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h(px(88.))
            .flex_shrink_0()
            .child(EmptyState::new("Player bar").detail("Not implemented yet"))
    }
}
