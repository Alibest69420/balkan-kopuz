//! The "up next" tab: the whole queue in play order, reorderable, and a drop target for track rows.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollStrategy, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, Subscription, UniformListScrollHandle, Window, div,
    px, uniform_list,
};
use store::{ConfigModel, PlayerModel, QueueModel};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, TrackDrag, icons, motion, style};

use crate::channel;

// Virtualized rows need one resolved height; this is the Dioxus rightbar's.
const ROW_HEIGHT: Pixels = px(60.);
// Above this point in a row a drop lands before it, below it after.
const DROP_SPLIT: Pixels = px(23.);
// The row's `w-4` lead column, its 40px `rounded-md` thumb and the `text-xs` mark in the lead.
const LEAD_WIDTH: Pixels = px(16.);
const ROW_COVER: Pixels = px(40.);
const ROW_COVER_RADIUS: Pixels = px(6.);
const LEAD_GLYPH: Pixels = px(12.);
// `text-[9px]` chevrons in a `p-0.5` box: the reorder pair that fades in on hover.
const REORDER_BUTTON: Pixels = px(14.);
const REORDER_GLYPH: Pixels = px(9.);
// `text-white/65`: the artist line under a queued title.
const ROW_SUBTITLE: f32 = 0.65;
// The name the row's hover group answers to, so its reorder buttons can watch it.
const ROW_GROUP: &str = "queue-row";
// `absolute bottom-4 right-4 w-9 h-9` with a `w-5 h-5` glyph: the jump-back-to-current button.
const JUMP_BUTTON: Pixels = px(36.);
const JUMP_GLYPH: Pixels = px(20.);

/// A queue row dragged over the list, to reorder it in place.
#[derive(Clone)]
struct QueueRowDrag {
    from: u32,
    label: SharedString,
}

impl Render for QueueRowDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_sm()
            .child(self.label.clone())
    }
}

/// An insertion point turned into the index `QueueEdit::Move` wants, or nothing when it is a no-op.
fn move_target(from: u32, insert_at: u32, count: u32) -> Option<u32> {
    if from >= count || insert_at > count {
        return None;
    }
    let to = if from < insert_at {
        insert_at.saturating_sub(1)
    } else {
        insert_at
    };
    (to < count && to != from).then_some(to)
}

fn insert_at(at: u32, keys: Vec<String>, cx: &mut gpui_kit::App) {
    QueueModel::global(cx).update(cx, move |queue, cx| queue.insert(at, keys, cx));
}

