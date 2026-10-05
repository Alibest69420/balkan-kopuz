//! The one cover card every grid draws, its list-row form, and the play button that rides on a cover.

use std::rc::Rc;
use std::time::Duration;

use api::{AlbumInfo, ArtistInfo, ArtworkRef, PlaylistInfo};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};
use store::{Navigator, Route, Target};

use crate::cover::Cover;
use crate::icon_button::{IconButton, IconTone};
use crate::icons;
use crate::motion;
use crate::style;

/// The tile's element and its transition channels, keyed off its id rather than formatted.
const ROOT: u64 = 0;
const HOVER: u64 = 1;
const WASH: u64 = 2;
const COVER: u64 = 3;
const TITLE: u64 = 4;
const DOTS: u64 = 5;
const MENU: u64 = 6;
const CORNER: u64 = 10;

/// `w-10 h-10`: the play button over a cover's corner.
const PLAY_SIZE: Pixels = px(40.);

/// What a click on a card or row does.
pub type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// How a card sits on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Frame {
    /// Cover and lines on the page itself, the cover washing under the pointer: shelves and grids of art.
    #[default]
    Bare,
    /// A raised, padded card whose menu shows on hover: the catalog grids.
    Card,
}

/// A cover with a title under it, in every grid of the app.
#[derive(IntoElement)]
pub struct Tile {
    id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    artwork: Option<ArtworkRef>,
    size: Pixels,
    round: bool,
    placeholder: &'static str,
    frame: Frame,
    click: Option<OnClick>,
    menu: Option<Target>,
    play: Option<(PlayState, OnClick)>,
    corner: Option<AnyElement>,
    playing: bool,
    selected: bool,
    cursor: bool,
    tint: bool,
    zoom_speed: Duration,
}

impl Tile {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            artwork: None,
            size: px(160.),
            round: false,
            placeholder: icons::MUSIC,
            frame: Frame::Bare,
            click: None,
            menu: None,
            play: None,
            corner: None,
            playing: false,
            selected: false,
            cursor: false,
            tint: false,
            zoom_speed: motion::SLOW,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        let subtitle = subtitle.into();
        self.subtitle = (!subtitle.is_empty()).then_some(subtitle);
        self
    }

    pub fn artwork(mut self, artwork: Option<&ArtworkRef>) -> Self {
        self.artwork = artwork.cloned();
        self
    }

    /// The cover's edge; a framed card adds its padding around it.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Artists are round everywhere in the app.
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    pub fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }

    pub fn frame(mut self, frame: Frame) -> Self {
        self.frame = frame;
        self
    }

    /// Where a click goes.
    pub fn route(self, route: Route) -> Self {
        self.on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
    }

    /// What a click does, when it is more than a navigation.
    pub fn on_click(
        mut self,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some(Rc::new(click));
        self
    }

    /// What its menu acts on: right-click anywhere, and on a framed card a dots button too.
    pub fn menu(mut self, target: Option<Target>) -> Self {
        self.menu = target;
        self
    }

    /// A play button over the cover's corner, shown under the pointer or the keyboard.
    pub fn play(
        mut self,
        state: PlayState,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.play = Some((state, Rc::new(click)));
        self
    }

    /// A control over the cover's top corner, shown under the pointer or the keyboard.
    pub fn corner(mut self, corner: Option<impl IntoElement>) -> Self {
        self.corner = corner.map(IntoElement::into_any_element);
        self
    }

    /// What it names is playing: a badge on the cover, and the title in the accent.
    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The keyboard's place in a grid it walks itself.
    pub fn cursor(mut self, cursor: bool) -> Self {
        self.cursor = cursor;
        self
    }

    /// The title takes the accent under the pointer, as artists and playlists do upstream.
    pub fn tint(mut self, tint: bool) -> Self {
        self.tint = tint;
        self
    }

    /// How long the cover's hover zoom takes.
    pub fn zoom_speed(mut self, duration: Duration) -> Self {
        self.zoom_speed = duration;
        self
    }
}

