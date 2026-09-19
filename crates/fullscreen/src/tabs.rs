//! The right-hand Up next / Lyrics panel. Its lists are deliberately plain; `side_panel` owns the full ones.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled, div, px, uniform_list,
};
use store::{ConfigModel, PlayerModel, QueueModel};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, Loading, icons};

use crate::player::{FullscreenPlayer, Tab};

const ROW_HEIGHT: Pixels = px(56.);

impl FullscreenPlayer {
    pub(crate) fn panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background.opacity(0.45))
            .child(self.tab_bar(cx))
            .child(match self.tab {
                Tab::Queue => self.queue(cx),
                Tab::Lyrics => self.lyrics(cx),
            })
    }

    /// The chevron that brings the panel back once it is collapsed.
    pub(crate) fn expand_button(&self) -> impl IntoElement {
        div().absolute().right_4().top_1_2().child(
            Button::new("fs-expand-panel")
                .ghost()
                .icon(icons::icon(icons::CHEVRON_LEFT))
                .tooltip(t("show_side_panel"))
                .on_click(|_, _, cx| set_collapsed(false, cx)),
        )
    }

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_1()
            .items_center()
            .px_4()
            .py_3()
            .child(
                Button::new("fs-tab-queue")
                    .ghost()
                    .small()
                    .label(t("up_next"))
                    .toggled(self.tab == Tab::Queue)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tab = Tab::Queue;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("fs-tab-lyrics")
                    .ghost()
                    .small()
                    .label(t("lyrics"))
                    .toggled(self.tab == Tab::Lyrics)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tab = Tab::Lyrics;
                        this.load_lyrics(cx);
                        cx.notify();
                    })),
            )
            .child(
                div().ml_auto().child(
                    Button::new("fs-collapse-panel")
                        .ghost()
                        .small()
                        .icon(icons::icon(icons::CHEVRON_RIGHT))
                        .tooltip(t("hide_side_panel"))
                        .on_click(|_, _, cx| set_collapsed(true, cx)),
                ),
            )
    }

    fn queue(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = QueueModel::global(cx).read(cx).len();
        if count == 0 {
            return EmptyState::new(t("up_next"))
                .icon(icons::LIST)
                .into_any_element();
        }
        uniform_list(
            "fs-queue",
            count,
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                range.map(|index| this.queue_row(index, cx)).collect()
            }),
        )
        .flex_1()
        .size_full()
        .into_any_element()
    }

    fn queue_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let queue = QueueModel::global(cx);
        let Some(track) = queue.read(cx).items().get(index).cloned() else {
            return div().into_any_element();
        };
        let current = queue.read(cx).position() == Some(index as u32);
        let length = track
            .duration_ms
            .map(duration)
            .unwrap_or_else(|| "--:--".into());

        div()
            .id(("fs-queue-row", index))
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .hover(|row| row.bg(cx.theme().list_hover))
            .when(current, |row| row.bg(cx.theme().list_active))
            .child(Cover::new(track.artwork.as_ref()).size(px(36.)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .when(current, |title| title.text_color(cx.theme().primary))
                            .child(SharedString::from(track.title.clone())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(track.artist.clone())),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(length),
            )
            .on_click(move |_, _, cx| {
                QueueModel::global(cx).update(cx, |queue, cx| queue.jump(index as u32, cx))
            })
            .into_any_element()
    }

    fn lyrics(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(query) = self.lyrics.as_ref() else {
            return EmptyState::new(t("lyrics_not_found"))
                .icon(icons::LYRICS)
                .into_any_element();
        };
        let query = query.read(cx);
        let Some(view) = query.value() else {
            return if query.is_loading() {
                Loading.into_any_element()
            } else {
                EmptyState::new(t("lyrics_not_found"))
                    .icon(icons::LYRICS)
                    .into_any_element()
            };
        };

        let position = PlayerModel::global(cx).read(cx).position_ms();
        let lines: Vec<(SharedString, bool)> = if view.synced.is_empty() {
            view.plain
                .as_deref()
                .unwrap_or_default()
                .lines()
                .map(|line| (SharedString::from(line.to_string()), false))
                .collect()
        } else {
            let active = view
                .synced
                .iter()
                .rposition(|line| line.start_ms <= position);
            view.synced
                .iter()
                .enumerate()
                .map(|(index, line)| (SharedString::from(line.text.clone()), Some(index) == active))
                .collect()
        };
        if lines.is_empty() {
            return EmptyState::new(t("lyrics_not_found"))
                .icon(icons::LYRICS)
                .into_any_element();
        }

        v_flex()
            .id("fs-lyrics")
            .flex_1()
            .size_full()
            .gap_2()
            .px_6()
            .py_4()
            .overflow_y_scroll()
            .text_color(cx.theme().muted_foreground)
            .children(lines.into_iter().map(|(text, active)| {
                div()
                    .when(active, |line| line.text_color(cx.theme().foreground))
                    .child(text)
            }))
            .into_any_element()
    }
}

fn set_collapsed(collapsed: bool, cx: &mut gpui_kit::App) {
    ConfigModel::global(cx).update(cx, |model, cx| {
        model.update(cx, |config| config.fullscreen_tabs_collapsed = collapsed)
    });
}