/// A strip of a row that accepts a drop, either reordering the queue or inserting dragged tracks.
fn drop_zone(id: (&'static str, usize), at: u32, count: u32) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .left_0()
        .right_0()
        .on_drop::<TrackDrag>(move |drag, _, cx| insert_at(at, drag.keys.clone(), cx))
        .on_drop::<QueueRowDrag>(move |drag, _, cx| {
            if let Some(to) = move_target(drag.from, at, count) {
                let from = drag.from;
                QueueModel::global(cx).update(cx, move |queue, cx| queue.move_item(from, to, cx));
            }
        })
        .drag_over::<TrackDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
        .drag_over::<QueueRowDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
}

/// One chevron of the reorder pair: a `text-slate-500` glyph with barely a box around it.
fn reorder_button(id: (&'static str, usize), glyph: &'static str, cx: &App) -> Button {
    Button::new(id)
        .custom(style::quiet_button(style::slate_500(cx), cx))
        .with_size(REORDER_GLYPH * (4. / 3.))
        .size(REORDER_BUTTON)
        .icon(icons::icon(glyph))
}

/// The queue tab: rows in play order, the summary line, and auto-follow of what is playing.
pub struct QueuePanel {
    scroll: UniformListScrollHandle,
    auto_follow: bool,
    followed: Option<u32>,
    _subscriptions: Vec<Subscription>,
}

impl QueuePanel {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let queue = QueueModel::global(cx);
        let player = PlayerModel::global(cx);
        let config = ConfigModel::global(cx);
        let subscriptions = vec![
            cx.observe(&queue, |this, model, cx| {
                let position = model.read(cx).position();
                if position != this.followed {
                    this.followed = position;
                    this.follow();
                }
                cx.notify();
            }),
            cx.observe(&player, |_, _, cx| cx.notify()),
            cx.observe(&config, |_, _, cx| cx.notify()),
        ];
        Self {
            scroll: UniformListScrollHandle::new(),
            auto_follow: true,
            followed: None,
            _subscriptions: subscriptions,
        }
    }

    fn follow(&self) {
        if let (true, Some(index)) = (self.auto_follow, self.followed) {
            self.scroll
                .scroll_to_item_strict(index as usize, ScrollStrategy::Center);
        }
    }

    fn rearm(&mut self, cx: &mut Context<Self>) {
        self.auto_follow = true;
        self.follow();
        cx.notify();
    }

    fn row(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let model = QueueModel::global(cx);
        let queue = model.read(cx);
        let Some(track) = queue.items().get(index) else {
            return div().into_any_element();
        };
        let count = queue.len() as u32;
        let active = queue.position() == Some(index as u32);
        let title = SharedString::from(track.title.clone());
        let artist = SharedString::from(track.artist.clone());
        let artwork = track.artwork.clone();
        // The row's own uid, so the wash follows the track rather than the slot it sits in.
        let uid = SharedString::from(track.uid.clone());
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let at = index as u32;
        let raised = style::white(style::RAISED, cx);
        let wash_target = if active {
            style::accent(style::PLAYING, cx)
        } else {
            cx.theme().transparent
        };
        let lit_target = if active {
            cx.theme().primary
        } else {
            cx.theme().foreground
        };
        let lead_target = if active {
            cx.theme().primary
        } else {
            style::white(style::TEXT_SECONDARY, cx)
        };
        let wash = motion::color(channel(uid.clone(), "wash"), wash_target, window, cx);
        let lit = motion::color(channel(uid.clone(), "title"), lit_target, window, cx);
        let lead = motion::color(channel(uid, "lead"), lead_target, window, cx);

        div()
            .id(("queue-row", index))
            .group(ROW_GROUP)
            .relative()
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .rounded(style::radius_lg())
            .bg(wash)
            .hover(move |row| row.bg(raised))
            .on_click(move |_, _, cx| {
                QueueModel::global(cx).update(cx, |queue, cx| queue.jump(at, cx));
            })
            .on_drag(
                QueueRowDrag {
                    from: at,
                    label: title.clone(),
                },
                |drag, _, _, cx| {
                    cx.stop_propagation();
                    cx.new(|_| drag.clone())
                },
            )
            .child(
                div()
                    .w(LEAD_WIDTH)
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .items_end()
                    .text_xs()
                    .text_color(lead)
                    .map(|lead| {
                        if active {
                            lead.child(icons::icon(icons::VOLUME).size(LEAD_GLYPH))
                        } else {
                            lead.child((index + 1).to_string())
                        }
                    }),
            )
            .when(covers, |row| {
                row.child(
                    Cover::new(artwork.as_ref())
                        .size(ROW_COVER)
                        .rounded(ROW_COVER_RADIUS),
                )
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .gap_0p5()
                    .child(div().truncate().text_sm().text_color(lit).child(title))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(style::white(ROW_SUBTITLE, cx))
                            .child(artist),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .pr_1()
                    .invisible()
                    .group_hover(ROW_GROUP, |buttons| buttons.visible())
                    .child(
                        reorder_button(("queue-up", index), icons::CHEVRON_UP, cx)
                            .tooltip(t("move_up"))
                            .disabled(index == 0)
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                QueueModel::global(cx).update(cx, |queue, cx| {
                                    queue.move_item(at, at.saturating_sub(1), cx)
                                });
                            }),
                    )
                    .child(
                        reorder_button(("queue-down", index), icons::CHEVRON_DOWN, cx)
                            .tooltip(t("move_down"))
                            .disabled(at + 1 >= count)
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                QueueModel::global(cx)
                                    .update(cx, |queue, cx| queue.move_item(at, at + 1, cx));
                            }),
                    )
                    .child(
                        // QueueModel::remove existed and nothing called it, so a track queued by
                        // mistake could only be escaped by replacing the whole queue.
                        reorder_button(("queue-remove", index), icons::CLOSE, cx)
                            .tooltip(t("remove"))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                QueueModel::global(cx).update(cx, |queue, cx| queue.remove(at, cx));
                            }),
                    ),
            )
            .child(
                drop_zone(("queue-drop-before", index), at, count)
                    .top_0()
                    .h(DROP_SPLIT),
            )
            .child(
                drop_zone(("queue-drop-after", index), at + 1, count)
                    .bottom_0()
                    .h(ROW_HEIGHT - DROP_SPLIT),
            )
            .into_any_element()
    }

    /// "current/total • total duration", the way the Dioxus rightbar read; a station has neither.
    fn summary(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let queue = QueueModel::global(cx).read(cx);
        if queue.current().is_some_and(|track| track.is_radio()) {
            return None;
        }
        let position = queue.position().unwrap_or(0) as usize + 1;
        let count = queue.len();
        let total: u64 = queue
            .items()
            .iter()
            .filter_map(|track| track.duration_secs())
            .fold(0, |sum, secs| sum.saturating_add(secs));
        let label = format!("{position}/{count} • {}", duration(total * 1000));

        Some(
            div()
                .id("queue-summary")
                .px_2()
                .pt_1()
                .pb_2()
                .text_size(style::text_11())
                .text_color(style::white(style::TEXT_SECONDARY, cx))
                .hover(|line| line.text_color(style::white(0.85, cx)))
                .tooltip(|window, cx| Tooltip::new(t("jump_to_current_song")).build(window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.rearm(cx)))
                .child(label)
                .into_any_element(),
        )
    }
}

