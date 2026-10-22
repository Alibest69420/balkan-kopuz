//! The command table: each MPD command as calls on the Kopuz API.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use api::{
    ArtworkData, ArtworkRequest, JobKind, LoopMode, Phase, PlayerCommand, PlayerState,
    PlaylistInfo, PlaylistReorder, QueueContext, QueueEdit, QueueMode, QueueSnapshot,
    SetQueueRequest,
};
use tokio::sync::broadcast;

use crate::Shared;
use crate::filter::{self, Filter, Op, Trailing};
use crate::idle;
use crate::library::{Library, Song, Tag};
use crate::protocol::{self, Ack, AckCode, Response, flag, number, range, seconds};

/// Every command this server answers, for `commands`.
const COMMANDS: &[&str] = &[
    "add",
    "addid",
    "albumart",
    "binarylimit",
    "channels",
    "clear",
    "clearerror",
    "close",
    "commands",
    "consume",
    "count",
    "crossfade",
    "currentsong",
    "decoders",
    "delete",
    "deleteid",
    "disableoutput",
    "enableoutput",
    "find",
    "findadd",
    "getvol",
    "idle",
    "list",
    "listall",
    "listallinfo",
    "listplaylist",
    "listplaylistinfo",
    "listplaylists",
    "load",
    "lsinfo",
    "move",
    "moveid",
    "next",
    "noidle",
    "notcommands",
    "outputs",
    "password",
    "pause",
    "ping",
    "play",
    "playid",
    "playlist",
    "playlistadd",
    "playlistclear",
    "playlistdelete",
    "playlistfind",
    "playlistid",
    "playlistinfo",
    "playlistmove",
    "playlistsearch",
    "plchanges",
    "plchangesposid",
    "previous",
    "random",
    "readmessages",
    "readpicture",
    "rename",
    "repeat",
    "replay_gain_status",
    "rescan",
    "rm",
    "save",
    "search",
    "searchadd",
    "seek",
    "seekcur",
    "seekid",
    "setvol",
    "shuffle",
    "single",
    "stats",
    "status",
    "stop",
    "swap",
    "swapid",
    "tagtypes",
    "toggleoutput",
    "update",
    "urlhandlers",
    "volume",
];

/// Answered before `password` when the server has one.
const OPEN: &[&str] = &[
    "binarylimit",
    "close",
    "commands",
    "notcommands",
    "password",
    "ping",
    "tagtypes",
];

/// MPD's default and its floor for `binarylimit`.
const BINARY_LIMIT: usize = 8192;
const MIN_BINARY_LIMIT: usize = 64;

/// The fixed timestamp stored playlists report, which Kopuz does not keep.
const EPOCH: &str = "1970-01-01T00:00:00Z";

pub(crate) struct Client {
    shared: Arc<Shared>,
    authenticated: bool,
    /// Subsystems changed since this client's last `idle` reported them.
    pub pending: u16,
    /// Which of [`Tag::SUPPORTED`] songs print, by index; `tagtypes` edits it.
    tags: u16,
    binary_limit: usize,
    /// The cover the last `albumart` / `readpicture` read, which clients
    /// fetch in chunks with one command per chunk.
    picture: Option<(String, Arc<ArtworkData>)>,
}

