//! Settings: offline downloads, metadata, the player and the tools category.

mod equalizer;
mod row;

use api::schema::FieldSpec;
use config::{
    AppConfig, ChannelMode, DeviceChangeBehavior, LYRICS_OFFSET_LIMIT_MS, OfflineQuality,
    SampleRateMode,
};
use equalizer::Equalizer;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled, Subscription, Window, div, rems,
};
use link::Link;
use row::{SettingRow, Toggle, section};
use store::{ConfigModel, PlayerModel, Query};
use widgets::format::t;
use widgets::{FieldChanged, SchemaForm, style};

/// `text-white/80`, what a slider's value reads in.
pub(crate) const READOUT: f32 = 0.80;

const CROSSFADE_MAX_SECONDS: f32 = 12.0;
const VOLUME_STEP_MIN_PERCENT: f32 = 1.0;
const VOLUME_STEP_MAX_PERCENT: f32 = 50.0;
const LYRICS_OFFSET_STEP_MS: f32 = 50.0;

pub struct PlaybackSettings {
    artwork: Entity<SchemaForm>,
    // The specs the form was last built from, so a refresh does not rebuild it under the caret.
    artwork_specs: Option<Vec<FieldSpec>>,
    artwork_settings: Entity<Query<Vec<FieldSpec>>>,
    crossfade: Entity<SliderState>,
    volume_scroll: Entity<SliderState>,
    lyrics_offset: Entity<SliderState>,
    equalizer: Entity<Equalizer>,
    section: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl PlaybackSettings {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = ConfigModel::global(cx);
        let (crossfade_seconds, volume_step, lyrics_offset_ms) = {
            let current = config.read(cx).config();
            (
                current.crossfade_seconds,
                current.volume_scroll_step,
                current.lyrics_offset_ms,
            )
        };
        let crossfade = slider(
            0.0,
            CROSSFADE_MAX_SECONDS,
            1.0,
            crossfade_seconds as f32,
            cx,
        );
        let volume_scroll = slider(
            VOLUME_STEP_MIN_PERCENT,
            VOLUME_STEP_MAX_PERCENT,
            1.0,
            volume_percent(volume_step),
            cx,
        );
        let limit = LYRICS_OFFSET_LIMIT_MS as f32;
        let lyrics_offset = slider(
            -limit,
            limit,
            LYRICS_OFFSET_STEP_MS,
            lyrics_offset_ms as f32,
            cx,
        );

        let artwork = cx.new(|cx| SchemaForm::new(Vec::new(), window, cx));
        let artwork_settings =
            cx.new(|cx| Query::new(&[], |api| async move { api.artwork_settings().await }, cx));
        let equalizer = cx.new(|cx| Equalizer::new(window, cx));

        let player = PlayerModel::global(cx);
        let mut subscriptions = vec![
            cx.observe_in(&config, window, |this, _, window, cx| this.sync(window, cx)),
            // The auto-offset row reads the engine's measured output latency.
            cx.observe(&player, |_, _, cx| cx.notify()),
            cx.observe_in(&artwork_settings, window, Self::adopt_artwork),
            cx.subscribe(&artwork, Self::save_artwork),
        ];
        subscriptions.push(on_slider_change(&crossfade, cx, |value, config| {
            config.crossfade_seconds = value.clamp(0.0, CROSSFADE_MAX_SECONDS) as u8;
        }));
        subscriptions.push(on_slider_change(&volume_scroll, cx, |value, config| {
            config.volume_scroll_step =
                value.clamp(VOLUME_STEP_MIN_PERCENT, VOLUME_STEP_MAX_PERCENT) / 100.0;
        }));
        subscriptions.push(on_slider_change(&lyrics_offset, cx, |value, config| {
            config.lyrics_offset_ms =
                (value as i32).clamp(-LYRICS_OFFSET_LIMIT_MS, LYRICS_OFFSET_LIMIT_MS);
        }));

        Self {
            section: None,
            artwork,
            artwork_specs: None,
            artwork_settings,
            crossfade,
            volume_scroll,
            lyrics_offset,
            equalizer,
            _subscriptions: subscriptions,
        }
    }

