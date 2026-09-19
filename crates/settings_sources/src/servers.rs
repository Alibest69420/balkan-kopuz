//! Media servers: what is configured, which one is live, and the options the active one publishes.

use std::collections::HashMap;

use api::schema::{FieldSpec, FieldValue};
use api::{ApiEvent, SignInKind, SourceFolderEntry, SourceInfo, SourceKind, SourceState, Table};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _,
    Render, SharedString, Styled, Subscription, Window, div, px,
};
use link::Link;
use store::{ConfigModel, Notices, Query, SourcesModel};
use widgets::format::{t, t_with, text};
use widgets::{FieldChanged, SchemaForm, icons};

use crate::add_server::{AddServer, ServerAdded};
use crate::local::write_directories;
use crate::login::LoginForm;
use crate::ui;

/// What the active server's row says about the link to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Connected,
    Connecting,
    Disconnected,
}

/// The path one level up, for the browser's back button.
fn parent_dir(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(cut) => trimmed[..cut].to_string(),
    }
}

/// Answer one of a source's own options. The value is never logged.
fn write_setting(id: String, value: FieldValue, cx: &mut App) {
    let call = Link::global(cx)
        .call(move |api| async move { api.set_source_settings(id, vec![value]).await });
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

/// The active server's options, kept in step with the specs the daemon last answered with.
struct ServerOptions {
    id: String,
    specs: Vec<FieldSpec>,
    form: Entity<SchemaForm>,
    _subscription: Subscription,
}

/// The in-place folder picker for a server whose library is a directory tree.
#[derive(Default)]
struct FolderBrowser {
    open: bool,
    path: String,
    entries: Vec<SourceFolderEntry>,
    error: Option<SharedString>,
    loading: bool,
    generation: u64,
}

pub struct MediaServers {
    options: Option<ServerOptions>,
    browser: FolderBrowser,
    statuses: HashMap<String, SourceState>,
    switching: Option<String>,
    host_access: Entity<Query<bool>>,
    add: Entity<AddServer>,
    login: Entity<LoginForm>,
    _subscriptions: Vec<Subscription>,
}

impl MediaServers {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        let config = ConfigModel::global(cx);
        let connection = Link::connection(cx);
        let host_access = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.can_open_browser().await },
                cx,
            )
        });
        let add = cx.new(|cx| AddServer::new(window, cx));
        let login = cx.new(|cx| LoginForm::new(window, cx));
        let subscriptions = vec![
            cx.observe_in(&sources, window, |this, _, window, cx| {
                this.sync_options(window, cx)
            }),
            cx.observe_in(&config, window, |this, _, window, cx| {
                this.sync_options(window, cx)
            }),
            cx.observe(&host_access, |this, query, cx| {
                let allowed = query.read(cx).value().copied().unwrap_or(true);
                this.add
                    .update(cx, |add, cx| add.set_host_access(allowed, cx));
                cx.notify();
            }),
            // The daemon reports a source going offline; the active row is what shows it.
            cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| {
                if let ApiEvent::SourceStatus { source, state } = event {
                    this.statuses.insert(source.clone(), *state);
                    cx.notify();
                }
            }),
            cx.subscribe_in(&add, window, |this, _, added: &ServerAdded, window, cx| {
                this.activate(added.0.clone(), window, cx)
            }),
        ];
        // The sources may already be loaded, in which case no observer would ever fire.
        cx.defer_in(window, |this, window, cx| this.sync_options(window, cx));
        Self {
            options: None,
            browser: FolderBrowser::default(),
            statuses: HashMap::new(),
            switching: None,
            host_access,
            add,
            login,
            _subscriptions: subscriptions,
        }
    }

    fn servers(cx: &App) -> Vec<SourceInfo> {
        SourcesModel::global(cx)
            .read(cx)
            .sources()
            .iter()
            .filter(|source| source.kind == SourceKind::Server)
            .cloned()
            .collect()
    }

    fn active(cx: &App) -> Option<SourceInfo> {
        SourcesModel::global(cx)
            .read(cx)
            .active()
            .filter(|source| source.kind == SourceKind::Server)
            .cloned()
    }

    fn host_access(&self, cx: &App) -> bool {
        self.host_access.read(cx).value().copied().unwrap_or(true)
    }

    /// Keep one form over the active server's options, rebuilt only when the daemon's fields change.
    fn sync_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locked = ConfigModel::global(cx).read(cx).locked_keys().to_vec();
        let active = Self::active(cx).filter(|source| !source.settings.is_empty());
        let Some(source) = active else {
            self.options = None;
            cx.notify();
            return;
        };
        let specs = source.settings.clone();
        let same = self
            .options
            .as_ref()
            .is_some_and(|options| options.id == source.id);
        if !same {
            let form = cx.new(|cx| SchemaForm::new(specs.clone(), window, cx));
            let id = source.id.clone();
            let subscription = cx.subscribe(&form, move |_, _, event: &FieldChanged, cx| {
                write_setting(id.clone(), event.0.clone(), cx);
            });
            self.options = Some(ServerOptions {
                id: source.id.clone(),
                specs: specs.clone(),
                form,
                _subscription: subscription,
            });
        }
        if let Some(options) = self.options.as_mut() {
            if options.specs != specs {
                options.specs = specs.clone();
                options
                    .form
                    .update(cx, |form, cx| form.set_specs(specs, window, cx));
            }
            options
                .form
                .update(cx, |form, cx| form.set_locked(locked, cx));
        }
        cx.notify();
    }

    fn status(&self, source: &SourceInfo) -> Status {
        if self.switching.as_deref() == Some(source.id.as_str()) {
            return Status::Connecting;
        }
        match self.statuses.get(&source.id) {
            Some(SourceState::Online) => Status::Connected,
            Some(_) => Status::Disconnected,
            None if source.authenticated => Status::Connected,
            None => Status::Disconnected,
        }
    }

    /// Make a server active, then pick up whichever sign-in the daemon says it still needs.
    fn activate(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.switching = Some(id.clone());
        cx.notify();
        let call = Link::global(cx).call(move |api| async move { api.switch_source(id).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                this.switching = None;
                cx.notify();
            })
            .ok();
            match result {
                Ok(source) => {
                    cx.update(|_, cx| {
                        SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
                        ConfigModel::global(cx).update(cx, |config, cx| config.reload(cx));
                    })
                    .ok();
                    this.update_in(cx, |this, window, cx| this.sign_in(&source, window, cx))
                        .ok();
                }
                Err(error) => {
                    cx.update(|_, cx| Notices::error(error.to_string(), cx))
                        .ok();
                }
            }
        })
        .detach();
    }

    /// Which sign-in a source takes belongs to the service; this only asks for whichever it named.
    fn sign_in(&mut self, source: &SourceInfo, window: &mut Window, cx: &mut Context<Self>) {
        match source.sign_in {
            SignInKind::None => {}
            SignInKind::Browser => self.authenticate(source.id.clone(), cx),
            SignInKind::Password => self.open_login(source, window, cx),
        }
    }

    /// Signing in again on a server already active: anything but a browser flow asks for a password.
    fn sign_in_again(&mut self, source: &SourceInfo, window: &mut Window, cx: &mut Context<Self>) {
        match source.sign_in {
            SignInKind::Browser => self.authenticate(source.id.clone(), cx),
            _ => self.open_login(source, window, cx),
        }
    }

    fn authenticate(&mut self, id: String, cx: &mut Context<Self>) {
        let call =
            Link::global(cx).call(move |api| async move { api.authenticate_source(id).await });
        cx.spawn(async move |_, cx| match call.await {
            Ok(_) => {
                cx.update(|cx| {
                    SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
                });
            }
            Err(error) => {
                cx.update(|cx| {
                    Notices::error(t_with("signin_failed", &[("error", error.to_string())]), cx);
                });
            }
        })
        .detach();
    }

    fn open_login(&mut self, source: &SourceInfo, window: &mut Window, cx: &mut Context<Self>) {
        let service = source
            .service
            .as_ref()
            .map(|service| text(&service.name))
            .unwrap_or_else(|| SharedString::from(source.name.clone()));
        self.login.update(cx, |form, cx| {
            form.reset(source.id.clone(), service, window, cx)
        });
        let form = self.login.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog
                .title(form.read(cx).title())
                .child(form.clone())
                .footer(div())
        });
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add.update(cx, |form, cx| form.reset(window, cx));
        let form = self.add.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(t("add_media_server"))
                .w(ui::dialog_width())
                .child(form.clone())
                .footer(div())
        });
    }

    fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.delete_server(id).await });
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

    fn browse(&mut self, id: String, path: String, cx: &mut Context<Self>) {
        self.browser.open = true;
        self.browser.path = path.clone();
        self.browser.loading = true;
        self.browser.error = None;
        self.browser.generation += 1;
        let generation = self.browser.generation;
        cx.notify();
        let call =
            Link::global(cx).call(move |api| async move { api.browse_source(id, path).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                if this.browser.generation != generation {
                    return;
                }
                this.browser.loading = false;
                match result {
                    Ok(entries) => {
                        this.browser.entries = entries;
                        this.browser.error = None;
                    }
                    Err(error) => {
                        this.browser.entries.clear();
                        this.browser.error = Some(t_with(
                            "folder_browse_failed",
                            &[("error", error.to_string())],
                        ));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn status_line(&self, source: &SourceInfo, cx: &mut Context<Self>) -> AnyElement {
        let status = self.status(source);
        let (label, color): (SharedString, Hsla) = match status {
            Status::Connected => (t("connected"), cx.theme().success),
            Status::Connecting => (t("connecting"), cx.theme().warning),
            Status::Disconnected => (t("disconnected"), cx.theme().danger),
        };
        h_flex()
            .gap_2()
            .items_center()
            .mt_1()
            .child(div().text_xs().text_color(color).child(label))
            .when(status == Status::Disconnected, |row| {
                let source = source.clone();
                row.child(
                    Button::new("server-login")
                        .custom(ui::soft(cx))
                        .xsmall()
                        .label(t("login"))
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.sign_in_again(&source, window, cx)
                        })),
                )
            })
            .into_any_element()
    }

    fn folders(&self, source: &SourceInfo, cx: &mut Context<Self>) -> AnyElement {
        let id = source.id.clone();
        let folders = source.directories.clone();
        let path = if self.browser.path.is_empty() {
            "/".to_string()
        } else {
            self.browser.path.clone()
        };

        v_flex()
            .w_full()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(ui::hint(t("remote_music_folders"), cx))
            .when(folders.is_empty(), |column| {
                column.child(ui::empty(t("no_music_folders"), cx))
            })
            .children(folders.iter().map(|folder| {
                let folder = folder.clone();
                ui::folder_row(
                    SharedString::from(format!("remote-{id}-{folder}")),
                    SharedString::from(folder.clone()),
                    false,
                    cx.listener({
                        let id = id.clone();
                        let folders = folders.clone();
                        move |_: &mut Self, _, _, cx| {
                            let kept: Vec<String> = folders
                                .iter()
                                .filter(|held| held.as_str() != folder.as_str())
                                .cloned()
                                .collect();
                            write_directories(id.clone(), kept, cx);
                        }
                    }),
                    cx,
                )
            }))
            .map(|column| {
                if self.browser.open {
                    column.child(self.browser_panel(&id, &path, &folders, cx))
                } else {
                    column.child(ui::add_folder_button(
                        "remote-add-folder",
                        false,
                        cx,
                        cx.listener({
                            let id = id.clone();
                            move |this: &mut Self, _, _, cx| {
                                this.browse(id.clone(), "/".to_string(), cx)
                            }
                        }),
                    ))
                }
            })
            .into_any_element()
    }

    fn browser_panel(
        &self,
        id: &str,
        path: &str,
        folders: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owned_id = id.to_string();
        let owned_path = path.to_string();
        ui::row(cx)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("remote-parent")
                            .outline()
                            .xsmall()
                            .label(t("parent_folder"))
                            .disabled(path == "/")
                            .on_click(cx.listener({
                                let id = owned_id.clone();
                                let path = owned_path.clone();
                                move |this: &mut Self, _, _, cx| {
                                    this.browse(id.clone(), parent_dir(&path), cx)
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(owned_path.clone())),
                    ),
            )
            .child(v_flex().w_full().gap_1().max_h(px(224.)).map(|list| {
                if self.browser.loading {
                    return list.child(ui::empty(t("loading_folders"), cx));
                }
                if let Some(error) = self.browser.error.clone() {
                    return list.child(ui::error(error, cx));
                }
                if self.browser.entries.is_empty() {
                    return list.child(ui::empty(t("no_subfolders"), cx));
                }
                list.children(self.browser.entries.iter().map(|entry| {
                    let target = entry.path.clone();
                    Button::new(SharedString::from(format!("browse-{}", entry.path)))
                        .ghost()
                        .xsmall()
                        .justify_start()
                        .w_full()
                        .icon(icons::icon(icons::FOLDER))
                        .label(SharedString::from(entry.name.clone()))
                        .on_click(cx.listener({
                            let id = owned_id.clone();
                            move |this: &mut Self, _, _, cx| {
                                this.browse(id.clone(), target.clone(), cx)
                            }
                        }))
                }))
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("remote-use-folder")
                            .primary()
                            .xsmall()
                            .label(t("use_this_folder"))
                            .on_click(cx.listener({
                                let id = owned_id.clone();
                                let path = owned_path.clone();
                                let folders = folders.to_vec();
                                move |this: &mut Self, _, _, cx| {
                                    let mut kept = folders.clone();
                                    if !kept.contains(&path) {
                                        kept.push(path.clone());
                                    }
                                    write_directories(id.clone(), kept, cx);
                                    this.browser.open = false;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("remote-cancel-browse")
                            .outline()
                            .xsmall()
                            .label(t("cancel"))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.browser.open = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn server(&self, source: &SourceInfo, cx: &mut Context<Self>) -> AnyElement {
        let id = source.id.clone();
        let service = source
            .service
            .as_ref()
            .map(|service| text(&service.name))
            .unwrap_or_default();
        let detail = source.detail.clone().unwrap_or_default();
        let host_access = self.host_access(cx);
        let browse_folders = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .browse_folders;
        let picker = source.active && browse_folders && source.authenticated;
        let needs_host = source.capabilities.browser_playback && !host_access;

        ui::row(cx)
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_start()
                    .justify_between()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(ui::label(SharedString::from(source.name.clone())))
                                    .when(source.active, |row| {
                                        row.child(ui::active_badge(t("active_server"), cx))
                                    }),
                            )
                            .child(ui::hint(
                                t_with("service", &[("name", service.to_string())]),
                                cx,
                            ))
                            .when(!detail.is_empty(), |column| {
                                column.child(ui::hint(SharedString::from(detail.clone()), cx))
                            })
                            .when(source.active, |column| {
                                column.child(self.status_line(source, cx))
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(!source.active, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!("switch-{id}")))
                                        .custom(ui::soft(cx))
                                        .xsmall()
                                        .label(t("switch_to_server"))
                                        .on_click(cx.listener({
                                            let id = id.clone();
                                            move |this: &mut Self, _, window, cx| {
                                                this.activate(id.clone(), window, cx)
                                            }
                                        })),
                                )
                            })
                            .child(
                                Button::new(SharedString::from(format!("delete-{id}")))
                                    .custom(ui::danger_text(cx))
                                    .small()
                                    .label(t("delete"))
                                    .on_click(cx.listener({
                                        let id = id.clone();
                                        move |this: &mut Self, _, _, cx| this.delete(id.clone(), cx)
                                    })),
                            ),
                    ),
            )
            .when(picker, |row| row.child(self.folders(source, cx)))
            .when(needs_host, |row| {
                row.child(ui::host_access_warning(
                    SharedString::from(format!("host-access-{id}")),
                    t("browser_playback_needs_host"),
                    cx,
                ))
            })
            .when(source.active, |row| {
                row.children(self.options.as_ref().map(|options| {
                    v_flex()
                        .w_full()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(options.form.clone())
                }))
            })
            .into_any_element()
    }
}

impl Render for MediaServers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let servers = Self::servers(cx);
        ui::section(t("media_servers")).child(
            ui::card(cx)
                .when(servers.is_empty(), |card| {
                    card.child(ui::hint(t("no_saved_servers"), cx))
                })
                .children(servers.iter().map(|source| self.server(source, cx)))
                .child(
                    Button::new("add-server")
                        .outline()
                        .small()
                        .icon(icons::icon(icons::PLUS))
                        .label(t("add_server"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_add(window, cx))),
                ),
        )
    }
}
