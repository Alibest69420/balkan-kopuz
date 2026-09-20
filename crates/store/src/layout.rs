//! Presentation preferences: what the app looks like, kept beside the app rather than in the daemon.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::{App, Context, Entity, Task};
use serde::{Deserialize, Serialize};

// Dragging a width or nudging a slider settles before the file is rewritten.
const SAVE_DELAY: Duration = Duration::from_millis(600);

/// How big the UI may be scaled, either way of 100%.
pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.8..=1.4;

/// Where the navigation lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavPosition {
    #[default]
    Top,
    Left,
}

/// How much of the app follows the current cover's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    #[default]
    On,
    SheetOnly,
    Off,
}

/// How much things move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    Full,
    Reduced,
    Off,
}

impl Motion {
    /// Whether a tween should run at all; `Reduced` keeps state changes but drops the travel.
    pub fn animates(self) -> bool {
        self == Motion::Full
    }

    /// The factor a duration is multiplied by, so one switch reaches every tween.
    pub fn scale(self) -> f32 {
        match self {
            Motion::Full => 1.,
            Motion::Reduced => 0.4,
            Motion::Off => 0.,
        }
    }
}

/// Which side of the content the now-playing sheet's panel sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Right,
    Left,
}

/// What is behind the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backdrop {
    #[default]
    Artwork,
    Palette,
    Solid,
}

/// The sheet's opening tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetTab {
    #[default]
    Queue,
    Lyrics,
    Related,
}

/// How a listing is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    #[default]
    Grid,
    Table,
}

/// Row height in a table, and how much padding a tile gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
}

impl Density {
    pub fn row_height(self) -> f32 {
        match self {
            Density::Compact => 32.,
            Density::Comfortable => 56.,
        }
    }
}

/// How wide a tile is in a band grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TileSize {
    pub fn width(self) -> f32 {
        match self {
            TileSize::Small => 140.,
            TileSize::Medium => 180.,
            TileSize::Large => 240.,
        }
    }
}

/// One column of a library table, in the order it is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
}

impl Column {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            visible: true,
            width: None,
        }
    }
}

/// What a listing is ordered by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sort {
    pub field: String,
    #[serde(default)]
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            field: "title".into(),
            descending: false,
        }
    }
}

/// Everything one library facet remembers about how it was left.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Facet {
    pub mode: ViewMode,
    pub density: Density,
    pub tile: TileSize,
    pub sort: Sort,
    pub group_by: Option<String>,
    pub columns: Vec<Column>,
}

/// The navigation bar: where it sits, what it holds, and where the app opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Nav {
    pub position: NavPosition,
    /// Route names in the order they are drawn; unknown names are ignored, missing ones appended.
    pub order: Vec<String>,
    pub hidden: Vec<String>,
    pub start: String,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            position: NavPosition::Top,
            order: DESTINATIONS.iter().map(|name| name.to_string()).collect(),
            hidden: DESTINATIONS
                .iter()
                .filter(|name| !PRIMARY.contains(*name))
                .map(|name| name.to_string())
                .collect(),
            start: "home".into(),
        }
    }
}

/// The destinations the bar shows until someone asks for more; the rest start hidden.
pub const PRIMARY: &[&str] = &["home", "library", "discover", "radio"];

/// The destinations a nav bar can hold, in their designed order.
pub const DESTINATIONS: &[&str] = &[
    "home",
    "library",
    "discover",
    "radio",
    "playlists",
    "favorites",
    "downloader",
    "activity",
];

impl Nav {
    /// The visible destinations, in order: the saved order first, then anything new, minus hidden.
    pub fn visible(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .order
            .iter()
            .map(String::as_str)
            .filter(|name| DESTINATIONS.contains(name))
            .collect();
        for name in DESTINATIONS {
            if !names.contains(name) {
                names.push(name);
            }
        }
        names.retain(|name| !self.hidden.iter().any(|hidden| hidden == name));
        names
    }
}

/// The optional playlists column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    pub shown: bool,
    pub width: f32,
    pub pinned: Vec<String>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            shown: false,
            width: 260.,
            pinned: Vec::new(),
        }
    }
}

