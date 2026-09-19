//! The player bar: now playing, transport, seek, volume, and the toggles for the side panel and fullscreen (docs/PARITY.md §3.5, §3.10).

mod buffer;
mod devices;

use api::LoopMode;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, App, AppContext as _, Context, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollDelta,
    ScrollWheelEvent, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
    div, px, relative,
};
use store::{
    Chrome, ConfigModel, FavoritesModel, Navigator, PlayerModel, Playhead, QueueModel, Route,
    SidePanel, SourcesModel, actions,
};
use widgets::format::{duration, t};
use widgets::{Cover, icons, style};

// The bar's own height, measured from the Dioxus frontend's `h-24`.
const BAR_HEIGHT: Pixels = px(96.);
// The seek slider's hit area; the component's own track is exactly this tall.
const SEEK_TRACK: Pixels = px(24.);
// The drawn bar inside that track, and its offset, so the buffer underlay lines up with it.
const SEEK_BAR: Pixels = px(6.);
const SEEK_BAR_TOP: Pixels = px(9.);
// `w-14` with a `rounded-md` corner.
const COVER_SIZE: Pixels = px(56.);
const COVER_RADIUS: Pixels = px(6.);
const TRANSPORT_SIDE: Pixels = px(36.);
const TRANSPORT_STEP: Pixels = px(40.);
const UTILITY_BUTTON: Pixels = px(36.);
const TIME_LABEL_WIDTH: Pixels = px(32.);
const VOLUME_WIDTH: Pixels = px(96.);
// The glyph inside each of those: `text-sm`, `text-xl`, `text-lg`, `text-xs` and the heart's bare `1em`.
const SIDE_GLYPH: Pixels = px(14.);
const STEP_GLYPH: Pixels = px(20.);
const PLAY_GLYPH: Pixels = px(18.);
const UTILITY_GLYPH: Pixels = px(12.);
const MARK_GLYPH: Pixels = px(16.);
// `text-[9px] leading-none`: the "1" over a repeat-one button.
const BADGE_TEXT: Pixels = px(9.);
// `bg-black/40` when something is painted behind the bar, `bg-black/60` when nothing is.
const SCRIM_OVER_ART: f32 = 0.4;
const SCRIM_PLAIN: f32 = 0.6;
// A thousandth of the track is finer than one pixel of slider in any window this app opens in.
const SEEK_STEP: f32 = 0.001;
const VOLUME_STEP: f32 = 0.01;
// What unmuting restores when the bar was already silent at startup.
const DEFAULT_UNMUTE: f32 = 0.5;
const SILENT: f32 = f32::EPSILON;
const SCAN_PERIOD: std::time::Duration = std::time::Duration::from_millis(1_200);

