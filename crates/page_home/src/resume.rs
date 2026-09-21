//! Continue: the queue the last session was interrupted in, and the one button that picks it up.

use api::ArtworkRef;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, StyledImage as _,
    Window, div, img, linear_color_stop, linear_gradient, px, relative,
};
use store::{Navigator, PlayerModel, QueueModel, Route, Target};
use widgets::format::{t, t_with};
use widgets::{Cover, icons, menus, motion, style};

use crate::Home;

const HOVER: u64 = 0;
const WASH: u64 = 1;
const COVER: u64 = 2;

/// How much of the cover's own colour the stage wears, and how much under the pointer.
const STAGE: f32 = 0.22;
const STAGE_LIT: f32 = 0.30;
/// Enough blur that the cover reads as colour rather than as a second, smaller picture.
const STAGE_BLUR: u8 = 64;
/// The cover on the stage, against the 88px it used to be.
const STAGE_COVER: f32 = 132.;

/// The banner the page opens with, or nothing at all when no queue was left behind.
pub(crate) fn banner(window: &mut Window, cx: &mut Context<Home>) -> Option<AnyElement> {
    let queue = QueueModel::global(cx).read(cx);
    let track = queue.current()?;
    let left = queue
        .len()
        .saturating_sub(queue.position().unwrap_or(0) as usize + 1);
    let title = SharedString::from(track.title.clone());
    let artist = SharedString::from(track.artist.clone());
    let artwork = track.artwork.clone();
    let key = SharedString::new_static("home-continue");
    let route = match track.album_id.is_empty() {
        false => Route::Album(track.album_id.clone().into()),
        true => Route::Artist {
            name: artist.clone(),
            channel: None,
        },
    };
    let target = Target::track(track.key.clone(), track.title.clone());
    let playing = PlayerModel::global(cx).read(cx).is_playing();

    let (over, pointer) = motion::hovered(motion::channel(&key, HOVER), window, cx);
    // The stage takes the colour of what was last playing, so it is never the same twice.
    // `wash_for` answers None when the accent preference is off, and the plain lift is kept.
    let tint = theme::wash_for(theme::Surface::Chrome, window, cx);
    let wash = motion::color_over(
        motion::channel(&key, WASH),
        match tint {
            Some(colour) => colour.opacity(match over {
                true => STAGE_LIT,
                false => STAGE,
            }),
            None => style::white(
                match over {
                    true => style::HOVER,
                    false => style::RAISED,
                },
                cx,
            ),
        },
        theme::ACCENT_TWEEN,
        window,
        cx,
    );
    // `bg-white text-black`: an opaque fill rests on the instance, never on the variant's colour.
    let resume = style::dress(
        Button::new("home-resume"),
        cx.theme().foreground,
        style::white(style::TEXT_STRONG, cx),
        cx.theme().background,
        cx.theme().foreground,
        cx,
    );
    Some(
        h_flex()
            .id(key.clone())
            .relative()
            .w_full()
            .gap_6()
            .p_6()
            .items_center()
            .overflow_hidden()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(cx.theme().background)
            .cursor_pointer()
            .on_hover(motion::track(pointer))
            .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
            .children(backdrop(artwork.as_ref(), cx))
            .child(div().absolute().inset_0().bg(wash))
            .child(
                Cover::new(artwork.as_ref())
                    .size(px(STAGE_COVER))
                    .rounded(style::to_px(style::radius_lg(), window))
                    .zoom(motion::channel(&key, COVER), over),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_size(style::text_10())
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().primary_hover)
                            .child(t("home_continue")),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_2xl()
                            .line_height(relative(1.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                                    .child(artist),
                            )
                            .when(left > 0, |line| {
                                line.child(
                                    div()
                                        .text_sm()
                                        .text_color(style::white(style::TEXT_DIM, cx))
                                        .child(t_with(
                                            "home_queue_left",
                                            &[("count", left.to_string())],
                                        )),
                                )
                            }),
                    ),
            )
            .child(
                resume
                    .h_11()
                    .px_6()
                    .rounded_full()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .icon(icons::icon(match playing {
                        true => icons::PAUSE,
                        false => icons::PLAY,
                    }))
                    .label(t(match playing {
                        true => "pause",
                        false => "play",
                    }))
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                        PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx));
                    }),
            )
            .context_menu(menus::builder(target))
            .into_any_element(),
    )
}

/// The cover behind the stage, blurred until it reads as colour rather than a second picture.
///
/// It fades out downward so the stage sits on the page instead of being a box pasted onto it.
fn backdrop(artwork: Option<&ArtworkRef>, cx: &Context<Home>) -> Option<AnyElement> {
    let source = store::artwork_source(artwork?, true);
    // Overscanned by the blur radius, so the softened edge never shows as a seam.
    let scale = 1. + f32::from(STAGE_BLUR) * 0.004;
    let inset = (1. - scale) / 2.;
    let background = cx.theme().background;
    Some(
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .child(
                img(store::blurred_backdrop(source, STAGE_BLUR))
                    .absolute()
                    .top(relative(inset))
                    .left(relative(inset))
                    .w(relative(scale))
                    .h(relative(scale))
                    .object_fit(ObjectFit::Cover),
            )
            .child(div().absolute().inset_0().bg(linear_gradient(
                180.,
                linear_color_stop(background.opacity(0.35), 0.),
                linear_color_stop(background, 1.),
            )))
            .into_any_element(),
    )
}
