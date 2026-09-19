use api::ArtworkRef;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, IntoElement, ObjectFit, ParentElement as _, Pixels, RenderOnce, Styled, StyledImage as _,
    Window, div, img, px,
};

use crate::icons;

/// A row, tile or hero cover. A row without artwork renders the placeholder and asks for nothing.
#[derive(IntoElement)]
pub struct Cover {
    artwork: Option<ArtworkRef>,
    size: Pixels,
    hq: bool,
    rounded: Pixels,
    placeholder: &'static str,
}

impl Cover {
    pub fn new(artwork: Option<&ArtworkRef>) -> Self {
        Self {
            artwork: artwork.cloned(),
            size: px(40.),
            hq: false,
            rounded: px(4.),
            placeholder: icons::MUSIC,
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        // Past a thumbnail, ask for the full-size picture.
        self.hq = size > px(512.);
        self
    }

    /// Ask for the full-size picture regardless of the rendered size.
    pub fn hq(mut self, hq: bool) -> Self {
        self.hq = hq;
        self
    }

    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.rounded = radius;
        self
    }

    /// The glyph shown when there is no picture: an artist gets a mic, a station a radio.
    pub fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }
}

impl RenderOnce for Cover {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .size(self.size)
            .rounded(self.rounded)
            .overflow_hidden()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .bg(cx.theme().secondary)
            .text_color(cx.theme().muted_foreground)
            .map(|frame| match &self.artwork {
                Some(artwork) => frame.child(
                    img(store::artwork_source(artwork, self.hq))
                        .size(self.size)
                        .object_fit(ObjectFit::Cover),
                ),
                None => frame.child(
                    Icon::default()
                        .path(self.placeholder)
                        .size(self.size * 0.45),
                ),
            })
    }
}
