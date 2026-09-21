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
// `bg-black/40` when something is painted behind the bar, `bg-black/60` when nothing is.
const SCRIM_OVER_ART: f32 = 0.4;
const SCRIM_PLAIN: f32 = 0.6;

impl NowPlaying {
    pub(crate) fn render_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let over_art = {
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
                    .w(relative(0.4))
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
        let panel = Chrome::global(cx).read(cx).panel();
        let wanted = store::layout(cx).player.clone();
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
            .gap_1()
            .justify_end()
            .when(wanted.volume, |row| {
                row.child(self.volume_control(window, cx))
            })
            .when(wanted.queue, |row| {
                row.child(
                    self.framed(
                        "bar-queue",
                        utility("bar-queue", icons::LIST, queue_tint, cx)
                            .tooltip(t("up_next"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.devices_open = false;
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
            .child(self.share_button(style::chrome().round_button, UTILITY_GLYPH, window, cx))
            .child(
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

/// The shape and glyph size of one bar-slot button, for a package contributing another.
pub fn utility_metrics() -> (Pixels, Pixels) {
    (style::chrome().round_button, UTILITY_GLYPH)
}

/// The bar owns its height; the shell does not have to guess it.
pub fn bar_height() -> Pixels {
    style::chrome().player_height
}
