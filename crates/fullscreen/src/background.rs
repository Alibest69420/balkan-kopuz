use std::path::PathBuf;

use api::ArtworkRef;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Div, ImageSource, ObjectFit, ParentElement as _, Styled, StyledImage as _, div, img, px,
};
use store::ConfigModel;

/// The art backdrop under a fullscreen or mini surface: a user image or the cover, darkened.
pub(crate) fn backdrop(artwork: Option<&ArtworkRef>, cx: &App) -> Option<Div> {
    let model = ConfigModel::global(cx);
    let config = model.read(cx);
    let source: ImageSource = if !config.config().custom_background_path.is_empty() {
        PathBuf::from(&config.config().custom_background_path).into()
    } else if config.config().cover_art_background {
        store::artwork_source(artwork?, true)
    } else {
        // TODO(palette): the album-art theme's gradient, extracted from the cover bytes.
        return None;
    };
    Some(paint(
        source,
        config.config().cover_art_darkening,
        config.config().cover_art_blur,
        cx,
    ))
}

/// The mini player is always art-backed, whatever `cover_art_background` says.
pub(crate) fn cover_backdrop(artwork: Option<&ArtworkRef>, cx: &App) -> Option<Div> {
    let model = ConfigModel::global(cx);
    let config = model.read(cx);
    let source = store::artwork_source(artwork?, true);
    Some(paint(
        source,
        config.config().cover_art_darkening.max(60),
        24,
        cx,
    ))
}

fn paint(source: ImageSource, darkening: u8, blur: u8, cx: &App) -> Div {
    let scrim = f32::from(darkening.min(95)) / 100.;
    // GPUI cannot blur an image, so the blur radius only overscans the art behind a heavier scrim.
    let overscan = px(f32::from(blur.min(100)) * 0.6);
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .overflow_hidden()
        .bg(cx.theme().background)
        .child(
            img(source)
                .absolute()
                .top(-overscan)
                .left(-overscan)
                .right(-overscan)
                .bottom(-overscan)
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
}
