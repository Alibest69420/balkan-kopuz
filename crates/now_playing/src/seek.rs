//! The seek bar: a hair-thin paint in a hit area you can actually hit, with the buffer under it.

use gpui_kit::base::slider::{Slider as BareSlider, SliderIndicator, SliderTrack};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, Axis, Context, Hsla, InteractiveElement as _, IntoElement,
    MouseMoveEvent, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled, Window, div, px, relative,
};
use store::PlayerModel;
use widgets::format::duration;
use widgets::{motion, style};

use crate::{NowPlaying, buffer, clock, played_fraction, track_duration_ms};

// The paint, and the hit area around it: a 3px target is not a target.
const BAR: Pixels = px(3.);
const HIT: Pixels = px(18.);
const KNOB: Pixels = px(12.);
// The floating time that follows the pointer, and how far above the bar it sits.
const PREVIEW_LIFT: Pixels = px(26.);
const PREVIEW_WIDTH: Pixels = px(56.);
const SCAN_PERIOD: std::time::Duration = std::time::Duration::from_millis(1_400);
/// How much of the track the sweep covers; the rest is how far it has to travel.
const SCAN_WIDTH: f32 = 0.25;

impl NowPlaying {
    /// The clocks either side of the bar; the right one toggles between the length and what is left.
    pub(crate) fn seek_row(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (position_ms, total_ms) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (player.position_ms(), track_duration_ms(player))
        };
        let shown_ms = self.shown_ms(position_ms, total_ms);
        let dim = style::slate_500(cx);
        let width = clock::width(total_ms);

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                div()
                    .w(width)
                    .flex_shrink_0()
                    .text_right()
                    .text_size(style::text_10())
                    .font_features(clock::tabular())
                    .text_color(dim)
                    .child(duration(shown_ms)),
            )
            .child(div().flex_1().min_w_0().child(self.seek_bar(window, cx)))
            .child(
                div()
                    .id("seek-trailing-clock")
                    .w(width)
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_size(style::text_10())
                    .font_features(clock::tabular())
                    .text_color(dim)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.remaining = !this.remaining;
                        cx.notify();
                    }))
                    .child(clock::trailing(shown_ms, total_ms, self.remaining)),
            )
    }

    /// What the clocks read: where the pointer is holding the bar, else where the daemon is.
    pub(crate) fn shown_ms(&self, position_ms: u64, total_ms: Option<u64>) -> u64 {
        match (self.scrubbing, total_ms) {
            (Some(fraction), Some(total)) => (f64::from(fraction) * total as f64) as u64,
            _ => position_ms,
        }
    }

    pub(crate) fn seek_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (played, total_ms, loading, ranges, seekable) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (
                played_fraction(player),
                track_duration_ms(player),
                player.is_loading(),
                player.buffered().to_vec(),
                player.track().is_none_or(|track| track.seekable),
            )
        };
        let played = self.scrubbing.unwrap_or(played);
        let live = total_ms.is_some() && !loading && seekable;
        let bars = buffer::visual_ranges(&ranges, played * 100.);
        let scanning = loading && bars.is_empty();

        let fill = motion::accent(theme::Surface::Chrome, cx.theme().foreground, window, cx);
        let track = style::white(style::ACTIVE, cx);
        let buffered = style::white(style::TEXT_DIM, cx);
        let held = self.scrubbing.is_some() || self.preview.is_some();
        let knob = motion::length(
            "seek-knob",
            if held && live { KNOB } else { px(0.) },
            motion::FAST,
            window,
            cx,
        );

        div()
            .id("seek")
            .relative()
            .w_full()
            .h(HIT)
            .when(live, |hit| {
                hit.on_mouse_move(cx.listener(Self::preview_seek))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered && this.preview.take().is_some() {
                            cx.notify();
                        }
                    }))
            })
            .child(
                BareSlider::new(&self.seek)
                    .axis(Axis::Horizontal)
                    .disabled(!live)
                    .w_full()
                    .child(
                        SliderTrack::new(&self.seek)
                            .axis(Axis::Horizontal)
                            .disabled(!live)
                            .w_full()
                            .h(HIT)
                            .flex()
                            .items_center()
                            .child(
                                SliderIndicator::new(&self.seek)
                                    .relative()
                                    .w_full()
                                    .h(BAR)
                                    .rounded_full()
                                    .bg(track)
                                    .children(
                                        bars.into_iter()
                                            .map(|(left, right)| span(left, right, buffered)),
                                    )
                                    .when(scanning, |bar| bar.child(scan(fill)))
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .h_full()
                                            .w(relative(played))
                                            .rounded_full()
                                            .bg(fill),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .top(-(knob - BAR) / 2.)
                                            .left(relative(played))
                                            .ml(-knob / 2.)
                                            .size(knob)
                                            .rounded_full()
                                            .bg(fill),
                                    ),
                            ),
                    ),
            )
            .children(self.preview_label(total_ms, cx))
    }

    /// The time under the pointer, floating where the pointer is rather than parked on the bar.
    fn preview_label(
        &self,
        total_ms: Option<u64>,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let (fraction, x) = self.preview?;
        let total = total_ms?;
        let at: SharedString = duration((f64::from(fraction) * total as f64) as u64);
        Some(
            div()
                .absolute()
                .bottom(PREVIEW_LIFT)
                .left(x)
                .ml(-PREVIEW_WIDTH / 2.)
                .w(PREVIEW_WIDTH)
                .text_center()
                .text_size(style::text_10())
                .font_features(clock::tabular())
                .px_1()
                .py_0p5()
                .rounded(style::radius(cx))
                .bg(cx.theme().popover)
                .text_color(cx.theme().popover_foreground)
                .shadow_md()
                .child(at),
        )
    }

    fn preview_seek(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let bounds = self.seek.read(cx).bounds();
        if bounds.size.width <= px(0.) {
            return;
        }
        let local = (event.position.x - bounds.left()).clamp(px(0.), bounds.size.width);
        let fraction = f32::from(local) / f32::from(bounds.size.width);
        let next = Some((fraction, local));
        if self.preview != next {
            self.preview = next;
            cx.notify();
        }
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

/// Before any range arrives, a sweep says the fetch is live rather than stalled.
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
                // Both edges are held inside the track, so it grows in and out of the round ends
                // rather than being cut square against them: an overflow clip is a rectangle here.
                let lead = progress * (1. + SCAN_WIDTH);
                let (left, right) = ((lead - SCAN_WIDTH).max(0.), lead.min(1.));
                bar.left(relative(left)).w(relative(right - left))
            },
        )
}
