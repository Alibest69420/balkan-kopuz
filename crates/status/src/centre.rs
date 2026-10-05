//! The notification centre: every toast the app has shown, still readable after it faded.

use api::NoticeLevel;
use components::format::t;
use components::{Chip, ChipTone, icons, style};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};
use store::{Notices, Record};

use crate::{DONE, FAILED, ROW_GLYPH, empty, header, row};

/// The bell's body.
pub struct NoticeCentre {
    _subscriptions: Vec<Subscription>,
}

impl NoticeCentre {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let notices = Notices::global(cx);
        Self {
            _subscriptions: vec![cx.observe(&notices, |_, _, cx| cx.notify())],
        }
    }

    fn entry(&self, record: &Record, cx: &mut Context<Self>) -> AnyElement {
        let (glyph, ink) = match record.level {
            NoticeLevel::Error => (FAILED, cx.theme().danger),
            NoticeLevel::Warning => (icons::WARNING, cx.theme().warning),
            _ => (DONE, style::slate_400(cx)),
        };
        let repeats =
            (record.repeats > 1).then(|| SharedString::from(format!("×{}", record.repeats)));
        row()
            .items_start()
            .child(
                icons::icon(glyph)
                    .size(ROW_GLYPH)
                    .flex_shrink_0()
                    .text_color(ink),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(style::white(style::TEXT_STRONG, cx))
                    .child(record.message.clone()),
            )
            .children(repeats.map(|repeats| {
                div()
                    .flex_shrink_0()
                    .text_size(style::text_10())
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .child(repeats)
                    .into_any_element()
            }))
            .into_any_element()
    }
}

impl Render for NoticeCentre {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = Notices::global(cx).read(cx).history().to_vec();
        let clear = (!history.is_empty()).then(|| {
            Chip::new("clear-notices")
                .label(t("clear"))
                .tone(ChipTone::Ink(style::slate_400(cx)))
                .on_click(|_, _, cx| {
                    Notices::global(cx).update(cx, |notices, cx| notices.clear(cx));
                })
                .into_any_element()
        });
        let entries: Vec<AnyElement> = history
            .iter()
            .map(|record| self.entry(record, cx))
            .collect();
        v_flex()
            .w_full()
            .child(header(t("notifications"), clear, cx))
            .when(entries.is_empty(), |body| {
                body.child(empty(t("notifications_empty"), cx))
            })
            .when(!entries.is_empty(), |body| {
                body.child(
                    v_flex()
                        .id("notice-rows")
                        .w_full()
                        .py_1()
                        .max_h(crate::LIST_HEIGHT)
                        .overflow_y_scroll()
                        .children(entries),
                )
            })
    }
}
