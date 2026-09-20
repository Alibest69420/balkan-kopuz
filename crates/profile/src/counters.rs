//! What the app is holding, counted where it is created rather than guessed from RSS.

use std::sync::atomic::Ordering;

use crate::{
    ARTWORK_BYTES, ARTWORK_DECODED, ARTWORK_LIVE, ARTWORK_LOADED, ARTWORK_RELEASED, FRAMES,
    SLOW_FRAMES,
};

/// One reading of every counter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub artwork_live: usize,
    pub artwork_loaded: u64,
    pub artwork_released: u64,
    pub artwork_bytes: u64,
    pub artwork_decoded: u64,
    pub frames: u64,
    pub slow_frames: u64,
}

pub struct Counters;

impl Counters {
    pub fn snapshot() -> Snapshot {
        Snapshot {
            artwork_live: ARTWORK_LIVE.load(Ordering::Relaxed),
            artwork_loaded: ARTWORK_LOADED.load(Ordering::Relaxed),
            artwork_released: ARTWORK_RELEASED.load(Ordering::Relaxed),
            artwork_bytes: ARTWORK_BYTES.load(Ordering::Relaxed),
            artwork_decoded: ARTWORK_DECODED.load(Ordering::Relaxed),
            frames: FRAMES.load(Ordering::Relaxed),
            slow_frames: SLOW_FRAMES.load(Ordering::Relaxed),
        }
    }
}

/// Artwork accounting, called from the asset loader.
pub mod artwork {
    use super::*;

    /// One cover arrived: `encoded` is what the daemon sent, `width`/`height` what it decodes to.
    pub fn loaded(encoded: usize, width: u32, height: u32) {
        ARTWORK_LIVE.fetch_add(1, Ordering::Relaxed);
        ARTWORK_LOADED.fetch_add(1, Ordering::Relaxed);
        ARTWORK_BYTES.fetch_add(encoded as u64, Ordering::Relaxed);
        // Every decoded cover is held as RGBA, whatever it was encoded as.
        ARTWORK_DECODED.fetch_add(u64::from(width) * u64::from(height) * 4, Ordering::Relaxed);
    }

    /// One cover was swept out of every cache holding it.
    pub fn released() {
        ARTWORK_LIVE.fetch_sub(1, Ordering::Relaxed);
        ARTWORK_RELEASED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Frame accounting, for callers that time their own work.
pub mod frames {
    use super::*;

    pub fn count() -> (u64, u64) {
        (
            FRAMES.load(Ordering::Relaxed),
            SLOW_FRAMES.load(Ordering::Relaxed),
        )
    }
}

/// How many decoded covers are alive, which is the number that explains the resident set.
pub mod images {
    use super::*;

    pub fn live() -> usize {
        ARTWORK_LIVE.load(Ordering::Relaxed)
    }
}
