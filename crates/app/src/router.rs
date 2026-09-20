//! Route to page view. Views are cached per route key, so going back restores a page as it was.

use std::collections::HashMap;

use gpui_kit::{AnyView, AppContext as _, Context, Window};
use store::Route;

use crate::shell::Shell;

#[derive(Default)]
pub struct Router {
    pages: HashMap<String, AnyView>,
    // Least recently visited first, so the cache can drop one page instead of all of them.
    order: Vec<String>,
}

// Enough for a back-and-forth between a grid and a few details before anything is rebuilt.
const CACHED_PAGES: usize = 8;

impl Router {
    /// The view for `route`, built on first visit.
    pub fn page(&mut self, route: &Route, window: &mut Window, cx: &mut Context<Shell>) -> AnyView {
        let key = route.key();
        if let Some(page) = self.pages.get(&key) {
            let page = page.clone();
            self.touch(key);
            return page;
        }
        // Evict the oldest, not the lot. A route key carries its parameters, so nine album visits
        // was enough to flush everything: going back then rebuilt the grid from scratch, scroll
        // position and all, and only ever on the ninth visit.
        while self.pages.len() >= CACHED_PAGES && !self.order.is_empty() {
            let oldest = self.order.remove(0);
            self.pages.remove(&oldest);
        }
        let page = build(route, window, cx);
        self.pages.insert(key.clone(), page.clone());
        self.touch(key);
        page
    }

    fn touch(&mut self, key: String) {
        self.order.retain(|held| held != &key);
        self.order.push(key);
    }

    /// Drop cached pages, for a source switch that makes every page's contents wrong.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.order.clear();
    }
}

fn build(route: &Route, window: &mut Window, cx: &mut Context<Shell>) -> AnyView {
    match route.clone() {
        Route::Home | Route::Downloader => cx.new(|cx| page_home::Home::new(window, cx)).into(),
        // One browsing destination: the landing, genres and results are the same entity.
        Route::Search | Route::Discover => cx.new(|cx| discover::Discover::new(window, cx)).into(),
        Route::Genre(name) => cx.new(|cx| discover::Genre::new(name, window, cx)).into(),
        Route::DiscoverPlaylist { id, title } => cx
            .new(|cx| discover::CatalogPlaylist::new(id, title, window, cx))
            .into(),
        Route::DiscoverArtist { id, name } => cx
            .new(|cx| discover::CatalogArtist::new(id, name, window, cx))
            .into(),
        // Both library routes collapse to one cache key, so a chip does not rebuild the page.
        // The four old listings redirect here, and only reach this arm if one ever slips through.
        Route::Library
        | Route::LibraryView(_)
        | Route::Albums
        | Route::Artists
        | Route::Playlists
        | Route::Favorites
        | Route::Activity => cx.new(|cx| library::Library::new(window, cx)).into(),
        Route::Album(id) => cx
            .new(|cx| page_albums::AlbumDetail::new(id, window, cx))
            .into(),
        Route::Artist { name, channel } => match channel {
            // A remote artist has a channel id and its own profile page.
            Some(channel) => cx
                .new(|cx| discover::CatalogArtist::new(channel, name, window, cx))
                .into(),
            None => cx
                .new(|cx| page_artists::ArtistDetail::new(name, window, cx))
                .into(),
        },
        Route::PlaylistFolder(id) => cx
            .new(|cx| page_playlists::PlaylistFolder::new(id, window, cx))
            .into(),
        Route::Playlist(id) => cx
            .new(|cx| page_playlists::PlaylistDetail::new(id, window, cx))
            .into(),
        Route::Radio => cx.new(|cx| page_radio::Radio::new(window, cx)).into(),
        Route::Settings { section } => cx
            .new(|cx| page_settings::Settings::new(section, window, cx))
            .into(),
    }
}
