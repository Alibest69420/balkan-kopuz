//! TODO(parity): implement against docs/PARITY.md §2.2.

use gpui_kit::{Context, IntoElement, Render, SharedString, Window};
use widgets::EmptyState;

/// Search: the genre browser when the query is blank, results when it is not.
pub struct Search {}

impl Search {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self {}
    }
}

impl Render for Search {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("Search").detail("Not implemented yet")
    }
}

/// Every track of one genre.
pub struct Genre {
    name: SharedString,
}

impl Genre {
    pub fn new(name: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = (window, cx);
        Self { name }
    }
}

impl Render for Genre {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new(format!("Genre {}", self.name)).detail("Not implemented yet")
    }
}
