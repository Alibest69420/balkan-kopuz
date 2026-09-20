//! Renders the daemon's own `FieldSpec` forms: source options, integrations, artwork providers, the downloader.

use std::collections::HashMap;

use api::schema::{FieldKind, FieldSpec, FieldValue, Problem};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window, div,
};

use crate::format::{t, text};
use crate::icons;
use crate::motion;
use crate::style;

/// One field was answered; the owner sends it to whichever `set_*_settings` call it belongs to.
#[derive(Debug, Clone)]
pub struct FieldChanged(pub FieldValue);

/// A form over daemon-supplied fields. The daemon owns which fields exist and what they mean.
pub struct SchemaForm {
    specs: Vec<FieldSpec>,
    values: HashMap<String, String>,
    inputs: HashMap<String, Entity<InputState>>,
    problems: Vec<Problem>,
    locked: Vec<String>,
    // Secret keys the daemon already holds a value for, since a secret never comes back to prefill.
    saved: Vec<String>,
    hovered: Option<String>,
    _subscriptions: Vec<Subscription>,
}

/// Whether a choice's current answer is one the daemon listed.
fn is_listed(options: &[api::schema::ChoiceOption], value: &str) -> bool {
    options.iter().any(|option| option.value == value)
}

impl EventEmitter<FieldChanged> for SchemaForm {}

impl SchemaForm {
    pub fn new(specs: Vec<FieldSpec>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut form = Self {
            specs: Vec::new(),
            values: HashMap::new(),
            inputs: HashMap::new(),
            problems: Vec::new(),
            locked: Vec::new(),
            saved: Vec::new(),
            hovered: None,
            _subscriptions: Vec::new(),
        };
        form.set_specs(specs, window, cx);
        form
    }