/// The bar across the bottom of the window. It owns its own height.
pub struct PlayerBar {
    seek: Entity<SliderState>,
    volume: Entity<SliderState>,
    // The fraction under the pointer while a seek is being dragged; the commit sends it.
    seeking: Option<f32>,
    volume_before_mute: f32,
    devices_open: bool,
    devices: Vec<api::ExternalDevice>,
    _devices_fetch: Option<gpui_kit::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl PlayerBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let level = PlayerModel::global(cx).read(cx).volume();
        let seek = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(1.)
                .step(SEEK_STEP)
                .default_value(0.)
        });
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(1.)
                .step(VOLUME_STEP)
                .default_value(level)
        });

        let subscriptions = vec![
            cx.subscribe(&seek, |this, _, event: &SliderEvent, cx| {
                this.on_seek(event, cx)
            }),
            cx.subscribe(&volume, |this, _, event: &SliderEvent, cx| {
                this.on_volume(event, cx)
            }),
            cx.observe_in(&Playhead::global(cx), window, |this, _, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
            cx.observe(&FavoritesModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&QueueModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&Chrome::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| {
                this.forget_devices(cx);
                cx.notify();
            }),
        ];

        Self {
            seek,
            volume,
            seeking: None,
            volume_before_mute: if level > SILENT {
                level
            } else {
                DEFAULT_UNMUTE
            },
            devices_open: false,
            devices: Vec::new(),
            _devices_fetch: None,
            _subscriptions: subscriptions,
        }
    }

    /// Push the daemon's position and volume into the sliders, except one the pointer is holding.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let player = PlayerModel::global(cx);
        let (played, level) = {
            let player = player.read(cx);
            (played_fraction(player), player.volume())
        };
        if self.seeking.is_none() {
            self.seek
                .update(cx, |state, cx| state.set_value(played, window, cx));
        }
        self.volume
            .update(cx, |state, cx| state.set_value(level, window, cx));
    }

    fn on_seek(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => self.seeking = Some(value.end()),
            SliderEvent::Release(value) => {
                let fraction = value.end();
                self.seeking = None;
                if let Some(total) = seekable_duration_ms(cx) {
                    let position = (f64::from(fraction) * total as f64).round() as u64;
                    PlayerModel::global(cx)
                        .update(cx, |player, cx| player.seek(position.min(total), cx));
                }
            }
        }
        cx.notify();
    }

    fn on_volume(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => self.apply_volume(value.end(), cx),
            SliderEvent::Release(value) => self.persist_volume(value.end(), cx),
        }
    }

    fn apply_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        let level = level.clamp(0., 1.);
        if level > SILENT {
            self.volume_before_mute = level;
        }
        PlayerModel::global(cx).update(cx, |player, cx| player.set_volume(level, cx));
        cx.notify();
    }

    /// The level the app opens at next time; the config model debounces the write.
    fn persist_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        let level = level.clamp(0., 1.);
        ConfigModel::global(cx).update(cx, |config, cx| {
            if config.is_loaded() && config.config().volume != level {
                config.update(cx, |config| config.volume = level);
            }
        });
    }

    fn set_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        self.apply_volume(level, cx);
        self.persist_volume(level, cx);
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let level = PlayerModel::global(cx).read(cx).volume();
        if level > SILENT {
            self.volume_before_mute = level;
            self.set_volume(0., cx);
        } else if self.volume_before_mute > SILENT {
            self.set_volume(self.volume_before_mute, cx);
        } else {
            self.set_volume(DEFAULT_UNMUTE, cx);
        }
    }

    fn scroll_volume(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let step = ConfigModel::global(cx)
            .read(cx)
            .config()
            .volume_scroll_step
            .max(0.);
        let delta = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.y),
            ScrollDelta::Lines(delta) => delta.y,
        };
        if step <= 0. || delta == 0. {
            return;
        }
        let level = PlayerModel::global(cx).read(cx).volume();
        let direction = if delta > 0. { 1. } else { -1. };
        self.set_volume((level + direction * step).clamp(0., 1.), cx);
    }

    /// The signal is the source's, the skip is ours: it happens whether or not the daemon took it.
    fn dont_recommend(&mut self, key: String, cx: &mut Context<Self>) {
        let call =
            store::Link::global(cx).call(move |api| async move { api.dont_recommend(key).await });
        cx.spawn(async move |_, cx| {
            if let Err(error) = call.await {
                cx.update(|cx| store::Notices::error(error.to_string(), cx));
            }
            cx.update(|cx| PlayerModel::global(cx).update(cx, |player, cx| player.next(cx)));
        })
        .detach();
    }

    fn now_playing(&self, cx: &App) -> Option<api::NowPlaying> {
        PlayerModel::global(cx).read(cx).track().cloned()
    }

    /// The queue row the current track came from; only it knows which album to open.
    fn current_album_id(&self, cx: &App) -> Option<String> {
        let uid = self.now_playing(cx)?.uid;
        let queue = QueueModel::global(cx);
        let queue = queue.read(cx);
        queue
            .items()
            .iter()
            .find(|item| item.uid == uid)
            .or_else(|| queue.current())
            .map(|item| item.album_id.clone())
            .filter(|album_id| !album_id.is_empty())
    }
}

fn played_fraction(player: &PlayerModel) -> f32 {
    match track_duration_ms(player) {
        Some(total) => (player.position_ms() as f64 / total as f64).clamp(0., 1.) as f32,
        None => 0.,
    }
}

/// A playable length, so radio (which reports none) and a stopped player both answer `None`.
fn track_duration_ms(player: &PlayerModel) -> Option<u64> {
    player
        .duration_ms()
        .filter(|total| *total > 0 && *total != u64::MAX)
}

fn seekable_duration_ms(cx: &App) -> Option<u64> {
    let player = PlayerModel::global(cx);
    let player = player.read(cx);
    if player.is_loading() {
        return None;
    }
    track_duration_ms(player)
}

