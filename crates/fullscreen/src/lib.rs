//! TODO(parity): implement against docs/PARITY.md.

use gpui_kit::{Context, IntoElement, Render, Window};
use widgets::EmptyState;

/// TODO(parity): see docs/PARITY.md for what this page owes.
pub struct FullscreenPlayer {}

impl FullscreenPlayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for FullscreenPlayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Fullscreen player").detail("Not implemented yet")
    }
}

/// TODO(parity): see docs/PARITY.md for what this page owes.
pub struct MiniPlayer {}

impl MiniPlayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for MiniPlayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Mini player").detail("Not implemented yet")
    }
}
