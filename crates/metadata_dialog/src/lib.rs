//! The track metadata dialog, read-only or editable per `capabilities().edit_tags` (docs/PARITY.md §5.5).

use std::path::PathBuf;

use api::{ArtworkChange, TrackInfo, TrackMetadataPatch};
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::motion::{Easing, Presence, Transition};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, PathPromptOptions, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Subscription, Task, Window, div,
    img, px,
};
use store::{Link, Notices};
use widgets::format::{duration, t};
use widgets::{Cover, Loading, icons, motion, style};

// Measured geometry: `max-w-lg`, the `w-24 h-24` cover, its `rounded-lg`, the `max-h-[60vh]` body.
const DIALOG_WIDTH: Pixels = px(512.);
const COVER_SIZE: Pixels = px(96.);
const COVER_RADIUS: Pixels = px(8.);
const BODY_HEIGHT: Pixels = px(360.);
const LOADING_HEIGHT: Pixels = px(160.);

// `text-white/35` on a field label, `text-white/30` on the edit warning.
const LABEL_TEXT: f32 = 0.35;
const WARNING_TEXT: f32 = 0.30;

mod batch;

/// Register the dialog so a track row's "View metadata" can open it.
pub fn register(cx: &mut App) {
    widgets::dialogs::register_track_metadata(open, cx);
    widgets::dialogs::register_batch_metadata(batch::open, cx);
}

fn open(key: String, editable: bool, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| MetadataDialog::new(key, editable, window, cx));
    window.open_dialog(cx, move |dialog, _, cx| {
        let editing = view.read(cx).editing;
        dialog
            .title(if editing {
                t("edit_metadata")
            } else {
                t("metadata")
            })
            .w(DIALOG_WIDTH)
            // An edit in progress is typed text; only Cancel or Save may end it.
            .overlay_closable(!editing)
            .child(view.clone())
    });
}

/// What the editor will do to the embedded cover; a picked file is read when the patch is sent.
#[derive(Clone)]
enum CoverEdit {
    Keep,
    Remove,
    Set(PathBuf),
}

