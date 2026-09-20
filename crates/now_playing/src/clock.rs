//! Clocks that hold still: tabular digits in a box sized for the longest reading it will hold.

use std::sync::{Arc, OnceLock};

use gpui_kit::{FontFeatures, Rems, SharedString, rems};
use widgets::format::duration;

const HOUR_MS: u64 = 3_600_000;
// `4:33` and `1:04:33` at the sizes the clocks are drawn.
const SHORT: f32 = 2.5;
const LONG: f32 = 4.;

/// Tabular figures, so a ticking second stops shuffling the minutes sideways.
pub fn tabular() -> FontFeatures {
    static FEATURES: OnceLock<FontFeatures> = OnceLock::new();
    FEATURES
        .get_or_init(|| FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
        .clone()
}

/// What the right-hand clock reads: the length, or how much of it is left.
pub fn trailing(position_ms: u64, total_ms: Option<u64>, remaining: bool) -> SharedString {
    let Some(total) = total_ms else {
        return "--:--".into();
    };
    if remaining {
        SharedString::from(format!("-{}", duration(total.saturating_sub(position_ms))))
    } else {
        duration(total)
    }
}

/// How wide both clocks are drawn, so neither resizes as the other one ticks.
pub fn width(total_ms: Option<u64>) -> Rems {
    match total_ms {
        Some(total) if total >= HOUR_MS => rems(LONG),
        _ => rems(SHORT),
    }
}