impl Render for QueuePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth_list("queue-scroll", &self.scroll, window, cx);
        let count = QueueModel::global(cx).read(cx).len();
        if count == 0 {
            return div()
                .size_full()
                .child(EmptyState::new(t("no_more_songs")).icon(icons::LIST))
                .into_any_element();
        }
        let end = count as u32;

        v_flex()
            .relative()
            .size_full()
            .children(self.summary(cx))
            .child(
                uniform_list(
                    "queue-list",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                        range.map(|index| this.row(index, window, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0()
                .px_2()
                .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                    if this.auto_follow {
                        this.auto_follow = false;
                        cx.notify();
                    }
                }))
                .on_drop::<TrackDrag>(move |drag, _, cx| insert_at(end, drag.keys.clone(), cx)),
            )
            .when(!self.auto_follow, |panel| {
                panel.child(
                    div().absolute().bottom_4().right_4().child(
                        Button::new("queue-jump-to-current")
                            // The variant's color is the ring; the scrim stays on the instance style.
                            .custom(style::button(
                                style::white(style::BORDER, cx),
                                style::black(0.6, cx),
                                style::white(style::TEXT_STRONG, cx),
                                cx,
                            ))
                            .with_size(JUMP_GLYPH * (4. / 3.))
                            .size(JUMP_BUTTON)
                            .rounded(JUMP_BUTTON / 2.)
                            .icon(icons::icon(icons::REFRESH))
                            .bg(style::black(0.4, cx))
                            .border_1()
                            .shadow_lg()
                            .tooltip(t("jump_to_current_song"))
                            .on_click(cx.listener(|this, _, _, cx| this.rearm(cx))),
                    ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::move_target;

    #[test]
    fn an_insertion_point_below_the_row_loses_the_row_itself() {
        assert_eq!(move_target(0, 3, 5), Some(2));
        assert_eq!(move_target(4, 1, 5), Some(1));
    }

    #[test]
    fn dropping_a_row_back_where_it_was_moves_nothing() {
        assert_eq!(move_target(2, 2, 5), None);
        assert_eq!(move_target(2, 3, 5), None);
        assert_eq!(move_target(5, 1, 5), None);
    }
}
