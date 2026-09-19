//! TODO(parity): the track metadata dialog, read-only or editable per `capabilities().edit_tags` (docs/PARITY.md §5.5).

use gpui_kit::App;

/// Register the dialog so a track row's "View metadata" can open it.
pub fn register(cx: &mut App) {
    widgets::dialogs::register_track_metadata(
        |key, editable, window, cx| {
            let _ = (key, editable, window, cx);
            tracing::warn!("the metadata dialog is not implemented yet");
        },
        cx,
    );
}
