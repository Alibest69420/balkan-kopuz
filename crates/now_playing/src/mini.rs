//! The mini surface: cover, title, transport and a hair of seek bar, in ~380×148.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Pixels, SharedString, Styled,
    Window, div, px,
};
use store::PlayerModel;
use widgets::format::t;
use widgets::{Cover, icons, style};

use crate::NowPlaying;
use crate::transport::Metrics;
use crate::{clock, track_duration_ms};

const COVER: Pixels = px(56.);
const RESTORE: Pixels = px(28.);
const RESTORE_GLYPH: Pixels = px(12.);

impl NowPlaying {
    pub(crate) fn render_mini(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (track, position_ms, total_ms) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (
                player.track().cloned(),
                player.position_ms(),
                track_duration_ms(player),
            )
        };
        let title = track
            .as_ref()
            .map(|track| SharedString::from(track.title.clone()))
            .unwrap_or_default();
        let artist = track
            .as_ref()
            .map(|track| SharedString::from(track.artist.clone()))
            .unwrap_or_default();
        let artwork = track.as_ref().and_then(|track| track.artwork.clone());
        let shown_ms = self.shown_ms(position_ms, total_ms);
        let reading: SharedString = format!(
            "{} {}",
            widgets::format::duration(shown_ms),
            clock::trailing(shown_ms, total_ms, true)
        )
        .into();

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
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .justify_center()
                            .gap_0p5()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(style::white(0.95, cx))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(style::text_11())
                                    .text_color(style::white(0.55, cx))
                                    .child(artist),
                            )
                            .child(
                                div()
                                    .text_size(style::text_9())
                                    .font_features(clock::tabular())
                                    .text_color(style::white(0.35, cx))
                                    .child(reading),
                            ),
                    )
                    .child(
                        Button::new("mini-restore")
                            .ghost()
                            .with_size(RESTORE_GLYPH * (4. / 3.))
                            .size(RESTORE)
                            .rounded(RESTORE / 2.)
                            .icon(icons::icon(icons::EXPAND))
                            .custom(style::bare_button(style::slate_400(cx), cx))
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
                    .child(self.transport(Metrics::COMPACT, false, window, cx)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .pb_2()
                    .child(self.seek_bar(window, cx)),
            )
            .into_any_element()
    }
}
