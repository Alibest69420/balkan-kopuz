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

/// Ask before something irreversible. `description` should name what goes, not just warn.
///
/// Deleting an album or a track takes files off disk, so every such path goes through here.
pub fn confirm_delete(
    title: gpui_kit::SharedString,
    description: impl Into<gpui_kit::SharedString>,
    delete: impl Fn(&mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::component::button::ButtonVariant;
    use gpui_kit::component::dialog::DialogButtonProps;

    let description = description.into();
    let delete = Rc::new(delete);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let delete = delete.clone();
        alert
            .title(title.clone())
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .ok_text(crate::format::t("delete"))
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text(crate::format::t("cancel")),
            )
            .on_ok(move |_, _, cx| {
                delete(cx);
                true
            })
    });
}