impl Render for PlayerBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (bar_is_fullscreen, over_art) = {
            let config = ConfigModel::global(cx);
            let config = config.read(cx);
            let config = config.config();
            (
                Chrome::global(cx).read(cx).is_fullscreen() && config.fullscreen_use_player_bar,
                config.cover_art_background || !config.custom_background_path.is_empty(),
            )
        };
        let scrim = if over_art {
            SCRIM_OVER_ART
        } else {
            SCRIM_PLAIN
        };
        h_flex()
            .w_full()
            .h(BAR_HEIGHT)
            .flex_shrink_0()
            .px_4()
            .justify_between()
            .border_t_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::black(scrim, cx))
            .child(self.now_playing_block(bar_is_fullscreen, cx))
            .child(self.transport_block(cx))
            .child(self.utilities_block(bar_is_fullscreen, cx))
    }
}

impl PlayerBar {
    fn now_playing_block(
        &self,
        bar_is_fullscreen: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let track = self.now_playing(cx);
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let key = track.as_ref().map(|track| track.key.clone());
        let favorite = key
            .as_deref()
            .is_some_and(|key| FavoritesModel::global(cx).read(cx).is_favorite(key));
        let title = SharedString::from(
            track
                .as_ref()
                .map(|track| track.title.clone())
                .unwrap_or_default(),
        );
        let artist = SharedString::from(
            track
                .as_ref()
                .map(|track| track.artist.clone())
                .unwrap_or_default(),
        );
        let album_id = self.current_album_id(cx);
        let muted = style::slate_400(cx);
        let hovered = style::white(0.7, cx);

        h_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .when(!bar_is_fullscreen, |block| {
                block
                    .child(
                        Cover::new(track.as_ref().and_then(|track| track.artwork.as_ref()))
                            .size(COVER_SIZE)
                            .rounded(COVER_RADIUS),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .id("now-playing-title")
                                    .truncate()
                                    .text_sm()
                                    .font_bold()
                                    .text_color(style::white(style::TEXT_STRONG, cx))
                                    .when(album_id.is_some(), |label| {
                                        label.cursor_pointer().hover(|label| label.underline())
                                    })
                                    .on_click(move |_, _, cx| {
                                        if let Some(album_id) = album_id.clone() {
                                            Navigator::navigate(Route::Album(album_id.into()), cx);
                                        }
                                    })
                                    .child(title),
                            )
                            .child(
                                div()
                                    .id("now-playing-artist")
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .when(!artist.is_empty(), |label| {
                                        label
                                            .cursor_pointer()
                                            .hover(move |label| label.text_color(hovered))
                                    })
                                    .on_click({
                                        let artist = artist.clone();
                                        move |_, _, cx| {
                                            if !artist.is_empty() {
                                                Navigator::navigate(
                                                    Route::Artist {
                                                        name: artist.clone(),
                                                        channel: None,
                                                    },
                                                    cx,
                                                );
                                            }
                                        }
                                    })
                                    .child(artist),
                            ),
                    )
            })
            .child(
                icon_button("favorite", icons::HEART, UTILITY_BUTTON, MARK_GLYPH)
                    .ml_2()
                    .tooltip(if favorite {
                        t("remove_from_favorites")
                    } else {
                        t("add_to_favorites")
                    })
                    .text_color(if favorite { cx.theme().danger } else { muted })
                    .disabled(key.is_none())
                    .on_click(cx.listener({
                        let key = key.clone();
                        move |_, _, _, cx| {
                            if let Some(key) = key.clone() {
                                FavoritesModel::global(cx)
                                    .update(cx, |favorites, cx| favorites.toggle(key, cx));
                            }
                        }
                    })),
            )
            .when(capabilities.dont_recommend, |block| {
                block.child(
                    icon_button(
                        "dont-recommend",
                        icons::THUMBS_DOWN,
                        UTILITY_BUTTON,
                        MARK_GLYPH,
                    )
                    .tooltip(t("dont_recommend"))
                    .text_color(if favorite { muted.opacity(0.4) } else { muted })
                    // Like and dislike are one setting on the remote, so both at once would undo the heart.
                    .disabled(key.is_none() || favorite)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(key) = key.clone() {
                            this.dont_recommend(key, cx);
                        }
                    })),
                )
            })
    }

    fn transport_block(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        v_flex()
            .w(relative(0.4))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_2()
            .child(self.transport_buttons(cx))
            .child(self.seek_row(cx))
    }

    fn transport_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (playing, shuffle, loop_mode) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (player.is_playing(), player.shuffle(), player.loop_mode())
        };
        let accent = cx.theme().primary;
        let idle = style::slate_400(cx);

        h_flex()
            .gap_2()
            .items_center()
            .child(
                icon_button("shuffle", icons::SHUFFLE, TRANSPORT_SIDE, SIDE_GLYPH)
                    .tooltip(if shuffle {
                        t("shuffle_on")
                    } else {
                        t("shuffle_off")
                    })
                    .text_color(if shuffle { accent } else { idle })
                    .on_click(move |_, _, cx| {
                        PlayerModel::global(cx)
                            .update(cx, |player, cx| player.set_shuffle(!shuffle, cx));
                    }),
            )
            .child(
                icon_button("previous", icons::SKIP_BACK, TRANSPORT_STEP, STEP_GLYPH)
                    .text_color(idle)
                    .on_click(|_, _, cx| {
                        PlayerModel::global(cx).update(cx, |player, cx| player.previous(cx));
                    }),
            )
            .child(
                icon_button(
                    "play-pause",
                    if playing { icons::PAUSE } else { icons::PLAY },
                    TRANSPORT_STEP,
                    PLAY_GLYPH,
                )
                .text_color(cx.theme().foreground)
                .on_click(|_, _, cx| {
                    PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                }),
            )
            .child(
                icon_button("next", icons::SKIP_FORWARD, TRANSPORT_STEP, STEP_GLYPH)
                    .text_color(idle)
                    .on_click(|_, _, cx| {
                        PlayerModel::global(cx).update(cx, |player, cx| player.next(cx));
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .child(
                        icon_button("repeat", icons::REPEAT, TRANSPORT_SIDE, SIDE_GLYPH)
                            .tooltip(match loop_mode {
                                LoopMode::None => t("repeat_off"),
                                LoopMode::Queue => t("repeat_queue"),
                                LoopMode::Track => t("repeat_track"),
                            })
                            .text_color(if loop_mode == LoopMode::None {
                                idle
                            } else {
                                accent
                            })
                            .on_click(|_, _, cx| {
                                PlayerModel::global(cx)
                                    .update(cx, |player, cx| player.cycle_loop_mode(cx));
                            }),
                    )
                    .when(loop_mode == LoopMode::Track, |wrapper| {
                        wrapper.child(
                            div()
                                .absolute()
                                .bottom(px(2.))
                                .left_0()
                                .right_0()
                                .text_center()
                                .text_size(BADGE_TEXT)
                                .line_height(relative(1.))
                                .font_bold()
                                .text_color(accent)
                                .child("1"),
                        )
                    }),
            )
    }

    fn seek_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (position_ms, total_ms, loading, ranges) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (
                player.position_ms(),
                track_duration_ms(player),
                player.is_loading(),
                player.buffered().to_vec(),
            )
        };
        let can_seek = total_ms.is_some() && !loading;
        let played = match (self.seeking, total_ms) {
            (Some(fraction), Some(_)) => fraction,
            (None, Some(total)) => (position_ms as f64 / total as f64).clamp(0., 1.) as f32,
            _ => 0.,
        };
        let shown_ms = match (self.seeking, total_ms) {
            (Some(fraction), Some(total)) => (f64::from(fraction) * total as f64) as u64,
            _ => position_ms,
        };
        let bars = buffer::visual_ranges(&ranges, played * 100.);
        let dim = style::slate_500(cx);

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(time_label(duration(shown_ms), dim).text_right())
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(SEEK_TRACK)
                    .child(buffer_underlay(bars, loading, cx))
                    .child(
                        Slider::new(&self.seek)
                            .disabled(!can_seek)
                            .bg(style::white(style::TEXT_STRONG, cx)),
                    ),
            )
            .child(time_label(
                match total_ms {
                    Some(total) => duration(total),
                    None => "--:--".into(),
                },
                dim,
            ))
    }

    fn utilities_block(
        &self,
        bar_is_fullscreen: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (panel, compact) = {
            let chrome = Chrome::global(cx);
            let chrome = chrome.read(cx);
            (chrome.panel(), chrome.is_compact())
        };
        let key = self.now_playing(cx).map(|track| track.key);
        let muted = style::slate_400(cx);
        let accent = cx.theme().primary;

        h_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .justify_end()
            .child(self.volume_control(cx))
            .child(
                icon_button("queue-panel", icons::LIST, UTILITY_BUTTON, UTILITY_GLYPH)
                    .tooltip(t("up_next"))
                    .text_color(if panel == Some(SidePanel::Queue) {
                        accent
                    } else {
                        muted
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.devices_open = false;
                        Chrome::global(cx)
                            .update(cx, |chrome, cx| chrome.toggle_panel(SidePanel::Queue, cx));
                    })),
            )
            .child(
                icon_button("lyrics-panel", icons::LYRICS, UTILITY_BUTTON, UTILITY_GLYPH)
                    .tooltip(t("lyrics"))
                    .text_color(if panel == Some(SidePanel::Lyrics) {
                        accent
                    } else {
                        muted
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.devices_open = false;
                        Chrome::global(cx)
                            .update(cx, |chrome, cx| chrome.toggle_panel(SidePanel::Lyrics, cx));
                    })),
            )
            .children(self.devices_button(cx))
            .child(
                icon_button("share", icons::SHARE, UTILITY_BUTTON, UTILITY_GLYPH)
                    .tooltip(t("share_musicbrainz"))
                    .text_color(muted)
                    .disabled(key.is_none())
                    .on_click(move |_, _, cx| {
                        if let Some(key) = key.clone() {
                            actions::share_track(key, cx);
                        }
                    }),
            )
            .child(
                icon_button(
                    "mini-player",
                    icons::COLLAPSE,
                    UTILITY_BUTTON,
                    UTILITY_GLYPH,
                )
                .tooltip(t("mini_player"))
                .text_color(muted)
                .on_click(move |_, _, cx| {
                    Chrome::global(cx).update(cx, |chrome, cx| chrome.set_compact(!compact, cx))
                }),
            )
            .when(!bar_is_fullscreen, |block| {
                block.child(
                    icon_button("fullscreen", icons::EXPAND, UTILITY_BUTTON, UTILITY_GLYPH)
                        // TODO(i18n): the Dioxus bar gave this button an icon and no label.
                        .tooltip("Fullscreen")
                        .text_color(muted)
                        .on_click(|_, _, cx| {
                            Chrome::global(cx)
                                .update(cx, |chrome, cx| chrome.set_fullscreen(true, cx))
                        }),
                )
            })
    }

    fn volume_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let level = PlayerModel::global(cx).read(cx).volume();
        let glyph = if level <= SILENT {
            icons::VOLUME_MUTED
        } else if level < 0.5 {
            icons::VOLUME_LOW
        } else {
            icons::VOLUME
        };

        h_flex()
            .id("volume")
            .gap_2()
            .items_center()
            .on_scroll_wheel(
                cx.listener(|this, event: &ScrollWheelEvent, _, cx| this.scroll_volume(event, cx)),
            )
            .child(
                icon_button("mute", glyph, UTILITY_BUTTON, UTILITY_GLYPH)
                    .tooltip(t("volume"))
                    .text_color(style::slate_400(cx))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_mute(cx))),
            )
            .child(
                div()
                    .w(VOLUME_WIDTH)
                    .child(Slider::new(&self.volume).bg(style::white(style::TEXT_STRONG, cx))),
            )
    }
}

