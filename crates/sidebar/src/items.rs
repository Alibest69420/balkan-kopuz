use store::Route;
use widgets::icons;

/// The Normal-style rail, in the order `cfg.sidebar_order` defaults to.
pub(crate) const MENU: &[&str] = &[
    "home",
    "search",
    "discover",
    "library",
    "albums",
    "artists",
    "playlists",
    "favorites",
    "radio",
    "activity",
    "downloader",
];

pub(crate) const SETTINGS: &str = "settings";

pub(crate) fn route(key: &str) -> Option<Route> {
    Some(match key {
        "home" => Route::Home,
        "search" => Route::Search,
        "discover" => Route::Discover,
        "library" => Route::Library,
        "albums" => Route::Albums,
        "artists" => Route::Artists,
        "playlists" => Route::Playlists,
        "favorites" => Route::Favorites,
        "radio" => Route::Radio,
        "activity" => Route::Activity,
        "downloader" => Route::Downloader,
        SETTINGS => Route::Settings { section: None },
        _ => return None,
    })
}

pub(crate) fn icon(key: &str) -> &'static str {
    match key {
        "home" => icons::HOME,
        "search" => icons::SEARCH,
        "discover" => icons::COMPASS,
        "library" => icons::LIBRARY,
        "albums" => icons::DISC,
        "artists" => icons::MIC,
        "playlists" => icons::LIST,
        "favorites" => icons::HEART,
        "radio" => icons::RADIO,
        "activity" => icons::ACTIVITY,
        "downloader" => icons::DOWNLOAD,
        SETTINGS => icons::SETTINGS,
        _ => icons::MUSIC,
    }
}

/// `sidebar_order` first, then whatever it has not heard of yet — a new item must still appear.
pub(crate) fn ordered(order: &[String], discover: bool) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = order
        .iter()
        .filter_map(|key| MENU.iter().copied().find(|item| *item == key.as_str()))
        .collect();
    for item in MENU {
        if !keys.contains(item) {
            keys.push(item);
        }
    }
    keys.retain(|key| *key != "discover" || discover);
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlisted_items_land_after_the_configured_ones() {
        let order = vec!["radio".to_string(), "home".to_string()];
        let keys = ordered(&order, true);
        assert_eq!(&keys[..2], &["radio", "home"]);
        assert!(keys.contains(&"downloader"));
    }

    #[test]
    fn discover_is_dropped_without_the_capability() {
        assert!(!ordered(&[], false).contains(&"discover"));
        assert!(ordered(&[], true).contains(&"discover"));
    }

    #[test]
    fn every_menu_key_has_a_route_and_an_icon() {
        for key in MENU.iter().chain(std::iter::once(&SETTINGS)) {
            assert!(route(key).is_some(), "{key} has no route");
            assert_ne!(icon(key), icons::MUSIC, "{key} has no icon");
        }
    }
}