struct MetadataDialog {
    key: String,
    editable: bool,
    track: Option<TrackInfo>,
    editing: bool,
    saving: bool,
    // Why the last write did not land; the editor stays open holding it.
    failure: Option<SharedString>,
    title: Entity<InputState>,
    artist: Entity<InputState>,
    album: Entity<InputState>,
    track_number: Entity<InputState>,
    disc_number: Entity<InputState>,
    cover: CoverEdit,
    _fetch: Task<()>,
    _save: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl MetadataDialog {
    fn new(key: String, editable: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let call = {
            let key = key.clone();
            Link::global(cx).call(move |api| async move { api.tracks_by_keys(vec![key]).await })
        };
        let fetch = cx.spawn_in(window, async move |this, cx| {
            let found = call.await;
            this.update_in(cx, |this, window, cx| match found {
                Ok(rows) => match rows.into_iter().next() {
                    Some(track) => this.adopt(track, window, cx),
                    None => tracing::warn!(key = %this.key, "the track has no row to show"),
                },
                Err(error) => Notices::error(store::human(&error), cx),
            })
            .ok();
        });
        let track_number = cx.new(|cx| InputState::new(window, cx));
        let disc_number = cx.new(|cx| InputState::new(window, cx));
        // A number that will not parse is flagged as it is typed, not when Save is pressed.
        let subscriptions = vec![
            cx.subscribe(&track_number, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&disc_number, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        Self {
            key,
            editable,
            track: None,
            editing: false,
            saving: false,
            failure: None,
            title: cx.new(|cx| InputState::new(window, cx)),
            artist: cx.new(|cx| InputState::new(window, cx)),
            album: cx.new(|cx| InputState::new(window, cx)),
            track_number,
            disc_number,
            cover: CoverEdit::Keep,
            _fetch: fetch,
            _save: None,
            _subscriptions: subscriptions,
        }
    }

    /// Take a row as the truth, refilling the editor with it.
    fn adopt(&mut self, track: TrackInfo, window: &mut Window, cx: &mut Context<Self>) {
        let artist = if track.artists.is_empty() {
            track.artist.clone()
        } else {
            track.artists.join(", ")
        };
        let fields = [
            (&self.title, track.title.clone()),
            (&self.artist, artist),
            (&self.album, track.album.clone()),
            (&self.track_number, number(track.track_number)),
            (&self.disc_number, number(track.disc_number)),
        ];
        for (input, value) in fields {
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        self.track = Some(track);
        self.cover = CoverEdit::Keep;
        cx.notify();
    }

    fn value(&self, input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().to_string()
    }

    /// A picture is shown unless the editor removed the one the track had.
    fn has_cover(&self) -> bool {
        match self.cover {
            CoverEdit::Keep => self
                .track
                .as_ref()
                .is_some_and(|track| track.artwork.is_some()),
            CoverEdit::Remove => false,
            CoverEdit::Set(_) => true,
        }
    }

    fn pick_cover(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                this.update(cx, |this, cx| {
                    this.cover = CoverEdit::Set(path);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.failure = None;
        if let Some(track) = self.track.clone() {
            self.adopt(track, window, cx);
        }
        cx.notify();
    }

    /// Which number fields hold something that is neither blank nor a number.
    fn unreadable(&self, cx: &Context<Self>) -> (bool, bool) {
        (
            numbered(&self.value(&self.track_number, cx)) == Numbered::Unreadable,
            numbered(&self.value(&self.disc_number, cx)) == Numbered::Unreadable,
        )
    }

    /// The editor sends the whole desired state, so a blank number clears that tag.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (bad_track, bad_disc) = self.unreadable(cx);
        // Saving now would leave both tags exactly as they are, which is not what the box says.
        if self.saving || bad_track || bad_disc {
            return;
        }
        let track_number = numbered(&self.value(&self.track_number, cx));
        let disc_number = numbered(&self.value(&self.disc_number, cx));
        let patch = TrackMetadataPatch {
            key: self.key.clone(),
            title: Some(self.value(&self.title, cx)),
            artist: Some(self.value(&self.artist, cx)),
            album: Some(self.value(&self.album, cx)),
            clear_track_number: track_number == Numbered::Cleared,
            track_number: track_number.value(),
            clear_disc_number: disc_number == Numbered::Cleared,
            disc_number: disc_number.value(),
            cover: ArtworkChange::Keep,
        };
        let cover = self.cover.clone();
        self.saving = true;
        self.failure = None;
        cx.notify();
        self._save = Some(cx.spawn_in(window, async move |this, cx| {
            let cover = match cover {
                CoverEdit::Keep => ArtworkChange::Keep,
                CoverEdit::Remove => ArtworkChange::Remove,
                CoverEdit::Set(path) => {
                    let read = cx
                        .background_executor()
                        .spawn(async move { std::fs::read(path) })
                        .await;
                    match read {
                        Ok(bytes) => ArtworkChange::Set(bytes),
                        // Reading the picked file, not a daemon call: its own message is the one to show.
                        Err(error) => {
                            this.update(cx, |this, cx| this.failed(error.to_string(), cx))
                                .ok();
                            return;
                        }
                    }
                }
            };
            let patch = TrackMetadataPatch { cover, ..patch };
            let Ok(call) = this.update(cx, |_, cx| {
                Link::global(cx)
                    .call(move |api| async move { api.update_track_metadata(patch).await })
            }) else {
                return;
            };
            let updated = call.await;
            this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match updated {
                    Ok(track) => {
                        this.editing = false;
                        this.adopt(track, window, cx);
                    }
                    // Closing the editor here threw away every edit the daemon had just refused.
                    Err(error) => this.failed(store::human(&error), cx),
                }
            })
            .ok();
        }));
    }

    /// Keep the editor open holding what was typed, and say why it is still open.
    fn failed(&mut self, reason: impl Into<SharedString>, cx: &mut Context<Self>) {
        let reason = reason.into();
        self.saving = false;
        self.failure = Some(reason.clone());
        Notices::error(reason, cx);
        cx.notify();
    }

    fn cover_preview(&self, _: &Context<Self>) -> AnyElement {
        match &self.cover {
            CoverEdit::Set(path) => img(path.clone())
                .size(COVER_SIZE)
                .rounded(COVER_RADIUS)
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            CoverEdit::Remove => Cover::new(None)
                .size(COVER_SIZE)
                .rounded(COVER_RADIUS)
                .into_any_element(),
            CoverEdit::Keep => Cover::new(self.track.as_ref().and_then(|t| t.artwork.as_ref()))
                .size(COVER_SIZE)
                .rounded(COVER_RADIUS)
                .hq(true)
                .into_any_element(),
        }
    }

    fn field(&self, label: SharedString, input: &Entity<InputState>, cx: &Context<Self>) -> Div {
        self.checked_field(label, input, false, cx)
    }

    /// The same field, with the reason it is holding something that cannot be written.
    fn checked_field(
        &self,
        label: SharedString,
        input: &Entity<InputState>,
        problem: bool,
        cx: &Context<Self>,
    ) -> Div {
        let danger = cx.theme().danger;
        v_flex()
            .gap_1()
            .child(caption(label, cx))
            .child(
                Input::new(input)
                    .bg(style::white(style::HOVER, cx))
                    .border_color(if problem {
                        danger
                    } else {
                        style::white(style::BORDER, cx)
                    })
                    .rounded(style::radius_lg())
                    .px_3()
                    .py_2()
                    .text_sm(),
            )
            .when(problem, |field| {
                field.child(
                    div()
                        .text_xs()
                        .text_color(danger)
                        .child(t("metadata_number_invalid")),
                )
            })
    }

    fn editor(&self, cx: &Context<Self>) -> AnyElement {
        let (bad_track, bad_disc) = self.unreadable(cx);
        let danger = cx.theme().danger;
        v_flex()
            .gap_3()
            .child(self.field(t("title"), &self.title, cx))
            .child(self.field(t("artist"), &self.artist, cx))
            .child(self.field(t("album"), &self.album, cx))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().flex_1().child(self.checked_field(
                        t("track_number"),
                        &self.track_number,
                        bad_track,
                        cx,
                    )))
                    .child(div().flex_1().child(self.checked_field(
                        t("disc_number"),
                        &self.disc_number,
                        bad_disc,
                        cx,
                    ))),
            )
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
                    .text_color(style::white(WARNING_TEXT, cx))
                    .child(t("metadata_edit_warning")),
            )
            .into_any_element()
    }

