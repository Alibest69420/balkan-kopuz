//! Where a hover listener goes when the thing you want to hover already has one.

use gpui_kit::{
    Div, Entity, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, div,
};

/// `control` in a frame that owns the hover, reporting into `state`.
///
/// An element carries one hover listener, and gpui-component spends a Button's
/// on a tooltip: `managed_tooltip_with_placement` installs it. A second is a
/// `debug_assert` panic in debug and a listener quietly dropped in release, so a
/// button that both says what it does and drives a tween has to be wrapped
/// rather than chained. Pair it with [`crate::motion::hovered`] for the state
/// and [`crate::motion::track`] is what this installs.
pub fn framed(id: &str, state: Entity<bool>, control: impl IntoElement) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("{id}-frame")))
        .flex()
        .on_hover(crate::motion::track(state))
        .child(control)
}
