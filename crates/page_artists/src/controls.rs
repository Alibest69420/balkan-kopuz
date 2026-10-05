//! The header controls the two artist pages share: view mode, stacked sort, and Tracks⇄Albums.

use components::format::t;
use components::{Segment, Segmented, SortControl};
use config::{
    AlbumSortField, AlbumViewMode, AppConfig, ArtistSortField, ArtistViewOrder, SortCriterion,
};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::{App, Hsla, IntoElement, Styled as _};
use store::ConfigModel;

/// A `Button` paints its own hover style over the instance one, so the variant is handed the tween too.
pub(crate) fn tweened(button: Button, fill: Hsla, text: Hsla, edge: Hsla, cx: &App) -> Button {
    button
        .custom(
            ButtonCustomVariant::new(cx)
                .color(edge)
                .hover(fill)
                .active(fill)
                .foreground(text),
        )
        .bg(fill)
        .border_color(edge)
        .text_color(text)
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

/// Grid ⇄ list, writing `key`'s config field.
pub fn view_mode_toggle(key: ViewModeKey, cx: &App) -> impl IntoElement {
    let grid = key.read(&config(cx)) == AlbumViewMode::Grid;
    let id = match key {
        ViewModeKey::Artists => "artists-view",
        ViewModeKey::ArtistAlbums => "artist-albums-view",
    };
    components::view_toggle(id, grid, move |grid, cx| {
        let mode = match grid {
            true => AlbumViewMode::Grid,
            false => AlbumViewMode::List,
        };
        ConfigModel::edit(cx, |config| key.write(config, mode))
    })
    .pill(true)
}

/// The stacked sort: the first field decides, the rest break ties, name always last.
pub fn sort_control<F: SortTarget>(
    id: &'static str,
    available: Vec<F>,
    cx: &App,
) -> impl IntoElement {
    SortControl::new(id, criteria::<F>(cx))
        .fields(
            available
                .into_iter()
                .map(|field| (field, t(field.label_key()))),
        )
        .on_change(|criteria, cx| ConfigModel::edit(cx, move |config| F::write(config, criteria)))
}

/// Tracks ⇄ Albums for one artist, writing `cfg.artist_view_order`.
pub fn view_order_toggle(order: &ArtistViewOrder) -> impl IntoElement {
    Segmented::new("artist-order")
        .pill(true)
        .segment(Segment::new("tracks").label(t("tracks")))
        .segment(Segment::new("albums").label(t("albums")))
        .selected(Some(match order {
            ArtistViewOrder::Tracks => 0,
            ArtistViewOrder::Albums => 1,
        }))
        .on_select(|index, _, cx| {
            let order = match index {
                0 => ArtistViewOrder::Tracks,
                _ => ArtistViewOrder::Albums,
            };
            ConfigModel::edit(cx, move |config| config.artist_view_order = order)
        })
}
