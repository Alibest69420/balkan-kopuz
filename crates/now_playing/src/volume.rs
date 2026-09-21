//! The volume control: a perceptual curve at the UI edge, a linear level on the wire.

use gpui_kit::base::slider::{Slider as BareSlider, SliderIndicator, SliderTrack};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    Axis, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    ScrollDelta, ScrollWheelEvent, StatefulInteractiveElement as _, Styled, Window, div, px,
    relative,
};
use store::{ConfigModel, PlayerModel};
use widgets::format::t;
use widgets::{focus, icons, motion, style};

use crate::{NowPlaying, channel};

// `w-24`, the width the bar gives the level.
const TRACK_WIDTH: Pixels = px(96.);
// A 3px paint in a hit area tall enough to catch a pointer aiming at it.
const TRACK_HIT: Pixels = px(16.);
const TRACK_BAR: Pixels = px(3.);
const KNOB: Pixels = px(10.);
const GLYPH: Pixels = px(14.);
// A wheel notch this many pixels of travel; a trackpad flick used to arrive as one step per event.
const PIXELS_PER_NOTCH: f32 = 60.;
// However hard the flick, one event never moves the level more than this many notches.
const MAX_NOTCHES: f32 = 3.;
const SILENT: f32 = f32::EPSILON;

/// The wire level a slider reading of `shown` means; loudness is cubic, not linear.
pub fn to_wire(shown: f32) -> f32 {
    let shown = shown.clamp(0., 1.);
    shown * shown * shown
}

/// Where a wire level sits on the slider.
pub fn to_ui(level: f32) -> f32 {
    level.clamp(0., 1.).cbrt()
}

fn glyph_for(level: f32) -> &'static str {
    if level <= SILENT {
        icons::VOLUME_MUTED
    } else if to_ui(level) < 0.5 {
        icons::VOLUME_LOW
    } else {
        icons::VOLUME
    }
}

impl NowPlaying {
    /// The mute glyph and the level beside it, sharing one wheel.
    pub(crate) fn volume_control(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let level = PlayerModel::global(cx).read(cx).volume();
        let tint = self.control_glyph("mute", style::slate_400(cx), false, window, cx);
        let fill = motion::accent(theme::Surface::Chrome, cx.theme().foreground, window, cx);

        h_flex()
            .id("volume")
            .gap_2()
            .items_center()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.scroll_volume(event, window, cx)
            }))
            .child(
                self.control_slot("mute", style::chrome().round_button, window, cx)
                    .child(
                        focus::quiet(Button::new("mute"), cx)
                            .ghost()
                            .with_size(GLYPH * (4. / 3.))
                            .size(style::chrome().round_button)
                            .rounded(style::chrome().round_button / 2.)
                            .icon(icons::icon(glyph_for(level)))
                            .custom(style::bare_button(tint, cx))
                            .tooltip(t("volume"))
                            .on_click(|_, _, cx| {
                                PlayerModel::global(cx)
                                    .update(cx, |player, cx| player.toggle_mute(cx))
                            }),
                    ),
            )
            .child(div().w(TRACK_WIDTH).child(self.volume_bar(fill, cx)))
    }

    fn volume_bar(&self, fill: Hsla, cx: &Context<Self>) -> impl IntoElement + use<> {
        let filled = self.volume.read(cx).percentage().end;
        let track = style::white(style::ACTIVE, cx);
        BareSlider::new(&self.volume)
            .axis(Axis::Horizontal)
            .w_full()
            .child(
                SliderTrack::new(&self.volume)
                    .axis(Axis::Horizontal)
                    .w_full()
                    .h(TRACK_HIT)
                    .flex()
                    .items_center()
                    .child(
                        SliderIndicator::new(&self.volume)
                            .relative()
                            .w_full()
                            .h(TRACK_BAR)
                            .rounded_full()
                            .bg(track)
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
                                    .top(-(KNOB - TRACK_BAR) / 2.)
                                    .left(relative(filled))
                                    .ml(-KNOB / 2.)
                                    .size(KNOB)
                                    .rounded_full()
                                    .bg(fill),
                            ),
                    ),
            )
    }

    /// A wheel over the control, in notches: the sub-notch remainder is kept for the next event.
    fn scroll_volume(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = ConfigModel::global(cx)
            .read(cx)
            .config()
            .volume_scroll_step
            .max(0.);
        if step <= 0. {
            return;
        }
        let notches = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.y) / PIXELS_PER_NOTCH,
            ScrollDelta::Lines(delta) => delta.y,
        };
        self.wheel = (self.wheel + notches).clamp(-MAX_NOTCHES, MAX_NOTCHES);
        let whole = self.wheel.trunc();
        if whole == 0. {
            return;
        }
        self.wheel -= whole;
        let shown = to_ui(PlayerModel::global(cx).read(cx).volume());
        let next = (shown + whole * step).clamp(0., 1.);
        self.volume
            .update(cx, |state, cx| state.set_value(next, window, cx));
        PlayerModel::global(cx).update(cx, |player, cx| player.set_volume_saved(to_wire(next), cx));
    }

    /// The wash under one control: `hover:bg-white/10`, driven by tracked hover rather than refined.
    pub(crate) fn control_slot(
        &self,
        id: &'static str,
        size: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let wash = if self.hovered == Some(id) {
            style::white(style::HOVER, cx)
        } else {
            cx.theme().transparent
        };
        div()
            .id(channel(id, "slot"))
            .flex_shrink_0()
            .rounded(size / 2.)
            .bg(motion::color(channel(id, "wash"), wash, window, cx))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hovered(id, *hovered, cx)),
            )
    }

    /// A control's glyph colour: a lit one holds its accent, the rest lift to the foreground on hover.
    pub(crate) fn control_glyph(
        &self,
        id: &'static str,
        resting: Hsla,
        lit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Hsla {
        let target = if !lit && self.hovered == Some(id) {
            cx.theme().foreground
        } else {
            resting
        };
        motion::color(channel(id, "glyph"), target, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{to_ui, to_wire};

    #[test]
    fn the_middle_of_the_slider_is_not_half_the_power() {
        assert!(to_wire(0.5) < 0.2);
    }

    #[test]
    fn the_ends_are_where_they_look() {
        assert_eq!(to_wire(0.), 0.);
        assert_eq!(to_wire(1.), 1.);
    }

    #[test]
    fn a_level_round_trips_through_the_curve() {
        for level in [0., 0.05, 0.3, 0.75, 1.] {
            assert!((to_wire(to_ui(level)) - level).abs() < 1e-5);
        }
    }
}
