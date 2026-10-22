# DLNA servers

Add **DLNA / UPnP** from the source selector to play the library of a UPnP AV
media server on your network: MiniDLNA (ReadyMedia), the media server built
into most NAS systems, Plex, Serviio, Universal Media Server or Kodi.

The **Server** field takes one of:

- nothing, to use the first media server that answers discovery;
- the server's name as it announces itself, or its host or IP address;
- its device description URL, such as `http://nas:8200/rootDesc.xml`, which
  skips discovery. Use this when multicast does not reach the server, for
  example across a VPN or between VLANs.

No sign-in is needed. Sync reads every audio item with the server's `Search`
action, or walks its folders when it has none, and takes titles, artists,
albums, genres, years, durations and album art from the server's own
metadata; files are not opened. A file the server lists under several views
is added once. Playback streams the server's original file over HTTP,
preferring it to a transcode, and asks the server for the stream address each
time, so a server that changes address keeps working after the next sync.

The library is read-only: playlists, tag edits and uploads are unavailable.
Favorites are kept in Kopuz. Downloads save the streamed file.

## Validation

```sh
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --release -- -D warnings
SQLX_OFFLINE=true cargo test -p kopuz-server dlna
SQLX_OFFLINE=true cargo test -p kopuz-db service_round_trips
```

On a network with a media server, verify that an empty server field finds it,
that a name and a description URL both select it, that sync lists its
library once, and that playback, seeking, covers and favorites work.
