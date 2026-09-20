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

/// How long a tween may take once the motion preference has had its say; `None` means snap.
pub fn span(duration: Duration, cx: &App) -> Option<Duration> {
    let scale = store::layout(cx).motion.scale();
    (scale > 0.).then(|| duration.mul_f32(scale))
}

/// A colour on its way to `target`: `transition-colors` for one property.
///
/// `id` must be stable and unique within the element, since it keys the channel.
pub fn color(id: impl Into<ElementId>, target: Hsla, window: &mut Window, cx: &mut App) -> Hsla {
    color_over(id, target, FAST, window, cx)
}

/// The same over `duration`, for a `duration-300` hover or a slower fade.
pub fn color_over(
    id: impl Into<ElementId>,
    target: Hsla,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Hsla {
    let Some(duration) = span(duration, cx) else {
        return target;
    };
    transition(
        id.into(),
        target,
        Transition::new(duration).easing(Easing::EaseInOut),
        window,
        cx,
    )
}

/// The accent `surface` wears, eased over a track change; `base` is what the theme would paint.
pub fn accent(surface: theme::Surface, base: Hsla, window: &mut Window, cx: &mut App) -> Hsla {
    let target = theme::accent_for(surface, window, cx).unwrap_or(base);
    color_over(
        id("accent", surface.key()),
        target,
        theme::ACCENT_TWEEN,
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
    let Some(duration) = span(duration, cx) else {
        return target;
    };
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
    let Some(duration) = span(duration, cx) else {
        return target;
    };
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
    zoom_over(id, base, hovered, SLOW, window, cx)
}

/// The same over `duration`, for the covers upstream marks `duration-500`.
pub fn zoom_over(
    id: impl Into<ElementId>,
    base: Pixels,
    hovered: bool,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    let target = if hovered { base * 1.05 } else { base };
    length(id, target, duration, window, cx)
}

/// `duration-500`, which several cover zooms upstream use.
pub const SLOWEST: Duration = Duration::from_millis(500);

/// A stable motion id from a prefix and a domain key, which is what every channel wants.
pub fn id(prefix: &str, key: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{prefix}-{key}").into())
}

/// The same for a row of a list, keyed by slot: a `NamedInteger` allocates nothing per frame.
pub fn row_id(prefix: &'static str, index: usize) -> ElementId {
    ElementId::from((prefix, index))
}

/// One of an element's channels, keyed off an id it already holds: a clone, not a `format!`.
pub fn channel(key: &gpui_kit::SharedString, channel: u64) -> ElementId {
    ElementId::NamedInteger(key.clone(), channel)
}

/// Fades in on the frame an element first renders, since a transition adopts its first target at once.
pub fn enter(
    id: impl Into<ElementId>,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let id: ElementId = id.into();
    // Without a tween there is nothing to fade, and the blank first frame would just flicker.
    if span(duration, cx).is_none() {
        return 1.;
    }
    let shown = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-entered").into()),
        cx,
        |_, _| false,
    );
    let entered = *shown.read(cx);
    if !entered {
        shown.update(cx, |shown, cx| {
            *shown = true;
            cx.notify();
        });
    }
    amount(id, if entered { 1. } else { 0. }, duration, window, cx)
}

/// `scale-105`, the square cards.
pub const ZOOM_SM: f32 = 1.05;
/// `scale-110`, the round artist card.
pub const ZOOM_LG: f32 = 1.10;

/// A cover's hovered size at an explicit factor, for the cards that zoom further.
pub fn zoom_by(
    id: impl Into<ElementId>,
    base: Pixels,
    factor: f32,
    hovered: bool,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    let target = if hovered { base * factor } else { base };
    length(id, target, duration, window, cx)
}

/// Hover as a value a tween can read; `.hover()` refines a style instead of reporting the state.
pub fn hovered(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> (bool, gpui_kit::Entity<bool>) {
    let state = window.use_keyed_state(id.into(), cx, |_, _| false);
    let hovered = *state.read(cx);
    (hovered, state)
}

/// The `on_hover` listener that keeps a [`hovered`] flag in step with the pointer.
pub fn track(
    state: gpui_kit::Entity<bool>,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static + use<> {
    move |over, _, cx| {
        let over = *over;
        state.update(cx, |flag, cx| {
            if *flag != over {
                *flag = over;
                cx.notify();
            }
        });
    }
}
