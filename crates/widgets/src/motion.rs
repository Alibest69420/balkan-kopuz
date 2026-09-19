//! Kopuz's transitions in GPUI terms; `gpui_base::motion` does the work and honours reduce-motion for us.

use std::time::Duration;

use gpui_kit::base::motion::{Easing, Transition, transition};
use gpui_kit::{App, ElementId, Hsla, Pixels, Window};

/// Tailwind's bare `transition-*`, which is what most Kopuz hovers carry.
pub const FAST: Duration = Duration::from_millis(150);
/// `duration-200`.
pub const MEDIUM: Duration = Duration::from_millis(200);
/// `duration-300`, the carousels and the larger reveals.
pub const SLOW: Duration = Duration::from_millis(300);

/// Tailwind's default curve, `cubic-bezier(0.4, 0, 0.2, 1)`.
fn curve() -> Transition {
    Transition::new(FAST).easing(Easing::EaseInOut)
}

/// A colour on its way to `target`: `transition-colors` for one property.
///
/// `id` must be stable and unique within the element, since it keys the channel.
pub fn color(id: impl Into<ElementId>, target: Hsla, window: &mut Window, cx: &mut App) -> Hsla {
    transition(id.into(), target, curve(), window, cx)
}

/// The same over `duration`, for a `duration-300` hover or a slower fade.
pub fn color_over(
    id: impl Into<ElementId>,
    target: Hsla,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Hsla {
    transition(
        id.into(),
        target,
        Transition::new(duration).easing(Easing::EaseInOut),
        window,
        cx,
    )
}

/// A length on its way to `target`: the cover zoom a `group-hover:scale-105` reads as.
pub fn length(
    id: impl Into<ElementId>,
    target: Pixels,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    transition(
        id.into(),
        target,
        Transition::new(duration).easing(Easing::EaseOut),
        window,
        cx,
    )
}

/// A bare number on its way to `target`, for an opacity or a progress fraction.
pub fn amount(
    id: impl Into<ElementId>,
    target: f32,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    transition(
        id.into(),
        target,
        Transition::new(duration).easing(Easing::EaseInOut),
        window,
        cx,
    )
}

/// A cover's hovered size: `scale-105` over the frame it sits in, clipped by `overflow_hidden`.
pub fn zoom(
    id: impl Into<ElementId>,
    base: Pixels,
    hovered: bool,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    let target = if hovered { base * 1.05 } else { base };
    length(id, target, SLOW, window, cx)
}
