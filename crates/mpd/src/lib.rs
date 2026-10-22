//! An MPD protocol server in front of the Kopuz API, so the clients written
//! for the Music Player Daemon (ncmpcpp, rmpc, Cantata, M.A.L.P., `mpc`, bar
//! widgets, Home Assistant) can drive a running Kopuz.
//!
//! It is a frontend like any other: it holds an `Arc<dyn KopuzApi>` and
//! nothing else, so what it can do is what the API offers. Where MPD and
//! Kopuz model a thing differently the mapping is documented at the command.
//! Two are worth knowing up front:
//!
//! - A song's URI is its library key, with a local path's leading `/`
//!   dropped so the paths form a tree; a remote row's key is its URI as is.
//! - A queue entry's id is its play-order position, so ids are stable only
//!   until the queue next changes, which every client watches for anyway.

mod commands;
mod filter;
mod idle;
mod library;
mod protocol;

use std::sync::Arc;
use std::time::Instant;

use api::KopuzApi;
use futures_util::StreamExt;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::library::Library;
use crate::protocol::{Ack, AckCode};

/// Longer than any command a client sends; a peer streaming without a
/// newline is cut off rather than buffered without bound.
const MAX_LINE: u64 = 64 * 1024;

/// The most commands one `command_list_begin` may hold.
const MAX_LIST: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// When set, a client must send `password` before any other command.
    pub password: Option<String>,
}

pub(crate) struct Shared {
    api: Arc<dyn KopuzApi>,
    password: Option<String>,
    library: Mutex<Option<Arc<Library>>>,
    changes: broadcast::Sender<u16>,
    started: Instant,
}

impl Shared {
    /// The library as last read, reading it again after the daemon said it changed.
    async fn library(&self) -> Result<Arc<Library>, Ack> {
        let mut cached = self.library.lock().await;
        if let Some(library) = cached.as_ref() {
            return Ok(library.clone());
        }
        let library = Arc::new(Library::load(&*self.api).await?);
        *cached = Some(library.clone());
        Ok(library)
    }
}

/// Serve MPD clients on `listener` until accepting fails for good.
pub async fn serve(
    listener: TcpListener,
    api: Arc<dyn KopuzApi>,
    options: Options,
) -> std::io::Result<()> {
    let shared = Arc::new(Shared {
        api,
        password: options.password.filter(|password| !password.is_empty()),
        library: Mutex::new(None),
        changes: broadcast::channel(256).0,
        started: Instant::now(),
    });
    tokio::spawn(follow_events(shared.clone()));
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) if is_transient(&error) => {
                tracing::warn!(%error, "mpd: accept failed");
                continue;
            }
            Err(error) => return Err(error),
        };
        tracing::debug!(%peer, "mpd: client connected");
        let shared = shared.clone();
        tokio::spawn(async move {
            if let Err(error) = connection(stream, shared).await {
                tracing::debug!(%peer, %error, "mpd: client dropped");
            }
        });
    }
}

fn is_transient(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::Interrupted
            | ErrorKind::WouldBlock
    )
}

/// Turn the daemon's events into `idle` subsystems for every connection, and
/// drop the cached library when it changed.
async fn follow_events(shared: Arc<Shared>) {
    let mut events = shared.api.events();
    let mut tracker = idle::Tracker::default();
    while let Some(event) = events.next().await {
        let bits = tracker.subsystems(&event);
        if bits & idle::DATABASE != 0 {
            *shared.library.lock().await = None;
        }
        if bits != 0 {
            let _ = shared.changes.send(bits);
        }
    }
}

async fn read_lines(read: tokio::net::tcp::OwnedReadHalf, lines: mpsc::Sender<String>) {
    let mut reader = BufReader::new(read);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE).read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) if !line.ends_with('\n') => return,
            Ok(_) => {
                let line = line.trim_end_matches(['\n', '\r']).to_string();
                if lines.send(line).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn connection(stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut changes = shared.changes.subscribe();
    let (line_tx, mut lines) = mpsc::channel(64);
    let reader = tokio::spawn(read_lines(read, line_tx));
    let result = async {
        write
            .write_all(format!("OK MPD {}\n", protocol::VERSION).as_bytes())
            .await?;
        let mut client = commands::Client::new(shared);
        while let Some(line) = lines.recv().await {
            client.absorb(&mut changes);
            let words = match protocol::tokenize(&line) {
                Ok(words) => words,
                Err(ack) => {
                    write.write_all(ack.render(0, "").as_bytes()).await?;
                    continue;
                }
            };
            let Some(name) = words.first() else {
                let ack = Ack::new(AckCode::Unknown, "No command given");
                write.write_all(ack.render(0, "").as_bytes()).await?;
                continue;
            };
            match name.as_str() {
                "close" => return Ok(()),
                // Outside an idle there is nothing to cancel, and MPD answers nothing.
                "noidle" => {}
                "idle" => {
                    let answer = match client.check(&words) {
                        Err(ack) => ack.render(0, "idle"),
                        Ok(()) => match idle::mask(&words[1..]) {
                            Err(ack) => ack.render(0, "idle"),
                            Ok(mask) => {
                                match wait_idle(&mut client, mask, &mut changes, &mut lines).await {
                                    Some(answer) => answer,
                                    None => return Ok(()),
                                }
                            }
                        },
                    };
                    write.write_all(answer.as_bytes()).await?;
                }
                "command_list_begin" | "command_list_ok_begin" => {
                    let ok_each = name == "command_list_ok_begin";
                    let mut list = Vec::new();
                    loop {
                        let Some(line) = lines.recv().await else {
                            return Ok(());
                        };
                        if line == "command_list_end" {
                            break;
                        }
                        if list.len() == MAX_LIST {
                            return Ok(());
                        }
                        list.push(line);
                    }
                    let answer = client.run_list(&list, ok_each).await;
                    write.write_all(&answer).await?;
                }
                _ => {
                    let answer = client.run_one(&words, 0).await;
                    write.write_all(&answer).await?;
                }
            }
        }
        Ok(())
    }
    .await;
    reader.abort();
    result
}

/// Block until a subsystem in `mask` changes or the client sends `noidle`.
/// `None` means the client went away, or broke protocol by sending anything
/// else mid-idle, which MPD answers by closing the connection.
async fn wait_idle(
    client: &mut commands::Client,
    mask: u16,
    changes: &mut broadcast::Receiver<u16>,
    lines: &mut mpsc::Receiver<String>,
) -> Option<String> {
    loop {
        client.absorb(changes);
        if client.pending & mask != 0 {
            return Some(client.take_changed(mask));
        }
        tokio::select! {
            change = changes.recv() => match change {
                Ok(bits) => client.pending |= bits,
                Err(broadcast::error::RecvError::Lagged(_)) => client.pending |= idle::ALL,
                Err(broadcast::error::RecvError::Closed) => {
                    let line = lines.recv().await?;
                    return (line.trim() == "noidle").then(|| client.take_changed(mask));
                }
            },
            line = lines.recv() => {
                return (line?.trim() == "noidle").then(|| client.take_changed(mask));
            }
        }
    }
}
