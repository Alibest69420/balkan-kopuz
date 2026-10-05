//! The cover, the title, the artist and the line that says where the queue came from.

use components::format::t;
use components::{Cover, Link, Tone, focus, icons, menus, motion, style};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    px,
};
use store::{
    Chrome, FavoritesModel, PlaylistsModel, QueueModel, Route, SourcesModel, Target, actions,
};

use crate::NowPlaying;

const MARK_GLYPH: Pixels = px(16.);
const MARQUEE_PERIOD: std::time::Duration = std::time::Duration::from_millis(6_000);

/// What `text_sm` is in rems, which is the size the title is drawn at and so the size it is measured at.
const TITLE_REMS: f32 = 0.875;

/// What the title measures as drawn, so overflow is a fact rather than a guess at a character count.
fn title_width(title: &SharedString, window: &Window, cx: &App) -> Pixels {
    let size = gpui_kit::rems(TITLE_REMS).to_pixels(window.rem_size());
    let run = gpui_kit::TextRun {
        len: title.len(),
        font: gpui_kit::Font {
            weight: FontWeight::BOLD,
            ..gpui_kit::font(cx.theme().font_family.clone())
        },
        color: cx.theme().foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(title.clone(), size, &[run], None)
        .width
}

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
        Route::Artist(artist) => QueueModel::global(cx)
            .read(cx)
            .current()
            .and_then(|track| {
                track
                    .credits
                    .iter()
                    .find(|credit| credit.key.as_ref() == Some(artist))
            })
            .map(|credit| SharedString::from(credit.name.clone()))
            .unwrap_or_default(),
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
        let artist_route = track.as_ref().and_then(Route::artist_of);
        let size = cover_size(cx);
        let muted = style::slate_400(cx);
        let origin = playing_from(cx);
        let album = track
            .as_ref()
            .map(|track| SharedString::from(track.album_id.clone()))
            .filter(|album| !album.is_empty());
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
                            .rounded(style::thumb_radius()),
                    ),
            )
            .child(
                v_flex()
                    // Sized by its text, not by the room going spare: the marks belong beside it.
                    .min_w_0()
                    .justify_center()
                    .gap_0p5()
                    .child(self.marquee_title(title.clone(), album, window, cx))
                    .child(
                        Link::new("bar-artist")
                            .label(artist)
                            .route(artist_route)
                            .tone(Tone::Muted)
                            .underline(true)
                            .text_xs(),
                    )
                    .children(origin.map(|(label, route)| playing_from_link(label, route))),
            )
            .children(self.marks(key, window, cx))
            .children(
                crate::bar::menu_fits(window)
                    .then_some(())
                    .and(target.clone())
                    .map(|target| {
                        let menu = self.control_glyph("track-meta-menu", muted, false, window, cx);
                        self.framed(
                            "track-meta-menu",
                            mark("track-meta-menu", icons::MORE, menu, cx)
                                .dropdown_menu(menus::builder(target)),
                            window,
                            cx,
                        )
                    }),
            );

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
        album: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let natural = title_width(&title, window, cx);
        // How far past its column the title runs, which is exactly how far it has to walk.
        let overflow = (natural - self.title_room).max(px(0.));
        let long = overflow > px(0.);
        let (over, pointer) = motion::hovered("bar-title-hover", window, cx);
        let running = long && over;
        let shown = title.clone();

        div()
            .id("bar-title-frame")
            .min_w_0()
            .overflow_hidden()
            .on_prepaint({
                let owner = cx.entity().downgrade();
                move |bounds, _, cx| {
                    owner
                        .update(cx, |this, cx| this.set_title_room(bounds.size.width, cx))
                        .ok();
                }
            })
            .child(
                Link::new("bar-title")
                    .route(album.map(Route::Album))
                    .underline(true)
                    .hover_state(pointer)
                    .tooltip(long.then_some(title))
                    .min_w_0()
                    .overflow_hidden()
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
                                            move |label, progress| {
                                                // There and back over exactly what does not fit, so the
                                                // end of the title is reached and never passed.
                                                let travel = 1. - (1. - 2. * progress).abs();
                                                label.ml(-overflow * travel)
                                            },
                                        )
                                        .into_any_element()
                                } else {
                                    label.truncate().child(shown.clone()).into_any_element()
                                }
                            }),
                    ),
            )
    }

    /// Favourite, don't-recommend: the two marks the preferences let a bar keep.
    fn marks(
        &self,
        key: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let wanted = store::layout(cx).player.clone();
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let favorite = key
            .as_deref()
            .is_some_and(|key| FavoritesModel::global(cx).read(cx).is_favorite(key));
        let muted = style::slate_400(cx);
        let heart = self.control_glyph(
            "meta-favorite",
            if favorite { cx.theme().danger } else { muted },
            favorite,
            window,
            cx,
        );
        let dislike = self.control_glyph(
            "meta-dont-recommend",
            if favorite { muted.opacity(0.4) } else { muted },
            favorite,
            window,
            cx,
        );

        let mut marks = Vec::new();
        if wanted.favorite {
            let button = mark("meta-favorite", icons::HEART, heart, cx)
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
                });
            marks.push(
                self.framed("meta-favorite", button, window, cx)
                    .into_any_element(),
            );
        }
        if capabilities.dont_recommend {
            let button = mark("meta-dont-recommend", icons::THUMBS_DOWN, dislike, cx)
                .tooltip(t("dont_recommend"))
                // Like and dislike are one setting on the remote, so both at once would undo the heart.
                .disabled(key.is_none() || favorite)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(key) = key.clone() {
                        this.dont_recommend(key, cx);
                    }
                }));
            marks.push(
                self.framed("meta-dont-recommend", button, window, cx)
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let key = self.track(cx).map(|track| track.key);
        let tint = self.control_glyph("meta-share", style::slate_400(cx), false, window, cx);
        let button = focus::quiet(Button::new("meta-share"), cx)
            .ghost()
            .with_size(glyph * (4. / 3.))
            .size(size)
            .rounded(size / 2.)
            .icon(icons::icon(icons::SHARE))
            .custom(style::bare_button(tint, cx))
            // It copies whatever page the source itself publishes, which is rarely MusicBrainz.
            .tooltip(t("share"))
            .disabled(key.is_none())
            .on_click(move |_, _, cx| {
                if let Some(key) = key.clone() {
                    actions::share_track(key, cx);
                }
            });
        self.framed("meta-share", button, window, cx)
    }
}

/// "Playing from <album>", which is the only thing on the bar that says how you got here.
fn playing_from_link(label: SharedString, route: Route) -> impl IntoElement + use<> {
    Link::new("playing-from")
        .route(Some(route))
        .tone(Tone::Faint)
        .flex()
        .items_center()
        .gap_1()
        .min_w_0()
        .text_size(style::text_10())
        .child(div().flex_shrink_0().child(t("playing_from")))
        .child(div().truncate().child(label))
}

fn mark(id: &'static str, glyph: &'static str, color: gpui_kit::Hsla, cx: &App) -> Button {
    let button = Button::new(id)
        .ghost()
        .with_size(MARK_GLYPH * (4. / 3.))
        .size(style::chrome().round_button)
        .rounded(style::chrome().round_button / 2.)
        .icon(icons::icon(glyph))
        .custom(style::bare_button(color, cx));
    focus::quiet(button, cx)
}

/// The one thing a `Target` is needed for here, kept out of the render so it allocates once.
pub(crate) fn track_target(track: &api::TrackInfo) -> Target {
    Target::track(track.key.clone(), track.title.clone())
}
