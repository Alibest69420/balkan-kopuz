//! The bar across the bottom: the one presentation that is always there.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, IntoElement, ParentElement as _, Pixels, Styled, Window, px,
    relative,
};
use store::{Chrome, ConfigModel, SidePanel};
use widgets::format::t;
use widgets::{focus, icons, motion, style};

use crate::NowPlaying;
use crate::transport::Metrics;

const UTILITY_GLYPH: Pixels = px(12.);
const TRANSPORT_WIDTH: Pixels = px(640.);

/// What the right end still affords. The title is what pays for it, so as the window narrows the
/// least useful goes first: the level's track, then share, then the mini window.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Affords {
    level: bool,
    share: bool,
    mini: bool,
    /// The dots beside the title; what it offers is on the block's own right-click either way.
    menu: bool,
    /// What the transport column takes, which is the rest of what the title has to live on.
    transport: f32,
}

const LEVEL_NEEDS: Pixels = px(1200.);
const SHARE_NEEDS: Pixels = px(1080.);
const MINI_NEEDS: Pixels = px(980.);
const WIDE_TRANSPORT: f32 = 0.4;
const TIGHT_TRANSPORT: f32 = 0.32;

fn affords(width: Pixels) -> Affords {
    Affords {
        level: width >= LEVEL_NEEDS,
        share: width >= SHARE_NEEDS,
        mini: width >= MINI_NEEDS,
        menu: width >= MINI_NEEDS,
        transport: match width >= LEVEL_NEEDS {
            true => WIDE_TRANSPORT,
            false => TIGHT_TRANSPORT,
        },
    }
}
// `bg-black/40` when something is painted behind the bar, `bg-black/60` when nothing is.
const SCRIM_OVER_ART: f32 = 0.4;
const SCRIM_PLAIN: f32 = 0.6;

impl NowPlaying {
    pub(crate) fn render_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // Anything behind the window is worth seeing through the bar, including the cover's own light.
        let over_art = theme::ambient(cx).is_some() || {
            let config = ConfigModel::global(cx);
            let config = config.read(cx);
            let config = config.config();
            config.cover_art_background || !config.custom_background_path.is_empty()
        };
        let scrim = if over_art {
            SCRIM_OVER_ART
        } else {
            SCRIM_PLAIN
        };

