//! TODO(parity): implement against docs/PARITY.md.

use gpui_kit::{Context, IntoElement, Render, Window};
use widgets::EmptyState;

/// TODO(parity): see docs/PARITY.md for what this page owes.
pub struct SidePanelView {}

impl SidePanelView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for SidePanelView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Queue and lyrics").detail("Not implemented yet")
    }
}
