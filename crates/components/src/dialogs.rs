//! Dialogs that live in a feature crate but are opened from shared rows; the app registers them at startup.

use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogButtonProps, DialogClose, DialogFooter};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyView, App, Global, ParentElement as _, Pixels, SharedString, Styled as _, Window, div, px,
};

use crate::format::t;

type MetadataOpener = Rc<dyn Fn(String, bool, &mut Window, &mut App)>;
type BatchMetadataOpener = Rc<dyn Fn(Vec<String>, &mut Window, &mut App)>;
type RenameOpener =
    Rc<dyn Fn(SharedString, SharedString, Rc<dyn Fn(String, &mut App)>, &mut Window, &mut App)>;
type FolderOpener = Rc<dyn Fn(String, &mut Window, &mut App)>;
type SheetOpener = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Default)]
struct Dialogs {
    track_metadata: Option<MetadataOpener>,
    batch_metadata: Option<BatchMetadataOpener>,
    rename: Option<RenameOpener>,
    folder_picker: Option<FolderOpener>,
    shortcuts: Option<SheetOpener>,
    url_download: Option<SheetOpener>,
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

/// Register the editor that writes one set of tags across a whole selection.
pub fn register_batch_metadata(
    opener: impl Fn(Vec<String>, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    dialogs(cx).batch_metadata = Some(Rc::new(opener));
}

pub fn open_batch_metadata(keys: Vec<String>, window: &mut Window, cx: &mut App) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.batch_metadata.clone());
    match opener {
        Some(opener) => opener(keys, window, cx),
        None => tracing::warn!("no batch metadata dialog is registered"),
    }
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

/// Register the sheet `?` opens; the registry's `app.shortcuts` is what asks for it.
pub fn register_shortcuts(opener: impl Fn(&mut Window, &mut App) + 'static, cx: &mut App) {
    dialogs(cx).shortcuts = Some(Rc::new(opener));
}

pub fn open_shortcuts(window: &mut Window, cx: &mut App) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.shortcuts.clone());
    match opener {
        Some(opener) => opener(window, cx),
        None => tracing::warn!("no shortcuts sheet is registered"),
    }
}

/// Register the "fetch this URL" box; the downloads popover and `downloads.add_url` both ask for it.
pub fn register_url_download(opener: impl Fn(&mut Window, &mut App) + 'static, cx: &mut App) {
    dialogs(cx).url_download = Some(Rc::new(opener));
}

pub fn open_url_download(window: &mut Window, cx: &mut App) {
    let opener = cx
        .try_global::<Dialogs>()
        .and_then(|dialogs| dialogs.url_download.clone());
    match opener {
        Some(opener) => opener(window, cx),
        None => tracing::warn!("no url download dialog is registered"),
    }
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

/// What a confirmation is asking leave to do; it names the button that says yes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    /// Throw away edits that were never saved.
    Discard,
    /// Remove something for good; the description should name what goes.
    Delete,
}

impl ConfirmKind {
    fn ok_text(self) -> SharedString {
        match self {
            ConfirmKind::Discard => t("discard_changes"),
            ConfirmKind::Delete => t("delete"),
        }
    }
}

/// Ask before something that cannot be taken back; `go` runs only when the answer is yes.
pub fn confirm(
    kind: ConfirmKind,
    title: SharedString,
    description: impl Into<SharedString>,
    go: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let description = description.into();
    let go = Rc::new(go);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let go = go.clone();
        alert
            .title(title.clone())
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .ok_text(kind.ok_text())
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text(t("cancel")),
            )
            .on_ok(move |_, window, cx| {
                go(window, cx);
                true
            })
    });
}

/// How wide a dialog is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DialogSize {
    /// `w-80`: a one-field prompt or a short pick.
    Prompt,
    /// The dialog's own 448px: a short form.
    #[default]
    Form,
    /// `max-w-lg`: a form in sections, such as a source definition or a track's tags.
    Wide,
    /// A reference sheet that lays its contents out in columns.
    Sheet,
}

impl DialogSize {
    fn width(self) -> Pixels {
        match self {
            DialogSize::Prompt => px(320.),
            DialogSize::Form => px(448.),
            DialogSize::Wide => px(512.),
            DialogSize::Sheet => px(720.),
        }
    }
}

/// What sits under a dialog's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FormFooter {
    /// Nothing at all, for a dialog whose body ends flush.
    None,
    /// An empty band: the body carries its own buttons, and the dialog's OK and Cancel stay hidden.
    #[default]
    Empty,
    /// One Cancel that closes it.
    Cancel,
    /// Cancel, and a Save that submits.
    CancelSave,
}

type Submit = Rc<dyn Fn(&mut Window, &mut App)>;
type Read<T> = Rc<dyn Fn(&App) -> T>;

/// The one dialog opener: a title, a width, a body view, what submitting does, and the footer under it.
pub struct FormDialog {
    title: Read<SharedString>,
    size: DialogSize,
    closable: Read<bool>,
    footer: FormFooter,
    submit: Option<Submit>,
}

impl FormDialog {
    pub fn new(title: impl Into<SharedString>) -> Self {
        let title = title.into();
        Self {
            title: Rc::new(move |_| title.clone()),
            size: DialogSize::Form,
            closable: Rc::new(|_| false),
            footer: FormFooter::Empty,
            submit: None,
        }
    }

    /// A title read each frame, for a dialog whose title follows its body's state.
    pub fn title_with(mut self, title: impl Fn(&App) -> SharedString + 'static) -> Self {
        self.title = Rc::new(title);
        self
    }

    pub fn size(mut self, size: DialogSize) -> Self {
        self.size = size;
        self
    }

    /// Whether a click on the backdrop closes it; off by default, since a stray click must not drop typed input.
    pub fn closable(self, closable: bool) -> Self {
        self.closable_with(move |_| closable)
    }

    /// The same, read each frame.
    pub fn closable_with(mut self, closable: impl Fn(&App) -> bool + 'static) -> Self {
        self.closable = Rc::new(closable);
        self
    }

    pub fn footer(mut self, footer: FormFooter) -> Self {
        self.footer = footer;
        self
    }

    /// What Enter and Save do; the dialog stays open, so `submit` closes it once its work lands.
    pub fn on_submit(mut self, submit: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.submit = Some(Rc::new(submit));
        self
    }

    pub fn open(self, body: impl Into<AnyView>, window: &mut Window, cx: &mut App) {
        let body = body.into();
        let FormDialog {
            title,
            size,
            closable,
            footer,
            submit,
        } = self;
        window.open_dialog(cx, move |dialog, _, cx| {
            let dialog = dialog
                .title(title(cx))
                .w(size.width())
                .overlay_closable(closable(cx))
                .child(body.clone())
                .when_some(submit.clone(), |dialog, submit| {
                    dialog.on_ok(move |_, window, cx| {
                        submit(window, cx);
                        false
                    })
                });
            let cancel = || DialogClose::new().child(Button::new("cancel").label(t("cancel")));
            match footer {
                FormFooter::None => dialog,
                FormFooter::Empty => dialog.footer(div()),
                FormFooter::Cancel => dialog.footer(DialogFooter::new().child(cancel())),
                FormFooter::CancelSave => {
                    let submit = submit.clone();
                    dialog.footer(DialogFooter::new().gap_2().child(cancel()).child(
                        Button::new("save").primary().label(t("save")).on_click(
                            move |_, window, cx| {
                                if let Some(submit) = &submit {
                                    submit(window, cx);
                                }
                            },
                        ),
                    ))
                }
            }
        });
    }
}
