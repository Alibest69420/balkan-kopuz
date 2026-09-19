use api::{ArtworkRef, LyricsView, NowPlaying, PlaylistCapability};
use gpui_kit::base::Disableable as _;
use gpui_kit::base::slider::{SliderEvent, SliderState};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    StyledImage as _, Subscription, Window, div, img, px, rems,
};
use store::{
    Chrome, ConfigModel, FavoritesModel, Link, Navigator, Notices, PlayerModel, Playhead, Query,
    QueueModel, Route, SourcesModel, actions,
};
use widgets::format::t;
use widgets::{dialogs, icons, playlist_picker, style};

use crate::background;
use crate::controls;

// `max-width: 640px`: the width the title, the bitrate line and the transport share.
fn column_width() -> gpui_kit::Rems {
    rems(40.)
}

// The `width: 50px` the Dioxus seek row reserves for each clock.
fn clock_width() -> gpui_kit::Rems {
    rems(3.125)
}

// `w-11 h-11`, `w-14 h-14`, `w-16 h-16`: a `Button`'s size is only ever `Pixels`.
const ACTION_SIZE: gpui_kit::Pixels = px(44.);
const STEP_SIZE: gpui_kit::Pixels = px(56.);
const PLAY_SIZE: gpui_kit::Pixels = px(64.);

/// Which of the two right-hand tabs is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Queue,
    Lyrics,
}

/// The whole-window player: the cover and its actions on the left, Up next / Lyrics on the right.
pub struct FullscreenPlayer {
    seek: Entity<SliderState>,
    volume: Entity<SliderState>,
    pub(crate) tab: Tab,
    pub(crate) lyrics: Option<Entity<Query<LyricsView>>>,
    lyrics_key: String,
    seeking: bool,
    // The level a mute returns to, so unmuting restores what the user had.
    premute: Option<f32>,
    _lyrics: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl FullscreenPlayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let seek = cx.new(|_| controls::fraction_state(0.001));
        let volume = cx.new(|_| controls::fraction_state(0.01));
        let player = PlayerModel::global(cx);
        let playhead = Playhead::global(cx);
        let queue = QueueModel::global(cx);
        let favorites = FavoritesModel::global(cx);
        let config = ConfigModel::global(cx);
        let sources = SourcesModel::global(cx);

        let subscriptions = vec![
            cx.observe_in(&player, window, |this, _, window, cx| this.sync(window, cx)),
            cx.observe_in(&playhead, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
            cx.observe(&queue, |_, _, cx| cx.notify()),
            cx.observe(&favorites, |_, _, cx| cx.notify()),
            cx.observe(&config, |_, _, cx| cx.notify()),
            cx.observe(&sources, |_, _, cx| cx.notify()),
            cx.subscribe_in(&seek, window, Self::on_seek),
            cx.subscribe_in(&volume, window, Self::on_volume),
        ];

        Self {
            seek,
            volume,
            tab: Tab::Queue,
            lyrics: None,
            lyrics_key: String::new(),
            seeking: false,
            premute: None,
            _lyrics: None,
            _subscriptions: subscriptions,
        }
    }

