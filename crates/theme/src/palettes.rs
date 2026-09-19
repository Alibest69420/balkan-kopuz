//! The built-in Kopuz palettes; `kopuz/assets/themes.css` is the source these were taken from.

use crate::Palette;

/// Every theme the settings picker offers, in the order it offers them.
pub const BUILT_IN: &[Palette] = &[
    Palette {
        name: "default",
        dark: true,
        bg: "#0f0f17",
        raised: "#1a1a2a",
        surface: "#282838",
        text: "#e2e2f0",
        text_muted: "#7878a0",
        accent: "#5f8aff",
        accent_soft: "#8faeff",
        accent_alt: "#3a5fd9",
        accent_deep: "#0a0a1a",
        highlight: "#c77dff",
        highlight_dark: "#9d4edd",
        progress: "#5f8aff",
        danger: "#ff6b6b",
    },
    Palette {
        name: "amoled",
        dark: true,
        bg: "#000000",
        raised: "#0a0a0a",
        surface: "#1a1a1a",
        text: "#ffffff",
        text_muted: "#808080",
        accent: "#5f8aff",
        accent_soft: "#8faeff",
        accent_alt: "#3a5fd9",
        accent_deep: "#000000",
        highlight: "#c77dff",
        highlight_dark: "#9d4edd",
        progress: "#5f8aff",
        danger: "#ff6b6b",
    },
    Palette {
        name: "default-light",
        dark: false,
        bg: "#ffffff",
        raised: "#f4f4f8",
        surface: "#e4e4ec",
        text: "#16161e",
        text_muted: "#6b6b80",
        accent: "#3a5fd9",
        accent_soft: "#5f8aff",
        accent_alt: "#2c48a8",
        accent_deep: "#dfe4f7",
        highlight: "#9d4edd",
        highlight_dark: "#7b2fb5",
        progress: "#3a5fd9",
        danger: "#d64545",
    },
];

/// The palette named by `AppConfig::theme`, if it is a built-in one.
pub fn palette(name: &str) -> Option<&'static Palette> {
    BUILT_IN.iter().find(|palette| palette.name == name)
}
