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
}

impl Navigator {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(_: &mut Context<Self>) -> Self {
        // KOPUZ_ROUTE opens straight onto a page, so a smoke run can render each one.
        let start = std::env::var("KOPUZ_ROUTE")
            .ok()
            .and_then(|name| Route::from_name(&name))
            .unwrap_or(Route::Home);
        Self {
            current: start,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    pub fn current(&self) -> &Route {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        if route == self.current {
            return;
        }
        self.back.push(std::mem::replace(&mut self.current, route));
        self.forward.clear();
        cx.notify();
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.back.pop() {
            self.forward
                .push(std::mem::replace(&mut self.current, route));
            cx.notify();
        }
    }

    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.current, route));
            cx.notify();
        }
    }

    /// Navigate from anywhere holding an `App`.
    pub fn navigate(route: Route, cx: &mut App) {
        Self::global(cx).update(cx, |this, cx| this.go(route, cx));
    }
}
