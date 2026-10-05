use std::time::Duration;

use api::ArtworkRef;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ObjectFit, ParentElement as _, Pixels, RenderOnce,
    Styled, StyledImage as _, Window, div, img, linear_color_stop, linear_gradient, px, relative,
};

use crate::icons;
use crate::motion;
use crate::style;

/// A black wash rising from a cover's foot, so the text laid over it stays legible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scrim {
    /// The wash's opacity at the foot.
    pub foot: f32,
    /// Its opacity from `reach` up.
    pub top: f32,
    /// How far up the cover the wash fades, as a fraction of its height.
    pub reach: f32,
}

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
    fill: bool,
    scrim: Option<Scrim>,
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
            fill: false,
            scrim: None,
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

    /// Cover the positioned parent edge to edge, as a banner or a card's backdrop; no placeholder.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    /// A wash over the picture; a filled cover without artwork still draws it.
    pub fn scrim(mut self, scrim: Scrim) -> Self {
        self.scrim = Some(scrim);
        self
    }

    fn filled(self, window: &mut Window, cx: &mut App) -> AnyElement {
        // A fraction of the frame per side, so the bleed is a true scale whatever the frame's shape.
        let bleed = match self.zoom.clone() {
            Some((id, hovered)) => {
                let grown = motion::amount(
                    id,
                    if hovered { 1. } else { 0. },
                    self.zoom_speed,
                    window,
                    cx,
                );
                -(self.zoom_factor - 1.) / 2. * grown
            }
            None => 0.,
        };
        let hq = self.hq;
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .when_some(self.rounded, |frame, radius| frame.rounded(radius))
            .when_some(self.artwork, |frame, artwork| {
                frame.child(
                    img(store::artwork_source(&artwork, hq))
                        .absolute()
                        .top(relative(bleed))
                        .left(relative(bleed))
                        .right(relative(bleed))
                        .bottom(relative(bleed))
                        .object_fit(ObjectFit::Cover),
                )
            })
            .when_some(self.scrim, |frame, scrim| {
                frame.child(div().absolute().inset_0().bg(linear_gradient(
                    0.,
                    linear_color_stop(style::black(scrim.foot, cx), 0.),
                    linear_color_stop(style::black(scrim.top, cx), scrim.reach),
                )))
            })
            .into_any_element()
    }
}

impl RenderOnce for Cover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.fill {
            return self.filled(window, cx);
        }
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
                Some(artwork) => frame.child({
                    let faint = style::white(style::TEXT_FAINT, cx);
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
                                .text_color(faint)
                                .into_any_element()
                        })
                }),
                None => frame.child(
                    Icon::default()
                        .path(self.placeholder)
                        .size(glyph)
                        .text_color(style::white(style::TEXT_FAINT, cx)),
                ),
            })
            .into_any_element()
    }
}
