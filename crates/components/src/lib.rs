//! Every shared component, each defined once: a page composes these and never draws its own copy.

pub mod album_collection;
pub mod callout;
pub mod cards;
pub mod carousel;
pub mod chip;
pub mod columns;
pub mod cover;
pub mod cover_overlay;
pub mod detail_actions;
pub mod device_row;
pub mod dialogs;
pub mod dock_panel;
pub mod download_toggle;
pub mod drag;
pub mod fields;
pub mod folders;
pub mod grid;
pub mod header;
pub mod heading;
pub mod icon_button;
pub mod job;
pub mod karaoke;
pub mod keycap;
pub mod lead;
pub mod link;
pub mod lyric_line;
pub mod menu_button;
pub mod menus;
pub mod nav_row;
pub mod page;
pub mod pick_list;
pub mod queue_row;
pub mod reorder;
pub mod resizer;
pub mod resume_card;
pub mod schema_form;
pub mod search;
pub mod seek_bar;
pub mod segmented;
pub mod selection;
pub mod settings;
pub mod skeleton;
pub mod sort;
pub mod source_card;
pub mod source_mark;
pub mod source_status;
pub mod states;
pub mod tabs;
pub mod thin_slider;
pub mod track_header;
pub mod track_list;
pub mod track_meta;
pub mod track_row;
pub mod track_section;
pub mod transport;
pub mod volume_control;

pub use primitives::{
    Axis, Roving, Step, backdrop, duration, focus, format, hover, icons, marker, motion, scroll,
    skin, style, t, t_with, text, total_duration,
};

pub use album_collection::AlbumCollection;
pub use callout::{Callout, CalloutTone};
pub use cards::{
    Frame, PlayState, Tile, TileRow, album_row, album_tile, artist_row, artist_tile, playlist_tile,
    tile_grid,
};
pub use carousel::{Arrows, Carousel};
pub use chip::{Badge, BadgeTone, Chip, ChipSize, ChipTone};
pub use cover::{Cover, EditableCover, Scrim};
pub use cover_overlay::{CoverOverlay, TilePlayButton, playing_badge};
pub use detail_actions::{DetailActions, Look};
pub use device_row::DeviceRow;
pub use dock_panel::{DockPanel, DockSize};
pub use download_toggle::{DownloadToggle, ToggleSize};
pub use drag::DragChip;
pub use fields::{ComboChoice, FieldFrame, PathField, PathKind, RadioGroup, SecretInput};
pub use folders::{FolderBrowser, FolderList, parent_dir, pick_folders};
pub use grid::{GridMode, JumpRail, TileGrid, rail_letter};
pub use header::{
    DetailHeader, HeaderLayout, MetaLine, PageHeader, TitleSize, detail_header_skeleton,
};
pub use heading::{Heading, SectionHeading};
pub use icon_button::{FollowButton, IconButton, IconTone, back_button};
pub use job::{JobStop, ProgressBar};
pub use karaoke::KaraokeText;
pub use keycap::{KeyHint, Keycap};
pub use link::{Link, Tone, heading_link};
pub use lyric_line::LyricLine;
pub use menu_button::{MenuButton, MenuVariant};
pub use menus::{Menu, WithContextMenu};
pub use nav_row::NavListRow;
pub use page::page;
pub use pick_list::{PickItem, PickList, QuickCreate};
pub use queue_row::QueueRow;
pub use reorder::ReorderDropZones;
pub use resizer::{Edge, EdgeResizer};
pub use resume_card::ResumeCard;
pub use schema_form::{FieldChanged, SchemaForm};
pub use search::{SearchField, SearchTrigger, TriggerSize};
pub use seek_bar::{SeekBar, TrackClock};
pub use segmented::{Segment, Segmented, view_toggle};
pub use selection::{SelectionBar, Verb};
pub use settings::{Choice, Lockable, Section, SettingRow, SliderRow, Toggle, section};
pub use sort::SortControl;
pub use source_card::SourceCard;
pub use source_mark::SourceMark;
pub use source_status::{StatusDot, StatusLine};
pub use states::{EmptyState, ErrorBanner, FieldProblem, Loading, StateSize};
pub use tabs::{Bar, Tab, TabStrip};
pub use thin_slider::ThinSlider;
pub use track_header::TrackHeader;
pub use track_list::{TrackActions, TrackDrag, TrackList, TrackListEvent, TrackListOptions};
pub use track_meta::{MarqueeText, MetaLayout, NowPlayingMeta, PlayingFromLink, TrackMarks};
pub use track_row::TrackRow;
pub use track_section::TrackSection;
pub use transport::{
    PlayPauseButton, RepeatButton, Transport, TransportMetrics, TransportOverflow, lit_accent,
};
pub use volume_control::VolumeControl;