    /// The read view, without the "path" row the Dioxus app filled with a `uid`.
    fn details(&self, track: &TrackInfo, cx: &Context<Self>) -> AnyElement {
        let mut rows: Vec<(SharedString, String)> = vec![
            (t("title"), self.value(&self.title, cx)),
            (t("artist"), self.value(&self.artist, cx)),
            (t("album"), self.value(&self.album, cx)),
            (t("track_number"), self.value(&self.track_number, cx)),
            (t("disc_number"), self.value(&self.disc_number, cx)),
        ];
        if let Some(secs) = track.duration_secs().filter(|secs| *secs > 0) {
            rows.push((t("duration"), duration(secs * 1000).to_string()));
        }
        // TODO(i18n): the units are the literals the Dioxus app printed.
        if track.khz > 0 {
            rows.push((
                t("sample_rate"),
                format!("{:.1} kHz", f64::from(track.khz) / 1000.0),
            ));
        }
        if track.bitrate > 0 {
            rows.push((t("bitrate"), format!("{} kbps", track.bitrate)));
        }
        let ids = [
            (t("musicbrainz_release"), &track.musicbrainz_release_id),
            (t("musicbrainz_recording"), &track.musicbrainz_recording_id),
            (t("musicbrainz_track"), &track.musicbrainz_track_id),
        ];
        for (label, id) in ids {
            rows.push((label, id.clone().unwrap_or_default()));
        }
        v_flex()
            .gap_3()
            .children(
                rows.into_iter()
                    .filter(|(_, value)| !value.trim().is_empty())
                    .map(|(label, value)| {
                        v_flex().gap_0p5().child(caption(label, cx)).child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(value),
                        )
                    }),
            )
            .into_any_element()
    }
}