        h_flex()
            .w_full()
            .h(style::chrome().player_height)
            .flex_shrink_0()
            .px_4()
            .gap_4()
            .justify_between()
            .border_t_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::black(scrim, cx))
            .child(self.track_meta(window, cx))
            .child(
                v_flex()
                    .w(relative(affords(window.viewport_size().width).transport))
                    // Past this the seek bar sprawls and the title beside it starves.
                    .max_w(TRANSPORT_WIDTH)
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(self.transport(Metrics::COMPACT, true, window, cx))
                    .child(self.seek_row(window, cx)),
            )
            .child(self.utilities(window, cx))
            .into_any_element()
    }

    /// The right end of the bar: level, the two panels, where else it can play, and the two sizes.
    fn utilities(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let panel = if self.presentation == crate::Presentation::SheetBar {
            match self.tab {
                store::layout::SheetTab::Lyrics => Some(SidePanel::Lyrics),
                store::layout::SheetTab::Queue => Some(SidePanel::Queue),
                _ => None,
            }
        } else {
            Chrome::global(cx).read(cx).panel()
        };
        let wanted = store::layout(cx).player.clone();
        let room = affords(window.viewport_size().width);
        let muted = style::slate_400(cx);
        let accent = motion::accent(theme::Surface::Chrome, cx.theme().primary, window, cx);
        let queued = panel == Some(SidePanel::Queue);
        let lyrical = panel == Some(SidePanel::Lyrics);
        let queue_tint = self.control_glyph(
            "bar-queue",
            if queued { accent } else { muted },
            queued,
            window,
            cx,
        );
        let lyrics_tint = self.control_glyph(
            "bar-lyrics",
            if lyrical { accent } else { muted },
            lyrical,
            window,
            cx,
        );

        h_flex()
            // The same weight the meta block carries, so the transport between them is centred.
            .flex_1()
            .min_w_0()
            .gap_4()
            .justify_end()
            .when(wanted.volume, |row| {
                row.child(self.volume_control(room.level, window, cx))
            })
            .when(wanted.queue, |row| {
                row.child(
                    self.framed(
                        "bar-queue",
                        utility("bar-queue", icons::LIST, queue_tint, cx)
                            .tooltip(t("up_next"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.devices_open = false;
                                if this.presentation == crate::Presentation::SheetBar {
                                    this.set_tab(store::layout::SheetTab::Queue, cx);
                                    return;
                                }
                                Chrome::global(cx).update(cx, |chrome, cx| {
                                    chrome.toggle_panel(SidePanel::Queue, cx)
                                });
                            })),
                        window,
                        cx,
                    ),
                )
            })
            .when(wanted.lyrics, |row| {
                row.child(
                    self.framed(
                        "bar-lyrics",
                        utility("bar-lyrics", icons::LYRICS, lyrics_tint, cx)
                            .tooltip(t("lyrics"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.devices_open = false;
                                if this.presentation == crate::Presentation::SheetBar {
                                    this.set_tab(store::layout::SheetTab::Lyrics, cx);
                                    return;
                                }
                                Chrome::global(cx).update(cx, |chrome, cx| {
                                    chrome.toggle_panel(SidePanel::Lyrics, cx)
                                });
                            })),
                        window,
                        cx,
                    ),
                )
            })
            .children(self.devices_button(window, cx))
            .when(room.share, |row| {
                row.child(self.share_button(
                    style::chrome().round_button,
                    UTILITY_GLYPH,
                    window,
                    cx,
                ))
            })
            .when(room.mini, |row| {
                row.child(
                    self.framed(
                        "bar-mini",
                        utility(
                            "bar-mini",
                            icons::COLLAPSE,
                            self.control_glyph("bar-mini", muted, false, window, cx),
                            cx,
                        )
                        .tooltip(t("mini_player"))
                        .on_click(|_, _, cx| crate::open_mini(cx)),
                        window,
                        cx,
                    ),
                )
            })
            .child(
                self.framed(
                    "bar-sheet",
                    utility(
                        "bar-sheet",
                        icons::EXPAND,
                        self.control_glyph("bar-sheet", muted, false, window, cx),
                        cx,
                    )
                    .tooltip(t("now_playing"))
                    .on_click(|_, _, cx| {
                        Chrome::global(cx).update(cx, |chrome, cx| chrome.toggle_sheet(cx))
                    }),
                    window,
                    cx,
                ),
            )
    }
}

/// The shape every button at the right end of the bar takes.
pub(crate) fn utility(id: &'static str, glyph: &'static str, tint: Hsla, cx: &App) -> Button {
    let button = Button::new(id)
        .ghost()
        .with_size(UTILITY_GLYPH * (4. / 3.))
        .size(style::chrome().round_button)
        .rounded(style::chrome().round_button / 2.)
        .icon(icons::icon(glyph))
        .custom(style::bare_button(tint, cx));
    focus::quiet(button, cx)
}

/// Whether the title's own dots button still fits, which the meta block has to ask.
pub(crate) fn menu_fits(window: &Window) -> bool {
    affords(window.viewport_size().width).menu
}

/// The shape and glyph size of one bar-slot button, for a package contributing another.
pub fn utility_metrics() -> (Pixels, Pixels) {
    (style::chrome().round_button, UTILITY_GLYPH)
}

/// The bar owns its height; the shell does not have to guess it.
pub fn bar_height() -> Pixels {
    style::chrome().player_height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_keeps_the_whole_bar() {
        let room = affords(px(1400.));
        assert!(room.level && room.share && room.mini && room.menu);
        assert_eq!(room.transport, WIDE_TRANSPORT);
    }

    #[test]
    fn the_level_is_the_first_thing_the_title_takes_back() {
        let room = affords(px(1100.));
        assert!(!room.level);
        assert!(room.share && room.mini && room.menu);
        assert_eq!(room.transport, TIGHT_TRANSPORT);
    }

    #[test]
    fn a_narrow_window_keeps_only_what_the_bar_cannot_do_without() {
        let room = affords(px(924.));
        assert!(!room.level && !room.share && !room.mini && !room.menu);
    }

    #[test]
    fn the_transport_never_takes_more_of_a_narrow_window_than_a_wide_one() {
        assert!(affords(px(924.)).transport < affords(px(1400.)).transport);
    }
}