    /// Follow the config, so a daemon-side change or a managed file moves the sliders too.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = ConfigModel::global(cx);
        let (crossfade, volume, offset) = {
            let config = model.read(cx).config();
            (
                config.crossfade_seconds as f32,
                volume_percent(config.volume_scroll_step),
                config.lyrics_offset_ms as f32,
            )
        };
        set_slider(&self.crossfade, crossfade, window, cx);
        set_slider(&self.volume_scroll, volume, window, cx);
        set_slider(&self.lyrics_offset, offset, window, cx);
        cx.notify();
    }

    /// The daemon owns which artwork providers exist; a settled read is the whole form.
    fn adopt_artwork(
        &mut self,
        query: Entity<Query<Vec<FieldSpec>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = query.read(cx);
        if query.is_loading() {
            return;
        }
        let Some(specs) = query.value().cloned() else {
            return;
        };
        let locked = ConfigModel::global(cx).read(cx).locked_keys().to_vec();
        // `set_specs` rebuilds every input from scratch, and saving one field refreshes this
        // query — so without the guard, tabbing from one artwork field to the next saved the
        // first, refetched, and swapped the second's input out from under the caret.
        let changed = self.artwork_specs.as_ref() != Some(&specs);
        if changed {
            self.artwork_specs = Some(specs.clone());
        }
        self.artwork.update(cx, |form, cx| {
            if changed {
                form.set_specs(specs, window, cx);
            }
            form.set_locked(locked, cx);
        });
    }

    fn save_artwork(
        &mut self,
        _: Entity<SchemaForm>,
        event: &FieldChanged,
        cx: &mut Context<Self>,
    ) {
        let value = event.0.clone();
        let call = Link::global(cx)
            .call(move |api| async move { api.set_artwork_settings(vec![value]).await });
        cx.spawn(async move |this, cx| match call.await {
            Ok(_) => {
                this.update(cx, |this, cx| {
                    this.artwork_settings
                        .update(cx, |query, cx| query.refresh(cx));
                })
                .ok();
            }
            Err(error) => tracing::warn!(%error, "could not save the artwork settings"),
        })
        .detach();
    }

    fn offline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = ConfigModel::global(cx);
        let (quality, locked) = {
            let config = model.read(cx);
            (
                config.config().offline_quality,
                config.is_locked("offline_quality"),
            )
        };
        section(t("offline_downloads")).child(
            SettingRow::new(t("download_quality"))
                .locked(locked)
                .control(choice(
                    "offline-quality",
                    quality,
                    OfflineQuality::ALL,
                    |quality| SharedString::from(quality.label()),
                    locked,
                    |quality, config| config.offline_quality = quality,
                )),
        )
    }

    fn metadata(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = ConfigModel::global(cx);
        let settings = {
            let config = model.read(cx);
            LyricsSettings {
                prefer_local: config.config().prefer_local_lyrics,
                musixmatch: config.config().enable_musixmatch_lyrics,
                auto: config.config().lyrics_offset_auto,
                offset_ms: config.config().lyrics_offset_ms,
                prefer_local_locked: config.is_locked("prefer_local_lyrics"),
                musixmatch_locked: config.is_locked("enable_musixmatch_lyrics"),
                auto_locked: config.is_locked("lyrics_offset_auto"),
                offset_locked: config.is_locked("lyrics_offset_ms"),
            }
        };
        let latency_ms = PlayerModel::global(cx)
            .read(cx)
            .output_latency_ms()
            .unwrap_or(0);
        let offset_label = if settings.auto {
            format!("{latency_ms} ms")
        } else if settings.offset_ms == 0 {
            "0 ms".to_string()
        } else {
            format!("{:+} ms", settings.offset_ms)
        };

        section(t("metadata"))
            .child(div().px_5().py_2p5().child(self.artwork.clone()))
            .child(
                SettingRow::new(t("prefer_local_lyrics"))
                    .locked(settings.prefer_local_locked)
                    .control(
                        Toggle::new("prefer_local_lyrics")
                            .enabled(settings.prefer_local)
                            .disabled(settings.prefer_local_locked)
                            .on_change(cx.listener(|_, checked: &bool, _, cx| {
                                let checked = *checked;
                                write_config(cx, move |config| {
                                    config.prefer_local_lyrics = checked;
                                });
                            })),
                    ),
            )
            .child(
                SettingRow::new(t("enable_musixmatch_lyrics"))
                    .locked(settings.musixmatch_locked)
                    .control(
                        Toggle::new("enable_musixmatch_lyrics")
                            .enabled(settings.musixmatch)
                            .disabled(settings.musixmatch_locked)
                            .on_change(cx.listener(|_, checked: &bool, _, cx| {
                                let checked = *checked;
                                write_config(cx, move |config| {
                                    config.enable_musixmatch_lyrics = checked;
                                });
                            })),
                    ),
            )
            .child(
                SettingRow::new(t("lyrics_offset_auto"))
                    .description(SharedString::from(format!("{latency_ms} ms")))
                    .locked(settings.auto_locked)
                    .control(
                        Toggle::new("lyrics_offset_auto")
                            .enabled(settings.auto)
                            .disabled(settings.auto_locked)
                            .on_change(cx.listener(|_, checked: &bool, _, cx| {
                                let checked = *checked;
                                write_config(cx, move |config| {
                                    config.lyrics_offset_auto = checked;
                                });
                            })),
                    ),
            )
            .child(
                SettingRow::new(t("lyrics_offset"))
                    .locked(settings.offset_locked)
                    .control(readout(
                        Slider::new(&self.lyrics_offset)
                            .disabled(settings.offset_locked || settings.auto),
                        offset_label,
                        cx,
                    )),
            )
    }

    fn player(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = ConfigModel::global(cx);
        let settings = {
            let config = model.read(cx);
            PlayerSettings {
                crossfade_seconds: config.config().crossfade_seconds,
                volume_step: config.config().volume_scroll_step,
                channel_mode: config.config().channel_mode,
                device_change: config.config().device_change_behavior,
                sample_rate: config.config().sample_rate_mode,
                crossfade_locked: config.is_locked("crossfade_seconds"),
                volume_locked: config.is_locked("volume_scroll_step"),
                channel_locked: config.is_locked("channel_mode"),
                device_locked: config.is_locked("device_change_behavior"),
                sample_rate_locked: config.is_locked("sample_rate_mode"),
                equalizer_locked: config.is_locked("equalizer"),
            }
        };
        let crossfade_label = if settings.crossfade_seconds == 0 {
            t("crossfade_off")
        } else {
            SharedString::from(format!("{}s", settings.crossfade_seconds))
        };

        section(t("player_settings"))
            .child(
                SettingRow::new(t("crossfade"))
                    .locked(settings.crossfade_locked)
                    .control(readout(
                        Slider::new(&self.crossfade).disabled(settings.crossfade_locked),
                        crossfade_label,
                        cx,
                    )),
            )
            .child(
                SettingRow::new(t("volume_scroll_step"))
                    .locked(settings.volume_locked)
                    .control(readout(
                        Slider::new(&self.volume_scroll).disabled(settings.volume_locked),
                        SharedString::from(format!("{}%", volume_percent(settings.volume_step))),
                        cx,
                    )),
            )
            .child(
                SettingRow::new(t("channel_mode"))
                    .locked(settings.channel_locked)
                    .control(choice(
                        "channel-mode",
                        settings.channel_mode,
                        ChannelMode::ALL,
                        channel_mode_label,
                        settings.channel_locked,
                        |mode, config| config.channel_mode = mode,
                    )),
            )
            .child(
                SettingRow::new(t("device_change_behavior"))
                    .locked(settings.device_locked)
                    .control(choice(
                        "device-change-behavior",
                        settings.device_change,
                        DeviceChangeBehavior::ALL,
                        device_change_label,
                        settings.device_locked,
                        |behavior, config| config.device_change_behavior = behavior,
                    )),
            )
            .child(
                SettingRow::new(t("sample_rate_mode"))
                    .locked(settings.sample_rate_locked)
                    .control(choice(
                        "sample-rate-mode",
                        settings.sample_rate,
                        SampleRateMode::ALL,
                        sample_rate_label,
                        settings.sample_rate_locked,
                        |mode, config| config.sample_rate_mode = mode,
                    )),
            )
            .child(
                SettingRow::new(t("equalizer"))
                    .locked(settings.equalizer_locked)
                    .stacked()
                    .control(self.equalizer.clone()),
            )
    }

    // TODO(logs): opening and exporting the log folder is local filesystem work a socket-only client has no business doing.
    fn tools(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = ConfigModel::global(cx);
        let (tracing_enabled, locked) = {
            let config = model.read(cx);
            (
                config.config().tracing_enabled,
                config.is_locked("tracing_enabled"),
            )
        };
        let warning = cx.theme().warning.opacity(0.8);
        section(t("logs"))
            .child(
                SettingRow::new(t("enable_tracing")).locked(locked).control(
                    Toggle::new("tracing_enabled")
                        .enabled(tracing_enabled)
                        .disabled(locked)
                        .on_change(cx.listener(|_, checked: &bool, _, cx| {
                            let checked = *checked;
                            write_config(cx, move |config| config.tracing_enabled = checked);
                        })),
                ),
            )
            .child(
                div()
                    .px_5()
                    .pb_3()
                    .text_xs()
                    .text_color(warning)
                    .child(t("tracing_warning")),
            )
    }
}

