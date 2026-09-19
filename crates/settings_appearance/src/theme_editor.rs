//! The custom theme editor: the saved list, thirteen color rows, a preview card, save and delete.

use std::collections::HashMap;

use config::CustomTheme;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Colorize as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled, Subscription, Window, div, px,
};
use store::ConfigModel;
use widgets::format::t;
use widgets::icons;

/// The thirteen palette tokens, in the order the editor lists them.
pub const VAR_KEYS: &[&str] = &[
    "bg",
    "raised",
    "surface",
    "text",
    "text-muted",
    "accent",
    "accent-soft",
    "accent-alt",
    "accent-deep",
    "highlight",
    "highlight-dark",
    "progress",
    "danger",
];

const DEFAULT_VARS: &[(&str, &str)] = &[
    ("bg", "#0f0f17"),
    ("raised", "#1a1a2a"),
    ("surface", "#282838"),
    ("text", "#e2e2f0"),
    ("text-muted", "#7878a0"),
    ("accent", "#5f8aff"),
    ("accent-soft", "#8faeff"),
    ("accent-alt", "#3a5fd9"),
    ("accent-deep", "#0a0a1a"),
    ("highlight", "#c77dff"),
    ("highlight-dark", "#9d4edd"),
    ("progress", "#5f8aff"),
    ("danger", "#ff6b6b"),
];

fn default_vars() -> HashMap<String, String> {
    DEFAULT_VARS
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// Keeps only hex digits, at most six, and the leading `#` if the user typed one.
fn sanitize_hex(value: &str) -> String {
    let trimmed = value.trim();
    let digits: String = trimmed
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(6)
        .collect();
    if trimmed.starts_with('#') {
        format!("#{digits}")
    } else {
        digits
    }
}

/// `#abc` and `abcdef` both normalize to `#aabbcc` / `#abcdef`; anything else is not a color yet.
fn normalize_hex(value: &str) -> Option<String> {
    let sanitized = sanitize_hex(value);
    let hex = sanitized.strip_prefix('#').unwrap_or(&sanitized);
    match hex.len() {
        3 => Some(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        6 => Some(format!("#{hex}")),
        _ => None,
    }
    .map(|hex| hex.to_lowercase())
}

fn parse_color(value: &str) -> Option<Hsla> {
    normalize_hex(value).and_then(|hex| Hsla::parse_hex(&hex).ok())
}

/// `custom-<slug>`, with `-n` appended until the id is free.
fn theme_id(name: &str, taken: &HashMap<String, CustomTheme>) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let base = format!("custom-{}", slug.trim_matches('-'));
    if !taken.contains_key(&base) {
        return base;
    }
    (1u32..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.contains_key(candidate))
        .unwrap_or(base)
}

struct ColorRow {
    key: &'static str,
    picker: Entity<ColorPickerState>,
    hex: Entity<InputState>,
}

pub struct ThemeEditor {
    selected: Option<String>,
    name: Entity<InputState>,
    rows: Vec<ColorRow>,
    vars: HashMap<String, String>,
    _subscriptions: Vec<Subscription>,
}

impl ThemeEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let vars = default_vars();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(t("my_custom_theme")));
        let mut subscriptions = vec![cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify())];
        let mut rows = Vec::new();
        for key in VAR_KEYS {
            let value = vars.get(*key).cloned().unwrap_or_default();
            let color = parse_color(&value);
            let picker = cx.new(|cx| {
                let mut state = ColorPickerState::new(window, cx);
                if let Some(color) = color {
                    state.set_value(color, window, cx);
                }
                state
            });
            let hex = cx.new(|cx| InputState::new(window, cx).default_value(value));
            subscriptions.push(cx.subscribe_in(
                &picker,
                window,
                move |this, _, event: &ColorPickerEvent, window, cx| {
                    let ColorPickerEvent::Change(Some(color)) = event else {
                        return;
                    };
                    this.set_var(key, color.to_hex().to_lowercase(), false, window, cx);
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &hex,
                window,
                move |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let typed = input.read(cx).value().to_string();
                        if let Some(hex) = normalize_hex(&typed) {
                            this.set_var(key, hex, true, window, cx);
                        }
                    }
                    InputEvent::Blur => this.revert_hex(key, window, cx),
                    _ => {}
                },
            ));
            rows.push(ColorRow { key, picker, hex });
        }
        Self {
            selected: None,
            name,
            rows,
            vars,
            _subscriptions: subscriptions,
        }
    }

    fn row(&self, key: &str) -> Option<&ColorRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    /// `from_hex` says the hex field is the one that changed, so it is left alone while typing.
    fn set_var(
        &mut self,
        key: &str,
        value: String,
        from_hex: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vars.get(key).is_some_and(|held| held == &value) {
            return;
        }
        self.vars.insert(key.to_string(), value.clone());
        if let Some(row) = self.row(key) {
            let (picker, hex) = (row.picker.clone(), row.hex.clone());
            if let Some(color) = parse_color(&value) {
                picker.update(cx, |state, cx| state.set_value(color, window, cx));
            }
            if !from_hex {
                hex.update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        cx.notify();
    }

    fn revert_hex(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.vars.get(key).cloned().unwrap_or_default();
        if let Some(row) = self.row(key) {
            let hex = row.hex.clone();
            hex.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }

    fn load(&mut self, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let (name, vars) = match id.as_ref() {
            Some(id) => ConfigModel::global(cx)
                .read(cx)
                .config()
                .custom_themes
                .get(id)
                .map(|theme| (theme.name.clone(), theme.vars.clone()))
                .unwrap_or_else(|| (String::new(), default_vars())),
            None => (String::new(), default_vars()),
        };
        self.selected = id;
        let input = self.name.clone();
        input.update(cx, |state, cx| state.set_value(name, window, cx));
        for key in VAR_KEYS {
            let value = vars
                .get(*key)
                .cloned()
                .or_else(|| default_vars().get(*key).cloned())
                .unwrap_or_default();
            self.vars.remove(*key);
            self.set_var(key, value, false, window, cx);
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        let config = ConfigModel::global(cx);
        // Saving before the daemon's config arrived would be dropped, taking the edits with it.
        if name.is_empty() || !config.read(cx).is_loaded() {
            return;
        }
        let vars = self.vars.clone();
        let id = self
            .selected
            .clone()
            .unwrap_or_else(|| theme_id(&name, &config.read(cx).config().custom_themes));
        let saved = id.clone();
        config.update(cx, |model, cx| {
            model.update(cx, |cfg| {
                cfg.custom_themes.insert(saved, CustomTheme { name, vars });
            })
        });
        self.load(Some(id), window, cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        ConfigModel::global(cx).update(cx, |model, cx| {
            model.update(cx, |cfg| {
                cfg.custom_themes.remove(&id);
                if cfg.theme == id {
                    cfg.theme = "default".to_string();
                }
            })
        });
        self.load(None, window, cx);
    }

    fn saved_themes(&self, cx: &Context<Self>) -> Vec<(String, String)> {
        let mut themes: Vec<(String, String)> = ConfigModel::global(cx)
            .read(cx)
            .config()
            .custom_themes
            .iter()
            .map(|(id, theme)| (id.clone(), theme.name.clone()))
            .collect();
        themes.sort_by(|left, right| left.1.cmp(&right.1));
        themes
    }

    fn color(&self, key: &str, fallback: Hsla) -> Hsla {
        self.vars
            .get(key)
            .and_then(|value| parse_color(value))
            .unwrap_or(fallback)
    }

    fn preview(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = self.color("bg", theme.background);
        let raised = self.color("raised", theme.muted);
        let surface = self.color("surface", theme.border);
        let text = self.color("text", theme.foreground);
        let muted = self.color("text-muted", theme.muted_foreground);
        let progress = self.color("progress", theme.primary);
        let chip = |key: &'static str, label: SharedString, color: Hsla| {
            div()
                .px_2()
                .py_0p5()
                .id(key)
                .rounded_full()
                .bg(raised)
                .text_xs()
                .text_color(color)
                .child(label)
        };
        v_flex()
            .gap_2()
            .p_4()
            .rounded(theme.radius)
            .bg(bg)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().text_sm().text_color(text).child(t("track_title")))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from("3:42")),
                    ),
            )
            .child(
                div().w_full().h(px(4.)).rounded_full().bg(surface).child(
                    div()
                        .w(gpui_kit::relative(0.66))
                        .h(px(4.))
                        .rounded_full()
                        .bg(progress),
                ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(chip(
                        "preview-accent",
                        t("accent"),
                        self.color("accent", theme.primary),
                    ))
                    .child(chip(
                        "preview-highlight",
                        t("highlight"),
                        self.color("highlight", theme.primary),
                    ))
                    .child(chip(
                        "preview-danger",
                        t("danger"),
                        self.color("danger", theme.danger),
                    )),
            )
    }

    fn color_row(&self, row: &ColorRow, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_3()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t(row.key)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(ColorPicker::new(&row.picker).small())
                    .child(div().w(px(104.)).child(Input::new(&row.hex).small())),
            )
    }
}