/// Which of the player bar's optional controls are drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Player {
    pub shuffle: bool,
    pub repeat: bool,
    pub volume: bool,
    pub lyrics: bool,
    pub queue: bool,
    pub favorite: bool,
    pub cover: TileSize,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            shuffle: true,
            repeat: true,
            volume: true,
            lyrics: true,
            queue: true,
            favorite: true,
            cover: TileSize::Medium,
        }
    }
}

/// The expanded now-playing surface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Sheet {
    pub tab: SheetTab,
    pub side: Side,
    pub backdrop: Backdrop,
}

/// The library's per-facet state, keyed by facet name.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, transparent)]
pub struct Library(pub BTreeMap<String, Facet>);

impl Library {
    /// How `facet` was left, or how it is drawn by default.
    pub fn facet(&self, name: &str) -> Facet {
        self.0.get(name).cloned().unwrap_or_default()
    }
}

/// Which home shelves are drawn, and in what order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Home {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

impl Default for Home {
    fn default() -> Self {
        Self {
            order: SHELVES.iter().map(|name| name.to_string()).collect(),
            hidden: Vec::new(),
        }
    }
}

/// The home shelves, in their designed order.
pub const SHELVES: &[&str] = &[
    "jump_back_in",
    "recently_added",
    "made_for_you",
    "top_artists",
    "your_playlists",
    "recently_played",
];

impl Home {
    pub fn visible(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .order
            .iter()
            .map(String::as_str)
            .filter(|name| SHELVES.contains(name))
            .collect();
        for name in SHELVES {
            if !names.contains(name) {
                names.push(name);
            }
        }
        names.retain(|name| !self.hidden.iter().any(|hidden| hidden == name));
        names
    }
}

/// What the command palette remembers, and how much of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    pub history: bool,
    pub recents: usize,
    pub current_track_first: bool,
    pub recent_commands: Vec<String>,
    pub recent_searches: Vec<String>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            history: true,
            recents: 5,
            current_track_first: true,
            recent_commands: Vec::new(),
            recent_searches: Vec::new(),
        }
    }
}

impl Palette {
    /// Put `entry` at the front of `list`, deduped, keeping at most `keep` of them.
    pub fn remember(list: &mut Vec<String>, entry: String, keep: usize) {
        if entry.is_empty() || keep == 0 {
            return;
        }
        list.retain(|held| held != &entry);
        list.insert(0, entry);
        list.truncate(keep);
    }
}

/// Type size and face, over whatever the theme asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Interface {
    pub scale: f32,
    pub font: Option<String>,
}

impl Default for Interface {
    fn default() -> Self {
        Self {
            scale: 1.,
            font: None,
        }
    }
}

/// What interrupts, and what only lands in the notification centre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub errors: bool,
    /// Per job kind; a kind with no entry toasts.
    pub jobs: BTreeMap<String, bool>,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            errors: true,
            jobs: BTreeMap::new(),
        }
    }
}

impl Notifications {
    pub fn toasts(&self, kind: &str) -> bool {
        self.jobs.get(kind).copied().unwrap_or(true)
    }
}

/// Where the window was, so it opens there again.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub maximized: bool,
}

/// Every presentation preference, as it sits in `layout.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub nav: Nav,
    pub panel: Panel,
    pub player: Player,
    pub sheet: Sheet,
    pub library: Library,
    /// The encoded facet and filter set the library was last left on, so a restart opens it again.
    pub library_view: Option<String>,
    pub home: Home,
    pub interface: Interface,
    pub palette: Palette,
    pub notifications: Notifications,
    pub window: WindowState,
    pub accent: Accent,
    pub motion: Motion,
    pub skin: String,
    /// Command id to keystroke, overriding what the registry binds by default.
    pub shortcuts: BTreeMap<String, String>,
    /// The first-run guide was skipped; cleared again once a source holds something.
    pub first_run_dismissed: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            nav: Nav::default(),
            panel: Panel::default(),
            player: Player::default(),
            sheet: Sheet::default(),
            library: Library::default(),
            library_view: None,
            home: Home::default(),
            interface: Interface::default(),
            palette: Palette::default(),
            notifications: Notifications::default(),
            window: WindowState::default(),
            accent: Accent::default(),
            motion: Motion::default(),
            skin: "normal".into(),
            shortcuts: BTreeMap::new(),
            first_run_dismissed: false,
        }
    }
}