    /// Carry the daemon's position and volume into the sliders the user is not holding.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let player = PlayerModel::global(cx);
        let (position, duration, volume) = {
            let player = player.read(cx);
            (player.position_ms(), player.duration_ms(), player.volume())
        };
        if !self.seeking {
            let value = controls::fraction(position, duration);
            self.seek
                .update(cx, |seek, cx| seek.set_value(value, window, cx));
        }
        self.volume
            .update(cx, |slider, cx| slider.set_value(volume, window, cx));
        if self.tab == Tab::Lyrics {
            self.load_lyrics(cx);
        }
        cx.notify();
    }

    fn on_seek(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SliderEvent::Change(_) => self.seeking = true,
            SliderEvent::Release(value) => {
                self.seeking = false;
                let player = PlayerModel::global(cx);
                let Some(duration) = player.read(cx).duration_ms() else {
                    return;
                };
                let position = (value.end().clamp(0., 1.) * duration as f32) as u64;
                player.update(cx, |player, cx| player.seek(position, cx));
            }
        }
    }

    fn on_volume(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let SliderEvent::Change(value) = event {
            let level = value.end().clamp(0., 1.);
            self.premute = None;
            PlayerModel::global(cx).update(cx, |player, cx| player.set_volume(level, cx));
        }
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let player = PlayerModel::global(cx);
        let volume = player.read(cx).volume();
        let level = if volume > 0. {
            self.premute = Some(volume);
            0.
        } else {
            self.premute.take().unwrap_or(1.)
        };
        player.update(cx, |player, cx| player.set_volume(level, cx));
    }

    /// The daemon finds the words; this only says which track they are for.
    pub(crate) fn load_lyrics(&mut self, cx: &mut Context<Self>) {
        let player = PlayerModel::global(cx);
        let key = {
            let player = player.read(cx);
            match player.track() {
                Some(track) if !player.is_radio() => track.key.clone(),
                _ => String::new(),
            }
        };
        if key == self.lyrics_key {
            return;
        }
        self.lyrics_key = key.clone();
        if key.is_empty() {
            self.lyrics = None;
            self._lyrics = None;
            return;
        }
        let query = cx.new(|cx| {
            Query::new(
                &[],
                move |api| {
                    let key = key.clone();
                    async move { api.lyrics(key).await }
                },
                cx,
            )
        });
        self._lyrics = Some(cx.observe(&query, |_, _, cx| cx.notify()));
        self.lyrics = Some(query);
    }

    fn close(cx: &mut gpui_kit::App) {
        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_fullscreen(false, cx));
    }

    /// Stop recommending the track, then move on whether or not the source took it.
    fn dont_recommend(&mut self, key: String, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(move |api| async move { api.dont_recommend(key).await });
        cx.spawn(async move |_, cx| {
            let result = call.await;
            cx.update(|cx| {
                if let Err(error) = result {
                    Notices::error(error.to_string(), cx);
                }
                PlayerModel::global(cx).update(cx, |player, cx| player.next(cx));
            });
        })
        .detach();
    }

    fn title_block(&self, track: &NowPlaying, cx: &mut Context<Self>) -> impl IntoElement {
        let queue = QueueModel::global(cx);
        let album_id = queue
            .read(cx)
            .current()
            .filter(|row| row.key == track.key)
            .map(|row| row.album_id.clone())
            .unwrap_or_default();
        let artist = track.artist.clone();
        let album = SharedString::from(track.album.clone());

        v_flex()
            .flex_1()
            .min_w_0()
            .items_start()
            .child(
                div()
                    // `text-[28px]`, which is off Tailwind's scale.
                    .text_size(rems(1.75))
                    .font_weight(FontWeight::SEMIBOLD)
                    .w_full()
                    .mb_1()
                    .line_clamp(2)
                    .child(SharedString::from(track.title.clone())),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .id("fs-artist")
                            .max_w_full()
                            .text_xl()
                            .font_weight(FontWeight::MEDIUM)
                            .line_clamp(2)
                            .text_color(style::white(0.7, cx))
                            .hover(|link| link.text_color(cx.theme().foreground).underline())
                            .child(SharedString::from(artist.clone()))
                            .on_click(move |_, _, cx| {
                                if artist.is_empty() {
                                    return;
                                }
                                Self::close(cx);
                                Navigator::navigate(
                                    Route::Artist {
                                        name: artist.clone().into(),
                                        channel: None,
                                    },
                                    cx,
                                );
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(style::white(style::TEXT_DIM, cx))
                            .child("•"),
                    )
                    .child(
                        div()
                            .id("fs-album")
                            .max_w_full()
                            .text_lg()
                            .line_clamp(2)
                            .text_color(style::white(0.5, cx))
                            .hover(|link| link.text_color(style::white(0.8, cx)).underline())
                            .child(album)
                            .on_click(move |_, _, cx| {
                                if album_id.is_empty() {
                                    return;
                                }
                                Self::close(cx);
                                Navigator::navigate(Route::Album(album_id.clone().into()), cx);
                            }),
                    ),
            )
    }

    /// `w-11 h-11 rounded-full bg-white/10`: the shape every button beside the title takes.
    fn round_action(id: &'static str, foreground: gpui_kit::Hsla, cx: &App) -> Button {
        Button::new(id)
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(style::white(style::HOVER, cx))
                    .foreground(foreground)
                    .hover(style::white(0.15, cx))
                    .active(style::white(style::ACTIVE, cx)),
            )
            .rounded(px(999.))
            .with_size(ACTION_SIZE)
    }

    fn track_actions(&self, track: &NowPlaying, cx: &mut Context<Self>) -> impl IntoElement {
        let sources = SourcesModel::global(cx);
        let capabilities = sources.read(cx).capabilities();
        let favorite = FavoritesModel::global(cx).read(cx).is_favorite(&track.key);
        let key = track.key.clone();

        let liked = cx.theme().danger;
        let resting = style::white(0.7, cx);

        h_flex()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .child(
                Self::round_action("fs-favorite", if favorite { liked } else { resting }, cx)
                    .icon(icons::icon(icons::HEART))
                    .tooltip(if favorite {
                        t("remove_from_favorites")
                    } else {
                        t("add_to_favorites")
                    })
                    .on_click({
                        let key = key.clone();
                        move |_, _, cx| {
                            FavoritesModel::global(cx)
                                .update(cx, |favorites, cx| favorites.toggle(key.clone(), cx))
                        }
                    }),
            )
            .when(capabilities.dont_recommend, |row| {
                row.child(
                    Self::round_action("fs-dont-recommend", style::white(0.5, cx), cx)
                        .icon(icons::icon(icons::THUMBS_DOWN))
                        .tooltip(t("dont_recommend"))
                        .disabled(favorite)
                        .on_click(cx.listener({
                            let key = key.clone();
                            move |this, _, _, cx| this.dont_recommend(key.clone(), cx)
                        })),
                )
            })
            .child(
                Self::round_action("fs-actions", resting, cx)
                    .icon(icons::icon(icons::MORE))
                    .dropdown_menu(move |menu, _, _| track_menu(menu, &key, capabilities)),
            )
    }

    fn transport(&self, playing: bool, seekable: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let player = PlayerModel::global(cx);
        let (position, duration, radio, loading, shuffle, loop_mode, volume) = {
            let player = player.read(cx);
            (
                player.position_ms(),
                player.duration_ms(),
                player.is_radio(),
                player.is_loading(),
                player.shuffle(),
                player.loop_mode(),
                player.volume(),
            )
        };
        let volume_icon = if volume <= 0. {
            icons::VOLUME_MUTED
        } else if volume < 0.5 {
            icons::VOLUME_LOW
        } else {
            icons::VOLUME
        };

        v_flex()
            .w_full()
            .max_w(column_width())
            .items_center()
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .mb_3()
                    .items_center()
                    .text_xs()
                    .text_color(style::white(0.7, cx))
                    .child(
                        div()
                            .w(clock_width())
                            .child(controls::clock(position, radio)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Slider::new(&self.seek).disabled(radio || loading || !seekable)),
                    )
                    .child(
                        div()
                            .w(clock_width())
                            .text_right()
                            .child(controls::clock(duration.unwrap_or(0), radio)),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .mb_3()
                    .items_center()
                    .justify_between()
                    .child(controls::shuffle_button(shuffle).with_size(ACTION_SIZE))
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .child(
                                controls::previous_button("fs-previous")
                                    .with_size(STEP_SIZE)
                                    .rounded(px(999.)),
                            )
                            .child(controls::play_button("fs-play", playing).with_size(PLAY_SIZE))
                            .child(
                                controls::next_button("fs-next")
                                    .with_size(STEP_SIZE)
                                    .rounded(px(999.)),
                            ),
                    )
                    .child(controls::repeat_button(loop_mode).with_size(ACTION_SIZE)),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_5()
                    .items_center()
                    .child(
                        Button::new("fs-mute")
                            .ghost()
                            .icon(icons::icon(volume_icon))
                            .tooltip(t("volume"))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mute(cx))),
                    )
                    .child(div().flex_1().child(Slider::new(&self.volume))),
            )
    }

    /// The cover grows to fill what the stage leaves it, the way the Dioxus `flex-1` wrapper does.
    fn cover(&self, artwork: Option<&ArtworkRef>, cx: &Context<Self>) -> impl IntoElement {
        let source = artwork.map(|artwork| store::artwork_source(artwork, true));
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .mb_6()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .max_w_full()
                    .aspect_ratio(1.)
                    .overflow_hidden()
                    .rounded(style::radius_xl())
                    .shadow_lg()
                    .bg(style::black(0.3, cx))
                    .map(|frame| match source {
                        Some(source) => {
                            frame.child(img(source).size_full().object_fit(ObjectFit::Cover))
                        }
                        None => frame.child(
                            icons::icon(icons::MUSIC)
                                .size(px(48.))
                                .text_color(style::white(style::TEXT_FAINT, cx)),
                        ),
                    }),
            )
    }

    fn stage(&mut self, full_width: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let player = PlayerModel::global(cx);
        let (track, playing) = {
            let player = player.read(cx);
            (player.track().cloned(), player.is_playing())
        };
        let config = ConfigModel::global(cx);
        let use_player_bar = config.read(cx).config().fullscreen_use_player_bar;
        let cover = self.cover(track.as_ref().and_then(|track| track.artwork.as_ref()), cx);

        v_flex()
            .h_full()
            .min_w_0()
            .relative()
            .flex_shrink_0()
            .overflow_hidden()
            .items_center()
            .justify_center()
            .p_10()
            .pt_16()
            .map(|column| {
                if full_width {
                    column.w_full()
                } else {
                    column.w_1_2()
                }
            })
            .child(
                div().absolute().top_8().left_8().child(
                    Button::new("fs-close")
                        .ghost()
                        .icon(icons::icon(icons::CHEVRON_DOWN).size(px(24.)))
                        .tooltip(t("close"))
                        .on_click(|_, _, cx| Self::close(cx)),
                ),
            )
            .child(cover)
            .when_some(track, |column, track| {
                let bitrate = track.bitrate;
                let seekable = track.seekable;
                column
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(column_width())
                            .child(
                                h_flex()
                                    .w_full()
                                    .mb_1()
                                    .gap_4()
                                    .items_center()
                                    .child(self.title_block(&track, cx))
                                    .child(self.track_actions(&track, cx)),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .h_5()
                                    .mb_3()
                                    .gap_4()
                                    .items_center()
                                    .text_color(style::white(style::TEXT_SECONDARY, cx))
                                    .when(bitrate > 0, |row| {
                                        row.child(
                                            div()
                                                .text_size(style::text_10())
                                                // TODO(i18n): the Dioxus app wrote the unit in English too.
                                                .child(SharedString::from(format!(
                                                    "{bitrate} kbps"
                                                ))),
                                        )
                                    }),
                            ),
                    )
                    .when(!use_player_bar, |column| {
                        column.child(self.transport(playing, seekable, cx))
                    })
            })
    }
}

