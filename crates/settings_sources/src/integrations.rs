//! Connectivity: everything configured per account rather than per source, as the daemon lists it.

use std::collections::HashMap;

use api::schema::{FieldSpec, FieldValue};
use api::{ConnectKind, IntegrationInfo, Table};
use components::format::{t, t_with, text};
use components::{FieldChanged, SchemaForm, style};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Subscription, Window,
};
use link::Link;
use store::{ConfigModel, Notices, Query};

use crate::ui;

/// Answer one integration's fields. The value is never logged.
fn write_settings(
    id: String,
    value: FieldValue,
    query: Entity<Query<Vec<IntegrationInfo>>>,
    cx: &mut App,
) {
    let call = Link::global(cx)
        .call(move |api| async move { api.set_integration_settings(id, vec![value]).await });
    cx.spawn(async move |cx| match call.await {
        Ok(_) => {
            cx.update(|cx| query.update(cx, |query, cx| query.refresh(cx)));
        }
        Err(error) => {
            cx.update(|cx| Notices::error(store::human(&error), cx));
        }
    })
    .detach();
}

struct IntegrationForm {
    specs: Vec<FieldSpec>,
    form: Entity<SchemaForm>,
    _subscription: Subscription,
}

pub struct Integrations {
    query: Entity<Query<Vec<IntegrationInfo>>>,
    forms: HashMap<String, IntegrationForm>,
    _subscriptions: Vec<Subscription>,
}

impl Integrations {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = ConfigModel::global(cx);
        let query = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.integrations().await },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe_in(&query, window, |this, _, window, cx| {
                this.sync_forms(window, cx)
            }),
            cx.observe_in(&config, window, |this, _, window, cx| {
                this.sync_forms(window, cx)
            }),
        ];
        // The listing may already be cached, in which case no observer would ever fire.
        cx.defer_in(window, |this, window, cx| this.sync_forms(window, cx));
        Self {
            query,
            forms: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    fn listed(&self, cx: &App) -> Vec<IntegrationInfo> {
        self.query.read(cx).value().cloned().unwrap_or_default()
    }

    /// One form per integration, rebuilt only when the daemon's fields change under it.
    fn sync_forms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locked = ConfigModel::global(cx).read(cx).locked_keys().to_vec();
        let listed = self.listed(cx);
        self.forms
            .retain(|id, _| listed.iter().any(|integration| &integration.id == id));
        for integration in listed {
            let specs = integration.fields.clone();
            if !self.forms.contains_key(&integration.id) {
                let form = cx.new(|cx| SchemaForm::new(specs.clone(), window, cx));
                let id = integration.id.clone();
                let query = self.query.clone();
                let subscription = cx.subscribe(&form, move |_, _, event: &FieldChanged, cx| {
                    write_settings(id.clone(), event.0.clone(), query.clone(), cx);
                });
                self.forms.insert(
                    integration.id.clone(),
                    IntegrationForm {
                        specs: specs.clone(),
                        form,
                        _subscription: subscription,
                    },
                );
            }
            let Some(held) = self.forms.get_mut(&integration.id) else {
                continue;
            };
            if held.specs != specs {
                held.specs = specs.clone();
                held.form
                    .update(cx, |form, cx| form.set_specs(specs.clone(), window, cx));
            }
            // A secret never comes back, so "configured" is the only word on whether one is stored.
            let saved: Vec<String> = match integration.configured {
                true => specs
                    .iter()
                    .filter(|field| matches!(field.kind, api::schema::FieldKind::Secret))
                    .map(|field| field.key.clone())
                    .collect(),
                false => Vec::new(),
            };
            held.form.update(cx, |form, cx| {
                form.set_locked(locked.clone(), cx);
                form.set_saved(saved, cx);
            });
        }
        cx.notify();
    }

    fn connect(&mut self, id: String, cx: &mut Context<Self>) {
        let call =
            Link::global(cx).call(move |api| async move { api.authenticate_integration(id).await });
        let query = self.query.clone();
        cx.spawn(async move |_, cx| match call.await {
            Ok(_) => {
                cx.update(|cx| query.update(cx, |query, cx| query.refresh(cx)));
            }
            Err(error) => {
                cx.update(|cx| {
                    Notices::error(t_with("signin_failed", &[("error", error.to_string())]), cx);
                });
            }
        })
        .detach();
    }

    fn integration(
        &self,
        integration: &IntegrationInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = integration.id.clone();
        let name = text(&integration.name);
        let configured = integration.configured;
        let success = cx.theme().success;
        v_flex()
            .w_full()
            .gap_2()
            .child(ui::label(name.clone()))
            .children(self.forms.get(&id).map(|held| held.form.clone()))
            .when(integration.connect == ConnectKind::WebSignIn, |row| {
                row.child(
                    h_flex().child(
                        Button::new(SharedString::from(format!("connect-{id}")))
                            .small()
                            .map(|button| {
                                if configured {
                                    button
                                        .custom(
                                            ButtonCustomVariant::new(cx)
                                                .color(style::rest(success.opacity(0.2)))
                                                .hover(success.opacity(0.2))
                                                .active(success.opacity(0.2))
                                                .foreground(success),
                                        )
                                        .bg(success.opacity(0.2))
                                } else {
                                    button.custom(ui::soft(cx))
                                }
                            })
                            .label(if configured {
                                t_with("integration_connected", &[("name", name.to_string())])
                            } else {
                                t_with("integration_connect", &[("name", name.to_string())])
                            })
                            .disabled(configured)
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |this: &mut Self, _, _, cx| this.connect(id.clone(), cx)
                            })),
                    ),
                )
            })
    }
}

impl Render for Integrations {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let listed = self.listed(cx);
        ui::section(t("connectivity")).opaque().child(
            ui::card(cx).children(
                listed
                    .iter()
                    .map(|integration| self.integration(integration, cx).into_any_element()),
            ),
        )
    }
}
