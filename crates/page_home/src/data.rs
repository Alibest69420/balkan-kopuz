//! What the shelves hold: every tile derived once per data change, never per frame.

use std::collections::{HashMap, HashSet};

use api::{AlbumInfo, AlbumPage, ArtistPage, ArtworkRef, PlaylistInfo, TrackInfo, TrackPage};
use gpui_kit::SharedString;
use store::{Route, Target};
use widgets::format::t_with;
use widgets::icons;

/// Recently Added shows a shelf, but untitled and same-titled albums drop out afterwards.
pub(crate) const RECENTLY_ADDED_WINDOW: u32 = 64;
pub(crate) const RECENT_TRACKS_WINDOW: u32 = 50;

/// How far a shelf reaches. Only the tiles in view are built, so this is reach, not cost.
const SHELF: usize = 18;

/// What a tile's play button starts; an artist has no button of its own.
#[derive(Clone)]
pub(crate) enum Play {
    None,
    Album(String),
    Track(String),
    Playlist(String),
}

/// One card, with everything a frame needs already in the shape the frame wants it.
pub(crate) struct Tile {
    pub key: SharedString,
    pub title: SharedString,
    pub subtitle: SharedString,
    pub artwork: Option<ArtworkRef>,
    pub route: Route,
    pub target: Target,
    pub play: Play,
    pub round: bool,
    pub placeholder: &'static str,
}

/// How a shelf is drawn, which follows from what it holds rather than from where it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// A flat list of equals: a row that glides sideways.
    Row,
    /// The newest session leads, because it is the one most likely wanted back.
    Mosaic,
    /// A ranking, so it is numbered and carries the count it is ranked by.
    Ranked,
}

/// One row of the page, under the name the layout orders it by.
pub(crate) struct Shelf {
    pub name: &'static str,
    pub tiles: Vec<Tile>,
    pub form: Form,
}

impl Shelf {
    fn new(name: &'static str, tiles: Vec<Tile>) -> Self {
        let form = match name {
            "jump_back_in" => Form::Mosaic,
            "top_artists" => Form::Ranked,
            _ => Form::Row,
        };
        Self { name, tiles, form }
    }
}

pub(crate) struct Inputs<'a> {
    pub albums: Option<&'a AlbumPage>,
    pub recently_added: Option<&'a AlbumPage>,
    pub artists: Option<&'a ArtistPage>,
    pub recents: Option<&'a TrackPage>,
    /// The most-played tracks, in the daemon's own play order, so an artist can be ranked.
    pub played: Option<&'a TrackPage>,
    pub plays: Option<&'a HashMap<String, u64>>,
    pub genre: Option<&'a String>,
    pub playlists: &'a [PlaylistInfo],
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

pub(crate) fn build(inputs: Inputs<'_>) -> Vec<Shelf> {
    let albums = inputs
        .albums
        .map(|page| page.albums.as_slice())
        .unwrap_or(&[]);
    let by_id: HashMap<&str, &AlbumInfo> = albums
        .iter()
        .map(|album| (album.id.as_str(), album))
        .collect();
    let recents = inputs
        .recents
        .map(|page| page.items.as_slice())
        .unwrap_or(&[]);

    let recently_added = take_unique(
        inputs
            .recently_added
            .map(|page| page.albums.clone())
            .unwrap_or_default()
            .into_iter(),
        SHELF,
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
    made_for_you.truncate(SHELF);

    vec![
        Shelf::new("jump_back_in", jump_back_in(recents, &by_id)),
        Shelf::new(
            "recently_added",
            recently_added
                .iter()
                .map(|album| album_tile("recently-added", album))
                .collect(),
        ),
        Shelf::new(
            "made_for_you",
            made_for_you
                .iter()
                .map(|album| album_tile("made-for-you", album))
                .collect(),
        ),
        Shelf::new(
            "top_artists",
            top_artists(inputs.played, inputs.plays, inputs.artists),
        ),
        Shelf::new(
            "your_playlists",
            inputs
                .playlists
                .iter()
                .rev()
                .take(SHELF)
                .map(playlist_tile)
                .collect(),
        ),
        Shelf::new("recently_played", recently_played(recents)),
    ]
}

fn album_tile(scope: &str, album: &AlbumInfo) -> Tile {
    Tile {
        key: SharedString::from(format!("{scope}-{}", album.id)),
        title: album.title.clone().into(),
        subtitle: album.artist.clone().into(),
        artwork: album.artwork.clone(),
        route: Route::Album(album.id.clone().into()),
        target: Target::Album {
            id: album.id.clone().into(),
            title: album.title.clone().into(),
        },
        play: Play::Album(album.id.clone()),
        round: false,
        placeholder: icons::DISC,
    }
}

/// The albums the last listening session was in, newest first, one tile per album.
fn jump_back_in(recents: &[TrackInfo], by_id: &HashMap<&str, &AlbumInfo>) -> Vec<Tile> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for track in recents {
        if track.title.trim().is_empty() {
            continue;
        }
        let album = by_id.get(track.album_id.as_str()).copied();
        match album {
            Some(album) if !named(album) => continue,
            Some(album) if !seen.insert(album.id.clone()) => continue,
            None if is_unknown_artist(&track.artist) => continue,
            _ => {}
        }
        out.push(match album {
            Some(album) => album_tile("jump-back-in", album),
            // A single, or anything the library holds no album row for, stands as the track it is.
            None => track_tile("jump-back-in", track),
        });
        if out.len() >= SHELF {
            break;
        }
    }
    out
}

