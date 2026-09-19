//! Kopuz themes over GPUI Component's: the 13 palette tokens a Kopuz theme defines, projected onto the component theme's roles.

mod palettes;

use std::collections::HashMap;
use std::rc::Rc;

use config::AppConfig;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Window};

pub use palettes::{BUILT_IN, palette};

/// A built-in Kopuz theme: the same thirteen colors the themes and the editor use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    pub dark: bool,
    pub bg: &'static str,
    pub raised: &'static str,
    pub surface: &'static str,
    pub text: &'static str,
    pub text_muted: &'static str,
    pub accent: &'static str,
    pub accent_soft: &'static str,
    pub accent_alt: &'static str,
    pub accent_deep: &'static str,
    pub highlight: &'static str,
    pub highlight_dark: &'static str,
    pub progress: &'static str,
    pub danger: &'static str,
}

/// The same thirteen colors, owned, so a user's custom theme can carry them too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colors {
    pub dark: bool,
    pub bg: String,
    pub raised: String,
    pub surface: String,
    pub text: String,
    pub text_muted: String,
    pub accent: String,
    pub accent_soft: String,
    pub accent_alt: String,
    pub accent_deep: String,
    pub highlight: String,
    pub highlight_dark: String,
    pub progress: String,
    pub danger: String,
}

impl From<&Palette> for Colors {
    fn from(palette: &Palette) -> Self {
        Self {
            dark: palette.dark,
            bg: palette.bg.into(),
            raised: palette.raised.into(),
            surface: palette.surface.into(),
            text: palette.text.into(),
            text_muted: palette.text_muted.into(),
            accent: palette.accent.into(),
            accent_soft: palette.accent_soft.into(),
            accent_alt: palette.accent_alt.into(),
            accent_deep: palette.accent_deep.into(),
            highlight: palette.highlight.into(),
            highlight_dark: palette.highlight_dark.into(),
            progress: palette.progress.into(),
            danger: palette.danger.into(),
        }
    }
}

impl Colors {
    /// The editor's variable names, as `AppConfig::custom_themes` stores them.
    pub fn from_vars(vars: &HashMap<String, String>) -> Option<Self> {
        let get = |key: &str| vars.get(key).cloned();
        let bg = get("bg")?;
        Some(Self {
            dark: is_dark(&bg),
            bg,
            raised: get("raised")?,
            surface: get("surface")?,
            text: get("text")?,
            text_muted: get("text-muted")?,
            accent: get("accent")?,
            accent_soft: get("accent-soft")?,
            accent_alt: get("accent-alt")?,
            accent_deep: get("accent-deep")?,
            highlight: get("highlight")?,
            highlight_dark: get("highlight-dark")?,
            progress: get("progress")?,
            danger: get("danger")?,
        })
    }
}

/// Make these colors the active theme, including the scrollbars the Base layer paints.
pub fn apply(name: &str, colors: &Colors, window: Option<&mut Window>, cx: &mut App) {
    let mode = if colors.dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    let config: ThemeConfig = match serde_json::from_value(config_value(name, colors, mode)) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, theme = name, "could not build the theme");
            return;
        }
    };
    let config = Rc::new(config);
    let theme = Theme::global_mut(cx);
    if colors.dark {
        theme.dark_theme = config;
    } else {
        theme.light_theme = config;
    }
    Theme::change(mode, window, cx);
}

/// Apply a built-in theme by name, falling back to the default.
pub fn apply_named(name: &str, window: Option<&mut Window>, cx: &mut App) {
    let palette = palette(name).unwrap_or_else(|| palette("default").expect("default theme"));
    apply(palette.name, &palette.into(), window, cx);
}

/// Apply whatever `AppConfig::theme` names: a built-in, or one of `custom_themes`.
pub fn apply_config(config: &AppConfig, window: Option<&mut Window>, cx: &mut App) {
    let name = config.theme.as_str();
    if let Some(palette) = palette(name) {
        apply(palette.name, &palette.into(), window, cx);
        return;
    }
    if let Some(colors) = config
        .custom_themes
        .get(name)
        .and_then(|custom| Colors::from_vars(&custom.vars))
    {
        apply(name, &colors, window, cx);
        return;
    }
    // "album-art" and "live" recolor the default from a picture; until that lands they are it.
    apply_named("default", window, cx);
}

