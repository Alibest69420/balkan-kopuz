//! TODO(parity): the navigation sidebar and source switcher (docs/PARITY.md §3.3, §3.4).

use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled, Window, div, px};
use widgets::EmptyState;

/// The navigation rail. It owns its own width, including the drag-to-resize and collapse behaviour.
pub struct Sidebar {}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .child(EmptyState::new("Sidebar").detail("Not implemented yet"))
    }
}
