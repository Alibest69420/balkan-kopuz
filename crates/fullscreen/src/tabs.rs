//! The right-hand Up next / Lyrics panel. Its lists are deliberately plain; `side_panel` owns the full ones.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled, Window, div, px, rems,
    uniform_list,
};
use store::{ConfigModel, PlayerModel, QueueModel};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, Loading, icons, motion, style};

use crate::player::{FullscreenPlayer, Tab};

// The Dioxus fullscreen queue's own row height, taller than the library's.
const ROW_HEIGHT: Pixels = px(76.);
const ROW_COVER: Pixels = px(48.);

impl FullscreenPlayer {
    pub(crate) fn panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bar = self.tab_bar(window, cx);
        let body = match self.tab {
            Tab::Queue => self.queue(cx),
            Tab::Lyrics => self.lyrics(window, cx),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .border_l_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::black(0.45, cx))
            .child(bar)
            .child(body)
    }

    /// The chevron that brings the panel back once it is collapsed.
    pub(crate) fn expand_button(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = self.hovered == Some("fs-expand-panel");
        let scrim = motion::color(
            "fs-expand-panel",
            if on {
                style::black(0.6, cx)
            } else {
                style::black(0.4, cx)
            },
            window,
            cx,
        );
        let glyph = motion::color(
            "fs-expand-glyph",
            if on {
                cx.theme().foreground
            } else {
                style::white(0.7, cx)
            },
            window,
            cx,
        );
        // The frame owns the hover: a tooltip installs its own, and two on one element panic.
        div()
            .id("fs-expand-frame")
            .absolute()
            .right_4()
            .top_1_2()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.set_hover("fs-expand-panel", *hovered, cx)
            }))
            .child(
                Button::new("fs-expand-panel")
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(scrim)
                            .foreground(glyph)
                            .hover(scrim)
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

    fn tab_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let collapse = self.tint(
            "fs-collapse-panel",
            style::white(0.4, cx),
            cx.theme().foreground,
            window,
            cx,
        );
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
                        self.tab_pill(
                            "fs-tab-queue",
                            t("up_next"),
                            self.tab == Tab::Queue,
                            window,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tab = Tab::Queue;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.tab_pill(
                            "fs-tab-lyrics",
                            t("lyrics"),
                            self.tab == Tab::Lyrics,
                            window,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tab = Tab::Lyrics;
                            this.load_lyrics(cx);
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .id("fs-collapse-frame")
                    .ml_auto()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.set_hover("fs-collapse-panel", *hovered, cx)
                    }))
                    .child(
                        Button::new("fs-collapse-panel")
                            .custom(ButtonCustomVariant::new(cx).foreground(collapse))
                            .with_size(px(32.))
                            .icon(icons::icon(icons::CHEVRON_RIGHT).small())
                            .tooltip(t("hide_side_panel"))
                            .on_click(|_, _, cx| set_collapsed(true, cx)),
                    ),
            )
            .into_any_element()
    }

    /// `px-4 py-1.5 text-xs font-medium rounded-md`, lit by `bg-white/20` when it is the open tab.
    fn tab_pill(
        &self,
        id: &'static str,
        label: SharedString,
        active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let wash = motion::color(
            SharedString::from(format!("{id}-wash")),
            if active {
                style::white(style::ACTIVE, cx)
            } else {
                style::white(0., cx)
            },
            window,
            cx,
        );
        let hovered = self.hovered == Some(id);
        let text = motion::color(
            SharedString::from(format!("{id}-text")),
            match (active, hovered) {
                (true, _) => cx.theme().foreground,
                (false, true) => style::white(0.8, cx),
                (false, false) => style::white(0.5, cx),
            },
            window,
            cx,
        );
        div()
            .id(id)
            .px_4()
            .py_1p5()
            .rounded(rems(0.375))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .bg(wash)
            .text_color(text)
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
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
            cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                range
                    .map(|index| this.queue_row(index, window, cx))
                    .collect()
            }),
        )
        .flex_1()
        .size_full()
        .px_4()
        .py_2()
        .into_any_element()
    }

    fn queue_row(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let queue = QueueModel::global(cx);
        let Some(track) = queue.read(cx).items().get(index).cloned() else {
            return div().into_any_element();
        };
        let current = queue.read(cx).position() == Some(index as u32);
        let length = track
            .duration_ms
            .map(duration)
            .unwrap_or_else(|| "--:--".into());
        // The wash and the title follow which row is playing; hover stays instant in a virtual list.
        // A queue may hold one track twice, so the slot disambiguates the channel.
        let slot = SharedString::from(format!("{}-{index}", track.key));
        let wash = motion::color(
            SharedString::from(format!("fs-queue-wash-{slot}")),
            if current {
                style::accent(style::PLAYING, cx)
            } else {
                style::white(0., cx)
            },
            window,
            cx,
        );
        let title_color = motion::color(
            SharedString::from(format!("fs-queue-title-{slot}")),
            if current {
                cx.theme().primary
            } else {
                cx.theme().foreground
            },
            window,
            cx,
        );

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
            .bg(wash)
            .hover(|row| row.bg(style::white(style::RAISED, cx)))
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
                            .text_color(title_color)
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

    fn lyrics(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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

        // `transition-colors duration-300` per line: the sung line lights while the last one fades.
        let idle = style::white(0.4, cx);
        let sung = cx.theme().foreground;
        let key = self.lyrics_key.clone();
        let painted: Vec<AnyElement> = lines
            .into_iter()
            .enumerate()
            .map(|(index, (text, active))| {
                let color = motion::color_over(
                    SharedString::from(format!("fs-lyric-{key}-{index}")),
                    if active { sung } else { idle },
                    motion::SLOW,
                    window,
                    cx,
                );
                div()
                    .w_full()
                    .text_color(color)
                    .child(text)
                    .into_any_element()
            })
            .collect();

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
            .children(painted)
            .into_any_element()
    }
}

fn set_collapsed(collapsed: bool, cx: &mut gpui_kit::App) {
    ConfigModel::global(cx).update(cx, |model, cx| {
        model.update(cx, |config| config.fullscreen_tabs_collapsed = collapsed)
    });
}
