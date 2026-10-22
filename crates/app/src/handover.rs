//! A source switch: the old page stays up until the new one's first reads land, then they crossfade.

use std::time::{Duration, Instant};

use components::motion::{self, Fade};
use gpui_kit::{AnyView, App, Window};

/// Long enough for the new page to be built and to start asking.
const SETTLE: Duration = Duration::from_millis(50);
/// A slow source gets no longer than this before its page comes up half-filled.
const PATIENCE: Duration = Duration::from_millis(1_500);

/// Whether the new page has waited enough: its reads are in, or the wait has run out.
pub fn ready(waited: Duration, in_flight: usize) -> bool {
    waited >= PATIENCE || (waited >= SETTLE && in_flight == 0)
}

/// The page being handed over from.
pub struct Handover {
    leaving: AnyView,
    since: Instant,
    arriving: Fade,
}

/// How a frame of the handover draws the two pages.
pub struct Frame {
    pub leaving: Option<AnyView>,
    pub arriving: f32,
}

impl Handover {
    pub fn new(leaving: AnyView) -> Self {
        Self {
            leaving,
            since: Instant::now(),
            arriving: Fade::hidden(),
        }
    }

    /// This frame; `None` once the new page is fully in and the old one can go.
    pub fn frame(&mut self, window: &mut Window, cx: &App) -> Option<Frame> {
        let landed = ready(self.since.elapsed(), store::FirstRead::in_flight(cx));
        if !landed {
            // Nothing else redraws the shell while a page in the background loads.
            window.request_animation_frame();
            return Some(Frame {
                leaving: Some(self.leaving.clone()),
                arriving: 0.,
            });
        }
        let arriving = self.arriving.sample(true, motion::MEDIUM, window, cx);
        (arriving < 1.).then(|| Frame {
            leaving: Some(self.leaving.clone()),
            arriving,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_page_waits_for_its_first_reads() {
        assert!(
            !ready(Duration::ZERO, 0),
            "not before it has had a chance to ask"
        );
        assert!(!ready(SETTLE, 3));
        assert!(ready(SETTLE, 0));
    }

    #[test]
    fn a_slow_source_does_not_hold_the_old_page_forever() {
        assert!(ready(PATIENCE, 3));
    }
}
