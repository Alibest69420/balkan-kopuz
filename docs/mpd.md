# MPD clients

`kopuzd --mpd <ip>:<port>` speaks the [MPD protocol] on that address, so the
clients written for the Music Player Daemon can drive Kopuz: ncmpcpp, rmpc,
Cantata, M.A.L.P., `mpc`, status bar widgets, Home Assistant's MPD
integration and scrobblers such as mpdscribble.

```sh
kopuzd --mpd 127.0.0.1:6600
mpc status
```

There is no password unless `$KOPUZ_MPD_PASSWORD` is set when the daemon
starts; then a client must send it before any other command. Bind to a LAN
address only on a network you trust: the protocol is plain text.

[MPD protocol]: https://mpd.readthedocs.io/en/latest/protocol.html

## What maps to what

- **Library**: the active source's library. A song's URI is its library key:
  a local file's path without the leading `/`, so `lsinfo` and `listall` walk
  the folders, and a remote track's id as it is. `find`, `search`, `list`
  (with `group`), `count`, `findadd` and `searchadd` take both filter
  expressions and the older `TAG VALUE` pairs. `update` starts a library scan.
- **Queue**: Kopuz's queue in play order. A song's `Id` is its position, so
  ids last until the queue next changes. `plchanges` answers the whole queue
  for any version but the current one.
- **Playback**: `repeat` loops the queue and `single` loops the track; `single
  oneshot`, `consume`, `crossfade` and regular expressions in filters are
  refused. The one output is the player; disabling it mutes it.
- **Stored playlists**: Kopuz playlists, by name, through the active source.
- **Covers**: `albumart` and `readpicture` serve the track's artwork.
- **`idle`**: `player`, `mixer`, `options`, `playlist`, `stored_playlist`,
  `database`, `update` and `output` fire on the daemon's own events.

The desktop app does not serve MPD; run `kopuzd` for it.
