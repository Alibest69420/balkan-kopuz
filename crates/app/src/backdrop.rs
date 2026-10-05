//! What is behind the window: a wallpaper the user asked for, else the light the playing cover casts.

use std::path::PathBuf;

use gpui_kit::{AnyElement, App, ImageSource, Window};
use store::{ConfigModel, PlayerModel};

/// A chosen picture outranks the cover's, and either outranks the light: the order the settings promise.
pub(crate) fn layer(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (custom, cover, darkening, blur) = {
        let config = ConfigModel::global(cx);
        let config = config.read(cx).config();
        (
            config.custom_background_path.clone(),
            config.cover_art_background,
            config.cover_art_darkening,
            config.cover_art_blur,
        )
    };
    if !custom.is_empty() {
        let source: ImageSource = PathBuf::from(custom).into();
        return Some(components::backdrop::wallpaper(source, darkening, blur, cx));
    }
    if cover {
        let artwork = PlayerModel::global(cx).read(cx).track()?.artwork.clone()?;
        let source = store::artwork_source(&artwork, true);
        return Some(components::backdrop::wallpaper(source, darkening, blur, cx));
    }
    components::backdrop::ambient(window, cx)
}
