//! Internet radio: the stations the daemon holds pinned, and the public directory it searches.

use std::collections::HashSet;
use std::time::Duration;

use api::{QueueContext, RadioStationInfo};
use gpui_kit::component::button::{
    Button, ButtonCustomVariant, ButtonRounded, ButtonVariants as _,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, px, rems,
};
use link::Link;
use store::{Notices, Query, actions};
use widgets::format::t;
use widgets::{Cover, EmptyState, icons, motion, style};

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
    // The card under the pointer, which its wash tweens toward.
    hovered_card: Option<SharedString>,
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
            hovered_card: None,
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
                    Notices::error(store::human(&error), cx);
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
                    .color(style::rest(style::white(style::HOVER, cx)))
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

    fn set_card_hover(&mut self, id: SharedString, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.hovered_card = Some(id);
        } else if self.hovered_card.as_ref() == Some(&id) {
            self.hovered_card = None;
        }
        cx.notify();
    }

    /// `p-4 rounded-xl border border-white/8 transition-colors hover:bg-white/10`.
    fn card(
        &self,
        id: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let hovered = self.hovered_card.as_ref() == Some(&id);
        let wash = motion::color(
            SharedString::from(format!("{id}-wash")),
            if hovered {
                style::white(style::HOVER, cx)
            } else {
                style::white(0., cx)
            },
            window,
            cx,
        );
        div()
            .id(id.clone())
            .w_full()
            .p_4()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(0.08, cx))
            .cursor_pointer()
            .bg(wash)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_card_hover(id.clone(), *hovered, cx)
            }))
    }

    /// A row of the selected table: the whole row plays, the buttons pick a stream.
    fn pinned_row(
        &self,
        station: &RadioStationInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded.contains(&station.id);
        let station_id = station.id.clone();
        let stream_id = first_stream(station);
        let muted = style::slate_400(cx);
        self.card(SharedString::from(station.id.clone()), window, cx)
            .flex()
            .flex_col()
            .on_click(
                cx.listener(move |_, _, _, cx| play(station_id.clone(), stream_id.clone(), cx)),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_4()
                    .child(
                        Cover::new(station.artwork.as_ref())
                            .size(ART)
                            .rounded(ART_RADIUS)
                            .placeholder(icons::RADIO),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t(&station.name)),
                    )
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
                                    .text_xs()
                                    .text_color(muted)
                                    .child(t(&station.description)),
                            )
                            .children(self.tags(station, cx)),
                    )
                    .child(
                        h_flex()
                            .w(streams_width())
                            .flex_shrink_0()
                            .gap_2()
                            .justify_end()
                            .children(self.streams(station, cx)),
                    ),
            )
            .when(expanded, |row| {
                row.child(h_flex().w_full().flex_wrap().gap_2().mt_2p5().children(
                    station.streams.iter().map(|stream| {
                        self.stream_button(station, stream.id.clone(), t(&stream.name), cx)
                    }),
                ))
            })
            .into_any_element()
    }

    /// A directory row: play it, or pin it into the selected table.
    fn directory_row(
        &self,
        station: &RadioStationInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pinned = self.pinned.contains(&station.id);
        let station_id = station.id.clone();
        let stream_id = first_stream(station);
        let toggle_id = station.id.clone();
        let muted = style::slate_400(cx);
        // The pin's answer is a round trip away, so the check mark lights as it arrives.
        let pin_tint = motion::color(
            SharedString::from(format!("pin-tint-{}", station.id)),
            if pinned {
                cx.theme().success
            } else {
                style::slate_400(cx)
            },
            window,
            cx,
        );
        self.card(
            SharedString::from(format!("directory-{}", station.id)),
            window,
            cx,
        )
        .flex()
        .items_start()
        .gap_4()
        .on_click(cx.listener(move |_, _, _, cx| play(station_id.clone(), stream_id.clone(), cx)))
        .child(
            Cover::new(station.artwork.as_ref())
                .size(ART)
                .rounded(ART_RADIUS)
                .placeholder(icons::RADIO),
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t(&station.name)),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(t(&station.description)),
                )
                .child(h_flex().gap_2().mt_2().children(self.tags(station, cx))),
        )
        .child(
            Button::new(SharedString::from(format!("pin-{}", station.id)))
                .custom(
                    ButtonCustomVariant::new(cx)
                        .foreground(pin_tint)
                        .hover(style::white(style::HOVER, cx))
                        .active(style::white(style::ACTIVE, cx)),
                )
                .rounded(px(999.))
                .with_size(px(36.))
                .icon(icons::icon(if pinned { icons::CHECK } else { icons::PLUS }).xsmall())
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_pin(toggle_id.clone(), cx);
                })),
        )
        .child(self.play_button(station, cx))
        .into_any_element()
    }

    /// `text-sm font-bold uppercase tracking-wider text-slate-400`, less the two GPUI cannot draw.
    fn heading(&self, label: SharedString, cx: &Context<Self>) -> impl IntoElement {
        div()
            .pt_4()
            .pb_3()
            .text_sm()
            .font_weight(FontWeight::BOLD)
            .text_color(style::slate_400(cx))
            .child(label)
    }

    fn selected(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
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
        Some(
            v_flex()
                .w_full()
                .child(self.heading(t("radio_selected"), cx))
                .child(
                    v_flex().w_full().gap_3().mb_8().children(
                        stations
                            .iter()
                            .map(|station| self.pinned_row(station, window, cx))
                            .collect::<Vec<_>>(),
                    ),
                )
                .into_any_element(),
        )
    }

    fn browser(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.directory.read(cx);
        let loading = query.is_loading() || !query.is_settled();
        let error = query.error().map(|error| error.to_string());
        let results = query.value().cloned().unwrap_or_default();
        if loading {
            return h_flex()
                .w_full()
                .justify_center()
                .items_center()
                .gap_3()
                .py_16()
                .text_sm()
                .text_color(style::white(style::TEXT_DIM, cx))
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
            .gap_3()
            .children(
                results
                    .iter()
                    .map(|station| self.directory_row(station, window, cx))
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    }
}

impl Render for Radio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth("radio-scroll", &self.scroll, window, cx);
        let searching = !self.query.trim().is_empty();
        let selected = self.selected(window, cx);
        let browser = self.browser(window, cx);
        v_flex()
            .size_full()
            .px_8()
            .pt_8()
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .flex_wrap()
                    .gap_4()
                    .mb_8()
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .mb_2()
                                    .child(
                                        icons::icon(icons::RADIO)
                                            .size(px(24.))
                                            .text_color(cx.theme().primary_hover),
                                    )
                                    .child(
                                        div()
                                            .text_3xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(t("radio")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(style::slate_400(cx))
                                    .child(t("radio_subtitle")),
                            ),
                    )
                    .child(div().w_full().max_w(search_width()).flex_shrink_0().child(
                        Input::new(&self.filter).prefix(icons::icon(icons::SEARCH).small()),
                    )),
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
                                            .pb_3()
                                            .text_size(style::text_10())
                                            .text_color(style::white(0.25, cx))
                                            .child(t("radio_powered_by")),
                                    ),
                            )
                            .child(browser),
                    ),
            )
    }
}