impl Client {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            authenticated: shared.password.is_none(),
            shared,
            pending: 0,
            tags: u16::MAX,
            binary_limit: BINARY_LIMIT,
            picture: None,
        }
    }

    pub fn absorb(&mut self, changes: &mut broadcast::Receiver<u16>) {
        loop {
            match changes.try_recv() {
                Ok(bits) => self.pending |= bits,
                Err(broadcast::error::TryRecvError::Lagged(_)) => self.pending |= idle::ALL,
                Err(_) => return,
            }
        }
    }

    /// The `changed:` lines for what changed in `mask`, which are then no longer pending.
    pub fn take_changed(&mut self, mask: u16) -> String {
        let hit = self.pending & mask;
        self.pending &= !hit;
        let mut answer: String = idle::names(hit)
            .map(|name| format!("changed: {name}\n"))
            .collect();
        answer.push_str("OK\n");
        answer
    }

    pub fn check(&self, words: &[String]) -> Result<(), Ack> {
        let name = words.first().map_or("", String::as_str);
        if self.authenticated || OPEN.contains(&name) {
            return Ok(());
        }
        Err(Ack::new(
            AckCode::Permission,
            format!("you don't have permission for \"{name}\""),
        ))
    }

    pub async fn run_one(&mut self, words: &[String], index: usize) -> Vec<u8> {
        let mut out = Response::default();
        match self.execute(words, &mut out).await {
            Ok(()) => {
                out.bytes.extend_from_slice(b"OK\n");
                out.bytes
            }
            Err(ack) => ack.render(index, &words[0]).into_bytes(),
        }
    }

    pub async fn run_list(&mut self, lines: &[String], ok_each: bool) -> Vec<u8> {
        let mut all = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let words = match protocol::tokenize(line) {
                Ok(words) if !words.is_empty() => words,
                Ok(_) => {
                    let ack = Ack::new(AckCode::Unknown, "No command given");
                    all.extend(ack.render(index, "").into_bytes());
                    return all;
                }
                Err(ack) => {
                    all.extend(ack.render(index, "").into_bytes());
                    return all;
                }
            };
            if matches!(
                words[0].as_str(),
                "idle" | "noidle" | "close" | "command_list_begin" | "command_list_ok_begin"
            ) {
                let ack = Ack::new(AckCode::NotList, "not allowed in a command list");
                all.extend(ack.render(index, &words[0]).into_bytes());
                return all;
            }
            let mut out = Response::default();
            match self.execute(&words, &mut out).await {
                Ok(()) => {
                    all.extend(out.bytes);
                    if ok_each {
                        all.extend_from_slice(b"list_OK\n");
                    }
                }
                Err(ack) => {
                    all.extend(ack.render(index, &words[0]).into_bytes());
                    return all;
                }
            }
        }
        all.extend_from_slice(b"OK\n");
        all
    }

    async fn execute(&mut self, words: &[String], out: &mut Response) -> Result<(), Ack> {
        self.check(words)?;
        let name = words[0].as_str();
        let args = &words[1..];
        let arg = |index: usize| {
            args.get(index)
                .map(String::as_str)
                .ok_or_else(|| Ack::arg(format!("too few arguments for \"{name}\"")))
        };
        let api = self.shared.api.clone();
        match name {
            "ping" | "clearerror" => Ok(()),
            "password" => {
                if self.shared.password.as_deref() == Some(arg(0)?) {
                    self.authenticated = true;
                    Ok(())
                } else {
                    Err(Ack::new(AckCode::Password, "incorrect password"))
                }
            }
            "binarylimit" => {
                self.binary_limit = (number(arg(0)?)? as usize).max(MIN_BINARY_LIMIT);
                Ok(())
            }
            "commands" => {
                for command in COMMANDS {
                    out.field("command", command);
                }
                Ok(())
            }
            "notcommands" | "decoders" | "urlhandlers" | "channels" | "readmessages" => Ok(()),
            "tagtypes" => self.tagtypes(args, out),

            "status" => self.status(out).await,
            "currentsong" => {
                let state = api.player_state().await?;
                let library = self.shared.library().await?;
                if let (Some(track), Some(position)) = (state.track, state.queue.index) {
                    self.print_song(out, &library.song(track), Some(position));
                }
                Ok(())
            }
            "stats" => {
                let library = self.shared.library().await?;
                out.field("artists", library.distinct(Tag::Artist));
                out.field("albums", library.distinct(Tag::Album));
                out.field("songs", library.songs.len());
                out.field("uptime", self.shared.started.elapsed().as_secs());
                out.field("db_playtime", library.playtime_secs());
                out.field("db_update", 0);
                out.field("playtime", 0);
                Ok(())
            }
            "replay_gain_status" => {
                out.field("replay_gain_mode", "off");
                Ok(())
            }
            "outputs" => {
                let state = api.player_state().await?;
                out.field("outputid", 0);
                out.field("outputname", "Kopuz");
                out.field("plugin", "kopuz");
                out.field("outputenabled", u8::from(!state.muted));
                Ok(())
            }
            // The one output is the player itself, and turning it off mutes it.
            "enableoutput" | "disableoutput" | "toggleoutput" => {
                if number(arg(0)?)? != 0 {
                    return Err(Ack::no_exist("No such audio output"));
                }
                let muted = match name {
                    "enableoutput" => false,
                    "disableoutput" => true,
                    _ => !api.player_state().await?.muted,
                };
                api.player_command(PlayerCommand::SetMuted { muted })
                    .await?;
                Ok(())
            }

            "play" | "playid" => {
                match args.first() {
                    Some(word) if word != "-1" => {
                        api.queue_edit(QueueEdit::Jump {
                            index: number(word)?,
                        })
                        .await?;
                    }
                    _ => {
                        let state = api.player_state().await?;
                        if state.queue.index.is_none() && state.queue.length > 0 {
                            api.queue_edit(QueueEdit::Jump { index: 0 }).await?;
                        } else {
                            api.player_command(PlayerCommand::Play).await?;
                        }
                    }
                }
                Ok(())
            }
            "pause" => {
                let command = match args.first() {
                    None => PlayerCommand::Toggle,
                    Some(word) if flag(word)? => PlayerCommand::Pause,
                    Some(_) => PlayerCommand::Play,
                };
                api.player_command(command).await?;
                Ok(())
            }
            "stop" => self.command(PlayerCommand::Stop).await,
            "next" => self.command(PlayerCommand::Next).await,
            "previous" => self.command(PlayerCommand::Previous).await,
            "seek" | "seekid" => {
                let index = number(arg(0)?)?;
                let position_ms = seconds(arg(1)?)?;
                if api.player_state().await?.queue.index != Some(index) {
                    api.queue_edit(QueueEdit::Jump { index }).await?;
                }
                self.command(PlayerCommand::Seek { position_ms }).await
            }
            "seekcur" => {
                let word = arg(0)?;
                let position_ms =
                    match word.as_bytes().first() {
                        Some(b'+') => elapsed_ms(&api.player_state().await?)
                            .saturating_add(seconds(&word[1..])?),
                        Some(b'-') => elapsed_ms(&api.player_state().await?)
                            .saturating_sub(seconds(&word[1..])?),
                        _ => seconds(word)?,
                    };
                self.command(PlayerCommand::Seek { position_ms }).await
            }
            "setvol" => {
                let percent = number(arg(0)?)?;
                if percent > 100 {
                    return Err(Ack::arg("Invalid volume value"));
                }
                self.set_volume(percent as i64).await
            }
            "volume" => {
                let change: i64 = arg(0)?.parse().map_err(|_| Ack::arg("Integer expected"))?;
                let state = api.player_state().await?;
                self.set_volume(percent(&state) as i64 + change).await
            }
            "getvol" => {
                out.field("volume", percent(&api.player_state().await?));
                Ok(())
            }
            "random" => {
                let shuffle = flag(arg(0)?)?;
                self.command(PlayerCommand::SetMode {
                    shuffle: Some(shuffle),
                    loop_mode: None,
                })
                .await
            }
            // Kopuz loops the queue or the track; MPD's repeat and single
            // flags spell the same two modes, and single alone (stop after
            // this song) has no counterpart, so it loops the track too.
            "repeat" | "single" => {
                let on = match arg(0)? {
                    "oneshot" => return Err(Ack::unsupported("single oneshot")),
                    word => flag(word)?,
                };
                let current = api.player_state().await?.queue.loop_mode;
                let loop_mode = match (name, on, current) {
                    ("repeat", true, LoopMode::Track) | ("single", true, _) => LoopMode::Track,
                    ("repeat", true, _) => LoopMode::Queue,
                    ("single", false, LoopMode::Track) => LoopMode::Queue,
                    ("single", false, other) => other,
                    _ => LoopMode::None,
                };
                self.command(PlayerCommand::SetMode {
                    shuffle: None,
                    loop_mode: Some(loop_mode),
                })
                .await
            }
            "consume" | "crossfade" => match arg(0)? {
                "0" => Ok(()),
                _ => Err(Ack::unsupported(name)),
            },

            "add" | "addid" => {
                let uri = arg(0)?;
                let library = self.shared.library().await?;
                let keys = if name == "addid" {
                    vec![library.key_of(uri).to_string()]
                } else {
                    keys_at(&library, uri)
                };
                let index = self.insert_at(args.get(1).map(String::as_str)).await?;
                self.insert(index, keys).await?;
                if name == "addid" {
                    out.field("Id", index);
                }
                Ok(())
            }
            "clear" => {
                api.set_queue(SetQueueRequest {
                    mode: QueueMode::Replace,
                    context: QueueContext::Tracks { keys: Vec::new() },
                    start_index: None,
                    shuffle: None,
                })
                .await?;
                Ok(())
            }
            "delete" | "deleteid" => {
                let len = api.player_state().await?.queue.length;
                let doomed = range(arg(0)?, len)?;
                if doomed.is_empty() {
                    return Err(Ack::arg("Bad song index"));
                }
                for index in doomed.rev() {
                    api.queue_edit(QueueEdit::Remove { index }).await?;
                }
                Ok(())
            }
            "move" | "moveid" => {
                let len = api.player_state().await?.queue.length;
                let from = range(arg(0)?, len)?;
                let to = number(arg(1)?)?;
                let count = from.end - from.start;
                if count == 0 || to + count > len {
                    return Err(Ack::arg("Bad song index"));
                }
                for step in 0..count {
                    let edit = if to > from.start {
                        QueueEdit::Move {
                            from: from.start,
                            to: to + count - 1,
                        }
                    } else {
                        QueueEdit::Move {
                            from: from.start + step,
                            to: to + step,
                        }
                    };
                    api.queue_edit(edit).await?;
                }
                Ok(())
            }
            "swap" | "swapid" => {
                let (a, b) = (number(arg(0)?)?, number(arg(1)?)?);
                if a != b {
                    let (low, high) = (a.min(b), a.max(b));
                    api.queue_edit(QueueEdit::Move {
                        from: low,
                        to: high,
                    })
                    .await?;
                    api.queue_edit(QueueEdit::Move {
                        from: high - 1,
                        to: low,
                    })
                    .await?;
                }
                Ok(())
            }
            "shuffle" => {
                api.queue_edit(QueueEdit::ShuffleUpcoming).await?;
                Ok(())
            }
            "playlist" => {
                let queue = api.queue_snapshot().await?;
                let library = self.shared.library().await?;
                for (index, track) in queue.items.into_iter().enumerate() {
                    out.field(
                        &index.to_string(),
                        format!("file: {}", library.song(track).uri),
                    );
                }
                Ok(())
            }
            "playlistinfo" | "playlistid" => {
                let queue = api.queue_snapshot().await?;
                let window = match args.first() {
                    Some(word) => {
                        let window = range(word, queue.items.len() as u32)?;
                        if window.is_empty() && name == "playlistid" {
                            return Err(Ack::no_exist("No such song"));
                        }
                        window
                    }
                    None => 0..queue.items.len() as u32,
                };
                self.print_queue(out, queue, window, false).await
            }
            "plchanges" | "plchangesposid" => {
                let since: u64 = arg(0)?.parse().map_err(|_| Ack::arg("Integer expected"))?;
                let queue = api.queue_snapshot().await?;
                // The daemon keeps no change log, so any other version means
                // the whole queue; clients just read more than they needed.
                if since == queue.rev {
                    return Ok(());
                }
                let len = queue.items.len() as u32;
                let window = match args.get(1) {
                    Some(word) => range(word, len)?,
                    None => 0..len,
                };
                self.print_queue(out, queue, window, name == "plchangesposid")
                    .await
            }
            "playlistfind" | "playlistsearch" => {
                let (legacy, fold) = if name == "playlistfind" {
                    (Op::Equals, false)
                } else {
                    (Op::Contains, true)
                };
                let (filter, _) = filter::parse(args, legacy)?;
                let filter = filter.ok_or_else(|| Ack::arg("filter expected"))?;
                let queue = api.queue_snapshot().await?;
                let library = self.shared.library().await?;
                for (index, track) in queue.items.into_iter().enumerate() {
                    let song = library.song(track);
                    if filter.matches(&song, fold) {
                        self.print_song(out, &song, Some(index as u32));
                    }
                }
                Ok(())
            }

            "find" | "search" | "findadd" | "searchadd" => {
                let (legacy, fold) = if name.starts_with("find") {
                    (Op::Equals, false)
                } else {
                    (Op::Contains, true)
                };
                let (filter, used) = filter::parse(args, legacy)?;
                let filter = filter.ok_or_else(|| Ack::arg("filter expected"))?;
                let options = filter::trailing(&args[used..])?;
                let library = self.shared.library().await?;
                let songs = select(&library, Some(&filter), fold, &options)?;
                if name.ends_with("add") {
                    let keys = songs.iter().map(|song| song.track.key.clone()).collect();
                    let index = self.insert_at(options.position.as_deref()).await?;
                    return self.insert(index, keys).await;
                }
                for song in songs {
                    self.print_song(out, song, None);
                }
                Ok(())
            }
            "count" => {
                let (filter, used) = filter::parse(args, Op::Equals)?;
                let options = filter::trailing(&args[used..])?;
                let library = self.shared.library().await?;
                let songs = select(&library, filter.as_ref(), false, &options)?;
                let mut groups: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                let group = options.groups.first().copied();
                for song in songs {
                    let key = group.map(|tag| song.tag(tag)).unwrap_or_default();
                    let entry = groups.entry(key).or_default();
                    entry.0 += 1;
                    entry.1 += song.duration_ms().unwrap_or(0) / 1000;
                }
                if groups.is_empty() && group.is_none() {
                    groups.insert(String::new(), (0, 0));
                }
                for (value, (songs, playtime)) in groups {
                    if let Some(tag) = group {
                        out.field(tag.name(), value);
                    }
                    out.field("songs", songs);
                    out.field("playtime", playtime);
                }
                Ok(())
            }
            "list" => self.list(args, out).await,
            "lsinfo" | "listall" | "listallinfo" => {
                let uri = args.first().map_or("", String::as_str);
                let library = self.shared.library().await?;
                if let Some(song) = library.by_uri(uri) {
                    if name == "listall" {
                        out.field("file", &song.uri);
                    } else {
                        self.print_song(out, song, None);
                    }
                    return Ok(());
                }
                if name == "lsinfo" {
                    let (dirs, files) = library
                        .list(uri)
                        .ok_or_else(|| Ack::no_exist("No such directory"))?;
                    for dir in dirs {
                        out.field("directory", dir);
                    }
                    for song in files {
                        self.print_song(out, song, None);
                    }
                    if uri.trim_matches('/').is_empty() {
                        for playlist in api.playlists().await?.playlists {
                            out.field("playlist", playlist.name);
                            out.field("Last-Modified", EPOCH);
                        }
                    }
                    return Ok(());
                }
                if library.list(uri).is_none() {
                    return Err(Ack::no_exist("No such directory"));
                }
                let mut seen = BTreeSet::new();
                let base = uri.trim_matches('/');
                for song in library.under(uri) {
                    let mut dir = song.uri.as_str();
                    let mut parents = Vec::new();
                    while let Some((parent, _)) = dir.rsplit_once('/') {
                        if parent.len() <= base.len() {
                            break;
                        }
                        parents.push(parent);
                        dir = parent;
                    }
                    for parent in parents.into_iter().rev() {
                        if seen.insert(parent.to_string()) {
                            out.field("directory", parent);
                        }
                    }
                    if name == "listall" {
                        out.field("file", &song.uri);
                    } else {
                        self.print_song(out, song, None);
                    }
                }
                Ok(())
            }
            "update" | "rescan" => {
                api.start_job(JobKind::Scan).await?;
                out.field("updating_db", 1);
                Ok(())
            }
            "albumart" | "readpicture" => {
                let uri = arg(0)?;
                let offset = number(arg(1)?)? as usize;
                let Some(picture) = self.picture(uri).await? else {
                    return if name == "albumart" {
                        Err(Ack::no_exist("No file exists"))
                    } else {
                        Ok(())
                    };
                };
                let size = picture.bytes.len();
                if offset > size {
                    return Err(Ack::arg("Bad file offset"));
                }
                let end = size.min(offset + self.binary_limit);
                out.field("size", size);
                if name == "readpicture" && !picture.content_type.is_empty() {
                    out.field("type", &picture.content_type);
                }
                out.binary(&picture.bytes[offset..end]);
                Ok(())
            }

            "listplaylists" => {
                for playlist in api.playlists().await?.playlists {
                    out.field("playlist", playlist.name);
                    out.field("Last-Modified", EPOCH);
                }
                Ok(())
            }
            "listplaylist" | "listplaylistinfo" => {
                let playlist = self.playlist(arg(0)?).await?;
                let library = self.shared.library().await?;
                let window = match args.get(1) {
                    Some(word) => range(word, playlist.track_keys.len() as u32)?,
                    None => 0..playlist.track_keys.len() as u32,
                };
                let keys = playlist.track_keys[window.start as usize..window.end as usize].to_vec();
                if name == "listplaylist" {
                    for key in keys {
                        out.field("file", crate::library::uri_of(&key));
                    }
                    return Ok(());
                }
                for track in api.tracks_by_keys(keys).await? {
                    self.print_song(out, &library.song(track), None);
                }
                Ok(())
            }
            "load" => {
                let playlist = self.playlist(arg(0)?).await?;
                let window = match args.get(1) {
                    Some(word) => range(word, playlist.track_keys.len() as u32)?,
                    None => 0..playlist.track_keys.len() as u32,
                };
                let keys = playlist.track_keys[window.start as usize..window.end as usize].to_vec();
                let index = self.insert_at(args.get(2).map(String::as_str)).await?;
                self.insert(index, keys).await
            }
            "playlistadd" => {
                let library = self.shared.library().await?;
                let keys = keys_at(&library, arg(1)?);
                match self.find_playlist(arg(0)?).await? {
                    Some(playlist) => api.add_playlist_tracks(playlist.id, keys).await?,
                    None => {
                        api.create_playlist(arg(0)?.to_string(), keys).await?;
                    }
                }
                Ok(())
            }
            "playlistclear" => {
                let playlist = self.playlist(arg(0)?).await?;
                for index in (0..playlist.track_keys.len() as u32).rev() {
                    api.remove_playlist_track(playlist.id.clone(), index)
                        .await?;
                }
                Ok(())
            }
            "playlistdelete" => {
                let playlist = self.playlist(arg(0)?).await?;
                let doomed = range(arg(1)?, playlist.track_keys.len() as u32)?;
                if doomed.is_empty() {
                    return Err(Ack::arg("Bad song index"));
                }
                for index in doomed.rev() {
                    api.remove_playlist_track(playlist.id.clone(), index)
                        .await?;
                }
                Ok(())
            }
            "playlistmove" => {
                let playlist = self.playlist(arg(0)?).await?;
                let reorder = PlaylistReorder {
                    from: number(arg(1)?)?,
                    to: number(arg(2)?)?,
                };
                api.reorder_playlist(playlist.id, reorder).await?;
                Ok(())
            }
            "rename" => {
                let playlist = self.playlist(arg(0)?).await?;
                if self.find_playlist(arg(1)?).await?.is_some() {
                    return Err(Ack::new(AckCode::Exist, "Playlist already exists"));
                }
                api.rename_playlist(playlist.id, arg(1)?.to_string())
                    .await?;
                Ok(())
            }
            "rm" => {
                let playlist = self.playlist(arg(0)?).await?;
                api.delete_playlist(playlist.id).await?;
                Ok(())
            }
            "save" => {
                let keys: Vec<String> = api
                    .queue_snapshot()
                    .await?
                    .items
                    .into_iter()
                    .map(|track| track.key)
                    .collect();
                let existing = self.find_playlist(arg(0)?).await?;
                match (args.get(1).map(String::as_str), existing) {
                    (None | Some("create"), Some(_)) => {
                        Err(Ack::new(AckCode::Exist, "Playlist already exists"))
                    }
                    (Some("append"), Some(playlist)) => {
                        api.add_playlist_tracks(playlist.id, keys).await?;
                        Ok(())
                    }
                    (Some("replace"), Some(playlist)) => {
                        api.delete_playlist(playlist.id).await?;
                        api.create_playlist(arg(0)?.to_string(), keys).await?;
                        Ok(())
                    }
                    (None | Some("create" | "append" | "replace"), None) => {
                        api.create_playlist(arg(0)?.to_string(), keys).await?;
                        Ok(())
                    }
                    (Some(mode), _) => Err(Ack::arg(format!("Unrecognized save mode: {mode}"))),
                }
            }

            "config" | "kill" => Err(Ack::new(
                AckCode::Permission,
                format!("you don't have permission for \"{name}\""),
            )),
            _ => Err(Ack::new(
                AckCode::Unknown,
                format!("unknown command \"{name}\""),
            )),
        }
    }

    async fn command(&self, command: PlayerCommand) -> Result<(), Ack> {
        self.shared.api.player_command(command).await?;
        Ok(())
    }

    async fn set_volume(&self, percent: i64) -> Result<(), Ack> {
        let volume = percent.clamp(0, 100) as f32 / 100.0;
        self.command(PlayerCommand::SetVolume { volume }).await
    }

    /// Where an `add` lands: the end, an absolute position, or `+N` / `-N`
    /// counted from the playing song.
    async fn insert_at(&self, word: Option<&str>) -> Result<u32, Ack> {
        let state = self.shared.api.player_state().await?;
        let len = state.queue.length;
        let Some(word) = word else {
            return Ok(len);
        };
        let relative = |offset: &str| -> Result<u32, Ack> {
            let current = state
                .queue
                .index
                .ok_or_else(|| Ack::arg("No current song"))?;
            let offset = number(offset)?;
            if word.starts_with('+') {
                Ok(current + 1 + offset)
            } else {
                current
                    .checked_sub(offset)
                    .ok_or_else(|| Ack::arg("Bad position"))
            }
        };
        let index = match word.as_bytes().first() {
            Some(b'+' | b'-') => relative(&word[1..])?,
            _ => number(word)?,
        };
        if index > len {
            return Err(Ack::arg("Bad song index"));
        }
        Ok(index)
    }

    async fn insert(&self, index: u32, keys: Vec<String>) -> Result<(), Ack> {
        if keys.is_empty() {
            return Err(Ack::no_exist("No such song"));
        }
        self.shared
            .api
            .queue_edit(QueueEdit::Insert { index, keys })
            .await?;
        Ok(())
    }

    async fn find_playlist(&self, name: &str) -> Result<Option<PlaylistInfo>, Ack> {
        Ok(self
            .shared
            .api
            .playlists()
            .await?
            .playlists
            .into_iter()
            .find(|playlist| playlist.name == name))
    }

    async fn playlist(&self, name: &str) -> Result<PlaylistInfo, Ack> {
        self.find_playlist(name)
            .await?
            .ok_or_else(|| Ack::no_exist("No such playlist"))
    }

    async fn picture(&mut self, uri: &str) -> Result<Option<Arc<ArtworkData>>, Ack> {
        if let Some((cached, picture)) = &self.picture
            && cached == uri
        {
            return Ok(Some(picture.clone()));
        }
        let library = self.shared.library().await?;
        let key = library.key_of(uri).to_string();
        let target = match library.by_uri(uri) {
            Some(song) => song.track.artwork.as_ref().map(|art| art.target.clone()),
            None => self
                .shared
                .api
                .tracks_by_keys(vec![key])
                .await?
                .into_iter()
                .next()
                .and_then(|track| track.artwork.map(|art| art.target)),
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let data = self
            .shared
            .api
            .artwork(ArtworkRequest { target, hq: true })
            .await?;
        if data.bytes.is_empty() {
            return Ok(None);
        }
        let data = Arc::new(data);
        self.picture = Some((uri.to_string(), data.clone()));
        Ok(Some(data))
    }

    async fn status(&self, out: &mut Response) -> Result<(), Ack> {
        let state = self.shared.api.player_state().await?;
        let queue = &state.queue;
        out.field("volume", percent(&state));
        out.field("repeat", u8::from(queue.loop_mode != LoopMode::None));
        out.field("random", u8::from(queue.shuffle));
        out.field("single", u8::from(queue.loop_mode == LoopMode::Track));
        out.field("consume", 0);
        out.field("playlist", queue.rev);
        out.field("playlistlength", queue.length);
        out.field("mixrampdb", "0.000000");
        let playing = match state.phase {
            Phase::Playing => "play",
            Phase::Paused => "pause",
            Phase::Idle | Phase::Ended => "stop",
        };
        out.field("state", playing);
        if let Some(index) = queue.index {
            out.field("song", index);
            out.field("songid", index);
            if index + 1 < queue.length {
                out.field("nextsong", index + 1);
                out.field("nextsongid", index + 1);
            }
        }
        if state.phase != Phase::Idle
            && state.phase != Phase::Ended
            && let Some(track) = &state.track
        {
            let elapsed = elapsed_ms(&state);
            let duration = track.duration_ms.filter(|_| !track.is_radio());
            out.field(
                "time",
                format!("{}:{}", elapsed / 1000, duration.unwrap_or(0) / 1000),
            );
            out.field("elapsed", format!("{:.3}", elapsed as f64 / 1000.0));
            if let Some(duration) = duration {
                out.field("duration", format!("{:.3}", duration as f64 / 1000.0));
            }
            if track.bitrate > 0 {
                out.field("bitrate", track.bitrate);
            }
        }
        if let Some(error) = &state.error {
            out.field("error", &error.message);
        }
        Ok(())
    }

    async fn print_queue(
        &self,
        out: &mut Response,
        queue: QueueSnapshot,
        window: std::ops::Range<u32>,
        positions_only: bool,
    ) -> Result<(), Ack> {
        let library = self.shared.library().await?;
        for (index, track) in queue.items.into_iter().enumerate() {
            let index = index as u32;
            if !window.contains(&index) {
                continue;
            }
            if positions_only {
                out.field("cpos", index);
                out.field("Id", index);
            } else {
                self.print_song(out, &library.song(track), Some(index));
            }
        }
        Ok(())
    }

    fn print_song(&self, out: &mut Response, song: &Song, position: Option<u32>) {
        out.field("file", &song.uri);
        for (bit, (tag, name)) in Tag::SUPPORTED.iter().enumerate() {
            if self.tags & (1 << bit) != 0 {
                out.tag(name, &song.tag(*tag));
            }
        }
        if let Some(ms) = song.duration_ms() {
            out.field("Time", (ms + 500) / 1000);
            out.field("duration", format!("{:.3}", ms as f64 / 1000.0));
        }
        if let Some(position) = position {
            out.field("Pos", position);
            out.field("Id", position);
        }
    }

    fn tagtypes(&mut self, args: &[String], out: &mut Response) -> Result<(), Ack> {
        let bits = |names: &[String]| -> Result<u16, Ack> {
            names.iter().try_fold(0u16, |bits, name| {
                Tag::SUPPORTED
                    .iter()
                    .position(|(_, known)| known.eq_ignore_ascii_case(name))
                    .map(|bit| bits | (1 << bit))
                    // A tag MPD has and Kopuz does not is never printed anyway.
                    .or_else(|| Tag::parse(name).map(|_| bits))
                    .ok_or_else(|| Ack::arg(format!("Unknown tag type: {name}")))
            })
        };
        match args.first().map(String::as_str) {
            None => {
                for (bit, (_, name)) in Tag::SUPPORTED.iter().enumerate() {
                    if self.tags & (1 << bit) != 0 {
                        out.field("tagtype", name);
                    }
                }
            }
            Some("all") => self.tags = u16::MAX,
            Some("clear") => self.tags = 0,
            Some("enable") => self.tags |= bits(&args[1..])?,
            Some("disable") => self.tags &= !bits(&args[1..])?,
            Some("reset") => self.tags = bits(&args[1..])?,
            Some(other) => return Err(Ack::arg(format!("Unknown sub command: {other}"))),
        }
        Ok(())
    }

    /// `list TYPE [FILTER] [group TAG]...`: the distinct values of one tag,
    /// under each distinct combination of the group tags.
    async fn list(&self, args: &[String], out: &mut Response) -> Result<(), Ack> {
        let kind = args
            .first()
            .ok_or_else(|| Ack::arg("too few arguments for \"list\""))?;
        let rest = &args[1..];
        let target = if kind.eq_ignore_ascii_case("file") {
            None
        } else {
            Some(Tag::parse(kind).ok_or_else(|| Ack::arg(format!("Unknown tag type: {kind}")))?)
        };
        // The pre-0.21 `list album ARTIST` form names an artist with no tag.
        let legacy_artist;
        let (filter, used) =
            if target == Some(Tag::Album) && rest.len() == 1 && !rest[0].starts_with('(') {
                legacy_artist = Filter::Compare {
                    subject: filter::Subject::Tag(Tag::Artist),
                    op: Op::Equals,
                    value: rest[0].clone(),
                    fold: None,
                };
                (Some(legacy_artist), 1)
            } else {
                filter::parse(rest, Op::Equals)?
            };
        let options = filter::trailing(&rest[used..])?;
        let library = self.shared.library().await?;
        let mut grouped: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
        for song in library.songs.iter() {
            if !filter
                .as_ref()
                .is_none_or(|filter| filter.matches(song, false))
            {
                continue;
            }
            let value = match target {
                Some(tag) => song.tag(tag),
                None => song.uri.clone(),
            };
            if value.is_empty() {
                continue;
            }
            let groups = options.groups.iter().map(|tag| song.tag(*tag)).collect();
            grouped.entry(groups).or_default().insert(value);
        }
        let label = target.map_or("file", Tag::name);
        let mut previous: Option<Vec<String>> = None;
        for (groups, values) in grouped {
            let first_change = previous.as_ref().map_or(0, |before| {
                before
                    .iter()
                    .zip(&groups)
                    .position(|(a, b)| a != b)
                    .unwrap_or(groups.len())
            });
            for (tag, value) in options.groups.iter().zip(&groups).skip(first_change) {
                out.field(tag.name(), value);
            }
            for value in values {
                out.field(label, value);
            }
            previous = Some(groups);
        }
        Ok(())
    }
}

