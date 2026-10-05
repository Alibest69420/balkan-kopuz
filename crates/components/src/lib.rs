//! Every shared component, each defined once: a page composes these and never draws its own copy.

pub mod cards;
pub mod carousel;
pub mod chip;
pub mod columns;
pub mod cover;
pub mod dialogs;
pub mod grid;
pub mod header;
pub mod heading;
pub mod karaoke;
pub mod lead;
pub mod link;
pub mod menus;
pub mod playlist_picker;
pub mod schema_form;
pub mod search;
pub mod settings;
pub mod skeleton;
pub mod states;
pub mod tabs;
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
pub use carousel::{Arrows, Carousel};
pub use chip::{Badge, BadgeTone, Chip, ChipSize, ChipTone};
pub use cover::{Cover, Scrim};
pub use grid::{GridMode, JumpRail, TileGrid, rail_letter};
pub use header::{DetailHeader, PageHeader, TitleSize, detail_header_skeleton};
pub use heading::{Heading, SectionHeading};
pub use karaoke::KaraokeText;
pub use link::{Link, Tone, heading_link};
pub use schema_form::{FieldChanged, SchemaForm};
pub use search::{SearchField, SearchTrigger, TriggerSize};
pub use settings::{Section, SettingRow, Toggle, section};
pub use states::{EmptyState, Loading};
pub use tabs::{Bar, Tab, TabStrip};
pub use track_header::TrackHeader;
pub use track_list::{TrackActions, TrackDrag, TrackList, TrackListEvent, TrackListOptions};
pub use track_row::TrackRow;