/// `text-[10px] font-bold text-white/35`: the label over a value or an edit field.
fn caption(label: SharedString, cx: &App) -> Div {
    div()
        .text_size(style::text_10())
        .font_bold()
        .text_color(style::white(LABEL_TEXT, cx))
        .child(label)
}

fn number(value: Option<u32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

impl Render for MetadataDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(track) = self.track.clone() else {
            return div().h(LOADING_HEIGHT).child(Loading).into_any_element();
        };
        let editing = self.editing;
        let (bad_track, bad_disc) = self.unreadable(cx);
        let blocked = bad_track || bad_disc;
        let has_cover = self.has_cover();
        // Keying the fade on the mode makes each switch a fresh entrance rather than a cut.
        let mode = if editing { "edit" } else { "read" };
        let switched = Presence::new(("metadata-mode", mode), true)
            .transition(Transition::new(motion::MEDIUM).easing(Easing::EaseOut))
            .sample(window, cx)
            .progress;
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(self.cover_preview(cx))
                    .when(editing, |row| {
                        row.child(
                            v_flex()
                                .gap_2()
                                .child(
                                    Button::new("metadata-cover")
                                        .outline()
                                        .label(if has_cover {
                                            t("change_photo")
                                        } else {
                                            t("add_photo")
                                        })
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.pick_cover(cx)),
                                        ),
                                )
                                .when(has_cover, |column| {
                                    column.child(
                                        Button::new("metadata-cover-remove")
                                            .ghost()
                                            .icon(icons::icon(icons::TRASH))
                                            .label(t("remove_photo"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cover = CoverEdit::Remove;
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .id("metadata-body")
                    .max_h(BODY_HEIGHT)
                    .overflow_y_scroll()
                    .opacity(switched)
                    .child(if editing {
                        self.editor(cx)
                    } else {
                        self.details(&track, cx)
                    }),
            )
            .when(self.editable && !editing, |body| {
                body.child(
                    h_flex().justify_end().child(
                        Button::new("metadata-edit")
                            .outline()
                            .icon(icons::icon(icons::EDIT))
                            .label(t("edit"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editing = true;
                                cx.notify();
                            })),
                    ),
                )
            })
            .when(editing, |body| {
                body.child(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("metadata-cancel")
                                .ghost()
                                .label(t("cancel"))
                                .disabled(self.saving)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                ),
                        )
                        .child(
                            Button::new("metadata-save")
                                .primary()
                                .label(if self.saving { t("saving") } else { t("save") })
                                .loading(self.saving)
                                .disabled(self.saving || blocked)
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        ),
                )
            })
            .into_any_element()
    }
}

/// What a number field is asking for: blank clears the tag, a number sets it, anything else is a typo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numbered {
    Cleared,
    Set(u32),
    Unreadable,
}

impl Numbered {
    fn value(self) -> Option<u32> {
        match self {
            Numbered::Set(number) => Some(number),
            _ => None,
        }
    }
}

fn numbered(text: &str) -> Numbered {
    let text = text.trim();
    if text.is_empty() {
        return Numbered::Cleared;
    }
    match text.parse::<u32>() {
        Ok(number) => Numbered::Set(number),
        Err(_) => Numbered::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_that_will_not_parse_never_clears_the_tag() {
        assert_eq!(numbered(""), Numbered::Cleared);
        assert_eq!(numbered("   "), Numbered::Cleared);
        assert_eq!(numbered("7"), Numbered::Set(7));
        assert_eq!(numbered(" 7 "), Numbered::Set(7));
        for typo in ["A1", "03/12", "1-2", "one"] {
            assert_eq!(numbered(typo), Numbered::Unreadable, "{typo}");
            assert_eq!(numbered(typo).value(), None, "{typo}");
        }
    }
}
