//! Albums, artists and playlists: one virtualized band grid, with a jump rail and type-ahead.

use api::{AlbumInfo, ArtistInfo, PlaylistInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, px, uniform_list,
};
use store::layout::ViewMode;
use store::{Navigator, PlaylistsModel, Route, Target};
use widgets::format::t_with;
use widgets::{Cover, icons, menus, motion, style};

use crate::facet::Facet;
use crate::{Library, Tile};

/// `gap-6` between tiles, so a column costs this much width.
const TILE_GAP: Pixels = px(24.);
/// Until the list has been laid out once there is no measured width to divide.
const COLUMNS_FALLBACK: usize = 4;
/// The jump rail's width, which the band grid does not get to use.
const RAIL: Pixels = px(20.);
/// Below this, every tile is on screen already and a rail would be decoration.
const RAIL_FROM: usize = 40;
/// The letters the rail offers, with everything else gathered under the first.
const LETTERS: &str = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";

impl Library {
    /// The tiles this facet draws, built once per change rather than once per frame.
    pub(crate) fn rebuild_tiles(&mut self, cx: &mut Context<Self>) {
        let mut tiles = match self.filters().facet {
            Facet::Albums => self.album_tiles(cx),
            Facet::Artists => self.artist_tiles(cx),
            Facet::Playlists => self.playlist_tiles(cx),
            Facet::Tracks => Vec::new(),
        };
        if self.whole {
            let descending = self.sort().descending;
            let field = self.sort().field.clone();
            tiles.sort_by(|left, right| {
                let order = match field.as_str() {
                    "tracks" | "year" => left.sort_rank.cmp(&right.sort_rank),
                    _ => left.sort_key.cmp(&right.sort_key),
                };
                match descending {
                    true => order.reverse(),
                    false => order,
                }
            });
        }
        self.tiles = tiles;
    }

    fn album_tiles(&self, cx: &Context<Self>) -> Vec<Tile> {
        let Some(page) = self.albums.read(cx).value() else {
            return Vec::new();
        };
        let query = self.filters().search.trim().to_lowercase();
        let genre = self.filters().genre.clone();
        let years = self.filters().years;
        page.albums
            .iter()
            .filter(|album| matches_album(album, &query, genre.as_deref(), years))
            .map(|album| self.album_tile(album))
            .collect()
    }

    fn album_tile(&self, album: &AlbumInfo) -> Tile {
        let playing = self
            .playing_title
            .as_ref()
            .is_some_and(|(_, title)| title == &album.title);
        Tile {
            id: SharedString::from(album.id.clone()),
            title: SharedString::from(album.title.clone()),
            subtitle: SharedString::from(album.artist.clone()),
            artwork: album.artwork.clone(),
            sort_key: album.title.to_lowercase(),
            sort_rank: album.year as u64,
            playing,
            target: Target::Album {
                id: album.id.clone().into(),
                title: album.title.clone().into(),
            },
            route: Route::Album(album.id.clone().into()),
        }
    }

    fn artist_tiles(&self, cx: &Context<Self>) -> Vec<Tile> {
        let Some(page) = self.artists.read(cx).value() else {
            return Vec::new();
        };
        let query = self.filters().search.trim().to_lowercase();
        page.artists
            .iter()
            .filter(|artist| query.is_empty() || artist.name.to_lowercase().contains(&query))
            .map(|artist| self.artist_tile(artist))
            .collect()
    }

    fn artist_tile(&self, artist: &ArtistInfo) -> Tile {
        let playing = self
            .playing_title
            .as_ref()
            .is_some_and(|(credited, _)| credited == &artist.name);
        Tile {
            // The id is the key, since a selection hands it back to `artist_tracks`.
            id: SharedString::from(artist.key.to_string()),
            title: SharedString::from(artist.name.clone()),
            subtitle: t_with(
                "library_artist_tracks",
                &[("count", artist.track_count.to_string())],
            ),
            artwork: artist.artwork.clone(),
            sort_key: artist.name.to_lowercase(),
            sort_rank: artist.track_count as u64,
            playing,
            target: Target::Artist {
                key: artist.key.clone(),
                name: artist.name.clone().into(),
            },
            route: Route::Artist(artist.key.clone()),
        }
    }

