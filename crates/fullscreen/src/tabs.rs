//! The right-hand Up next / Lyrics panel. Its lists are deliberately plain; `side_panel` owns the full ones.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled, div, px, rems, uniform_list,
};
use store::{ConfigModel, PlayerModel, QueueModel};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, Loading, icons, style};

use crate::player::{FullscreenPlayer, Tab};

// The Dioxus fullscreen queue's own row height, taller than the library's.
const ROW_HEIGHT: Pixels = px(76.);
const ROW_COVER: Pixels = px(48.);

impl FullscreenPlayer {
    pub(crate) fn panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .border_l_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::black(0.45, cx))
            .child(self.tab_bar(cx))
            .child(match self.tab {
                Tab::Queue => self.queue(cx),
                Tab::Lyrics => self.lyrics(cx),
            })
    }

    /// The chevron that brings the panel back once it is collapsed.
    pub(crate) fn expand_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().absolute().right_4().top_1_2().child(
            Button::new("fs-expand-panel")
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(style::black(0.4, cx))
                        .foreground(style::white(0.7, cx))
                        .hover(style::black(0.6, cx))
                        .active(style::black(0.6, cx))
                        .shadow(true),
                )
                .rounded(px(999.))
                .with_size(px(36.))
                .icon(icons::icon(icons::CHEVRON_LEFT))
                .tooltip(t("show_side_panel"))
                .on_click(|_, _, cx| set_collapsed(false, cx)),
        )
    }

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .px_6()
            .pt_4()
            .pb_2()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .p_1()
                    .rounded(style::radius_lg())
                    .bg(style::white(style::HOVER, cx))
                    .child(
                        self.tab_pill("fs-tab-queue", t("up_next"), self.tab == Tab::Queue, cx)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tab = Tab::Queue;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.tab_pill("fs-tab-lyrics", t("lyrics"), self.tab == Tab::Lyrics, cx)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tab = Tab::Lyrics;
                                this.load_lyrics(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().ml_auto().child(
                    Button::new("fs-collapse-panel")
                        .ghost()
                        .with_size(px(32.))
                        .icon(icons::icon(icons::CHEVRON_RIGHT).small())
                        .tooltip(t("hide_side_panel"))
                        .on_click(|_, _, cx| set_collapsed(true, cx)),
                ),
            )
    }

    /// `px-4 py-1.5 text-xs font-medium rounded-md`, lit by `bg-white/20` when it is the open tab.
    fn tab_pill(
        &self,
        id: &'static str,
        label: SharedString,
        active: bool,
        cx: &Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .px_4()
            .py_1p5()
            .rounded(rems(0.375))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .map(|pill| {
                if active {
                    pill.bg(style::white(style::ACTIVE, cx))
                        .text_color(cx.theme().foreground)
                } else {
                    pill.text_color(style::white(0.5, cx))
                        .hover(|idle| idle.text_color(style::white(0.8, cx)))
                }
            })
            .child(label)
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
        .px_4()
        .py_2()
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
            .gap_4()
            .px_4()
            .py_2()
            .rounded(style::radius_lg())
            .cursor_pointer()
            .hover(|row| row.bg(style::white(style::RAISED, cx)))
            .when(current, |row| row.bg(style::accent(style::PLAYING, cx)))
            .child(
                div()
                    .w_4()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                    .child((index + 1).to_string()),
            )
            .child(
                Cover::new(track.artwork.as_ref())
                    .size(ROW_COVER)
                    .rounded(px(6.)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .truncate()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .when(current, |title| title.text_color(cx.theme().primary))
                            .child(SharedString::from(track.title.clone())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(style::white(0.65, cx))
                            .child(SharedString::from(track.artist.clone())),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(style::white(style::TEXT_SECONDARY, cx))
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
            .gap_1()
            .px_4()
            .py_2()
            .overflow_y_scroll()
            .text_2xl()
            .font_weight(FontWeight::SEMIBOLD)
            .text_center()
            .text_color(style::white(0.4, cx))
            .children(lines.into_iter().map(|(text, active)| {
                div()
                    .w_full()
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