/// The library keys a URI names: one song, every song under a directory, or
/// the whole library for the root. Anything else passes through as a key,
/// which the daemon resolves or refuses.
fn keys_at(library: &Library, uri: &str) -> Vec<String> {
    if let Some(song) = library.by_uri(uri) {
        return vec![song.track.key.clone()];
    }
    let under: Vec<String> = library
        .under(uri)
        .map(|song| song.track.key.clone())
        .collect();
    if under.is_empty() && !uri.trim_matches('/').is_empty() {
        vec![uri.to_string()]
    } else {
        under
    }
}

fn select<'a>(
    library: &'a Library,
    filter: Option<&Filter>,
    fold: bool,
    options: &Trailing,
) -> Result<Vec<&'a Song>, Ack> {
    let mut songs: Vec<&Song> = library
        .songs
        .iter()
        .filter(|song| filter.is_none_or(|filter| filter.matches(song, fold)))
        .collect();
    if let Some((tag, descending)) = options.sort {
        songs.sort_by_cached_key(|song| song.tag(tag));
        if descending {
            songs.reverse();
        }
    }
    if let Some(window) = &options.window {
        let window = range(window, songs.len() as u32)?;
        songs = songs[window.start as usize..window.end as usize].to_vec();
    }
    Ok(songs)
}

