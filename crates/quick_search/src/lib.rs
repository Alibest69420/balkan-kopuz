//! TODO(parity): the Ctrl/Cmd+K search overlay (docs/PARITY.md §3.13).

use gpui_kit::{Context, IntoElement, Render, Window, div};

/// Rendered only while `Chrome::is_quick_search_open`.
pub struct QuickSearch {}

impl QuickSearch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for QuickSearch {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
