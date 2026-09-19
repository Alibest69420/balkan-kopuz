//! The Ctrl/Cmd+K overlay: type, arrow, Enter, and the track plays (docs/PARITY.md §3.13).

use std::time::Duration;

use api::{TrackFilter, TrackInfo, TrackSort};
use gpui_kit::component::input::{Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Task, Window, div, px,
};
use store::{Chrome, ConfigModel, Link};
use widgets::format::{duration, t, t_with};
use widgets::{Cover, icons};

const SHOWN_RESULTS: usize = 8;
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Rendered only while `Chrome::is_quick_search_open`.
pub struct QuickSearch {
    input: Entity<InputState>,
    results: Vec<TrackInfo>,
    query: String,
    selected: usize,
    // The current query has an answer; the "no results" line waits for it.
    settled: bool,
    // Enter arrived before the answer did, so the first result plays once it lands.
    pending_play: bool,
    open: bool,
    // Closing hands focus back, or the shell's keys would have nowhere to dispatch from.
    was_focused: Option<FocusHandle>,
    _search: Option<Task<()>>,
    _play: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl QuickSearch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("quick_search_placeholder")));
        let chrome = Chrome::global(cx);
        let subscriptions =
            vec![
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::Change => this.typed(window, cx),
                        InputEvent::PressEnter { .. } => this.enter(cx),
                        _ => {}
                    }
                }),
                cx.observe_in(&chrome, window, |this, chrome, window, cx| {
                    let open = chrome.read(cx).is_quick_search_open();
                    if open != this.open {
                        this.open = open;
                        if open {
                            this.reopen(window, cx);
                        } else if let Some(handle) = this.was_focused.take() {
                            window.focus(&handle, cx);
                        }
                    }
                }),
            ];
        let open = chrome.read(cx).is_quick_search_open();
        Self {
            input,
            results: Vec::new(),
            query: String::new(),
            selected: 0,
            settled: false,
            pending_play: false,
            open,
            was_focused: None,
            _search: None,
            _play: None,
            _subscriptions: subscriptions,
        }
    }

    /// Every opening starts from an empty palette, with the caret already in it.
    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.results.clear();
        self.query.clear();
        self.selected = 0;
        self.settled = false;
        self.pending_play = false;
        self._search = None;
        self.was_focused = window.focused(cx);
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_quick_search(false, cx));
    }

    fn typed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.query = self.input.read(cx).value().trim().to_string();
        self.selected = 0;
        self.settled = false;
        self.pending_play = false;
        if self.query.is_empty() {
            self.results.clear();
            self._search = None;
            cx.notify();
            return;
        }
        let query = self.query.clone();
        self._search = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let Ok(call) = this.update(cx, |_, cx| {
                Link::global(cx).call(move |api| async move { api.search(query).await })
            }) else {
                return;
            };
            let found = call.await;
            this.update(cx, |this, cx| {
                match found {
                    Ok(found) => this.results = found.tracks,
                    Err(error) => {
                        tracing::debug!(%error, "quick search failed");
                        this.results.clear();
                    }
                }
                this.settled = true;
                this.selected = 0;
                if this.pending_play {
                    this.pending_play = false;
                    this.play(0, cx);
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn enter(&mut self, cx: &mut Context<Self>) {
        if self.settled && !self.results.is_empty() {
            let selected = self.selected.min(self.shown().saturating_sub(1));
            self.play(selected, cx);
        } else if !self.query.is_empty() {
            self.pending_play = true;
        }
    }

    fn shown(&self) -> usize {
        self.results.len().min(SHOWN_RESULTS)
    }

    fn move_selection(&mut self, forward: bool, cx: &mut Context<Self>) {
        let last = self.shown().saturating_sub(1);
        self.selected = if forward {
            (self.selected + 1).min(last)
        } else {
            self.selected.saturating_sub(1)
        };
        cx.notify();
    }

    /// Plays the whole library so shuffle keeps covering it, and the results alone when the hit is not in it.
    fn play(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(track) = self.results.get(index).cloned() else {
            return;
        };
        let results = self.results.clone();
        self.close(cx);
        let sort = ConfigModel::global(cx)
            .read(cx)
            .config()
            .library_sort
            .clone();
        let filter = TrackFilter {
            sort: TrackSort::Fields(sort),
            ..Default::default()
        };
        let call =
            Link::global(cx).call(move |api| async move { api.tracks(filter, store::all()).await });
        self._play = Some(cx.spawn(async move |_, cx| {
            let library = match call.await {
                Ok(page) => page.items,
                Err(error) => {
                    tracing::debug!(%error, "could not read the library to play a search hit");
                    Vec::new()
                }
            };
            cx.update(|cx| {
                if let Some(at) = library.iter().position(|row| row.uid == track.uid) {
                    store::actions::play_tracks(keys(&library), Some(at as u32), cx);
                } else if let Some(at) = results.iter().position(|row| row.uid == track.uid) {
                    store::actions::play_tracks(keys(&results), Some(at as u32), cx);
                }
            });
        }));
    }

    fn row(&self, index: usize, track: &TrackInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let covers = ConfigModel::global(cx).read(cx).config().show_row_images;
        let selected = index == self.selected.min(self.shown().saturating_sub(1));
        h_flex()
            .id(SharedString::from(format!("quick-search-{}", track.uid)))
            .gap_3()
            .px_4()
            .py_2()
            .when(selected, |row| row.bg(cx.theme().list_active))
            .hover(|row| row.bg(cx.theme().list_hover))
            .when(covers, |row| {
                row.child(Cover::new(track.artwork.as_ref()).size(px(36.)))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(track.title.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(track.artist.clone()),
                    ),
            )
            .children(track.duration_secs().map(|secs| {
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(duration(secs * 1000))
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.selected != index {
                    this.selected = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.play(index, cx)))
    }
}

fn keys(tracks: &[TrackInfo]) -> Vec<String> {
    tracks.iter().map(|track| track.key.clone()).collect()
}

impl Render for QuickSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown: Vec<TrackInfo> = self.results.iter().take(SHOWN_RESULTS).cloned().collect();
        let empty = shown.is_empty() && self.settled && !self.query.is_empty();
        let rows: Vec<_> = shown
            .iter()
            .enumerate()
            .map(|(index, track)| self.row(index, track, cx).into_any_element())
            .collect();
        div()
            .id("quick-search-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .px_4()
            .pt(px(128.))
            .bg(cx.theme().overlay.opacity(0.6))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.move_selection(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.move_selection(true, cx);
                cx.stop_propagation();
            }))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
            .child(
                v_flex()
                    .id("quick-search-panel")
                    .w_full()
                    .max_w(px(576.))
                    .max_h(px(480.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .overflow_hidden()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                Input::new(&self.input)
                                    .appearance(false)
                                    .h(px(52.))
                                    .prefix(icons::icon(icons::SEARCH)),
                            ),
                    )
                    .when(!rows.is_empty(), |panel| {
                        panel.child(
                            v_flex()
                                .id("quick-search-results")
                                .py_2()
                                .min_h_0()
                                .overflow_y_scroll()
                                .children(rows),
                        )
                    })
                    .when(empty, |panel| {
                        panel.child(
                            div()
                                .px_5()
                                .py_6()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t_with(
                                    "no_results_found",
                                    &[("query", self.query.clone())],
                                )),
                        )
                    }),
            )
    }
}
