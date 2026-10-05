//! Step one: every service the daemon offers, each opening the shared "add source" form on itself.

use api::ServiceInfo;
use components::format::{t, text};
use components::icons;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, IntoElement, ParentElement as _, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div,
};
use settings_sources::ui as forms;

use crate::{FirstRun, ui};

impl FirstRun {
    pub(crate) fn pick_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let services: Vec<ServiceInfo> =
            self.services.read(cx).value().cloned().unwrap_or_default();
        v_flex()
            .w_full()
            .gap_3()
            .child(ui::heading(t("first_run_step_source"), cx))
            .when(services.is_empty(), |step| {
                step.child(forms::empty(t("first_run_no_services"), cx))
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
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(t("add_source"))
                .w(forms::dialog_width())
                // Enter saves the draft rather than throwing it away.
                .on_ok({
                    let form = form.clone();
                    move |_, window, cx| {
                        form.update(cx, |form, cx| form.save(window, cx));
                        false
                    }
                })
                .overlay_closable(false)
                .child(form.clone())
                .footer(div())
        });
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
