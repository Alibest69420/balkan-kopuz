//! Dialogs that live in a feature crate but are opened from shared rows; the app registers them at startup.

use std::rc::Rc;

use gpui_kit::{App, Global, SharedString, Window};

type MetadataOpener = Rc<dyn Fn(String, bool, &mut Window, &mut App)>;
type RenameOpener =
    Rc<dyn Fn(SharedString, SharedString, Rc<dyn Fn(String, &mut App)>, &mut Window, &mut App)>;
type FolderOpener = Rc<dyn Fn(String, &mut Window, &mut App)>;

#[derive(Default)]
struct Dialogs {
    track_metadata: Option<MetadataOpener>,
    rename: Option<RenameOpener>,
    folder_picker: Option<FolderOpener>,
}

impl Global for Dialogs {}

fn dialogs(cx: &mut App) -> &mut Dialogs {
    if cx.try_global::<Dialogs>().is_none() {
        cx.set_global(Dialogs::default());
    }
    cx.global_mut::<Dialogs>()
}

/// Register the track metadata dialog; `editable` says whether the source's tags can be written.
pub fn register_track_metadata(
    opener: impl Fn(String, bool, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    dialogs(cx).track_metadata = Some(Rc::new(opener));
}

/// Register the rename prompt: a title, the current name, and what to do with the new one.
pub fn register_rename(
    opener: impl Fn(SharedString, SharedString, Rc<dyn Fn(String, &mut App)>, &mut Window, &mut App)
    + 'static,
    cx: &mut App,
) {
    dialogs(cx).rename = Some(Rc::new(opener));
}

/// Register the "move this playlist to a folder" picker.
pub fn register_folder_picker(
    opener: impl Fn(String, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    dialogs(cx).folder_picker = Some(Rc::new(opener));
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

pub fn open_rename(
    title: impl Into<SharedString>,
    value: impl Into<SharedString>,
    save: impl Fn(String, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.rename.clone());
    match opener {
        Some(opener) => opener(title.into(), value.into(), Rc::new(save), window, cx),
        None => tracing::warn!("no rename dialog is registered"),
    }
}

pub fn open_folder_picker(playlist_id: String, window: &mut Window, cx: &mut App) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.folder_picker.clone());
    match opener {
        Some(opener) => opener(playlist_id, window, cx),
        None => tracing::warn!("no folder picker is registered"),
    }
}

/// Ask before something irreversible; `description` should name what goes, not just warn.
pub fn confirm_delete(
    title: SharedString,
    description: impl Into<SharedString>,
    delete: impl Fn(&mut Window, &mut App) + 'static,
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
            .on_ok(move |_, window, cx| {
                delete(window, cx);
                true
            })
    });
}
