//! Which artists the grid shows, and under what key, ported from the Dioxus `utils::artist`.

use std::collections::{BTreeSet, HashMap, HashSet};

use api::{AlbumInfo, ArtistInfo, ArtworkRef, TrackInfo};
use config::{ArtistSortField, SortCriterion, SortDirection};

/// The key artist caches and this page's buckets are stored under: trimmed, lowercased.
pub fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// The normalized primary of a joined collab credit, or None for a plain name.
pub fn joined_credit_primary(norm: &str) -> Option<&str> {
    let (primary, rest) = norm.split_once(',')?;
    let primary = primary.trim();
    (!primary.is_empty() && !rest.trim().is_empty()).then_some(primary)
}

/// One tile: the display name and the photo the daemon resolved for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistEntry {
    pub name: String,
    pub artwork: Option<ArtworkRef>,
    /// The source's own id, where a listing linked this name to one.
    pub id: Option<String>,
}

/// Every credited name, deduped by exact spelling, minus joined credits whose primary stands alone.
pub fn photo_queue(albums: &[AlbumInfo], samples: &[TrackInfo]) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for album in albums {
        if !album.artist.trim().is_empty() {
            names.insert(album.artist.clone());
        }
    }
    for track in samples {
        for artist in &track.artists {
            if !artist.trim().is_empty() {
                names.insert(artist.clone());
            }
        }
    }
    let norms: HashSet<String> = names.iter().map(|name| normalize(name)).collect();
    let mut names: Vec<String> = names
        .into_iter()
        .filter(|name| {
            let norm = normalize(name);
            !joined_credit_primary(&norm).is_some_and(|primary| norms.contains(primary))
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// The grid, in its sorted order: album artists ∪ sample-track credits, collapsed by normalized name.
pub fn grid_entries(
    albums: &[AlbumInfo],
    samples: &[TrackInfo],
    artists: &[ArtistInfo],
    criteria: &[SortCriterion<ArtistSortField>],
) -> Vec<ArtistEntry> {
    let mut display: HashMap<String, String> = HashMap::new();
    for album in albums {
        if album.artist.trim().is_empty() {
            continue;
        }
        display
            .entry(normalize(&album.artist))
            .or_insert_with(|| album.artist.clone());
    }
    for track in samples {
        for artist in &track.artists {
            if artist.trim().is_empty() {
                continue;
            }
            display
                .entry(normalize(artist))
                .or_insert_with(|| artist.clone());
        }
    }
    let joined: Vec<String> = display
        .keys()
        .filter(|norm| joined_credit_primary(norm).is_some_and(|p| display.contains_key(p)))
        .cloned()
        .collect();
    for norm in joined {
        display.remove(&norm);
    }

    let photos = photos_by_key(artists);
    let mut track_counts: HashMap<String, u32> = HashMap::new();
    for artist in artists {
        *track_counts.entry(normalize(&artist.name)).or_default() += artist.track_count;
    }
    let mut album_counts: HashMap<String, u32> = HashMap::new();
    for album in albums {
        *album_counts.entry(normalize(&album.artist)).or_default() += 1;
    }

    let ids: HashMap<String, String> = artists
        .iter()
        .filter_map(|artist| Some((normalize(&artist.name), artist.id.clone()?)))
        .collect();
    let mut keyed: Vec<(String, ArtistEntry)> = display
        .into_iter()
        .map(|(norm, name)| {
            let artwork = photos.get(&norm).cloned();
            let id = ids.get(&norm).cloned();
            (norm, ArtistEntry { name, artwork, id })
        })
        .collect();
    keyed.sort_by(|(left, _), (right, _)| {
        for criterion in criteria {
            let ordering = match criterion.field {
                ArtistSortField::Name => left.cmp(right),
                ArtistSortField::Tracks => {
                    count(&track_counts, left).cmp(&count(&track_counts, right))
                }
                ArtistSortField::Albums => {
                    count(&album_counts, left).cmp(&count(&album_counts, right))
                }
            };
            let ordering = match criterion.direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.cmp(right)
    });
    keyed.into_iter().map(|(_, entry)| entry).collect()
}

/// The daemon's photo for each artist, by normalized name.
pub fn photos_by_key(artists: &[ArtistInfo]) -> HashMap<String, ArtworkRef> {
    artists
        .iter()
        .filter_map(|artist| {
            let artwork = artist.artwork.clone()?;
            Some((normalize(&artist.name), artwork))
        })
        .collect()
}

fn count(counts: &HashMap<String, u32>, key: &str) -> u32 {
    counts.get(key).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(artist: &str) -> AlbumInfo {
        AlbumInfo {
            id: format!("al-{artist}"),
            title: "A".into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    fn track(artists: &[&str]) -> TrackInfo {
        TrackInfo {
            key: "/music/x.flac".into(),
            artists: artists.iter().map(|name| name.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(normalize("  COOL&CREATE "), "cool&create");
    }

    #[test]
    fn joined_credit_primary_splits_only_real_joins() {
        assert_eq!(
            joined_credit_primary("cool&create, beatmario, & maron"),
            Some("cool&create")
        );
        assert_eq!(joined_credit_primary("cool&create"), None);
        assert_eq!(joined_credit_primary("name,"), None);
        assert_eq!(joined_credit_primary(", name"), None);
    }

    #[test]
    fn a_joined_credit_loses_its_tile_to_its_primary() {
        let albums = [album("COOL&CREATE")];
        let samples = [track(&["COOL&CREATE, beatMARIO, & MARON"])];
        let entries = grid_entries(&albums, &samples, &[], &[]);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["COOL&CREATE"]);
    }

    #[test]
    fn a_comma_name_keeps_its_tile_when_no_primary_stands_alone() {
        let samples = [track(&["Tyler, The Creator"])];
        let entries = grid_entries(&[], &samples, &[], &[]);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Tyler, The Creator"]);
    }

    #[test]
    fn differently_cased_credits_collapse_into_one_tile() {
        let albums = [album("Aphex Twin")];
        let samples = [track(&["aphex twin"])];
        let entries = grid_entries(&albums, &samples, &[], &[]);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn tracks_sort_orders_by_credit_count_then_name() {
        let artists = [
            ArtistInfo {
                name: "Zeta".into(),
                track_count: 9,
                ..Default::default()
            },
            ArtistInfo {
                name: "Alpha".into(),
                track_count: 1,
                ..Default::default()
            },
        ];
        let albums = [album("Zeta"), album("Alpha")];
        let criteria = [SortCriterion::new(
            ArtistSortField::Tracks,
            SortDirection::Desc,
        )];
        let entries = grid_entries(&albums, &[], &artists, &criteria);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Zeta", "Alpha"]);
    }

    #[test]
    fn the_photo_queue_drops_joined_credits_and_blanks() {
        let albums = [album("COOL&CREATE"), album("   ")];
        let samples = [track(&["COOL&CREATE, beatMARIO, & MARON", "Alpha"])];
        assert_eq!(photo_queue(&albums, &samples), ["Alpha", "COOL&CREATE"]);
    }
}
