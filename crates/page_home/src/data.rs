//! The lists the Home sections render, derived once per data change rather than per frame.

use std::collections::{HashMap, HashSet};

use api::{AlbumInfo, AlbumPage, ArtistPage, ArtworkRef, TrackInfo, TrackPage};

/// Recently Added shows 12, but untitled and same-titled albums drop out afterwards.
pub(crate) const RECENTLY_ADDED_WINDOW: u32 = 64;
pub(crate) const RECENT_TRACKS_WINDOW: u32 = 50;
pub(crate) const ARTIST_SAMPLE_WINDOW: u32 = 30;

const ROW_LIMIT: usize = 12;
const CONTINUE_LIMIT: usize = 10;
const ARTIST_LIMIT: usize = 10;

pub(crate) struct Artist {
    pub name: String,
    pub artwork: Option<ArtworkRef>,
}

/// A recently played row with the album it belongs to, where the albums query has caught up.
pub(crate) struct Recent {
    pub track: TrackInfo,
    pub album: Option<AlbumInfo>,
}

#[derive(Default)]
pub(crate) struct Data {
    pub shuffled: Vec<AlbumInfo>,
    pub new_releases: Vec<AlbumInfo>,
    pub recently_added: Vec<AlbumInfo>,
    pub made_for_you: Vec<AlbumInfo>,
    pub genre: String,
    pub continue_listening: Vec<Recent>,
    pub hero: Option<Recent>,
    pub top_artists: Vec<Artist>,
}

pub(crate) struct Inputs<'a> {
    pub albums: Option<&'a AlbumPage>,
    pub recently_added: Option<&'a AlbumPage>,
    pub artists: Option<&'a ArtistPage>,
    pub recents: Option<&'a TrackPage>,
    pub samples: Option<&'a TrackPage>,
    pub genre: Option<&'a String>,
    pub seed: u64,
}

pub(crate) fn is_unknown_artist(value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    normalized.is_empty() || normalized == "unknown artist"
}

pub(crate) fn is_unknown_album(value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    normalized.is_empty() || normalized == "unknown album"
}

fn named(album: &AlbumInfo) -> bool {
    !is_unknown_album(&album.title) && !is_unknown_artist(&album.artist)
}

fn title_key(album: &AlbumInfo) -> String {
    album.title.trim().to_lowercase()
}

fn artist_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Named albums, sorted by title, one per title.
// TODO(offline): offline mode keeps only albums whose tracks are downloaded.
fn unique_albums(albums: &[AlbumInfo]) -> Vec<AlbumInfo> {
    let mut sorted: Vec<AlbumInfo> = albums
        .iter()
        .filter(|album| named(album))
        .cloned()
        .collect();
    sorted.sort_by_key(title_key);
    let mut seen = HashSet::new();
    sorted.retain(|album| seen.insert(title_key(album)));
    sorted
}

/// Up to `limit` named albums in the order given, one per title.
fn take_unique(albums: impl Iterator<Item = AlbumInfo>, limit: usize) -> Vec<AlbumInfo> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for album in albums {
        if !named(&album) || !seen.insert(title_key(&album)) {
            continue;
        }
        out.push(album);
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub(crate) fn build(inputs: Inputs<'_>) -> Data {
    let albums = inputs
        .albums
        .map(|page| page.albums.as_slice())
        .unwrap_or(&[]);
    let by_id: HashMap<&str, &AlbumInfo> = albums
        .iter()
        .map(|album| (album.id.as_str(), album))
        .collect();

    let mut shuffled = unique_albums(albums);
    shuffle(&mut shuffled, inputs.seed);

    let mut by_year: Vec<AlbumInfo> = albums.to_vec();
    by_year.sort_by_key(|album| std::cmp::Reverse(album.year));
    let new_releases = take_unique(by_year.into_iter(), ROW_LIMIT);

    let recently_added = take_unique(
        inputs
            .recently_added
            .map(|page| page.albums.clone())
            .unwrap_or_default()
            .into_iter(),
        ROW_LIMIT,
    );

    let genre = inputs.genre.cloned().unwrap_or_default();
    let mut made_for_you: Vec<AlbumInfo> = if genre.is_empty() {
        Vec::new()
    } else {
        albums
            .iter()
            .filter(|album| album.genre == genre && named(album))
            .cloned()
            .collect()
    };
    shuffle(&mut made_for_you, inputs.seed);
    made_for_you.truncate(ROW_LIMIT);

    let recents = inputs
        .recents
        .map(|page| page.items.as_slice())
        .unwrap_or(&[]);
    let continue_listening = continue_listening(recents, &by_id);
    let hero = recents
        .iter()
        .find(|track| !track.title.trim().is_empty())
        .map(|track| Recent {
            track: track.clone(),
            album: by_id
                .get(track.album_id.as_str())
                .map(|album| (*album).clone()),
        });

    Data {
        shuffled,
        new_releases,
        recently_added,
        made_for_you,
        genre,
        continue_listening,
        hero,
        top_artists: top_artists(inputs.samples, inputs.artists),
    }
}

/// One card per album, newest first, skipping rows with nothing to name them by.
fn continue_listening(recents: &[TrackInfo], by_id: &HashMap<&str, &AlbumInfo>) -> Vec<Recent> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for track in recents {
        if track.title.trim().is_empty() {
            continue;
        }
        let album = by_id
            .get(track.album_id.as_str())
            .map(|album| (*album).clone());
        match &album {
            Some(album) if !named(album) => continue,
            Some(album) if !seen.insert(album.id.clone()) => continue,
            None if is_unknown_artist(&track.artist) => continue,
            _ => {}
        }
        out.push(Recent {
            track: track.clone(),
            album,
        });
        if out.len() >= CONTINUE_LIMIT {
            break;
        }
    }
    out
}

fn top_artists(samples: Option<&TrackPage>, artists: Option<&ArtistPage>) -> Vec<Artist> {
    // TODO(offline): offline mode draws these from the downloaded tracks instead.
    let photos: HashMap<String, &ArtworkRef> = artists
        .map(|page| page.artists.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|artist| Some((artist_key(&artist.name), artist.artwork.as_ref()?)))
        .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for track in samples.map(|page| page.items.as_slice()).unwrap_or(&[]) {
        if is_unknown_artist(&track.artist) || !seen.insert(track.artist.clone()) {
            continue;
        }
        out.push(Artist {
            name: track.artist.clone(),
            artwork: photos
                .get(&artist_key(&track.artist))
                .map(|art| (*art).clone()),
        });
        if out.len() >= ARTIST_LIMIT {
            break;
        }
    }
    out
}

/// Fisher-Yates from the page's own seed, so a row keeps its order until its data changes.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    for index in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        items.swap(index, (state % (index as u64 + 1)) as usize);
    }
}
