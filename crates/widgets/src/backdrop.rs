//! A picture behind a whole surface: blurred, darkened, and read over rather than looked at.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, ImageSource, IntoElement, ObjectFit, ParentElement as _, Styled,
    StyledImage as _, div, img, relative,
};

/// `source` filling its parent, under a scrim of the theme's own background at `darkening` percent.
pub fn wallpaper(source: ImageSource, darkening: u8, blur: u8, cx: &App) -> AnyElement {
    let scrim = f32::from(darkening.min(95)) / 100.;
    // Overscanned by the blur radius, so the softened edge never shows as a seam.
    let scale = 1. + f32::from(blur.min(100)) * 0.004;
    let inset = (1. - scale) / 2.;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .overflow_hidden()
        .bg(cx.theme().background)
        .child(
            img(store::blurred_backdrop(source, blur))
                .absolute()
                .top(relative(inset))
                .left(relative(inset))
                .w(relative(scale))
                .h(relative(scale))
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(cx.theme().background.opacity(scrim)),
        )
        .into_any_element()
}