    fn playlist_tiles(&self, cx: &Context<Self>) -> Vec<Tile> {
        let query = self.filters().search.trim().to_lowercase();
        let playing = self.playing_key.clone();
        PlaylistsModel::global(cx)
            .read(cx)
            .playlists()
            .iter()
            .filter(|playlist| query.is_empty() || playlist.name.to_lowercase().contains(&query))
            .map(|playlist| playlist_tile(playlist, playing.as_deref()))
            .collect()
    }

    /// The grid itself: bands of tiles, or one row per entry, beside the jump rail.
    pub(crate) fn grid(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        widgets::scroll::smooth_list("library-grid", &self.scroll, window, cx);
        let facet = self.filters().facet;
        let tiles = self.mode == ViewMode::Grid;
        let width = px(self.tile.width());
        let columns = match tiles {
            true => self.fit_columns(width),
            false => 1,
        };
        self.band_columns = columns;
        let bands = self.tiles.len().div_ceil(columns.max(1));
        let rail = self.jump_rail(cx);
        h_flex()
            .size_full()
            .items_stretch()
            .child(
                div().flex_1().min_w_0().child(
                    uniform_list(
                        "library-grid",
                        bands,
                        cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                            let rows = (range.start * columns)..(range.end * columns);
                            this.ensure_window(&rows, window, cx);
                            range
                                .map(|band| this.band(band, columns, tiles, facet, window, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                ),
            )
            .children(rail)
            .into_any_element()
    }

    /// How many tiles fit across, from the width the list reported at its last layout.
    fn fit_columns(&self, width: Pixels) -> usize {
        let measured: Pixels = self.scroll.0.borrow().base_handle.bounds().size.width;
        if measured <= px(0.) {
            return COLUMNS_FALLBACK;
        }
        // The last tile in a band needs no gap after it.
        (((measured + TILE_GAP) / (width + TILE_GAP)) as usize).max(1)
    }

    /// One virtualized band: a row of tiles, or a single entry in list mode.
    fn band(
        &mut self,
        band: usize,
        columns: usize,
        tiles: bool,
        facet: Facet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first = band * columns;
        let last = (first + columns).min(self.tiles.len());
        let width = px(self.tile.width());
        let held: Vec<(usize, Tile)> = self.tiles[first..last]
            .iter()
            .cloned()
            .enumerate()
            .map(|(offset, tile)| (first + offset, tile))
            .collect();
        let cards: Vec<AnyElement> = held
            .into_iter()
            .map(|(index, tile)| {
                profile::elements::built();
                match tiles {
                    true => self.card(&tile, index, width, facet, window, cx),
                    false => self.line_of(&tile, index, facet, window, cx),
                }
            })
            .collect();
        match tiles {
            true => h_flex()
                .w_full()
                .items_start()
                .gap_6()
                .pb_6()
                .children(cards)
                .into_any_element(),
            false => v_flex().w_full().pb_1().children(cards).into_any_element(),
        }
    }

    fn card(
        &self,
        tile: &Tile,
        index: usize,
        width: Pixels,
        facet: Facet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hovered = self.hovered.as_ref() == Some(&tile.id);
        let selected = self.selection.contains(&tile.id);
        let cursor = self.selection.cursor() == Some(index);
        let cover = width - px(32.);
        let wash = motion::color(
            motion::row_id("library-card", index),
            match selected {
                true => style::white(style::SELECTED, cx),
                false => style::white(if hovered { style::HOVER } else { style::RAISED }, cx),
            },
            window,
            cx,
        );
        let dots = motion::amount(
            motion::row_id("library-card-dots", index),
            widgets::focus::reveal(hovered || cursor || self.menu_open.as_ref() == Some(&tile.id)),
            motion::FAST,
            window,
            cx,
        );
        let title_ink = match tile.playing {
            true => cx.theme().primary,
            false => cx.theme().foreground,
        };
        v_flex()
            .id(("library-card", index))
            .relative()
            .w(width)
            .p_4()
            .rounded(style::radius_xl())
            .bg(wash)
            .when(cursor, |card| card.border_1().border_color(cx.theme().ring))
            .on_hover(self.hover_listener(tile.id.clone(), cx))
            .on_click(self.click_listener(tile.clone(), index, cx))
            .child(
                div()
                    .relative()
                    .child(
                        Cover::new(tile.artwork.as_ref())
                            .size(cover)
                            .rounded(match facet.round_tiles() {
                                true => cover / 2.,
                                false => px(8.),
                            })
                            .zoom(motion::row_id("library-card-cover", index), hovered)
                            .placeholder(facet.placeholder()),
                    )
                    // Nothing in a grid used to say what was playing; a tile of this album does.
                    .when(tile.playing, |frame| {
                        frame.child(
                            div()
                                .absolute()
                                .bottom_2()
                                .left_2()
                                .size_6()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(cx.theme().primary)
                                .child(
                                    icons::icon(icons::VOLUME)
                                        .size(px(12.))
                                        .text_color(cx.theme().primary_foreground),
                                ),
                        )
                    }),
            )
            .child(
                v_flex()
                    .w_full()
                    .mt_3()
                    .when(facet.round_tiles(), |block| block.items_center())
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(title_ink)
                            .when(facet.round_tiles(), |line| line.text_center())
                            .child(tile.title.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .when(facet.round_tiles(), |line| line.text_center())
                            .child(tile.subtitle.clone()),
                    ),
            )
            .child(
                div()
                    .id(("library-card-menu", index))
                    .absolute()
                    .bottom_3()
                    .right_3()
                    // A hidden element takes its own open popup with it, so the fade stops there.
                    .opacity(dots)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        Button::new(("library-card-dots", index))
                            .custom(style::button(
                                style::black(0.4, cx),
                                style::white(style::HOVER, cx),
                                cx.theme().secondary_foreground,
                                cx,
                            ))
                            .icon(icons::icon(icons::MORE))
                            .rounded(cx.theme().radius_full())
                            .dropdown_menu(menus::builder(tile.target.clone()))
                            .on_open_change(cx.listener({
                                let id = tile.id.clone();
                                move |this: &mut Library, open: &bool, _, cx| {
                                    this.menu_open = open.then(|| id.clone());
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .context_menu(menus::builder(tile.target.clone()))
            .into_any_element()
    }

    fn line_of(
        &self,
        tile: &Tile,
        index: usize,
        facet: Facet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hovered = self.hovered.as_ref() == Some(&tile.id);
        let selected = self.selection.contains(&tile.id);
        let wash = motion::color(
            motion::row_id("library-grid-row", index),
            match selected {
                true => style::white(style::SELECTED, cx),
                false => style::white(if hovered { style::HOVER } else { 0. }, cx),
            },
            window,
            cx,
        );
        h_flex()
            .id(("library-grid-row", index))
            .w_full()
            .gap_3()
            .items_center()
            .px_3()
            .py_2()
            .mb_1()
            .rounded(style::radius_lg())
            .bg(wash)
            .on_hover(self.hover_listener(tile.id.clone(), cx))
            .on_click(self.click_listener(tile.clone(), index, cx))
            .when(
                store::ConfigModel::global(cx)
                    .read(cx)
                    .config()
                    .show_row_images,
                |row| {
                    row.child(
                        Cover::new(tile.artwork.as_ref())
                            .size(px(48.))
                            .rounded(match facet.round_tiles() {
                                true => px(24.),
                                false => px(6.),
                            })
                            .placeholder(facet.placeholder()),
                    )
                },
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(match tile.playing {
                                true => cx.theme().primary,
                                false => cx.theme().foreground,
                            })
                            .child(tile.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tile.subtitle.clone()),
                    ),
            )
            .when(tile.playing, |row| {
                row.child(
                    icons::icon(icons::VOLUME)
                        .size(px(14.))
                        .text_color(cx.theme().primary),
                )
            })
            .child(
                div()
                    .id(("library-grid-row-menu", index))
                    .ml_auto()
                    .flex_shrink_0()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        Button::new(("library-grid-row-dots", index))
                            .ghost()
                            .icon(icons::icon(icons::MORE))
                            .dropdown_menu(menus::builder(tile.target.clone())),
                    ),
            )
            .context_menu(menus::builder(tile.target.clone()))
            .into_any_element()
    }

    fn hover_listener(
        &self,
        id: SharedString,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this: &mut Library, hovered: &bool, _, cx| {
            // A leave for a tile that is no longer the hovered one would clear its successor.
            if !*hovered && this.hovered.as_ref() != Some(&id) {
                return;
            }
            this.hovered = hovered.then(|| id.clone());
            cx.notify();
        })
    }

    /// A plain click opens; ctrl and shift build a selection, the same as in the table.
    fn click_listener(
        &self,
        tile: Tile,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this: &mut Library, event: &ClickEvent, window, cx| {
            window.focus(&this.focus, cx);
            let modifiers = event.modifiers();
            let id = tile.id.to_string();
            if modifiers.secondary() {
                this.selection.toggle(&id, index);
            } else if modifiers.shift && this.selection.is_active() {
                let ids: Vec<String> = this.tiles.iter().map(|tile| tile.id.to_string()).collect();
                this.selection
                    .extend_to(index, move |slot| ids.get(slot).cloned());
            } else if this.selection.is_active() {
                this.selection.toggle(&id, index);
            } else {
                this.selection.set_cursor(Some(index));
                Navigator::navigate(tile.route.clone(), cx);
                return;
            }
            cx.notify();
        })
    }

    /// The letters down the right edge: a long grid is reachable without dragging a scrollbar.
    fn jump_rail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.tiles.len() < RAIL_FROM || self.sort().field != "title" {
            return None;
        }
        let present: std::collections::HashSet<char> = self
            .tiles
            .iter()
            .map(|tile| bucket(&tile.sort_key))
            .collect();
        Some(
            v_flex()
                .w(RAIL)
                .flex_shrink_0()
                .items_center()
                .justify_between()
                .py_2()
                .children(LETTERS.chars().map(|letter| {
                    let known = present.contains(&letter);
                    let ink = match known {
                        true => style::white(style::TEXT_SECONDARY, cx),
                        false => style::white(style::TEXT_FAINT, cx),
                    };
                    div()
                        .id(SharedString::from(format!("library-rail-{letter}")))
                        .w_full()
                        .text_center()
                        .text_size(style::text_10())
                        .text_color(ink)
                        .when(known, |entry| {
                            entry
                                .cursor_pointer()
                                .hover(|entry| entry.text_color(cx.theme().foreground))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to_letter(letter, cx)
                                }))
                        })
                        .child(letter.to_string())
                }))
                .into_any_element(),
        )
    }

