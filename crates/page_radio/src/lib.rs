//! Internet radio: the stations the daemon holds pinned, and the public directory it searches.

use std::collections::HashSet;
use std::time::Duration;

use api::{QueueContext, RadioStationInfo, Table};
use components::Loading;
use components::format::t;
use components::{
    Badge, BadgeTone, Chip, ChipSize, ChipTone, Cover, EmptyState, Heading, PageHeader,
    SearchField, SectionHeading, icons, motion, style,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ListAlignment, ListOffset, ListState, ParentElement as _, Pixels, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, list,
    px, rems,
};
use link::Link;
use store::{Notices, Query, actions};

// Typing settles before the directory is searched; the Dioxus page used the same delay.
const DEBOUNCE: Duration = Duration::from_millis(300);
const RESULTS: u32 = 60;
// `w-11 h-11` station art, and `rounded-md` on it; `Cover` only takes pixels.
const ART: Pixels = px(44.);
const CARD_HOVER: u64 = 0;
const CARD_WASH: u64 = 1;

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
    list: ListState,
    keys: Vec<SharedString>,
    lines: Vec<Line>,
    _debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Radio {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("radio_search_stations")));
        let stations = cx.new(|cx| {
            Query::new(
                &[Table::Stations],
                |api| async move { api.radio_stations().await },
                cx,
            )
        });
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
            cx.observe(&directory, |this, _, cx| this.relist(cx)),
        ];
        Self {
            filter,
            query: String::new(),
            stations,
            directory,
            pinned: HashSet::new(),
            expanded: HashSet::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            keys: Vec::new(),
            lines: Vec::new(),
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
        self.relist(cx);
    }

    fn adopt_pins(&mut self, cx: &mut Context<Self>) {
        if let Some(stations) = self.stations.read(cx).value() {
            self.pinned = stations
                .iter()
                .filter(|station| station.pinned)
                .map(|station| station.id.clone())
                .collect();
        }
        self.relist(cx);
    }

    /// Move the local set first: the answer is a round trip away and a check mark that waits reads as a dead button.
    fn toggle_pin(&mut self, id: String, cx: &mut Context<Self>) {
        let pinned = self.pinned.contains(&id);
        if pinned {
            self.pinned.remove(&id);
        } else {
            self.pinned.insert(id.clone());
        }
        self.relist(cx);
        let call = Link::global(cx).call({
            let id = id.clone();
            move |api| async move { api.pin_radio_station(id, !pinned).await }
        });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| match result {
                // The daemon announces the new list on the Stations table, which refetches it.
                Ok(()) => {}
                Err(error) => {
                    tracing::warn!(%error, "pinning a station failed");
                    if pinned {
                        this.pinned.insert(id);
                    } else {
                        this.pinned.remove(&id);
                    }
                    Notices::error(store::human(&error), cx);
                    this.relist(cx);
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

    /// `px-3 py-1 rounded-lg text-xs font-medium bg-white/10 text-slate-300`, inside a row that plays.
    fn chip(id: SharedString) -> Chip {
        Chip::new(id)
            .tone(ChipTone::Filter)
            .size(ChipSize::Medium)
            .stop_propagation(true)
    }

    fn stream_button(
        &self,
        station: &RadioStationInfo,
        stream_id: String,
        label: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let station_id = station.id.clone();
        Self::chip(SharedString::from(format!(
            "stream-{}-{stream_id}",
            station.id
        )))
        .label(label)
        .on_click(cx.listener(move |_, _, _, cx| {
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
                    Self::chip(SharedString::from(format!("less-{}", station.id)))
                        .icon(icons::CHEVRON_UP)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.expanded.remove(&id);
                            this.list.remeasure();
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
                    Self::chip(SharedString::from(format!("more-{}", station.id)))
                        .label(format!("+{}", count - 1))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.expanded.insert(id.clone());
                            this.list.remeasure();
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                buttons
            }
        }
    }

    fn tags(&self, station: &RadioStationInfo) -> Vec<AnyElement> {
        station
            .tags
            .iter()
            .take(2)
            .map(|tag| {
                Badge::new(tag.clone())
                    .tone(BadgeTone::Neutral)
                    .size(ChipSize::Medium)
                    .into_any_element()
            })
            .collect()
    }

    /// `p-4 rounded-xl border border-white/8 transition-colors hover:bg-white/10`.
    fn card(
        &self,
        id: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let (hovered, pointer) = motion::hovered(motion::channel(&id, CARD_HOVER), window, cx);
        let wash = motion::color(
            motion::channel(&id, CARD_WASH),
            style::white(if hovered { style::HOVER } else { 0. }, cx),
            window,
            cx,
        );
        div()
            .id(id)
            .w_full()
            .p_4()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(0.08, cx))
            .cursor_pointer()
            .bg(wash)
            .on_hover(motion::track(pointer))
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
                            .rounded(style::thumb_radius())
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
                            .children(self.tags(station)),
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
                .rounded(style::thumb_radius())
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
                .child(h_flex().gap_2().mt_2().children(self.tags(station))),
        )
        .child(
            Button::new(SharedString::from(format!("pin-{}", station.id)))
                .custom(style::quiet_button(pin_tint, cx))
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
    fn heading(heading: SectionHeading) -> impl IntoElement {
        div()
            .w_full()
            .pt_4()
            .pb_3()
            .child(heading.heading(Heading::Group))
    }

    /// What the list shows, in order: the pinned stations, then the directory or its state.
    fn lines(&self, cx: &App) -> (Vec<SharedString>, Vec<Line>) {
        let query = self.query.trim().to_lowercase();
        let mut keys = Vec::new();
        let mut lines = Vec::new();
        let pinned: Vec<RadioStationInfo> = self
            .stations
            .read(cx)
            .value()
            .map(|stations| {
                stations
                    .iter()
                    .filter(|station| self.pinned.contains(&station.id) && matches(station, &query))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !pinned.is_empty() {
            keys.push(SharedString::new_static("selected"));
            lines.push(Line::SelectedHeading);
            let last = pinned.len() - 1;
            for (at, station) in pinned.into_iter().enumerate() {
                keys.push(SharedString::from(format!("pinned-{}", station.id)));
                lines.push(Line::Pinned(station, at == last));
            }
        }
        keys.push(SharedString::new_static("directory"));
        lines.push(Line::DirectoryHeading);
        let directory = self.directory.read(cx);
        let status = if directory.is_loading() || !directory.is_settled() {
            Some(Status::Loading)
        } else if let Some(error) = directory.error() {
            Some(Status::Failed(SharedString::from(error.to_string())))
        } else if directory.value().is_none_or(|stations| stations.is_empty()) {
            Some(Status::Empty)
        } else {
            None
        };
        match status {
            Some(status) => {
                keys.push(SharedString::new_static("status"));
                lines.push(Line::Status(status));
            }
            None => {
                for station in directory.value().cloned().unwrap_or_default() {
                    keys.push(SharedString::from(format!("station-{}", station.id)));
                    lines.push(Line::Station(station));
                }
            }
        }
        (keys, lines)
    }

    /// Rebuild the lines from what changed, not per frame: render only reads them.
    fn relist(&mut self, cx: &mut Context<Self>) {
        let (keys, lines) = self.lines(cx);
        self.adopt_lines(keys, lines);
        cx.notify();
    }

    /// Point the list at new lines, keeping whichever line was at the top where it was.
    fn adopt_lines(&mut self, keys: Vec<SharedString>, lines: Vec<Line>) {
        self.lines = lines;
        if keys == self.keys {
            return;
        }
        let top = self.list.logical_scroll_top();
        let anchor = self.keys.get(top.item_ix).cloned();
        self.list.reset(keys.len());
        if let Some(item_ix) = anchor.and_then(|anchor| keys.iter().position(|key| *key == anchor))
        {
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: top.offset_in_item,
            });
        }
        self.keys = keys;
    }

    fn line(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(line) = self.lines.get(index).cloned() else {
            return div().into_any_element();
        };
        match line {
            Line::SelectedHeading => {
                Self::heading(SectionHeading::new(t("radio_selected"))).into_any_element()
            }
            Line::Pinned(station, last) => div()
                .w_full()
                .map(|slot| match last {
                    true => slot.pb_8(),
                    false => slot.pb_3(),
                })
                .child(self.pinned_row(&station, window, cx))
                .into_any_element(),
            Line::DirectoryHeading => {
                let searching = !self.query.trim().is_empty();
                let title = match searching {
                    true => t("radio_search_results"),
                    false => t("radio_top_stations"),
                };
                Self::heading(
                    SectionHeading::new(title).trailing(
                        div()
                            .text_size(style::text_10())
                            .text_color(style::white(0.25, cx))
                            .child(t("radio_powered_by")),
                    ),
                )
                .into_any_element()
            }
            Line::Status(Status::Loading) => Loading::new()
                .label(t("radio_loading_stations"))
                .into_any_element(),
            Line::Status(Status::Failed(error)) => EmptyState::failed(
                t("radio_search_failed"),
                cx.listener(|this, _, _, cx| {
                    this.directory.update(cx, |query, cx| query.refresh(cx));
                }),
            )
            .detail(error)
            .into_any_element(),
            Line::Status(Status::Empty) => EmptyState::new(t("radio_no_stations_match"))
                .icon(icons::RADIO)
                .into_any_element(),
            Line::Station(station) => div()
                .w_full()
                .pb_3()
                .child(self.directory_row(&station, window, cx))
                .into_any_element(),
        }
    }
}

/// What the directory says in place of stations.
#[derive(Clone)]
enum Status {
    Loading,
    Failed(SharedString),
    Empty,
}

/// One line of the page's list; only the ones on screen are ever built.
#[derive(Clone)]
enum Line {
    SelectedHeading,
    // The last pinned row carries the gap before the directory.
    Pinned(RadioStationInfo, bool),
    DirectoryHeading,
    Status(Status),
    Station(RadioStationInfo),
}

impl Render for Radio {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        components::page()
            .child(
                div().w_full().mb_8().child(
                    PageHeader::new(t("radio"))
                        .icon(icons::RADIO)
                        .subtitle(t("radio_subtitle"))
                        .flush()
                        .action(
                            div()
                                .w(search_width())
                                .flex_shrink_0()
                                .child(SearchField::new(&self.filter).cleanable(true)),
                        ),
                ),
            )
            .child(
                // Virtualized: a whole directory rebuilt on every scrolled frame is what made this page lag.
                list(
                    self.list.clone(),
                    cx.processor(|this, index: usize, window, cx| this.line(index, window, cx)),
                )
                .flex_1()
                .min_h_0()
                .w_full(),
            )
    }
}
