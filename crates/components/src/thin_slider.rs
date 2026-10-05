//! A hair-thin slider in a hit area you can actually hit: the paint the seek bar and the level share.

use gpui_kit::base::slider::{Slider as BareSlider, SliderIndicator, SliderState, SliderTrack};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, Axis, Entity, Hsla, IntoElement, ParentElement as _, Pixels,
    RenderOnce, SharedString, Styled as _, Window, div, px, relative,
};

use crate::{motion, style};

const KNOB_CHANNEL: u64 = 90;
const SCAN_PERIOD: std::time::Duration = std::time::Duration::from_millis(1_400);
/// How much of the track the sweep covers; the rest is how far it has to travel.
const SCAN_WIDTH: f32 = 0.25;

/// A slider's track, fill and knob over a `SliderState` the caller owns.
#[derive(IntoElement)]
pub struct ThinSlider {
    id: SharedString,
    state: Entity<SliderState>,
    filled: f32,
    hit: Pixels,
    bar: Pixels,
    knob: Pixels,
    show_knob: bool,
    disabled: bool,
    buffered: Vec<(f32, f32)>,
    scanning: bool,
    fill: Option<Hsla>,
}

impl ThinSlider {
    pub fn new(id: impl Into<SharedString>, state: &Entity<SliderState>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            filled: 0.,
            hit: px(16.),
            bar: px(3.),
            knob: px(10.),
            show_knob: false,
            disabled: false,
            buffered: Vec::new(),
            scanning: false,
            fill: None,
        }
    }

    /// How much of the track is filled, as a 0..1 fraction.
    pub fn filled(mut self, filled: f32) -> Self {
        self.filled = filled.clamp(0., 1.);
        self
    }

    /// The hit area's height, the paint's, and the knob's diameter when it is out.
    pub fn metrics(mut self, hit: Pixels, bar: Pixels, knob: Pixels) -> Self {
        self.hit = hit;
        self.bar = bar;
        self.knob = knob;
        self
    }

    /// Whether the knob is out, which it grows to and shrinks from rather than snapping.
    pub fn knob(mut self, shown: bool) -> Self {
        self.show_knob = shown;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Ranges already fetched, as `(left, right)` percentages of the track.
    pub fn buffered(mut self, ranges: Vec<(f32, f32)>) -> Self {
        self.buffered = ranges;
        self
    }

    /// A sweep across the track, saying a fetch is live rather than stalled.
    pub fn scanning(mut self, scanning: bool) -> Self {
        self.scanning = scanning;
        self
    }

    /// The fill and knob colour; the chrome's accent unless set.
    pub fn fill(mut self, fill: Hsla) -> Self {
        self.fill = Some(fill);
        self
    }
}

impl RenderOnce for ThinSlider {
    fn render(self, window: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        let fill = self.fill.unwrap_or_else(|| {
            motion::accent(theme::Surface::Chrome, cx.theme().foreground, window, cx)
        });
        let track = style::white(style::ACTIVE, cx);
        let spans = style::white(style::TEXT_DIM, cx);
        let knob = motion::length(
            motion::channel(&self.id, KNOB_CHANNEL),
            if self.show_knob && !self.disabled {
                self.knob
            } else {
                px(0.)
            },
            motion::FAST,
            window,
            cx,
        );
        let (bar, filled) = (self.bar, self.filled);

        BareSlider::new(&self.state)
            .axis(Axis::Horizontal)
            .disabled(self.disabled)
            .w_full()
            .child(
                SliderTrack::new(&self.state)
                    .axis(Axis::Horizontal)
                    .disabled(self.disabled)
                    .w_full()
                    .h(self.hit)
                    .flex()
                    .items_center()
                    .child(
                        SliderIndicator::new(&self.state)
                            .relative()
                            .w_full()
                            .h(bar)
                            .rounded_full()
                            .bg(track)
                            .children(
                                self.buffered
                                    .into_iter()
                                    .map(|(left, right)| span(left, right, spans)),
                            )
                            .when(self.scanning, |bar| bar.child(scan(fill)))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .h_full()
                                    .w(relative(filled))
                                    .rounded_full()
                                    .bg(fill),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(-(knob - bar) / 2.)
                                    .left(relative(filled))
                                    .ml(-knob / 2.)
                                    .size(knob)
                                    .rounded_full()
                                    .bg(fill),
                            ),
                    ),
            )
    }
}

/// One buffered range, as a fraction of the track it covers.
fn span(left: f32, right: f32, color: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .h_full()
        .left(relative(left / 100.))
        .w(relative((right - left).max(0.) / 100.))
        .rounded_full()
        .bg(color)
}

fn scan(color: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .h_full()
        .rounded_full()
        .bg(color)
        .with_animation(
            "seek-scan",
            Animation::new(SCAN_PERIOD).repeat(),
            |bar, progress| {
                // Both edges stay inside the track, since an overflow clip is a rectangle here.
                let lead = progress * (1. + SCAN_WIDTH);
                let (left, right) = ((lead - SCAN_WIDTH).max(0.), lead.min(1.));
                bar.left(relative(left)).w(relative(right - left))
            },
        )
}