impl RenderOnce for Tile {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        profile::elements::built();
        let id = self.id.clone();
        let framed = self.frame == Frame::Card;
        let radius = match (self.round, framed) {
            (true, _) => self.size / 2.,
            (false, true) => style::to_px(style::radius_lg(), window),
            (false, false) => style::to_px(style::radius_xl(), window),
        };
        let (hovered, pointer) = motion::hovered(motion::channel(&id, HOVER), window, cx);
        let focus = crate::focus::handle(format!("tile-focus:{id}"), window, cx);
        let focused = focus.is_focused(window);
        let menu_open = window.use_keyed_state(motion::channel(&id, MENU), cx, |_, _| false);
        let open = *menu_open.read(cx);
        let wash = motion::color(
            motion::channel(&id, WASH),
            match (framed, self.selected, hovered || focused) {
                (true, true, _) => style::white(style::SELECTED, cx),
                (true, false, true) => style::white(style::HOVER, cx),
                (true, false, false) => style::white(style::RAISED, cx),
                (false, _, true) => style::black(0.2, cx),
                (false, _, false) => style::black(0., cx),
            },
            window,
            cx,
        );
        let title_ink = motion::color(
            motion::channel(&id, TITLE),
            match (self.playing, self.tint && hovered) {
                (true, _) => cx.theme().primary,
                (false, true) => cx.theme().primary_hover,
                (false, false) => cx.theme().foreground,
            },
            window,
            cx,
        );
        // Only the card under the pointer or the keyboard builds its button; the rest are art and lines.
        let play = self
            .play
            .filter(|_| hovered || focused)
            .map(|(state, click)| {
                let glyph = match state {
                    PlayState::Playing => icons::PAUSE,
                    _ => icons::PLAY,
                };
                IconButton::new(SharedString::from(format!("{id}-play")), glyph)
                    .size(PLAY_SIZE)
                    .glyph(px(14.))
                    .tone(IconTone::Accent)
                    .shadow(true)
                    .loading(state == PlayState::Loading)
                    // The button sits on the card, whose own click opens what it names.
                    .stop_propagation(true)
                    .on_click(move |event, window, cx| click(event, window, cx))
            });
        let click = self.click.clone();
        let opens = self.click;

        let cover = div()
            .relative()
            .child(
                Cover::new(self.artwork.as_ref())
                    .size(self.size)
                    .rounded(radius)
                    .placeholder(self.placeholder)
                    .zoom(motion::channel(&id, COVER), hovered)
                    .zoom_speed(self.zoom_speed),
            )
            .when(!framed, |cover| {
                cover.child(div().absolute().inset_0().rounded(radius).bg(wash))
            })
            .when(self.playing, |cover| cover.child(playing_badge(cx)))
            .when_some(play, |cover, play| {
                cover.child(div().absolute().right_2().bottom_2().child(play))
            })
            .when_some(
                self.corner.filter(|_| hovered || focused),
                |cover, corner| {
                    cover.child(
                        div()
                            .id(motion::channel(&id, CORNER))
                            .absolute()
                            .top_2()
                            .right_2()
                            // The corner sits on the card, whose own click opens what it names.
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(corner),
                    )
                },
            );

        let lines = v_flex()
            .w(self.size)
            .when(!framed, |lines| lines.px_1().gap_1())
            .when(self.round, |lines| lines.items_center())
            .child(
                div()
                    .w_full()
                    .truncate()
                    .when(self.round, |line| line.text_center())
                    .map(|line| match framed {
                        true => line.font_weight(FontWeight::MEDIUM),
                        false => line.font_weight(FontWeight::BOLD),
                    })
                    .text_color(title_ink)
                    .child(self.title),
            )
            .when_some(self.subtitle, |lines, subtitle| {
                lines.child(
                    div()
                        .w_full()
                        .truncate()
                        .text_sm()
                        .when(!framed, |line| line.font_weight(FontWeight::SEMIBOLD))
                        .when(self.round, |line| line.text_center())
                        .text_color(style::white(style::TEXT_MUTED, cx))
                        .child(subtitle),
                )
            });

        let dots = (framed && self.menu.is_some()).then(|| {
            let shown = motion::amount(
                motion::channel(&id, DOTS),
                crate::focus::reveal(hovered || focused || self.cursor || open),
                motion::FAST,
                window,
                cx,
            );
            dots_button(&id, self.menu.clone(), menu_open.clone(), cx)
                .absolute()
                .bottom_3()
                .right_3()
                // A hidden element takes its own open popup with it, so the fade stops at a resting level.
                .opacity(shown)
        });

        let tile = crate::focus::stop(
            v_flex().id(motion::channel(&id, ROOT)),
            &focus,
            move |window, cx| {
                if let Some(click) = opens.as_ref() {
                    click(&ClickEvent::default(), window, cx);
                }
            },
        )
        .relative()
        .flex_shrink_0()
        .map(|tile| match framed {
            true => tile.gap_3().p_4().rounded(style::radius_xl()).bg(wash),
            false => tile.gap_4(),
        })
        .when(click.is_some(), |tile| tile.cursor_pointer())
        .on_hover(motion::track(pointer))
        .when_some(click, |tile, click| {
            tile.on_click(move |event, window, cx| click(event, window, cx))
        })
        .child(cover)
        .child(lines)
        .children(dots)
        .when(focused || self.cursor, |tile| {
            tile.child(crate::focus::ring(style::radius_xl(), cx))
        });
        with_menu(tile, self.menu)
    }
}

