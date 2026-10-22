use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use config::{MusicService, Source};
use db::Db;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;

use super::{
    AlbumType, ArtistView, AuthOutcome, Capabilities, FavoritesSync, LibraryActions,
    LibrarySnapshot, MediaSource, PlaylistOps, RadioSeeds, SourceError, StreamInfo,
};
use crate::dlna::{self, Device, didl};
use crate::server_ops::ServerConn;

const PAGE: u32 = 500;
/// A walk stops here rather than following a server's container loop forever.
const MAX_DEPTH: usize = 32;
const MAX_OBJECTS: usize = 200_000;

/// Item ids are the server's object ids; the stream URL is looked up at play
/// time, since a server's address can change between syncs.
pub(super) struct DlnaSource {
    db: Db,
    source: Source,
    configured: String,
    device: OnceCell<Device>,
}

impl DlnaSource {
    pub(super) fn new(db: Db, source: Source, conn: &ServerConn) -> Self {
        Self {
            db,
            source,
            configured: conn.url.clone(),
            device: OnceCell::new(),
        }
    }

    async fn device(&self) -> Result<&Device, SourceError> {
        self.device
            .get_or_try_init(|| async {
                let device = dlna::locate(&dlna::client()?, &self.configured).await?;
                tracing::info!(name = %device.name, location = %device.location, "DLNA server found");
                Ok(device)
            })
            .await
    }

    /// Every audio item, by `Search` where the server has it and by walking
    /// the container tree where it does not.
    async fn audio_items(
        &self,
        on_progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<Vec<didl::Item>, SourceError> {
        let device = self.device().await?;
        let client = dlna::client()?;
        match search_all(device, &client).await {
            Ok(items) => return Ok(items),
            Err(error) => {
                tracing::info!(%error, "DLNA search unavailable; walking the folders instead");
            }
        }
        let mut items = Vec::new();
        let mut pending = VecDeque::from([("0".to_string(), 0)]);
        let mut visited = HashSet::new();
        let mut objects = 0;
        while let Some((container, depth)) = pending.pop_front() {
            if depth > MAX_DEPTH || !visited.insert(container.clone()) {
                continue;
            }
            let mut start = 0;
            loop {
                let page = device
                    .browse_children(&client, &container, start, PAGE)
                    .await?;
                objects += page.returned as usize;
                if objects > MAX_OBJECTS {
                    return Err(SourceError::Backend(format!(
                        "DLNA library exceeds {MAX_OBJECTS} entries"
                    )));
                }
                for child in page.didl.containers {
                    on_progress(child.title);
                    pending.push_back((child.id, depth + 1));
                }
                items.extend(page.didl.items.into_iter().filter(didl::Item::is_audio));
                start += page.returned;
                if page.returned == 0 || (page.total > 0 && start >= page.total) {
                    break;
                }
            }
        }
        Ok(items)
    }
}

async fn search_all(
    device: &Device,
    client: &reqwest::Client,
) -> Result<Vec<didl::Item>, SourceError> {
    let mut items = Vec::new();
    let mut start = 0;
    loop {
        let page = device.search_audio(client, start, PAGE).await?;
        items.extend(page.didl.items.into_iter().filter(didl::Item::is_audio));
        start += page.returned;
        if page.returned == 0 || (page.total > 0 && start >= page.total) {
            return Ok(items);
        }
        if start as usize > MAX_OBJECTS {
            return Err(SourceError::Backend(format!(
                "DLNA library exceeds {MAX_OBJECTS} entries"
            )));
        }
    }
}

#[async_trait]
impl MediaSource for DlnaSource {
    fn source(&self) -> &Source {
        &self.source
    }