/// A group of preferences that resets together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Nav,
    Panel,
    Player,
    Sheet,
    Library,
    Home,
    Interface,
    Palette,
    Notifications,
    Shortcuts,
    Skin,
    All,
}

impl Section {
    fn reset(self, layout: &mut Layout) {
        let fresh = Layout::default();
        match self {
            Section::Nav => layout.nav = fresh.nav,
            Section::Panel => layout.panel = fresh.panel,
            Section::Player => layout.player = fresh.player,
            Section::Sheet => layout.sheet = fresh.sheet,
            Section::Library => {
                layout.library = fresh.library;
                layout.library_view = fresh.library_view;
            }
            Section::Home => layout.home = fresh.home,
            Section::Interface => {
                layout.interface = fresh.interface;
                layout.accent = fresh.accent;
                layout.motion = fresh.motion;
            }
            Section::Palette => layout.palette = fresh.palette,
            Section::Notifications => layout.notifications = fresh.notifications,
            Section::Shortcuts => layout.shortcuts = fresh.shortcuts,
            Section::Skin => layout.skin = fresh.skin,
            // The window is where the person left it, not a preference; resetting all keeps it.
            Section::All => {
                let window = layout.window;
                *layout = fresh;
                layout.window = window;
            }
        }
    }
}

fn yes() -> bool {
    true
}

/// Where `layout.toml` lives; `KOPUZ_GPUI_LAYOUT` moves it, so a smoke run cannot clobber it.
pub fn layout_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KOPUZ_GPUI_LAYOUT") {
        return Some(PathBuf::from(path));
    }
    let base = directories::BaseDirs::new()?;
    Some(base.config_dir().join("kopuz-gpui").join("layout.toml"))
}

/// The app's own look, loaded at startup and written back debounced.
pub struct LayoutModel {
    layout: Layout,
    path: Option<PathBuf>,
    _save: Option<Task<()>>,
}

impl LayoutModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(_: &mut Context<Self>) -> Self {
        let path = layout_path();
        let layout = path.as_deref().map(read).unwrap_or_default();
        Self {
            layout,
            path,
            _save: None,
        }
    }

    pub fn get(&self) -> &Layout {
        &self.layout
    }

    /// Change a preference; the file catches up once edits settle.
    pub fn update(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Layout)) {
        let before = self.layout.clone();
        edit(&mut self.layout);
        if self.layout == before {
            return;
        }
        cx.notify();
        self.schedule_save(cx);
    }

    /// Put one group of preferences back the way it shipped.
    pub fn reset(&mut self, section: Section, cx: &mut Context<Self>) {
        self.update(cx, |layout| section.reset(layout));
    }

    /// Change a preference from anywhere holding an `App`.
    pub fn edit(cx: &mut App, edit: impl FnOnce(&mut Layout)) {
        Self::global(cx).update(cx, |this, cx| this.update(cx, edit));
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        self._save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(layout) = this.read_with(cx, |this, _| this.layout.clone()) else {
                return;
            };
            cx.background_executor()
                .spawn(async move { write(&path, &layout) })
                .await;
            this.update(cx, |this, _| this._save = None).ok();
        }));
    }
}

/// A missing or unreadable file is the default look, not a failure worth stopping for.
fn read(path: &std::path::Path) -> Layout {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Layout::default();
    };
    match toml::from_str(&text) {
        Ok(layout) => layout,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "layout file ignored");
            Layout::default()
        }
    }
}