impl PlaybackSettings {
    /// Which rail entry is showing; `None` stacks every section.
    pub fn show(&mut self, section: Option<SharedString>, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn shows(&self, section: &str) -> bool {
        self.section.as_deref().is_none_or(|shown| shown == section)
    }
}

impl Render for PlaybackSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The settings page owns the scroll; this panel is content inside it.
        v_flex()
            .id("playback-settings")
            .w_full()
            .gap_4()
            .pb_4()
            .when(self.shows("offline"), |panel| panel.child(self.offline(cx)))
            .when(self.shows("metadata"), |panel| {
                panel.child(self.metadata(cx))
            })
            .when(self.shows("player"), |panel| panel.child(self.player(cx)))
            .when(self.shows("tools"), |panel| panel.child(self.tools(cx)))
    }
}

/// What the metadata rows read, copied out so the config borrow ends before the rows are built.
struct LyricsSettings {
    prefer_local: bool,
    musixmatch: bool,
    auto: bool,
    offset_ms: i32,
    prefer_local_locked: bool,
    musixmatch_locked: bool,
    auto_locked: bool,
    offset_locked: bool,
}

struct PlayerSettings {
    crossfade_seconds: u8,
    volume_step: f32,
    channel_mode: ChannelMode,
    device_change: DeviceChangeBehavior,
    sample_rate: SampleRateMode,
    crossfade_locked: bool,
    volume_locked: bool,
    channel_locked: bool,
    device_locked: bool,
    sample_rate_locked: bool,
    equalizer_locked: bool,
}

