//! Settings → General and Appearance, plus the custom theme editor embedded under them.

mod row;
mod theme_editor;

use config::{AppConfig, BackBehavior, PlayerBarPosition};
use gpui_kit::component::button::{Button, ButtonVariants as _, ButtonCustomVariant};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window, div, rems,
};
use row::{SettingRow, section};
use store::ConfigModel;
use theme_editor::ThemeEditor;
use widgets::format::t;
use widgets::{icons, style};

/// `text-white/80`, what a slider's value reads in.
const READOUT: f32 = 0.80;
/// `text-white/50`, the path a file setting shows.
const PATH: f32 = 0.50;
/// `w-40`, the width every settings slider is drawn at.
fn slider_width() -> gpui_kit::Rems {
    rems(10.)
}
/// `.settings-select`: `width: min(14rem, …)`.
fn select_width() -> gpui_kit::Rems {
    rems(14.)
}
/// `bg-white/10 hover:bg-white/20`, the quiet button the settings rows use.
fn soft(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(style::white(style::HOVER, cx))
        .hover(style::white(style::ACTIVE, cx))
        .active(style::white(style::ACTIVE, cx))
        .foreground(cx.theme().foreground)
}

/// The i18n key each built-in theme is listed under; `kettek16` is a name, not a phrase.
fn theme_label(id: &str) -> SharedString {
    let key = match id {
        "default" => "default_theme",
        "amoled" => "amoled_black",
        "gruvbox" => "gruvbox_material",
        "gruvbox-classic" => "gruvbox_classic",
        "gruvbox-dark-soft" => "gruvbox_dark_soft",
        "catppuccin" => "catppuccin_mocha",
        "ef-night" => "ef_night",
        "ayu-dark" => "ayu_dark",
        "ayu-mirage" => "ayu_mirage",
        "onedarkpro" => "one_dark_pro",
        "kanagawa-dragon" => "kanagawa_dragon",
        "default-light" => "default_light",
        "catppuccin-latte" => "catppuccin_latte",
        "rosepine-dawn" => "rosepine_dawn",
        "everforest-light" => "everforest_light",
        "ayu-light" => "ayu_light",
        "one-light" => "one_light",
        "gruvbox-light" => "gruvbox_light_soft",
        "dracula" | "nord" | "vague" | "osmium" | "everforest" | "rosepine" => id,
        other => return SharedString::from(other.to_string()),
    };
    t(key)
}

fn edit(cx: &mut App, change: impl FnOnce(&mut AppConfig)) {
    ConfigModel::global(cx).update(cx, |model, cx| model.update(cx, change));
}

fn set_slider(state: &Entity<SliderState>, value: f32, window: &mut Window, cx: &mut App) {
    if state.read(cx).value().start() == value {
        return;
    }
    state.update(cx, |state, cx| state.set_value(value, window, cx));
}