/// Bytes already fetched, drawn under the played position; before any totals arrive, a scan says loading is live.
fn buffer_underlay(
    bars: Vec<(f32, f32)>,
    loading: bool,
    cx: &mut Context<PlayerBar>,
) -> impl IntoElement + use<> {
    let scanning = loading && bars.is_empty();
    let buffered = style::white(style::TEXT_DIM, cx);
    let scan = cx.theme().primary;
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(SEEK_BAR_TOP)
        .h(SEEK_BAR)
        .overflow_hidden()
        .rounded_full()
        .children(bars.into_iter().map(|(left, right)| {
            div()
                .absolute()
                .top_0()
                .h_full()
                .left(relative(left / 100.))
                .w(relative((right - left).max(0.) / 100.))
                .rounded_full()
                .bg(buffered)
        }))
        .when(scanning, |track| {
            track.child(
                div()
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(relative(0.25))
                    .rounded_full()
                    .bg(scan)
                    .with_animation(
                        "seek-scan",
                        Animation::new(SCAN_PERIOD).repeat(),
                        |bar, progress| bar.left(relative(progress * 1.25 - 0.25)),
                    ),
            )
        })
}

/// A circular ghost button carrying one glyph: the shape every control in this bar takes.
fn icon_button(id: &'static str, glyph: &'static str, size: Pixels, glyph_size: Pixels) -> Button {
    // Button draws its glyph at three quarters of its own size, so the box is restated after it.
    Button::new(id)
        .ghost()
        .with_size(glyph_size * (4. / 3.))
        .size(size)
        .rounded(size / 2.)
        .icon(icons::icon(glyph))
}

fn time_label(text: SharedString, color: Hsla) -> gpui_kit::Div {
    div()
        .w(TIME_LABEL_WIDTH)
        .flex_shrink_0()
        .text_size(style::text_10())
        .text_color(color)
        .child(text)
}
