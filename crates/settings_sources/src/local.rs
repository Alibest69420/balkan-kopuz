//! Local libraries: the default "Local" source and the named ones, each with the folders it scans.

use api::{LocalSourceDraft, SourceInfo, SourceKind};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, PathPromptOptions,
    Render, SharedString, Styled, Subscription, Window, div,
};
use link::Link;
use store::{ConfigModel, Notices, SourcesModel};
use widgets::format::t;
use widgets::icons;

use crate::ui;

/// Replace a source's scan roots; the sources refetch is what puts the answer on screen.
pub fn write_directories(id: String, directories: Vec<String>, cx: &mut App) {
    let call = Link::global(cx)
        .call(move |api| async move { api.set_source_directories(id, directories).await });
    cx.spawn(async move |cx| match call.await {
        Ok(_) => {
            cx.update(|cx| {
                SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
            });
        }
        Err(error) => {
            cx.update(|cx| Notices::error(error.to_string(), cx));
        }
    })
    .detach();
}

/// Ask the platform for folders and hand `apply` whatever was picked.
pub fn pick_folders(
    multiple: bool,
    cx: &mut App,
    apply: impl FnOnce(Vec<String>, &mut App) + 'static,
) {
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

pub struct LocalLibraries {
    add: Entity<AddLibrary>,
    _subscriptions: Vec<Subscription>,
}

impl LocalLibraries {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        let config = ConfigModel::global(cx);
        let add = cx.new(|cx| AddLibrary::new(window, cx));
        let subscriptions = vec![
            cx.observe(&sources, |_, _, cx| cx.notify()),
            cx.observe(&config, |_, _, cx| cx.notify()),
        ];
        Self {
            add,
            _subscriptions: subscriptions,
        }
    }

    /// A managed settings file pinning either key freezes every library here.
    fn locked(cx: &App) -> bool {
        let config = ConfigModel::global(cx);
        let config = config.read(cx);
        config.is_locked("music_directory") || config.is_locked("local_sources")
    }

    fn libraries(cx: &App) -> Vec<SourceInfo> {
        SourcesModel::global(cx)
            .read(cx)
            .sources()
            .iter()
            .filter(|source| matches!(source.kind, SourceKind::Local | SourceKind::LocalLibrary))
            .cloned()
            .collect()
    }

    fn add_folders(&mut self, source: SourceInfo, cx: &mut Context<Self>) {
        pick_folders(true, cx, move |picked, cx| {
            let mut directories = source.directories.clone();
            for path in picked {
                if !directories.contains(&path) {
                    directories.push(path);
                }
            }
            write_directories(source.id.clone(), directories, cx);
        });
    }

    fn remove_folder(&mut self, source: &SourceInfo, folder: &str, cx: &mut Context<Self>) {
        let directories: Vec<String> = source
            .directories
            .iter()
            .filter(|held| held.as_str() != folder)
            .cloned()
            .collect();
        write_directories(source.id.clone(), directories, cx);
    }

    fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        let call =
            Link::global(cx).call(move |api| async move { api.delete_local_source(id).await });
        cx.spawn(async move |_, cx| match call.await {
            Ok(()) => {
                cx.update(|cx| {
                    SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
                });
            }
            Err(error) => {
                cx.update(|cx| Notices::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add.update(cx, |form, cx| form.reset(window, cx));
        let form = self.add.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(t("add_local_library"))
                .child(form.clone())
                .footer(div())
        });
    }

    fn library(
        &self,
        source: &SourceInfo,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = source.id.clone();
        let named = source.kind == SourceKind::LocalLibrary;
        let name = if named {
            SharedString::from(source.name.clone())
        } else {
            t("local")
        };
        let folders = source.directories.clone();

        ui::row(cx)
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_2()
                            .items_center()
                            .child(div().text_sm().truncate().child(name))
                            .when(source.active, |row| {
                                row.child(ui::active_badge(t("active_local_library"), cx))
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(!source.active, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!("switch-{id}")))
                                        .outline()
                                        .small()
                                        .label(t("switch_to_local_library"))
                                        .on_click(cx.listener({
                                            let id = id.clone();
                                            move |_, _, _, cx| {
                                                SourcesModel::global(cx)
                                                    .update(cx, |sources, cx| {
                                                        sources.switch(id.clone(), cx)
                                                    });
                                            }
                                        })),
                                )
                            })
                            .when(named, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!("delete-{id}")))
                                        .danger()
                                        .small()
                                        .label(t("delete"))
                                        .disabled(locked)
                                        .on_click(cx.listener({
                                            let id = id.clone();
                                            move |this: &mut Self, _, _, cx| {
                                                this.delete(id.clone(), cx)
                                            }
                                        })),
                                )
                            }),
                    ),
            )
            .when(folders.is_empty(), |row| {
                row.child(ui::hint(t("no_music_folders"), cx))
            })
            .children(folders.iter().map(|folder| {
                let folder = folder.clone();
                ui::folder_row(
                    SharedString::from(format!("folder-{id}-{folder}")),
                    SharedString::from(folder.clone()),
                    locked,
                    cx.listener({
                        let source = source.clone();
                        move |this: &mut Self, _, _, cx| this.remove_folder(&source, &folder, cx)
                    }),
                    cx,
                )
            }))
            .child(ui::add_folder_button(
                SharedString::from(format!("add-folder-{id}")),
                locked,
                cx.listener({
                    let source = source.clone();
                    move |this: &mut Self, _, _, cx| this.add_folders(source.clone(), cx)
                }),
            ))
    }
}

