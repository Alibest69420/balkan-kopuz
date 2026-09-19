//! The featured-album banner: the newest recent track, its cover, and the resize handle edit mode adds.

use api::QueueContext;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, DragMoveEvent, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, ObjectFit, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Window, div, img, linear_color_stop,
    linear_gradient, px,
};
use store::{ConfigModel, FavoritesModel, actions};
use widgets::format::{t, t_with};
use widgets::icons;

use crate::Home;

const MIN_HEIGHT: f32 = 140.;
const MAX_HEIGHT: f32 = 800.;

/// The dragged value that tells the hero's move listener the gesture is its own.
pub(crate) struct HeroResize;

impl Render for HeroResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Home {
    pub(crate) fn hero(&mut self, height: u32, edit: bool, cx: &mut Context<Self>) -> AnyElement {
        let banner = self.hero_banner(cx);
        div()
            .relative()
            .w_full()
            .h(px(height as f32))
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .bg(cx.theme().secondary)
            .children(banner)
            .when(edit, |hero| hero.child(self.resize_handle(cx)))
            .into_any_element()
    }

    fn hero_banner(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let hero = self.data.hero.as_ref()?;
        let album_id = hero.track.album_id.clone();
        let artwork = hero
            .album
            .as_ref()
            .and_then(|album| album.artwork.clone())
            .or_else(|| hero.track.artwork.clone());
        let title = hero.track.title.clone();
        let artist = if crate::data::is_unknown_artist(&hero.track.artist) {
            hero.album
                .as_ref()
                .map(|album| album.artist.clone())
                .unwrap_or_default()
        } else {
            hero.track.artist.clone()
        };
        let background = cx.theme().background;
        Some(
            div()
                .absolute()
                .inset_0()
                .size_full()
                .when_some(artwork, |frame, artwork| {
                    frame.child(
                        img(store::artwork_source(&artwork, true))
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .child(div().absolute().inset_0().size_full().bg(linear_gradient(
                    90.,
                    linear_color_stop(background.opacity(0.95), 0.),
                    linear_color_stop(background.opacity(0.2), 1.),
                )))
                .child(
                    v_flex()
                        .relative()
                        .size_full()
                        .justify_center()
                        .gap_2()
                        .p_8()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(cx.theme().primary)
                                .child(t("featured_album")),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_3xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .when(!artist.is_empty(), |column| {
                            column.child(
                                div()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t_with("by_artist_full", &[("artist", artist)])),
                            )
                        })
                        .child(
                            h_flex()
                                .pt_4()
                                .gap_3()
                                .items_center()
                                .child(
                                    Button::new("hero-play")
                                        .primary()
                                        .icon(icons::icon(icons::PLAY))
                                        .label(t("start_listening"))
                                        .on_click(move |_, _, cx| {
                                            if !album_id.is_empty() {
                                                actions::play_context(
                                                    QueueContext::Album {
                                                        id: album_id.clone(),
                                                    },
                                                    Some(0),
                                                    cx,
                                                );
                                            }
                                        }),
                                )
                                .child(self.hero_heart(cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The album is favorited when every one of its tracks is; the click brings the rest into line.
    fn hero_heart(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let keys: Vec<String> = self
            .hero_tracks
            .read(cx)
            .value()
            .map(|page| page.items.iter().map(|track| track.key.clone()).collect())
            .unwrap_or_default();
        let favorited = {
            let favorites = FavoritesModel::global(cx);
            let favorites = favorites.read(cx);
            !keys.is_empty()
                && keys
                    .iter()
                    .all(|key| !key.is_empty() && favorites.is_favorite(key))
        };
        let target = !favorited;
        Button::new("hero-favorite")
            .ghost()
            .icon(
                icons::icon(icons::HEART)
                    .when(favorited, |heart| heart.text_color(cx.theme().danger)),
            )
            .on_click(move |_, _, cx| {
                let keys = keys.clone();
                FavoritesModel::global(cx).update(cx, |favorites, cx| {
                    for key in keys.into_iter().filter(|key| !key.is_empty()) {
                        if favorites.is_favorite(&key) != target {
                            favorites.set(key, target, cx);
                        }
                    }
                });
            })
            .into_any_element()
    }

    fn resize_handle(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("hero-resize")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_ns_resize()
            .bg(cx.theme().overlay)
            .hover(|handle| handle.bg(cx.theme().primary))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    let height = ConfigModel::global(cx).read(cx).config().hero_height;
                    this.resize = Some((event.position.y, height));
                }),
            )
            .on_drag(HeroResize, |_, _, _, cx| cx.new(|_| HeroResize))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<HeroResize>, _, cx| {
                    this.resize_hero(event.event.position.y, cx);
                }),
            )
            .child(
                div()
                    .w(px(40.))
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(cx.theme().foreground),
            )
            .into_any_element()
    }

    fn resize_hero(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let Some((start_y, start_height)) = self.resize else {
            return;
        };
        let delta = f32::from(y - start_y);
        let height = (start_height as f32 + delta).clamp(MIN_HEIGHT, MAX_HEIGHT) as u32;
        ConfigModel::global(cx).update(cx, |config, cx| {
            config.update(cx, |app| app.hero_height = height)
        });
    }
}
