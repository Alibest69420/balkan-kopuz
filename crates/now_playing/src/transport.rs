//! Shuffle · previous · play · next · repeat, plus the overflow the timer and the speed will land in.

use api::LoopMode;
use components::format::t;
use components::{focus, icons, motion, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, FontWeight, Hsla, IntoElement, ParentElement as _, Pixels, Styled, Window, div,
    px, relative,
};
use store::PlayerModel;

// Lucide names the shared icon set has no constant for yet.
const TIMER: &str = "icons/timer.svg";
const GAUGE: &str = "icons/gauge.svg";

// `text-[9px] leading-none`: the "1" over a repeat-one button.
const BADGE_TEXT: Pixels = px(9.);

/// How big one surface draws the transport.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Metrics {
    pub side: Pixels,
    pub step: Pixels,
    pub play: Pixels,
    pub side_glyph: Pixels,
    pub step_glyph: Pixels,
    pub play_glyph: Pixels,
    pub gap: Pixels,
}

impl Metrics {
    /// The bar's, and the mini window's: small enough to sit under a 96px bar.
    pub(crate) const COMPACT: Self = Self {
        side: px(36.),
        step: px(40.),
        play: px(40.),
        side_glyph: px(14.),
        step_glyph: px(20.),
        // The Dioxus bar drew this smaller than the skips and let a solid glyph carry the weight;
        // these are outlines, so the size has to do it, as `HERO` already has it do.
        play_glyph: px(22.),
        gap: px(8.),
    };

    /// The sheet's: the one surface with room for a real transport.
    pub(crate) const HERO: Self = Self {
        side: px(44.),
        step: px(56.),
        play: px(64.),
        side_glyph: px(18.),
        step_glyph: px(26.),
        play_glyph: px(28.),
        gap: px(16.),
    };
}

use crate::NowPlaying;

impl NowPlaying {
    /// The transport row. `full` draws the optional side toggles the preferences allow.
    pub(crate) fn transport(
        &self,
        metrics: Metrics,
        full: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (playing, shuffle, loop_mode, loaded) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (
                player.is_playing(),
                player.shuffle(),
                player.loop_mode(),
                player.track().is_some(),
            )
        };
        let wanted = store::layout(cx).player.clone();
        let balanced = full && self.presentation == crate::Presentation::Sheet;
        let accent = motion::accent(theme::Surface::Chrome, cx.theme().primary, window, cx);
        let idle = style::slate_400(cx);
        let repeating = loop_mode != LoopMode::None;

        let shuffle_glyph = self.control_glyph(
            "shuffle",
            if shuffle { accent } else { idle },
            shuffle,
            window,
            cx,
        );
        let previous_glyph = self.control_glyph("previous", idle, false, window, cx);
        let play_glyph = self.control_glyph("play-pause", cx.theme().foreground, true, window, cx);
        let next_glyph = self.control_glyph("next", idle, false, window, cx);
        let repeat_glyph = self.control_glyph(
            "repeat",
            if repeating { accent } else { idle },
            repeating,
            window,
            cx,
        );