impl Render for ThemeEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let saved = self.saved_themes(cx);
        let selected = self.selected.clone();
        h_flex()
            .gap_6()
            .items_start()
            .child(
                v_flex()
                    .w(px(200.))
                    .flex_none()
                    .gap_1()
                    .child(
                        Button::new("new-theme")
                            .outline()
                            .w_full()
                            .justify_start()
                            .icon(icons::icon(icons::PLUS))
                            .label(t("new_theme"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.load(None, window, cx)),
                            ),
                    )
                    .children(saved.into_iter().map(|(id, name)| {
                        let chosen = selected.as_deref() == Some(id.as_str());
                        let target = id.clone();
                        Button::new(SharedString::from(format!("theme-{id}")))
                            .ghost()
                            .w_full()
                            .justify_start()
                            .label(SharedString::from(name))
                            .selected(chosen)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.load(Some(target.clone()), window, cx)
                            }))
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("theme_name")),
                            )
                            .child(Input::new(&self.name)),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("colors")),
                            )
                            .children(self.rows.iter().map(|row| self.color_row(row, cx))),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("preview")),
                            )
                            .child(self.preview(cx)),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .child(
                                Button::new("save-theme")
                                    .primary()
                                    .label(t("save_theme"))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.save(window, cx)),
                                    ),
                            )
                            .when(self.selected.is_some(), |actions| {
                                actions.child(
                                    Button::new("delete-theme")
                                        .danger()
                                        .icon(icons::icon(icons::TRASH))
                                        .label(t("delete"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.delete(window, cx)
                                        })),
                                )
                            }),
                    ),
            )
    }
}
