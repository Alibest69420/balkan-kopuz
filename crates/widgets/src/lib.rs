//! The app's shared UI vocabulary: covers, tiles, the track list, forms and the states pages show instead of rows.

pub mod backdrop;
pub mod cards;
pub mod cover;
pub mod dialogs;
pub mod focus;
pub mod format;
pub mod header;
pub mod hover;
pub mod icons;
pub mod karaoke;
pub mod marker;
pub mod menus;
pub mod motion;
pub mod playlist_picker;
pub mod schema_form;
pub mod scroll;
pub mod settings;
pub mod skin;
pub mod states;
pub mod style;
pub mod track_list;

pub use cards::{Tile, album_tile, artist_tile, playlist_tile, tile_grid};
pub use cover::Cover;
pub use focus::{Axis, Roving, Step};
pub use format::{duration, t, t_with, text, total_duration};
pub use header::{DetailHeader, PageHeader, detail_header_skeleton};
pub use karaoke::KaraokeText;
pub use schema_form::{FieldChanged, SchemaForm};
pub use settings::{Section, SettingRow, Toggle, section};
pub use states::{EmptyState, Loading};
pub use track_list::{
    TrackActions, TrackDrag, TrackList, TrackListEvent, TrackListOptions, columns,
};
