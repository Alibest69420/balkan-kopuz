//! Step one: every service the daemon offers, each opening the shared "add source" form on itself.

use api::ServiceInfo;
use components::EmptyState;
use components::dialogs::{DialogSize, FormDialog};
use components::format::{t, text};
use components::{Heading, SectionHeading, icons};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, IntoElement, ParentElement as _, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};

use crate::{FirstRun, ui};

impl FirstRun {
    pub(crate) fn pick_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let services: Vec<ServiceInfo> =
            self.services.read(cx).value().cloned().unwrap_or_default();
        v_flex()
            .w_full()
            .gap_3()
            .child(SectionHeading::new(t("first_run_step_source")).heading(Heading::Step))
            .when(services.is_empty(), |step| {
                step.child(EmptyState::new(t("first_run_no_services")).compact())
            })
            .children(services.into_iter().map(|service| {
                let accent = accent_of(&service, cx);
                let detail = service.experimental.then(|| t("experimental"));
                ui::choice_card(
                    SharedString::from(format!("first-run-service-{}", service.id)),
                    glyph_of(&service),
                    accent,
                    text(&service.name),
                    detail,
                    window,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.open_add(service.clone(), window, cx)
                }))
                .into_any_element()
            }))
            .into_any_element()
    }

    fn open_add(&mut self, service: ServiceInfo, window: &mut Window, cx: &mut Context<Self>) {
        self.add.update(cx, |form, cx| {
            form.reset(window, cx);
            form.select(service, window, cx);
        });
        let form = self.add.clone();
        FormDialog::new(t("add_source"))
            .size(DialogSize::Wide)
            .on_submit(move |window, cx| form.update(cx, |form, cx| form.save(window, cx)))
            .open(self.add.clone(), window, cx);
    }
}

/// The daemon names its icon by class; a bundled glyph of the same shape stands in for it.
fn glyph_of(service: &ServiceInfo) -> &'static str {
    match &service.icon {
        api::Icon::Class(class) if class.contains("folder") => icons::FOLDER,
        api::Icon::Class(class) if class.contains("cloud") => ui::CLOUD,
        _ => ui::SERVER,
    }
}

fn accent_of(service: &ServiceInfo, cx: &App) -> Hsla {
    Rgba::try_from(service.accent.as_str())
        .ok()
        .map(Hsla::from)
        .unwrap_or(cx.theme().primary)
}