/// Edit the daemon's settings; the write itself is debounced by [`ConfigModel`].
pub(crate) fn write_config(cx: &mut App, edit: impl FnOnce(&mut AppConfig)) {
    ConfigModel::global(cx).update(cx, |model, cx| model.update(cx, edit));
}

pub(crate) fn set_slider(
    state: &Entity<SliderState>,
    value: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if (state.read(cx).value().end() - value).abs() < f32::EPSILON {
        return;
    }
    state.update(cx, |state, cx| state.set_value(value, window, cx));
}

fn slider(
    min: f32,
    max: f32,
    step: f32,
    value: f32,
    cx: &mut Context<PlaybackSettings>,
) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(min)
            .max(max)
            .step(step)
            .default_value(value)
    })
}

fn on_slider_change(
    state: &Entity<SliderState>,
    cx: &mut Context<PlaybackSettings>,
    apply: impl Fn(f32, &mut AppConfig) + 'static,
) -> Subscription {
    cx.subscribe(state, move |_, _, event: &SliderEvent, cx| {
        if let SliderEvent::Change(value) = event {
            let value = value.end().round();
            write_config(cx, |config| apply(value, config));
        }
    })
}

/// A slider with its current value beside it, the way every numeric row here reads.
fn readout(slider: Slider, label: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_3()
        .items_center()
        .min_w(rems(13.75))
        .child(div().w_40().child(slider))
        .child(
            div()
                .w_16()
                .text_xs()
                .text_right()
                .text_color(style::white(READOUT, cx))
                .child(label.into()),
        )
}

/// One config value picked from a fixed set of variants.
fn choice<T: Copy + PartialEq + 'static>(
    id: &'static str,
    current: T,
    options: &'static [T],
    label: fn(T) -> SharedString,
    locked: bool,
    write: fn(T, &mut AppConfig),
) -> impl IntoElement {
    Button::new(id)
        .outline()
        .label(label(current))
        .disabled(locked)
        .dropdown_menu(move |mut menu, _, _| {
            for option in options.iter().copied() {
                menu = menu.item(
                    PopupMenuItem::new(label(option))
                        .checked(option == current)
                        .on_click(move |_, _, cx| {
                            write_config(cx, |config| write(option, config));
                        }),
                );
            }
            menu
        })
}

fn volume_percent(step: f32) -> f32 {
    (step * 100.0).round()
}

fn channel_mode_label(mode: ChannelMode) -> SharedString {
    t(match mode {
        ChannelMode::Stereo => "channel_mode_stereo",
        ChannelMode::Mono => "channel_mode_mono",
        ChannelMode::LeftOnly => "channel_mode_left_only",
        ChannelMode::RightOnly => "channel_mode_right_only",
        ChannelMode::SwapLeftRight => "channel_mode_swap_left_right",
    })
}

fn device_change_label(behavior: DeviceChangeBehavior) -> SharedString {
    t(match behavior {
        DeviceChangeBehavior::Resume => "device_change_resume",
        DeviceChangeBehavior::Pause => "device_change_pause",
    })
}

fn sample_rate_label(mode: SampleRateMode) -> SharedString {
    t(match mode {
        SampleRateMode::System => "sample_rate_mode_system",
        SampleRateMode::Source => "sample_rate_mode_source",
    })
}