/// Every recent track in the order it was played, so a one-off song is still findable.
fn recently_played(recents: &[TrackInfo]) -> Vec<Tile> {
    recents
        .iter()
        .filter(|track| !track.title.trim().is_empty())
        .take(SHELF)
        .map(|track| track_tile("recently-played", track))
        .collect()
}

fn track_tile(scope: &str, track: &TrackInfo) -> Tile {
    // Without an album row there is no album page to open, so the tile goes to the artist.
    let route = match track.album_id.is_empty() {
        false => Route::Album(track.album_id.clone().into()),
        true => Route::Artist {
            name: track.artist.clone().into(),
            channel: None,
        },
    };
    Tile {
        key: SharedString::from(format!("{scope}-{}", track.uid)),
        title: track.title.clone().into(),
        subtitle: track.artist.clone().into(),
        artwork: track.artwork.clone(),
        route,
        target: Target::track(track.key.clone(), track.title.clone()),
        play: Play::Track(track.key.clone()),
        round: false,
        placeholder: icons::MUSIC,
    }
}

fn playlist_tile(playlist: &PlaylistInfo) -> Tile {
    Tile {
        key: SharedString::from(format!("playlist-{}", playlist.id)),
        title: playlist.name.clone().into(),
        subtitle: t_with(
            "playlist_track_count",
            &[("count", playlist.track_keys.len().to_string())],
        ),
        artwork: playlist.artwork.clone(),
        route: Route::Playlist(playlist.id.clone().into()),
        target: Target::Playlist {
            id: playlist.id.clone().into(),
            name: playlist.name.clone().into(),
        },
        play: Play::Playlist(playlist.id.clone()),
        round: false,
        placeholder: icons::LIST,
    }
}

/// How many artists a ranking is worth reading; past this it stops being a top anything.
pub(crate) const RANKED: usize = 8;

/// Sum the listen counts per artist. An artist with no plays is not in the ranking.
pub(crate) fn plays_by_artist(
    played: &[TrackInfo],
    plays: &HashMap<String, u64>,
) -> Vec<(String, u64)> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for track in played {
        if is_unknown_artist(&track.artist) {
            continue;
        }
        // The counts are keyed by uid; `key` is the library ref and would match nothing.
        let Some(count) = plays.get(&track.uid) else {
            continue;
        };
        *totals.entry(track.artist.clone()).or_default() += count;
    }
    let mut ranked: Vec<(String, u64)> = totals.into_iter().collect();
    // Ties settle by name, so a redraw cannot shuffle two artists past each other.
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked.truncate(RANKED);
    ranked
}

