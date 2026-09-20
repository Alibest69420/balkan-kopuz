//! Step two: the settings page's own forms, reached from here instead of from Settings.

use api::ServiceInfo;
use api::schema::FieldKind;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, IntoElement, ParentElement as _, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div,
};
use settings_sources::local::{pick_folders, write_directories};
use settings_sources::ui as forms;
use store::SourcesModel;
use widgets::format::{t, text};

use crate::{FirstRun, Kind, local_source, ui};

impl FirstRun {
    pub(crate) fn configure_step(
        &mut self,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match kind {
            Kind::Local => self.local_step(cx),
            _ => self.service_step(kind, window, cx),
        }
    }

    /// The folders a local library scans, added with the same picker the settings page uses.
    fn local_step(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let folders = local_source(cx)
            .map(|source| source.directories)
            .unwrap_or_default();
        let empty = folders.is_empty();
        v_flex()
            .w_full()
            .gap_3()
            .child(ui::heading(t("first_run_local_title"), cx))
            .child(ui::hint(t("first_run_local_hint"), cx))
            .when(empty, |step| {
                step.child(forms::empty(t("no_music_folders"), cx))
            })
            .children(folders.into_iter().map(|folder| {
                forms::folder_row(
                    SharedString::from(format!("first-run-folder-{folder}")),
                    SharedString::from(folder.clone()),
                    false,
                    cx.listener(move |this: &mut Self, _, _, cx| this.drop_folder(&folder, cx)),
                    cx,
                )
                .into_any_element()
            }))
            .child(forms::add_folder_button(
                "first-run-add-folder",
                false,
                cx,
                cx.listener(|this: &mut Self, _, _, cx| this.add_folders(cx)),
            ))
            .child(
                Button::new("first-run-scan")
                    .primary()
                    .label(t("first_run_scan"))
                    .disabled(empty)
                    .on_click(
                        cx.listener(|this: &mut Self, _, window, cx| this.start_local(window, cx)),
                    )
                    .self_start(),
            )
            .into_any_element()
    }

    fn add_folders(&mut self, cx: &mut Context<Self>) {
        let Some(source) = local_source(cx) else {
            return;
        };
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

    fn drop_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        let Some(source) = local_source(cx) else {
            return;
        };
        let directories: Vec<String> = source
            .directories
            .iter()
            .filter(|held| held.as_str() != folder)
            .cloned()
            .collect();
        write_directories(source.id.clone(), directories, cx);
    }

    /// Make the chosen library live, then let the sync step ask for its first scan.
    fn start_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = local_source(cx) else {
            return;
        };
        self.step = crate::Step::Sync;
        self.job = None;
        self.target = Some(source.id.clone());
        if source.active {
            self.maybe_start(cx);
        } else {
            SourcesModel::global(cx).update(cx, |sources, cx| {
                sources.switch(source.id.clone(), window, cx)
            });
        }
        cx.notify();
    }

    /// Every service of the chosen shape, each opening the real "add server" form on itself.
    fn service_step(
        &mut self,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let streaming = kind == Kind::Streaming;
        let services: Vec<ServiceInfo> = self
            .services
            .read(cx)
            .value()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|service| is_streaming(service) == streaming)
            .collect();
        let blocked = streaming && !self.host_access.read(cx).value().copied().unwrap_or(true);
        let title = if streaming {
            t("first_run_streaming_title")
        } else {
            t("first_run_server_title")
        };
        let hint = if streaming {
            t("first_run_streaming_hint")
        } else {
            t("first_run_server_hint")
        };
        v_flex()
            .w_full()
            .gap_3()
            .child(ui::heading(title, cx))
            .child(ui::hint(hint, cx))
            .when(blocked, |step| {
                step.child(forms::host_access_warning(
                    "first-run-host-access",
                    t("browser_sign_in_needs_host"),
                    cx,
                ))
            })
            .when(services.is_empty(), |step| {
                step.child(forms::empty(t("first_run_no_services"), cx))
            })
            .children(services.into_iter().map(|service| {
                let glyph = if streaming { ui::CLOUD } else { ui::SERVER };
                let accent = accent_of(&service, cx);
                let label = text(&service.name);
                let detail = service.experimental.then(|| t("experimental"));
                ui::choice_card(
                    SharedString::from(format!("first-run-service-{}", service.id)),
                    glyph,
                    accent,
                    label,
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
                .title(t("add_media_server"))
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

/// A service with nowhere to type an address is one the daemon reaches on its own.
fn is_streaming(service: &ServiceInfo) -> bool {
    !service
        .fields
        .iter()
        .any(|field| matches!(field.kind, FieldKind::Url))
}

fn accent_of(service: &ServiceInfo, cx: &App) -> Hsla {
    Rgba::try_from(service.accent.as_str())
        .ok()
        .map(Hsla::from)
        .unwrap_or(cx.theme().primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::schema::{FieldSpec, Text};

    fn service(fields: Vec<FieldKind>) -> ServiceInfo {
        ServiceInfo {
            name: Text::literal("test"),
            fields: fields
                .into_iter()
                .map(|kind| FieldSpec {
                    kind,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn an_address_field_is_what_makes_a_service_a_server() {
        assert!(!is_streaming(&service(vec![FieldKind::Url])));
        assert!(is_streaming(&service(vec![FieldKind::Note])));
        assert!(is_streaming(&service(Vec::new())));
    }
}
