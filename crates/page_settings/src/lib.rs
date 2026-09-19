//! Settings: the category rail and the panel it shows; each category lives in its own crate.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled, Window, div, px,
};
use settings_appearance::AppearanceSettings;
use settings_playback::PlaybackSettings;
use settings_sources::SourceSettings;
use store::ConfigModel;
use widgets::format::t;
use widgets::{PageHeader, icons};

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

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = !ConfigModel::global(cx).read(cx).locked_keys().is_empty();
        v_flex()
            .size_full()
            .p_4()
            .child(PageHeader::new(t("settings")))
            .when(locked, |page| {
                page.child(
                    div()
                        .mb_2()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().muted)
                        .text_sm()
                        .child(t("settings_managed_notice")),
                )
            })
            .child(
                h_flex()
                    .size_full()
                    .gap_4()
                    .items_start()
                    .child(v_flex().w(px(200.)).gap_1().children(
                        Category::ALL.iter().copied().map(|category| {
                            Button::new(category.slug())
                                .ghost()
                                .w_full()
                                .justify_start()
                                .icon(widgets::icons::icon(category.icon()))
                                .label(category.label())
                                .selected(category == self.category)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select(category, cx);
                                }))
                        }),
                    ))
                    .child(div().flex_1().min_w_0().child(self.panel())),
            )
    }
}
