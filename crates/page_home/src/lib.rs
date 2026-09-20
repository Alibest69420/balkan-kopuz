//! Home: the sections `cfg.home_sections` lists, in its order, and the edit mode that rearranges them.

mod cards;
mod data;
mod hero;
mod sections;

use std::collections::HashSet;

use api::{AlbumPage, ArtistPage, JobKind, Page, Table, TrackPage};
use config::{HomeSection, ListenNowStyle, UiStyle};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};
use store::{
    ConfigModel, FavoritesModel, JobsModel, PlayerModel, PlaylistsModel, Query, QueueModel,
    SourcesModel,
};
use widgets::format::t;
use widgets::{PageHeader, icons, motion, style};

use data::{ARTIST_SAMPLE_WINDOW, Data, RECENT_TRACKS_WINDOW, RECENTLY_ADDED_WINDOW};

/// Every element the pointer is over: a tween needs the state, which a `.hover()` refinement cannot give.
pub(crate) type Hovered = HashSet<SharedString>;

/// One motion channel of an element, keyed by that element's own id: `("home-album-42", "cover")`.
pub(crate) fn channel(base: impl Into<SharedString>, name: &'static str) -> ElementId {
    (ElementId::from(base.into()), name).into()
}

/// Feed an element's hover into the page's set, so the next frame can tween towards it.
pub(crate) fn track_hover<E: StatefulInteractiveElement>(
    element: E,
    id: SharedString,
    cx: &mut Context<Home>,
) -> E {
    element.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
        let changed = if *hovered {
            this.hovered.insert(id.clone())
        } else {
            this.hovered.remove(&id)
        };
        if changed {
            cx.notify();
        }
    }))
}

/// One scroller per row, kept across frames so an arrow nudges from where the row already is.
#[derive(Default)]
struct Scrollers {
    continue_listening: ScrollHandle,
    listen_now: ScrollHandle,
    top_artists: ScrollHandle,
    new_releases: ScrollHandle,
    made_for_you: ScrollHandle,
    recently_added: ScrollHandle,
    playlists: ScrollHandle,
    // The page itself, so its wheel glides the way every other page's does.
    page: ScrollHandle,
}

pub struct Home {
    albums: Entity<Query<AlbumPage>>,
    recently_added: Entity<Query<AlbumPage>>,
    artists: Entity<Query<ArtistPage>>,
    recents: Entity<Query<TrackPage>>,
    samples: Entity<Query<TrackPage>>,
    genre: Entity<Query<Option<String>>>,
    hero_tracks: Entity<Query<TrackPage>>,
    hero_album_id: String,
    data: Data,
    edit_mode: bool,
    hovered: Hovered,
    // Where the hero resize drag started, and the height it started from.
    resize: Option<(Pixels, u32)>,
    scroll: Scrollers,
    seed: u64,
    // The empty-library sync is offered once per visit, not once per refetch.
    synced: bool,
    _subscriptions: Vec<Subscription>,
}

