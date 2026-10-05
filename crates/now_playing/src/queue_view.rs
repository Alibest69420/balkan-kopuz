//! The queue: what has been played, what is playing, and what is next, in one list.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollStrategy, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, Subscription, UniformListScrollHandle, Window, div,
    px, uniform_list,
};
use store::{ConfigModel, PlaylistsModel, QueueModel, Target};
use widgets::format::{duration, t};
use widgets::{Cover, EmptyState, TrackDrag, dialogs, icons, menus, motion, style};

// Above this point in a row a drop lands before it, below it after.
const DROP_SPLIT: Pixels = px(23.);
const LEAD_WIDTH: Pixels = px(16.);
const ROW_COVER: Pixels = px(40.);
const LEAD_GLYPH: Pixels = px(12.);
const REORDER_BUTTON: Pixels = px(14.);
const REORDER_GLYPH: Pixels = px(9.);
const ROW_SUBTITLE: f32 = 0.65;
const ROW_GROUP: &str = "queue-row";
const JUMP_GLYPH: Pixels = px(20.);
const TOOL_GLYPH: Pixels = px(13.);

const CLEAR: &str = "icons/list-x.svg";
const SAVE: &str = "icons/bookmark-plus.svg";

/// One line of the list: a section heading, or the queue row at that play-order index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    History(usize),
    Now,
    Next(usize),
    Track(usize),
}

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

