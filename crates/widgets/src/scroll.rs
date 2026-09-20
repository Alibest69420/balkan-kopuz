//! Smooth scrolling. GPUI turns a wheel notch into `line_height * delta` and applies it at once,
//! so the view teleports; here the drawn offset glides toward where the wheel has put it.

use std::time::Duration;

use gpui_kit::{App, ElementId, Pixels, ScrollHandle, UniformListScrollHandle, Window, point, px};

use crate::motion;

/// How long the view takes to catch up with the wheel. Long enough to read as motion, short
/// enough that the list still feels bolted to the pointer.
pub const GLIDE: Duration = Duration::from_millis(140);

/// Where the wheel has put this scroller, and what was last drawn for it.
#[derive(Default, Clone, Copy)]
struct Glide {
    target: Pixels,
    drawn: Pixels,
    started: bool,
}

/// Glides `handle` toward wherever the wheel left it. Call it from the render that draws the
/// scroller, before the element is built.
///
/// GPUI owns the wheel: its handler adds the delta straight to the offset and there is no
/// cancelling it from a listener. So the offset it writes is read as the *target*, the tween
/// supplies what is actually drawn, and the difference between the two is how much wheel
/// arrived since the last frame.
pub fn smooth(id: impl Into<ElementId>, handle: &ScrollHandle, window: &mut Window, cx: &mut App) {
    let id: ElementId = id.into();
    let glide = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-glide").into()),
        cx,
        |_, _| Glide::default(),
    );
    let state = *glide.read(cx);
    let offset = handle.offset();
    let limit = handle.max_offset().y;

    let mut target = match state.started {
        // Whatever GPUI added since the last frame is wheel the tween has not accounted for.
        true => state.target + (offset.y - state.drawn),
        false => offset.y,
    };
    // Offsets run negative going down, and `max_offset` is a magnitude.
    target = target.clamp(-limit, px(0.));

    let drawn = motion::length(id, target, GLIDE, window, cx);
    handle.set_offset(point(offset.x, drawn));
    if state.target != target || state.drawn != drawn || !state.started {
        glide.update(cx, |glide, _| {
            glide.target = target;
            glide.drawn = drawn;
            glide.started = true;
        });
    }
}

/// The same for a `uniform_list`, which keeps an ordinary handle underneath.
pub fn smooth_list(
    id: impl Into<ElementId>,
    handle: &UniformListScrollHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let base = handle.0.borrow().base_handle.clone();
    smooth(id, &base, window, cx);
}
