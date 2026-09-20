//! The track metadata dialog, read-only or editable per `capabilities().edit_tags` (docs/PARITY.md §5.5).

use std::path::PathBuf;

use api::{ArtworkChange, TrackInfo, TrackMetadataPatch};
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::motion::{Easing, Presence, Transition};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, PathPromptOptions, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Task, Window, div, img, px,
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

/// Register the dialog so a track row's "View metadata" can open it.
pub fn register(cx: &mut App) {
    widgets::dialogs::register_track_metadata(open, cx);
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
    title: Entity<InputState>,
    artist: Entity<InputState>,
    album: Entity<InputState>,
    track_number: Entity<InputState>,
    disc_number: Entity<InputState>,
    cover: CoverEdit,
    _fetch: Task<()>,
    _save: Option<Task<()>>,
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
                Err(error) => Notices::error(error.to_string(), cx),
            })
            .ok();
        });
        Self {
            key,
            editable,
            track: None,
            editing: false,
            title: cx.new(|cx| InputState::new(window, cx)),
            artist: cx.new(|cx| InputState::new(window, cx)),
            album: cx.new(|cx| InputState::new(window, cx)),
            track_number: cx.new(|cx| InputState::new(window, cx)),
            disc_number: cx.new(|cx| InputState::new(window, cx)),
            cover: CoverEdit::Keep,
            _fetch: fetch,
            _save: None,
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
        if let Some(track) = self.track.clone() {
            self.adopt(track, window, cx);
        }
        cx.notify();
    }

    /// The editor sends the whole desired state, so a blank number clears that tag.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let track_number = self
            .value(&self.track_number, cx)
            .trim()
            .parse::<u32>()
            .ok();
        let disc_number = self.value(&self.disc_number, cx).trim().parse::<u32>().ok();
        let patch = TrackMetadataPatch {
            key: self.key.clone(),
            title: Some(self.value(&self.title, cx)),
            artist: Some(self.value(&self.artist, cx)),
            album: Some(self.value(&self.album, cx)),
            clear_track_number: track_number.is_none(),
            track_number,
            clear_disc_number: disc_number.is_none(),
            disc_number,
            cover: ArtworkChange::Keep,
        };
        let cover = self.cover.clone();
        self.editing = false;
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
                        Err(error) => {
                            cx.update(|_, cx| Notices::error(error.to_string(), cx))
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
            this.update_in(cx, |this, window, cx| match updated {
                Ok(track) => this.adopt(track, window, cx),
                Err(error) => Notices::error(error.to_string(), cx),
            })
            .ok();
        }));
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

    fn editor(&self, cx: &Context<Self>) -> AnyElement {
        v_flex()
            .gap_3()
            .child(self.field(t("title"), &self.title, cx))
            .child(self.field(t("artist"), &self.artist, cx))
            .child(self.field(t("album"), &self.album, cx))
            .child(
                h_flex()
                    .gap_3()
                    .child(div().flex_1().child(self.field(
                        t("track_number"),
                        &self.track_number,
                        cx,
                    )))
                    .child(div().flex_1().child(self.field(
                        t("disc_number"),
                        &self.disc_number,
                        cx,
                    ))),
            )
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
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                ),
                        )
                        .child(
                            Button::new("metadata-save")
                                .primary()
                                .label(t("save"))
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        ),
                )
            })
            .into_any_element()
    }
}
