//! Kopuz themes over GPUI Component's: the 13 palette tokens a Kopuz theme defines, projected onto the component theme's roles.

mod palettes;

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Window};

pub use palettes::{BUILT_IN, palette};

/// A Kopuz theme: the same thirteen colors the Dioxus themes and the theme editor use.
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

/// Make `palette` the active theme, including the scrollbars the Base layer paints.
pub fn apply(palette: &Palette, window: Option<&mut Window>, cx: &mut App) {
    let mode = if palette.dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    let config: ThemeConfig = match serde_json::from_value(config_value(palette, mode)) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, theme = palette.name, "could not build the theme");
            return;
        }
    };
    let config = Rc::new(config);
    let theme = Theme::global_mut(cx);
    if palette.dark {
        theme.dark_theme = config;
    } else {
        theme.light_theme = config;
    }
    Theme::change(mode, window, cx);
}

/// Apply a theme by its Kopuz name (`AppConfig::theme`), falling back to the default.
pub fn apply_named(name: &str, window: Option<&mut Window>, cx: &mut App) {
    let palette = palette(name).unwrap_or_else(|| palette("default").expect("default theme"));
    apply(palette, window, cx);
}

fn config_value(palette: &Palette, mode: ThemeMode) -> serde_json::Value {
    let mut colors = serde_json::Map::new();
    let mut set = |key: &str, value: &str| {
        colors.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    };
    set("background", palette.bg);
    set("foreground", palette.text);
    set("border", palette.surface);
    set("muted.background", palette.raised);
    set("muted.foreground", palette.text_muted);
    set("secondary.background", palette.raised);
    set("secondary.foreground", palette.text);
    set("secondary.hover.background", palette.surface);
    set("secondary.active.background", palette.surface);
    set("popover.background", palette.raised);
    set("popover.foreground", palette.text);
    set("accent.background", palette.surface);
    set("accent.foreground", palette.text);
    set("primary.background", palette.accent);
    set("primary.foreground", palette.bg);
    set("primary.hover.background", palette.accent_soft);
    set("primary.active.background", palette.accent_alt);
    set("ring", palette.accent);
    set("link", palette.accent);
    set("link.hover", palette.accent_soft);
    set("link.active", palette.accent_alt);
    set("progress.bar.background", palette.progress);
    set("slider.background", palette.progress);
    set("slider.thumb.background", palette.text);
    set("switch.background", palette.surface);
    set("switch.thumb.background", palette.text);
    set("selection.background", palette.accent_deep);
    set("danger.background", palette.danger);
    set("danger.foreground", palette.text);
    set("sidebar.background", palette.raised);
    set("sidebar.foreground", palette.text);
    set("sidebar.border", palette.surface);
    set("sidebar.accent.background", palette.surface);
    set("sidebar.accent.foreground", palette.text);
    set("sidebar.primary.background", palette.accent);
    set("sidebar.primary.foreground", palette.bg);
    set("title_bar.background", palette.bg);
    set("title_bar.border", palette.surface);
    set("status_bar.background", palette.raised);
    set("status_bar.border", palette.surface);
    set("list.background", palette.bg);
    set("list.hover.background", palette.raised);
    set("list.active.background", palette.surface);
    set("list.active.border", palette.accent);
    set("table.background", palette.bg);
    set("table.hover.background", palette.raised);
    set("table.active.background", palette.surface);
    set("input.border", palette.surface);
    set("tab.background", palette.raised);
    set("tab.foreground", palette.text_muted);
    set("tab.active.background", palette.surface);
    set("tab.active.foreground", palette.text);
    set("tab_bar.background", palette.raised);
    set("scrollbar.background", palette.bg);
    set("scrollbar.thumb.background", palette.surface);
    set("scrollbar.thumb.hover.background", palette.text_muted);
    set("skeleton.background", palette.surface);
    set("drag.border", palette.accent);
    set("drop_target.background", palette.accent_deep);
    set("caret", palette.accent);
    set("overlay", palette.accent_deep);
    set("button.background", palette.raised);
    set("button.foreground", palette.text);
    set("button.hover.background", palette.surface);
    set("button.active.background", palette.surface);
    set("button.primary.background", palette.accent);
    set("button.primary.foreground", palette.bg);
    set("button.primary.hover.background", palette.accent_soft);
    set("button.primary.active.background", palette.accent_alt);
    set("button.danger.background", palette.danger);
    set("button.danger.foreground", palette.text);
    set("info.background", palette.highlight);
    set("info.foreground", palette.text);
    set("success.background", palette.progress);
    set("success.foreground", palette.bg);
    let mut root = serde_json::Map::new();
    root.insert(
        "name".into(),
        serde_json::Value::String(palette.name.to_string()),
    );
    let mode = if mode.is_dark() { "dark" } else { "light" };
    root.insert("mode".into(), serde_json::Value::String(mode.to_string()));
    root.insert("colors".into(), serde_json::Value::Object(colors));
    serde_json::Value::Object(root)
}