/// Who actually gets played, in order. Empty until the counts arrive, rather than alphabetical.
fn top_artists(
    played: Option<&TrackPage>,
    plays: Option<&HashMap<String, u64>>,
    artists: Option<&ArtistPage>,
) -> Vec<Tile> {
    // TODO(offline): offline mode draws these from the downloaded tracks instead.
    let (Some(played), Some(plays)) = (played, plays) else {
        return Vec::new();
    };
    let played = played.items.as_slice();
    let photos: HashMap<String, &ArtworkRef> = artists
        .map(|page| page.artists.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|artist| Some((artist_key(&artist.name), artist.artwork.as_ref()?)))
        .collect();
    plays_by_artist(played, plays)
        .into_iter()
        .map(|(artist, count)| Tile {
            key: SharedString::from(format!("artist-{artist}")),
            title: artist.clone().into(),
            subtitle: t_with("home_plays", &[("count", count.to_string())]),
            artwork: photos.get(&artist_key(&artist)).map(|art| (*art).clone()),
            route: Route::Artist {
                name: artist.clone().into(),
                channel: None,
            },
            target: Target::Artist {
                name: artist.into(),
                channel: None,
            },
            play: Play::None,
            round: true,
            placeholder: icons::MIC,
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn track(uid: &str, title: &str, artist: &str, album_id: &str) -> TrackInfo {
        TrackInfo {
            uid: uid.into(),
            key: format!("key-{uid}"),
            title: title.into(),
            artist: artist.into(),
            album_id: album_id.into(),
            ..Default::default()
        }
    }

    fn album(id: &str, title: &str, artist: &str) -> AlbumInfo {
        AlbumInfo {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    #[test]
    fn jump_back_in_holds_one_tile_per_album() {
        let first = album("a1", "Pixel Down", "NONEP");
        let by_id = HashMap::from([("a1", &first)]);
        let recents = vec![
            track("t1", "One", "NONEP", "a1"),
            track("t2", "Two", "NONEP", "a1"),
        ];
        let tiles = jump_back_in(&recents, &by_id);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].title, "Pixel Down");
    }

    #[test]
    fn a_track_with_no_album_row_still_gets_a_tile() {
        let by_id = HashMap::new();
        let recents = vec![track("t1", "A single", "Someone", "")];
        let tiles = jump_back_in(&recents, &by_id);
        assert_eq!(tiles.len(), 1);
        assert!(matches!(tiles[0].play, Play::Track(_)));
    }

    #[test]
    fn recently_played_keeps_every_play_in_order() {
        let recents = vec![
            track("t1", "One", "NONEP", "a1"),
            track("t2", "Two", "NONEP", "a1"),
            track("t3", "", "NONEP", "a1"),
        ];
        let tiles = recently_played(&recents);
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[1].title, "Two");
    }

    #[test]
    fn every_shelf_the_layout_names_is_built() {
        let shelves = build(Inputs {
            albums: None,
            recently_added: None,
            artists: None,
            recents: None,
            played: None,
            plays: None,
            genre: None,
            playlists: &[],
            seed: 1,
        });
        let names: Vec<&str> = shelves.iter().map(|shelf| shelf.name).collect();
        assert_eq!(names, store::layout::SHELVES);
    }

    #[test]
    fn a_shelf_is_drawn_the_way_its_contents_ask_to_be() {
        assert_eq!(Shelf::new("jump_back_in", Vec::new()).form, Form::Mosaic);
        assert_eq!(Shelf::new("top_artists", Vec::new()).form, Form::Ranked);
        assert_eq!(Shelf::new("recently_added", Vec::new()).form, Form::Row);
    }

    #[test]
    fn an_artist_is_ranked_by_the_plays_of_every_track_of_theirs() {
        let played = vec![
            track("t1", "One", "NONEP", "a1"),
            track("t2", "Two", "NONEP", "a1"),
            track("t3", "Three", "AKASAKI", "a2"),
        ];
        // Keyed by uid, which is what the daemon counts against.
        let plays = HashMap::from([
            ("t1".to_string(), 3),
            ("t2".to_string(), 4),
            ("t3".to_string(), 5),
        ]);
        // NONEP's two tracks outweigh AKASAKI's one, which counting tracks would miss.
        assert_eq!(
            plays_by_artist(&played, &plays),
            vec![("NONEP".to_string(), 7), ("AKASAKI".to_string(), 5)]
        );
    }

    #[test]
    fn a_track_nobody_has_played_is_not_in_the_ranking() {
        let played = vec![track("t1", "One", "NONEP", "a1")];
        assert!(plays_by_artist(&played, &HashMap::new()).is_empty());
    }

    #[test]
    fn without_the_counts_the_ranking_is_empty_rather_than_alphabetical() {
        let played = TrackPage {
            total: 1,
            offset: 0,
            items: vec![track("t1", "One", "NONEP", "a1")],
        };
        assert!(top_artists(None, None, None).is_empty());
        assert!(top_artists(Some(&played), None, None).is_empty());
    }
}
