//! The header controls the two artist pages share: view mode, stacked sort, and Tracks⇄Albums.

use config::{
    AlbumSortField, AlbumViewMode, AppConfig, ArtistSortField, ArtistViewOrder, SortCriterion,
    SortDirection,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex};
use gpui_kit::{
    App, Div, ElementId, Entity, IntoElement, ParentElement as _, Pixels, SharedString,
    Styled as _, Window, px,
};
use store::ConfigModel;
use widgets::format::t;
use widgets::{icons, motion, style};

/// `text-white/40`: the resting label of a segmented pill's unselected half.
const SEGMENT_IDLE: f32 = 0.40;
/// `rounded-md`, which the shared radius helpers do not carry.
const SEGMENT_RADIUS: Pixels = px(6.);

/// Hover as a value a tween can read; `.hover()` refines a style instead of reporting the state.
pub(crate) fn hovered(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> (bool, Entity<bool>) {
    let state = window.use_keyed_state(id, cx, |_, _| false);
    let hovered = *state.read(cx);
    (hovered, state)
}

/// The `on_hover` listener that keeps a [`hovered`] flag in step with the pointer.
pub(crate) fn track(state: Entity<bool>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, _, cx| {
        let hovered = *hovered;
        state.update(cx, |flag, cx| {
            if *flag != hovered {
                *flag = hovered;
                cx.notify();
            }
        });
    }
}

/// Which config key a view-mode toggle writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewModeKey {
    Artists,
    ArtistAlbums,
}

impl ViewModeKey {
    fn read(self, config: &AppConfig) -> AlbumViewMode {
        match self {
            Self::Artists => config.artists_view_mode,
            Self::ArtistAlbums => config.artist_album_view_mode,
        }
    }

    fn write(self, config: &mut AppConfig, mode: AlbumViewMode) {
        match self {
            Self::Artists => config.artists_view_mode = mode,
            Self::ArtistAlbums => config.artist_album_view_mode = mode,
        }
    }
}

/// A sortable field: its label, and the config key the criteria stack lives under.
pub trait SortTarget: Copy + PartialEq + std::fmt::Debug + 'static {
    fn label_key(self) -> &'static str;
    fn read(config: &AppConfig) -> Vec<SortCriterion<Self>>;
    fn write(config: &mut AppConfig, criteria: Vec<SortCriterion<Self>>);
}

impl SortTarget for ArtistSortField {
    fn label_key(self) -> &'static str {
        match self {
            Self::Name => "sort_field_name",
            Self::Tracks => "sort_field_tracks",
            Self::Albums => "sort_field_albums",
        }
    }

    fn read(config: &AppConfig) -> Vec<SortCriterion<Self>> {
        config.artist_sort.clone()
    }

    fn write(config: &mut AppConfig, criteria: Vec<SortCriterion<Self>>) {
        config.artist_sort = criteria;
    }
}

impl SortTarget for AlbumSortField {
    fn label_key(self) -> &'static str {
        match self {
            Self::Title => "sort_field_title",
            Self::Artist => "sort_field_artist",
            Self::Year => "sort_field_year",
            Self::Genre => "sort_field_genre",
        }
    }

    fn read(config: &AppConfig) -> Vec<SortCriterion<Self>> {
        config.artist_album_sort.clone()
    }

    fn write(config: &mut AppConfig, criteria: Vec<SortCriterion<Self>>) {
        config.artist_album_sort = criteria;
    }
}

/// Read the criteria stack a control edits; empty until the daemon config loads.
pub fn criteria<F: SortTarget>(cx: &App) -> Vec<SortCriterion<F>> {
    let model = ConfigModel::global(cx);
    F::read(model.read(cx).config())
}

pub fn config(cx: &App) -> AppConfig {
    let model = ConfigModel::global(cx);
    model.read(cx).config().clone()
}

fn edit(cx: &mut App, change: impl FnOnce(&mut AppConfig)) {
    ConfigModel::global(cx).update(cx, |model, cx| model.update(cx, change));
}

/// The `h-9 p-1 rounded-full bg-white/5` track both segmented toggles sit in.
fn pill(cx: &App) -> Div {
    h_flex()
        .h_9()
        .p_1()
        .gap_1()
        .rounded_full()
        .bg(style::white(style::RAISED, cx))
        .border_1()
        .border_color(style::white(style::BORDER_SOFT, cx))
}

