//! The "up next" tab: the whole queue in play order, reorderable, and a drop target for track rows.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollStrategy, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, Subscription, UniformListScrollHandle, Window, div,
    px, uniform_list,
};
use store::{ConfigModel, PlayerModel, QueueModel};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, TrackDrag, icons};

// Virtualized rows need one resolved height; this is the Dioxus rightbar's.
const ROW_HEIGHT: Pixels = px(60.);
// Above this point in a row a drop lands before it, below it after.
const DROP_SPLIT: Pixels = px(23.);

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

    fn row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
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
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let at = index as u32;

        div()
            .id(("queue-row", index))
            .relative()
            .h(ROW_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .rounded(cx.theme().radius)
            .when(active, |row| row.bg(cx.theme().list_active))
            .hover(|row| row.bg(cx.theme().list_hover))
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
                    .w(px(20.))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(if active {
                        cx.theme().primary
                    } else {
                        cx.theme().muted_foreground
                    })
                    .map(|lead| {
                        if active {
                            lead.child(icons::icon(icons::VOLUME).size(px(14.)))
                        } else {
                            lead.child((index + 1).to_string())
                        }
                    }),
            )
            .when(covers, |row| {
                row.child(Cover::new(artwork.as_ref()).size(px(40.)))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .when(active, |line| line.text_color(cx.theme().primary))
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(artist),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .child(
                        Button::new(("queue-up", index))
                            .ghost()
                            .xsmall()
                            .icon(icons::icon(icons::CHEVRON_UP))
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
                        Button::new(("queue-down", index))
                            .ghost()
                            .xsmall()
                            .icon(icons::icon(icons::CHEVRON_DOWN))
                            .tooltip(t("move_down"))
                            .disabled(at + 1 >= count)
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                QueueModel::global(cx)
                                    .update(cx, |queue, cx| queue.move_item(at, at + 1, cx));
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
                .py_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .hover(|line| line.text_color(cx.theme().foreground))
                .tooltip(|window, cx| Tooltip::new(t("jump_to_current_song")).build(window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.rearm(cx)))
                .child(label)
                .into_any_element(),
        )
    }
}

impl Render for QueuePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| this.row(index, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0()
                .px_1()
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
                    div().absolute().bottom_2().right_2().child(
                        Button::new("queue-jump-to-current")
                            .ghost()
                            .small()
                            .icon(icons::icon(icons::REFRESH))
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