pub struct AppearanceSettings {
    editor: Entity<ThemeEditor>,
    darkening: Entity<SliderState>,
    blur: Entity<SliderState>,
    lyrics_blur: Entity<SliderState>,
    section: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl AppearanceSettings {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = ConfigModel::global(cx);
        let cfg = config.read(cx).config().clone();
        let slider = |min: f32, max: f32, step: f32, value: f32, cx: &mut Context<Self>| {
            cx.new(|_| {
                SliderState::new()
                    .min(min)
                    .max(max)
                    .step(step)
                    .default_value(value)
            })
        };
        let darkening = slider(0., 95., 5., cfg.cover_art_darkening as f32, cx);
        let blur = slider(0., 100., 5., cfg.cover_art_blur as f32, cx);
        let lyrics_blur = slider(10., 200., 10., cfg.lyrics_depth_blur_strength as f32, cx);

        let subscriptions = vec![
            cx.observe_in(&config, window, |this, _, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
            cx.subscribe(&darkening, |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    let percent = value.start().round().clamp(0., 95.) as u8;
                    edit(cx, |cfg| cfg.cover_art_darkening = percent);
                }
            }),
            cx.subscribe(&blur, |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    let radius = value.start().round().clamp(0., 100.) as u8;
                    edit(cx, |cfg| cfg.cover_art_blur = radius);
                }
            }),
            cx.subscribe(&lyrics_blur, |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    let strength = value.start().round().clamp(10., 200.) as u8;
                    edit(cx, |cfg| cfg.lyrics_depth_blur_strength = strength);
                }
            }),
        ];

        Self {
            editor: cx.new(|cx| ThemeEditor::new(window, cx)),
            darkening,
            blur,
            lyrics_blur,
            section: None,
            _subscriptions: subscriptions,
        }
    }

    /// The daemon can answer with values this app did not type, so the sliders follow the config.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = ConfigModel::global(cx).read(cx).config().clone();
        set_slider(&self.darkening, cfg.cover_art_darkening as f32, window, cx);
        set_slider(&self.blur, cfg.cover_art_blur as f32, window, cx);
        set_slider(
            &self.lyrics_blur,
            cfg.lyrics_depth_blur_strength as f32,
            window,
            cx,
        );
    }

    fn row(&self, label: SharedString, key: &str, control: AnyElement, cx: &App) -> SettingRow {
        let locked = ConfigModel::global(cx).read(cx).is_locked(key);
        SettingRow::new(label).locked(locked).control(control)
    }

    fn toggle(
        &self,
        key: &'static str,
        checked: bool,
        change: impl Fn(&mut AppConfig, bool) + 'static,
        cx: &App,
    ) -> AnyElement {
        let locked = ConfigModel::global(cx).read(cx).is_locked(key);
        Switch::new(key)
            .checked(checked)
            .disabled(locked)
            .on_click(move |value: &bool, _, cx| {
                let value = *value;
                edit(cx, |cfg| change(cfg, value));
            })
            .into_any_element()
    }

    /// One control shape for every enumerated setting: a button that drops its options down.
    fn choice<T: Clone + PartialEq + 'static>(
        &self,
        key: &'static str,
        current: &T,
        options: Vec<(T, SharedString)>,
        change: impl Fn(&mut AppConfig, T) + Clone + 'static,
        cx: &App,
    ) -> AnyElement {
        let locked = ConfigModel::global(cx).read(cx).is_locked(key);
        let label = options
            .iter()
            .find(|(value, _)| value == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| t("choose"));
        // A disabled trigger still opens its dropdown, so a locked row gets a plain button.
        if locked {
            return Button::new(key)
                .outline()
                .w(select_width())
                .justify_between()
                .label(label)
                .disabled(true)
                .into_any_element();
        }
        Button::new(key)
            .outline()
            .w(select_width())
            .justify_between()
            .icon(icons::icon(icons::CHEVRON_DOWN))
            .label(label)
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in &options {
                    let value = value.clone();
                    let change = change.clone();
                    menu =
                        menu.item(PopupMenuItem::new(label.clone()).on_click(move |_, _, cx| {
                            let value = value.clone();
                            edit(cx, |cfg| change(cfg, value));
                        }));
                }
                menu
            })
            .into_any_element()
    }

    fn slider_row(
        &self,
        key: &'static str,
        state: &Entity<SliderState>,
        readout: SharedString,
        cx: &App,
    ) -> AnyElement {
        let locked = ConfigModel::global(cx).read(cx).is_locked(key);
        h_flex()
            .gap_3()
            .items_center()
            .min_w(rems(13.75))
            .child(
                div()
                    .w(slider_width())
                    .child(Slider::new(state).disabled(locked)),
            )
            .child(
                div()
                    .w_16()
                    .text_xs()
                    .text_right()
                    .text_color(style::white(READOUT, cx))
                    .child(readout),
            )
            .into_any_element()
    }

    /// Path settings share one shape: the chosen path, Remove when set, and the picker button.
    fn path_row(
        &self,
        key: &'static str,
        path: String,
        choose: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locked = ConfigModel::global(cx).read(cx).is_locked(key);
        let danger = cx.theme().danger;
        h_flex()
            .gap_2()
            .items_center()
            .when(!path.is_empty(), |row| {
                row.child(
                    div()
                        .max_w(rems(13.75))
                        .truncate()
                        .text_xs()
                        .text_color(style::white(PATH, cx))
                        .child(SharedString::from(path)),
                )
                .child(
                    Button::new(SharedString::from(format!("{key}-remove")))
                        .custom(soft(cx).foreground(danger))
                        .small()
                        .icon(icons::icon(icons::TRASH))
                        .label(t("remove"))
                        .disabled(locked)
                        .on_click(move |_, _, cx| {
                            edit(cx, |cfg| set_path(cfg, key, String::new()));
                        }),
                )
            })
            .child(
                Button::new(SharedString::from(format!("{key}-choose")))
                    .custom(soft(cx))
                    .small()
                    .icon(icons::icon(icons::FOLDER))
                    .label(choose)
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_file(key, cx))),
            )
            .into_any_element()
    }

    fn pick_file(&mut self, key: &'static str, cx: &mut Context<Self>) {
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
                let value = path.to_string_lossy().to_string();
                this.update(cx, |_, cx| {
                    edit(cx, |cfg| set_path(cfg, key, value));
                })
                .ok();
            }
        })
        .detach();
    }

    fn general(&self, cfg: &AppConfig, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            self.row(
                t("auto_check_updates"),
                "auto_check_updates",
                self.toggle(
                    "auto_check_updates",
                    cfg.auto_check_updates,
                    |cfg, value| cfg.auto_check_updates = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
            self.row(
                t("minimize_to_tray"),
                "minimize_to_tray",
                self.toggle(
                    "minimize_to_tray",
                    cfg.minimize_to_tray,
                    |cfg, value| cfg.minimize_to_tray = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
            self.row(
                t("back_behavior"),
                "back_behavior",
                self.choice(
                    "back_behavior",
                    &cfg.back_behavior,
                    vec![
                        (BackBehavior::RewindThenPrev, t("back_behavior_rewind")),
                        (BackBehavior::AlwaysPrev, t("back_behavior_always_prev")),
                    ],
                    |cfg, value| cfg.back_behavior = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        ]
    }

    fn appearance(&self, cfg: &AppConfig, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = vec![
            self.row(t("language"), "language", self.language(cfg, cx), cx)
                .into_any_element(),
            self.row(t("appearance"), "theme", self.theme(cfg, cx), cx)
                .into_any_element(),
            self.row(
                t("cover_art_background"),
                "cover_art_background",
                self.toggle(
                    "cover_art_background",
                    cfg.cover_art_background,
                    |cfg, value| cfg.cover_art_background = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
            self.row(
                t("custom_background"),
                "custom_background_path",
                self.path_row(
                    "custom_background_path",
                    cfg.custom_background_path.clone(),
                    t("choose_image"),
                    cx,
                ),
                cx,
            )
            .into_any_element(),
            self.row(
                t("custom_font"),
                "custom_font_path",
                self.path_row(
                    "custom_font_path",
                    cfg.custom_font_path.clone(),
                    t("choose_font"),
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        ];

        if cfg.cover_art_background || !cfg.custom_background_path.is_empty() {
            rows.push(
                self.row(
                    t("cover_art_darkening"),
                    "cover_art_darkening",
                    self.slider_row(
                        "cover_art_darkening",
                        &self.darkening,
                        SharedString::from(format!("{}%", cfg.cover_art_darkening)),
                        cx,
                    ),
                    cx,
                )
                .into_any_element(),
            );
            rows.push(
                self.row(
                    t("cover_art_blur"),
                    "cover_art_blur",
                    self.slider_row(
                        "cover_art_blur",
                        &self.blur,
                        SharedString::from(format!("{}px", cfg.cover_art_blur)),
                        cx,
                    ),
                    cx,
                )
                .into_any_element(),
            );
        }

        rows.push(
            self.row(
                t("lyrics_depth_blur"),
                "lyrics_depth_blur",
                self.toggle(
                    "lyrics_depth_blur",
                    cfg.lyrics_depth_blur,
                    |cfg, value| cfg.lyrics_depth_blur = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        if cfg.lyrics_depth_blur {
            rows.push(
                self.row(
                    t("lyrics_depth_blur_strength"),
                    "lyrics_depth_blur_strength",
                    self.slider_row(
                        "lyrics_depth_blur_strength",
                        &self.lyrics_blur,
                        SharedString::from(format!("{}%", cfg.lyrics_depth_blur_strength)),
                        cx,
                    ),
                    cx,
                )
                .into_any_element(),
            );
        }

        rows.push(
            self.row(
                t("reduce_animations"),
                "reduce_animations",
                self.toggle(
                    "reduce_animations",
                    cfg.reduce_animations,
                    |cfg, value| cfg.reduce_animations = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            self.row(
                t("fullscreen_use_player_bar"),
                "fullscreen_use_player_bar",
                self.toggle(
                    "fullscreen_use_player_bar",
                    cfg.fullscreen_use_player_bar,
                    |cfg, value| cfg.fullscreen_use_player_bar = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            self.row(
                t("show_source_toggle"),
                "show_source_toggle",
                self.toggle(
                    "show_source_toggle",
                    cfg.show_source_toggle,
                    |cfg, value| cfg.show_source_toggle = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            self.row(
                t("show_row_images"),
                "show_row_images",
                self.toggle(
                    "show_row_images",
                    cfg.show_row_images,
                    |cfg, value| cfg.show_row_images = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            self.row(
                t("player_bar_position"),
                "player_bar_position",
                self.choice(
                    "player_bar_position",
                    &cfg.player_bar_position,
                    vec![
                        (PlayerBarPosition::Bottom, t("position_bottom")),
                        (PlayerBarPosition::Top, t("position_top")),
                    ],
                    |cfg, value| cfg.player_bar_position = value,
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );
        // TODO(parity): titlebar_mode, ui_style (no Vaxry layout), settings_layout and the live theme.
        rows
    }

    fn language(&self, cfg: &AppConfig, cx: &App) -> AnyElement {
        let options = i18n::available_languages()
            .iter()
            .map(|(code, name)| (code.to_string(), SharedString::from(*name)))
            .collect();
        self.choice(
            "language",
            &cfg.language,
            options,
            |cfg, value| cfg.language = value,
            cx,
        )
    }

    fn theme(&self, cfg: &AppConfig, cx: &App) -> AnyElement {
        let mut options: Vec<(String, SharedString)> = theme::BUILT_IN
            .iter()
            .map(|palette| (palette.name.to_string(), theme_label(palette.name)))
            .collect();
        let mut custom: Vec<(String, SharedString)> = cfg
            .custom_themes
            .iter()
            .map(|(id, custom)| (id.clone(), SharedString::from(custom.name.clone())))
            .collect();
        custom.sort_by(|left, right| left.1.cmp(&right.1));
        options.extend(custom);
        self.choice(
            "theme",
            &cfg.theme,
            options,
            |cfg, value| cfg.theme = value,
            cx,
        )
    }
}

fn set_path(cfg: &mut AppConfig, key: &str, value: String) {
    match key {
        "custom_background_path" => cfg.custom_background_path = value,
        "custom_font_path" => cfg.custom_font_path = value,
        other => tracing::warn!(key = other, "no path setting by that name"),
    }
}

impl AppearanceSettings {
    /// Which rail entry is showing; `None` stacks every section.
    pub fn show(&mut self, section: Option<SharedString>, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn shows(&self, section: &str) -> bool {
        self.section.as_deref().is_none_or(|shown| shown == section)
    }
}

impl Render for AppearanceSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = ConfigModel::global(cx).read(cx).config().clone();
        v_flex()
            .id("appearance-settings")
            .size_full()
            .gap_4()
            .pb_4()
            .pr_2()
            .overflow_y_scroll()
            .when(self.shows("general"), |panel| {
                panel.child(section(t("general")).children(self.general(&cfg, cx)))
            })
            .when(self.shows("appearance"), |panel| {
                panel.child(section(t("appearance")).children(self.appearance(&cfg, cx)))
            })
            .when(self.shows("appearance"), |panel| {
                panel.child(
                    section(t("theme_editor"))
                        .child(div().px_5().py_4().child(self.editor.clone())),
                )
            })
    }
}