fn insert_at(at: u32, keys: Vec<String>, cx: &mut App) {
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

fn reorder_button(id: (&'static str, usize), glyph: &'static str, cx: &App) -> Button {
    Button::new(id)
        .custom(style::quiet_button(style::slate_500(cx), cx))
        .with_size(REORDER_GLYPH * (4. / 3.))
        .size(REORDER_BUTTON)
        .icon(icons::icon(glyph))
}

/// The queue, with its history folded away and its upcoming rows under your hand.
pub struct QueueView {
    scroll: UniformListScrollHandle,
    lines: Vec<Line>,
    auto_follow: bool,
    history_open: bool,
    followed: Option<u32>,
    _subscriptions: Vec<Subscription>,
}

impl QueueView {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let queue = QueueModel::global(cx);
        let subscriptions = vec![
            cx.observe(&queue, |this, model, cx| {
                let position = model.read(cx).position();
                this.rebuild(cx);
                if position != this.followed {
                    this.followed = position;
                    this.follow();
                }
                cx.notify();
            }),
            cx.observe(&store::PlayerModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
        ];
        let mut this = Self {
            scroll: UniformListScrollHandle::new(),
            lines: Vec::new(),
            auto_follow: true,
            history_open: false,
            followed: None,
            _subscriptions: subscriptions,
        };
        this.rebuild(cx);
        this
    }

    /// The section layout, recomputed where the queue changes rather than once a frame.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let queue = QueueModel::global(cx);
        let queue = queue.read(cx);
        let count = queue.len();
        let current = queue.position().map(|at| at as usize);
        self.lines.clear();
        let Some(current) = current else {
            self.lines.extend((0..count).map(Line::Track));
            return;
        };
        if current > 0 {
            self.lines.push(Line::History(current));
            if self.history_open {
                self.lines.extend((0..current).map(Line::Track));
            }
        }
        self.lines.push(Line::Now);
        self.lines.push(Line::Track(current));
        let upcoming = count.saturating_sub(current + 1);
        if upcoming > 0 {
            self.lines.push(Line::Next(upcoming));
            self.lines.extend((current + 1..count).map(Line::Track));
        }
    }

    fn follow(&self) {
        let Some(index) = self.followed else { return };
        if !self.auto_follow {
            return;
        }
        if let Some(line) = self
            .lines
            .iter()
            .position(|line| *line == Line::Track(index as usize))
        {
            self.scroll
                .scroll_to_item_strict(line, ScrollStrategy::Center);
        }
    }

    fn rearm(&mut self, cx: &mut Context<Self>) {
        self.auto_follow = true;
        self.follow();
        cx.notify();
    }

    fn clear_upcoming(&mut self, cx: &mut Context<Self>) {
        let model = QueueModel::global(cx);
        let (at, keys) = {
            let queue = model.read(cx);
            let at = queue.position().map_or(0, |at| at + 1);
            (
                at,
                queue
                    .upcoming()
                    .iter()
                    .map(|track| track.key.clone())
                    .collect::<Vec<_>>(),
            )
        };
        if keys.is_empty() {
            return;
        }
        let count = keys.len();
        model.update(cx, |queue, cx| queue.set_upcoming(Vec::new(), cx));
        store::Notices::undo(
            widgets::format::t_with("cleared_upcoming", &[("count", count.to_string())]),
            move |cx| insert_at(at, keys.clone(), cx),
            cx,
        );
    }

    fn shuffle_upcoming(&mut self, cx: &mut Context<Self>) {
        let model = QueueModel::global(cx);
        let mut keys: Vec<String> = model
            .read(cx)
            .upcoming()
            .iter()
            .map(|track| track.key.clone())
            .collect();
        if keys.len() < 2 {
            return;
        }
        shuffle(&mut keys);
        model.update(cx, |queue, cx| queue.set_upcoming(keys, cx));
    }

    fn save_as_playlist(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys: Vec<String> = QueueModel::global(cx)
            .read(cx)
            .items()
            .iter()
            .map(|track| track.key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        dialogs::open_rename(
            t("save_queue_as_playlist"),
            t("up_next"),
            move |name, cx| {
                let keys = keys.clone();
                PlaylistsModel::global(cx)
                    .update(cx, |playlists, cx| playlists.create(name, keys, cx));
            },
            window,
            cx,
        );
    }

    /// The row a list slot holds: a heading, or the queue row at that play-order index.
    fn line(&self, slot: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.lines.get(slot).copied() {
            Some(Line::Track(index)) => self.row(index, window, cx),
            Some(Line::History(count)) => self.history_heading(count, cx),
            Some(Line::Now) => heading(t("queue_now"), None, cx).into_any_element(),
            Some(Line::Next(count)) => {
                heading(t("queue_next_up"), Some(count), cx).into_any_element()
            }
            None => div().into_any_element(),
        }
    }

    fn history_heading(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let glyph = if self.history_open {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        heading(t("queue_history"), Some(count), cx)
            .id("queue-history-heading")
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.history_open = !this.history_open;
                this.rebuild(cx);
                cx.notify();
            }))
            .child(
                icons::icon(glyph)
                    .size(LEAD_GLYPH)
                    .text_color(style::slate_500(cx)),
            )
            .into_any_element()
    }

    fn row(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let model = QueueModel::global(cx);
        let queue = model.read(cx);
        let Some(track) = queue.items().get(index) else {
            return div().into_any_element();
        };
        let count = queue.len() as u32;
        let active = queue.position() == Some(index as u32);
        let played = queue.position().is_some_and(|at| (index as u32) < at);
        let title = SharedString::from(track.title.clone());
        let key = SharedString::from(track.key.clone());
        let artist = SharedString::from(track.artist.clone());
        let artwork = track.artwork.clone();
        let uid = SharedString::from(track.uid.clone());
        let covers = ConfigModel::global(cx).read(cx).row_images();
        let at = index as u32;
        let raised = style::white(style::RAISED, cx);
        let accent = motion::accent(theme::Surface::Sheet, cx.theme().primary, window, cx);
        let wash_target = if active {
            accent.opacity(style::PLAYING)
        } else {
            cx.theme().transparent
        };
        let lit_target = if active {
            theme::readable_on(accent, cx.theme().background)
        } else if played {
            style::white(style::TEXT_MUTED, cx)
        } else {
            cx.theme().foreground
        };
        let lead_target = if active {
            theme::readable_on(accent, cx.theme().background)
        } else {
            style::white(style::TEXT_SECONDARY, cx)
        };
        let wash = motion::color(crate::channel(uid.clone(), "wash"), wash_target, window, cx);
        let lit = motion::color(crate::channel(uid.clone(), "title"), lit_target, window, cx);
        let lead = motion::color(crate::channel(uid, "lead"), lead_target, window, cx);

        div()
            .id(("queue-row", index))
            .group(ROW_GROUP)
            .relative()
            .h(style::media_row())
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
                    .relative()
                    .w(LEAD_WIDTH)
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .items_end()
                    .text_xs()
                    .text_color(lead)
                    .map(|slot| {
                        if active {
                            slot.child(icons::icon(icons::VOLUME).size(LEAD_GLYPH))
                        } else {
                            // Upstream the number steps aside for a play glyph under the pointer.
                            slot.child(
                                div()
                                    .group_hover(ROW_GROUP, |number| number.invisible())
                                    .child((index + 1).to_string()),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .invisible()
                                    .group_hover(ROW_GROUP, |play| play.visible())
                                    .child(icons::icon(icons::PLAY).size(LEAD_GLYPH)),
                            )
                        }
                    }),
            )
            .when(covers, |row| {
                row.child(
                    Cover::new(artwork.as_ref())
                        .size(ROW_COVER)
                        .rounded(style::thumb_radius()),
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
                        reorder_button(("queue-remove", index), icons::CLOSE, cx)
                            .tooltip(t("remove"))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                store::actions::queue_remove(at, cx);
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
                    .h(style::media_row() - DROP_SPLIT),
            )
            .context_menu(menus::builder(Target::QueueItem { index: at, key }))
            .into_any_element()
    }

    /// "3/57 • 3:41:02", then everything that can be done to the rest of the queue.
    fn toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let _ = window;
        let model = QueueModel::global(cx);
        let queue = model.read(cx);
        let radio = queue.current().is_some_and(|track| track.is_radio());
        let summary = queue.summary();
        let upcoming = queue.upcoming().len();
        let empty = queue.is_empty();
        let label = SharedString::from(format!(
            "{}/{} • {}",
            summary.position.max(1),
            summary.count,
            duration(summary.total_ms)
        ));

        h_flex()
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .pt_1()
            .pb_2()
            .child(
                div()
                    .id("queue-summary")
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(style::text_11())
                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                    .hover(|line| line.text_color(style::white(0.85, cx)))
                    .tooltip(|window, cx| Tooltip::new(t("jump_to_current_song")).build(window, cx))
                    .on_click(cx.listener(|this, _, _, cx| this.rearm(cx)))
                    .child(if radio { t("radio") } else { label }),
            )
            .child(
                tool("queue-shuffle-upcoming", icons::SHUFFLE, cx)
                    .tooltip(t("shuffle_upcoming"))
                    .disabled(upcoming < 2)
                    .on_click(cx.listener(|this, _, _, cx| this.shuffle_upcoming(cx))),
            )
            .child(
                tool("queue-clear-upcoming", CLEAR, cx)
                    .tooltip(t("clear_upcoming"))
                    .disabled(upcoming == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.clear_upcoming(cx))),
            )
            .child(
                tool("queue-save", SAVE, cx)
                    .tooltip(t("save_queue_as_playlist"))
                    .disabled(empty)
                    .on_click(cx.listener(|this, _, window, cx| this.save_as_playlist(window, cx))),
            )
    }
}

