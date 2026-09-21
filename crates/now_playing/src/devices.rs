//! Where the active source can play, when it plays somewhere other than this app.

use api::ExternalDevice;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled, WeakEntity, Window, div, px,
};
use store::{Chrome, Link, Notices, PlayerModel, SourcesModel};
use widgets::format::t;
use widgets::{icons, motion, style};

use crate::NowPlaying;
use crate::bar::utility;
use crate::channel;

const PANEL_WIDTH: gpui_kit::Pixels = px(280.);
// `w-10 h-10 rounded-md` with a `text-sm` glyph: the tile in front of a device's name.
const ROW_TILE: gpui_kit::Pixels = px(40.);
const ROW_ICON: gpui_kit::Pixels = px(14.);

/// The active source's id, but only while it has devices of its own and is signed in.
fn device_source(cx: &App) -> Option<String> {
    let sources = SourcesModel::global(cx);
    let source = sources.read(cx).active()?.clone();
    (source.capabilities.external_devices && source.authenticated).then_some(source.id)
}

impl NowPlaying {
    pub(crate) fn devices_button(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        device_source(cx)?;
        let elsewhere = PlayerModel::global(cx)
            .read(cx)
            .external()
            .is_some_and(|external| external.device.is_some());
        let tint = motion::color(
            "devices-glyph",
            if elsewhere {
                cx.theme().primary_hover
            } else {
                style::slate_400(cx)
            },
            window,
            cx,
        );
        let devices = self.devices.clone();
        let owner = cx.entity().downgrade();

        Some(
            Popover::new("external-devices")
                .anchor(Anchor::BottomRight)
                .open(self.devices_open)
                .on_open_change(
                    cx.listener(|this, open: &bool, _, cx| this.set_devices_open(*open, cx)),
                )
                .trigger(
                    utility("devices", icons::SPEAKER, tint, cx).tooltip(t("external_play_on")),
                )
                .content(move |_, window, cx| {
                    device_list(devices.clone(), owner.clone(), window, cx)
                }),
        )
    }

    fn set_devices_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.devices_open = open;
        if open {
            // The devices popover and the docked panel both want the right edge, so only one holds it.
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
                cx.update(|cx| Notices::error(store::human(&error), cx));
            }
            this.update(cx, |this, cx| this.refresh_devices(cx)).ok();
        })
        .detach();
    }
}

fn device_list(
    devices: Vec<ExternalDevice>,
    owner: WeakEntity<NowPlaying>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let chosen = devices
        .iter()
        .find(|device| device.active)
        .map(|device| device.id.clone());
    let rows: Vec<_> = devices
        .iter()
        .map(|device| {
            device_row(
                SharedString::from(device.id.clone()).into(),
                icons::SPEAKER,
                device.name.clone().into(),
                (!device.kind.is_empty()).then(|| device.kind.clone().into()),
                chosen.as_deref() == Some(device.id.as_str()),
                Some(device.id.clone()),
                owner.clone(),
                window,
                cx,
            )
        })
        .collect();

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
            owner,
            window,
            cx,
        ))
        .children(rows)
}

#[allow(clippy::too_many_arguments)]
fn device_row(
    id: ElementId,
    glyph: &'static str,
    name: SharedString,
    subtitle: Option<SharedString>,
    chosen: bool,
    target: Option<String>,
    owner: WeakEntity<NowPlaying>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let accent = cx.theme().primary;
    let raised = style::white(style::RAISED, cx);
    let wash_target = if chosen {
        style::accent(style::PLAYING, cx)
    } else {
        cx.theme().transparent
    };
    let tile_target = if chosen {
        style::accent(0.18, cx)
    } else {
        style::white(0.06, cx)
    };
    let mark_target = if chosen {
        accent
    } else {
        style::white(style::TEXT_SECONDARY, cx)
    };
    let name_target = if chosen {
        accent
    } else {
        cx.theme().foreground
    };
    let subtitle_target = if chosen {
        accent.opacity(0.7)
    } else {
        style::white(0.5, cx)
    };
    let wash = motion::color(channel(id.clone(), "wash"), wash_target, window, cx);
    let tile = motion::color(channel(id.clone(), "tile"), tile_target, window, cx);
    let mark = motion::color(channel(id.clone(), "mark"), mark_target, window, cx);
    let label = motion::color(channel(id.clone(), "label"), name_target, window, cx);
    let caption = motion::color(channel(id.clone(), "caption"), subtitle_target, window, cx);

    h_flex()
        .id(id)
        .w_full()
        .gap_3()
        .px_3()
        .py_2p5()
        .rounded(style::radius_lg())
        .cursor_pointer()
        .bg(wash)
        .hover(move |row| row.bg(raised))
        .on_click(move |_, _, cx| {
            owner
                .update(cx, |this, cx| this.select_device(target.clone(), cx))
                .ok();
        })
        .child(
            div()
                .size(ROW_TILE)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(style::thumb_radius())
                .bg(tile)
                .child(icons::icon(glyph).size(ROW_ICON).text_color(mark)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .justify_center()
                .child(div().truncate().text_sm().text_color(label).child(name))
                .when_some(subtitle, |column, subtitle| {
                    column.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(caption)
                            .child(subtitle),
                    )
                }),
        )
        .when(chosen, |row| {
            row.child(icons::icon(icons::VOLUME).size(ROW_ICON).text_color(accent))
        })
}
