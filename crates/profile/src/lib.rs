//! The frontend's profiler: a Chrome trace of the spans, and live counters for what the heap holds.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

mod counters;
mod heap;

pub use counters::{Counters, Snapshot, artwork, images, rebuilds};
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
            artwork_tracked = counters.artwork_tracked,
            artwork_loaded = counters.artwork_loaded,
            artwork_released = counters.artwork_released,
            artwork_bytes_mb = counters.artwork_bytes / 1_048_576,
            artwork_decoded_mb = counters.artwork_decoded / 1_048_576,
            rebuilds = counters.rebuilds,
            "profile"
        );
        // The renders since the last report, worst first; then cleared, so each line is an interval.
        let mut sections: Vec<(&'static str, SectionStat)> = {
            let mut held = SECTIONS.lock().unwrap_or_else(|held| held.into_inner());
            let taken = held.iter().map(|(name, stat)| (*name, *stat)).collect();
            held.clear();
            taken
        };
        sections.sort_unstable_by_key(|(_, stat)| std::cmp::Reverse(stat.total_us));
        for (name, stat) in sections.into_iter().take(4) {
            tracing::info!(
                view = name,
                renders = stat.calls,
                total_ms = stat.total_us / 1000,
                worst_ms = stat.worst_us / 1000,
                "render"
            );
        }
    }
}

/// Counts how often the window's whole view tree is rebuilt.
///
/// This is NOT frame time. GPUI renders a child during the parent's prepaint, so a guard taken
/// in the shell's `render` is dropped before any page has rendered, let alone been laid out or
/// painted. What it does measure is the thing worth watching: any entity notifying marks its
/// ancestors dirty, so one 250ms playhead tick rebuilds every visible view. A rate far above
/// the tick rate means something is notifying in a loop.
pub struct Rebuild;

impl Default for Rebuild {
    fn default() -> Self {
        Self::new()
    }
}

impl Rebuild {
    pub fn new() -> Self {
        REBUILDS.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

/// Times one view's `render`, which is where the work actually is.
///
/// `profile::render("albums")` at the top of a `Render::render`; the guard records on drop.
pub fn render(name: &'static str) -> Section {
    Section {
        name,
        started: Instant::now(),
    }
}

pub struct Section {
    name: &'static str,
    started: Instant,
}

impl Drop for Section {
    fn drop(&mut self) {
        let micros = self.started.elapsed().as_micros() as u64;
        let mut sections = SECTIONS.lock().unwrap_or_else(|held| held.into_inner());
        let stat = sections.entry(self.name).or_default();
        stat.calls += 1;
        stat.total_us += micros;
        stat.worst_us = stat.worst_us.max(micros);
    }
}

/// What one named render has cost since the last report.
#[derive(Debug, Clone, Copy, Default)]
pub struct SectionStat {
    pub calls: u64,
    pub total_us: u64,
    pub worst_us: u64,
}

pub(crate) static SECTIONS: std::sync::Mutex<
    std::collections::BTreeMap<&'static str, SectionStat>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(crate) static REBUILDS: AtomicU64 = AtomicU64::new(0);

pub(crate) static ARTWORK_LIVE: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARTWORK_BYTES: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARTWORK_DECODED: AtomicU64 = AtomicU64::new(0);

pub(crate) static ARTWORK_LOADED: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARTWORK_RELEASED: AtomicU64 = AtomicU64::new(0);

pub(crate) static ARTWORK_TRACKED: AtomicUsize = AtomicUsize::new(0);
