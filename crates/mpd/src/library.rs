//! The library as MPD sees it: songs with URIs and tags, and the directory
//! tree their URIs spell out.
//!
//! MPD answers whole listings at once while the Kopuz API pages them, so the
//! library is read once into a [`Library`] and kept until the daemon says a
//! table changed.

use std::collections::{BTreeSet, HashMap};

use api::{AlbumInfo, AlbumQuery, ApiError, KopuzApi, Page, TrackFilter, TrackInfo};

const PAGE: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Tag {
    Artist,
    Album,
    AlbumArtist,
    Title,
    Track,
    Disc,
    Genre,
    Date,
    MusicBrainzAlbumId,
    MusicBrainzTrackId,
    MusicBrainzReleaseTrackId,
    /// A tag MPD knows that no Kopuz row carries; it is always empty.
    Unsupported,
}

impl Tag {
    /// What `tagtypes` lists and songs print, in MPD's spelling.
    pub const SUPPORTED: [(Tag, &'static str); 11] = [
        (Tag::Artist, "Artist"),
        (Tag::Album, "Album"),
        (Tag::AlbumArtist, "AlbumArtist"),
        (Tag::Title, "Title"),
        (Tag::Track, "Track"),
        (Tag::Disc, "Disc"),
        (Tag::Genre, "Genre"),
        (Tag::Date, "Date"),
        (Tag::MusicBrainzAlbumId, "MUSICBRAINZ_ALBUMID"),
        (Tag::MusicBrainzTrackId, "MUSICBRAINZ_TRACKID"),
        (Tag::MusicBrainzReleaseTrackId, "MUSICBRAINZ_RELEASETRACKID"),
    ];

    const OTHER: [&'static str; 24] = [
        "ArtistSort",
        "AlbumSort",
        "AlbumArtistSort",
        "TitleSort",
        "Name",
        "Mood",
        "OriginalDate",
        "Composer",
        "ComposerSort",
        "Performer",
        "Conductor",
        "Work",
        "Ensemble",
        "Movement",
        "MovementNumber",
        "ShowMovement",
        "Location",
        "Grouping",
        "Comment",
        "Label",
        "MUSICBRAINZ_ARTISTID",
        "MUSICBRAINZ_ALBUMARTISTID",
        "MUSICBRAINZ_WORKID",
        "MUSICBRAINZ_RELEASEGROUPID",
    ];

    /// Tag names are matched without regard to case, as MPD does.
    pub fn parse(name: &str) -> Option<Tag> {
        Self::SUPPORTED
            .iter()
            .find(|(_, known)| known.eq_ignore_ascii_case(name))
            .map(|(tag, _)| *tag)
            .or_else(|| {
                Self::OTHER
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(name))
                    .then_some(Tag::Unsupported)
            })
    }

    pub fn name(self) -> &'static str {
        Self::SUPPORTED
            .iter()
            .find(|(tag, _)| *tag == self)
            .map_or("", |(_, name)| name)
    }
}

/// A row with what MPD needs beside it: its URI and the album-level tags
/// Kopuz keeps on the album rather than the track.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Song {
    pub uri: String,
    pub track: TrackInfo,
    pub album_artist: String,
    pub genre: String,
    pub year: Option<u16>,
}

impl Song {
    pub fn new(track: TrackInfo, album: Option<&AlbumInfo>) -> Self {
        Self {
            uri: uri_of(&track.key),
            album_artist: album.map(|album| album.artist.clone()).unwrap_or_default(),
            genre: album.map(|album| album.genre.clone()).unwrap_or_default(),
            year: album.map(|album| album.year).filter(|year| *year > 0),
            track,
        }
    }