/// One half of a pill: `h-7 px-3 text-xs rounded-md`, dim until it is the chosen one.
///
/// `transition-all`: the fill and the label tween as the selection moves between the halves.
fn segment(id: &'static str, selected: bool, window: &mut Window, cx: &mut App) -> Button {
    let fill = motion::color(
        SharedString::from(format!("{id}-fill")),
        style::white(if selected { style::HOVER } else { 0. }, cx),
        window,
        cx,
    );
    let label = motion::color(
        SharedString::from(format!("{id}-label")),
        style::white(if selected { 1. } else { SEGMENT_IDLE }, cx),
        window,
        cx,
    );
    Button::new(id)
        .ghost()
        .xsmall()
        .h_7()
        .px_3()
        .rounded(SEGMENT_RADIUS)
        .selected(selected)
        .bg(fill)
        .text_color(label)
}

/// Grid ⇄ list, writing `key`'s config field.
pub fn view_mode_toggle(key: ViewModeKey, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let mode = key.read(&config(cx));
    pill(cx)
        .child(
            segment("view-grid", mode == AlbumViewMode::Grid, window, cx)
                .icon(icons::icon(icons::GRID))
                .tooltip(t("view_grid"))
                .on_click(move |_, _, cx| {
                    edit(cx, |config| key.write(config, AlbumViewMode::Grid))
                }),
        )
        .child(
            segment("view-list", mode == AlbumViewMode::List, window, cx)
                .icon(icons::icon(icons::ROWS))
                .tooltip(t("view_list"))
                .on_click(move |_, _, cx| {
                    edit(cx, |config| key.write(config, AlbumViewMode::List))
                }),
        )
}

/// The stacked sort: the first field decides, the rest break ties, name always last.
pub fn sort_control<F: SortTarget>(
    id: &'static str,
    available: Vec<F>,
    cx: &App,
) -> impl IntoElement {
    let current: Vec<SortCriterion<F>> = criteria(cx);
    let label = match current.first() {
        Some(criterion) => SharedString::from(format!(
            "{}: {}",
            t("sort_by"),
            t(criterion.field.label_key())
        )),
        None => t("sort_by"),
    };
    Button::new(id)
        .ghost()
        .small()
        .icon(icons::icon(icons::SORT))
        .label(label)
        .dropdown_menu(move |mut menu, _, cx| {
            let current: Vec<SortCriterion<F>> = criteria(cx);
            for field in &available {
                let field = *field;
                let position = current
                    .iter()
                    .position(|criterion| criterion.field == field);
                let direction = position.map(|index| current[index].direction);
                let item = PopupMenuItem::new(t(field.label_key()))
                    .checked(position.is_some())
                    .on_click(move |_, _, cx| promote::<F>(field, cx));
                menu = menu.item(match direction {
                    Some(SortDirection::Asc) => item.icon(icons::icon(icons::CHEVRON_UP)),
                    Some(SortDirection::Desc) => item.icon(icons::icon(icons::CHEVRON_DOWN)),
                    None => item,
                });
            }
            menu.separator().item(
                PopupMenuItem::new(t("sort_none"))
                    .icon(icons::icon(icons::CLOSE))
                    .on_click(|_, _, cx| edit(cx, |config| F::write(config, Vec::new()))),
            )
        })
}

/// Clicking the deciding field flips its direction; any other field becomes the new one.
fn promote<F: SortTarget>(field: F, cx: &mut App) {
    let mut current: Vec<SortCriterion<F>> = criteria(cx);
    match current.first_mut() {
        Some(first) if first.field == field => {
            first.direction = match first.direction {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            };
        }
        _ => {
            current.retain(|criterion| criterion.field != field);
            current.insert(0, SortCriterion::new(field, SortDirection::Asc));
        }
    }
    edit(cx, move |config| F::write(config, current));
}

/// Tracks ⇄ Albums for one artist, writing `cfg.artist_view_order`.
pub fn view_order_toggle(
    order: &ArtistViewOrder,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let tracks = *order == ArtistViewOrder::Tracks;
    pill(cx)
        .child(
            segment("order-tracks", tracks, window, cx)
                .label(t("tracks"))
                .on_click(|_, _, cx| {
                    edit(cx, |config| {
                        config.artist_view_order = ArtistViewOrder::Tracks
                    })
                }),
        )
        .child(
            segment("order-albums", !tracks, window, cx)
                .label(t("albums"))
                .on_click(|_, _, cx| {
                    edit(cx, |config| {
                        config.artist_view_order = ArtistViewOrder::Albums
                    })
                }),
        )
}
