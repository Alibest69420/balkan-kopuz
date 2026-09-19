//! The playlists inside one folder.

use gpui_kit::component::v_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window,
};
use store::{PlaylistsModel, Route};
use widgets::format::t;
use widgets::{EmptyState, Loading, PageHeader, icons, tile_grid};

use crate::cards::playlist_card;

pub struct PlaylistFolder {
    id: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl PlaylistFolder {
    pub fn new(id: SharedString, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&PlaylistsModel::global(cx), |_, _, cx| cx.notify())];
        Self {
            id,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for PlaylistFolder {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = PlaylistsModel::global(cx);
        let loaded = model.read(cx).is_loaded();
        let folder = model
            .read(cx)
            .folders()
            .iter()
            .find(|folder| folder.id == self.id.as_ref())
            .cloned();
        let playlists = model.read(cx).playlists().to_vec();
        let Some(folder) = folder else {
            return v_flex().size_full().p_8().child(if loaded {
                EmptyState::new(t("no_playlists_yet"))
                    .icon(icons::FOLDER)
                    .into_any_element()
            } else {
                Loading.into_any_element()
            });
        };
        let inside: Vec<_> = folder
            .playlist_ids
            .iter()
            .filter_map(|id| playlists.iter().find(|playlist| &playlist.id == id))
            .cloned()
            .collect();

        v_flex()
            .size_full()
            .p_8()
            .child(
                PageHeader::new(folder.name.clone())
                    .subtitle(t("playlists"))
                    .back(Route::Playlists),
            )
            .child(
                v_flex()
                    .id("folder-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(if inside.is_empty() {
                        EmptyState::new(t("no_playlists_yet"))
                            .icon(icons::LIST)
                            .into_any_element()
                    } else {
                        tile_grid()
                            .children(
                                inside
                                    .iter()
                                    .map(|playlist| playlist_card(playlist, Some(&folder.id))),
                            )
                            .into_any_element()
                    }),
            )
    }
}