    /// Replace the fields, keeping nothing: the daemon answered with the current values.
    pub fn set_specs(
        &mut self,
        specs: Vec<FieldSpec>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.values = specs
            .iter()
            .map(|spec| (spec.key.clone(), spec.value.clone().unwrap_or_default()))
            .collect();
        self.inputs.clear();
        self._subscriptions.clear();
        for spec in &specs {
            let free_text = matches!(&spec.kind, FieldKind::Choice { custom: true, .. });
            if !free_text
                && !matches!(
                    spec.kind,
                    FieldKind::Text | FieldKind::Secret | FieldKind::Url | FieldKind::Directory
                )
            {
                continue;
            }
            let key = spec.key.clone();
            let initial = spec.value.clone().unwrap_or_default();
            let placeholder = spec.placeholder.as_ref().map(text).unwrap_or_default();
            let input = cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                // A secret is write-only, so its stored value never comes back to prefill.
                if !matches!(spec.kind, FieldKind::Secret) {
                    state = state.default_value(initial);
                }
                state
            });
            self._subscriptions.push(cx.subscribe(
                &input,
                move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        let value = state.read(cx).value().to_string();
                        this.answer(&key, value, cx);
                    }
                },
            ));
            self.inputs.insert(spec.key.clone(), input);
        }
        self.specs = specs;
        cx.notify();
    }

    pub fn set_problems(&mut self, problems: Vec<Problem>, cx: &mut Context<Self>) {
        self.problems = problems;
        cx.notify();
    }

    /// Secret keys the daemon already holds, so a blank box reads as "kept" rather than "unset".
    pub fn set_saved(&mut self, saved: Vec<String>, cx: &mut Context<Self>) {
        if self.saved == saved {
            return;
        }
        self.saved = saved;
        cx.notify();
    }

    /// Keys a managed settings file pins; their controls render disabled.
    pub fn set_locked(&mut self, locked: Vec<String>, cx: &mut Context<Self>) {
        self.locked = locked;
        cx.notify();
    }

    pub fn values(&self) -> Vec<FieldValue> {
        self.values
            .iter()
            .map(|(key, value)| FieldValue::new(key.clone(), value.clone()))
            .collect()
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    fn answer(&mut self, key: &str, value: String, cx: &mut Context<Self>) {
        if self.values.get(key).is_some_and(|held| held == &value) {
            return;
        }
        self.values.insert(key.to_string(), value.clone());
        cx.notify();
        cx.emit(FieldChanged(FieldValue::new(key.to_string(), value)));
    }

    fn is_locked(&self, spec: &FieldSpec) -> bool {
        spec.config_key
            .as_ref()
            .is_some_and(|key| self.locked.iter().any(|locked| locked == key))
    }

    /// `show_when` hides a field until another field holds the value it names.
    fn is_visible(&self, spec: &FieldSpec) -> bool {
        spec.show_when.as_ref().is_none_or(|condition| {
            self.values
                .get(&condition.key)
                .is_some_and(|value| value == &condition.value)
        })
    }

    fn problem_for(&self, key: &str) -> Option<SharedString> {
        self.problems
            .iter()
            .find(|problem| problem.field.as_deref() == Some(key))
            .map(|problem| text(&problem.label))
    }

    /// Take the answer off the listed options and onto whatever the box is about to hold.
    fn start_custom(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get(key).cloned() {
            input.update(cx, |state, cx| state.set_value("", window, cx));
            input.update(cx, |state, cx| state.focus(window, cx));
        }
        self.answer(key, String::new(), cx);
    }

    /// One option of a radio group: the dot says which, and the whole line is the target.
    fn radio(
        &self,
        key: &str,
        option: &api::schema::ChoiceOption,
        chosen: bool,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let accent = cx.theme().primary;
        let id = SharedString::from(format!("radio-{key}-{}", option.value));
        let dot = div()
            .size_3p5()
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(if chosen {
                accent
            } else {
                style::white(style::BORDER, cx)
            })
            .when(chosen, |ring| {
                ring.child(
                    div()
                        .size_full()
                        .p_0p5()
                        .child(div().size_full().rounded_full().bg(accent)),
                )
            });
        h_flex()
            .id(id)
            .gap_2()
            .items_center()
            .text_sm()
            .text_color(style::white(style::TEXT_STRONG, cx))
            .when(!locked, |row| row.cursor_pointer())
            .child(dot)
            .child(text(&option.label))
            .when(!locked, |row| {
                row.on_click(cx.listener({
                    let key = key.to_string();
                    let value = option.value.clone();
                    move |this, _, _, cx| this.answer(&key, value.clone(), cx)
                }))
            })
    }

    fn field(
        &self,
        spec: &FieldSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locked = self.is_locked(spec);
        let key = spec.key.clone();
        let value = self.values.get(&key).cloned().unwrap_or_default();
        let control = match &spec.kind {
            // A note carries no answer: its whole row is the sentence, drawn where the label would be.
            FieldKind::Note => div().into_any_element(),
            FieldKind::Toggle => Switch::new(SharedString::from(format!("toggle-{key}")))
                .checked(value == "true")
                .disabled(locked)
                .on_click(cx.listener({
                    let key = key.clone();
                    move |this, checked: &bool, _, cx| {
                        this.answer(&key, checked.to_string(), cx);
                    }
                }))
                .into_any_element(),
            FieldKind::Directory => h_flex()
                .gap_2()
                .children(
                    self.inputs
                        .get(&key)
                        .map(|input| div().flex_1().child(Input::new(input).disabled(locked))),
                )
                .child(
                    Button::new(SharedString::from(format!("browse-{key}")))
                        .icon(crate::icons::icon(icons::FOLDER))
                        .disabled(locked)
                        .on_click(cx.listener({
                            let key = key.clone();
                            move |this, _, _, cx| this.pick_directory(key.clone(), cx)
                        })),
                )
                .into_any_element(),
            FieldKind::Radio { options } => v_flex()
                .gap_1p5()
                .children(
                    options
                        .iter()
                        .map(|option| self.radio(&key, option, option.value == value, locked, cx)),
                )
                .into_any_element(),
            FieldKind::Choice { options, custom } => {
                let typed = *custom && !is_listed(options, &value);
                let label = options
                    .iter()
                    .find(|option| option.value == value)
                    .map(|option| text(&option.label))
                    .unwrap_or_else(|| {
                        if typed {
                            t("custom_manual")
                        } else {
                            t("choose")
                        }
                    });
                v_flex()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("choice-{key}")))
                            .outline()
                            .label(label)
                            .disabled(locked)
                            .dropdown_menu({
                                let options = options.clone();
                                let custom = *custom;
                                let form = cx.entity().downgrade();
                                let key = key.clone();
                                move |mut menu, _, _| {
                                    for option in &options {
                                        let value = option.value.clone();
                                        let form = form.clone();
                                        let key = key.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(text(&option.label)).on_click(
                                                move |_, _, cx| {
                                                    form.update(cx, |form, cx| {
                                                        form.answer(&key, value.clone(), cx)
                                                    })
                                                    .ok();
                                                },
                                            ),
                                        );
                                    }
                                    if custom {
                                        let form = form.clone();
                                        let key = key.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(t("custom_manual")).on_click(
                                                move |_, window, cx| {
                                                    form.update(cx, |form, cx| {
                                                        form.start_custom(&key, window, cx)
                                                    })
                                                    .ok();
                                                },
                                            ),
                                        );
                                    }
                                    menu
                                }
                            }),
                    )
                    .children(
                        typed
                            .then(|| self.inputs.get(&key))
                            .flatten()
                            .map(|input| Input::new(input).disabled(locked)),
                    )
                    .into_any_element()
            }
            FieldKind::Text | FieldKind::Url | FieldKind::Secret => {
                let secret = matches!(spec.kind, FieldKind::Secret);
                let held = secret && self.saved.iter().any(|saved| saved == &key);
                h_flex()
                    .gap_2()
                    .items_center()
                    .children(self.inputs.get(&key).map(|input| {
                        div().flex_1().min_w_0().child(
                            Input::new(input).disabled(locked).when(secret, |field| {
                                field.content_type(InputContentType::Password).mask_toggle()
                            }),
                        )
                    }))
                    .when(held, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_size(style::text_10())
                                .text_color(style::white(style::TEXT_MUTED, cx))
                                .child(t("secret_saved")),
                        )
                    })
                    .into_any_element()
            }
        };

        // `.settings-row` washes to white/5 over 120ms; a form holds few enough rows to track hover.
        let wash = motion::color(
            motion::id("field-wash", &key),
            style::white(
                if self.hovered.as_deref() == Some(key.as_str()) {
                    style::RAISED
                } else {
                    0.
                },
                cx,
            ),
            window,
            cx,
        );
        let note = matches!(spec.kind, FieldKind::Note);
        let sentence = spec
            .help
            .as_ref()
            .map(text)
            .filter(|help| !help.is_empty())
            .unwrap_or_else(|| text(&spec.label));
        v_flex()
            .id(SharedString::from(format!("field-{key}")))
            .gap_1()
            .px_5()
            .py_2p5()
            .bg(wash)
            .on_hover(cx.listener({
                let key = key.clone();
                move |this, over: &bool, _, cx| {
                    // Only the row being left clears the mark, so entering the next one wins the race.
                    if *over {
                        this.hovered = Some(key.clone());
                    } else if this.hovered.as_deref() == Some(key.as_str()) {
                        this.hovered = None;
                    }
                    cx.notify();
                }
            }))
            .when(note, |field| {
                field.child(
                    div()
                        .text_xs()
                        .text_color(style::white(style::TEXT_SECONDARY, cx))
                        .child(sentence),
                )
            })
            .when(!note, |field| {
                field
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .text_color(style::white(style::TEXT_STRONG, cx))
                                    .child(text(&spec.label)),
                            )
                            .when(locked, |row| {
                                row.child(
                                    div()
                                        .text_size(style::text_10())
                                        .text_color(style::white(style::TEXT_SUBTLE, cx))
                                        .child(t("setting_managed_by_system")),
                                )
                            }),
                    )
                    .child(control)
                    .when_some(spec.help.as_ref(), |field, help| {
                        field.child(
                            div()
                                .text_xs()
                                .text_color(style::white(style::TEXT_MUTED, cx))
                                .child(text(help)),
                        )
                    })
            })
            .when_some(self.problem_for(&key), |field, problem| {
                field.child(div().text_xs().text_color(cx.theme().danger).child(problem))
            })
    }

    fn pick_directory(&mut self, key: String, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let value = path.to_string_lossy().to_string();
                this.update(cx, |this, cx| this.answer(&key, value, cx))
                    .ok();
            }
        })
        .detach();
    }
}

impl Render for SchemaForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut section: Option<SharedString> = None;
        let mut rows = Vec::new();
        for spec in self.specs.clone() {
            if !self.is_visible(&spec) {
                continue;
            }
            let heading = spec.section.as_ref().map(text);
            if heading.is_some() && heading != section {
                section = heading.clone();
                // `uppercase` is a CSS transform GPUI has no equivalent for.
                let title = heading.clone().unwrap_or_default().to_uppercase();
                rows.push(
                    div()
                        .px_5()
                        .pt_4()
                        .pb_1()
                        .text_xs()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(style::white(style::TEXT_MUTED, cx))
                        .child(SharedString::from(title))
                        .into_any_element(),
                );
            }
            rows.push(self.field(&spec, window, cx).into_any_element());
        }
        let form_problems: Vec<SharedString> = self
            .problems
            .iter()
            .filter(|problem| problem.field.is_none())
            .map(|problem| text(&problem.label))
            .collect();
        v_flex()
            .w_full()
            .children(rows)
            .children(form_problems.into_iter().map(|problem| {
                div()
                    .px_5()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(problem)
            }))
    }
}
