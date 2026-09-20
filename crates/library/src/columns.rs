//! The tracks table's columns: which exist, which are shown, how wide, and in what order.

use store::layout::Column;

/// One column the table can draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub id: &'static str,
    pub label_key: &'static str,
    /// A column with no natural width shares the leftover space this many ways.
    pub grow: f32,
    /// What a flexible column never goes below, or what a fixed one is.
    pub base: f32,
    pub align_end: bool,
    /// The title column carries the cover and can never be hidden; without it there is no row.
    pub required: bool,
}

/// Every column, in the order the table ships with.
pub const SPECS: &[Spec] = &[
    Spec {
        id: "title",
        label_key: "title",
        grow: 2.,
        base: 200.,
        align_end: false,
        required: true,
    },
    Spec {
        id: "artist",
        label_key: "artist",
        grow: 1.,
        base: 150.,
        align_end: false,
        required: false,
    },
    Spec {
        id: "album",
        label_key: "album",
        grow: 1.,
        base: 150.,
        align_end: false,
        required: false,
    },
    Spec {
        id: "plays",
        label_key: "library_column_plays",
        grow: 0.,
        base: 64.,
        align_end: true,
        required: false,
    },
    Spec {
        id: "format",
        label_key: "library_column_format",
        grow: 0.,
        base: 72.,
        align_end: false,
        required: false,
    },
    Spec {
        id: "duration",
        label_key: "duration",
        grow: 0.,
        base: 64.,
        align_end: true,
        required: false,
    },
];

/// The columns that are drawn when nobody has changed anything.
pub const SHIPPED: &[&str] = &["title", "artist", "album", "duration"];

pub fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == id)
}

/// The saved layout reconciled with the columns this build has: unknown ids dropped, new ones
/// appended hidden, and the title column forced back on if a file ever says otherwise.
pub fn resolve(saved: &[Column]) -> Vec<Column> {
    if saved.is_empty() {
        return SPECS
            .iter()
            .map(|spec| Column {
                id: spec.id.to_string(),
                visible: SHIPPED.contains(&spec.id),
                width: None,
            })
            .collect();
    }
    let mut columns: Vec<Column> = saved
        .iter()
        .filter(|column| spec(&column.id).is_some())
        .cloned()
        .collect();
    for known in SPECS {
        if !columns.iter().any(|column| column.id == known.id) {
            columns.push(Column {
                id: known.id.to_string(),
                visible: false,
                width: None,
            });
        }
    }
    for column in &mut columns {
        if spec(&column.id).is_some_and(|spec| spec.required) {
            column.visible = true;
        }
    }
    columns
}

/// The shown columns, paired with what they are, in draw order.
pub fn visible(columns: &[Column]) -> Vec<(&Column, &'static Spec)> {
    columns
        .iter()
        .filter(|column| column.visible)
        .filter_map(|column| spec(&column.id).map(|spec| (column, spec)))
        .collect()
}

/// Put `moved` where `before` sits; dropping a column on itself changes nothing.
pub fn reorder(columns: &mut Vec<Column>, moved: &str, before: &str) {
    if moved == before {
        return;
    }
    let Some(from) = columns.iter().position(|column| column.id == moved) else {
        return;
    };
    let column = columns.remove(from);
    let to = columns
        .iter()
        .position(|held| held.id == before)
        .unwrap_or(columns.len());
    columns.insert(to, column);
}

/// What a resize is clamped to, so a column can never be dragged to nothing or past the window.
pub const MIN_WIDTH: f32 = 60.;
pub const MAX_WIDTH: f32 = 600.;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_layout_ships_the_designed_columns() {
        let columns = resolve(&[]);
        let shown: Vec<&str> = visible(&columns)
            .iter()
            .map(|(column, _)| column.id.as_str())
            .collect();
        assert_eq!(shown, SHIPPED);
    }

    #[test]
    fn a_column_from_a_later_version_shows_up_hidden_rather_than_vanishing_the_layout() {
        let saved = vec![Column::new("title"), Column::new("artist")];
        let columns = resolve(&saved);
        assert_eq!(columns.len(), SPECS.len());
        assert!(
            columns
                .iter()
                .find(|column| column.id == "duration")
                .is_some_and(|column| !column.visible)
        );
    }

    #[test]
    fn a_column_this_build_no_longer_has_is_dropped() {
        let saved = vec![Column::new("title"), Column::new("gone")];
        let columns = resolve(&saved);
        assert!(!columns.iter().any(|column| column.id == "gone"));
    }

    #[test]
    fn the_title_column_cannot_be_hidden() {
        let saved = vec![Column {
            id: "title".into(),
            visible: false,
            width: None,
        }];
        let columns = resolve(&saved);
        assert!(columns[0].visible);
    }

    #[test]
    fn a_column_dropped_on_another_lands_in_its_place() {
        let mut columns = resolve(&[]);
        reorder(&mut columns, "duration", "artist");
        let order: Vec<&str> = columns.iter().map(|column| column.id.as_str()).collect();
        assert_eq!(&order[..3], &["title", "duration", "artist"]);
    }

    #[test]
    fn a_column_dropped_on_itself_changes_nothing() {
        let mut columns = resolve(&[]);
        let before = columns.clone();
        reorder(&mut columns, "title", "title");
        assert_eq!(columns, before);
    }
}
