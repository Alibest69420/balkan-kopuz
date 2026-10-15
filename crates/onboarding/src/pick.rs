//! Step one: every service the daemon offers, each opening the shared "add source" form on itself.

use api::ServiceInfo;
use components::dialogs::{DialogSize, FormDialog};
use components::format::{t, text};
use components::source_mark::{self, SourceMark};
use components::{ChoiceCard, EmptyState};
use components::{Heading, SectionHeading};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled, Window, px,
};

use crate::FirstRun;

impl FirstRun {
    pub(crate) fn pick_step(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
                let accent = source_mark::accent(Some(&service), cx);
                let detail = service.experimental.then(|| t("experimental"));
                ChoiceCard::new(
                    SharedString::from(format!("first-run-service-{}", service.id)),
                    text(&service.name),
                )
                .glyph(SourceMark::new(&service).size(px(18.)))
                .accent(accent)
                .detail(detail)
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
        self.add.update(cx, |form, cx| form.focus(window, cx));
    }
}