/// Right-click opens what `target` names, when there is one.
fn with_menu(element: Stateful<Div>, target: Option<Target>) -> AnyElement {
    match target {
        Some(target) => element
            .context_menu(crate::menus::builder(target))
            .into_any_element(),
        None => element.into_any_element(),
    }
}

/// The same thing as a list row: a thumb, two lines, and its menu at the end.
#[derive(IntoElement)]
pub struct TileRow {
    id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    artwork: Option<ArtworkRef>,
    round: bool,
    placeholder: &'static str,
    lead: Option<SharedString>,
    detail: Option<SharedString>,
    click: Option<OnClick>,
    menu: Option<Target>,
    playing: bool,
    selected: bool,
}

impl TileRow {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            artwork: None,
            round: false,
            placeholder: icons::MUSIC,
            lead: None,
            detail: None,
            click: None,
            menu: None,
            playing: false,
            selected: false,
        }
    }

    /// A rank before the thumb, for a list that is an ordering.
    pub fn lead(mut self, lead: impl Into<SharedString>) -> Self {
        self.lead = Some(lead.into());
        self
    }

    /// A muted note at the row's end, such as a year or a count.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.is_empty()).then_some(detail);
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        let subtitle = subtitle.into();
        self.subtitle = (!subtitle.is_empty()).then_some(subtitle);
        self
    }

    pub fn artwork(mut self, artwork: Option<&ArtworkRef>) -> Self {
        self.artwork = artwork.cloned();
        self
    }

    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    pub fn placeholder(mut self, icon: &'static str) -> Self {
        self.placeholder = icon;
        self
    }

    pub fn route(self, route: Route) -> Self {
        self.on_click(move |_, _, cx| Navigator::navigate(route.clone(), cx))
    }

    pub fn on_click(
        mut self,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some(Rc::new(click));
        self
    }

    pub fn menu(mut self, target: Option<Target>) -> Self {
        self.menu = target;
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

/// The edge of a row's thumb.
const ROW_THUMB: Pixels = px(48.);

impl RenderOnce for TileRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        profile::elements::built();
        let id = self.id.clone();
        let (hovered, pointer) = motion::hovered(motion::channel(&id, HOVER), window, cx);
        let focus = crate::focus::handle(format!("row-focus:{id}"), window, cx);
        let focused = focus.is_focused(window);
        let menu_open = window.use_keyed_state(motion::channel(&id, MENU), cx, |_, _| false);
        let open = *menu_open.read(cx);
        let wash = motion::color(
            motion::channel(&id, WASH),
            match (self.selected, hovered || focused) {
                (true, _) => style::white(style::SELECTED, cx),
                (false, true) => style::white(style::HOVER, cx),
                (false, false) => style::white(0., cx),
            },
            window,
            cx,
        );
        let dots = motion::amount(
            motion::channel(&id, DOTS),
            crate::focus::reveal(hovered || focused || open),
            motion::FAST,
            window,
            cx,
        );
        let covers = store::ConfigModel::global(cx).read(cx).row_images();
        let opens = self.click.clone();
        let row = crate::focus::stop(
            h_flex().id(motion::channel(&id, ROOT)),
            &focus,
            move |window, cx| {
                if let Some(click) = opens.as_ref() {
                    click(&ClickEvent::default(), window, cx);
                }
            },
        )
        .relative()
        .w_full()
        .gap_3()
        .items_center()
        .px_3()
        .py_2()
        .mb_1()
        .rounded(style::radius_lg())
        .bg(wash)
        .on_hover(motion::track(pointer))
        .when_some(self.click, |row, click| {
            row.cursor_pointer()
                .on_click(move |event, window, cx| click(event, window, cx))
        })
        .when_some(self.lead, |row, lead| {
            row.child(
                div()
                    .w(px(24.))
                    .flex_none()
                    .text_right()
                    // Tabular digits, so the numbers line up rather than wander by glyph width.
                    .font_family(SharedString::new_static("monospace"))
                    .text_xs()
                    .text_color(style::white(style::TEXT_DIM, cx))
                    .child(lead),
            )
        })
        .when(covers, |row| {
            row.child(
                Cover::new(self.artwork.as_ref())
                    .size(ROW_THUMB)
                    .rounded(match self.round {
                        true => ROW_THUMB / 2.,
                        false => style::thumb_radius(),
                    })
                    .placeholder(self.placeholder),
            )
        })
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(match self.playing {
                            true => cx.theme().primary,
                            false => cx.theme().foreground,
                        })
                        .child(self.title),
                )
                .when_some(self.subtitle, |lines, subtitle| {
                    lines.child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(style::white(style::TEXT_MUTED, cx))
                            .child(subtitle),
                    )
                }),
        )
        .when(self.playing, |row| {
            row.child(
                icons::icon(icons::VOLUME)
                    .size(px(14.))
                    .text_color(cx.theme().primary),
            )
        })
        .when_some(self.detail, |row, detail| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .child(detail),
            )
        })
        .when(self.menu.is_some(), |row| {
            row.child(
                dots_button(&id, self.menu.clone(), menu_open, cx)
                    .flex_shrink_0()
                    .opacity(dots),
            )
        })
        .when(focused, |row| {
            row.child(crate::focus::ring(style::radius_lg(), cx))
        });
        with_menu(row, self.menu)
    }
}