    fn db(&self) -> &Db {
        &self.db
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            edit_tags: false,
            delete_from_disk: false,
            scan_folders: false,
            folders: false,
            browse_folders: false,
            external_devices: false,
            browser_playback: false,
            accounts: false,
            sync: true,
            downloads: true,
            uploads: false,
            storage_quota: false,
            discover: false,
            dont_recommend: false,
            music_videos: false,
            radio: RadioSeeds::NONE,
            play_queue: false,
            playlists: PlaylistOps::None,
            artist_view: ArtistView::Library,
            albums: AlbumType::Standard,
            favorites_sync: FavoritesSync::Instant,
            account_avatar: false,
            stream_quality: false,
            explicit_flags: false,
            watch_history: false,
            library_actions: LibraryActions::NONE,
        }
    }

    async fn fetch_library(&self) -> Result<LibrarySnapshot, SourceError> {
        self.fetch_library_with_progress(Arc::new(|_| {})).await
    }

    async fn fetch_library_with_progress(
        &self,
        on_progress: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<LibrarySnapshot, SourceError> {
        let items = self.audio_items(on_progress.as_ref()).await?;
        let snapshot = snapshot(items);
        tracing::info!(total_tracks = snapshot.tracks.len(), "DLNA library read");
        Ok(snapshot)
    }

    async fn resolve_stream(
        &self,
        item_id: &str,
        _: config::StreamQuality,
    ) -> Result<StreamInfo, SourceError> {
        let device = self.device().await?;
        let item = device
            .metadata(&dlna::client()?, item_id)
            .await?
            .ok_or_else(|| SourceError::InvalidInput(format!("no DLNA item {item_id}")))?;
        let stream = item.stream().ok_or_else(|| {
            SourceError::Backend(format!("DLNA item {item_id} offers no audio stream"))
        })?;
        Ok(StreamInfo {
            url: stream.url.clone(),
            format: None,
            user_agent: None,
            duration_secs: stream.duration_ms.map(|ms| ms / 1000),
            bitrate: stream.bitrate.map(|bytes| bytes * 8 / 1000),
            content_length: stream.size,
            loudness_db: None,
        })
    }

    async fn validate(&self) -> AuthOutcome {
        match self.device().await {
            Ok(_) => AuthOutcome::Valid,
            Err(_) => AuthOutcome::Unreachable,
        }
    }

    async fn fetch_favorites(&self) -> Result<Vec<String>, SourceError> {
        self.db
            .favorites(self.source.as_str())
            .await
            .map_err(SourceError::from)
    }

    async fn push_favorite(&self, _: &str, _: bool) -> Result<(), SourceError> {
        Ok(())
    }

    async fn add_to_playlist(&self, _: &str, _: &[String]) -> Result<Vec<String>, SourceError> {
        Err(SourceError::unsupported("playlists"))
    }

    async fn create_playlist(&self, _: &str, _: &[String]) -> Result<String, SourceError> {
        Err(SourceError::unsupported("playlists"))
    }

    async fn remove_from_playlist(
        &self,
        _: &str,
        _: &reader::Track,
        _: usize,
    ) -> Result<(), SourceError> {
        Err(SourceError::unsupported("playlists"))
    }
}

/// Servers list one file under several views (by artist, by album, by
/// folder), so the same stream URL is kept once.
fn snapshot(items: Vec<didl::Item>) -> LibrarySnapshot {
    let mut seen = HashSet::new();
    let mut albums = BTreeMap::new();
    let mut tracks = Vec::new();
    for item in items {
        let Some(stream) = item.stream() else {
            continue;
        };
        if !seen.insert(stream.url.clone()) {
            continue;
        }
        let artist = item
            .artist
            .clone()
            .or_else(|| item.creator.clone())
            .unwrap_or_default();
        let album_artist = item.album_artist.clone().unwrap_or_else(|| artist.clone());
        let album = item.album.clone().unwrap_or_default();
        let album_id = album_id(&album_artist, &album);
        let cover = item.album_art.as_deref().map(reader::CoverRef::encode_url);
        albums
            .entry(album_id.clone())
            .or_insert_with(|| reader::Album {
                id: album_id.clone(),
                title: album.clone(),
                artist: album_artist.clone(),
                genre: item.genre.clone().unwrap_or_default(),
                year: item.year(),
                cover_path: cover.as_ref().map(std::path::PathBuf::from),
                manual_cover: false,
                artist_id: None,
                artist_key: None,
            });
        tracks.push(reader::Track {
            id: reader::TrackId::Server {
                service: MusicService::Dlna,
                item_id: item.id.clone(),
            },
            cover,
            album_id,
            title: item.title.clone(),
            artist: artist.clone(),
            album,
            duration: stream.duration_ms.map_or(0, |ms| ms / 1000),
            khz: stream.sample_rate.map_or(0, |rate| rate / 1000),
            bitrate: stream
                .bitrate
                .map_or(0, |bytes| (bytes * 8 / 1000).min(u16::MAX as u32) as u16),
            track_number: item.track_number,
            disc_number: None,
            musicbrainz_release_id: None,
            musicbrainz_recording_id: None,
            musicbrainz_track_id: None,
            playlist_item_id: None,
            artists: if artist.is_empty() {
                Vec::new()
            } else {
                vec![artist]
            },
            replay_gain: config::ReplayGainInfo::default(),
            counterpart: None,
            credits: Vec::new(),
            explicit: false,
            plays: None,
        });
    }
    LibrarySnapshot {
        albums: albums.into_values().collect(),
        tracks,
        artists: Vec::new(),
    }
}

/// An album is its billing and title; servers give it no id of its own that
/// survives a rescan.
fn album_id(artist: &str, title: &str) -> String {
    let digest = Sha256::digest(format!(
        "{}\0{}",
        artist.to_lowercase(),
        title.to_lowercase()
    ));
    reader::CoverRef::stored_item_ref(MusicService::Dlna, &hex::encode(&digest[..12]), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, url: &str, album: &str) -> didl::Item {
        didl::Item {
            id: id.into(),
            title: format!("title {id}"),
            class: "object.item.audioItem.musicTrack".into(),
            artist: Some("Ada".into()),
            album: Some(album.into()),
            album_art: Some("http://nas/art.jpg".into()),
            resources: vec![didl::Resource {
                url: url.into(),
                protocol_info: "http-get:*:audio/flac:*".into(),
                duration_ms: Some(61_000),
                bitrate: Some(100_000),
                sample_rate: Some(48_000),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn one_file_listed_under_several_views_is_one_track() {
        let snapshot = snapshot(vec![
            item("1$7", "http://nas/a.flac", "One"),
            item("4$2", "http://nas/a.flac", "One"),
            item("1$8", "http://nas/b.flac", "Two"),
        ]);
        let ids: Vec<_> = snapshot
            .tracks
            .iter()
            .map(|t| t.id.key().into_owned())
            .collect();
        assert_eq!(ids, ["1$7", "1$8"]);
        assert_eq!(snapshot.albums.len(), 2);
    }

    #[test]
    fn a_track_carries_its_stream_facts_and_cover() {
        let snapshot = snapshot(vec![item("1$7", "http://nas/a.flac", "One")]);
        let track = &snapshot.tracks[0];
        assert_eq!(track.duration, 61);
        assert_eq!(track.khz, 48);
        assert_eq!(track.bitrate, 800);
        assert_eq!(track.album_id, snapshot.albums[0].id);
        assert!(track.album_id.starts_with("dlna:"));
        assert_eq!(
            reader::CoverRef::for_track(track),
            reader::CoverRef::EmbeddedUrl("http://nas/art.jpg".into())
        );
    }

    #[test]
    fn albums_group_by_billing_and_title_ignoring_case() {
        assert_eq!(album_id("Ada", "One"), album_id("ada", "ONE"));
        assert_ne!(album_id("Ada", "One"), album_id("Ada", "Two"));
    }
}
