//! The internet is down: said quietly across the top of the page, which keeps working on what is here.

use components::format::t;
use components::{icons, style};
use gpui_kit::component::h_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Styled,
    Subscription, Window, div, px,
};
use store::{NetworkNotice, SourcesModel};

const GLYPH: Pixels = px(12.);
// A scrim like the scan line's, since nothing is wrong with the app itself.
const FILL: f32 = 0.45;

/// The network strip. It renders nothing while the internet is reachable.
pub struct NetworkBanner {
    _subscriptions: Vec<Subscription>,
}

impl NetworkBanner {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        Self {
            _subscriptions: vec![cx.observe(&sources, |_, _, cx| cx.notify())],
        }
    }

    /// Whether the strip has anything to say, for whoever lays it out.
    pub fn is_showing(cx: &gpui_kit::App) -> bool {
        SourcesModel::global(cx).read(cx).network_notice().is_some()
    }
}

impl Render for NetworkBanner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notice = SourcesModel::global(cx).read(cx).network_notice();
        let Some(NetworkNotice::Down) = notice else {
            return div().into_any_element();
        };
        let ink = style::white(style::TEXT_SECONDARY, cx);
        h_flex()
            .w_full()
            .flex_shrink_0()
            .occlude()
            .items_center()
            .gap_2()
            .px_4()
            .py_1p5()
            .text_xs()
            .bg(style::black(FILL, cx))
            .border_b_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .text_color(ink)
            .child(
                icons::icon(icons::WIFI_OFF)
                    .size(GLYPH)
                    .flex_shrink_0()
                    .text_color(ink),
            )
            .child(div().flex_1().min_w_0().child(t("error_network_down")))
            .into_any_element()
    }
}