/// The dots that open a card's or a row's menu; pressing them is never a click on what holds them.
fn dots_button(
    id: &SharedString,
    target: Option<Target>,
    open: Entity<bool>,
    cx: &App,
) -> Stateful<Div> {
    let slot = div()
        .id(SharedString::from(format!("menu-slot-{id}")))
        .on_click(|_, _, cx| cx.stop_propagation());
    let Some(target) = target else {
        return slot;
    };
    slot.child(
        Button::new(SharedString::from(format!("menu-{id}")))
            .custom(style::button(
                style::black(0.4, cx),
                style::white(style::HOVER, cx),
                cx.theme().secondary_foreground,
                cx,
            ))
            .icon(icons::icon(icons::MORE))
            .rounded(cx.theme().radius_full())
            .dropdown_menu(crate::menus::builder(target))
            .on_open_change(move |shown: &bool, _, cx| {
                let shown = *shown;
                open.update(cx, |open, cx| {
                    *open = shown;
                    cx.notify();
                });
            }),
    )
}

/// `bg-primary` circle on a playing cover's corner.
fn playing_badge(cx: &App) -> AnyElement {
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
        )
        .into_any_element()
}

/// What a cover's play button shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Idle,
    /// What it names is the thing playing, so a press pauses.
    Playing,
    /// The press is on its way; the queue is not built yet.
    Loading,
}

/// The album tile: click opens the album, its menu is the album's.
pub fn album_tile(album: &AlbumInfo) -> Tile {
    Tile::new(album.id.clone(), album.title.clone())
        .subtitle(album.artist.clone())
        .artwork(album.artwork.as_ref())
        .placeholder(icons::DISC)
        .route(Route::Album(album.id.clone().into()))
        .menu(Some(Target::Album {
            id: album.id.clone().into(),
            title: album.title.clone().into(),
        }))
}

/// The artist tile: round, click opens the artist.
pub fn artist_tile(artist: &ArtistInfo) -> Tile {
    Tile::new(artist.key.to_string(), artist.name.clone())
        .artwork(artist.artwork.as_ref())
        .placeholder(icons::MIC)
        .round(true)
        .tint(true)
        .zoom_speed(motion::SLOWEST)
        .route(Route::Artist(artist.key.clone()))
        .menu(Some(Target::Artist {
            key: artist.key.clone(),
            name: artist.name.clone().into(),
        }))
}

/// The album as a list row: click opens the album, its year at the end.
pub fn album_row(album: &AlbumInfo) -> TileRow {
    TileRow::new(album.id.clone(), album.title.clone())
        .subtitle(album.artist.clone())
        .artwork(album.artwork.as_ref())
        .placeholder(icons::DISC)
        .detail(match album.year {
            0 => SharedString::default(),
            year => year.to_string().into(),
        })
        .route(Route::Album(album.id.clone().into()))
        .menu(Some(Target::Album {
            id: album.id.clone().into(),
            title: album.title.clone().into(),
        }))
}

/// The artist as a list row: a round thumb, click opens the artist.
pub fn artist_row(artist: &ArtistInfo) -> TileRow {
    TileRow::new(artist.key.clone(), artist.name.clone())
        .artwork(artist.artwork.as_ref())
        .placeholder(icons::MIC)
        .round(true)
        .route(Route::Artist(artist.key.clone()))
        .menu(Some(Target::Artist {
            key: artist.key.clone(),
            name: artist.name.clone().into(),
        }))
}

/// The playlist tile: click opens the playlist.
pub fn playlist_tile(playlist: &PlaylistInfo) -> Tile {
    Tile::new(playlist.id.clone(), playlist.name.clone())
        .artwork(playlist.artwork.as_ref())
        .placeholder(icons::LIST)
        .tint(true)
        .zoom_speed(motion::SLOWEST)
        .route(Route::Playlist(playlist.id.clone().into()))
}

/// The wrapping grid every tile page lays its tiles out in.
pub fn tile_grid() -> gpui_kit::Div {
    div().flex().flex_wrap().gap_6()
}
