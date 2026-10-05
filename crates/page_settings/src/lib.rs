//! Settings: the category rail and the panel it shows; each category lives in its own crate.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, px, rems,
};
use settings_appearance::AppearanceSettings;
use settings_interface::InterfaceSettings;
use settings_playback::PlaybackSettings;
use settings_sources::SourceSettings;
use store::ConfigModel;
use widgets::focus::{Axis, Roving, Step};
use widgets::format::{t, t_with};
use widgets::{EmptyState, PageHeader, icons, marker, motion, style};

/// `border-white/9` under the fan, over `bg-black/92`.
const FAN_BORDER: f32 = 0.09;
const FAN_BG: f32 = 0.92;
/// `text-white/52` on a resting fan item, `bg-white/4` and `text-white/78` under the pointer.
const ITEM: f32 = 0.52;
const ITEM_HOVER: f32 = 0.04;
const ITEM_HOVER_TEXT: f32 = 0.78;
/// `bg-white/3` on the chosen one.
const ITEM_SELECTED: f32 = 0.03;
/// `text-white/40` on the lock, `text-white/70` on the line beside it.
const LOCK: f32 = 0.40;
const NOTICE: f32 = 0.70;

/// `prefix` over a category's slug: one stable tween channel per rail item.
fn channel(prefix: &'static str, slug: &'static str) -> ElementId {
    (ElementId::from(prefix), slug).into()
}

/// Which panel the rail is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    General,
    Appearance,
    Interface,
    Library,
    Connectivity,
    Offline,
    Metadata,
    Player,
    Tools,
}

impl Category {
    pub const ALL: &'static [Category] = &[
        Category::General,
        Category::Appearance,
        Category::Interface,
        Category::Library,
        Category::Connectivity,
        Category::Offline,
        Category::Metadata,
        Category::Player,
        Category::Tools,
    ];

    /// The `Route::Settings { section }` name, which is also how an anchor asks for a category.
    pub fn slug(self) -> &'static str {
        match self {
            Category::General => "general",
            Category::Appearance => "appearance",
            Category::Interface => "interface",
            Category::Library => "library",
            Category::Connectivity => "connectivity",
            Category::Offline => "offline",
            Category::Metadata => "metadata",
            Category::Player => "player",
            Category::Tools => "tools",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|category| category.slug() == slug)
    }

    fn label(self) -> SharedString {
        t(match self {
            Category::General => "settings_general",
            Category::Appearance => "settings_customization",
            Category::Interface => "settings_interface",
            Category::Library => "settings_library",
            Category::Connectivity => "settings_connectivity",
            Category::Offline => "settings_offline",
            Category::Metadata => "settings_metadata",
            Category::Player => "settings_player",
            Category::Tools => "settings_tools",
        })
    }

    fn icon(self) -> &'static str {
        match self {
            Category::General => icons::SETTINGS,
            Category::Appearance => icons::GRID,
            Category::Interface => icons::ROWS,
            Category::Library => icons::LIBRARY,
            Category::Connectivity => icons::SPEAKER,
            Category::Offline => icons::DOWNLOAD,
            Category::Metadata => icons::EDIT,
            Category::Player => icons::PLAY,
            Category::Tools => icons::WARNING,
        }
    }
}

