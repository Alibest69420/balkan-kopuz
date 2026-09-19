//! The playlist and folder cards, and the menus they carry.

use api::{ArtworkRef, PlaylistCapability, PlaylistFolderInfo, PlaylistInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use store::{Navigator, Route, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{Cover, icons};

const COVER: Pixels = px(160.);

/// A cover card with a title, a subtitle, a menu beside the title and an optional button over the cover.
#[derive(IntoElement)]
pub(crate) struct Card {
    id: SharedString,
    title: SharedString,
    subtitle: SharedString,
    artwork: Option<ArtworkRef>,
    placeholder: &'static str,
    route: Route,
    menu: Option<AnyElement>,
    corner: Option<AnyElement>,
}

impl Card {
    pub(crate) fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        subtitle: impl Into<SharedString>,
        route: Route,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            artwork: None,
            placeholder: icons::LIST,
            route,
            menu: None,
            corner: None,
        }
    }

    pub(crate) fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub(crate) fn artwork(mut self, artwork: Option<&ArtworkRef>) -> Self {
        self.artwork = artwork.cloned();
        self
    }

    pub(crate) fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }

    pub(crate) fn menu(mut self, menu: Option<AnyElement>) -> Self {
        self.menu = menu;
        self
    }

    /// A button drawn over the cover, revealed on hover.
    pub(crate) fn corner(mut self, corner: Option<AnyElement>) -> Self {
        self.corner = corner;
        self
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let group = SharedString::from(format!("card-{}", self.id));
        let cover_route = self.route.clone();
        let title_route = self.route;
        v_flex()
            .id(SharedString::from(format!("card-{}", self.id)))
            .group(group.clone())
            .w(COVER)
            .gap_2()
            .p_2()
            .rounded(cx.theme().radius)
            .hover(|card| card.bg(cx.theme().list_hover))
            .child(
                // The corner button is a sibling of the clickable cover, so pressing it is not a navigation.
                div()
                    .relative()
                    .child(
                        div()
                            .id(SharedString::from(format!("cover-{}", self.id)))
                            .on_click(move |_, _, cx| Navigator::navigate(cover_route.clone(), cx))
                            .child(
                                Cover::new(self.artwork.as_ref())
                                    .size(COVER)
                                    .rounded(cx.theme().radius)
                                    .placeholder(self.placeholder),
                            ),
                    )
                    .when_some(self.corner, |cover, corner| {
                        cover.child(
                            div()
                                .absolute()
                                .top_1()
                                .right_1()
                                .opacity(0.)
                                .group_hover(group, |style| style.opacity(1.))
                                .child(corner),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_start()
                    .child(
                        v_flex()
                            .id(SharedString::from(format!("title-{}", self.id)))
                            .flex_1()
                            .min_w_0()
                            .on_click(move |_, _, cx| Navigator::navigate(title_route.clone(), cx))
                            .child(div().truncate().text_sm().child(self.title))
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.subtitle),
                            ),
                    )
                    .children(self.menu),
            )
    }
}

/// The card for one playlist; `folder` is the folder it is being shown inside, if any.
pub(crate) fn playlist_card(playlist: &PlaylistInfo, folder: Option<&str>) -> Card {
    Card::new(
        playlist.id.clone(),
        playlist.name.clone(),
        t_with(
            "playlist_track_count",
            &[("count", playlist.track_keys.len().to_string())],
        ),
        Route::Playlist(playlist.id.clone().into()),
    )
    .artwork(playlist.artwork.as_ref())
    .menu(Some(playlist_menu(playlist, folder.is_some())))
}

/// The card for one folder; its cover is the first playlist's.
pub(crate) fn folder_card(folder: &PlaylistFolderInfo, playlists: &[PlaylistInfo]) -> Card {
    let cover = folder
        .playlist_ids
        .first()
        .and_then(|id| playlists.iter().find(|playlist| &playlist.id == id))
        .and_then(|playlist| playlist.artwork.as_ref());
    // TODO(i18n): the Dioxus card spells this count out in English too.
    Card::new(
        folder.id.clone(),
        folder.name.clone(),
        format!("{} playlists", folder.playlist_ids.len()),
        Route::PlaylistFolder(folder.id.clone().into()),
    )
    .artwork(cover)
    .placeholder(icons::FOLDER)
    .menu(Some(folder_menu(folder)))
}