    fn jump_to_letter(&mut self, letter: char, cx: &mut Context<Self>) {
        let Some(index) = self
            .tiles
            .iter()
            .position(|tile| bucket(&tile.sort_key) == letter)
        else {
            return;
        };
        self.selection.set_cursor(Some(index));
        self.jump_to(index);
        cx.notify();
    }
}

fn playlist_tile(playlist: &PlaylistInfo, playing: Option<&str>) -> Tile {
    let playing = playing.is_some_and(|key| playlist.track_keys.iter().any(|held| held == key));
    Tile {
        id: SharedString::from(playlist.id.clone()),
        title: SharedString::from(playlist.name.clone()),
        subtitle: t_with(
            "playlist_track_count",
            &[("count", playlist.track_keys.len().to_string())],
        ),
        artwork: playlist.artwork.clone(),
        sort_key: playlist.name.to_lowercase(),
        sort_rank: playlist.track_keys.len() as u64,
        playing,
        target: Target::Playlist {
            id: playlist.id.clone().into(),
            name: playlist.name.clone().into(),
        },
        route: Route::Playlist(playlist.id.clone().into()),
    }
}

fn matches_album(
    album: &AlbumInfo,
    query: &str,
    genre: Option<&str>,
    years: Option<(u16, u16)>,
) -> bool {
    if let Some(genre) = genre
        && !album.genre.eq_ignore_ascii_case(genre)
    {
        return false;
    }
    if let Some((from, to)) = years
        && (album.year < from || album.year > to)
    {
        return false;
    }
    query.is_empty()
        || album.title.to_lowercase().contains(query)
        || album.artist.to_lowercase().contains(query)
}

/// Which letter of the rail a title falls under; everything non-alphabetic gathers under `#`.
fn bucket(sort_key: &str) -> char {
    match sort_key.chars().next() {
        Some(first) if first.is_ascii_alphabetic() => first.to_ascii_uppercase(),
        _ => '#',
    }
}

#[cfg(test)]
mod tests {
    use super::bucket;

    #[test]
    fn a_title_that_starts_with_anything_but_a_letter_gathers_under_the_hash() {
        assert_eq!(bucket("abbey road"), 'A');
        assert_eq!(bucket("1989"), '#');
        assert_eq!(bucket(""), '#');
    }
}
