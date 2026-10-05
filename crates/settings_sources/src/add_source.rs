//! The "add source" dialog: a name, a service, and whatever form the daemon publishes for it.

use api::schema::{FieldKind, FieldValue};
use api::{DraftCheck, ServiceInfo, SignInKind, SourceDraft, Table};
use components::format::{t, text};
use components::{FieldChanged, SchemaForm};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    SharedString, Styled, Subscription, Window,
};
use link::Link;
use store::Query;

use crate::ui;

/// A source was saved; the owner makes it active and picks up whatever sign-in it needs.
#[derive(Debug, Clone)]
pub struct SourceAdded(pub String);

pub struct AddSource {
    services: Entity<Query<Vec<ServiceInfo>>>,
    service: Option<ServiceInfo>,
    name: Entity<InputState>,
    name_problem: Option<SharedString>,
    form: Option<Entity<SchemaForm>>,
    check: Option<DraftCheck>,
    error: Option<SharedString>,
    saving: bool,
    host_access: bool,
    // Bumped per check, so a slower answer never overwrites a newer one.
    generation: u64,
    _form_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SourceAdded> for AddSource {}

impl AddSource {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let services = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.services().await },
                cx,
            )
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(t("source_name")));
        let subscriptions = vec![
            cx.observe_in(&services, window, |this, _, window, cx| {
                this.ensure_service(window, cx)
            }),
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.recheck(cx);
                }
            }),
        ];
        Self {
            services,
            service: None,
            name,
            name_problem: None,
            form: None,
            check: None,
            error: None,
            saving: false,
            host_access: true,
            generation: 0,
            _form_subscription: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_host_access(&mut self, host_access: bool, cx: &mut Context<Self>) {
        if self.host_access != host_access {
            self.host_access = host_access;
            cx.notify();
        }
    }

    /// Clear the form for a fresh source, keeping the service list already read.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.check = None;
        self.error = None;
        self.name_problem = None;
        self.saving = false;
        self.service = None;
        self.form = None;
        self._form_subscription = None;
        self.ensure_service(window, cx);
        // The name is the first thing asked for, so put the caret in it.
        self.name.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    fn offered(&self, cx: &App) -> Vec<ServiceInfo> {
        self.services.read(cx).value().cloned().unwrap_or_default()
    }

    /// Nothing is chosen until the list arrives, and then the first is.
    fn ensure_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.service.is_some() {
            cx.notify();
            return;
        }
        let Some(first) = self.offered(cx).into_iter().next() else {
            cx.notify();
            return;
        };
        self.select(first, window, cx);
    }

    pub fn select(&mut self, service: ServiceInfo, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| SchemaForm::new(service.fields.clone(), window, cx));
        self._form_subscription =
            Some(cx.subscribe(&form, |this, _, _: &FieldChanged, cx| this.recheck(cx)));
        self.form = Some(form);
        self.service = Some(service);
        self.check = None;
        self.error = None;
        self.recheck(cx);
        cx.notify();
    }

    fn draft(&self, cx: &App) -> SourceDraft {
        let Some(service) = self.service.as_ref() else {
            return SourceDraft::default();
        };
        let secret_keys: Vec<String> = service
            .fields
            .iter()
            .filter(|field| matches!(field.kind, FieldKind::Secret))
            .map(|field| field.key.clone())
            .collect();
        let answers = self
            .form
            .as_ref()
            .map(|form| form.read(cx).values())
            .unwrap_or_default();
        let (secrets, values): (Vec<FieldValue>, Vec<FieldValue>) = answers
            .into_iter()
            .partition(|answer| secret_keys.contains(&answer.key));
        SourceDraft {
            id: None,
            name: self.name.read(cx).value().to_string(),
            service: service.id.clone(),
            values,
            secrets,
        }
    }

    /// Ask the daemon what the draft would do; cheap enough to run on every edit.
    fn recheck(&mut self, cx: &mut Context<Self>) {
        if self.service.is_none() {
            return;
        }
        let draft = self.draft(cx);
        self.generation += 1;
        let generation = self.generation;
        let call =
            Link::global(cx).call(move |api| async move { api.check_source_draft(draft).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.check = result.ok();
                let problems = this
                    .check
                    .as_ref()
                    .map(|check| check.problems.clone())
                    .unwrap_or_default();
                // The name is this dialog's own field, so its problem has nowhere else to land.
                this.name_problem = problems
                    .iter()
                    .find(|problem| problem.field.as_deref() == Some("name"))
                    .map(|problem| text(&problem.label));
                if let Some(form) = this.form.as_ref() {
                    form.update(cx, |form, cx| form.set_problems(problems, cx));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A sandboxed daemon cannot open a browser, so a service whose sign-in is one cannot be saved.
    fn blocked(&self) -> bool {
        !self.host_access
            && self
                .check
                .as_ref()
                .is_some_and(|check| check.sign_in == SignInKind::Browser)
    }

    /// Save the draft. Closes the dialog itself once the daemon accepts it and keeps it open with
    /// the problems otherwise, so whatever calls this must not close the dialog as well.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.blocked() || self.service.is_none() {
            return;
        }
        let draft = self.draft(cx);
        self.error = None;
        self.saving = true;
        cx.notify();
        let link = Link::global(cx).clone();
        let checked = link.call({
            let draft = draft.clone();
            move |api| async move { api.check_source_draft(draft).await }
        });
        cx.spawn_in(window, async move |this, cx| {
            let problem = match checked.await {
                Ok(check) => check.problems.first().map(|problem| text(&problem.label)),
                Err(error) => Some(SharedString::from(error.to_string())),
            };
            if let Some(problem) = problem {
                this.update(cx, |this, cx| {
                    this.saving = false;
                    this.error = Some(problem);
                    cx.notify();
                })
                .ok();
                return;
            }
            let saved = cx
                .update(|_, cx| {
                    Link::global(cx).call(move |api| async move { api.upsert_source(draft).await })
                })
                .ok();
            let Some(saved) = saved else { return };
            let result = saved.await;
            this.update(cx, |this, cx| {
                this.saving = false;
                cx.notify();
            })
            .ok();
            match result {
                Ok(source) => {
                    cx.update(|window, cx| window.close_dialog(cx)).ok();
                    // A secret left in a live form is one the next source could pick up.
                    this.update_in(cx, |this, window, cx| this.reset(window, cx))
                        .ok();
                    this.update(cx, |_, cx| cx.emit(SourceAdded(source.id.clone())))
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

    fn service_label(service: &ServiceInfo) -> SharedString {
        let name = text(&service.name);
        if service.experimental {
            SharedString::from(format!("{name} ({})", t("experimental")))
        } else {
            name
        }
    }
}

impl Render for AddSource {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let services = self.offered(cx);
        let chosen = self
            .service
            .as_ref()
            .map(Self::service_label)
            .unwrap_or_else(|| t("choose"));
        let saving = self.saving;
        let blocked = self.blocked();
        let this = cx.entity().downgrade();

        v_flex()
            .w_full()
            .gap_3()
            .when(blocked, |form| {
                form.child(ui::host_access_warning(
                    "add-source-host-access",
                    t("browser_sign_in_needs_host"),
                    cx,
                ))
            })
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(Input::new(&self.name).disabled(saving))
                    .when_some(self.name_problem.clone(), |field, problem| {
                        field.child(ui::field_problem(problem, cx))
                    }),
            )
            .child(
                Button::new("add-source-service")
                    .outline()
                    .w_full()
                    .justify_between()
                    .label(chosen)
                    .disabled(saving)
                    .dropdown_menu(move |mut menu, _, _| {
                        for service in &services {
                            let service = service.clone();
                            let this = this.clone();
                            menu = menu.item(
                                PopupMenuItem::new(Self::service_label(&service)).on_click(
                                    move |_, window, cx| {
                                        this.update(cx, |this, cx| {
                                            this.select(service.clone(), window, cx)
                                        })
                                        .ok();
                                    },
                                ),
                            );
                        }
                        menu
                    }),
            )
            .children(self.form.clone())
            .when_some(self.error.clone(), |form, message| {
                form.child(ui::error(message, cx))
            })
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-add-source")
                            .outline()
                            .label(t("cancel"))
                            .disabled(saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-add-source")
                            .primary()
                            .label(t("save"))
                            .loading(saving)
                            .disabled(saving || blocked)
                            .on_click(
                                cx.listener(|this: &mut Self, _, window, cx| this.save(window, cx)),
                            ),
                    ),
            )
    }
}