fn playlist_menu(playlist: &PlaylistInfo, in_folder: bool) -> AnyElement {
    let id = playlist.id.clone();
    let name = playlist.name.clone();
    Button::new(SharedString::from(format!("playlist-menu-{id}")))
        .ghost()
        .xsmall()
        .icon(icons::icon(icons::MORE))
        .dropdown_menu(move |menu, _, cx| {
            let capabilities = SourcesModel::global(cx).read(cx).capabilities();
            let mut menu = menu;
            if capabilities.folders {
                menu = menu.item(
                    PopupMenuItem::new(t("move_to_folder"))
                        .icon(icons::icon(icons::FOLDER))
                        .on_click({
                            let id = id.clone();
                            move |_, window, cx| {
                                crate::dialogs::open_folder_picker(id.clone(), window, cx)
                            }
                        }),
                );
            }
            if in_folder {
                menu = menu.item(
                    PopupMenuItem::new(t("remove_from_folder"))
                        .icon(icons::icon(icons::CLOSE))
                        .on_click({
                            let id = id.clone();
                            move |_, _, cx| {
                                let id = id.clone();
                                actions::run(cx, move |api| async move {
                                    api.move_playlist(id, None).await
                                });
                            }
                        }),
                );
            }
            let mutable = capabilities.playlists != PlaylistCapability::None;
            if mutable {
                menu = menu.item(
                    PopupMenuItem::new(t("rename_playlist"))
                        .icon(icons::icon(icons::EDIT))
                        .on_click({
                            let id = id.clone();
                            let name = name.clone();
                            move |_, window, cx| {
                                let id = id.clone();
                                crate::dialogs::open_rename(
                                    t("rename_playlist"),
                                    name.clone(),
                                    move |name, cx| {
                                        let id = id.clone();
                                        actions::run(cx, move |api| async move {
                                            api.rename_playlist(id, name).await
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                );
            }
            if capabilities.playlist_radio {
                menu = menu.item(
                    PopupMenuItem::new(t("start_radio"))
                        .icon(icons::icon(icons::RADIO))
                        .on_click({
                            let id = id.clone();
                            move |_, _, cx| actions::start_playlist_radio(id.clone(), cx)
                        }),
                );
            }
            if mutable {
                menu = menu.separator().item(
                    PopupMenuItem::new(t("delete_playlist"))
                        .icon(icons::icon(icons::TRASH))
                        .on_click({
                            let id = id.clone();
                            let name = name.clone();
                            move |_, window, cx| {
                                let id = id.clone();
                                crate::dialogs::confirm_delete(
                                    t("delete_playlist"),
                                    name.clone(),
                                    move |cx| {
                                        let id = id.clone();
                                        actions::run(cx, move |api| async move {
                                            api.delete_playlist(id).await
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                );
            }
            menu
        })
        .into_any_element()
}

fn folder_menu(folder: &PlaylistFolderInfo) -> AnyElement {
    let id = folder.id.clone();
    let name = folder.name.clone();
    Button::new(SharedString::from(format!("folder-menu-{id}")))
        .ghost()
        .xsmall()
        .icon(icons::icon(icons::MORE))
        .dropdown_menu(move |menu, _, _| {
            menu.item(
                PopupMenuItem::new(t("rename_folder"))
                    .icon(icons::icon(icons::EDIT))
                    .on_click({
                        let id = id.clone();
                        let name = name.clone();
                        move |_, window, cx| {
                            let id = id.clone();
                            crate::dialogs::open_rename(
                                t("rename_folder"),
                                name.clone(),
                                move |name, cx| {
                                    let id = id.clone();
                                    actions::run(cx, move |api| async move {
                                        api.rename_playlist_folder(id, name).await
                                    });
                                },
                                window,
                                cx,
                            );
                        }
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new(t("delete_folder"))
                    .icon(icons::icon(icons::TRASH))
                    .on_click({
                        let id = id.clone();
                        let name = name.clone();
                        move |_, window, cx| {
                            let id = id.clone();
                            crate::dialogs::confirm_delete(
                                t("delete_folder"),
                                name.clone(),
                                move |cx| {
                                    let id = id.clone();
                                    actions::run(cx, move |api| async move {
                                        api.delete_playlist_folder(id).await
                                    });
                                },
                                window,
                                cx,
                            );
                        }
                    }),
            )
        })
        .into_any_element()
}