pub struct Settings {
    category: Category,
    rail: Roving,
    // A ring belongs to the keyboard; a click already says where it landed.
    ringing: bool,
    hovered: Option<Category>,
    scroll: gpui_kit::ScrollHandle,
    search: Entity<InputState>,
    searching: bool,
    appearance: Entity<AppearanceSettings>,
    interface: Entity<InterfaceSettings>,
    sources: Entity<SourceSettings>,
    playback: Entity<PlaybackSettings>,
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    pub fn new(section: Option<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let category = section
            .and_then(|section| Category::from_slug(&section))
            .unwrap_or(Category::General);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(t("settings_search")));
        // The box is new, so whatever a previous visit was filtering by is not.
        widgets::settings::set_query("", cx);
        let subscriptions = vec![
            cx.subscribe(&search, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let typed = state.read(cx).value().to_string();
                    this.filter(&typed, cx);
                }
            }),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
        ];
        let mut rail = Roving::new(Axis::Row, cx);
        rail.set_len(Category::ALL.len());
        rail.set_cursor(
            Category::ALL
                .iter()
                .position(|held| *held == category)
                .unwrap_or(0),
        );
        let mut settings = Self {
            category,
            rail,
            ringing: false,
            hovered: None,
            scroll: gpui_kit::ScrollHandle::new(),
            search,
            searching: false,
            appearance: cx.new(|cx| AppearanceSettings::new(window, cx)),
            interface: cx.new(|cx| InterfaceSettings::new(window, cx)),
            sources: cx.new(|cx| SourceSettings::new(window, cx)),
            playback: cx.new(|cx| PlaybackSettings::new(window, cx)),
            _subscriptions: subscriptions,
        };
        settings.select(category, cx);
        settings
    }

    /// Filtering spans every category, so the panels stop showing one section each while it lasts.
    fn filter(&mut self, typed: &str, cx: &mut Context<Self>) {
        widgets::settings::set_query(typed, cx);
        let searching = widgets::settings::query(cx).is_some();
        if searching == self.searching {
            cx.notify();
            return;
        }
        self.searching = searching;
        if searching {
            self.appearance.update(cx, |panel, cx| panel.show(None, cx));
            self.sources.update(cx, |panel, cx| panel.show(None, cx));
            self.playback.update(cx, |panel, cx| panel.show(None, cx));
            cx.notify();
        } else {
            self.select(self.category, cx);
        }
    }

    /// Show one category, telling its crate which of its sections to draw.
    fn select(&mut self, category: Category, cx: &mut Context<Self>) {
        self.category = category;
        let section = Some(SharedString::from(category.slug()));
        match category {
            Category::General | Category::Appearance => self
                .appearance
                .update(cx, |panel, cx| panel.show(section, cx)),
            // The Interface panel draws whatever registered itself; it has no sections to pick.
            Category::Interface => {}
            Category::Library | Category::Connectivity => {
                self.sources.update(cx, |panel, cx| panel.show(section, cx))
            }
            _ => self
                .playback
                .update(cx, |panel, cx| panel.show(section, cx)),
        }
        cx.notify();
    }

    /// A category rail entry was clicked; a search that was on is what it replaces.
    fn choose(&mut self, category: Category, window: &mut Window, cx: &mut Context<Self>) {
        if self.searching {
            self.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.searching = false;
            widgets::settings::set_query("", cx);
        }
        self.select(category, cx);
    }

    fn panel(&self) -> AnyElement {
        if self.searching {
            return v_flex()
                .w_full()
                .gap_4()
                .child(self.appearance.clone())
                .child(self.interface.clone())
                .child(self.sources.clone())
                .child(self.playback.clone())
                .into_any_element();
        }
        match self.category {
            Category::Appearance | Category::General => self.appearance.clone().into_any_element(),
            Category::Interface => self.interface.clone().into_any_element(),
            Category::Library | Category::Connectivity => self.sources.clone().into_any_element(),
            Category::Offline | Category::Metadata | Category::Player | Category::Tools => {
                self.playback.clone().into_any_element()
            }
        }
    }
}

