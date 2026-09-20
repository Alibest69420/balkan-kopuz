use gpui_kit::{App, Context, Entity, SharedString};

/// Everywhere the app can be. Detail params travel in the route, so back navigation restores them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Route {
    Home,
    Search,
    Genre(SharedString),
    Discover,
    DiscoverPlaylist {
        id: SharedString,
        title: SharedString,
    },
    DiscoverArtist {
        id: SharedString,
        name: SharedString,
    },
    Library,
    Albums,
    Album(SharedString),
    Artists,
    /// `channel` routes to the remote profile when the source presents artists remotely.
    Artist {
        name: SharedString,
        channel: Option<SharedString>,
    },
    Playlists,
    PlaylistFolder(SharedString),
    Playlist(SharedString),
    Favorites,
    Activity,
    Radio,
    Downloader,
    Settings {
        section: Option<SharedString>,
    },
}

impl Route {
    /// The sidebar item this route lights up.
    pub fn root(&self) -> Route {
        match self {
            Route::Genre(_) => Route::Search,
            Route::Album(_) => Route::Albums,
            Route::Artist { .. } => Route::Artists,
            Route::Playlist(_) | Route::PlaylistFolder(_) => Route::Playlists,
            Route::DiscoverPlaylist { .. } | Route::DiscoverArtist { .. } => Route::Discover,
            Route::Settings { .. } => Route::Settings { section: None },
            other => other.clone(),
        }
    }

    /// Stable key for caching a page view and its scroll position.
    pub fn key(&self) -> String {
        format!("{self:?}")
    }

    /// Where a route that is no longer a place now lands, so an old link never reaches a 404.
    pub fn redirect(self) -> Route {
        match self {
            Route::Favorites => Route::Library,
            Route::Activity | Route::Downloader => Route::Home,
            other => other,
        }
    }

    /// What a redirected route was asking for, for the view that has to honour it instead.
    pub fn intent(&self) -> Option<&'static str> {
        match self {
            Route::Favorites => Some("favorites"),
            Route::Activity => Some("activity"),
            Route::Downloader => Some("downloads"),
            _ => None,
        }
    }

    /// The route a nav destination opens; the names are `layout::DESTINATIONS`.
    pub fn destination(name: &str) -> Option<Route> {
        Some(match name {
            "home" => Route::Home,
            "library" => Route::Library,
            "discover" => Route::Discover,
            "radio" => Route::Radio,
            "playlists" => Route::Playlists,
            "favorites" => Route::Favorites,
            "activity" => Route::Activity,
            "downloader" => Route::Downloader,
            _ => return None,
        })
    }

    /// The destination this route lights up, detail pages included.
    pub fn destination_of(&self) -> Option<&'static str> {
        Some(match self {
            Route::Home => "home",
            Route::Library
            | Route::Albums
            | Route::Album(_)
            | Route::Artists
            | Route::Favorites => "library",
            // A remote artist is reached through Discover, a local one through the library.
            Route::Artist { channel, .. } => match channel {
                Some(_) => "discover",
                None => "library",
            },
            Route::Discover
            | Route::DiscoverPlaylist { .. }
            | Route::DiscoverArtist { .. }
            | Route::Search
            | Route::Genre(_) => "discover",
            Route::Radio => "radio",
            Route::Playlists | Route::PlaylistFolder(_) | Route::Playlist(_) => "playlists",
            Route::Activity => "activity",
            Route::Downloader => "downloader",
            Route::Settings { .. } => return None,
        })
    }

    /// The parameterless routes, under the names `KOPUZ_ROUTE` accepts.
    pub fn from_name(name: &str) -> Option<Route> {
        Some(match name {
            "home" => Route::Home,
            "search" => Route::Search,
            "discover" => Route::Discover,
            "library" => Route::Library,
            "albums" => Route::Albums,
            "artists" => Route::Artists,
            "playlists" => Route::Playlists,
            "favorites" => Route::Favorites,
            "activity" => Route::Activity,
            "radio" => Route::Radio,
            "downloader" => Route::Downloader,
            "settings" => Route::Settings { section: None },
            // `settings/appearance` opens straight onto one category.
            other => match other.strip_prefix("settings/") {
                Some(section) => Route::Settings {
                    section: Some(SharedString::from(section.to_string())),
                },
                None => return None,
            },
        })
    }
}

/// Where the app is, and how it got there.
pub struct Navigator {
    current: Route,
    back: Vec<Route>,
    forward: Vec<Route>,
    intent: Option<&'static str>,
}

