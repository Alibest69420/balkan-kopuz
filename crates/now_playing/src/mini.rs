//! The mini surface: cover, title, transport and a hair of seek bar, in ~380×148.

use components::format::t;
use components::{Cover, IconButton, MetaLayout, NowPlayingMeta, TransportMetrics, icons, style};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Pixels, Styled, Window, div, px,
};

use crate::NowPlaying;

const COVER: Pixels = px(56.);
const RESTORE: Pixels = px(28.);
const RESTORE_GLYPH: Pixels = px(12.);

impl NowPlaying {
    pub(crate) fn render_mini(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let track = self.track(cx);
        let artwork = track.as_ref().and_then(|track| track.artwork.clone());

        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .pt_3()
                    .child(
                        Cover::new(artwork.as_ref())
                            .size(COVER)
                            .rounded(style::cover_radius()),
                    )
                    .child(match track.as_ref() {
                        Some(track) => NowPlayingMeta::new("mini", track)
                            .layout(MetaLayout::Mini)
                            .footer(div().children(self.clock.clone()))
                            .into_any_element(),
                        None => div().flex_1().into_any_element(),
                    })
                    .child(
                        IconButton::new("mini-restore", icons::EXPAND)
                            .size(RESTORE)
                            .glyph(RESTORE_GLYPH)
                            .tooltip(t("restore_full_player"))
                            .on_click(|_, _, cx| crate::close_mini(cx)),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .pb_1()
                    .child(self.transport(TransportMetrics::COMPACT, false, cx)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .pb_2()
                    .children(self.seek.clone()),
            )
            .into_any_element()
    }
}
