//! Internet radio: the stations the daemon holds pinned, and the public directory it searches.

use std::collections::HashSet;
use std::time::Duration;

use api::{QueueContext, RadioStationInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, px,
};
use link::Link;
use store::{Notices, Query, actions};
use widgets::format::t;
use widgets::{Cover, EmptyState, PageHeader, icons};

// Typing settles before the directory is searched; the Dioxus page used the same delay.
const DEBOUNCE: Duration = Duration::from_millis(300);
const RESULTS: u32 = 60;
// `w-11 h-11` station art, and `rounded-md` on it; `Cover` only takes pixels.
const ART: Pixels = px(44.);
const ART_RADIUS: Pixels = px(6.);

// The width the streams column reserves on the right of a selected row.
fn streams_width() -> gpui_kit::Rems {
    rems(13.75)
}

// `max-w-sm w-full`: the directory search box.
fn search_width() -> gpui_kit::Rems {
    rems(24.)
}

/// The stream a click plays when the row does not name one.
fn first_stream(station: &RadioStationInfo) -> String {
    station
        .streams
        .first()
        .map(|stream| stream.id.clone())
        .unwrap_or_default()
}

/// Registry text may be a translation key, so the filter matches what is on screen.
fn matches(station: &RadioStationInfo, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    t(&station.name).to_lowercase().contains(query)
        || t(&station.description).to_lowercase().contains(query)
        || station
            .streams
            .iter()
            .any(|stream| t(&stream.name).to_lowercase().contains(query))
        || station
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
}

fn play(station_id: String, stream_id: String, cx: &mut gpui_kit::App) {
    actions::play_context(
        QueueContext::Radio {
            station_id,
            stream_id,
        },
        Some(0),
        cx,
    );
}