fn write(path: &std::path::Path, layout: &Layout) {
    let text = match toml::to_string_pretty(layout) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(%error, "could not serialise the layout");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(%error, path = %parent.display(), "could not make the config directory");
        return;
    }
    // Through a neighbour, so a crash mid-write cannot leave half a file behind.
    let temporary = path.with_extension("toml.new");
    if let Err(error) = std::fs::write(&temporary, text) {
        tracing::warn!(%error, path = %temporary.display(), "could not write the layout");
        return;
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        tracing::warn!(%error, path = %path.display(), "could not replace the layout");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_layout_round_trips() {
        let layout = Layout::default();
        let text = toml::to_string_pretty(&layout).expect("serialise");
        let back: Layout = toml::from_str(&text).expect("deserialise");
        assert_eq!(layout, back);
    }

    #[test]
    fn an_empty_file_is_the_default_look() {
        let layout: Layout = toml::from_str("").expect("deserialise");
        assert_eq!(layout, Layout::default());
    }

    #[test]
    fn a_key_from_a_later_version_is_ignored() {
        let layout: Layout =
            toml::from_str("skin = \"neon\"\nwhat_is_this = 3\n").expect("deserialise");
        assert_eq!(layout.skin, "neon");
        assert_eq!(layout.nav, Nav::default());
    }

    #[test]
    fn a_partial_section_keeps_its_other_defaults() {
        let layout: Layout = toml::from_str("[panel]\nshown = true\n").expect("deserialise");
        assert!(layout.panel.shown);
        assert_eq!(layout.panel.width, Panel::default().width);
    }

    #[test]
    fn hiding_a_destination_drops_it_from_the_bar() {
        let mut nav = Nav::default();
        nav.hidden.push("radio".into());
        assert!(!nav.visible().contains(&"radio"));
        assert!(nav.visible().contains(&"home"));
    }

    #[test]
    fn a_destination_added_later_shows_up_at_the_end() {
        let nav = Nav {
            order: vec!["home".into(), "library".into()],
            hidden: Vec::new(),
            ..Nav::default()
        };
        let visible = nav.visible();
        assert_eq!(&visible[..2], &["home", "library"]);
        assert_eq!(visible.len(), DESTINATIONS.len());
    }

    #[test]
    fn the_bar_ships_holding_the_four_destinations() {
        assert_eq!(Nav::default().visible(), PRIMARY);
    }

    #[test]
    fn showing_a_destination_puts_it_back_in_its_designed_place() {
        let mut nav = Nav::default();
        nav.hidden.retain(|name| name != "playlists");
        assert_eq!(
            nav.visible(),
            &["home", "library", "discover", "radio", "playlists"]
        );
    }

    #[test]
    fn reordering_the_bar_survives_a_round_trip() {
        let mut nav = Nav::default();
        nav.order.swap(0, 1);
        let text = toml::to_string_pretty(&nav).expect("serialise");
        let back: Nav = toml::from_str(&text).expect("deserialise");
        assert_eq!(back.visible(), &["library", "home", "discover", "radio"]);
    }

    #[test]
    fn an_unknown_destination_name_is_dropped() {
        let nav = Nav {
            order: vec!["gone".into(), "home".into()],
            ..Nav::default()
        };
        assert!(!nav.visible().contains(&"gone"));
    }

    #[test]
    fn resetting_a_section_leaves_the_others_alone() {
        let mut layout = Layout::default();
        layout.panel.shown = true;
        layout.skin = "neon".into();
        Section::Panel.reset(&mut layout);
        assert!(!layout.panel.shown);
        assert_eq!(layout.skin, "neon");
    }

    #[test]
    fn resetting_everything_keeps_where_the_window_was() {
        let mut layout = Layout::default();
        layout.window.width = Some(1200.);
        layout.skin = "neon".into();
        Section::All.reset(&mut layout);
        assert_eq!(layout.window.width, Some(1200.));
        assert_eq!(layout.skin, "normal");
    }

    #[test]
    fn a_remembered_entry_moves_to_the_front_and_the_list_stays_capped() {
        let mut list = vec!["b".to_string(), "c".to_string()];
        Palette::remember(&mut list, "b".into(), 2);
        assert_eq!(list, ["b", "c"]);
        Palette::remember(&mut list, "a".into(), 2);
        assert_eq!(list, ["a", "b"]);
    }

    #[test]
    fn a_job_kind_nobody_configured_still_toasts() {
        let notifications = Notifications::default();
        assert!(notifications.toasts("scan"));
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let dir = std::env::temp_dir().join(format!("kopuz-layout-{}", std::process::id()));
        let path = dir.join("nested").join("layout.toml");
        let mut layout = Layout::default();
        layout.panel.shown = true;
        layout.interface.scale = 1.25;
        layout.nav.position = NavPosition::Left;
        layout
            .shortcuts
            .insert("play.toggle".into(), "ctrl-space".into());
        write(&path, &layout);
        assert_eq!(read(&path), layout);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_there_is_the_default_look() {
        let path = std::env::temp_dir().join("kopuz-layout-that-does-not-exist.toml");
        std::fs::remove_file(&path).ok();
        assert_eq!(read(&path), Layout::default());
    }
}
