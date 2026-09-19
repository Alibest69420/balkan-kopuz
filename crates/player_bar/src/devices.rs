//! Where the active source can play, when it plays somewhere other than this app (docs/PARITY.md §3.10).

use api::ExternalDevice;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled, WeakEntity, div, px,
};
use store::{Chrome, Link, Notices, PlayerModel, SourcesModel};
use widgets::format::t;
use widgets::icons;

use crate::{PlayerBar, UTILITY_BUTTON, icon_button};

const PANEL_WIDTH: gpui_kit::Pixels = px(280.);
const ROW_ICON: gpui_kit::Pixels = px(18.);

/// The active source's id, but only while it has devices of its own and is signed in.
fn device_source(cx: &App) -> Option<String> {
    let sources = SourcesModel::global(cx);
    let source = sources.read(cx).active()?.clone();
    (source.capabilities.external_devices && source.authenticated).then_some(source.id)
}

impl PlayerBar {
    pub(crate) fn devices_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        device_source(cx)?;
        let elsewhere = PlayerModel::global(cx)
            .read(cx)
            .external()
            .is_some_and(|external| external.device.is_some());
        let color = if elsewhere {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        let devices = self.devices.clone();
        let bar = cx.entity().downgrade();

        Some(
            Popover::new("external-devices")
                .anchor(Anchor::BottomRight)
                .open(self.devices_open)
                .on_open_change(
                    cx.listener(|this, open: &bool, _, cx| this.set_devices_open(*open, cx)),
                )
                .trigger(
                    icon_button("devices", icons::SPEAKER, UTILITY_BUTTON)
                        .tooltip(t("external_play_on"))
                        .text_color(color),
                )
                .content(move |_, _, cx| device_list(devices.clone(), bar.clone(), cx)),
        )
    }

    fn set_devices_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.devices_open = open;
        if open {
            // The devices panel and the queue panel both want the right edge, so only one holds it.
            Chrome::global(cx).update(cx, |chrome, cx| chrome.set_panel(None, cx));
            self.refresh_devices(cx);
        }
        cx.notify();
    }

    /// A source switch invalidates the list; keep it only while the new source still offers one.
    pub(crate) fn forget_devices(&mut self, cx: &mut Context<Self>) {
        if device_source(cx).is_none() {
            self.devices_open = false;
            self.devices.clear();
        } else if self.devices_open {
            self.refresh_devices(cx);
        }
    }

    fn refresh_devices(&mut self, cx: &mut Context<Self>) {
        let Some(source) = device_source(cx) else {
            self.devices.clear();
            return;
        };
        let call =
            Link::global(cx).call(move |api| async move { api.external_devices(source).await });
        self._devices_fetch = Some(cx.spawn(async move |this, cx| {
            let devices = call.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                this.devices = devices;
                cx.notify();
            })
            .ok();
        }));
    }

    fn select_device(&mut self, device_id: Option<String>, cx: &mut Context<Self>) {
        let Some(source) = device_source(cx) else {
            return;
        };
        let call = Link::global(cx)
            .call(move |api| async move { api.select_external_device(source, device_id).await });
        cx.spawn(async move |this, cx| {
            if let Err(error) = call.await {
                cx.update(|cx| Notices::error(error.to_string(), cx));
            }
            this.update(cx, |this, cx| this.refresh_devices(cx)).ok();
        })
        .detach();
    }
}

fn device_list(
    devices: Vec<ExternalDevice>,
    bar: WeakEntity<PlayerBar>,
    cx: &App,
) -> impl IntoElement + use<> {
    let chosen = devices
        .iter()
        .find(|device| device.active)
        .map(|device| device.id.clone());

    v_flex()
        .w(PANEL_WIDTH)
        .gap_0p5()
        .child(
            div()
                .px_2()
                .pb_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t("external_play_on")),
        )
        .child(device_row(
            "external-this-app".into(),
            icons::MUSIC,
            t("external_this_app"),
            None,
            chosen.is_none(),
            None,
            bar.clone(),
            cx,
        ))
        .children(devices.iter().map(|device| {
            device_row(
                SharedString::from(device.id.clone()).into(),
                icons::SPEAKER,
                device.name.clone().into(),
                (!device.kind.is_empty()).then(|| device.kind.clone().into()),
                chosen.as_deref() == Some(device.id.as_str()),
                Some(device.id.clone()),
                bar.clone(),
                cx,
            )
        }))
}

#[allow(clippy::too_many_arguments)]
fn device_row(
    id: ElementId,
    glyph: &'static str,
    name: SharedString,
    subtitle: Option<SharedString>,
    chosen: bool,
    target: Option<String>,
    bar: WeakEntity<PlayerBar>,
    cx: &App,
) -> impl IntoElement + use<> {
    let accent = cx.theme().primary;
    let muted = cx.theme().muted_foreground;

    h_flex()
        .id(id)
        .w_full()
        .gap_3()
        .px_2()
        .py_1p5()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .when(chosen, |row| row.bg(cx.theme().list_active))
        .hover(|row| row.bg(cx.theme().list_hover))
        .on_click(move |_, _, cx| {
            bar.update(cx, |this, cx| this.select_device(target.clone(), cx))
                .ok();
        })
        .child(
            icons::icon(glyph)
                .size(ROW_ICON)
                .text_color(if chosen { accent } else { muted }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .when(chosen, |label| label.text_color(accent))
                        .child(name),
                )
                .when_some(subtitle, |column, subtitle| {
                    column.child(div().truncate().text_xs().text_color(muted).child(subtitle))
                }),
        )
        .when(chosen, |row| {
            row.child(icons::icon(icons::VOLUME).size(ROW_ICON).text_color(accent))
        })
}
