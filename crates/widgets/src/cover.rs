use std::time::Duration;

use api::ArtworkRef;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, IntoElement, ObjectFit, ParentElement as _, Pixels, RenderOnce, Styled,
    StyledImage as _, Window, div, img, px,
};

use crate::icons;
use crate::motion;
use crate::style;

/// A row, tile or hero cover. A row without artwork renders the placeholder and asks for nothing.
#[derive(IntoElement)]
pub struct Cover {
    artwork: Option<ArtworkRef>,
    size: Pixels,
    hq: bool,
    rounded: Option<Pixels>,
    placeholder: &'static str,
    zoom: Option<(ElementId, bool)>,
    zoom_speed: Duration,
    zoom_factor: f32,
}

impl Cover {
    pub fn new(artwork: Option<&ArtworkRef>) -> Self {
        Self {
            artwork: artwork.cloned(),
            size: px(40.),
            hq: false,
            rounded: None,
            placeholder: icons::MUSIC,
            zoom: None,
            zoom_speed: motion::SLOW,
            zoom_factor: motion::ZOOM_SM,
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
        self.rounded = Some(radius);
        self
    }

    /// The glyph shown when there is no picture: an artist gets a mic, a station a radio.
    pub fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }

    /// `group-hover:scale-105`: the picture grows inside its own clipped frame while `hovered`.
    pub fn zoom(mut self, id: impl Into<ElementId>, hovered: bool) -> Self {
        self.zoom = Some((id.into(), hovered));
        self
    }

    /// How far it zooms; `scale-105` unless the original says otherwise.
    pub fn zoom_factor(mut self, factor: f32) -> Self {
        self.zoom_factor = factor;
        self
    }

    /// How long that zoom takes; `duration-300` unless the original says `duration-500`.
    pub fn zoom_speed(mut self, duration: Duration) -> Self {
        self.zoom_speed = duration;
        self
    }
}

impl RenderOnce for Cover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // `rounded` on the thumb; a caller that wants a card or an avatar says so.
        let radius = self
            .rounded
            .unwrap_or_else(|| style::to_px(style::radius(cx), window));
        let glyph = px((self.size.as_f32() * 0.22).max(12.));
        let placeholder = self.placeholder;
        // The frame keeps its size and clips; only the picture inside it grows.
        let drawn = match self.zoom.clone() {
            Some((id, hovered)) => motion::zoom_by(
                id,
                self.size,
                self.zoom_factor,
                hovered,
                self.zoom_speed,
                window,
                cx,
            ),
            None => self.size,
        };
        div()
            .size(self.size)
            .rounded(radius)
            .overflow_hidden()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .bg(style::white(style::RAISED, cx))
            .map(|frame| match &self.artwork {
                Some(artwork) => frame.child(
                    img(store::artwork_source(artwork, self.hq))
                        .size(drawn)
                        .flex_shrink_0()
                        // The frame's mask is a rect, so the picture rounds its own corners.
                        .rounded(radius)
                        .object_fit(ObjectFit::Cover)
                        // A cover that will not load is otherwise a blank square, which reads as
                        // one that is merely slow.
                        .with_fallback(move || {
                            Icon::default()
                                .path(placeholder)
                                .size(glyph)
                                .into_any_element()
                        }),
                ),
                None => frame.child(
                    Icon::default()
                        .path(self.placeholder)
                        .size(glyph)
                        .text_color(style::white(style::TEXT_FAINT, cx)),
                ),
            })
    }
}