/// A theme is dark when its background is; the editor never asks which it is.
fn is_dark(background: &str) -> bool {
    let hex = background.trim_start_matches('#');
    if hex.len() < 6 {
        return true;
    }
    let channel =
        |at: usize| u8::from_str_radix(hex.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0) as f32;
    (0.299 * channel(0) + 0.587 * channel(2) + 0.114 * channel(4)) < 128.
}

fn config_value(name: &str, colors: &Colors, mode: ThemeMode) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut set = |key: &str, value: &str| {
        map.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    };
    set("background", &colors.bg);
    set("foreground", &colors.text);
    set("border", &colors.surface);
    set("muted.background", &colors.raised);
    set("muted.foreground", &colors.text_muted);
    set("secondary.background", &colors.raised);
    set("secondary.foreground", &colors.text);
    set("secondary.hover.background", &colors.surface);
    set("secondary.active.background", &colors.surface);
    set("popover.background", &colors.raised);
    set("popover.foreground", &colors.text);
    set("accent.background", &colors.surface);
    set("accent.foreground", &colors.text);
    set("primary.background", &colors.accent);
    set("primary.foreground", &colors.bg);
    set("primary.hover.background", &colors.accent_soft);
    set("primary.active.background", &colors.accent_alt);
    set("ring", &colors.accent);
    set("link", &colors.accent);
    set("link.hover", &colors.accent_soft);
    set("link.active", &colors.accent_alt);
    set("progress.bar.background", &colors.progress);
    set("slider.background", &colors.progress);
    set("slider.thumb.background", &colors.text);
    set("switch.background", &colors.surface);
    set("switch.thumb.background", &colors.text);
    set("selection.background", &colors.accent_deep);
    set("danger.background", &colors.danger);
    set("danger.foreground", &colors.text);
    set("sidebar.background", &colors.raised);
    set("sidebar.foreground", &colors.text);
    set("sidebar.border", &colors.surface);
    set("sidebar.accent.background", &colors.surface);
    set("sidebar.accent.foreground", &colors.text);
    set("sidebar.primary.background", &colors.accent);
    set("sidebar.primary.foreground", &colors.bg);
    set("title_bar.background", &colors.bg);
    set("title_bar.border", &colors.surface);
    set("status_bar.background", &colors.raised);
    set("status_bar.border", &colors.surface);
    set("list.background", &colors.bg);
    set("list.hover.background", &colors.raised);
    set("list.active.background", &colors.surface);
    set("list.active.border", &colors.accent);
    set("table.background", &colors.bg);
    set("table.hover.background", &colors.raised);
    set("table.active.background", &colors.surface);
    set("input.border", &colors.surface);
    set("tab.background", &colors.raised);
    set("tab.foreground", &colors.text_muted);
    set("tab.active.background", &colors.surface);
    set("tab.active.foreground", &colors.text);
    set("tab_bar.background", &colors.raised);
    set("scrollbar.background", &colors.bg);
    set("scrollbar.thumb.background", &colors.surface);
    set("scrollbar.thumb.hover.background", &colors.text_muted);
    set("skeleton.background", &colors.surface);
    set("drag.border", &colors.accent);
    set("drop_target.background", &colors.accent_deep);
    set("caret", &colors.accent);
    set("overlay", &colors.accent_deep);
    set("button.background", &colors.raised);
    set("button.foreground", &colors.text);
    set("button.hover.background", &colors.surface);
    set("button.active.background", &colors.surface);
    set("button.primary.background", &colors.accent);
    set("button.primary.foreground", &colors.bg);
    set("button.primary.hover.background", &colors.accent_soft);
    set("button.primary.active.background", &colors.accent_alt);
    set("button.danger.background", &colors.danger);
    set("button.danger.foreground", &colors.text);
    set("info.background", &colors.highlight);
    set("info.foreground", &colors.text);
    set("success.background", &colors.progress);
    set("success.foreground", &colors.bg);

    let mut root = serde_json::Map::new();
    root.insert("name".into(), serde_json::Value::String(name.to_string()));
    let mode = if mode.is_dark() { "dark" } else { "light" };
    root.insert("mode".into(), serde_json::Value::String(mode.to_string()));
    root.insert("colors".into(), serde_json::Value::Object(map));
    serde_json::Value::Object(root)
}
