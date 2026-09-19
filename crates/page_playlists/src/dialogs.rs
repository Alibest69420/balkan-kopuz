//! The dialogs the playlists page opens: add playlist, rename, move to folder, and the delete confirmations.

use std::rc::Rc;

use api::PlaylistCapability;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogButtonProps, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};
use store::{PlaylistsModel, SourcesModel, actions};
use widgets::format::t;
use widgets::{icons, style};

/// `.popup`'s `min-width: 400px`.
const DIALOG_WIDTH: gpui_kit::Pixels = px(400.);
/// `w-80`: the narrower prompts the Dioxus page opens.
const PROMPT_WIDTH: gpui_kit::Pixels = px(320.);

/// Ask for a name and create a playlist, or import a directory as one.
struct AddPlaylist {
    name: Entity<InputState>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl AddPlaylist {
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        if name.trim().is_empty() {
            return;
        }
        // A source that cannot mutate playlists gets the friendly line, not a raw refusal.
        if SourcesModel::global(cx).read(cx).capabilities().playlists == PlaylistCapability::None {
            self.error = Some(t("error_server_not_configured"));
            cx.notify();
            return;
        }
        PlaylistsModel::global(cx)
            .update(cx, |playlists, cx| playlists.create(name, Vec::new(), cx));
        window.close_dialog(cx);
    }

    fn import_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        window.close_dialog(cx);
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let raw = path.to_string_lossy().to_string();
            let prefix = if raw.ends_with(std::path::MAIN_SEPARATOR) {
                raw
            } else {
                format!("{raw}{}", std::path::MAIN_SEPARATOR)
            };
            cx.update(|cx| {
                actions::run(cx, move |api| async move {
                    let page = api.folder_tracks(prefix, store::all()).await?;
                    let keys = page.items.into_iter().map(|track| track.key).collect();
                    api.create_playlist(name, keys).await
                });
            });
        })
        .detach();
    }
}

impl Render for AddPlaylist {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folders = SourcesModel::global(cx).read(cx).capabilities().folders;
        v_flex()
            .gap_4()
            .w(DIALOG_WIDTH)
            .when_some(self.error.clone(), |column, error| {
                column.child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(px(6.))
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .bg(cx.theme().danger.opacity(0.1))
                        .border_1()
                        .border_color(cx.theme().danger.opacity(0.3))
                        .child(error),
                )
            })
            .child(Input::new(&self.name))
            .when(folders, |column| {
                column.child(
                    Button::new("import-folder")
                        .outline()
                        .text_sm()
                        .icon(icons::icon(icons::FOLDER))
                        .label(t("add_folder"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.import_folder(window, cx)),
                        ),
                )
            })
    }
}

/// Open the add-playlist dialog.
pub(crate) fn open_add_playlist(window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| {
        let name =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("playlist_name_placeholder")));
        let subscription = cx.subscribe_in(
            &name,
            window,
            |this: &mut AddPlaylist, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.save(window, cx),
                InputEvent::Change => {
                    this.error = None;
                    cx.notify();
                }
                _ => {}
            },
        );
        AddPlaylist {
            name,
            error: None,
            _subscriptions: vec![subscription],
        }
    });
    window.open_dialog(cx, move |plate, _, _| {
        let dialog = dialog.clone();
        plate
            .title(t("add_playlist"))
            .w(DIALOG_WIDTH)
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .gap_2()
                    .child(DialogClose::new().child(Button::new("cancel").label(t("cancel"))))
                    .child(Button::new("save").primary().label(t("save")).on_click(
                        move |_, window, cx| {
                            dialog.update(cx, |this, cx| this.save(window, cx));
                        },
                    )),
            )
    });
}

type Save = Rc<dyn Fn(String, &mut App)>;

/// A one-field prompt: Enter or Save hands the trimmed name to `save`.
struct TextPrompt {
    value: Entity<InputState>,
    save: Save,
    _subscriptions: Vec<Subscription>,
}

impl TextPrompt {
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.value.read(cx).value().trim().to_string();
        if value.is_empty() {
            return;
        }
        (self.save)(value, cx);
        window.close_dialog(cx);
    }
}

impl Render for TextPrompt {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(PROMPT_WIDTH).child(Input::new(&self.value))
    }
}

