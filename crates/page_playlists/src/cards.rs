//! The playlist and folder cards, and the menus they carry.

use std::time::Duration;

use api::{ArtworkRef, PlaylistFolderInfo, PlaylistInfo};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use store::{Navigator, Route, Target};
use widgets::format::t_with;
use widgets::{Cover, icons, menus, motion, style};

/// The card, and the `w-full h-32` picture inside its `p-4`.
const CARD: Pixels = px(160.);
const COVER: Pixels = px(128.);
/// `group-hover:scale-105 duration-500` on a card's cover.
const COVER_ZOOM: f32 = 1.05;
const COVER_ZOOM_TIME: Duration = Duration::from_millis(500);

/// The `gap-6` wrap every playlist grid lays its cards out in.
pub(crate) fn card_grid() -> Div {
    div().flex().flex_wrap().gap_6()
}

/// Hover as a value a tween can read; `.hover()` refines a style instead of reporting the state.
pub(crate) fn hovered(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> (bool, Entity<bool>) {
    let state = window.use_keyed_state(id, cx, |_, _| false);
    let hovered = *state.read(cx);
    (hovered, state)
}

/// A `Button` paints its own hover style over the instance one, so the variant is handed the tween too.
pub(crate) fn tweened(button: Button, fill: Hsla, text: Hsla, edge: Hsla, cx: &App) -> Button {
    button
        .custom(
            ButtonCustomVariant::new(cx)
                .color(edge)
                .hover(fill)
                .active(fill)
                .foreground(text),
        )
        .bg(fill)
        .border_color(edge)
        .text_color(text)
}

/// The `on_hover` listener that keeps a [`hovered`] flag in step with the pointer.
pub(crate) fn track(state: Entity<bool>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, _, cx| {
        let hovered = *hovered;
        state.update(cx, |flag, cx| {
            if *flag != hovered {
                *flag = hovered;
                cx.notify();
            }
        });
    }
}

/// A cover card with a title, a subtitle, a menu beside the title and an optional button over the cover.
#[derive(IntoElement)]
pub(crate) struct Card {
    id: SharedString,
    title: SharedString,
    subtitle: SharedString,
    artwork: Option<ArtworkRef>,
    placeholder: &'static str,
    route: Route,
    target: Option<Target>,
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
            target: None,
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

    /// What the dots button and the right-click both act on.
    pub(crate) fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// A button drawn over the cover, revealed on hover.
    pub(crate) fn corner(mut self, corner: Option<AnyElement>) -> Self {
        self.corner = corner;
        self
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let group = SharedString::from(format!("card-{}", self.id));
        let cover_route = self.route.clone();
        let title_route = self.route;
        let (hot, hover) = hovered(
            SharedString::from(format!("card-hover-{}", self.id)),
            window,
            cx,
        );
        let fill = motion::color(
            SharedString::from(format!("card-fill-{}", self.id)),
            style::white(if hot { style::HOVER } else { style::RAISED }, cx),
            window,
            cx,
        );
        let cover = motion::length(
            SharedString::from(format!("card-cover-{}", self.id)),
            if hot { COVER * COVER_ZOOM } else { COVER },
            COVER_ZOOM_TIME,
            window,
            cx,
        );
        let target = self.target.clone();
        let card = v_flex()
            .id(SharedString::from(format!("card-{}", self.id)))
            .group(group.clone())
            .w(CARD)
            .gap_4()
            .p_4()
            .rounded(style::radius_xl())
            .bg(fill)
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .cursor_pointer()
            .on_hover(track(hover))
            .child(
                // The corner button is a sibling of the clickable cover, so pressing it is not a navigation.
                div()
                    .relative()
                    .child(
                        // `group-hover:scale-105` reads as the cover growing inside a frame that clips it.
                        div()
                            .id(SharedString::from(format!("cover-{}", self.id)))
                            .size(COVER)
                            .rounded(px(12.))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(move |_, _, cx| Navigator::navigate(cover_route.clone(), cx))
                            .child(
                                Cover::new(self.artwork.as_ref())
                                    .size(cover)
                                    .rounded(px(12.))
                                    .placeholder(self.placeholder),
                            ),
                    )
                    .when_some(self.corner, |cover, corner| {
                        cover.child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .invisible()
                                .group_hover(group.clone(), |corner| corner.visible())
                                .child(corner),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        v_flex()
                            .id(SharedString::from(format!("title-{}", self.id)))
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .on_click(move |_, _, cx| Navigator::navigate(title_route.clone(), cx))
                            .child(
                                div()
                                    .truncate()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(style::slate_400(cx))
                                    .child(self.subtitle),
                            ),
                    )
                    .children(self.target.clone().map(|target| {
                        // A hidden element takes its own open menu with it, so the dots only dim.
                        div()
                            .flex_shrink_0()
                            .opacity(widgets::focus::RESTING)
                            .group_hover(group, |menu| menu.opacity(1.))
                            .child(
                                Button::new(SharedString::from(format!("card-menu-{}", self.id)))
                                    .ghost()
                                    .xsmall()
                                    .icon(icons::icon(icons::MORE))
                                    .dropdown_menu(menus::builder(target)),
                            )
                    })),
            );
        // Right-click gets the same menu the dots do, on a card that has one.
        match target {
            Some(target) => card.context_menu(menus::builder(target)).into_any_element(),
            None => card.into_any_element(),
        }
    }
}

/// The card for one playlist; whether it sits in a folder is the catalog's answer, not a flag.
pub(crate) fn playlist_card(playlist: &PlaylistInfo) -> Card {
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
    .target(Target::Playlist {
        id: playlist.id.clone().into(),
        name: playlist.name.clone().into(),
    })
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
    .target(Target::Folder {
        id: folder.id.clone().into(),
        name: folder.name.clone().into(),
    })
}
