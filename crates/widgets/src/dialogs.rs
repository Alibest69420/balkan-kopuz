//! Dialogs that live in a feature crate but are opened from shared rows; the app registers them at startup.

use std::rc::Rc;

use gpui_kit::{App, Global, Window};

type Opener = Rc<dyn Fn(String, bool, &mut Window, &mut App)>;

#[derive(Default)]
struct Dialogs {
    track_metadata: Option<Opener>,
}

impl Global for Dialogs {}

/// Register the track metadata dialog; `editable` says whether the source's tags can be written.
pub fn register_track_metadata(
    opener: impl Fn(String, bool, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    let dialogs = Dialogs {
        track_metadata: Some(Rc::new(opener)),
    };
    cx.set_global(dialogs);
}

pub fn open_track_metadata(key: String, editable: bool, window: &mut Window, cx: &mut App) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.track_metadata.clone());
    match opener {
        Some(opener) => opener(key, editable, window, cx),
        None => tracing::warn!("no track metadata dialog is registered"),
    }
}