/// Open a rename prompt titled `title`, starting from `value`.
pub(crate) fn open_rename(
    title: SharedString,
    value: impl Into<SharedString>,
    save: impl Fn(String, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let value = value.into();
    let prompt = cx.new(|cx| {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut TextPrompt, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(window, cx);
                }
            },
        );
        TextPrompt {
            value: input,
            save: Rc::new(save),
            _subscriptions: vec![subscription],
        }
    });
    window.open_dialog(cx, move |plate, _, _| {
        let prompt = prompt.clone();
        plate
            .title(title.clone())
            .w(PROMPT_WIDTH)
            .child(prompt.clone())
            .footer(
                DialogFooter::new()
                    .gap_2()
                    .child(DialogClose::new().child(Button::new("cancel").label(t("cancel"))))
                    .child(Button::new("save").primary().label(t("save")).on_click(
                        move |_, window, cx| {
                            prompt.update(cx, |this, cx| this.submit(window, cx));
                        },
                    )),
            )
    });
}

/// Pick the folder a playlist moves into, or make one on the spot.
struct FolderPicker {
    playlist_id: String,
    name: Entity<InputState>,
    creating: bool,
    _subscriptions: Vec<Subscription>,
}

impl FolderPicker {
    fn move_into(&mut self, folder: String, window: &mut Window, cx: &mut Context<Self>) {
        let playlist = self.playlist_id.clone();
        actions::run(cx, move |api| async move {
            api.move_playlist(playlist, Some(folder)).await
        });
        window.close_dialog(cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let playlist = self.playlist_id.clone();
        actions::run(cx, move |api| async move {
            let folder = api.create_playlist_folder(name).await?;
            api.move_playlist(playlist, Some(folder)).await
        });
        window.close_dialog(cx);
    }
}

impl Render for FolderPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folders = PlaylistsModel::global(cx).read(cx).folders().to_vec();
        v_flex()
            .gap_3()
            .w(PROMPT_WIDTH)
            .when(folders.is_empty(), |column| {
                column.child(
                    div()
                        .text_sm()
                        .text_color(style::slate_500(cx))
                        .child(t("no_folders_yet")),
                )
            })
            .child(
                v_flex()
                    .id("folder-list")
                    .gap_1()
                    .max_h_48()
                    .overflow_y_scroll()
                    .children(folders.into_iter().map(|folder| {
                        let id = folder.id.clone();
                        h_flex()
                            .id(SharedString::from(format!("folder-{}", folder.id)))
                            .gap_2()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .rounded(style::radius_lg())
                            .hover(|row| row.bg(style::white(style::HOVER, cx)))
                            .child(
                                icons::icon(icons::FOLDER)
                                    .size(px(12.))
                                    .text_color(cx.theme().warning),
                            )
                            .child(folder.name.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.move_into(id.clone(), window, cx)
                            }))
                    })),
            )
            .when(self.creating, |column| {
                column.child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&self.name)))
                        .child(
                            Button::new("create-folder")
                                .primary()
                                .text_sm()
                                .label(t("create"))
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.create(window, cx)),
                                ),
                        ),
                )
            })
            .child(
                Button::new("new-folder")
                    .outline()
                    .text_sm()
                    .icon(icons::icon(icons::FOLDER))
                    .label(t("new_folder"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.creating = !this.creating;
                        cx.notify();
                    })),
            )
    }
}

/// Open the folder picker for `playlist_id`.
pub(crate) fn open_folder_picker(playlist_id: String, window: &mut Window, cx: &mut App) {
    let picker = cx.new(|cx| {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(t("folder_name")));
        let subscription = cx.subscribe_in(
            &name,
            window,
            |this: &mut FolderPicker, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create(window, cx);
                }
            },
        );
        FolderPicker {
            playlist_id,
            name,
            creating: false,
            _subscriptions: vec![subscription],
        }
    });
    window.open_dialog(cx, move |plate, _, _| {
        plate
            .title(t("move_to_folder"))
            .w(PROMPT_WIDTH)
            .child(picker.clone())
            .footer(
                DialogFooter::new()
                    .child(DialogClose::new().child(Button::new("cancel").label(t("cancel")))),
            )
    });
}

/// Confirm a delete before it happens; `description` names what is going.
pub(crate) fn confirm_delete(
    title: SharedString,
    description: impl Into<SharedString>,
    delete: impl Fn(&mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
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
                    .ok_text(t("delete"))
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text(t("cancel")),
            )
            .on_ok(move |_, _, cx| {
                delete(cx);
                true
            })
    });
}