        h_flex()
            .gap(metrics.gap)
            .items_center()
            .when(balanced && !wanted.shuffle && wanted.repeat, |row| {
                row.child(div().w(metrics.side).flex_shrink_0())
            })
            .when(full && wanted.shuffle, |row| {
                row.child(
                    self.control_slot("shuffle", metrics.side, window, cx)
                        .child(
                            bare(
                                "shuffle",
                                icons::SHUFFLE,
                                metrics.side,
                                metrics.side_glyph,
                                shuffle_glyph,
                                cx,
                            )
                            .tooltip(if shuffle {
                                t("shuffle_on")
                            } else {
                                t("shuffle_off")
                            })
                            .on_click(move |_, _, cx| {
                                PlayerModel::global(cx)
                                    .update(cx, |player, cx| player.set_shuffle(!shuffle, cx));
                            }),
                        ),
                )
            })
            .child(
                self.control_slot("previous", metrics.step, window, cx)
                    .child(
                        bare(
                            "previous",
                            icons::SKIP_BACK,
                            metrics.step,
                            metrics.step_glyph,
                            previous_glyph,
                            cx,
                        )
                        .disabled(!loaded)
                        .tooltip(t("previous_track"))
                        .on_click(|_, _, cx| {
                            PlayerModel::global(cx).update(cx, |player, cx| player.previous(cx));
                        }),
                    ),
            )
            .child(
                self.control_slot("play-pause", metrics.play, window, cx)
                    .child(
                        bare(
                            "play-pause",
                            if playing { icons::PAUSE } else { icons::PLAY },
                            metrics.play,
                            metrics.play_glyph,
                            play_glyph,
                            cx,
                        )
                        .disabled(!loaded)
                        .tooltip(if playing { t("pause") } else { t("play") })
                        .on_click(|_, _, cx| {
                            PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                        }),
                    ),
            )
            .child(
                self.control_slot("next", metrics.step, window, cx).child(
                    bare(
                        "next",
                        icons::SKIP_FORWARD,
                        metrics.step,
                        metrics.step_glyph,
                        next_glyph,
                        cx,
                    )
                    .disabled(!loaded)
                    .tooltip(t("next_track"))
                    .on_click(|_, _, cx| {
                        PlayerModel::global(cx).update(cx, |player, cx| player.next(cx));
                    }),
                ),
            )
            .when(full && wanted.repeat, |row| {
                row.child(
                    self.control_slot("repeat", metrics.side, window, cx)
                        .relative()
                        .flex()
                        .child(
                            bare(
                                "repeat",
                                icons::REPEAT,
                                metrics.side,
                                metrics.side_glyph,
                                repeat_glyph,
                                cx,
                            )
                            .tooltip(match loop_mode {
                                LoopMode::None => t("repeat_off"),
                                LoopMode::Queue => t("repeat_queue"),
                                LoopMode::Track => t("repeat_track"),
                            })
                            .on_click(|_, _, cx| {
                                PlayerModel::global(cx)
                                    .update(cx, |player, cx| player.cycle_loop_mode(cx));
                            }),
                        )
                        .when(loop_mode == LoopMode::Track, |wrapper| {
                            wrapper.child(
                                div()
                                    .absolute()
                                    .bottom(px(2.))
                                    .left_0()
                                    .right_0()
                                    .text_center()
                                    .text_size(BADGE_TEXT)
                                    .line_height(relative(1.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(repeat_glyph)
                                    .child("1"),
                            )
                        }),
                )
            })
            .when(balanced && !wanted.repeat && wanted.shuffle, |row| {
                row.child(div().w(metrics.side).flex_shrink_0())
            })
    }

    /// The overflow beside the transport: where a sleep timer and a playback speed will live.
    pub(crate) fn transport_overflow(
        &self,
        size: Pixels,
        glyph: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let tint = style::slate_400(cx);
        focus::quiet(Button::new("transport-overflow"), cx)
            .ghost()
            .with_size(glyph * (4. / 3.))
            .size(size)
            .rounded(size / 2.)
            .icon(icons::icon(icons::MORE))
            .custom(style::bare_button(tint, cx))
            .dropdown_menu(|menu, _, _| slots(menu))
    }
}

/// The two controls the daemon has not grown yet, drawn so the shape of the menu is already right.
fn slots(menu: PopupMenu) -> PopupMenu {
    menu.label(t("playback_extras"))
        .item(
            PopupMenuItem::new(t("sleep_timer"))
                .icon(icons::icon(TIMER))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new(t("playback_speed"))
                .icon(icons::icon(GAUGE))
                .disabled(true),
        )
}

/// A round glyph with no hover of its own, so the slot under it owns the tweened wash.
fn bare(
    id: &'static str,
    glyph: &'static str,
    size: Pixels,
    glyph_size: Pixels,
    color: Hsla,
    cx: &App,
) -> Button {
    // Button draws its glyph at three quarters of its own size, so the box is restated after it.
    let button = Button::new(id)
        .ghost()
        .with_size(glyph_size * (4. / 3.))
        .size(size)
        .rounded(size / 2.)
        .icon(icons::icon(glyph))
        .custom(style::bare_button(color, cx));
    focus::quiet(button, cx)
}
