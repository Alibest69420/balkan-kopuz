//! "Add to playlist": pick one of the source's playlists, or type a name to make one.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    px,
};
use store::{Notices, PlaylistsModel};

use crate::format::t;
use crate::icons;
use crate::style;

struct PlaylistPicker {
    keys: Vec<String>,
    name: Entity<InputState>,
}

impl PlaylistPicker {
    fn add(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.keys.clone();
        PlaylistsModel::global(cx).update(cx, |playlists, cx| playlists.add_tracks(id, keys, cx));
        window.close_dialog(cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        if name.trim().is_empty() {
            Notices::error(t("playlist_name_required"), cx);
            return;
        }
        let keys = self.keys.clone();
        PlaylistsModel::global(cx).update(cx, |playlists, cx| playlists.create(name, keys, cx));
        window.close_dialog(cx);
    }
}

impl Render for PlaylistPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let playlists = PlaylistsModel::global(cx).read(cx).playlists().to_vec();
        v_flex()
            .gap_4()
            .w(px(448.))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.name)))
                    .child(
                        Button::new("create-playlist")
                            .primary()
                            .icon(crate::icons::icon(icons::PLUS))
                            .label(t("create"))
                            .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
                    ),
            )
            .child(
                v_flex()
                    .id("playlist-list")
                    .gap_2()
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .children(playlists.into_iter().map(|playlist| {
                        let id = playlist.id.clone();
                        div()
                            .id(SharedString::from(format!("playlist-{}", playlist.id)))
                            .p_3()
                            .text_sm()
                            .rounded(style::radius_lg())
                            .bg(style::white(style::RAISED, cx))
                            .hover(|row| row.bg(style::white(style::HOVER, cx)))
                            .child(playlist.name.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add(id.clone(), window, cx)
                            }))
                    })),
            )
    }
}

/// Open the picker for `keys`.
pub fn open(keys: Vec<String>, window: &mut Window, cx: &mut App) {
    let picker = cx.new(|cx| PlaylistPicker {
        keys,
        name: cx.new(|cx| InputState::new(window, cx).placeholder(t("new_playlist_name"))),
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title(t("add_to_playlist")).child(picker.clone())
    });
}
