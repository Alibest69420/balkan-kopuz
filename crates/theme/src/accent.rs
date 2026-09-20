//! The accent the playing cover lends the chrome, once the *Accent from artwork* preference has had its say.

use std::time::Duration;

use gpui_kit::{App, Hsla, Window};
use store::layout::Accent;
use store::{PlayerModel, Swatches, api::ArtworkRef};

/// How long the accent takes to travel on a track change.
pub const ACCENT_TWEEN: Duration = Duration::from_millis(600);

/// Where an accent is being asked for, which is what *sheet only* discriminates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The now-playing sheet and anything that opens with it.
    Sheet,
    /// The bar, the seek fill, the playing row: everything that is always on screen.
    Chrome,
}

impl Surface {
    /// A stable name, for the motion channel a tween on this surface is keyed by.
    pub fn key(self) -> &'static str {
        match self {
            Surface::Sheet => "sheet",
            Surface::Chrome => "chrome",
        }
    }

    fn allowed(self, accent: Accent) -> bool {
        match accent {
            Accent::On => true,
            Accent::SheetOnly => self == Surface::Sheet,
            Accent::Off => false,
        }
    }
}

/// One cover's colours, with no regard for the preference; extraction starts on the first call.
pub fn swatches(
    artwork: Option<&ArtworkRef>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Swatches> {
    store::palette::of(artwork?, window, cx)
}

/// The accent a given cover lends, with no regard for the preference.
pub fn accent_from(
    artwork: Option<&ArtworkRef>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Hsla> {
    swatches(artwork, window, cx).map(|swatches| swatches.vibrant)
}

/// The accent `surface` should be wearing: the playing cover's, or `None` to stay on the theme's.
pub fn accent_for(surface: Surface, window: &mut Window, cx: &mut App) -> Option<Hsla> {
    playing(surface, cx).and_then(|artwork| accent_from(Some(&artwork), window, cx))
}

/// The wash behind `surface`: the playing cover's bulk colour rather than its liveliest.
pub fn wash_for(surface: Surface, window: &mut Window, cx: &mut App) -> Option<Hsla> {
    let artwork = playing(surface, cx)?;
    swatches(Some(&artwork), window, cx).map(|swatches| swatches.dominant)
}

fn playing(surface: Surface, cx: &App) -> Option<ArtworkRef> {
    if !surface.allowed(store::layout(cx).accent) {
        return None;
    }
    PlayerModel::global(cx)
        .read(cx)
        .track()
        .and_then(|track| track.artwork.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_lends_its_accent_to_nothing() {
        assert!(!Surface::Sheet.allowed(Accent::Off));
        assert!(!Surface::Chrome.allowed(Accent::Off));
    }

    #[test]
    fn sheet_only_leaves_the_always_on_chrome_alone() {
        assert!(Surface::Sheet.allowed(Accent::SheetOnly));
        assert!(!Surface::Chrome.allowed(Accent::SheetOnly));
    }

    #[test]
    fn on_reaches_every_surface() {
        assert!(Surface::Sheet.allowed(Accent::On));
        assert!(Surface::Chrome.allowed(Accent::On));
    }

    #[test]
    fn every_surface_keys_its_own_motion_channel() {
        assert_ne!(Surface::Sheet.key(), Surface::Chrome.key());
    }
}
