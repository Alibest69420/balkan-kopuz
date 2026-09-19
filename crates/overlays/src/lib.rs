//! TODO(parity): banners (scan, playback error, offline, update) and the download overlay (docs/PARITY.md §3.1, §3.15).

use gpui_kit::{Context, IntoElement, Render, Window, div};

/// Drawn over the routed page, so it paints nothing until it has something to say.
pub struct Overlays {}

impl Overlays {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Overlays {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