fn percent(state: &PlayerState) -> u32 {
    if state.muted {
        0
    } else {
        (state.volume * 100.0).round().clamp(0.0, 100.0) as u32
    }
}

/// Where playback is now: the anchor plus the time since it was taken.
fn elapsed_ms(state: &PlayerState) -> u64 {
    state.position.map_or(0, |anchor| {
        if anchor.playing {
            anchor.ms + state.now_ms.saturating_sub(anchor.at_ms)
        } else {
            anchor.ms
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::library;

    fn words(text: &str) -> Vec<String> {
        protocol::tokenize(text).unwrap()
    }

    #[test]
    fn a_directory_adds_everything_under_it() {
        let library = library();
        assert_eq!(keys_at(&library, "music/boc").len(), 2);
        assert_eq!(keys_at(&library, "").len(), 4);
        assert_eq!(keys_at(&library, "music/loose.mp3"), ["/music/loose.mp3"]);
        assert_eq!(keys_at(&library, "elsewhere"), ["elsewhere"]);
    }

    #[test]
    fn sort_and_window_shape_a_selection() {
        let library = library();
        let options = filter::trailing(&words("sort -Title window 1:3")).unwrap();
        let titles: Vec<_> = select(&library, None, false, &options)
            .unwrap()
            .into_iter()
            .map(|song| song.track.title.as_str())
            .collect();
        assert_eq!(titles, ["Music Is Math", "Avril 14th"]);
    }

    #[test]
    fn elapsed_runs_on_from_a_playing_anchor() {
        let mut state = PlayerState {
            now_ms: 5_000,
            position: Some(api::PositionAnchor {
                ms: 1_000,
                at_ms: 4_000,
                playing: true,
            }),
            ..Default::default()
        };
        assert_eq!(elapsed_ms(&state), 2_000);
        state.position = state.position.map(|anchor| api::PositionAnchor {
            playing: false,
            ..anchor
        });
        assert_eq!(elapsed_ms(&state), 1_000);
    }
}