pub struct Radio {
    filter: Entity<InputState>,
    query: String,
    stations: Entity<Query<Vec<RadioStationInfo>>>,
    directory: Entity<Query<Vec<RadioStationInfo>>>,
    pinned: HashSet<String>,
    expanded: HashSet<String>,
    scroll: ScrollHandle,
    _debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Radio {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("radio_search_stations")));
        let stations =
            cx.new(|cx| Query::new(&[], |api| async move { api.radio_stations().await }, cx));
        let directory = cx.new(|cx| {
            Query::new(
                &[],
                |api| async move { api.search_radio(String::new(), RESULTS).await },
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe(&filter, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.debounce(value, cx);
                }
            }),
            cx.observe(&stations, |this, _, cx| this.adopt_pins(cx)),
            cx.observe(&directory, |_, _, cx| cx.notify()),
        ];
        Self {
            filter,
            query: String::new(),
            stations,
            directory,
            pinned: HashSet::new(),
            expanded: HashSet::new(),
            scroll: ScrollHandle::new(),
            _debounce: None,
            _subscriptions: subscriptions,
        }
    }

    /// A newer keystroke replaces the pending task, which cancels it.
    fn debounce(&mut self, value: String, cx: &mut Context<Self>) {
        self._debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            this.update(cx, |this, cx| this.apply(value, cx)).ok();
        }));
    }

    fn apply(&mut self, value: String, cx: &mut Context<Self>) {
        if self.query == value {
            return;
        }
        self.query = value;
        let search = self.query.trim().to_string();
        self.directory.update(cx, |query, cx| {
            query.set_fetch(
                move |api| {
                    let search = search.clone();
                    async move { api.search_radio(search, RESULTS).await }
                },
                cx,
            )
        });
        cx.notify();
    }

    fn adopt_pins(&mut self, cx: &mut Context<Self>) {
        if let Some(stations) = self.stations.read(cx).value() {
            self.pinned = stations
                .iter()
                .filter(|station| station.pinned)
                .map(|station| station.id.clone())
                .collect();
        }
        cx.notify();
    }

    /// Move the local set first: the answer is a round trip away and a check mark that waits reads as a dead button.
    fn toggle_pin(&mut self, id: String, cx: &mut Context<Self>) {
        let pinned = self.pinned.contains(&id);
        if pinned {
            self.pinned.remove(&id);
        } else {
            self.pinned.insert(id.clone());
        }
        cx.notify();
        let call = Link::global(cx).call({
            let id = id.clone();
            move |api| async move { api.pin_radio_station(id, !pinned).await }
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.stations.update(cx, |query, cx| query.refresh(cx)),
                Err(error) => {
                    tracing::warn!(%error, "pinning a station failed");
                    if pinned {
                        this.pinned.insert(id);
                    } else {
                        this.pinned.remove(&id);
                    }
                    Notices::error(error.to_string(), cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn play_button(&self, station: &RadioStationInfo, cx: &mut Context<Self>) -> AnyElement {
        let station_id = station.id.clone();
        let stream_id = first_stream(station);
        Button::new(SharedString::from(format!("play-{}", station.id)))
            .ghost()
            .icon(icons::icon(icons::PLAY))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                play(station_id.clone(), stream_id.clone(), cx);
            }))
            .into_any_element()
    }

    /// `px-3 py-1 rounded-lg text-xs font-medium bg-white/10 text-slate-300`.
    fn chip(id: SharedString, cx: &App) -> Button {
        Button::new(id)
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(style::white(style::HOVER, cx))
                    .foreground(style::slate_400(cx))
                    .hover(style::white(style::ACTIVE, cx))
                    .active(style::white(style::ACTIVE, cx)),
            )
            .rounded(ButtonRounded::Large)
            .xsmall()
    }

    fn stream_button(
        &self,
        station: &RadioStationInfo,
        stream_id: String,
        label: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let station_id = station.id.clone();
        Self::chip(
            SharedString::from(format!("stream-{}-{stream_id}", station.id)),
            cx,
        )
        .label(label)
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.stop_propagation();
            play(station_id.clone(), stream_id.clone(), cx);
        }))
        .into_any_element()
    }

    /// One button per stream while there are few, the first plus a "+N" expander past that.
    fn streams(&self, station: &RadioStationInfo, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let expanded = self.expanded.contains(&station.id);
        match station.streams.len() {
            0 | 1 => vec![self.play_button(station, cx)],
            2 => station
                .streams
                .iter()
                .map(|stream| self.stream_button(station, stream.id.clone(), t(&stream.name), cx))
                .collect(),
            _ if expanded => {
                let id = station.id.clone();
                vec![
                    Self::chip(SharedString::from(format!("less-{}", station.id)), cx)
                        .icon(icons::icon(icons::CHEVRON_UP).xsmall())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.expanded.remove(&id);
                            cx.notify();
                        }))
                        .into_any_element(),
                ]
            }
            count => {
                let id = station.id.clone();
                let mut buttons = Vec::new();
                if let Some(first) = station.streams.first() {
                    buttons.push(self.stream_button(station, first.id.clone(), t(&first.name), cx));
                }
                buttons.push(
                    Self::chip(SharedString::from(format!("more-{}", station.id)), cx)
                        .label(format!("+{}", count - 1))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.expanded.insert(id.clone());
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                buttons
            }
        }
    }

    fn tags(&self, station: &RadioStationInfo, cx: &Context<Self>) -> Vec<AnyElement> {
        station
            .tags
            .iter()
            .take(2)
            .map(|tag| {
                div()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .rounded(style::radius_lg())
                    .bg(style::white(style::RAISED, cx))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(style::slate_500(cx))
                    .child(SharedString::from(tag.clone()))
                    .into_any_element()
            })
            .collect()
    }

    /// `p-4 rounded-xl border border-white/8 hover:bg-white/10`: the shape both lists' rows take.
    fn card(id: SharedString, cx: &Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .w_full()
            .p_4()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(0.08, cx))
            .cursor_pointer()
            .hover(|row| row.bg(style::white(style::HOVER, cx)))
    }

    /// A row of the selected table: the whole row plays, the buttons pick a stream.
    fn pinned_row(&self, station: &RadioStationInfo, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.expanded.contains(&station.id);
        let station_id = station.id.clone();
        let stream_id = first_stream(station);
        let muted = cx.theme().muted_foreground;
        v_flex()
            .id(SharedString::from(station.id.clone()))
            .w_full()
            .rounded(cx.theme().radius)
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(
                cx.listener(move |_, _, _, cx| play(station_id.clone(), stream_id.clone(), cx)),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .child(
                        Cover::new(station.artwork.as_ref())
                            .size(ART)
                            .placeholder(icons::RADIO),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(t(&station.name)))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(t(&station.description)),
                            )
                            .children(self.tags(station, cx)),
                    )
                    .child(
                        h_flex()
                            .w(STREAMS_WIDTH)
                            .flex_shrink_0()
                            .gap_2()
                            .justify_end()
                            .children(self.streams(station, cx)),
                    ),
            )
            .when(expanded, |row| {
                row.child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_2()
                        .px_3()
                        .pb_2()
                        .children(station.streams.iter().map(|stream| {
                            self.stream_button(station, stream.id.clone(), t(&stream.name), cx)
                        })),
                )
            })
            .into_any_element()
    }

    /// A directory row: play it, or pin it into the selected table.
    fn directory_row(&self, station: &RadioStationInfo, cx: &mut Context<Self>) -> AnyElement {
        let pinned = self.pinned.contains(&station.id);
        let station_id = station.id.clone();
        let stream_id = first_stream(station);
        let toggle_id = station.id.clone();
        let muted = cx.theme().muted_foreground;
        h_flex()
            .id(SharedString::from(format!("directory-{}", station.id)))
            .w_full()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(
                cx.listener(move |_, _, _, cx| play(station_id.clone(), stream_id.clone(), cx)),
            )
            .child(
                Cover::new(station.artwork.as_ref())
                    .size(ART)
                    .placeholder(icons::RADIO),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(t(&station.name)))
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(muted)
                            .child(t(&station.description)),
                    ),
            )
            .children(self.tags(station, cx))
            .child(
                Button::new(SharedString::from(format!("pin-{}", station.id)))
                    .ghost()
                    .icon(icons::icon(if pinned { icons::CHECK } else { icons::PLUS }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_pin(toggle_id.clone(), cx);
                    })),
            )
            .child(self.play_button(station, cx))
            .into_any_element()
    }

    fn heading(&self, label: SharedString, cx: &Context<Self>) -> impl IntoElement {
        div()
            .pt_4()
            .pb_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    fn selected(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let query = self.query.trim().to_lowercase();
        let stations: Vec<RadioStationInfo> = self
            .stations
            .read(cx)
            .value()?
            .iter()
            .filter(|station| self.pinned.contains(&station.id) && matches(station, &query))
            .cloned()
            .collect();
        if stations.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        Some(
            v_flex()
                .w_full()
                .child(self.heading(t("radio_selected"), cx))
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .pb_1()
                        .text_xs()
                        .text_color(muted)
                        .child(div().w(ART).flex_shrink_0())
                        .child(div().flex_1().min_w_0().child(t("radio_station_col")))
                        .child(div().flex_1().min_w_0().child(t("radio_description_col")))
                        .child(
                            div()
                                .w(STREAMS_WIDTH)
                                .flex_shrink_0()
                                .text_right()
                                .child(t("radio_streams_col")),
                        ),
                )
                .children(
                    stations
                        .iter()
                        .map(|station| self.pinned_row(station, cx))
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
        )
    }

    fn browser(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.directory.read(cx);
        let loading = query.is_loading() || !query.is_settled();
        let error = query.error().map(|error| error.to_string());
        let results = query.value().cloned().unwrap_or_default();
        let muted = cx.theme().muted_foreground;
        if loading {
            return h_flex()
                .w_full()
                .justify_center()
                .items_center()
                .gap_2()
                .py_8()
                .text_sm()
                .text_color(muted)
                .child(Spinner::new())
                .child(t("radio_loading_stations"))
                .into_any_element();
        }
        if let Some(error) = error {
            return EmptyState::new(t("radio_search_failed"))
                .detail(error)
                .icon(icons::RADIO)
                .into_any_element();
        }
        if results.is_empty() {
            return EmptyState::new(t("radio_no_stations_match"))
                .icon(icons::RADIO)
                .into_any_element();
        }
        v_flex()
            .w_full()
            .children(
                results
                    .iter()
                    .map(|station| self.directory_row(station, cx))
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    }
}

impl Render for Radio {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.query.trim().is_empty();
        let selected = self.selected(cx);
        let browser = self.browser(cx);
        let muted = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .p_4()
            .child(
                PageHeader::new(t("radio"))
                    .subtitle(t("radio_subtitle"))
                    .action(div().w(px(260.)).child(Input::new(&self.filter))),
            )
            .child(
                div()
                    .id("radio-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        v_flex()
                            .w_full()
                            .children(selected)
                            .child(
                                h_flex()
                                    .w_full()
                                    .items_end()
                                    .justify_between()
                                    .child(self.heading(
                                        if searching {
                                            t("radio_search_results")
                                        } else {
                                            t("radio_top_stations")
                                        },
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(t("radio_powered_by")),
                                    ),
                            )
                            .child(browser),
                    ),
            )
    }
}
