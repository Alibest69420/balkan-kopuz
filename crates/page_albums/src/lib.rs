//! The albums grid and both album detail styles.

mod detail;
mod grid;
mod ops;
mod sort;

use components::style;
use gpui_kit::component::button::ButtonCustomVariant;
use gpui_kit::{App, Hsla};

pub use detail::AlbumDetail;
pub use grid::Albums;

/// A Button that paints nothing of its own, so the frame around it can own the tween.
fn clear_variant(ink: Hsla, cx: &App) -> ButtonCustomVariant {
    style::bare_button(ink, cx)
}
