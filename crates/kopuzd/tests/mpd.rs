//! The MPD server over a real daemon core, spoken to as an MPD client would.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use api::{ApiError, KopuzApi, QueueContext};
use daemon::session::FactoryOverride;
use daemon::{LibraryService, LocalApi, PlaybackServices, QueueMaterializer, SessionHandle};
use player::engine::{NullSink, SourceFactory};
use player::player::Player;
use reader::Track;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

struct StubLibrary;

#[async_trait::async_trait]
impl QueueMaterializer for StubLibrary {
    async fn materialize(&self, context: &QueueContext) -> Result<Vec<Track>, ApiError> {
        match context {
            QueueContext::Tracks { keys } => Ok(keys
                .iter()
                .filter_map(|key| seeded().into_iter().find(|track| track.id.key() == *key))
                .collect()),
            _ => Err(ApiError::unsupported("stub resolves raw tracks only")),
        }
    }
}

fn seeded() -> [Track; 2] {
    [
        track("/lib/ann/alpha.flac", "Alpha", "Ann"),
        track("/lib/bob/beta.flac", "Beta", "Bob"),
    ]
}

fn track(key: &str, title: &str, artist: &str) -> Track {
    Track {
        id: reader::models::TrackId::Local(std::path::PathBuf::from(key)),
        cover: None,
        album_id: String::new(),
        title: title.to_string(),
        artist: artist.to_string(),
        album: String::new(),
        duration: 6,
        khz: 44,
        bitrate: 320,
        track_number: None,
        disc_number: None,
        musicbrainz_release_id: None,
        musicbrainz_recording_id: None,
        musicbrainz_track_id: None,
        playlist_item_id: None,
        artists: vec![],
        replay_gain: config::ReplayGainInfo::default(),
        counterpart: None,
        credits: vec![],
        explicit: false,
        plays: None,
    }
}

fn wav_factory(seconds: u64) -> SourceFactory {
    let sample_rate: u32 = 44_100;
    let data_len = seconds as usize * sample_rate as usize * 4;
    let mut bytes = Vec::with_capacity(44 + data_len);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
    bytes.resize(44 + data_len, 0);
    Box::new(move || Ok(player::decoder::from_stream(Cursor::new(bytes.clone()))))
}

struct Daemon {
    address: String,
    _dir: tempfile::TempDir,
}

async fn spawn(password: Option<&str>) -> Daemon {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = db::init(&dir.path().join("mpd.db")).await.expect("db init");
    database
        .upsert_tracks(&config::Source::default(), &seeded())
        .await
        .expect("seed tracks");
    let credentials = daemon::CredentialStore::load(&database)
        .await
        .expect("credentials");
    let library = Arc::new(LibraryService::new(
        database.clone(),
        config::Source::default(),
        Arc::new(radio::registry::StationRegistry::default()),
        dir.path().join("covers"),
        credentials,
    ));
    let player = Player::try_with_sink(Box::new(NullSink::new())).expect("headless player");
    let provider: FactoryOverride = Arc::new(|_| Some(wav_factory(30)));
    let session = SessionHandle::spawn_with_factory(
        Arc::new(StubLibrary),
        player,
        PlaybackServices::default(),
        provider,
    );
    library.attach_session(session.clone());
    let playlists = daemon::PlaylistService::new(database.clone(), session.clone());
    let api: Arc<dyn KopuzApi> = Arc::new(
        LocalApi::new(session)
            .with_library(library)
            .with_playlists(playlists),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let address = listener.local_addr().expect("address").to_string();
    let options = mpd::Options {
        password: password.map(str::to_string),
    };
    tokio::spawn(mpd::serve(listener, api, options));
    Daemon { address, _dir: dir }
}

struct Client {
    read: BufReader<OwnedReadHalf>,
    write: OwnedWriteHalf,
}

impl Client {
    async fn connect(daemon: &Daemon) -> Client {
        let stream = TcpStream::connect(&daemon.address).await.expect("connect");
        let (read, write) = stream.into_split();
        let mut client = Client {
            read: BufReader::new(read),
            write,
        };
        let greeting = client.line().await;
        assert!(greeting.starts_with("OK MPD "), "{greeting}");
        client
    }

    async fn line(&mut self) -> String {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(10), self.read.read_line(&mut line))
            .await
            .expect("a reply in time")
            .expect("read");
        line
    }

    async fn send(&mut self, command: &str) {
        self.write
            .write_all(format!("{command}\n").as_bytes())
            .await
            .expect("write");
    }

    /// The lines of one answer, its closing `OK` or `ACK` last.
    async fn ask(&mut self, command: &str) -> Vec<String> {
        self.send(command).await;
        self.answer().await
    }

    async fn answer(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = self.line().await;
            let line = line.trim_end_matches('\n').to_string();
            let done = line == "OK" || line.starts_with("ACK ");
            lines.push(line);
            if done {
                return lines;
            }
        }
    }

    async fn ok(&mut self, command: &str) -> Vec<String> {
        let mut lines = self.ask(command).await;
        assert_eq!(lines.pop().as_deref(), Some("OK"), "{command}: {lines:?}");
        lines
    }

    async fn field(&mut self, command: &str, key: &str) -> Option<String> {
        let prefix = format!("{key}: ");
        self.ok(command)
            .await
            .into_iter()
            .find_map(|line| line.strip_prefix(&prefix).map(str::to_string))
    }

    async fn wait_field(&mut self, command: &str, key: &str, want: &str) {
        for _ in 0..100 {
            if self.field(command, key).await.as_deref() == Some(want) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("{command} never reported {key}: {want}");
    }
}