impl Settings {
    /// The category fan, as its top-bar branch: the CD disc is rotated clip-paths GPUI cannot draw.
    fn rail(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let underline = cx.theme().primary_hover;
        let items: Vec<_> = Category::ALL
            .iter()
            .copied()
            .enumerate()
            .map(|(index, category)| {
                let selected = category == self.category;
                let hovered = self.hovered == Some(category);
                let surface = style::white(
                    match (selected, hovered) {
                        (true, _) => ITEM_SELECTED,
                        (false, true) => ITEM_HOVER,
                        (false, false) => 0.,
                    },
                    cx,
                );
                let ink = style::white(
                    match (selected, hovered) {
                        (true, _) => 1.,
                        (false, true) => ITEM_HOVER_TEXT,
                        (false, false) => ITEM,
                    },
                    cx,
                );
                (
                    index,
                    category,
                    self.rail.is_active(index, window) && self.ringing,
                    motion::color(channel("fan-bg", category.slug()), surface, window, cx),
                    motion::color(channel("fan-ink", category.slug()), ink, window, cx),
                )
            })
            .collect();
        // The mark travels to the chosen tab, so it has to know where the tabs actually are.
        let marks = marker::marks("settings-fan-marks", window, cx);
        let (start, width) =
            marker::slide("settings-fan", &marks, self.category.slug(), window, cx);
        // The mark rides in the flow beside the tabs rather than absolutely over them: an
        // absolute child does not scroll with the rail, so it drifted by the scroll offset.
        let mark = h_flex()
            .flex_none()
            .h(px(2.))
            .mt(px(-2.))
            .child(div().flex_none().w(start))
            .child(div().flex_none().w(width).h_full().bg(underline));
        let track = h_flex().h_full().flex_none().gap_0p5();
        self.rail
            .group(h_flex().id("settings-fan"), "SettingsRail")
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| this.walk(event, window, cx)),
            )
            .w_full()
            .flex_none()
            .mb_4()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .border_b_1()
            .border_color(style::white(FAN_BORDER, cx))
            .bg(widgets::settings::panel_background(FAN_BG, cx))
            .child(
                v_flex()
                    .flex_none()
                    .h_full()
                    // A column centres what it holds, which would push the narrower mark right.
                    .items_start()
                    .child(marker::origin(marker::Along::Row, marks.clone()))
                    .child(track.children(items.into_iter().map(
                        |(index, category, ringed, surface, ink)| {
                            let marks = marks.clone();
                            h_flex()
                                .id(category.slug())
                                .relative()
                                .flex_none()
                                .h_11()
                                .items_center()
                                .bg(surface)
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(ink)
                                .cursor_pointer()
                                .child(marker::probe(category.slug(), marker::Along::Row, marks))
                                .child(
                                    h_flex()
                                        .h_full()
                                        .px_3()
                                        .gap_2()
                                        .items_center()
                                        .child(icons::icon(category.icon()))
                                        .child(category.label()),
                                )
                                .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                                    let held = over.then_some(category);
                                    if this.hovered != held {
                                        this.hovered = held;
                                        cx.notify();
                                    }
                                }))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.rail.set_cursor(index);
                                    this.ringing = false;
                                    this.choose(category, window, cx);
                                }))
                                .when(ringed, |item| {
                                    item.child(widgets::focus::ring(style::radius_md(), cx))
                                })
                        },
                    )))
                    .child(mark),
            )
    }

    /// Left and right walk the fan; enter shows whatever the walk landed on.
    fn walk(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.ringing = true;
        match self.rail.walk(event) {
            Step::Moved(_) => cx.notify(),
            Step::Activate(at) => {
                if let Some(category) = Category::ALL.get(at).copied() {
                    self.choose(category, window, cx);
                }
            }
            Step::Ignored => cx.propagate(),
        }
    }

    /// The box that filters every category at once.
    fn search_box(&self, cx: &App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .mb_4()
            .px_3()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .bg(style::white(style::RAISED, cx))
            .child(
                icons::icon(icons::SEARCH)
                    .with_size(style::text_11())
                    .text_color(style::white(style::TEXT_MUTED, cx)),
            )
            .child(div().flex_1().min_w_0().child(Input::new(&self.search)))
    }

    /// Nothing to edit until the daemon has answered; dead controls would save nothing.
    fn unavailable(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .w_full()
            .py_8()
            .child(EmptyState::new(t("settings_unavailable")).icon(icons::WARNING))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_xs()
                    .text_color(style::white(NOTICE, cx))
                    .child(t("settings_unavailable_hint")),
            )
    }

    /// The notice a managed settings file puts above the fan.
    fn managed_notice(&self, cx: &App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .mb_4()
            .px_5()
            .py_3()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .bg(style::white(style::RAISED, cx))
            .child(
                icons::icon(icons::LOCK)
                    .with_size(style::text_11())
                    .text_color(style::white(LOCK, cx)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(style::white(NOTICE, cx))
                    .child(t("settings_managed_notice")),
            )
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth("settings-panel", &self.scroll, window, cx);
        let config = ConfigModel::global(cx);
        let loaded = config.read(cx).is_loaded();
        let locked = !config.read(cx).locked_keys().is_empty();
        let empty = self.searching && widgets::settings::nothing_matched(cx);
        let typed = SharedString::from(self.search.read(cx).value().to_string());
        // The page root is the scroller: a nested `flex_1 min_h_0` child measured no height here.
        v_flex()
            .id("settings-panel")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                v_flex()
                    .w_full()
                    .max_w(rems(80.))
                    .mx_auto()
                    .px_6()
                    .py_7()
                    .child(PageHeader::new(t("settings")))
                    .map(|page| {
                        if !loaded {
                            return page.child(self.unavailable(cx));
                        }
                        page.child(self.search_box(cx))
                            .when(locked, |page| page.child(self.managed_notice(cx)))
                            .child(self.rail(window, cx))
                            .when(empty, |page| {
                                page.child(EmptyState::new(t_with(
                                    "no_results_found",
                                    &[("query", typed.to_string())],
                                )))
                            })
                            .child(self.panel())
                    }),
            )
    }
}
