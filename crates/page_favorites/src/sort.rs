//! The tri-state column sort the favorites header drives, matching `components::showcase`.

use std::cmp::Ordering;

use api::TrackInfo;
use widgets::icons;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Artist,
    Album,
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

/// No sort, or a field and the way it runs.
pub type State = Option<(Field, Direction)>;

impl Field {
    /// The element id its header button carries.
    pub fn id(self) -> &'static str {
        match self {
            Field::Title => "sort-title",
            Field::Artist => "sort-artist",
            Field::Album => "sort-album",
            Field::Duration => "sort-duration",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Artist => "artist",
            Field::Album => "album",
            Field::Duration => "duration",
        }
    }
}

/// Clicking a header cycles ascending, descending, unsorted.
pub fn next(current: State, field: Field) -> State {
    match current {
        Some((sorted, Direction::Asc)) if sorted == field => Some((field, Direction::Desc)),
        Some((sorted, Direction::Desc)) if sorted == field => None,
        _ => Some((field, Direction::Asc)),
    }
}

/// The glyph a header shows for its own field.
pub fn indicator(state: State, field: Field) -> &'static str {
    match state {
        Some((sorted, Direction::Asc)) if sorted == field => icons::CHEVRON_UP,
        Some((sorted, Direction::Desc)) if sorted == field => icons::CHEVRON_DOWN,
        _ => icons::SORT,
    }
}

/// Order `tracks` in place; an unsorted state keeps the order the daemon gave.
pub fn sort(tracks: &mut [TrackInfo], state: State) {
    let Some((field, direction)) = state else {
        return;
    };
    tracks.sort_by(|left, right| {
        let primary = match field {
            Field::Title => compare(&left.title, &right.title),
            Field::Artist => compare(&left.artist, &right.artist),
            Field::Album => compare(&left.album, &right.album),
            Field::Duration => left.duration_ms.cmp(&right.duration_ms),
        };
        let directional = match direction {
            Direction::Asc => primary,
            Direction::Desc => primary.reverse(),
        };
        match field {
            Field::Album => directional
                .then_with(|| {
                    left.disc_number
                        .unwrap_or(0)
                        .cmp(&right.disc_number.unwrap_or(0))
                })
                .then_with(|| {
                    left.track_number
                        .unwrap_or(0)
                        .cmp(&right.track_number.unwrap_or(0))
                })
                .then_with(|| compare(&left.title, &right.title)),
            _ => directional,
        }
    });
}

fn compare(left: &str, right: &str) -> Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{Direction, Field, State, next};

    #[test]
    fn a_header_cycles_ascending_descending_unsorted() {
        let mut state: State = None;
        state = next(state, Field::Title);
        assert_eq!(state, Some((Field::Title, Direction::Asc)));
        state = next(state, Field::Title);
        assert_eq!(state, Some((Field::Title, Direction::Desc)));
        state = next(state, Field::Title);
        assert_eq!(state, None);
    }

    #[test]
    fn another_header_starts_its_own_cycle() {
        let state = next(Some((Field::Title, Direction::Desc)), Field::Album);
        assert_eq!(state, Some((Field::Album, Direction::Asc)));
    }
}
