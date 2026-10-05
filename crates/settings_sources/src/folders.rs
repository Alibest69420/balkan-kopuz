//! A source's folders: the `directories` row it publishes among its settings.

use api::SourceInfo;
use api::schema::{FieldValue, decode_directories, encode_directories, spec_value};
use gpui_kit::{App, PathPromptOptions};
use link::Link;
use store::{Notices, SourcesModel};

/// The settings key a source's folders are published and answered under.
pub const KEY: &str = "directories";

/// The folders `source` holds, as its settings row reads them.
pub fn of(source: &SourceInfo) -> Vec<String> {
    spec_value(&source.settings, KEY)
        .map(decode_directories)
        .unwrap_or_default()
}

/// Replace a source's folders; the sources refetch is what puts the answer on screen.
pub fn write(id: String, directories: Vec<String>, cx: &mut App) {
    let value = FieldValue::new(KEY.to_string(), encode_directories(&directories));
    let call = Link::global(cx)
        .call(move |api| async move { api.set_source_settings(id, vec![value]).await });
    cx.spawn(async move |cx| match call.await {
        Ok(_) => {
            cx.update(|cx| {
                SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
            });
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

/// Ask the platform for folders and hand `apply` whatever was picked.
pub fn pick(multiple: bool, cx: &mut App, apply: impl FnOnce(Vec<String>, &mut App) + 'static) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple,
        prompt: None,
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = paths.await else {
            return;
        };
        let picked: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        if picked.is_empty() {
            return;
        }
        cx.update(|cx| apply(picked, cx));
    })
    .detach();
}
