//! TODO(parity): implement against docs/PARITY.md §2.8.

use gpui_kit::{Context, IntoElement, Render, SharedString, Window};
use widgets::EmptyState;

/// The artists grid.
pub struct Artists {}

impl Artists {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Artists {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Artists").detail("Not implemented yet")
    }
}

/// One artist from the library: tracks and albums.
pub struct ArtistDetail {
    name: SharedString,
}

impl ArtistDetail {
    pub fn new(name: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self { name }
    }
}

impl Render for ArtistDetail {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(format!("Artist {}", self.name)).detail("Not implemented yet")
    }
}
