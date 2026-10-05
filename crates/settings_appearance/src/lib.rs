//! Settings → General and Appearance, plus the custom theme editor embedded under them.

mod theme_editor;

use components::format::t;
use components::settings::{
    Choice, SettingRow, SliderRow, on_slider_change, section, set_slider, slider_state, toggle_row,
};
use components::{icons, style};
use config::{AppConfig, BackBehavior, PlayerBarPosition};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, SharedString, Styled, Subscription, Window, div,
    rems,
};
use store::ConfigModel;
use theme_editor::ThemeEditor;

/// `text-white/50`, the path a file setting shows.
const PATH: f32 = 0.50;

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

/// What a whole-percent slider reads as.
fn percent(value: f32) -> SharedString {
    SharedString::from(format!("{}%", value as i32))
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
        let darkening = cx.new(|_| slider_state(0., 95., 5., cfg.cover_art_darkening as f32));
        let blur = cx.new(|_| slider_state(0., 100., 5., cfg.cover_art_blur as f32));
        let lyrics_blur =
            cx.new(|_| slider_state(10., 200., 10., cfg.lyrics_depth_blur_strength as f32));

        let subscriptions = vec![
            cx.observe_in(&config, window, |this, _, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
            on_slider_change(
                &darkening,
                cx,
                ConfigModel::writer("cover_art_darkening", |cfg, value: f32| {
                    cfg.cover_art_darkening = value.clamp(0., 95.) as u8
                }),
            ),
            on_slider_change(
                &blur,
                cx,
                ConfigModel::writer("cover_art_blur", |cfg, value: f32| {
                    cfg.cover_art_blur = value.clamp(0., 100.) as u8
                }),
            ),
            on_slider_change(
                &lyrics_blur,
                cx,
                ConfigModel::writer("lyrics_depth_blur_strength", |cfg, value: f32| {
                    cfg.lyrics_depth_blur_strength = value.clamp(10., 200.) as u8
                }),
            ),
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
                        .custom(style::soft_button(danger, cx))
                        .small()
                        .icon(icons::icon(icons::TRASH))
                        .label(t("remove"))
                        .disabled(locked)
                        .on_click(move |_, _, cx| {
                            ConfigModel::edit_key(key, cx, |cfg| set_path(cfg, key, String::new()));
                        }),
                )
            })
            .child(
                Button::new(SharedString::from(format!("{key}-choose")))
                    .custom(style::soft_button(cx.theme().foreground, cx))
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
                    ConfigModel::edit_key(key, cx, |cfg| set_path(cfg, key, value));
                })
                .ok();
            }
        })
        .detach();
    }

    fn general(&self, cfg: &AppConfig, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        push(
            &mut rows,
            toggle_row(
                "auto_check_updates",
                t("auto_check_updates"),
                cfg.auto_check_updates,
                ConfigModel::writer("auto_check_updates", |cfg, on| cfg.auto_check_updates = on),
            )
            .bind("auto_check_updates", cx)
            .description(t("auto_check_updates_description")),
            cx,
        );
        push(
            &mut rows,
            toggle_row(
                "minimize_to_tray",
                t("minimize_to_tray"),
                cfg.minimize_to_tray,
                ConfigModel::writer("minimize_to_tray", |cfg, on| cfg.minimize_to_tray = on),
            )
            .bind("minimize_to_tray", cx)
            .description(t("minimize_to_tray_description")),
            cx,
        );
        push(
            &mut rows,
            SettingRow::new(t("back_behavior"))
                .bind("back_behavior", cx)
                .description(t("back_behavior_description"))
                .setting(
                    Choice::new(
                        "back_behavior",
                        cfg.back_behavior,
                        vec![
                            (BackBehavior::RewindThenPrev, t("back_behavior_rewind")),
                            (BackBehavior::AlwaysPrev, t("back_behavior_always_prev")),
                        ],
                    )
                    .on_change(ConfigModel::writer("back_behavior", |cfg, value| {
                        cfg.back_behavior = value
                    })),
                ),
            cx,
        );
        rows
    }

    fn appearance(&self, cfg: &AppConfig, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        push(
            &mut rows,
            SettingRow::new(t("language"))
                .bind("language", cx)
                .setting(self.language(cfg)),
            cx,
        );
        push(
            &mut rows,
            SettingRow::new(t("appearance"))
                .bind("theme", cx)
                .description(t("theme_description"))
                .setting(self.theme(cfg)),
            cx,
        );
        push(
            &mut rows,
            toggle_row(
                "cover_art_background",
                t("cover_art_background"),
                cfg.cover_art_background,
                ConfigModel::writer("cover_art_background", |cfg, on| {
                    cfg.cover_art_background = on
                }),
            )
            .bind("cover_art_background", cx)
            .description(t("cover_art_background_description")),
            cx,
        );
        push(
            &mut rows,
            SettingRow::new(t("custom_background"))
                .bind("custom_background_path", cx)
                .description(t("custom_background_description"))
                .control(self.path_row(
                    "custom_background_path",
                    cfg.custom_background_path.clone(),
                    t("choose_image"),
                    cx,
                )),
            cx,
        );
        push(
            &mut rows,
            SettingRow::new(t("custom_font"))
                .bind("custom_font_path", cx)
                .description(t("custom_font_description"))
                .control(self.path_row(
                    "custom_font_path",
                    cfg.custom_font_path.clone(),
                    t("choose_font"),
                    cx,
                )),
            cx,
        );

        if cfg.cover_art_background || !cfg.custom_background_path.is_empty() {
            push(
                &mut rows,
                SettingRow::new(t("cover_art_darkening"))
                    .bind("cover_art_darkening", cx)
                    .setting(SliderRow::new(&self.darkening).readout(percent)),
                cx,
            );
            push(
                &mut rows,
                SettingRow::new(t("cover_art_blur"))
                    .bind("cover_art_blur", cx)
                    .setting(
                        SliderRow::new(&self.blur)
                            .readout(|value| SharedString::from(format!("{}px", value as i32))),
                    ),
                cx,
            );
        }

        push(
            &mut rows,
            toggle_row(
                "lyrics_depth_blur",
                t("lyrics_depth_blur"),
                cfg.lyrics_depth_blur,
                ConfigModel::writer("lyrics_depth_blur", |cfg, on| cfg.lyrics_depth_blur = on),
            )
            .bind("lyrics_depth_blur", cx),
            cx,
        );
        if cfg.lyrics_depth_blur {
            push(
                &mut rows,
                SettingRow::new(t("lyrics_depth_blur_strength"))
                    .bind("lyrics_depth_blur_strength", cx)
                    .setting(SliderRow::new(&self.lyrics_blur).readout(percent)),
                cx,
            );
        }

        push(
            &mut rows,
            toggle_row(
                "reduce_animations",
                t("reduce_animations"),
                cfg.reduce_animations,
                ConfigModel::writer("reduce_animations", |cfg, on| cfg.reduce_animations = on),
            )
            .bind("reduce_animations", cx)
            .description(t("reduce_animations_description")),
            cx,
        );
        push(
            &mut rows,
            toggle_row(
                "fullscreen_use_player_bar",
                t("fullscreen_use_player_bar"),
                cfg.fullscreen_use_player_bar,
                ConfigModel::writer("fullscreen_use_player_bar", |cfg, on| {
                    cfg.fullscreen_use_player_bar = on
                }),
            )
            .bind("fullscreen_use_player_bar", cx),
            cx,
        );
        push(
            &mut rows,
            toggle_row(
                "show_source_toggle",
                t("show_source_toggle"),
                cfg.show_source_toggle,
                ConfigModel::writer("show_source_toggle", |cfg, on| cfg.show_source_toggle = on),
            )
            .bind("show_source_toggle", cx)
            .description(t("show_source_toggle_description")),
            cx,
        );
        push(
            &mut rows,
            toggle_row(
                "show_row_images",
                t("show_row_images"),
                cfg.show_row_images,
                ConfigModel::writer("show_row_images", |cfg, on| cfg.show_row_images = on),
            )
            .bind("show_row_images", cx)
            .description(t("show_row_images_description")),
            cx,
        );
        push(
            &mut rows,
            SettingRow::new(t("player_bar_position"))
                .bind("player_bar_position", cx)
                .setting(
                    Choice::new(
                        "player_bar_position",
                        cfg.player_bar_position,
                        vec![
                            (PlayerBarPosition::Bottom, t("position_bottom")),
                            (PlayerBarPosition::Top, t("position_top")),
                        ],
                    )
                    .on_change(ConfigModel::writer("player_bar_position", |cfg, value| {
                        cfg.player_bar_position = value
                    })),
                ),
            cx,
        );
        // TODO(parity): titlebar_mode, ui_style (no Vaxry layout), settings_layout and the live theme.
        rows
    }

    fn language(&self, cfg: &AppConfig) -> Choice<String> {
        let options = i18n::available_languages()
            .iter()
            .map(|(code, name)| (code.to_string(), SharedString::from(*name)))
            .collect();
        Choice::new("language", cfg.language.clone(), options)
            .on_change(ConfigModel::writer("language", |cfg, value| {
                cfg.language = value
            }))
    }

    fn theme(&self, cfg: &AppConfig) -> Choice<String> {
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
        Choice::new("theme", cfg.theme.clone(), options)
            .on_change(ConfigModel::writer("theme", |cfg, value| cfg.theme = value))
    }
}

/// Keep a row unless the settings search is filtering it out.
fn push(rows: &mut Vec<AnyElement>, row: SettingRow, cx: &App) {
    if let Some(row) = row.shown(cx) {
        rows.push(row.into_any_element());
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
        // The settings page owns the scroll; this panel is content inside it.
        v_flex()
            .id("appearance-settings")
            .w_full()
            .gap_4()
            .pb_4()
            .pr_2()
            .when(self.shows("general"), |panel| {
                panel.child(section(t("general")).children(self.general(&cfg, cx)))
            })
            .when(self.shows("appearance"), |panel| {
                panel.child(section(t("appearance")).children(self.appearance(&cfg, cx)))
            })
            .when(self.shows("appearance"), |panel| {
                panel.child(
                    section(t("theme_editor"))
                        .opaque()
                        .child(div().px_5().py_4().child(self.editor.clone())),
                )
            })
    }
}
