//! Every configured source, which one is live, and the options each publishes.

use std::collections::HashMap;

use api::schema::{FieldKind, FieldSpec, FieldValue};
use api::{SignInKind, SourceFolderEntry, SourceInfo, SourceState, Table};
use components::dialogs::{DialogSize, FormDialog};
use components::format::{t, t_with, text};
use components::{Badge, FieldChanged, SchemaForm, icons, style};
use components::{EmptyState, ErrorBanner, Loading};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _,
    Render, SharedString, Styled, Subscription, Window, div, px,
};
use link::Link;
use store::{ConfigModel, Notices, Query, SourcesModel};

use crate::add_source::{AddSource, SourceAdded};
use crate::folders;
use crate::ui;

/// What the active source's row says about the link to it.
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
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

/// The fields a source's form shows: a folder tree the source browses itself is picked with the browser below it.
fn form_fields(source: &SourceInfo) -> Vec<FieldSpec> {
    source
        .settings
        .iter()
        .filter(|field| {
            !(source.capabilities.browse_folders && matches!(field.kind, FieldKind::Directories))
        })
        .cloned()
        .collect()
}

/// One source's options, kept in step with the specs the daemon last answered with.
struct SourceOptions {
    specs: Vec<FieldSpec>,
    form: Entity<SchemaForm>,
    _subscription: Subscription,
}

/// The in-place folder picker for a source whose library is a remote directory tree.
#[derive(Default)]
struct FolderBrowser {
    open: bool,
    path: String,
    entries: Vec<SourceFolderEntry>,
    error: Option<SharedString>,
    loading: bool,
    generation: u64,
}

pub struct Sources {
    options: HashMap<String, SourceOptions>,
    browser: FolderBrowser,
    switching: Option<String>,
    host_access: Entity<Query<bool>>,
    add: Entity<AddSource>,
    _subscriptions: Vec<Subscription>,
}

