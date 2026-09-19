//! TODO(parity): implement against docs/PARITY.md §2.3–2.5.

use gpui_kit::{Context, IntoElement, Render, SharedString, Window};
use widgets::EmptyState;

/// The active source's browse feed.
pub struct Discover {}

impl Discover {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Discover {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Discover").detail("Not implemented yet")
    }
}

/// One catalog playlist or album opened from the feed.
pub struct DiscoverPlaylist {
    id: SharedString,
    title: SharedString,
}

impl DiscoverPlaylist {
    pub fn new(
        id: SharedString,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = (window, cx);
        Self { id, title }
    }
}

impl Render for DiscoverPlaylist {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(self.title.clone()).detail(format!("Not implemented yet ({})", self.id))
    }
}

/// A remote artist profile.
pub struct DiscoverArtist {
    id: SharedString,
    name: SharedString,
}

impl DiscoverArtist {
    pub fn new(
        id: SharedString,
        name: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = (window, cx);
        Self { id, name }
    }
}

impl Render for DiscoverArtist {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(self.name.clone()).detail(format!("Not implemented yet ({})", self.id))
    }
}
