//! `idle` subsystems and which daemon events raise them.

use api::{ApiEvent, JobKind, LoopMode, Phase, PlayerState, Table};

use crate::protocol::Ack;

pub(crate) const DATABASE: u16 = 1 << 0;
pub(crate) const UPDATE: u16 = 1 << 1;
pub(crate) const STORED_PLAYLIST: u16 = 1 << 2;
pub(crate) const PLAYLIST: u16 = 1 << 3;
pub(crate) const PLAYER: u16 = 1 << 4;
pub(crate) const MIXER: u16 = 1 << 5;
pub(crate) const OUTPUT: u16 = 1 << 6;
pub(crate) const OPTIONS: u16 = 1 << 7;
pub(crate) const ALL: u16 = u16::MAX;

const NAMES: [(&str, u16); 8] = [
    ("database", DATABASE),
    ("update", UPDATE),
    ("stored_playlist", STORED_PLAYLIST),
    ("playlist", PLAYLIST),
    ("player", PLAYER),
    ("mixer", MIXER),
    ("output", OUTPUT),
    ("options", OPTIONS),
];

/// The subsystems an `idle` waits on; none named means all of them.
/// Subsystems this server never raises (`sticker`, `message`, ...) are
/// accepted and simply never fire.
pub(crate) fn mask(args: &[String]) -> Result<u16, Ack> {
    if args.is_empty() {
        return Ok(ALL);
    }
    let mut mask = 0;
    for name in args {
        match NAMES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some((_, bit)) => mask |= bit,
            None if matches!(
                name.as_str(),
                "sticker" | "subscription" | "message" | "neighbor" | "mount" | "partition"
            ) => {}
            None => return Err(Ack::arg(format!("Unrecognized idle event: {name}"))),
        }
    }
    Ok(mask)
}

pub(crate) fn names(mask: u16) -> impl Iterator<Item = &'static str> {
    NAMES
        .into_iter()
        .filter(move |(_, bit)| mask & bit != 0)
        .map(|(name, _)| name)
}

/// The parts of the player snapshot each subsystem reports on, so a state
/// event that only moved the volume does not wake a client waiting on `player`.
#[derive(Debug, Default, PartialEq)]
struct Seen {
    phase: Phase,
    track: Option<String>,
    position: Option<(u64, u64, bool)>,
    volume: (u32, bool),
    options: (bool, LoopMode),
}

impl Seen {
    fn of(state: &PlayerState) -> Self {
        Self {
            phase: state.phase,
            track: state.track.as_ref().map(|track| track.key.clone()),
            position: state
                .position
                .map(|anchor| (anchor.ms, anchor.at_ms, anchor.playing)),
            volume: ((state.volume * 100.0).round() as u32, state.muted),
            options: (state.queue.shuffle, state.queue.loop_mode),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Tracker {
    seen: Option<Seen>,
}

impl Tracker {
    pub fn subsystems(&mut self, event: &ApiEvent) -> u16 {
        match event {
            ApiEvent::PlayerState(state) => {
                // The first state only says where things stand, not that they moved.
                let Some(before) = self.seen.replace(Seen::of(state)) else {
                    return 0;
                };
                let now = self.seen.as_ref().unwrap_or(&before);
                let mut bits = 0;
                if (before.phase, &before.track, before.position)
                    != (now.phase, &now.track, now.position)
                {
                    bits |= PLAYER;
                }
                if before.volume != now.volume {
                    bits |= MIXER;
                }
                // The one output is the player, and disabling it mutes.
                if before.volume.1 != now.volume.1 {
                    bits |= OUTPUT;
                }
                if before.options != now.options {
                    bits |= OPTIONS;
                }
                bits
            }
            ApiEvent::QueueChanged { .. } => PLAYLIST,
            ApiEvent::LibraryInvalidated { table } => match table {
                Table::Playlists | Table::Folders => STORED_PLAYLIST,
                Table::Tracks | Table::Albums | Table::Artists | Table::Unknown => DATABASE,
                _ => 0,
            },
            ApiEvent::JobProgress(progress) if is_scan(progress.kind) => UPDATE,
            ApiEvent::JobFinished { kind, .. } if is_scan(*kind) => UPDATE | DATABASE,
            ApiEvent::Resync => ALL,
            _ => 0,
        }
    }
}

fn is_scan(kind: JobKind) -> bool {
    matches!(kind, JobKind::Scan | JobKind::LibrarySync)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(volume: f32, phase: Phase, shuffle: bool) -> ApiEvent {
        let mut state = PlayerState {
            volume,
            phase,
            ..Default::default()
        };
        state.queue.shuffle = shuffle;
        ApiEvent::PlayerState(Box::new(state))
    }

    #[test]
    fn a_state_event_raises_only_what_moved() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.subsystems(&state(0.5, Phase::Idle, false)), 0);
        assert_eq!(tracker.subsystems(&state(0.6, Phase::Idle, false)), MIXER);
        assert_eq!(
            tracker.subsystems(&state(0.6, Phase::Playing, false)),
            PLAYER
        );
        assert_eq!(
            tracker.subsystems(&state(0.6, Phase::Playing, true)),
            OPTIONS
        );
        assert_eq!(tracker.subsystems(&state(0.6, Phase::Playing, true)), 0);
    }

    #[test]
    fn library_and_queue_events_map_to_their_subsystems() {
        let mut tracker = Tracker::default();
        let table = |table| ApiEvent::LibraryInvalidated { table };
        assert_eq!(tracker.subsystems(&table(Table::Tracks)), DATABASE);
        assert_eq!(
            tracker.subsystems(&table(Table::Playlists)),
            STORED_PLAYLIST
        );
        let queue = ApiEvent::QueueChanged {
            rev: 2,
            length: 1,
            index: None,
        };
        assert_eq!(tracker.subsystems(&queue), PLAYLIST);
    }

    #[test]
    fn masks_parse_names_and_ignore_unraised_ones() {
        let words = |list: &[&str]| list.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert_eq!(mask(&[]).unwrap(), ALL);
        assert_eq!(mask(&words(&["player", "mixer"])).unwrap(), PLAYER | MIXER);
        assert_eq!(mask(&words(&["sticker"])).unwrap(), 0);
        assert!(mask(&words(&["bogus"])).is_err());
        assert_eq!(
            names(PLAYER | PLAYLIST).collect::<Vec<_>>(),
            ["playlist", "player"]
        );
    }
}
