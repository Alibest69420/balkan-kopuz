//! What rides on a cover: the hover wash, the playing badge, the play button and a corner control.

use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};

use crate::icon_button::{IconButton, IconTone};
use crate::{icons, motion};

const CORNER: u64 = 10;

/// `w-10 h-10`: the play button over a cover's corner.
const PLAY_SIZE: Pixels = px(40.);

/// What a cover's play button shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Idle,
    /// What it names is the thing playing, so a press pauses.
    Playing,
    /// The press is on its way; the queue is not built yet.
    Loading,
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The accent play button that rides on a cover's corner; a press is never a click on the card under it.
#[derive(IntoElement)]
pub struct TilePlayButton {
    id: SharedString,
    state: PlayState,
    click: OnClick,
}

impl TilePlayButton {
    pub fn new(
        id: impl Into<SharedString>,
        state: PlayState,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state,
            click: Rc::new(click),
        }
    }
}

impl RenderOnce for TilePlayButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let glyph = match self.state {
            PlayState::Playing => icons::PAUSE,
            _ => icons::PLAY,
        };
        let click = self.click;
        IconButton::new(self.id, glyph)
            .size(PLAY_SIZE)
            .glyph(px(14.))
            .tone(IconTone::Accent)
            .shadow(true)
            .loading(self.state == PlayState::Loading)
            .stop_propagation(true)
            .on_click(move |event, window, cx| click(event, window, cx))
    }
}

/// `bg-primary` circle on a playing cover's corner.
pub fn playing_badge(cx: &App) -> AnyElement {
    div()
        .absolute()
        .bottom_2()
        .left_2()
        .size_6()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(cx.theme().primary)
        .child(
            icons::icon(icons::VOLUME)
                .size(px(12.))
                .text_color(cx.theme().primary_foreground),
        )
        .into_any_element()
}

/// A cover with the things that sit over it; the owner decides which are showing.
#[derive(IntoElement)]
pub struct CoverOverlay {
    id: SharedString,
    cover: AnyElement,
    radius: Pixels,
    wash: Option<Hsla>,
    playing: bool,
    play: Option<TilePlayButton>,
    corner: Option<AnyElement>,
}

impl CoverOverlay {
    pub fn new(id: impl Into<SharedString>, cover: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            cover: cover.into_any_element(),
            radius: px(0.),
            wash: None,
            playing: false,
            play: None,
            corner: None,
        }
    }

    /// The cover's corner radius, which the wash follows.
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    /// A tint over the whole cover, such as the dimming under the pointer.
    pub fn wash(mut self, wash: Option<Hsla>) -> Self {
        self.wash = wash;
        self
    }

    /// The badge that says what the cover names is playing.
    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn play(mut self, play: Option<TilePlayButton>) -> Self {
        self.play = play;
        self
    }

    /// A control in the top corner, whose press is never a click on the card under it.
    pub fn corner(mut self, corner: Option<AnyElement>) -> Self {
        self.corner = corner;
        self
    }
}

impl RenderOnce for CoverOverlay {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let radius = self.radius;
        div()
            .relative()
            .child(self.cover)
            .when_some(self.wash, |cover, wash| {
                cover.child(div().absolute().inset_0().rounded(radius).bg(wash))
            })
            .when(self.playing, |cover| cover.child(playing_badge(cx)))
            .when_some(self.play, |cover, play| {
                cover.child(div().absolute().right_2().bottom_2().child(play))
            })
            .when_some(self.corner, |cover, corner| {
                cover.child(
                    div()
                        .id(motion::channel(&id, CORNER))
                        .absolute()
                        .top_2()
                        .right_2()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(corner),
                )
            })
    }
}
