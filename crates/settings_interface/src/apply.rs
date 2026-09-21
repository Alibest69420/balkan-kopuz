//! Making `layout.interface` real: the rem size every `rems(…)` in the app resolves against, and the face.

use std::cell::RefCell;

use gpui_kit::component::Theme;
use gpui_kit::{App, Global, Pixels, SharedString};
use store::LayoutModel;
use store::layout::SCALE_RANGE;

/// What this module last wrote, and the theme's own values under it.
#[derive(Default)]
struct Applied {
    wrote: RefCell<Option<(Pixels, SharedString)>>,
    base: RefCell<Option<(Pixels, SharedString)>>,
}

impl Global for Applied {}

/// Follow `layout.interface` from here on, re-applying it over whatever a theme change installs.
pub fn watch(cx: &mut App) {
    cx.set_global(Applied::default());
    resolve(cx);
    cx.observe(&LayoutModel::global(cx), |_, cx| resolve(cx))
        .detach();
    // `Root::render` pushes the theme's font size into the window every frame, so this is the lever.
    cx.observe_global::<Theme>(resolve).detach();
}

fn resolve(cx: &mut App) {
    skin(cx);
    apply(cx);
}

/// Resolve `layout.skin` once per change, and hand the component library the radii it draws with.
fn skin(cx: &mut App) {
    widgets::skin::select(&store::layout(cx).skin);
    let metrics = widgets::skin::metrics();
    let theme = Theme::global(cx);
    if theme.radius == metrics.thumb_radius && theme.radius_lg == metrics.cover_radius {
        return;
    }
    let theme = Theme::global_mut(cx);
    theme.radius = metrics.thumb_radius;
    theme.radius_lg = metrics.cover_radius;
    cx.refresh_windows();
}

fn apply(cx: &mut App) {
    let current = {
        let theme = Theme::global(cx);
        (theme.font_size, theme.font_family.clone())
    };
    let layout = store::layout(cx);
    let scale = layout
        .interface
        .scale
        .clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
    let font = layout.interface.font.clone();
    let Some(applied) = cx.try_global::<Applied>() else {
        return;
    };
    // Anything this module did not write is the theme's own answer, and so the new baseline.
    if applied.wrote.borrow().as_ref() != Some(&current) {
        *applied.base.borrow_mut() = Some(current.clone());
    }
    let (base_size, base_family) = applied
        .base
        .borrow()
        .clone()
        .unwrap_or_else(|| current.clone());
    let wanted = (
        base_size * scale,
        font.map(SharedString::from).unwrap_or(base_family),
    );
    *applied.wrote.borrow_mut() = Some(wanted.clone());
    if wanted == current {
        return;
    }
    let theme = Theme::global_mut(cx);
    theme.font_size = wanted.0;
    theme.font_family = wanted.1;
    cx.refresh_windows();
}
