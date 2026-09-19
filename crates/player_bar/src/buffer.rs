use api::BufferedRange;

// Two ranges this close read as one bar, so the seek track does not look shredded.
const MERGE_GAP_PERCENT: f32 = 0.75;
// A range that all but touches the playhead is pulled onto it, so no sliver of track shows through.
const PLAYHEAD_SNAP_PERCENT: f32 = 5.0;

/// Buffered byte ranges as `(left, right)` percentages, merged and snapped the way the bar draws them.
pub fn visual_ranges(ranges: &[BufferedRange], played_percent: f32) -> Vec<(f32, f32)> {
    let mut visual: Vec<(f32, f32)> = Vec::with_capacity(ranges.len());
    for range in ranges {
        let Some(total) = range.total.filter(|total| *total > 0) else {
            continue;
        };
        let start = (range.start as f32 / total as f32 * 100.0).clamp(0.0, 100.0);
        let end = (range.end as f32 / total as f32 * 100.0).clamp(start, 100.0);
        match visual.last_mut() {
            Some(previous) if start - previous.1 <= MERGE_GAP_PERCENT => {
                previous.1 = previous.1.max(end)
            }
            _ => visual.push((start, end)),
        }
    }

    let played_percent = played_percent.clamp(0.0, 100.0);
    if let Some(active) = visual.iter().position(|(start, end)| {
        *end >= played_percent && *start <= played_percent + PLAYHEAD_SNAP_PERCENT
    }) && visual[active].0 > played_percent
    {
        visual[active].0 = played_percent;
    }
    visual
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u64, end: u64) -> BufferedRange {
        BufferedRange {
            start,
            end,
            total: Some(1_000),
        }
    }

    #[test]
    fn small_gaps_coalesce_and_the_bar_attaches_to_the_playhead() {
        let ranges = [range(680, 700), range(704, 720)];
        assert_eq!(visual_ranges(&ranges, 65.0), vec![(65.0, 72.0)]);
    }

    #[test]
    fn distant_seek_ranges_stay_separate() {
        let ranges = [range(100, 200), range(800, 900)];
        assert_eq!(
            visual_ranges(&ranges, 10.0),
            vec![(10.0, 20.0), (80.0, 90.0)]
        );
    }

    #[test]
    fn a_range_without_a_total_is_not_drawn() {
        let ranges = [BufferedRange {
            start: 0,
            end: 10,
            total: None,
        }];
        assert!(visual_ranges(&ranges, 0.0).is_empty());
    }
}
