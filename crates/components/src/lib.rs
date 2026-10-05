//! Every shared component, each defined once: a page composes these and never draws its own copy.

pub mod cards;
pub mod carousel;
pub mod columns;
pub mod cover;
pub mod dialogs;
pub mod grid;
pub mod header;
pub mod karaoke;
pub mod lead;
pub mod menus;
pub mod playlist_picker;
pub mod schema_form;
pub mod settings;
pub mod skeleton;
pub mod states;
pub mod track_header;
pub mod track_list;
pub mod track_row;

pub use primitives::{
    Axis, Roving, Step, backdrop, duration, focus, format, hover, icons, marker, motion, scroll,
    skin, style, t, t_with, text, total_duration,
};

pub use cards::{
    Frame, PlayState, Tile, TileRow, album_row, album_tile, artist_row, artist_tile, playlist_tile,
    tile_grid,
};
pub use carousel::{Arrows, Carousel, Heading, ShelfHeading, heading_link};
pub use cover::Cover;
pub use grid::{GridMode, JumpRail, TileGrid, rail_letter};
pub use header::{DetailHeader, PageHeader, detail_header_skeleton};
pub use karaoke::KaraokeText;
pub use schema_form::{FieldChanged, SchemaForm};
pub use settings::{Section, SettingRow, Toggle, section};
pub use states::{EmptyState, Loading};
pub use track_header::TrackHeader;
pub use track_list::{TrackActions, TrackDrag, TrackList, TrackListEvent, TrackListOptions};
pub use track_row::TrackRow;
