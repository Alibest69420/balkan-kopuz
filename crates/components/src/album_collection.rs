//! Albums as virtualized tiles or rows, and the one order they are sorted in.

use std::cmp::Ordering;
use std::rc::Rc;

use api::AlbumInfo;
use config::{AlbumSortField, SortCriterion, SortDirection};
use gpui_kit::{AnyElement, Context, IntoElement as _, Pixels, SharedString, Window, px};

use crate::cards::{Frame, album_row, album_tile};
use crate::format::t;
use crate::grid::{GridMode, TileGrid};

// `grid-cols-[repeat(auto-fill,minmax(180px,1fr))]`: GPUI wraps flex children instead, at the column's minimum.
const TILE: Pixels = px(180.);
// The tile's cover fills it inside `p-4`.
const TILE_COVER: Pixels = px(148.);

/// Every album of a listing, drawn in bands of tiles or as rows, built only once on screen.
pub struct AlbumCollection {
    grid: TileGrid,
}

impl AlbumCollection {
    pub fn new(id: &'static str) -> Self {
        Self {
            grid: TileGrid::new(id),
        }
    }

    /// The albums as the page's own scroller; `albums` is shared so a band never clones the listing.
    pub fn bands<V: 'static>(
        &self,
        albums: Rc<[AlbumInfo]>,
        mode: GridMode,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> AnyElement {
        self.grid
            .bands(
                albums.len(),
                TILE,
                mode,
                move |_: &mut V, index, _, _| match mode {
                    GridMode::Tiles => album_tile(&albums[index])
                        .size(TILE_COVER)
                        .frame(Frame::Card)
                        .into_any_element(),
                    GridMode::Rows => album_row(&albums[index]).into_any_element(),
                },
            )
            .build(window, cx)
    }
}

/// The fields worth offering: one nothing carries a value for would sort the list into the order it had.
pub fn available_fields(albums: &[AlbumInfo], with_artist: bool) -> Vec<AlbumSortField> {
    let mut fields = vec![AlbumSortField::Title];
    if with_artist {
        fields.push(AlbumSortField::Artist);
    }
    if albums.iter().any(|album| album.year > 0) {
        fields.push(AlbumSortField::Year);
    }
    if albums.iter().any(|album| !album.genre.trim().is_empty()) {
        fields.push(AlbumSortField::Genre);
    }
    fields
}

pub fn sort_albums(albums: &mut [AlbumInfo], criteria: &[SortCriterion<AlbumSortField>]) {
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

pub fn field_label(field: AlbumSortField) -> SharedString {
    t(match field {
        AlbumSortField::Title => "sort_field_title",
        AlbumSortField::Artist => "sort_field_artist",
        AlbumSortField::Year => "sort_field_year",
        AlbumSortField::Genre => "sort_field_genre",
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

/// Case-insensitive without allocating: a comparator runs O(n log n) times over a whole library.
pub fn compare_text(left: &str, right: &str) -> Ordering {
    left.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .cmp(right.trim().chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(title: &str, year: u16) -> AlbumInfo {
        AlbumInfo {
            title: title.to_string(),
            year,
            ..AlbumInfo::default()
        }
    }

    fn by(field: AlbumSortField, direction: SortDirection) -> Vec<SortCriterion<AlbumSortField>> {
        vec![SortCriterion { field, direction }]
    }

    #[test]
    fn titles_sort_without_regard_to_case_or_padding() {
        let mut albums = vec![album(" beta", 0), album("Alpha", 0), album("gamma", 0)];
        sort_albums(&mut albums, &by(AlbumSortField::Title, SortDirection::Asc));
        let titles: Vec<&str> = albums.iter().map(|album| album.title.as_str()).collect();
        assert_eq!(titles, ["Alpha", " beta", "gamma"]);
    }

    #[test]
    fn a_later_criterion_breaks_the_tie_of_an_earlier_one() {
        let mut albums = vec![album("b", 2001), album("a", 2001), album("c", 1999)];
        let criteria = vec![
            SortCriterion {
                field: AlbumSortField::Year,
                direction: SortDirection::Desc,
            },
            SortCriterion {
                field: AlbumSortField::Title,
                direction: SortDirection::Asc,
            },
        ];
        sort_albums(&mut albums, &criteria);
        let titles: Vec<&str> = albums.iter().map(|album| album.title.as_str()).collect();
        assert_eq!(titles, ["a", "b", "c"]);
    }

    #[test]
    fn a_field_nothing_carries_is_not_offered() {
        let fields = available_fields(&[album("a", 0)], false);
        assert_eq!(fields, [AlbumSortField::Title]);
    }
}
