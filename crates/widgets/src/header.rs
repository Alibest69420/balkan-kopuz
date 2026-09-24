use std::rc::Rc;

use api::ArtworkRef;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window, div, px, rems,
};
use store::Route;

use crate::cover::Cover;
use crate::style;

/// The title block every page opens with, with its actions on the right.
#[derive(IntoElement)]
pub struct PageHeader {
    title: SharedString,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
}

impl PageHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            actions: Vec::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Taken and ignored: the nav bar owns history, so no page draws a back chevron of its own.
    pub fn back(self, _: Route) -> Self {
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for PageHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_3()
            .pb_6()
            .child(
                h_flex().gap_3().items_center().child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(self.title),
                        )
                        .when_some(self.subtitle, |column, subtitle| {
                            column.child(
                                div()
                                    .text_sm()
                                    .text_color(style::slate_400(cx))
                                    .child(subtitle),
                            )
                        }),
                ),
            )
            .child(h_flex().gap_2().items_center().children(self.actions))
    }
}

/// `w-64 h-64`: the cover a detail page opens with.
pub const HERO: Pixels = px(256.);

/// What a page does when its cover is clicked; only it knows what a new picture means.
type CoverAction = Rc<dyn Fn(&mut Window, &mut App)>;

/// The block every detail page opens with: the cover, what it is, and what you can do to it.
///
/// Album, artist and playlist detail all drew their own; the only differences that mattered were
/// the shape of the cover, the line above the title, and which buttons sat beside it.
#[derive(IntoElement)]
pub struct DetailHeader {
    title: SharedString,
    artwork: Option<ArtworkRef>,
    placeholder: &'static str,
    round: bool,
    size: Pixels,
    eyebrow: Option<AnyElement>,
    meta: Vec<SharedString>,
    actions: Vec<AnyElement>,
    cover_id: Option<SharedString>,
    on_cover: Option<CoverAction>,
    // A page whose cover carries its own overlay hands the whole thing over instead.
    custom_cover: Option<AnyElement>,
}

impl DetailHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            artwork: None,
            placeholder: crate::icons::DISC,
            round: false,
            size: HERO,
            eyebrow: None,
            meta: Vec::new(),
            actions: Vec::new(),
            cover_id: None,
            on_cover: None,
            custom_cover: None,
        }
    }

    /// Draw this instead of the plain cover, for a page whose cover is also an upload target.
    pub fn cover(mut self, cover: impl IntoElement) -> Self {
        self.custom_cover = Some(cover.into_any_element());
        self
    }

    pub fn artwork(mut self, artwork: Option<&ArtworkRef>) -> Self {
        self.artwork = artwork.cloned();
        self
    }

    pub fn placeholder(mut self, placeholder: &'static str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// An artist is a person, so their picture is a circle.
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// The line above the title: an artist link, a kind, a source.
    pub fn eyebrow(mut self, eyebrow: impl IntoElement) -> Self {
        self.eyebrow = Some(eyebrow.into_any_element());
        self
    }

    /// One more fact for the line under the title; they are joined with a dot.
    pub fn meta(mut self, fact: impl Into<SharedString>) -> Self {
        self.meta.push(fact.into());
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Make the cover a target for a new picture; the page decides what that means.
    pub fn on_cover(
        mut self,
        id: impl Into<SharedString>,
        act: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.cover_id = Some(id.into());
        self.on_cover = Some(Rc::new(act));
        self
    }
}

impl RenderOnce for DetailHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let radius = match self.round {
            true => self.size / 2.,
            false => px(12.),
        };
        let cover = Cover::new(self.artwork.as_ref())
            .size(self.size)
            .hq(true)
            .rounded(radius)
            .placeholder(self.placeholder);
        let meta = self.meta.clone();
        h_flex()
            .w_full()
            .gap_8()
            .items_end()
            .child(div().flex_shrink_0().map(|frame| {
                match self.custom_cover {
                    Some(custom) => frame.child(custom).into_any_element(),
                    None => match (self.cover_id, self.on_cover) {
                        (Some(id), Some(act)) => frame
                            .id(id)
                            .cursor_pointer()
                            .on_click(move |_, window, cx| act(window, cx))
                            .child(cover)
                            .into_any_element(),
                        _ => frame.child(cover).into_any_element(),
                    },
                }
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .children(self.eyebrow.map(|eyebrow| div().mb_2().child(eyebrow)))
                    .child(
                        div()
                            .mb_6()
                            .line_clamp(3)
                            .text_size(rems(4.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .when(!meta.is_empty(), |block| {
                        block.child(
                            h_flex()
                                .gap_6()
                                .items_center()
                                .flex_wrap()
                                .text_color(style::slate_400(cx))
                                .children(meta.into_iter().enumerate().flat_map(|(at, fact)| {
                                    let dot = (at > 0).then(|| div().child("•").into_any_element());
                                    dot.into_iter().chain(std::iter::once(
                                        div().child(fact).into_any_element(),
                                    ))
                                })),
                        )
                    }),
            )
            // Upstream the actions are the row's third column, at its bottom-right, not under the facts.
            .when(!self.actions.is_empty(), |row| {
                row.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_4()
                        .items_center()
                        .children(self.actions),
                )
            })
    }
}

/// The same shapes in the theme's placeholder wash, for a detail page that has not been answered.
pub fn detail_header_skeleton(size: Pixels, round: bool, cx: &App) -> impl IntoElement {
    let ink = cx.theme().skeleton;
    let bar = move |width: Pixels, height: Pixels| {
        div().w(width).h(height).rounded(style::radius(cx)).bg(ink)
    };
    h_flex()
        .w_full()
        .gap_8()
        .items_end()
        .child(
            div()
                .size(size)
                .flex_shrink_0()
                .bg(ink)
                .map(|block| match round {
                    true => block.rounded(size / 2.),
                    false => block.rounded(px(12.)),
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_4()
                .child(bar(px(120.), px(12.)))
                .child(bar(px(380.), px(48.)))
                .child(bar(px(220.), px(12.))),
        )
}
