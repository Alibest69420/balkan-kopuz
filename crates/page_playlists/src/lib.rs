//! TODO(parity): implement against docs/PARITY.md §2.9.

use gpui_kit::{Context, IntoElement, Render, SharedString, Window};
use widgets::EmptyState;

/// The playlists page: folders and playlists, or the flat remote layout.
pub struct Playlists {}

impl Playlists {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Playlists {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Playlists").detail("Not implemented yet")
    }
}

/// One playlist's tracks.
pub struct PlaylistDetail {
    id: SharedString,
}

impl PlaylistDetail {
    pub fn new(id: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self { id }
    }
}

impl Render for PlaylistDetail {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(format!("Playlist {}", self.id)).detail("Not implemented yet")
    }
}

/// The playlists inside one folder.
pub struct PlaylistFolder {
    id: SharedString,
}

impl PlaylistFolder {
    pub fn new(id: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self { id }
    }
}

impl Render for PlaylistFolder {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(format!("Folder {}", self.id)).detail("Not implemented yet")
    }
}
