//! Client-side album ordering: the rows are already here, so reordering them is not a round trip.

use std::cmp::Ordering;

use api::AlbumInfo;
use config::{AlbumSortField, SortCriterion, SortDirection};
use gpui_kit::SharedString;
use widgets::format::t;

/// The fields worth offering: one nothing carries a value for would sort the list into the order it had.
pub fn available_fields(albums: &[AlbumInfo]) -> Vec<AlbumSortField> {
    let mut fields = vec![AlbumSortField::Title, AlbumSortField::Artist];
    if albums.iter().any(|album| album.year > 0) {
        fields.push(AlbumSortField::Year);
    }
    if albums.iter().any(|album| !album.genre.trim().is_empty()) {
        fields.push(AlbumSortField::Genre);
    }
    fields
}

pub fn sort(albums: &mut [AlbumInfo], criteria: &[SortCriterion<AlbumSortField>]) {
    albums.sort_by(|left, right| {
        for criterion in criteria {
            let ordering = compare(left, right, criterion);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });
}

pub fn label(field: AlbumSortField) -> SharedString {
    t(match field {
        AlbumSortField::Title => "sort_field_title",
        AlbumSortField::Artist => "sort_field_artist",
        AlbumSortField::Year => "sort_field_year",
        AlbumSortField::Genre => "sort_field_genre",
    })
}

pub fn direction_label(direction: SortDirection) -> SharedString {
    t(match direction {
        SortDirection::Asc => "sort_ascending",
        SortDirection::Desc => "sort_descending",
    })
}

fn compare(
    left: &AlbumInfo,
    right: &AlbumInfo,
    criterion: &SortCriterion<AlbumSortField>,
) -> Ordering {
    let ordering = match criterion.field {
        AlbumSortField::Title => compare_text(&left.title, &right.title),
        AlbumSortField::Artist => compare_text(&left.artist, &right.artist),
        AlbumSortField::Year => left.year.cmp(&right.year),
        AlbumSortField::Genre => compare_text(&left.genre, &right.genre),
    };
    match criterion.direction {
        SortDirection::Asc => ordering,
        SortDirection::Desc => ordering.reverse(),
    }
}

fn compare_text(left: &str, right: &str) -> Ordering {
    left.trim().to_lowercase().cmp(&right.trim().to_lowercase())
}
