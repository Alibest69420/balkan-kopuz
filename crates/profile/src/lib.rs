//! The frontend's profiler: a Chrome trace of the spans, and live counters for what the heap holds.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

mod counters;
mod heap;

pub use counters::{Counters, Snapshot, artwork, frames, images};
pub use heap::Heap;

/// Kept alive for the process; dropping it is what flushes the trace file.
pub struct Session {
    _chrome: Option<tracing_chrome::FlushGuard>,
}

/// Installs the log subscriber, and a Chrome trace layer when `KOPUZ_TRACE` names a file.
///
/// `KOPUZ_TRACE=trace.json kopuz-gpui` then opens in <https://ui.perfetto.dev>.
pub fn init() -> Session {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let trace = std::env::var("KOPUZ_TRACE").ok();
    match trace {
        Some(path) => {
            let (chrome, guard) = tracing_chrome::ChromeLayerBuilder::new()
                .file(&path)
                .include_args(true)
                .build();
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .with(chrome)
                .init();
            tracing::info!(path, "writing a chrome trace; open it in ui.perfetto.dev");
            Session {
                _chrome: Some(guard),
            }
        }
        None => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
            Session { _chrome: None }
        }
    }
}

/// How often the heartbeat reports, since a profiler nobody reads is worth nothing.
pub const HEARTBEAT: Duration = Duration::from_secs(5);

/// Reports the heap and the counters; drive it from a timer, not from a render.
pub struct Heartbeat {
    heap: Heap,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

impl Heartbeat {
    pub fn new() -> Self {
        Self { heap: Heap::new() }
    }

    /// Logs one line of what the process is holding.
    pub fn report(&mut self) {
        let counters = Counters::snapshot();
        let (allocated, resident) = self.heap.read();
        tracing::info!(
            allocated_mb = allocated / 1_048_576,
            resident_mb = resident / 1_048_576,
            artwork_live = counters.artwork_live,
            artwork_loaded = counters.artwork_loaded,
            artwork_released = counters.artwork_released,
            artwork_bytes_mb = counters.artwork_bytes / 1_048_576,
            artwork_decoded_mb = counters.artwork_decoded / 1_048_576,
            frames = counters.frames,
            slow_frames = counters.slow_frames,
            "profile"
        );
    }
}

/// A frame that took longer than this is worth counting; 16.6ms is one refresh at 60Hz.
const SLOW_FRAME: Duration = Duration::from_micros(16_600);

/// Times a frame and records whether it missed the refresh.
pub struct Frame(Instant);

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

/// Counted on drop, so a render with several return paths needs no bookkeeping.
impl Drop for Frame {
    fn drop(&mut self) {
        FRAMES.fetch_add(1, Ordering::Relaxed);
        if self.0.elapsed() >= SLOW_FRAME {
            SLOW_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) static FRAMES: AtomicU64 = AtomicU64::new(0);
pub(crate) static SLOW_FRAMES: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARTWORK_LIVE: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARTWORK_BYTES: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARTWORK_DECODED: AtomicU64 = AtomicU64::new(0);

pub(crate) static ARTWORK_LOADED: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARTWORK_RELEASED: AtomicU64 = AtomicU64::new(0);
