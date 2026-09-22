//! What a command acts on. Pure data, so it sits below both `commands` and `widgets`.

use gpui_kit::SharedString;

/// The thing under the pointer, the keyboard, or the selection, named well enough to talk about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    None,
    /// One or more tracks; `label` names the selection for a confirm or a toast.
    Tracks {
        keys: Vec<SharedString>,
        label: SharedString,
    },
    Album {
        id: SharedString,
        title: SharedString,
    },
    /// `artist_id` is set when the click carried the source's own id.
    Artist {
        name: SharedString,
        artist_id: Option<SharedString>,
    },
    Playlist {
        id: SharedString,
        name: SharedString,
    },
    Folder {
        id: SharedString,
        name: SharedString,
    },
    /// A row of the queue, by position, since a queue may hold one track twice.
    QueueItem { index: u32, key: SharedString },
    Source {
        id: SharedString,
        name: SharedString,
    },
}

/// What kind of thing a target is, for a command to say what it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    None,
    Tracks,
    Album,
    Artist,
    Playlist,
    Folder,
    QueueItem,
    Source,
}

impl Target {
    pub fn track(key: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Target::Tracks {
            keys: vec![key.into()],
            label: title.into(),
        }
    }

    pub fn tracks(keys: Vec<SharedString>, label: impl Into<SharedString>) -> Self {
        Target::Tracks {
            keys,
            label: label.into(),
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Target::None => Kind::None,
            Target::Tracks { .. } => Kind::Tracks,
            Target::Album { .. } => Kind::Album,
            Target::Artist { .. } => Kind::Artist,
            Target::Playlist { .. } => Kind::Playlist,
            Target::Folder { .. } => Kind::Folder,
            Target::QueueItem { .. } => Kind::QueueItem,
            Target::Source { .. } => Kind::Source,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Target::None)
    }

    /// How many tracks this stands for; anything that is not a track selection counts as one.
    pub fn count(&self) -> usize {
        match self {
            Target::None => 0,
            Target::Tracks { keys, .. } => keys.len(),
            _ => 1,
        }
    }

    /// The track keys a play or queue command should act on, if any.
    pub fn keys(&self) -> &[SharedString] {
        match self {
            Target::Tracks { keys, .. } => keys,
            _ => &[],
        }
    }

    /// What to call this in a menu item, a confirm, or a toast.
    pub fn label(&self) -> SharedString {
        match self {
            Target::None => SharedString::default(),
            Target::Tracks { label, .. } => label.clone(),
            Target::Album { title, .. } => title.clone(),
            Target::Artist { name, .. } => name.clone(),
            Target::Playlist { name, .. } => name.clone(),
            Target::Folder { name, .. } => name.clone(),
            Target::QueueItem { key, .. } => key.clone(),
            Target::Source { name, .. } => name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_track_is_a_selection_of_one() {
        let target = Target::track("t1", "Blue Monday");
        assert_eq!(target.kind(), Kind::Tracks);
        assert_eq!(target.count(), 1);
        assert_eq!(target.label(), "Blue Monday");
        assert_eq!(target.keys(), ["t1"]);
    }

    #[test]
    fn nothing_selected_acts_on_nothing() {
        let target = Target::default();
        assert!(target.is_none());
        assert_eq!(target.count(), 0);
        assert!(target.keys().is_empty());
    }

    #[test]
    fn an_album_stands_for_itself_not_its_tracks() {
        let target = Target::Album {
            id: "a1".into(),
            title: "Power, Corruption & Lies".into(),
        };
        assert_eq!(target.count(), 1);
        assert!(target.keys().is_empty());
    }
}