    pub fn tag(&self, tag: Tag) -> String {
        let number = |value: Option<u32>| value.map(|n| n.to_string()).unwrap_or_default();
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        match tag {
            Tag::Artist => self.track.artist.clone(),
            Tag::Album => self.track.album.clone(),
            Tag::AlbumArtist => self.album_artist.clone(),
            Tag::Title => self.track.title.clone(),
            Tag::Track => number(self.track.track_number),
            Tag::Disc => number(self.track.disc_number),
            Tag::Genre => self.genre.clone(),
            Tag::Date => self.year.map(|year| year.to_string()).unwrap_or_default(),
            Tag::MusicBrainzAlbumId => text(&self.track.musicbrainz_release_id),
            Tag::MusicBrainzTrackId => text(&self.track.musicbrainz_recording_id),
            Tag::MusicBrainzReleaseTrackId => text(&self.track.musicbrainz_track_id),
            Tag::Unsupported => String::new(),
        }
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.track.duration_ms.filter(|_| !self.track.is_radio())
    }
}

/// A key as an MPD URI: relative, so a local path loses its leading slash
/// and the tree roots at the filesystem's top directory.
pub(crate) fn uri_of(key: &str) -> String {
    key.strip_prefix('/').unwrap_or(key).to_string()
}

pub(crate) struct Library {
    pub songs: Vec<Song>,
    by_uri: HashMap<String, usize>,
    by_key: HashMap<String, usize>,
    pub albums: HashMap<String, AlbumInfo>,
}

impl Library {
    pub async fn load(api: &dyn KopuzApi) -> Result<Library, ApiError> {
        let mut albums = HashMap::new();
        let mut offset = 0;
        loop {
            let page = api
                .albums(
                    AlbumQuery::default(),
                    Page {
                        offset,
                        limit: PAGE,
                    },
                )
                .await?;
            let got = page.albums.len() as u32;
            albums.extend(
                page.albums
                    .into_iter()
                    .map(|album| (album.id.clone(), album)),
            );
            offset += got;
            if got == 0 || offset >= page.total {
                break;
            }
        }
        let mut tracks = Vec::new();
        let mut offset = 0;
        loop {
            let page = api
                .tracks(
                    TrackFilter::default(),
                    Page {
                        offset,
                        limit: PAGE,
                    },
                )
                .await?;
            let got = page.items.len() as u32;
            tracks.extend(page.items);
            offset += got;
            if got == 0 || offset >= page.total {
                break;
            }
        }
        Ok(Library::new(tracks, albums))
    }

    pub fn new(tracks: Vec<TrackInfo>, albums: HashMap<String, AlbumInfo>) -> Library {
        let mut songs: Vec<Song> = tracks
            .into_iter()
            .map(|track| {
                let album = albums.get(&track.album_id);
                Song::new(track, album)
            })
            .collect();
        songs.sort_by(|a, b| a.uri.cmp(&b.uri));
        songs.dedup_by(|a, b| a.uri == b.uri);
        let by_uri = songs
            .iter()
            .enumerate()
            .map(|(index, song)| (song.uri.clone(), index))
            .collect();
        let by_key = songs
            .iter()
            .enumerate()
            .map(|(index, song)| (song.track.key.clone(), index))
            .collect();
        Library {
            songs,
            by_uri,
            by_key,
            albums,
        }
    }

    pub fn by_uri(&self, uri: &str) -> Option<&Song> {
        self.by_uri.get(uri).map(|&index| &self.songs[index])
    }

    /// A row from elsewhere (the queue, a playlist) as a song, with the
    /// album tags the library holds for it.
    pub fn song(&self, track: TrackInfo) -> Song {
        if let Some(&index) = self.by_key.get(&track.key) {
            let known = &self.songs[index];
            return Song {
                track,
                ..known.clone()
            };
        }
        let album = self.albums.get(&track.album_id);
        Song::new(track, album)
    }

    pub fn key_of<'a>(&'a self, uri: &'a str) -> &'a str {
        self.by_uri(uri).map_or(uri, |song| song.track.key.as_str())
    }

    /// The songs at or under a directory URI; the root is `""`.
    pub fn under(&self, dir: &str) -> impl Iterator<Item = &Song> {
        let dir = dir.trim_matches('/').to_string();
        self.songs
            .iter()
            .filter(move |song| within(&song.uri, &dir))
    }

