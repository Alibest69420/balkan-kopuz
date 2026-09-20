//! The cover, the title, the artist and the line that says where the queue came from.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    px, relative,
};
use store::{
    Chrome, FavoritesModel, Navigator, PlaylistsModel, QueueModel, Route, SourcesModel, Target,
    actions,
};
use widgets::format::t;
use widgets::{Cover, icons, menus, motion, style};

use crate::NowPlaying;

// `w-14` with a `rounded-md` corner, and the sizes the cover preference asks for instead.
const COVER_RADIUS: Pixels = px(6.);
const MARK_BUTTON: Pixels = px(36.);
const MARK_GLYPH: Pixels = px(16.);
// A title this long in a bar-width column is worth walking past the pointer.
const MARQUEE_MIN: usize = 28;
const MARQUEE_TRAVEL: f32 = 0.4;
const MARQUEE_PERIOD: std::time::Duration = std::time::Duration::from_millis(6_000);

/// The bar's cover, at whatever size the preference asks for.
pub(crate) fn cover_size(cx: &App) -> Pixels {
    match store::layout(cx).player.cover {
        store::layout::TileSize::Small => px(40.),
        store::layout::TileSize::Medium => px(56.),
        store::layout::TileSize::Large => px(72.),
    }
}

/// Where the queue came from, and what to call it.
pub(crate) fn playing_from(cx: &App) -> Option<(SharedString, Route)> {
    let route = Chrome::global(cx).read(cx).origin()?.clone();
    let label = match &route {
        Route::Album(id) => QueueModel::global(cx)
            .read(cx)
            .current()
            .map(|track| SharedString::from(track.album.clone()))
            .filter(|album| !album.is_empty())
            .unwrap_or_else(|| id.clone()),
        Route::Artist { name, .. } => name.clone(),
        Route::Genre(name) => name.clone(),
        Route::Playlist(id) => PlaylistsModel::global(cx)
            .read(cx)
            .playlist(id)
            .map(|playlist| SharedString::from(playlist.name.clone()))
            .unwrap_or_else(|| id.clone()),
        _ => return None,
    };
    (!label.is_empty()).then_some((label, route))
}

impl NowPlaying {
    /// The block on the left of the bar: cover, title, artist, and where it all came from.
    pub(crate) fn track_meta(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let track = self.track(cx);
        let key = track.as_ref().map(|track| track.key.clone());
        let title = SharedString::from(
            track
                .as_ref()
                .map(|track| track.title.clone())
                .unwrap_or_default(),
        );
        let artist = SharedString::from(
            track
                .as_ref()
                .map(|track| track.artist.clone())
                .unwrap_or_default(),
        );
        let size = cover_size(cx);
        let muted = style::slate_400(cx);
        let lifted = style::white(0.7, cx);
        let origin = playing_from(cx);
        let target = key
            .clone()
            .map(|key| store::Target::track(key, title.clone()));

        let block = h_flex()
            .id("track-meta")
            .flex_1()
            .min_w_0()
            .gap_4()
            .child(
                div()
                    .flex_shrink_0()
                    .id("bar-cover")
                    .on_prepaint(|bounds, _, cx| crate::remember_cover_slot(bounds, cx))
                    .cursor_pointer()
                    .on_click(|_, _, cx| {
                        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_sheet(true, cx))
                    })
                    .child(
                        Cover::new(track.as_ref().and_then(|track| track.artwork.as_ref()))
                            .size(size)
                            .rounded(COVER_RADIUS),
                    ),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .justify_center()
                    .gap_0p5()
                    .child(self.marquee_title(title.clone(), window, cx))
                    .child(
                        div()
                            .id("bar-artist")
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .when(!artist.is_empty(), |label| {
                                label
                                    .cursor_pointer()
                                    .hover(move |label| label.text_color(lifted))
                            })
                            .on_click({
                                let artist = artist.clone();
                                move |_, _, cx| {
                                    if !artist.is_empty() {
                                        Navigator::navigate(
                                            Route::Artist {
                                                name: artist.clone(),
                                                channel: None,
                                            },
                                            cx,
                                        );
                                    }
                                }
                            })
                            .child(artist),
                    )
                    .children(origin.map(|(label, route)| playing_from_link(label, route, cx))),
            )
            .children(self.marks(key, title, window, cx))
            .children(target.clone().map(|target| {
                Button::new("track-meta-menu")
                    .ghost()
                    .xsmall()
                    .icon(icons::icon(icons::MORE))
                    .dropdown_menu(menus::builder(target))
            }));

