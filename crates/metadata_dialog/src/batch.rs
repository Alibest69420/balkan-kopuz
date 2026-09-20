//! One set of tags written across a whole selection: a blank field leaves that tag as it is.

use api::{ArtworkChange, TrackMetadataPatch};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Task, Window, div,
};
use store::{Link, Notices};
use widgets::format::{t, t_with};
use widgets::style;

use crate::caption;

pub(crate) fn open(keys: Vec<String>, window: &mut Window, cx: &mut App) {
    if keys.is_empty() {
        return;
    }
    let view = cx.new(|cx| BatchDialog::new(keys, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t("edit_metadata"))
            .w(crate::DIALOG_WIDTH)
            .child(view.clone())
            .footer(div())
    });
}

struct BatchDialog {
    keys: Vec<String>,
    artist: Entity<InputState>,
    album: Entity<InputState>,
    saving: bool,
    failure: Option<SharedString>,
    _save: Option<Task<()>>,
}

impl BatchDialog {
    fn new(keys: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            keys,
            artist: cx.new(|cx| InputState::new(window, cx).placeholder(t("metadata_batch_keep"))),
            album: cx.new(|cx| InputState::new(window, cx).placeholder(t("metadata_batch_keep"))),
            saving: false,
            failure: None,
            _save: None,
        }
    }

    fn typed(&self, input: &Entity<InputState>, cx: &Context<Self>) -> Option<String> {
        let value = input.read(cx).value().trim().to_string();
        (!value.is_empty()).then_some(value)
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let artist = self.typed(&self.artist, cx);
        let album = self.typed(&self.album, cx);
        if self.saving || (artist.is_none() && album.is_none()) {
            return;
        }
        let patches: Vec<TrackMetadataPatch> = self
            .keys
            .iter()
            .map(|key| TrackMetadataPatch {
                key: key.clone(),
                title: None,
                artist: artist.clone(),
                album: album.clone(),
                clear_track_number: false,
                track_number: None,
                clear_disc_number: false,
                disc_number: None,
                cover: ArtworkChange::Keep,
            })
            .collect();
        self.saving = true;
        self.failure = None;
        cx.notify();
        self._save = Some(cx.spawn_in(window, async move |this, cx| {
            let mut failure = None;
            for patch in patches {
                let Ok(call) = this.update(cx, |_, cx| {
                    Link::global(cx)
                        .call(move |api| async move { api.update_track_metadata(patch).await })
                }) else {
                    return;
                };
                if let Err(error) = call.await {
                    failure = Some(store::human(&error));
                    break;
                }
            }
            this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match failure {
                    // A write that failed partway leaves the rest untouched, so the box stays open.
                    Some(reason) => {
                        this.failure = Some(reason.clone());
                        Notices::error(reason, cx);
                        cx.notify();
                    }
                    None => window.close_dialog(cx),
                }
            })
            .ok();
        }));
    }
}

impl Render for BatchDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        let nothing =
            self.typed(&self.artist, cx).is_none() && self.typed(&self.album, cx).is_none();
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(style::slate_400(cx))
                    .child(t_with(
                        "metadata_batch_count",
                        &[("count", self.keys.len().to_string())],
                    )),
            )
            .child(self.field(t("artist"), &self.artist, cx))
            .child(self.field(t("album"), &self.album, cx))
            .when_some(self.failure.clone(), |body, reason| {
                body.child(
                    div()
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded(style::radius_lg())
                        .border_1()
                        .border_color(danger.opacity(0.3))
                        .bg(danger.opacity(0.1))
                        .text_sm()
                        .text_color(danger)
                        .child(reason),
                )
            })
            .child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(style::white(style::TEXT_DIM, cx))
                    .child(t("metadata_edit_warning")),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("batch-cancel")
                            .ghost()
                            .label(t("cancel"))
                            .disabled(self.saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("batch-save")
                            .primary()
                            .label(if self.saving { t("saving") } else { t("save") })
                            .loading(self.saving)
                            .disabled(self.saving || nothing)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

impl BatchDialog {
    fn field(
        &self,
        label: SharedString,
        input: &Entity<InputState>,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        v_flex().gap_1().child(caption(label, cx)).child(
            Input::new(input)
                .bg(style::white(style::HOVER, cx))
                .border_color(style::white(style::BORDER, cx))
                .rounded(style::radius_lg())
                .px_3()
                .py_2()
                .text_sm(),
        )
    }
}