impl Home {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums],
                |client| async move { client.albums(store::all()).await },
                cx,
            )
        });
        let recently_added = cx.new(|cx| {
            Query::new(
                &[Table::Albums, Table::Tracks],
                |client| async move {
                    client
                        .albums_recently_added(page(RECENTLY_ADDED_WINDOW))
                        .await
                },
                cx,
            )
        });
        let artists = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |client| async move { client.artists(store::all()).await },
                cx,
            )
        });
        let recents = cx.new(|cx| {
            Query::new(
                &[Table::Recents],
                |client| async move { client.recent_tracks(page(RECENT_TRACKS_WINDOW)).await },
                cx,
            )
        });
        let samples = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |client| async move {
                    client
                        .artist_sample_tracks(page(ARTIST_SAMPLE_WINDOW))
                        .await
                },
                cx,
            )
        });
        let genre = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |client| async move { client.top_genre().await },
                cx,
            )
        });
        let hero_tracks = cx.new(|cx| {
            Query::new(
                &[Table::Tracks],
                |_| async move { Ok(TrackPage::default()) },
                cx,
            )
        });

        let subscriptions = vec![
            cx.observe(&albums, |this, _, cx| this.rebuild(cx)),
            cx.observe(&recently_added, |this, _, cx| this.rebuild(cx)),
            cx.observe(&artists, |this, _, cx| this.rebuild(cx)),
            cx.observe(&recents, |this, _, cx| this.rebuild(cx)),
            cx.observe(&samples, |this, _, cx| this.rebuild(cx)),
            cx.observe(&genre, |this, _, cx| this.rebuild(cx)),
            cx.observe(&hero_tracks, |_, _, cx| cx.notify()),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| this.rebuild(cx)),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&PlaylistsModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&FavoritesModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&PlayerModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&QueueModel::global(cx), |_, _, cx| cx.notify()),
        ];

        Self {
            albums,
            recently_added,
            artists,
            recents,
            samples,
            genre,
            hero_tracks,
            hero_album_id: String::new(),
            data: Data::default(),
            edit_mode: false,
            hovered: Hovered::default(),
            resize: None,
            scroll: Scrollers::default(),
            seed: seed(),
            synced: false,
            _subscriptions: subscriptions,
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let built = data::build(data::Inputs {
            albums: self.albums.read(cx).value(),
            recently_added: self.recently_added.read(cx).value(),
            artists: self.artists.read(cx).value(),
            recents: self.recents.read(cx).value(),
            samples: self.samples.read(cx).value(),
            genre: self.genre.read(cx).value().and_then(|genre| genre.as_ref()),
            seed: self.seed,
        });
        self.data = built;
        self.sync_hero(cx);
        self.start_empty_library_sync(cx);
        cx.notify();
    }

    /// The hero's album tracks answer its heart, so the read follows whichever track is featured.
    fn sync_hero(&mut self, cx: &mut Context<Self>) {
        let album_id = self
            .data
            .hero
            .as_ref()
            .map(|hero| hero.track.album_id.clone())
            .unwrap_or_default();
        if album_id == self.hero_album_id {
            return;
        }
        self.hero_album_id = album_id.clone();
        self.hero_tracks.update(cx, |query, cx| {
            query.set_fetch(
                move |client| {
                    let id = album_id.clone();
                    async move {
                        if id.is_empty() {
                            return Ok(TrackPage::default());
                        }
                        client.album_tracks(id, store::all()).await
                    }
                },
                cx,
            )
        });
    }

    /// A server fills an empty cache by syncing; a local library is populated by the scan instead.
    fn start_empty_library_sync(&mut self, cx: &mut Context<Self>) {
        if self.synced || !SourcesModel::global(cx).read(cx).capabilities().sync {
            return;
        }
        let empty = match self.albums.read(cx).value() {
            Some(page) => page.albums.is_empty(),
            None => return,
        };
        self.synced = true;
        if empty {
            JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(JobKind::LibrarySync, cx));
        }
    }

    fn move_section(&mut self, key: &str, delta: isize, cx: &mut Context<Self>) {
        let key = key.to_string();
        ConfigModel::global(cx).update(cx, |config, cx| {
            config.update(cx, |app| {
                let Some(index) = app.home_sections.iter().position(|s| s.key == key) else {
                    return;
                };
                let target = index as isize + delta;
                if target >= 0 && (target as usize) < app.home_sections.len() {
                    app.home_sections.swap(index, target as usize);
                }
            })
        });
    }

    fn toggle_section(&mut self, key: &str, cx: &mut Context<Self>) {
        let key = key.to_string();
        ConfigModel::global(cx).update(cx, |config, cx| {
            config.update(cx, |app| {
                if let Some(section) = app.home_sections.iter_mut().find(|s| s.key == key) {
                    section.enabled = !section.enabled;
                }
            })
        });
    }

    fn reset_sections(&mut self, cx: &mut Context<Self>) {
        ConfigModel::global(cx).update(cx, |config, cx| {
            config.update(cx, |app| {
                app.home_sections = config::default_home_sections();
            })
        });
    }

    fn toggle_listen_now_style(&mut self, cx: &mut Context<Self>) {
        ConfigModel::global(cx).update(cx, |config, cx| {
            config.update(cx, |app| {
                app.listen_now_style = match app.listen_now_style {
                    ListenNowStyle::List => ListenNowStyle::Cards,
                    ListenNowStyle::Cards => ListenNowStyle::List,
                };
            })
        });
    }

    fn section(
        &mut self,
        key: &str,
        skin: Skin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match key {
            "hero" => self.hero(skin.hero_height, self.edit_mode, window, cx),
            "continue_listening" => self.continue_listening(skin, window, cx),
            "listen_now" => self.listen_now(skin, window, cx),
            "top_artists" => self.top_artists(skin, window, cx),
            "new_releases" => sections::album_row(
                "home-new-releases",
                t("new_releases"),
                skin.eyebrow(t("albums")),
                &self.data.new_releases,
                &self.scroll.new_releases,
                &self.hovered,
                window,
                cx,
            ),
            "made_for_you" => {
                let eyebrow = if self.data.genre.is_empty() {
                    t("music")
                } else {
                    SharedString::from(self.data.genre.clone())
                };
                sections::album_row(
                    "home-made-for-you",
                    t("made_for_you"),
                    // The genre is what "made for you" means, so this row names it on any skin.
                    Some(eyebrow),
                    &self.data.made_for_you,
                    &self.scroll.made_for_you,
                    &self.hovered,
                    window,
                    cx,
                )
            }
            "recently_added" => sections::album_row(
                "home-recently-added",
                t("recently_added"),
                skin.eyebrow(t("library")),
                &self.data.recently_added,
                &self.scroll.recently_added,
                &self.hovered,
                window,
                cx,
            ),
            "playlists" => self.playlists(skin, window, cx),
            _ => div().into_any_element(),
        }
    }

    fn continue_listening(
        &mut self,
        skin: Skin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.data.continue_listening.is_empty() {
            return div().into_any_element();
        }
        let hovered = &self.hovered;
        let cards: Vec<AnyElement> = self
            .data
            .continue_listening
            .iter()
            .map(|recent| cards::recent_card(recent, hovered, window, cx))
            .collect();
        v_flex()
            .w_full()
            .child(sections::heading(
                "home-continue",
                t("continue_listening"),
                skin.eyebrow(t("library")),
                Some(&self.scroll.continue_listening),
                &self.hovered,
                window,
                cx,
            ))
            .child(sections::scroller(
                "home-continue",
                &self.scroll.continue_listening,
                cards,
            ))
            .into_any_element()
    }

    fn listen_now(
        &mut self,
        skin: Skin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.data.shuffled.is_empty() {
            return div().into_any_element();
        }
        // The first album is the one the rest of the page leads with, so the row starts after it.
        let limit = if skin.listen_now_cards { 10 } else { 8 };
        let albums: Vec<api::AlbumInfo> = self
            .data
            .shuffled
            .iter()
            .skip(1)
            .take(limit)
            .cloned()
            .collect();
        let hovered = &self.hovered;
        let body = if skin.listen_now_cards {
            let cards: Vec<AnyElement> = albums
                .iter()
                .map(|album| cards::listen_now_card(album, hovered, window, cx))
                .collect();
            sections::scroller("home-listen-now", &self.scroll.listen_now, cards)
        } else {
            let rows: Vec<AnyElement> = albums
                .iter()
                .map(|album| cards::album_row(album, hovered, window, cx))
                .collect();
            h_flex()
                .w_full()
                .flex_wrap()
                .gap_4()
                .children(rows)
                .into_any_element()
        };
        v_flex()
            .w_full()
            .child(sections::heading_lead(
                "home-listen-now",
                t("listen_now"),
                skin.eyebrow(t("music")),
                skin.listen_now_cards.then_some(&self.scroll.listen_now),
                &self.hovered,
                window,
                cx,
            ))
            .child(body)
            .into_any_element()
    }

    fn top_artists(
        &mut self,
        skin: Skin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.data.top_artists.is_empty() {
            return div().into_any_element();
        }
        let hovered = &self.hovered;
        let cards: Vec<AnyElement> = self
            .data
            .top_artists
            .iter()
            .map(|artist| {
                cards::artist_card(&artist.name, artist.artwork.as_ref(), hovered, window, cx)
            })
            .collect();
        v_flex()
            .w_full()
            .child(sections::heading(
                "home-artists",
                t("top_artists"),
                skin.eyebrow(t("artists")),
                Some(&self.scroll.top_artists),
                &self.hovered,
                window,
                cx,
            ))
            .child(sections::scroller(
                "home-artists",
                &self.scroll.top_artists,
                cards,
            ))
            .into_any_element()
    }

    fn playlists(&mut self, skin: Skin, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // TODO(offline): offline mode keeps only playlists whose every track is downloaded.
        let playlists: Vec<api::PlaylistInfo> = PlaylistsModel::global(cx)
            .read(cx)
            .playlists()
            .iter()
            .rev()
            .take(10)
            .cloned()
            .collect();
        if playlists.is_empty() {
            return div().into_any_element();
        }
        let hovered = &self.hovered;
        let cards: Vec<AnyElement> = playlists
            .iter()
            .map(|playlist| cards::playlist_card(playlist, hovered, window, cx))
            .collect();
        v_flex()
            .w_full()
            .child(sections::heading(
                "home-playlists",
                t("playlists"),
                skin.eyebrow(t("library")),
                Some(&self.scroll.playlists),
                &self.hovered,
                window,
                cx,
            ))
            .child(sections::scroller(
                "home-playlists",
                &self.scroll.playlists,
                cards,
            ))
            .into_any_element()
    }

    fn header(&mut self, skin: Skin, cx: &mut Context<Self>) -> AnyElement {
        let title = if skin.vaxry {
            SharedString::default()
        } else {
            t("home")
        };
        PageHeader::new(title)
            .when_else(
                self.edit_mode,
                |header| {
                    header
                        .action(
                            outlined(pill(Button::new("home-reset").custom(chip(cx))), cx)
                                .icon(icons::icon(icons::REFRESH))
                                .label(t("reset_home"))
                                .on_click(cx.listener(|this, _, _, cx| this.reset_sections(cx))),
                        )
                        .action(
                            pill(Button::new("home-done").primary())
                                .px_4()
                                .font_weight(FontWeight::BOLD)
                                .icon(icons::icon(icons::CHECK))
                                .label(t("done_editing"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit_mode = false;
                                    cx.notify();
                                })),
                        )
                },
                |header| {
                    header.action(
                        outlined(pill(Button::new("home-customize").custom(chip(cx))), cx)
                            .icon(icons::icon(icons::SETTINGS))
                            .label(t("customize_home"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = true;
                                cx.notify();
                            })),
                    )
                },
            )
            .into_any_element()
    }

    fn edit_row(
        &mut self,
        section: &HomeSection,
        index: usize,
        total: usize,
        skin: Skin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = section.key.clone();
        let enabled = section.enabled;
        let toggle = toggle_chip(&key, enabled, window, cx);
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .p_2()
            .rounded(style::radius_lg())
            .bg(style::white(style::RAISED, cx))
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(style::white(0.8, cx))
                    .child(section_label(&key)),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .when(key == "listen_now", |row| {
                        row.child(
                            Button::new("home-listen-now-layout")
                                .custom(chip(cx))
                                .h_7()
                                .px_3()
                                .rounded(px(6.))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .icon(icons::icon(if skin.listen_now_cards {
                                    icons::GRID
                                } else {
                                    icons::ROWS
                                }))
                                .label(t(if skin.listen_now_cards {
                                    "layout_cards"
                                } else {
                                    "layout_list"
                                }))
                                .tooltip(t("listen_now_layout"))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_listen_now_style(cx)),
                                ),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("home-up-{key}")))
                            .custom(chip(cx))
                            .size_7()
                            .rounded(px(6.))
                            .icon(icons::icon(icons::CHEVRON_UP))
                            .tooltip(t("move_up"))
                            .disabled(index == 0)
                            .on_click(cx.listener({
                                let key = key.clone();
                                move |this, _, _, cx| this.move_section(&key, -1, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("home-down-{key}")))
                            .custom(chip(cx))
                            .size_7()
                            .rounded(px(6.))
                            .icon(icons::icon(icons::CHEVRON_DOWN))
                            .tooltip(t("move_down"))
                            .disabled(index + 1 >= total)
                            .on_click(cx.listener({
                                let key = key.clone();
                                move |this, _, _, cx| this.move_section(&key, 1, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("home-toggle-{key}")))
                            .custom(toggle)
                            .h_7()
                            .px_3()
                            .rounded(px(6.))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .icon(icons::icon(if enabled {
                                icons::CLOSE
                            } else {
                                icons::CHECK
                            }))
                            .label(t(if enabled {
                                "hide_section"
                            } else {
                                "show_section"
                            }))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.toggle_section(&key, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

/// What the skin and the settings decide before any section renders.
#[derive(Clone, Copy)]
struct Skin {
    vaxry: bool,
    listen_now_cards: bool,
    hero_height: u32,
}

impl Skin {
    /// The small label the Vaxry skin puts above a heading; the normal skin has none.
    fn eyebrow(self, label: SharedString) -> Option<SharedString> {
        self.vaxry.then_some(label)
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        widgets::scroll::smooth("home-scroll", &self.scroll.page, window, cx);
        let (skin, sections) = {
            let model = ConfigModel::global(cx);
            let config = model.read(cx).config();
            (
                Skin {
                    vaxry: config.ui_style == UiStyle::Vaxry,
                    listen_now_cards: config.listen_now_style == ListenNowStyle::Cards,
                    hero_height: config.hero_height,
                },
                config.home_sections.clone(),
            )
        };
        let edit = self.edit_mode;
        let total = sections.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, section) in sections.iter().enumerate() {
            if !section.enabled && !edit {
                continue;
            }
            let controls = edit.then(|| self.edit_row(section, index, total, skin, window, cx));
            let body = self.section(&section.key, skin, window, cx);
            let fade = motion::amount(
                channel(format!("home-section-{}", section.key), "fade"),
                if section.enabled { 1. } else { 0.4 },
                motion::MEDIUM,
                window,
                cx,
            );
            rows.push(
                v_flex()
                    .w_full()
                    .gap_2()
                    .when(fade < 1., |row| row.opacity(fade))
                    .children(controls)
                    .child(body)
                    .into_any_element(),
            );
        }
        let header = self.header(skin, cx);
        v_flex()
            .id("home")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll.page)
            .p_8()
            .pb_32()
            .gap_12()
            .child(header)
            .children(rows)
    }
}

fn page(limit: u32) -> Page {
    Page { offset: 0, limit }
}

/// `px-3 h-8 rounded-full text-xs font-semibold`: the chrome pills the header carries.
fn pill(button: Button) -> Button {
    button
        .h_8()
        .px_3()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
}

/// `border border-white/10`, which only the quiet pills wear.
fn outlined(button: Button, cx: &App) -> Button {
    button
        .border_1()
        .border_color(style::white(style::BORDER, cx))
}

/// `bg-white/5 hover:bg-white/15 text-white/70`: a quiet control in the chrome.
fn chip(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(style::white(style::RAISED, cx))
        .foreground(style::white(0.7, cx))
        .hover(style::white(0.15, cx))
        .active(style::white(style::ACTIVE, cx))
}

/// The show/hide chip, tweening between the quiet chip and `bg-indigo-500/20 text-indigo-300`.
fn toggle_chip(
    key: &str,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<Home>,
) -> ButtonCustomVariant {
    let id = format!("home-toggle-{key}");
    let color = if enabled {
        cx.theme().primary.opacity(0.2)
    } else {
        style::white(style::RAISED, cx)
    };
    let foreground = if enabled {
        cx.theme().primary_hover
    } else {
        style::white(0.7, cx)
    };
    let hover = if enabled {
        cx.theme().primary.opacity(0.3)
    } else {
        style::white(0.15, cx)
    };
    let color = motion::color(channel(id.clone(), "bg"), color, window, cx);
    let foreground = motion::color(channel(id, "fg"), foreground, window, cx);
    ButtonCustomVariant::new(cx)
        .color(color)
        .foreground(foreground)
        .hover(hover)
        .active(hover)
}

fn section_label(key: &str) -> SharedString {
    match key {
        "hero" => t("home_section_hero"),
        "continue_listening" => t("home_section_continue_listening"),
        "listen_now" => t("home_section_listen_now"),
        "top_artists" => t("home_section_top_artists"),
        "new_releases" => t("home_section_new_releases"),
        "made_for_you" => t("home_section_made_for_you"),
        "recently_added" => t("home_section_recently_added"),
        "playlists" => t("home_section_playlists"),
        other => SharedString::from(other.to_string()),
    }
}

/// A per-visit seed, so the shuffled rows keep their order while the page is open.
fn seed() -> u64 {
    use std::hash::{BuildHasher as _, Hasher as _};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}