        match target {
            Some(target) => block
                .context_menu(menus::builder(target))
                .into_any_element(),
            None => block.into_any_element(),
        }
    }

    /// A title too long for its column walks past the pointer, and carries the whole of itself in a tooltip.
    fn marquee_title(
        &self,
        title: SharedString,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let long = title.chars().count() > MARQUEE_MIN;
        let running = long && self.hovered == Some("bar-title");
        let shown = title.clone();

        div()
            .id("bar-title-frame")
            .w_full()
            .overflow_hidden()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.set_hovered("bar-title", *hovered, cx)
            }))
            .when(long, |frame| {
                frame.tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
            })
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(style::white(style::TEXT_STRONG, cx))
                    .map(|label| {
                        if running {
                            label
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .child(shown.clone())
                                .with_animation(
                                    "bar-title-marquee",
                                    Animation::new(MARQUEE_PERIOD).repeat(),
                                    |label, progress| {
                                        let travel = 1. - (1. - 2. * progress).abs();
                                        label.ml(relative(-travel * MARQUEE_TRAVEL))
                                    },
                                )
                                .into_any_element()
                        } else {
                            label.truncate().child(shown.clone()).into_any_element()
                        }
                    }),
            )
    }

    /// Favourite, don't-recommend: the two marks the preferences let a bar keep.
    fn marks(
        &self,
        key: Option<String>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let _ = title;
        let wanted = store::layout(cx).player.clone();
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let favorite = key
            .as_deref()
            .is_some_and(|key| FavoritesModel::global(cx).read(cx).is_favorite(key));
        let muted = style::slate_400(cx);
        let heart = motion::color(
            "meta-heart",
            if favorite { cx.theme().danger } else { muted },
            window,
            cx,
        );
        let dislike = motion::color(
            "meta-dont-recommend",
            if favorite { muted.opacity(0.4) } else { muted },
            window,
            cx,
        );

        let mut marks = Vec::new();
        if wanted.favorite {
            marks.push(
                mark("meta-favorite", icons::HEART, heart, cx)
                    .tooltip(if favorite {
                        t("remove_from_favorites")
                    } else {
                        t("add_to_favorites")
                    })
                    .disabled(key.is_none())
                    .on_click({
                        let key = key.clone();
                        move |_, _, cx| {
                            if let Some(key) = key.clone() {
                                FavoritesModel::global(cx)
                                    .update(cx, |favorites, cx| favorites.toggle(key, cx));
                            }
                        }
                    })
                    .into_any_element(),
            );
        }
        if capabilities.dont_recommend {
            marks.push(
                mark("meta-dont-recommend", icons::THUMBS_DOWN, dislike, cx)
                    .tooltip(t("dont_recommend"))
                    // Like and dislike are one setting on the remote, so both at once would undo the heart.
                    .disabled(key.is_none() || favorite)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(key) = key.clone() {
                            this.dont_recommend(key, cx);
                        }
                    }))
                    .into_any_element(),
            );
        }
        marks
    }

    /// The signal is the source's, the skip is ours: it happens whether or not the daemon took it.
    pub(crate) fn dont_recommend(&mut self, key: String, cx: &mut Context<Self>) {
        let call =
            store::Link::global(cx).call(move |api| async move { api.dont_recommend(key).await });
        cx.spawn(async move |_, cx| {
            if let Err(error) = call.await {
                cx.update(|cx| store::Notices::error(store::human(&error), cx));
            }
            cx.update(|cx| store::PlayerModel::global(cx).update(cx, |player, cx| player.next(cx)));
        })
        .detach();
    }

    /// Share, for the surfaces that keep it in reach.
    pub(crate) fn share_button(
        &self,
        size: Pixels,
        glyph: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let key = self.track(cx).map(|track| track.key);
        Button::new("meta-share")
            .ghost()
            .with_size(glyph * (4. / 3.))
            .size(size)
            .rounded(size / 2.)
            .icon(icons::icon(icons::SHARE))
            .custom(style::bare_button(style::slate_400(cx), cx))
            .tooltip(t("share_musicbrainz"))
            .disabled(key.is_none())
            .on_click(move |_, _, cx| {
                if let Some(key) = key.clone() {
                    actions::share_track(key, cx);
                }
            })
    }
}

/// "Playing from <album>", which is the only thing on the bar that says how you got here.
fn playing_from_link(label: SharedString, route: Route, cx: &App) -> impl IntoElement + use<> {
    let muted = style::white(style::TEXT_MUTED, cx);
    let lifted = style::white(0.7, cx);
    h_flex()
        .id("playing-from")
        .gap_1()
        .min_w_0()
        .cursor_pointer()
        .text_size(style::text_10())
        .text_color(muted)
        .hover(move |line| line.text_color(lifted))
        .on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
        .child(div().flex_shrink_0().child(t("playing_from")))
        .child(div().truncate().child(label))
}

fn mark(id: &'static str, glyph: &'static str, color: gpui_kit::Hsla, cx: &App) -> Button {
    Button::new(id)
        .ghost()
        .with_size(MARK_GLYPH * (4. / 3.))
        .size(MARK_BUTTON)
        .rounded(MARK_BUTTON / 2.)
        .icon(icons::icon(glyph))
        .custom(style::bare_button(color, cx))
}

/// The one thing a `Target` is needed for here, kept out of the render so it allocates once.
pub(crate) fn track_target(track: &api::NowPlaying) -> Target {
    Target::track(track.key.clone(), track.title.clone())
}
