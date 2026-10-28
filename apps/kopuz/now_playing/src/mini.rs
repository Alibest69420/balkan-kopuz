//! The mini surface: cover, title, transport with shuffle and the heart, over the blurred cover, in ~380×148.

use components::format::t;
use components::{
    Cover, IconButton, IconTone, MetaLayout, NowPlayingMeta, TransportMetrics, icons, style,
};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled,
    Window, div, px,
};
use store::PlayerModel;

use crate::NowPlaying;

const COVER: Pixels = px(64.);
const RESTORE: Pixels = px(28.);
const RESTORE_GLYPH: Pixels = px(12.);
// The cover behind it: dark enough that white text and the accent always read over it.
const WASH_DARKENING: u8 = 45;
const WASH_BLUR: u8 = 40;
const GROUP: &str = "mini-player";

impl NowPlaying {
    pub(crate) fn render_mini(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track = self.track(cx);
        let artwork = track.as_ref().and_then(|track| track.artwork.clone());
        let swap = self.swap.frame(
            components::cover::is_ready(artwork.as_ref(), false, cx),
            window,
            cx,
        );
        let cover = |artwork: Option<&api::ArtworkRef>| {
            Cover::new(artwork)
                .size(COVER)
                .rounded(style::cover_radius())
        };
        let leaving_cover = swap
            .leaving
            .as_ref()
            .filter(|_| swap.cover < 1.)
            .map(|old| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(cover(old.artwork.as_ref()))
            });
        // The clock is one view, so only the arriving title carries it.
        let meta = |id: &'static str, track: &api::TrackInfo, clock: bool| {
            NowPlayingMeta::new(id, track)
                .layout(MetaLayout::Mini)
                .when(clock, |meta| {
                    meta.footer(div().children(self.clock.clone()))
                })
        };
        let title = match (track.as_ref(), swap.leaving.as_ref()) {
            (Some(track), Some(old)) if swap.title < 1. => crate::meta::crossfade(
                meta("mini-leaving", old, false),
                meta("mini", track, true),
                swap.title,
            )
            .flex_1()
            .min_w_0()
            .into_any_element(),
            (Some(track), _) => meta("mini", track, true).into_any_element(),
            (None, _) => div().flex_1().into_any_element(),
        };

        let wash = artwork.as_ref().map(|artwork| {
            components::backdrop::wallpaper(
                store::artwork_source(artwork, true),
                WASH_DARKENING,
                WASH_BLUR,
                window,
                cx,
            )
        });
        let metrics = TransportMetrics::COMPACT;
        let shuffle = PlayerModel::global(cx).read(cx).shuffle();
        // Shuffle and the heart flank the transport at the same size, so play stays centred.
        let heart = self
            .marks("mini-marks", false, track.as_ref(), cx)
            .dont_recommend(false)
            .size(metrics.side, metrics.side_glyph);

        v_flex()
            .group(GROUP)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(wash)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .pt_3()
                    .child(
                        div()
                            .relative()
                            .flex_shrink_0()
                            .children(leaving_cover)
                            .child(div().opacity(swap.cover).child(cover(artwork.as_ref()))),
                    )
                    .child(title),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .gap(metrics.gap)
                    .px_3()
                    .pb_1()
                    .child(
                        self.transport(metrics, true, cx)
                            .shuffle(Some(shuffle))
                            .repeat(None)
                            .balanced(false)
                            .filled(true),
                    )
                    .child(heart),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .pb_2()
                    .children(self.seek.clone()),
            )
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .invisible()
                    .group_hover(GROUP, |restore| restore.visible())
                    .child(
                        IconButton::new("mini-restore", icons::EXPAND)
                            .size(RESTORE)
                            .glyph(RESTORE_GLYPH)
                            .tone(IconTone::Scrim)
                            .tooltip(t("restore_full_player"))
                            .on_click(|_, _, cx| crate::close_mini(cx)),
                    ),
            )
            .into_any_element()
    }
}