    /// The immediate subdirectories and songs of a directory, or `None` when
    /// nothing lives under it.
    pub fn list(&self, dir: &str) -> Option<(BTreeSet<String>, Vec<&Song>)> {
        let dir = dir.trim_matches('/');
        let mut dirs = BTreeSet::new();
        let mut files = Vec::new();
        for song in self.under(dir) {
            let rest = if dir.is_empty() {
                song.uri.as_str()
            } else {
                &song.uri[dir.len() + 1..]
            };
            match rest.split_once('/') {
                Some((child, _)) if dir.is_empty() => {
                    dirs.insert(child.to_string());
                }
                Some((child, _)) => {
                    dirs.insert(format!("{dir}/{child}"));
                }
                None => files.push(song),
            }
        }
        (dir.is_empty() || !dirs.is_empty() || !files.is_empty()).then_some((dirs, files))
    }

    pub fn playtime_secs(&self) -> u64 {
        self.songs
            .iter()
            .filter_map(Song::duration_ms)
            .map(|ms| ms / 1000)
            .sum()
    }

    pub fn distinct(&self, tag: Tag) -> usize {
        self.songs
            .iter()
            .map(|song| song.tag(tag))
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

fn within(uri: &str, dir: &str) -> bool {
    dir.is_empty()
        || uri
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn track(key: &str, artist: &str, album: &str, title: &str) -> TrackInfo {
        TrackInfo {
            key: key.into(),
            artist: artist.into(),
            album: album.into(),
            album_id: album.to_lowercase(),
            title: title.into(),
            duration_ms: Some(61_500),
            ..Default::default()
        }
    }

    pub fn library() -> Library {
        let albums = [("Geogaddi", "Boards of Canada", "Electronic", 2002)]
            .into_iter()
            .map(|(title, artist, genre, year)| {
                (
                    title.to_lowercase(),
                    AlbumInfo {
                        id: title.to_lowercase(),
                        title: title.into(),
                        artist: artist.into(),
                        genre: genre.into(),
                        year,
                        ..Default::default()
                    },
                )
            })
            .collect();
        Library::new(
            vec![
                track(
                    "/music/boc/geogaddi/01.flac",
                    "Boards of Canada",
                    "Geogaddi",
                    "Ready Lets Go",
                ),
                track(
                    "/music/boc/geogaddi/02.flac",
                    "Boards of Canada",
                    "Geogaddi",
                    "Music Is Math",
                ),
                track("/music/loose.mp3", "Aphex Twin", "", "Avril 14th"),
                track("ytm-song:abc", "Burial", "Untrue", "Archangel"),
            ],
            albums,
        )
    }

    #[test]
    fn local_paths_become_relative_uris_and_map_back() {
        let library = library();
        let song = library.by_uri("music/loose.mp3").unwrap();
        assert_eq!(song.track.title, "Avril 14th");
        assert_eq!(library.key_of("music/loose.mp3"), "/music/loose.mp3");
        assert_eq!(library.key_of("ytm-song:abc"), "ytm-song:abc");
        assert_eq!(library.key_of("not-held"), "not-held");
    }

    #[test]
    fn album_tags_come_from_the_album_row() {
        let library = library();
        let song = library.by_uri("music/boc/geogaddi/01.flac").unwrap();
        assert_eq!(song.tag(Tag::AlbumArtist), "Boards of Canada");
        assert_eq!(song.tag(Tag::Genre), "Electronic");
        assert_eq!(song.tag(Tag::Date), "2002");
    }

    #[test]
    fn directories_list_their_children_only() {
        let library = library();
        let (dirs, files) = library.list("").unwrap();
        assert_eq!(dirs.into_iter().collect::<Vec<_>>(), ["music"]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].uri, "ytm-song:abc");

        let (dirs, files) = library.list("music").unwrap();
        assert_eq!(dirs.into_iter().collect::<Vec<_>>(), ["music/boc"]);
        assert_eq!(files[0].uri, "music/loose.mp3");

        assert!(library.list("mus").is_none());
        assert_eq!(library.under("music/boc").count(), 2);
    }

    #[test]
    fn tag_names_ignore_case_and_known_extras_are_empty() {
        assert_eq!(Tag::parse("albumartist"), Some(Tag::AlbumArtist));
        assert_eq!(Tag::parse("Composer"), Some(Tag::Unsupported));
        assert_eq!(Tag::parse("nonsense"), None);
    }
}
