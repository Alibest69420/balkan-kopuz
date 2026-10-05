//! The order the artists grid shows the daemon's listing in.

use std::collections::HashMap;

use api::{AlbumInfo, ArtistInfo};
use config::{ArtistSortField, SortCriterion, SortDirection};

/// The grid, in its sorted order; the name, then the key, break remaining ties.
pub fn grid_entries(
    albums: &[AlbumInfo],
    artists: &[ArtistInfo],
    criteria: &[SortCriterion<ArtistSortField>],
) -> Vec<ArtistInfo> {
    let mut album_counts: HashMap<&String, u32> = HashMap::new();
    for artist in albums.iter().filter_map(|album| album.artist_key.as_ref()) {
        *album_counts.entry(artist).or_default() += 1;
    }
    let albums_of = |artist: &ArtistInfo| album_counts.get(&artist.key).copied().unwrap_or(0);
    let by_name = |left: &ArtistInfo, right: &ArtistInfo| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.key.cmp(&right.key))
    };
    let mut shown = artists.to_vec();
    shown.sort_by(|left, right| {
        for criterion in criteria {
            let ordering = match criterion.field {
                ArtistSortField::Name => by_name(left, right),
                ArtistSortField::Tracks => left.track_count.cmp(&right.track_count),
                ArtistSortField::Albums => albums_of(left).cmp(&albums_of(right)),
            };
            let ordering = match criterion.direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        by_name(left, right)
    });
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(key: &str, name: &str, track_count: u32) -> ArtistInfo {
        ArtistInfo {
            key: String::from(key),
            name: name.into(),
            track_count,
            artwork: None,
        }
    }

    fn album(artist: &str) -> AlbumInfo {
        AlbumInfo {
            id: format!("al-{artist}"),
            artist_key: Some(String::from(artist)),
            ..Default::default()
        }
    }

    fn names(entries: &[ArtistInfo]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn with_no_criteria_the_grid_reads_by_name_in_any_case() {
        let artists = [artist("b", "beta", 1), artist("a", "Alpha", 1)];
        assert_eq!(names(&grid_entries(&[], &artists, &[])), ["Alpha", "beta"]);
    }

    #[test]
    fn tracks_sort_orders_by_count_then_name() {
        let artists = [artist("a", "Alpha", 1), artist("z", "Zeta", 9)];
        let criteria = [SortCriterion::new(
            ArtistSortField::Tracks,
            SortDirection::Desc,
        )];
        assert_eq!(
            names(&grid_entries(&[], &artists, &criteria)),
            ["Zeta", "Alpha"]
        );
    }

    /// Two artists the daemon keyed apart stay apart, however alike their names.
    #[test]
    fn albums_count_by_key_not_by_name() {
        let artists = [artist("one", "Same", 1), artist("two", "Same", 1)];
        let albums = [album("two"), album("two")];
        let criteria = [SortCriterion::new(
            ArtistSortField::Albums,
            SortDirection::Desc,
        )];
        let entries = grid_entries(&albums, &artists, &criteria);
        assert_eq!(entries[0].key, String::from("two"));
        assert_eq!(entries.len(), 2);
    }
}
