//! The playlist and folder cards, and the menus they carry.

use api::{PlaylistFolderInfo, PlaylistInfo};
use components::format::t_with;
use components::{Frame, Tile, icons};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::{App, Entity, Hsla, IntoElement, Pixels, Styled as _, px};
use store::{Route, Target};

/// The `w-full h-32` picture inside a card's `p-4`.
const COVER: Pixels = px(128.);

/// The tween reaches the variant too, since a Button's own hover overrides the instance style.
pub(crate) fn tweened(
    id: &'static str,
    button: Button,
    hover: Entity<bool>,
    fill: Hsla,
    text: Hsla,
    edge: Hsla,
    cx: &App,
) -> impl IntoElement {
    let button = button
        .custom(
            ButtonCustomVariant::new(cx)
                .color(edge)
                .hover(fill)
                .active(fill)
                .foreground(text),
        )
        .bg(fill)
        .border_color(edge)
        .text_color(text);
    components::hover::framed(id, hover, button)
}

/// The card for one playlist; whether it sits in a folder is the catalog's answer, not a flag.
pub(crate) fn playlist_card(playlist: &PlaylistInfo) -> Tile {
    Tile::new(playlist.id.clone(), playlist.name.clone())
        .subtitle(t_with(
            "playlist_track_count",
            &[("count", playlist.track_keys.len().to_string())],
        ))
        .artwork(playlist.artwork.as_ref())
        .placeholder(icons::LIST)
        .size(COVER)
        .frame(Frame::Card)
        .route(Route::Playlist(playlist.id.clone().into()))
        .menu(Some(Target::Playlist {
            id: playlist.id.clone().into(),
            name: playlist.name.clone().into(),
        }))
}

/// The card for one folder; its cover is the first playlist's.
pub(crate) fn folder_card(folder: &PlaylistFolderInfo, playlists: &[PlaylistInfo]) -> Tile {
    let cover = folder
        .playlist_ids
        .first()
        .and_then(|id| playlists.iter().find(|playlist| &playlist.id == id))
        .and_then(|playlist| playlist.artwork.as_ref());
    // TODO(i18n): the Dioxus card spells this count out in English too.
    Tile::new(folder.id.clone(), folder.name.clone())
        .subtitle(format!("{} playlists", folder.playlist_ids.len()))
        .artwork(cover)
        .placeholder(icons::FOLDER)
        .size(COVER)
        .frame(Frame::Card)
        .route(Route::PlaylistFolder(folder.id.clone().into()))
        .menu(Some(Target::Folder {
            id: folder.id.clone().into(),
            name: folder.name.clone().into(),
        }))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, InputEvent as _, MouseMoveEvent, ParentElement as _, Render,
        TestAppContext, Window, div, point,
    };

    struct ActionFixture {
        hover: Entity<bool>,
        clicks: Rc<Cell<usize>>,
        loading: bool,
    }

    impl Render for ActionFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.clicks.clone();
            div().size(px(200.)).child(tweened(
                "playlist-action",
                Button::new("playlist-action")
                    .label("Play")
                    .tooltip("Play playlist")
                    .loading(self.loading)
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                self.hover.clone(),
                gpui_kit::black(),
                gpui_kit::white(),
                gpui_kit::black(),
                cx,
            ))
        }
    }

    #[gpui_kit::test]
    fn playlist_button_tooltips_preserve_hover_and_clicks(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for loading in [false, true] {
            let hover = cx.new(|_| false);
            let clicks = Rc::new(Cell::new(0));
            let view = cx.add_window({
                let hover = hover.clone();
                let clicks = clicks.clone();
                move |window, cx| {
                    let fixture = cx.new(|_| ActionFixture {
                        hover,
                        clicks,
                        loading,
                    });
                    Root::new(fixture, window, cx)
                }
            });
            cx.update_window(view.into(), |_, window, cx| {
                window.render_frame(cx);
                window.hover("playlist-action", cx);
                assert!(*hover.read(cx));
                window.click("playlist-action", cx);
                assert_eq!(clicks.get(), usize::from(!loading));
                window.dispatch_event(
                    MouseMoveEvent {
                        position: point(px(180.), px(180.)),
                        pressed_button: None,
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
                assert!(!*hover.read(cx));
            })
            .unwrap();
        }
    }
}
