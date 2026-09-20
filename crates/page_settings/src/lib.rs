//! Settings: the category rail and the panel it shows; each category lives in its own crate.

use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, rems,
};
use settings_appearance::AppearanceSettings;
use settings_playback::PlaybackSettings;
use settings_sources::SourceSettings;
use store::ConfigModel;
use widgets::format::t;
use widgets::{PageHeader, icons, motion, style};

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
    hovered: Option<Category>,
    appearance: Entity<AppearanceSettings>,
    sources: Entity<SourceSettings>,
    playback: Entity<PlaybackSettings>,
}

impl Settings {
    pub fn new(section: Option<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let category = section
            .and_then(|section| Category::from_slug(&section))
            .unwrap_or(Category::General);
        let mut settings = Self {
            category,
            hovered: None,
            appearance: cx.new(|cx| AppearanceSettings::new(window, cx)),
            sources: cx.new(|cx| SourceSettings::new(window, cx)),
            playback: cx.new(|cx| PlaybackSettings::new(window, cx)),
        };
        settings.select(category, cx);
        settings
    }

    /// Show one category, telling its crate which of its sections to draw.
    fn select(&mut self, category: Category, cx: &mut Context<Self>) {
        self.category = category;
        let section = Some(SharedString::from(category.slug()));
        match category {
            Category::General | Category::Appearance => self
                .appearance
                .update(cx, |panel, cx| panel.show(section, cx)),
            Category::Library | Category::Connectivity => {
                self.sources.update(cx, |panel, cx| panel.show(section, cx))
            }
            _ => self
                .playback
                .update(cx, |panel, cx| panel.show(section, cx)),
        }
        cx.notify();
    }

    fn panel(&self) -> AnyElement {
        match self.category {
            Category::Appearance | Category::General => self.appearance.clone().into_any_element(),
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
            .map(|category| {
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
                let mark = underline.opacity(if selected { 1. } else { 0. });
                (
                    category,
                    motion::color(channel("fan-bg", category.slug()), surface, window, cx),
                    motion::color(channel("fan-ink", category.slug()), ink, window, cx),
                    motion::color(channel("fan-mark", category.slug()), mark, window, cx),
                )
            })
            .collect();
        h_flex()
            .id("settings-fan")
            .w_full()
            .flex_none()
            .mb_4()
            .gap_0p5()
            .overflow_x_scroll()
            .border_b_1()
            .border_color(style::white(FAN_BORDER, cx))
            .bg(style::black(FAN_BG, cx))
            .children(items.into_iter().map(|(category, surface, ink, mark)| {
                h_flex()
                    .id(category.slug())
                    .flex_none()
                    .h_11()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .border_b_2()
                    .border_color(mark)
                    .bg(surface)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(ink)
                    .cursor_pointer()
                    .child(icons::icon(category.icon()))
                    .child(category.label())
                    .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                        let held = over.then_some(category);
                        if this.hovered != held {
                            this.hovered = held;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select(category, cx);
                    }))
            }))
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
        let locked = !ConfigModel::global(cx).read(cx).locked_keys().is_empty();
        v_flex()
            .size_full()
            .max_w(rems(80.))
            .mx_auto()
            .px_6()
            .py_7()
            .child(PageHeader::new(t("settings")))
            .when(locked, |page| page.child(self.managed_notice(cx)))
            .child(self.rail(window, cx))
            // min_h_0 is what lets the panel shrink, and so scroll, inside this column.
            .child(div().flex_1().min_h_0().min_w_0().child(self.panel()))
    }
}
