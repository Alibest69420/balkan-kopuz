//! Settings → Interface → Shortcuts: rebind a key, see what it would collide with, put it back.

use components::focus::{Axis, Roving, Step};
use components::format::{t, t_with};
use components::{icons, style};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};

/// A lone modifier is not a shortcut, so a recording waits for a real key.
const MODIFIERS: &[&str] = &[
    "shift", "control", "ctrl", "alt", "platform", "cmd", "function",
];

/// One listed command, flattened out of its group so the arrows walk the whole panel.
struct Row {
    id: &'static str,
    label: SharedString,
    group: Option<&'static str>,
}

pub(crate) struct Shortcuts {
    rows: Vec<Row>,
    roving: Roving,
    recording: Option<usize>,
    refused: Option<SharedString>,
    _capture: Option<Subscription>,
}

impl Shortcuts {
    pub(crate) fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut rows = Vec::new();
        for (title, entries) in crate::groups(cx) {
            let mut heading = Some(title);
            for entry in entries {
                rows.push(Row {
                    id: entry.id,
                    label: entry.label,
                    group: heading.take(),
                });
            }
        }
        let mut roving = Roving::new(Axis::Column, cx);
        roving.set_len(rows.len());
        Self {
            rows,
            roving,
            recording: None,
            refused: None,
            _capture: None,
        }
    }

    /// Take the next keystroke for `index`; the interceptor runs before any binding claims it.
    fn record(&mut self, index: usize, cx: &mut Context<Self>) {
        self.recording = Some(index);
        self.refused = None;
        let this = cx.entity().downgrade();
        self._capture = Some(cx.intercept_keystrokes(move |event, _, cx| {
            let key = event.keystroke.key.clone();
            if MODIFIERS.contains(&key.as_str()) {
                return;
            }
            cx.stop_propagation();
            let written = commands::written(&event.keystroke);
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |this, cx| this.pressed(&key, written, cx));
        }));
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.recording = None;
        self._capture = None;
        cx.notify();
    }

    /// What a recorded keystroke does: escape abandons, a collision is refused, anything else binds.
    fn pressed(&mut self, key: &str, written: String, cx: &mut Context<Self>) {
        let Some(index) = self.recording else {
            return;
        };
        let Some(id) = self.rows.get(index).map(|row| row.id) else {
            self.stop(cx);
            return;
        };
        if key == "escape" {
            self.stop(cx);
            return;
        }
        if let Some(held) = commands::conflict(id, &written, cx) {
            let label = held.label(&store::Target::None, cx);
            self.refused = Some(t_with(
                "shortcuts_conflict",
                &[
                    ("key", commands::keycap(&written).to_string()),
                    ("command", label.to_string()),
                ],
            ));
            self.stop(cx);
            return;
        }
        commands::rebind(id, Some(written), cx);
        self.stop(cx);
    }

    fn reset(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(id) = self.rows.get(index).map(|row| row.id) {
            commands::rebind(id, None, cx);
        }
        self.refused = None;
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.recording.is_some() {
            return;
        }
        match self.roving.walk(event) {
            Step::Moved(_) => cx.notify(),
            Step::Activate(at) => self.record(at, cx),
            Step::Ignored => cx.propagate(),
        }
    }

    fn row(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let row = self.rows.get(index)?;
        if !components::settings::matches(&[row.label.as_str()], cx) {
            return None;
        }
        let recording = self.recording == Some(index);
        let ringed = self.roving.is_active(index, window);
        let keystroke = commands::keystroke_for(row.id, cx);
        let changed = store::layout(cx).shortcuts.contains_key(row.id);
        let chip = match (recording, keystroke.as_deref()) {
            (true, _) => div()
                .px_2()
                .rounded(style::radius(cx))
                .bg(style::accent(style::ACTIVE, cx))
                .text_size(style::text_10())
                .text_color(cx.theme().foreground)
                .child(t("shortcuts_recording"))
                .into_any_element(),
            (false, Some(keystroke)) => crate::sheet::keycap(keystroke, cx).into_any_element(),
            (false, None) => div()
                .text_size(style::text_10())
                .text_color(style::white(style::TEXT_SUBTLE, cx))
                .child(t("shortcuts_unbound"))
                .into_any_element(),
        };
        Some(
            h_flex()
                .id(("shortcut-row", index))
                .relative()
                .w_full()
                .gap_3()
                .px_3()
                .py_1p5()
                .items_center()
                .rounded(style::radius_lg())
                .cursor_pointer()
                .hover(|row| row.bg(style::white(style::RAISED, cx)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.roving.set_cursor(index);
                    match this.recording == Some(index) {
                        true => this.stop(cx),
                        false => this.record(index, cx),
                    }
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(row.label.clone()),
                )
                // The dot a changed key wears, so a rebind is visible without opening the file.
                .when(changed, |row| {
                    row.child(
                        div()
                            .size_1p5()
                            .rounded_full()
                            .flex_shrink_0()
                            .bg(cx.theme().primary),
                    )
                })
                .child(chip)
                .when(changed, |row| {
                    row.child(
                        Button::new(("shortcut-reset", index))
                            .ghost()
                            .xsmall()
                            .icon(icons::icon(icons::REFRESH))
                            .tooltip(t("reset_home"))
                            .on_click(cx.listener(move |this, _, _, cx| this.reset(index, cx))),
                    )
                })
                .when(ringed, |row| {
                    row.child(components::focus::ring(style::radius_lg(), cx))
                })
                .into_any_element(),
        )
    }
}

impl Render for Shortcuts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut blocks: Vec<AnyElement> = Vec::new();
        for index in 0..self.rows.len() {
            let Some(rendered) = self.row(index, window, cx) else {
                continue;
            };
            if let Some(title) = self.rows[index].group {
                blocks.push(
                    div()
                        .w_full()
                        .pt_2()
                        .px_3()
                        .text_xs()
                        .font_semibold()
                        .text_color(style::white(style::TEXT_MUTED, cx))
                        .child(t(title))
                        .into_any_element(),
                );
            }
            blocks.push(rendered);
        }
        let list = self
            .roving
            .group(v_flex().id("shortcut-list"), "ShortcutList")
            .w_full()
            .gap_0p5()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key(event, cx)))
            .children(blocks);
        components::settings::section(crate::title())
            .child(
                div()
                    .w_full()
                    .px_3()
                    .pb_1()
                    .text_xs()
                    .text_color(style::white(style::TEXT_SUBTLE, cx))
                    .child(t("shortcuts_editor_hint")),
            )
            .children(self.refused.clone().map(|said| {
                div()
                    .w_full()
                    .px_3()
                    .pb_1()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(said)
            }))
            .child(list)
    }
}