impl Sources {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        let config = ConfigModel::global(cx);
        let host_access = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.can_open_browser().await },
                cx,
            )
        });
        let add = cx.new(|cx| AddSource::new(window, cx));
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
            cx.subscribe_in(&add, window, |this, _, added: &SourceAdded, window, cx| {
                this.activate(added.0.clone(), window, cx)
            }),
        ];
        // The sources may already be loaded, in which case no observer would ever fire.
        cx.defer_in(window, |this, window, cx| this.sync_options(window, cx));
        Self {
            options: HashMap::new(),
            browser: FolderBrowser::default(),
            switching: None,
            host_access,
            add,
            _subscriptions: subscriptions,
        }
    }

    fn sources(cx: &App) -> Vec<SourceInfo> {
        SourcesModel::global(cx).read(cx).sources().to_vec()
    }

    fn host_access(&self, cx: &App) -> bool {
        self.host_access.read(cx).value().copied().unwrap_or(true)
    }

    /// Keep one form per source over its options, rebuilt only when the daemon's fields change.
    fn sync_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locked = ConfigModel::global(cx).read(cx).locked_keys().to_vec();
        let sources = Self::sources(cx);
        self.options
            .retain(|id, _| sources.iter().any(|source| &source.id == id));
        for source in sources {
            let specs = form_fields(&source);
            if specs.is_empty() {
                self.options.remove(&source.id);
                continue;
            }
            let options = self.options.entry(source.id.clone()).or_insert_with(|| {
                let form = cx.new(|cx| SchemaForm::new(specs.clone(), window, cx));
                let id = source.id.clone();
                let subscription = cx.subscribe(&form, move |_, _, event: &FieldChanged, cx| {
                    write_setting(id.clone(), event.0.clone(), cx);
                });
                SourceOptions {
                    specs: specs.clone(),
                    form,
                    _subscription: subscription,
                }
            });
            if options.specs != specs {
                options.specs = specs.clone();
                options
                    .form
                    .update(cx, |form, cx| form.set_specs(specs.clone(), window, cx));
            }
            // A secret never comes back from the daemon, so a signed-in source is what says one is held.
            let saved: Vec<String> = match source.authenticated {
                true => specs
                    .iter()
                    .filter(|field| matches!(field.kind, FieldKind::Secret))
                    .map(|field| field.key.clone())
                    .collect(),
                false => Vec::new(),
            };
            let locked = locked.clone();
            options.form.update(cx, |form, cx| {
                form.set_locked(locked, cx);
                form.set_saved(saved, cx);
            });
        }
        cx.notify();
    }

    /// The daemon probes the active source and reports on the row; a switch in flight reads as connecting.
    fn status(&self, source: &SourceInfo) -> Status {
        if self.switching.as_deref() == Some(source.id.as_str()) {
            return Status::Connecting;
        }
        match source.state {
            Some(SourceState::Online) => Status::Connected,
            Some(SourceState::AuthExpired | SourceState::Offline) => Status::Disconnected,
            Some(SourceState::Checking) | None => Status::Connecting,
        }
    }

    /// Make a source active, then pick up whichever sign-in the daemon says it still needs.
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
                    cx.update(|_, cx| Notices::error(store::human(&error), cx))
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

    /// Signing in again reads `reauth`: `sign_in` is None once a source holds credentials.
    fn sign_in_again(&mut self, source: &SourceInfo, window: &mut Window, cx: &mut Context<Self>) {
        match source.reauth {
            SignInKind::Browser => self.authenticate(source.id.clone(), cx),
            _ => self.open_login(source, window, cx),
        }
    }

    /// The daemon owns the browser window and the token it ends holding; this only asks for it.
    fn authenticate(&mut self, id: String, cx: &mut Context<Self>) {
        store::browser_sign_in(id, cx);
    }

    fn open_login(&mut self, source: &SourceInfo, window: &mut Window, cx: &mut Context<Self>) {
        crate::login::open(source.id.clone(), text(&source.service.name), window, cx);
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add.update(cx, |form, cx| form.reset(window, cx));
        let form = self.add.clone();
        FormDialog::new(t("add_source"))
            .size(DialogSize::Wide)
            .on_submit(move |window, cx| form.update(cx, |form, cx| form.save(window, cx)))
            .open(self.add.clone(), window, cx);
    }

    fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.delete_source(id).await });
        cx.spawn(async move |_, cx| match call.await {
            Ok(()) => {
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
        let waiting = store::is_waiting_for_browser(&source.id, cx);
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
            .when(waiting, |row| {
                row.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Spinner::new().small().color(cx.theme().warning))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .child(t("waiting_for_browser")),
                        ),
                )
            })
            .when(status == Status::Disconnected && !waiting, |row| {
                let source = source.clone();
                row.child(
                    Button::new("source-login")
                        .custom(style::soft_button(cx.theme().foreground, cx))
                        .xsmall()
                        .label(t("login"))
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.sign_in_again(&source, window, cx)
                        })),
                )
            })
            .into_any_element()
    }

    /// A remote folder tree's chosen folders, and the browser that adds to them.
    fn remote_folders(&self, source: &SourceInfo, cx: &mut Context<Self>) -> AnyElement {
        let id = source.id.clone();
        let chosen = folders::of(source);
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
            .when(chosen.is_empty(), |column| {
                column.child(EmptyState::new(t("no_music_folders")).compact())
            })
            .children(chosen.iter().map(|folder| {
                let folder = folder.clone();
                ui::folder_row(
                    SharedString::from(format!("remote-{id}-{folder}")),
                    SharedString::from(folder.clone()),
                    false,
                    cx.listener({
                        let id = id.clone();
                        let chosen = chosen.clone();
                        move |_: &mut Self, _, _, cx| {
                            let kept: Vec<String> = chosen
                                .iter()
                                .filter(|held| held.as_str() != folder.as_str())
                                .cloned()
                                .collect();
                            folders::write(id.clone(), kept, cx);
                        }
                    }),
                    cx,
                )
            }))
            .map(|column| {
                if self.browser.open {
                    column.child(self.browser_panel(&id, &path, &chosen, cx))
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
        chosen: &[String],
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
                    return list.child(Loading::new().label(t("loading_folders")).compact());
                }
                if let Some(error) = self.browser.error.clone() {
                    return list.child(ErrorBanner::new(error));
                }
                if self.browser.entries.is_empty() {
                    return list.child(EmptyState::new(t("no_subfolders")).compact());
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
                                let chosen = chosen.to_vec();
                                move |this: &mut Self, _, _, cx| {
                                    let mut kept = chosen.clone();
                                    if !kept.contains(&path) {
                                        kept.push(path.clone());
                                    }
                                    folders::write(id.clone(), kept, cx);
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

    fn source(
        &self,
        source: &SourceInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = source.id.clone();
        let service = text(&source.service.name);
        let detail = source.detail.clone().unwrap_or_default();
        let host_access = self.host_access(cx);
        let picker = source.active && source.capabilities.browse_folders && source.authenticated;
        let needs_host = source.capabilities.browser_playback && !host_access;

        ui::hover_row(&id, window, cx)
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
                                        row.child(Badge::new(t("active_server")))
                                    }),
                            )
                            .child(ui::hint(
                                t_with("service", &[("name", service.to_string())]),
                                cx,
                            ))
                            .when(!detail.is_empty(), |column| {
                                column.child(ui::hint(SharedString::from(detail.clone()), cx))
                            })
                            .when(source.active && source.needs_network, |column| {
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
                                        .custom(style::soft_button(cx.theme().foreground, cx))
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
                            .when(!source.permanent, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!("delete-{id}")))
                                        .custom(style::tint_text_button(cx.theme().danger, cx))
                                        .small()
                                        .label(t("delete"))
                                        .on_click(cx.listener({
                                            let id = id.clone();
                                            let name = SharedString::from(source.name.clone());
                                            // This takes the source's stored sign-in and library with it.
                                            move |_: &mut Self, _, window, cx| {
                                                let id = id.clone();
                                                let entity = cx.entity();
                                                components::dialogs::confirm(
                                                    components::dialogs::ConfirmKind::Delete,
                                                    t("delete"),
                                                    name.clone(),
                                                    move |_, cx| {
                                                        let id = id.clone();
                                                        entity.update(cx, |this, cx| {
                                                            this.delete(id, cx)
                                                        });
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            }
                                        })),
                                )
                            }),
                    ),
            )
            .when(picker, |row| row.child(self.remote_folders(source, cx)))
            .when(needs_host, |row| {
                row.child(ui::host_access_warning(
                    SharedString::from(format!("host-access-{id}")),
                    t("browser_playback_needs_host"),
                    cx,
                ))
            })
            .children(self.options.get(&source.id).map(|options| {
                v_flex()
                    .w_full()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(options.form.clone())
            }))
            .into_any_element()
    }
}

impl Render for Sources {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sources = Self::sources(cx);
        let rows: Vec<_> = sources
            .iter()
            .map(|source| self.source(source, window, cx))
            .collect();
        ui::section(t("sources")).opaque().child(
            ui::card(cx)
                .when(sources.is_empty(), |card| {
                    card.child(EmptyState::new(t("no_saved_servers")).compact())
                })
                .children(rows)
                .child(
                    Button::new("add-source")
                        .custom(style::soft_button(cx.theme().foreground, cx))
                        .small()
                        .icon(icons::icon(icons::PLUS))
                        .label(t("add_source"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_add(window, cx)))
                        .self_start(),
                ),
        )
    }
}