impl Render for QueueView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth_list("queue-scroll", &self.scroll, window, cx);
        let count = QueueModel::global(cx).read(cx).len();
        if count == 0 {
            return v_flex()
                .size_full()
                .child(EmptyState::new(t("no_more_songs")).icon(icons::LIST))
                .into_any_element();
        }
        let end = count as u32;
        let lines = self.lines.len();

        v_flex()
            .relative()
            .size_full()
            .child(self.toolbar(window, cx))
            .child(
                uniform_list(
                    "queue-list",
                    lines,
                    cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                        range.map(|slot| this.line(slot, window, cx)).collect()
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
                        style::dress(
                            Button::new("queue-jump-to-current"),
                            style::black(0.4, cx),
                            style::black(0.6, cx),
                            style::white(style::TEXT_STRONG, cx),
                            style::white(style::BORDER, cx),
                            cx,
                        )
                        .with_size(JUMP_GLYPH * (4. / 3.))
                        .size(style::chrome().round_button)
                        .rounded(style::chrome().round_button / 2.)
                        .icon(icons::icon(icons::REFRESH))
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

/// A section heading, drawn at row height so the whole list can stay one `uniform_list`.
fn heading(label: SharedString, count: Option<usize>, cx: &App) -> Stateful<Div> {
    h_flex()
        .id("queue-heading")
        .h(style::media_row())
        .w_full()
        .items_end()
        .gap_2()
        .px_2()
        .pb_2()
        .text_size(style::text_11())
        .text_color(style::white(style::TEXT_MUTED, cx))
        .child(div().child(label))
        .children(count.map(|count| div().child(count.to_string())))
}

fn tool(id: &'static str, glyph: &'static str, cx: &App) -> Button {
    Button::new(id)
        .custom(style::quiet_button(style::slate_500(cx), cx))
        .with_size(TOOL_GLYPH * (4. / 3.))
        .size(style::chrome().tool_button)
        .rounded(style::chrome().tool_button / 2.)
        .icon(icons::icon(glyph))
}

/// A Fisher-Yates walk off the hasher's per-process seed; nothing here needs a real RNG.
fn shuffle(keys: &mut [String]) {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    for index in (1..keys.len()).rev() {
        let mut hasher = state.build_hasher();
        hasher.write_usize(index);
        let pick = (hasher.finish() % (index as u64 + 1)) as usize;
        keys.swap(index, pick);
    }
}

#[cfg(test)]
mod tests {
    use super::{move_target, shuffle};

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

    #[test]
    fn shuffling_keeps_every_row_it_was_given() {
        let mut keys: Vec<String> = (0..20).map(|n| n.to_string()).collect();
        let before = keys.clone();
        shuffle(&mut keys);
        keys.sort();
        let mut before = before;
        before.sort();
        assert_eq!(keys, before);
    }
}
