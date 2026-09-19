//! Transport pieces both surfaces share; every action goes through the global models, so no view owns them.

use api::LoopMode;
use gpui_kit::base::slider::SliderState;
use gpui_kit::base::{Slider as BareSlider, SliderIndicator, SliderTrack};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Axis, Entity, IntoElement, ParentElement as _, Pixels, SharedString, Styled, div, px,
    relative,
};
use store::PlayerModel;
use widgets::format::{duration, t};
use widgets::{icons, style};

/// A slider over the 0..1 fraction of a track, since a `SliderState`'s range is fixed at build time.
pub(crate) fn fraction_state(step: f32) -> SliderState {
    SliderState::new().min(0.).max(1.).step(step)
}

pub(crate) fn fraction(position_ms: u64, duration_ms: Option<u64>) -> f32 {
    match duration_ms {
        Some(total) if total > 0 => (position_ms as f32 / total as f32).clamp(0., 1.),
        _ => 0.,
    }
}

/// `m:ss`, or the radio dash where there is no length to run against.
pub(crate) fn clock(ms: u64, radio: bool) -> SharedString {
    if radio { "--:--".into() } else { duration(ms) }
}

pub(crate) fn previous_button(id: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .icon(icons::icon(icons::SKIP_BACK))
        .on_click(|_, _, cx| PlayerModel::global(cx).update(cx, |player, cx| player.previous(cx)))
}

pub(crate) fn next_button(id: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .icon(icons::icon(icons::SKIP_FORWARD))
        .on_click(|_, _, cx| PlayerModel::global(cx).update(cx, |player, cx| player.next(cx)))
}

pub(crate) fn play_button(id: &'static str, playing: bool) -> Button {
    let icon = if playing { icons::PAUSE } else { icons::PLAY };
    // Kopuz's play button is a bare round glyph on both surfaces, not a filled accent pill.
    Button::new(id)
        .ghost()
        .rounded(px(999.))
        .icon(icons::icon(icon))
        .when(!playing, |button| button.tooltip(t("play")))
        .on_click(|_, _, cx| PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx)))
}

pub(crate) fn shuffle_button(shuffle: bool) -> Button {
    Button::new("fs-shuffle")
        .ghost()
        .rounded(px(999.))
        .icon(icons::icon(icons::SHUFFLE))
        .toggled(shuffle)
        .tooltip(if shuffle {
            t("shuffle_on")
        } else {
            t("shuffle_off")
        })
        .on_click(move |_, _, cx| {
            PlayerModel::global(cx).update(cx, |player, cx| player.set_shuffle(!shuffle, cx))
        })
}

pub(crate) fn repeat_button(mode: LoopMode) -> Button {
    let icon = match mode {
        LoopMode::Track => icons::REPEAT_ONE,
        _ => icons::REPEAT,
    };
    Button::new("fs-repeat")
        .ghost()
        .rounded(px(999.))
        .icon(icons::icon(icon))
        .toggled(mode != LoopMode::None)
        .tooltip(match mode {
            LoopMode::None => t("repeat_off"),
            LoopMode::Queue => t("repeat_queue"),
            LoopMode::Track => t("repeat_track"),
        })
        .on_click(|_, _, cx| {
            PlayerModel::global(cx).update(cx, |player, cx| player.cycle_loop_mode(cx))
        })
}

/// A track-thin seek bar: the component slider's thumb and 24px row are too tall for the mini player.
pub(crate) fn thin_slider(
    state: &Entity<SliderState>,
    height: Pixels,
    disabled: bool,
    cx: &App,
) -> impl IntoElement {
    let filled = state.read(cx).percentage().end;
    BareSlider::new(state)
        .axis(Axis::Horizontal)
        .disabled(disabled)
        .w_full()
        .child(
            SliderTrack::new(state)
                .axis(Axis::Horizontal)
                .disabled(disabled)
                .w_full()
                .h(height)
                .child(
                    SliderIndicator::new(state)
                        .relative()
                        .w_full()
                        .h_full()
                        .bg(style::white(style::HOVER, cx))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .h_full()
                                .w(relative(filled))
                                .bg(style::white(style::TEXT_STRONG, cx)),
                        ),
                ),
        )
}
