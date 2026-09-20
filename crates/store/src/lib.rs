//! App-wide mirrors of daemon state, each a GPUI entity reachable through `Model::global(cx)`.

pub mod actions;
mod artwork;
mod chrome;
mod config;
mod downloads;
mod favorites;
mod invalidation;
mod jobs;
mod nav;
mod notices;
mod player;
mod playlists;
mod query;
mod queue;
mod sources;

use gpui_kit::{App, AppContext as _, Entity, Global};

pub use artwork::{ArtworkBytes, ArtworkKey, Residents, artwork_source};
pub use chrome::{Chrome, SidePanel};
pub use config::ConfigModel;
pub use downloads::DownloadsModel;
pub use favorites::FavoritesModel;
pub use invalidation::{Invalidation, TablesChanged};
pub use jobs::{JobEvent, JobsModel};
pub use link::{Link, api};
pub use nav::{Navigator, Route};
pub use notices::{Notice, Notices};
pub use player::{PlayerModel, Playhead};
pub use playlists::PlaylistsModel;
pub use query::Query;
pub use queue::QueueModel;
pub use sources::SourcesModel;

/// Every row of a listing, the way the Dioxus frontend asks for "all".
pub fn all() -> api::Page {
    api::Page {
        offset: 0,
        limit: u32::MAX,
    }
}

/// What to put in front of a person when a call fails.
///
/// `ApiError`'s `Display` is `"{code:?}: {message}"`, so every toast in the app used to open with
/// a Rust enum variant — "Unavailable: transport error: connection refused". The daemon's own
/// sentence is the useful half; the code is for the log.
pub fn human(error: &api::ApiError) -> gpui_kit::SharedString {
    match error.code {
        // The one failure the message cannot explain, because there is nobody to write it.
        api::ErrorCode::DaemonGone => gpui_kit::SharedString::from(i18n::t("daemon_unreachable")),
        // A code with no message is the daemon's bug, not something to translate around.
        _ if error.message.trim().is_empty() => {
            gpui_kit::SharedString::from(format!("{:?}", error.code))
        }
        _ => gpui_kit::SharedString::from(error.message.clone()),
    }
}

struct Handle<T>(Entity<T>);

impl<T: 'static> Global for Handle<T> {}

fn handle<T: 'static>(cx: &App) -> Entity<T> {
    cx.global::<Handle<T>>().0.clone()
}

fn register<T: 'static>(cx: &mut App, build: impl FnOnce(&mut gpui_kit::Context<T>) -> T) {
    let entity = cx.new(build);
    cx.set_global(Handle(entity));
}

/// Create every model; call once after `link::init`, before opening a window.
pub fn init(cx: &mut App) {
    register(cx, Notices::new);
    register(cx, Invalidation::new);
    register(cx, ConfigModel::new);
    register(cx, SourcesModel::new);
    register(cx, PlayerModel::new);
    register(cx, Playhead::new);
    register(cx, QueueModel::new);
    register(cx, FavoritesModel::new);
    register(cx, PlaylistsModel::new);
    register(cx, JobsModel::new);
    register(cx, DownloadsModel::new);
    register(cx, Navigator::new);
    register(cx, Chrome::new);
}
