//! The source's own browse catalog: shelves of albums, playlists, artists and
//! songs that the library does not hold.
//!
//! A client renders these rows without knowing which service produced them or
//! how to talk to it. Songs arrive as ordinary [`TrackInfo`], so queueing one
//! is the same call as queueing a library track; the daemon remembers the row
//! so that key still resolves.

use crate::library::TrackInfo;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CatalogItemKind {
    Track,
    Album,
    Playlist,
    Artist,
    /// A curated mood or genre page.
    Mood,
    Podcast,
    Episode,
    Video,
    /// Any other page the source can open, such as a chart.
    Page,
    #[default]
    Unknown,
}

/// One tile. `id` is what [`CatalogDetailRequest`] takes to open it; an artist tile's is its key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogItem {
    pub kind: CatalogItemKind,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork: Option<crate::ArtworkRef>,
    /// Present for the kinds that play as one song, so a shelf of them is
    /// playable without a second round trip.
    pub track: Option<TrackInfo>,
    /// `#rrggbb`, where the source gives the tile a colour.
    pub accent: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShelfLayout {
    #[default]
    Carousel,
    Grid,
    List,
    /// Track rows in columns of four that scroll sideways ("Quick picks").
    TrackGrid,
    /// One large tile with a few rows under it ("Top result").
    Hero,
}

/// A row of tiles, laid out as `layout` says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogShelf {
    pub title: String,
    pub strapline: Option<String>,
    pub items: Vec<CatalogItem>,
    /// Opens the shelf's own page, when it has one.
    pub more_ref: Option<String>,
    pub layout: ShelfLayout,
    /// The kind that opens `more_ref`.
    pub more_kind: CatalogItemKind,
    /// More items for this shelf, not more shelves for the page.
    pub continuation: Option<String>,
    /// On an "all" search, the filter that shows every result of this kind.
    pub search_filter: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogPage {
    pub shelves: Vec<CatalogShelf>,
    /// Pass back to [`crate::LibraryApi::catalog`] for the next page.
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogDetailRequest {
    pub kind: CatalogItemKind,
    /// What the daemon handed out for the entity: a tile's `id`, or an artist's key.
    pub id: String,
    pub continuation: Option<String>,
}

impl CatalogDetailRequest {
    pub fn new(kind: CatalogItemKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            continuation: None,
        }
    }

    pub fn artist(artist: &str) -> Self {
        Self::new(CatalogItemKind::Artist, artist)
    }

    pub fn page(id: impl Into<String>) -> Self {
        Self::new(CatalogItemKind::Page, id)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CatalogHeader {
    #[default]
    None,
    Title,
    /// Artwork beside the title.
    Detail,
    /// A full-width banner.
    Artist,
}

/// A filter over a page; `id` opens the filtered page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogChip {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

/// One catalog entity opened: its tracks, or its own shelves, or both.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogDetail {
    pub kind: CatalogItemKind,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub artwork: Option<crate::ArtworkRef>,
    /// The id to play the whole thing, where that differs from `id`.
    pub playback_id: Option<String>,
    pub year: Option<String>,
    pub tracks: Vec<TrackInfo>,
    pub shelves: Vec<CatalogShelf>,
    pub continuation: Option<String>,
    /// For an album, the artist its header opens; absent when it bills nobody.
    pub artist_key: Option<String>,
    pub header: CatalogHeader,
    pub chips: Vec<CatalogChip>,
}