#[tokio::test]
async fn the_library_reads_as_an_mpd_database() {
    let daemon = spawn(None).await;
    let mut client = Client::connect(&daemon).await;

    let found = client.ok(r#"find "(Artist == 'Ann')""#).await;
    assert!(
        found.contains(&"file: lib/ann/alpha.flac".to_string()),
        "{found:?}"
    );
    assert!(found.contains(&"Title: Alpha".to_string()), "{found:?}");
    assert!(!found.iter().any(|line| line.contains("Beta")), "{found:?}");

    let searched = client.ok("search title et").await;
    assert!(
        searched.contains(&"Title: Beta".to_string()),
        "{searched:?}"
    );

    assert_eq!(
        client.ok("list artist").await,
        ["Artist: Ann", "Artist: Bob"]
    );
    assert_eq!(client.ok("lsinfo").await, ["directory: lib"]);
    assert_eq!(
        client.ok("lsinfo lib").await,
        ["directory: lib/ann", "directory: lib/bob"]
    );
    assert_eq!(client.field("stats", "songs").await.as_deref(), Some("2"));
}

#[tokio::test]
async fn the_queue_and_transport_follow_mpd_commands() {
    let daemon = spawn(None).await;
    let mut client = Client::connect(&daemon).await;

    client.ok("add lib/ann/alpha.flac").await;
    assert_eq!(client.ok("addid lib/bob/beta.flac 0").await, ["Id: 0"]);
    let queue = client.ok("playlistinfo").await;
    let files: Vec<_> = queue
        .iter()
        .filter(|line| line.starts_with("file: "))
        .collect();
    assert_eq!(
        files,
        ["file: lib/bob/beta.flac", "file: lib/ann/alpha.flac"]
    );
    client.wait_field("status", "playlistlength", "2").await;

    client.ok("play 1").await;
    client.wait_field("status", "state", "play").await;
    assert_eq!(client.field("status", "song").await.as_deref(), Some("1"));
    assert_eq!(
        client.field("currentsong", "Title").await.as_deref(),
        Some("Alpha")
    );

    client.ok("setvol 40").await;
    client.wait_field("status", "volume", "40").await;
    client.ok("pause 1").await;
    client.wait_field("status", "state", "pause").await;

    client.ok("stop").await;
    client.ok("clear").await;
    client.wait_field("status", "playlistlength", "0").await;
}

#[tokio::test]
async fn idle_wakes_on_a_change_another_client_makes() {
    let daemon = spawn(None).await;
    let mut idler = Client::connect(&daemon).await;
    let mut actor = Client::connect(&daemon).await;

    idler.send("idle playlist").await;
    actor.ok("add lib/ann/alpha.flac").await;
    assert_eq!(idler.answer().await, ["changed: playlist", "OK"]);

    idler.send("idle mixer").await;
    idler.send("noidle").await;
    assert_eq!(idler.answer().await, ["OK"]);
}

#[tokio::test]
async fn command_lists_and_errors_answer_as_mpd_does() {
    let daemon = spawn(None).await;
    let mut client = Client::connect(&daemon).await;

    client.send("command_list_ok_begin").await;
    client.send("ping").await;
    client.send("getvol").await;
    client.send("command_list_end").await;
    let answer = client.answer().await;
    assert_eq!(answer[0], "list_OK");
    assert!(answer[1].starts_with("volume: "), "{answer:?}");
    assert_eq!(&answer[2..], ["list_OK", "OK"]);

    client.send("command_list_begin").await;
    client.send("ping").await;
    client.send("frobnicate").await;
    client.send("ping").await;
    client.send("command_list_end").await;
    assert_eq!(
        client.answer().await,
        [r#"ACK [5@1] {frobnicate} unknown command "frobnicate""#]
    );

    let missing = client.ask("add lib/nowhere.flac").await;
    assert!(missing[0].starts_with("ACK [50@0] {add}"), "{missing:?}");
}

#[tokio::test]
async fn a_password_gates_everything_but_the_handshake() {
    let daemon = spawn(Some("hunter2")).await;
    let mut client = Client::connect(&daemon).await;

    client.ok("ping").await;
    let refused = client.ask("status").await;
    assert!(refused[0].starts_with("ACK [4@0] {status}"), "{refused:?}");
    let wrong = client.ask("password nope").await;
    assert!(wrong[0].starts_with("ACK [3@0] {password}"), "{wrong:?}");
    client.ok("password hunter2").await;
    client.ok("status").await;
}
