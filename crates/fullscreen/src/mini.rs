use gpui_kit::base::slider::{SliderEvent, SliderState};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Pixels, Render,
    SharedString, Styled, Subscription, Window, div, px,
};
use store::{Chrome, PlayerModel, Playhead};
use widgets::format::{duration, t};
use widgets::{Cover, icons, style};

use crate::background;
use crate::controls;

// The mini surface is measured geometry: it is drawn for a window the shell fixes at ~380×148.
const COVER: Pixels = px(56.);
const CONTROL: Pixels = px(36.);
const PLAY: Pixels = px(40.);
const PROGRESS_HEIGHT: Pixels = px(3.);
// `text-[9px]`, below anything the shared scale names.
const CLOCK_SIZE: Pixels = px(9.);

/// The compact surface, drawn for the ~380×148 window the shell shrinks to.
pub struct MiniPlayer {
    seek: Entity<SliderState>,
    seeking: bool,
    _subscriptions: Vec<Subscription>,
}

impl MiniPlayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let seek = cx.new(|_| controls::fraction_state(0.001));
        let player = PlayerModel::global(cx);
        let playhead = Playhead::global(cx);
        let subscriptions = vec![
            cx.observe_in(&player, window, |this, _, window, cx| this.sync(window, cx)),
            cx.observe_in(&playhead, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
            cx.subscribe_in(&seek, window, Self::on_seek),
        ];
        Self {
            seek,
            seeking: false,
            _subscriptions: subscriptions,
        }
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.seeking {
            let player = PlayerModel::global(cx);
            let (position, total) = {
                let player = player.read(cx);
                (player.position_ms(), player.duration_ms())
            };
            let value = controls::fraction(position, total);
            self.seek
                .update(cx, |seek, cx| seek.set_value(value, window, cx));
        }
        cx.notify();
    }

    fn on_seek(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SliderEvent::Change(_) => self.seeking = true,
            SliderEvent::Release(value) => {
                self.seeking = false;
                let player = PlayerModel::global(cx);
                let Some(total) = player.read(cx).duration_ms() else {
                    return;
                };
                let position = (value.end().clamp(0., 1.) * total as f32) as u64;
                player.update(cx, |player, cx| player.seek(position, cx));
            }
        }
    }
}

impl Render for MiniPlayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = PlayerModel::global(cx);
        let (track, playing, radio, loading, position, total) = {
            let player = player.read(cx);
            (
                player.track().cloned(),
                player.is_playing(),
                player.is_radio(),
                player.is_loading(),
                player.position_ms(),
                player.duration_ms(),
            )
        };
        let clock: SharedString = if radio {
            // TODO(i18n): the Dioxus mini player wrote this in English too.
            "LIVE".into()
        } else {
            format!("{} / {}", duration(position), duration(total.unwrap_or(0))).into()
        };
        let artwork = track.as_ref().and_then(|track| track.artwork.clone());
        let title = track
            .as_ref()
            .map(|track| SharedString::from(track.title.clone()))
            .unwrap_or_default();
        let artist = track
            .as_ref()
            .map(|track| SharedString::from(track.artist.clone()))
            .unwrap_or_default();
        let seekable = track.as_ref().is_some_and(|track| track.seekable);

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(background::cover_backdrop(artwork.as_ref(), cx))
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h_0()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .pt_2()
                            .pb_1()
                            .child(
                                Cover::new(artwork.as_ref())
                                    .size(COVER)
                                    .rounded(px(8.)),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .justify_center()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::BOLD)
                                            .truncate()
                                            .text_color(style::white(0.95, cx))
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(style::text_11())
                                            .truncate()
                                            .text_color(style::white(0.55, cx))
                                            .child(artist),
                                    )
                                    .child(
                                        div()
                                            .text_size(CLOCK_SIZE)
                                            .text_color(style::white(0.35, cx))
                                            .child(clock),
                                    ),
                            )
                            .child(
                                Button::new("mini-restore")
                                    .ghost()
                                    .rounded(px(999.))
                                    .with_size(CONTROL)
                                    .icon(icons::icon(icons::EXPAND).xsmall())
                                    .tooltip(t("restore_full_player"))
                                    .on_click(|_, _, cx| {
                                        Chrome::global(cx)
                                            .update(cx, |chrome, cx| chrome.set_compact(false, cx))
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .px_3()
                            .pb_2()
                            .child(
                                controls::previous_button("mini-previous")
                                    .rounded(px(999.))
                                    .with_size(CONTROL),
                            )
                            .child(
                                controls::play_button("mini-play", playing).with_size(PLAY),
                            )
                            .child(
                                controls::next_button("mini-next")
                                    .rounded(px(999.))
                                    .with_size(CONTROL),
                            ),
                    )
                    .child(controls::thin_slider(
                        &self.seek,
                        PROGRESS_HEIGHT,
                        radio || loading || !seekable,
                        cx,
                    )),
            )
    }
}