impl Render for LocalLibraries {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = Self::locked(cx);
        let libraries = Self::libraries(cx);
        ui::section(t("local_libraries")).child(
            ui::card(cx)
                .children(
                    libraries
                        .iter()
                        .map(|source| self.library(source, locked, cx).into_any_element()),
                )
                .child(
                    Button::new("add-local-library")
                        .outline()
                        .small()
                        .icon(icons::icon(icons::PLUS))
                        .label(t("add_local_library"))
                        .disabled(locked)
                        .on_click(cx.listener(|this, _, window, cx| this.open_add(window, cx))),
                ),
        )
    }
}

/// The "add local library" dialog: a name and the folders it scans.
pub struct AddLibrary {
    name: Entity<InputState>,
    directories: Vec<String>,
    error: Option<SharedString>,
    saving: bool,
}

impl AddLibrary {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder(t("local_library_name"))),
            directories: Vec::new(),
            error: None,
            saving: false,
        }
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.directories.clear();
        self.error = None;
        self.saving = false;
        cx.notify();
    }

    fn add_folders(&mut self, cx: &mut Context<Self>) {
        let form = cx.entity().downgrade();
        pick_folders(true, cx, move |picked, cx| {
            form.update(cx, |form, cx| {
                for path in picked {
                    if !form.directories.contains(&path) {
                        form.directories.push(path);
                    }
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.error = Some(t("local_library_name_required"));
            cx.notify();
            return;
        }
        if self.directories.is_empty() {
            self.error = Some(t("local_library_folder_required"));
            cx.notify();
            return;
        }
        let draft = LocalSourceDraft {
            id: None,
            name,
            directories: self.directories.clone(),
        };
        self.error = None;
        self.saving = true;
        cx.notify();
        let call =
            Link::global(cx).call(move |api| async move { api.upsert_local_source(draft).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                this.saving = false;
                cx.notify();
            })
            .ok();
            match result {
                // A library is added to be used, so it becomes the active source.
                Ok(source) => {
                    cx.update(|window, cx| {
                        SourcesModel::global(cx)
                            .update(cx, |sources, cx| sources.switch(source.id.clone(), cx));
                        window.close_dialog(cx);
                    })
                    .ok();
                }
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.error = Some(SharedString::from(error.to_string()));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }
}

impl Render for AddLibrary {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let saving = self.saving;
        v_flex()
            .w_full()
            .gap_3()
            .child(Input::new(&self.name).disabled(saving))
            .when(self.directories.is_empty(), |form| {
                form.child(ui::hint(t("no_music_folders"), cx))
            })
            .children(self.directories.clone().into_iter().map(|folder| {
                ui::folder_row(
                    SharedString::from(format!("draft-folder-{folder}")),
                    SharedString::from(folder.clone()),
                    saving,
                    cx.listener(move |this: &mut Self, _, _, cx| {
                        this.directories.retain(|held| held != &folder);
                        cx.notify();
                    }),
                    cx,
                )
            }))
            .child(ui::add_folder_button(
                "draft-add-folder",
                saving,
                cx.listener(|this: &mut Self, _, _, cx| this.add_folders(cx)),
            ))
            .when_some(self.error.clone(), |form, message| {
                form.child(ui::error(message, cx))
            })
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-add-library")
                            .outline()
                            .label(t("cancel"))
                            .disabled(saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-add-library")
                            .primary()
                            .label(t("save"))
                            .loading(saving)
                            .disabled(saving)
                            .on_click(
                                cx.listener(|this: &mut Self, _, window, cx| this.save(window, cx)),
                            ),
                    ),
            )
    }
}
