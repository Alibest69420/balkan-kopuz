//! What is behind the window: a wallpaper the user asked for, else the light the playing cover casts.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    AnyElement, App, ImageSource, IntoElement, ObjectFit, ParentElement as _, RenderImage, Styled,
    StyledImage as _, Window, div, img,
};
use store::ambient::Cast;
use store::{ConfigModel, PlayerModel};
use widgets::motion;

/// Slow on purpose: light that changes quickly reads as a fault, not as a new song.
const CROSSFADE: Duration = Duration::from_millis(1_200);

/// The field that is up, the one it is fading from, and a count that keys each change's own fade.
#[derive(Default)]
struct Shown {
    light: Option<Arc<RenderImage>>,
    from: Option<Arc<RenderImage>>,
    turn: u64,
}

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
        return Some(widgets::backdrop::wallpaper(source, darkening, blur, cx));
    }
    if cover {
        let artwork = PlayerModel::global(cx).read(cx).track()?.artwork.clone()?;
        let source = store::artwork_source(&artwork, true);
        return Some(widgets::backdrop::wallpaper(source, darkening, blur, cx));
    }
    ambient(window, cx)
}

fn ambient(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    // `None` is still rendering, so whatever is up stays up rather than flashing the bare theme.
    let wanted = match theme::ambient(cx) {
        None => Some(None),
        Some((artwork, spread)) => {
            match store::ambient::cast(&artwork, theme::ground(cx), spread, window, cx) {
                Cast::Pending => None,
                Cast::Nothing => Some(None),
                Cast::Light(field) => Some(Some(field)),
            }
        }
    };
    let shown = window.use_keyed_state("window-ambient", cx, |_, _| Shown::default());
    if let Some(wanted) = wanted {
        let changed = match (&shown.read(cx).light, &wanted) {
            (Some(up), Some(wanted)) => !Arc::ptr_eq(up, wanted),
            (None, None) => false,
            _ => true,
        };
        if changed {
            shown.update(cx, |shown, cx| {
                shown.from = shown.light.take();
                shown.light = wanted;
                shown.turn += 1;
                cx.notify();
            });
        }
    }

    let (light, from, turn) = {
        let shown = shown.read(cx);
        (shown.light.clone(), shown.from.clone(), shown.turn)
    };
    let arrived = motion::enter(motion::id("window-ambient", turn), CROSSFADE, window, cx);
    if arrived >= 1. && from.is_some() {
        shown.update(cx, |shown, _| shown.from = None);
    }
    if light.is_none() && from.is_none() {
        return None;
    }
    // An opaque field fades in over the old one; with nothing arriving, the old one fades out.
    let leaving = match light.is_some() {
        true => 1.,
        false => 1. - arrived,
    };
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(from.map(|field| stretched(field, leaving)))
            .children(light.map(|field| stretched(field, arrived)))
            .into_any_element(),
    )
}

/// The field is a colour field with no shape of its own, so it is stretched rather than fitted.
fn stretched(field: Arc<RenderImage>, opacity: f32) -> impl IntoElement {
    img(ImageSource::Render(field))
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .object_fit(ObjectFit::Fill)
        .opacity(opacity)
}
