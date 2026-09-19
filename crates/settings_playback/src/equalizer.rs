//! The equalizer panel: enable, preset, preamp and the ten bands, heard live through `preview_equalizer`.

use config::{EqPreset, EqualizerSettings};
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Subscription, Window, div,
};
use link::Link;
use store::ConfigModel;
use widgets::format::t;

use crate::{set_slider, write_config};

const BAND_MIN_DB: f32 = -12.0;
const BAND_MAX_DB: f32 = 12.0;
const PREAMP_MIN_DB: f32 = -12.0;
const PREAMP_MAX_DB: f32 = 6.0;
const STEP_DB: f32 = 0.5;
const CONFIG_KEY: &str = "equalizer";

const BAND_LABELS: [&str; 10] = [
    "32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

pub struct Equalizer {
    preamp: Entity<SliderState>,
    bands: Vec<Entity<SliderState>>,
    _subscriptions: Vec<Subscription>,
}

impl Equalizer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = current(cx);
        let gains = settings.resolved_bands();
        let preamp = cx.new(|_| {
            SliderState::new()
                .min(PREAMP_MIN_DB)
                .max(PREAMP_MAX_DB)
                .step(STEP_DB)
                .default_value(settings.preamp_db)
        });
        let bands: Vec<Entity<SliderState>> = gains
            .iter()
            .map(|gain| {
                cx.new(|_| {
                    SliderState::new()
                        .min(BAND_MIN_DB)
                        .max(BAND_MAX_DB)
                        .step(STEP_DB)
                        .default_value(*gain)
                })
            })
            .collect();

        let config = ConfigModel::global(cx);
        let mut subscriptions = vec![
            cx.observe_in(&config, window, |this, _, window, cx| this.sync(window, cx)),
            cx.subscribe(&preamp, |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    let mut settings = current(cx);
                    settings.preamp_db = value.end();
                    apply(settings, cx);
                }
            }),
        ];
        for (index, band) in bands.iter().enumerate() {
            subscriptions.push(cx.subscribe(band, move |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    set_band(index, value.end(), cx);
                }
            }));
        }

        Self {
            preamp,
            bands,
            _subscriptions: subscriptions,
        }
    }

    /// Follow the config: a preset, a daemon-side change or another row moved the curve.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = current(cx);
        set_slider(&self.preamp, settings.preamp_db, window, cx);
        for (band, gain) in self.bands.iter().zip(settings.resolved_bands()) {
            set_slider(band, gain, window, cx);
        }
        cx.notify();
    }
}

impl Render for Equalizer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = ConfigModel::global(cx);
        let locked = model.read(cx).is_locked(CONFIG_KEY);
        let settings = current(cx);
        let enabled = settings.enabled;
        let preset = settings.preset;
        let gains = settings.resolved_bands();
        let muted = cx.theme().muted_foreground;

        v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .items_center()
                    .child(
                        Switch::new("eq-enabled")
                            .checked(enabled)
                            .disabled(locked)
                            .on_click(cx.listener(|_, checked: &bool, _, cx| {
                                let mut settings = current(cx);
                                settings.enabled = *checked;
                                apply(settings, cx);
                            })),
                    )
                    .child(div().text_sm().child(if enabled {
                        t("enabled")
                    } else {
                        t("disabled")
                    }))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(muted).child(t("eq_preset")))
                    .child(
                        Button::new("eq-preset")
                            .outline()
                            .label(preset_label(preset))
                            .disabled(locked)
                            .dropdown_menu(move |mut menu, _, _| {
                                for option in EqPreset::all() {
                                    menu = menu.item(
                                        PopupMenuItem::new(preset_label(option))
                                            .checked(option == preset)
                                            .on_click(move |_, _, cx| set_preset(option, cx)),
                                    );
                                }
                                menu
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(div().text_sm().child(t("eq_preamp")))
                            .child(div().text_xs().text_color(muted).child(t("eq_preamp_desc"))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Slider::new(&self.preamp).disabled(locked)),
                    )
                    .child(
                        div()
                            .w_16()
                            .text_xs()
                            .text_right()
                            .child(SharedString::from(format!("{:+.1} dB", settings.preamp_db))),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_between()
                    .items_end()
                    .children(self.bands.iter().enumerate().map(|(index, band)| {
                        v_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(SharedString::from(format!("{:+.1}", gains[index]))),
                            )
                            .child(Slider::new(band).vertical().disabled(locked))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(SharedString::from(BAND_LABELS[index])),
                            )
                    })),
            )
            .child(div().text_xs().text_color(muted).child(t("eq_graph_hint")))
    }
}

fn current(cx: &App) -> EqualizerSettings {
    ConfigModel::global(cx).read(cx).config().equalizer.clone()
}

/// Save the curve and let the engine play it at once; a preview is not persisted, the write is.
fn apply(settings: EqualizerSettings, cx: &mut App) {
    let preview = settings.clone();
    write_config(cx, move |config| config.equalizer = settings);
    Link::global(cx).fire(cx, "preview equalizer", move |api| async move {
        api.preview_equalizer(preview).await
    });
}

fn set_band(index: usize, gain: f32, cx: &mut App) {
    let mut settings = current(cx);
    let mut bands = settings.resolved_bands();
    let Some(band) = bands.get_mut(index) else {
        return;
    };
    *band = gain.clamp(BAND_MIN_DB, BAND_MAX_DB);
    settings.bands = bands;
    settings.preset = EqPreset::Custom;
    apply(settings, cx);
}

// TODO(animation): the Dioxus panel interpolated the bands between presets over ten frames.
fn set_preset(preset: EqPreset, cx: &mut App) {
    let mut settings = current(cx);
    settings.preset = preset;
    if let Some(preamp_db) = preset.default_preamp_db() {
        settings.preamp_db = preamp_db;
    }
    apply(settings, cx);
}

fn preset_label(preset: EqPreset) -> SharedString {
    t(match preset {
        EqPreset::Flat => "eq_preset_flat",
        EqPreset::BassBoost => "eq_preset_bass_boost",
        EqPreset::TrebleBoost => "eq_preset_treble_boost",
        EqPreset::VocalBoost => "eq_preset_vocal_boost",
        EqPreset::Loudness => "eq_preset_loudness",
        EqPreset::Custom => "eq_preset_custom",
    })
}