impl Render for FullscreenPlayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = ConfigModel::global(cx);
        let collapsed = config.read(cx).config().fullscreen_tabs_collapsed;
        let player = PlayerModel::global(cx);
        let artwork = player
            .read(cx)
            .track()
            .and_then(|track| track.artwork.clone());

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(background::backdrop(artwork.as_ref(), cx))
            .child(
                h_flex()
                    .size_full()
                    .items_stretch()
                    .child(self.stage(collapsed, cx))
                    .when(!collapsed, |row| row.child(self.panel(cx))),
            )
            .when(collapsed, |view| view.child(self.expand_button(cx)))
    }
}

fn track_menu(menu: PopupMenu, key: &str, capabilities: api::SourceCapabilities) -> PopupMenu {
    let key = key.to_string();
    let mut menu = menu
        .item(
            PopupMenuItem::new(t("play_next"))
                .icon(icons::icon(icons::QUEUE_NEXT))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::play_next(vec![key.clone()], cx)
                }),
        )
        .item(
            PopupMenuItem::new(t("add_to_queue"))
                .icon(icons::icon(icons::QUEUE_ADD))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::append(vec![key.clone()], cx)
                }),
        );
    if capabilities.playlists != PlaylistCapability::None {
        menu = menu.item(
            PopupMenuItem::new(t("add_to_playlist"))
                .icon(icons::icon(icons::PLUS))
                .on_click({
                    let key = key.clone();
                    move |_, window, cx| playlist_picker::open(vec![key.clone()], window, cx)
                }),
        );
    }
    menu = menu.item(
        PopupMenuItem::new(t("share_musicbrainz"))
            .icon(icons::icon(icons::SHARE))
            .on_click({
                let key = key.clone();
                move |_, _, cx| actions::share_track(key.clone(), cx)
            }),
    );
    if capabilities.track_radio {
        menu = menu.item(
            PopupMenuItem::new(t("start_radio"))
                .icon(icons::icon(icons::RADIO))
                .on_click({
                    let key = key.clone();
                    move |_, _, cx| actions::start_track_radio(key.clone(), cx)
                }),
        );
    }
    menu.item(
        PopupMenuItem::new(t("view_metadata"))
            .icon(icons::icon(icons::EDIT))
            .on_click(move |_, window, cx| {
                dialogs::open_track_metadata(key.clone(), false, window, cx)
            }),
    )
}
