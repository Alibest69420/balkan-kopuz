//! The bar's track block: cover, text and marks, wired to the player; the paint is in `components`.

use components::{Cover, MetaLayout, NowPlayingMeta, TrackMarks, menus, style};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rems,
};
use store::{
    Chrome, FavoritesModel, PlayerModel, PlaylistsModel, QueueModel, Route, SourcesModel, Target,
    actions,
};

use crate::NowPlaying;

const MARK_GLYPH: Pixels = px(16.);

/// The bar's cover, at whatever size the preference asks for, drawn at the bar's `scale`.
pub(crate) fn cover_size(scale: f32, cx: &App) -> Pixels {
    let size = match store::layout(cx).player.cover {
        store::layout::TileSize::Small => px(40.),
        store::layout::TileSize::Medium => px(56.),
        store::layout::TileSize::Large => px(72.),
    };
    size * scale
}

/// Where the daemon says the queue came from, and what to call it.
pub(crate) fn playing_from(cx: &App) -> Option<(SharedString, Route)> {
    let route = PlayerModel::global(cx)
        .read(cx)
        .queue_context()
        .and_then(Route::playing_from)?;
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
    /// The block on the left of the bar: cover, title, artist, where it all came from, and the marks.
    pub(crate) fn track_meta(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let track = self.track(cx);
        let target = track.as_ref().map(track_target);
        let scale = self.scale(cx);
        let size = cover_size(scale, cx);
        let origin = playing_from(cx);
        let marks = self
            .marks(
                "meta",
                crate::bar::menu_fits(window, scale),
                track.as_ref(),
                cx,
            )
            .size(style::chrome().round_button * scale, MARK_GLYPH * scale)
            .gap(px(16.) * scale);
        // The heart is the preferences' to keep on the bar; the sheet always has it.
        let marks = match store::layout(cx).player.favorite {
            true => marks,
            false => marks.favorite(None),
        };

        let block = h_flex()
            .id("track-meta")
            .flex_1()
            .min_w_0()
            .gap(rems(scale))
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
            .children(track.as_ref().map(|track| {
                NowPlayingMeta::new("bar", track)
                    .layout(MetaLayout::Bar)
                    .scale(scale)
                    .origin(origin)
            }))
            .child(marks);

        match target {
            Some(target) => block
                .context_menu(menus::builder(target))
                .into_any_element(),
            None => block.into_any_element(),
        }
    }

    /// The marks the preferences and the source allow, acting on `track`; the caller sets their size.
    pub(crate) fn marks(
        &self,
        id: &'static str,
        menu: bool,
        track: Option<&api::TrackInfo>,
        cx: &App,
    ) -> TrackMarks {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let key = track.map(|track| track.key.clone());
        let favorite = key
            .as_deref()
            .is_some_and(|key| FavoritesModel::global(cx).read(cx).is_favorite(key));
        let (heart, dislike) = (key.clone(), key);

        TrackMarks::new(id)
            .favorite(Some(favorite))
            .dont_recommend(capabilities.dont_recommend)
            .menu(track.map(track_target).filter(|_| menu))
            .disabled(track.is_none())
            .on_favorite(move |_, cx| {
                if let Some(key) = heart.clone() {
                    FavoritesModel::global(cx)
                        .update(cx, |favorites, cx| favorites.toggle(key, cx));
                }
            })
            .on_dont_recommend(move |_, cx| {
                if let Some(key) = dislike.clone() {
                    dont_recommend(key, cx);
                }
            })
    }

    /// Share, for the surfaces that keep it in reach.
    pub(crate) fn share_mark(&self, id: &'static str, cx: &App) -> TrackMarks {
        let key = self.track(cx).map(|track| track.key);
        TrackMarks::new(id)
            .share(true)
            .disabled(key.is_none())
            .on_share(move |_, cx| {
                if let Some(key) = key.clone() {
                    actions::share_track(key, cx);
                }
            })
    }
}

/// The signal is the source's, the skip is ours: it happens whether or not the daemon took it.
pub(crate) fn dont_recommend(key: String, cx: &mut App) {
    let call =
        store::Link::global(cx).call(move |api| async move { api.dont_recommend(key).await });
    cx.spawn(async move |cx| {
        if let Err(error) = call.await {
            cx.update(|cx| store::Notices::failed(&error, cx));
        }
        cx.update(|cx| store::PlayerModel::global(cx).update(cx, |player, cx| player.next(cx)));
    })
    .detach();
}

/// The one thing a `Target` is needed for here, kept out of the render so it allocates once.
pub(crate) fn track_target(track: &api::TrackInfo) -> Target {
    Target::track(track.key.clone(), track.title.clone())
}