impl Navigator {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        // KOPUZ_ROUTE opens straight onto a page, so a smoke run can render each one.
        let asked = std::env::var("KOPUZ_ROUTE")
            .ok()
            .and_then(|name| Route::from_name(&name))
            .or_else(|| Route::destination(&crate::layout(cx).nav.start))
            .unwrap_or(Route::Home);
        Self {
            current: asked.clone().redirect(),
            back: Vec::new(),
            forward: Vec::new(),
            intent: asked.intent(),
        }
    }

    pub fn current(&self) -> &Route {
        &self.current
    }

    /// What the route that redirected here was after, for the page that has to stand in for it.
    pub fn intent(&self) -> Option<&'static str> {
        self.intent
    }

    /// Take the intent, so asking for the same one again is a fresh request, not a no-op.
    pub fn take_intent(&mut self) -> Option<&'static str> {
        self.intent.take()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        let intent = route.intent();
        let route = route.redirect();
        if route == self.current {
            // A redirected route still says what it wanted, even when it lands where it already is.
            if self.intent != intent {
                self.intent = intent;
                crate::notify(cx);
            }
            return;
        }
        self.intent = intent;
        self.back.push(std::mem::replace(&mut self.current, route));
        self.forward.clear();
        crate::notify(cx);
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.back.pop() {
            self.forward
                .push(std::mem::replace(&mut self.current, route));
            self.intent = None;
            crate::notify(cx);
        }
    }

    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.current, route));
            self.intent = None;
            crate::notify(cx);
        }
    }

    /// Navigate from anywhere holding an `App`.
    pub fn navigate(route: Route, cx: &mut App) {
        Self::global(cx).update(cx, |this, cx| this.go(route, cx));
    }

    /// What a back chevron does: pop history, or go to `fallback` if there is none to pop.
    ///
    /// Navigating to the parent instead would push onto the back stack, so pressing back would
    /// walk further forward — and from a page reached sideways it lands somewhere never visited.
    pub fn back_or(fallback: Route, cx: &mut App) {
        Self::global(cx).update(cx, |this, cx| match this.can_go_back() {
            true => this.go_back(cx),
            false => this.go(fallback, cx),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::DESTINATIONS;

    #[test]
    fn a_route_that_is_no_longer_a_place_lands_somewhere() {
        assert_eq!(Route::Favorites.redirect(), Route::Library);
        assert_eq!(Route::Activity.redirect(), Route::Home);
        assert_eq!(Route::Downloader.redirect(), Route::Home);
    }

    #[test]
    fn a_route_that_is_still_a_place_is_left_alone() {
        for route in [Route::Home, Route::Library, Route::Discover, Route::Radio] {
            assert_eq!(route.clone().redirect(), route);
        }
    }

    #[test]
    fn a_redirect_says_what_it_was_after() {
        assert_eq!(Route::Favorites.intent(), Some("favorites"));
        assert_eq!(Route::Library.intent(), None);
    }

    #[test]
    fn the_smoke_route_names_still_resolve() {
        for name in ["home", "library", "favorites", "activity", "downloader"] {
            let route = Route::from_name(name).expect("a name the smoke script uses");
            assert!(matches!(
                route.redirect(),
                Route::Home | Route::Library | Route::Discover | Route::Radio
            ));
        }
    }

    #[test]
    fn every_destination_opens_a_route() {
        for name in DESTINATIONS {
            assert!(Route::destination(name).is_some(), "{name} opens nothing");
        }
    }

    #[test]
    fn a_destination_lights_up_from_its_own_route() {
        // Favorites is the exception: it is a filter on the library, so it lights the library.
        for name in DESTINATIONS.iter().filter(|name| **name != "favorites") {
            let route = Route::destination(name).expect("a destination route");
            assert_eq!(route.destination_of(), Some(*name));
        }
        assert_eq!(Route::Favorites.destination_of(), Some("library"));
    }

    #[test]
    fn a_detail_page_lights_the_destination_it_came_from() {
        assert_eq!(Route::Album("x".into()).destination_of(), Some("library"));
        assert_eq!(
            Route::Artist {
                name: "x".into(),
                channel: None
            }
            .destination_of(),
            Some("library")
        );
        assert_eq!(
            Route::Artist {
                name: "x".into(),
                channel: Some("UC".into())
            }
            .destination_of(),
            Some("discover")
        );
        assert_eq!(Route::Search.destination_of(), Some("discover"));
        assert_eq!(
            Route::Playlist("x".into()).destination_of(),
            Some("playlists")
        );
    }

    #[test]
    fn settings_is_not_a_destination() {
        assert_eq!(Route::Settings { section: None }.destination_of(), None);
    }

    #[test]
    fn a_redirected_route_never_becomes_the_destination_it_lights() {
        // Favorites lights Library and lands on it, so the bar and the page agree.
        assert_eq!(
            Route::Favorites.clone().redirect().destination_of(),
            Route::Favorites.destination_of()
        );
    }
}
