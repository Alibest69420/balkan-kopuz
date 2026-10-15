//! The person's own browser profiles, as opposed to the isolated ones kopuz
//! launches for a sign-in. Listing reads `Local State` and `profiles.ini`
//! only, so it never starts, signals or locks a running browser.

use std::path::{Path, PathBuf};

use config::{Browser, BrowserEngine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    pub browser: Browser,
    /// What [`super::read_cookies`] takes as its profile root.
    pub dir: PathBuf,
    pub name: String,
}

/// Every profile with a cookie store, browser by browser in
/// [`Browser::ALL`] order and each browser's own order within it.
pub fn browser_profiles() -> Vec<BrowserProfile> {
    let Some(dirs) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    Browser::ALL
        .iter()
        .flat_map(|&browser| {
            roots(browser, &dirs)
                .into_iter()
                .flat_map(move |root| match browser.engine() {
                    BrowserEngine::Chromium => chromium_profiles(browser, &root),
                    BrowserEngine::Gecko => gecko_profiles(browser, &root),
                })
        })
        .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "android")))]
/// Data directories holding a Chromium `Local State` or a Gecko
/// `profiles.ini`, relative to the platform's application data directory.
fn data_dirs(browser: Browser, mac: bool) -> &'static [&'static str] {
    match (browser, mac) {
        (Browser::Helium, _) => &["net.imput.helium"],
        (Browser::Chrome, false) => &["google-chrome"],
        (Browser::Chrome, true) => &["Google/Chrome"],
        (Browser::Chromium, false) => &["chromium"],
        (Browser::Chromium, true) => &["Chromium"],
        (Browser::Brave, _) => &["BraveSoftware/Brave-Browser"],
        (Browser::Vivaldi, false) => &["vivaldi"],
        (Browser::Vivaldi, true) => &["Vivaldi"],
        (Browser::Edge, false) => &["microsoft-edge"],
        (Browser::Edge, true) => &["Microsoft Edge"],
        (Browser::Firefox, false) => &["mozilla/firefox"],
        (Browser::Firefox, true) => &["Firefox"],
        (Browser::LibreWolf, false) => &["librewolf/librewolf"],
        (Browser::LibreWolf, true) => &["librewolf"],
        (Browser::Zen, _) => &["zen"],
        (Browser::Floorp, false) => &["floorp"],
        (Browser::Floorp, true) => &["Floorp"],
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
/// Gecko browsers still default to a dot directory in `$HOME` on Linux.
fn home_dirs(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Firefox => &[".mozilla/firefox"],
        Browser::LibreWolf => &[".librewolf"],
        Browser::Zen => &[".zen"],
        Browser::Floorp => &[".floorp"],
        _ => &[],
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn flatpak_id(browser: Browser) -> Option<&'static str> {
    match browser {
        Browser::Chrome => Some("com.google.Chrome"),
        Browser::Chromium => Some("org.chromium.Chromium"),
        Browser::Brave => Some("com.brave.Browser"),
        Browser::Vivaldi => Some("com.vivaldi.Vivaldi"),
        Browser::Edge => Some("com.microsoft.Edge"),
        Browser::Firefox => Some("org.mozilla.firefox"),
        Browser::LibreWolf => Some("io.gitlab.librewolf-community"),
        Browser::Zen => Some("app.zen_browser.zen"),
        Browser::Floorp => Some("one.ablaze.floorp"),
        Browser::Helium => None,
    }
}

/// Chromium keeps its `User Data` under `%LOCALAPPDATA%`, Gecko its
/// `profiles.ini` under `%APPDATA%`.
#[cfg(target_os = "windows")]
fn roots(browser: Browser, dirs: &directories::BaseDirs) -> Vec<PathBuf> {
    let (base, dir) = match browser {
        Browser::Helium => (dirs.data_local_dir(), r"imput\Helium\User Data"),
        Browser::Chrome => (dirs.data_local_dir(), r"Google\Chrome\User Data"),
        Browser::Chromium => (dirs.data_local_dir(), r"Chromium\User Data"),
        Browser::Brave => (
            dirs.data_local_dir(),
            r"BraveSoftware\Brave-Browser\User Data",
        ),
        Browser::Vivaldi => (dirs.data_local_dir(), r"Vivaldi\User Data"),
        Browser::Edge => (dirs.data_local_dir(), r"Microsoft\Edge\User Data"),
        Browser::Firefox => (dirs.data_dir(), r"Mozilla\Firefox"),
        Browser::LibreWolf => (dirs.data_dir(), "librewolf"),
        Browser::Zen => (dirs.data_dir(), "zen"),
        Browser::Floorp => (dirs.data_dir(), "Floorp"),
    };
    vec![base.join(dir)]
}

#[cfg(target_os = "macos")]
fn roots(browser: Browser, dirs: &directories::BaseDirs) -> Vec<PathBuf> {
    let support = dirs.home_dir().join("Library/Application Support");
    data_dirs(browser, true)
        .iter()
        .map(|d| support.join(d))
        .collect()
}

/// Android has no desktop browser profile to read.
#[cfg(target_os = "android")]
fn roots(_browser: Browser, _dirs: &directories::BaseDirs) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn roots(browser: Browser, dirs: &directories::BaseDirs) -> Vec<PathBuf> {
    let (home, config) = (dirs.home_dir(), dirs.config_dir());
    let mut roots: Vec<PathBuf> = data_dirs(browser, false)
        .iter()
        .map(|d| config.join(d))
        .collect();
    roots.extend(home_dirs(browser).iter().map(|d| home.join(d)));
    if let Some(id) = flatpak_id(browser) {
        let app = home.join(".var/app").join(id);
        roots.extend(
            data_dirs(browser, false)
                .iter()
                .map(|d| app.join("config").join(d)),
        );
        roots.extend(home_dirs(browser).iter().map(|d| app.join(d)));
    }
    roots
}

fn has_chromium_cookies(dir: &Path) -> bool {
    dir.join("Network/Cookies").is_file() || dir.join("Cookies").is_file()
}

/// The profiles `Local State` knows about that still have a cookie store, in
/// the browser's own order.
fn chromium_profiles(browser: Browser, root: &Path) -> Vec<BrowserProfile> {
    let Ok(text) = std::fs::read_to_string(root.join("Local State")) else {
        return Vec::new();
    };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(cache) = state["profile"]["info_cache"].as_object() else {
        return Vec::new();
    };
    let order: Vec<&str> = state["profile"]["profiles_order"]
        .as_array()
        .map(|o| o.iter().filter_map(|k| k.as_str()).collect())
        .unwrap_or_default();
    let mut keys: Vec<&String> = cache.keys().collect();
    keys.sort_by_key(|k| {
        (
            order.iter().position(|o| o == k).unwrap_or(usize::MAX),
            k.as_str() != "Default",
            k.as_str(),
        )
    });
    keys.into_iter()
        .filter(|key| has_chromium_cookies(&root.join(key)))
        .map(|key| BrowserProfile {
            browser,
            dir: root.join(key),
            name: cache[key.as_str()]["name"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(key)
                .to_string(),
        })
        .collect()
}

/// `[Profile*]` sections of `profiles.ini` whose directory has a cookie
/// store, the default one first.
fn gecko_profiles(browser: Browser, root: &Path) -> Vec<BrowserProfile> {
    #[derive(Default)]
    struct Section {
        name: Option<String>,
        path: Option<String>,
        relative: bool,
        default: bool,
    }
    let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut sections = Vec::new();
    let mut current: Option<Section> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            sections.extend(current.take());
            if line.starts_with("[Profile") {
                current = Some(Section {
                    relative: true,
                    ..Section::default()
                });
            }
            continue;
        }
        let (Some(section), Some((key, value))) = (current.as_mut(), line.split_once('=')) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => section.name = Some(value.to_string()),
            "Path" => section.path = Some(value.to_string()),
            "IsRelative" => section.relative = value == "1",
            "Default" => section.default = value == "1",
            _ => {}
        }
    }
    sections.extend(current);
    sections.sort_by_key(|s| !s.default);
    sections
        .into_iter()
        .filter_map(|s| {
            let (name, path) = (s.name?, s.path?);
            let dir = if s.relative {
                root.join(path)
            } else {
                PathBuf::from(path)
            };
            dir.join("cookies.sqlite")
                .is_file()
                .then_some(BrowserProfile { browser, dir, name })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn lists_chromium_profiles_from_local_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join("Local State"),
            r#"{"profile":{"profiles_order":["Profile 1","Default"],"info_cache":{
                "Default":{"name":"Personal"},
                "Profile 1":{"name":"Work"},
                "Profile 2":{"name":"Deleted"}}}}"#,
        );
        write(&root.join("Default/Network/Cookies"), "");
        write(&root.join("Profile 1/Cookies"), "");
        let names: Vec<String> = chromium_profiles(Browser::Chrome, root)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["Work", "Personal"]);
    }

    #[test]
    fn lists_gecko_profiles_default_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join("profiles.ini"),
            "[General]\nStartWithLastProfile=1\n\n\
             [Profile1]\nName=other\nIsRelative=1\nPath=Profiles/b.other\n\n\
             [Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/a.default\nDefault=1\n\n\
             [Profile2]\nName=empty\nIsRelative=1\nPath=Profiles/c.empty\n",
        );
        write(&root.join("Profiles/a.default/cookies.sqlite"), "");
        write(&root.join("Profiles/b.other/cookies.sqlite"), "");
        let profiles = gecko_profiles(Browser::Firefox, root);
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["default-release", "other"]);
        assert_eq!(profiles[0].dir, root.join("Profiles/a.default"));
    }
}
