//! TODO(parity): implement against docs/PARITY.md §2.7.

use gpui_kit::{Context, IntoElement, Render, SharedString, Window};
use widgets::EmptyState;

/// The albums grid.
pub struct Albums {}

impl Albums {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Albums {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Albums").detail("Not implemented yet")
    }
}

/// One album: the standard detail, or the remote one when the source presents albums remotely.
pub struct AlbumDetail {
    id: SharedString,
}

impl AlbumDetail {
    pub fn new(id: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self { id }
    }
}

impl Render for AlbumDetail {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(format!("Album {}", self.id)).detail("Not implemented yet")
    }
}
