//! The sheet: the bar grown to fill the page, with the cover travelling out of its bar slot.

use std::path::PathBuf;

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, Hsla, ImageSource, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled,
    StyledImage as _, Window, div, img, point, px, relative, rems,
};
use store::layout::{Backdrop, Side};
use store::{Chrome, ConfigModel, FavoritesModel, Navigator, Route, SourcesModel};
use widgets::format::t;
use widgets::{Cover, icons, menus, motion, style};

use crate::NowPlaying;
use crate::meta::{cover_size, playing_from, track_target};
use crate::transport::Metrics;

// The column the title, the clocks and the transport share, and the panel beside them.
const COLUMN: f32 = 40.;
const PANEL: f32 = 0.38;
const ACTION: Pixels = px(44.);
const ACTION_GLYPH: Pixels = px(18.);
const CLOSE: Pixels = px(40.);
const CLOSE_GLYPH: Pixels = px(22.);
// The hero cover never grows past this, however tall the window is.
const HERO_MAX: Pixels = px(420.);

impl NowPlaying {
    pub(crate) fn render_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = store::layout(cx).sheet.clone();
        let track = self.track(cx);
        let artwork = track.as_ref().and_then(|track| track.artwork.clone());
        let entered = motion::enter("sheet-enter", motion::SLOW, window, cx);
        let owner = cx.entity().downgrade();
        let showing = self.tab;

        let stage = self.stage(window, cx);
        let panel = v_flex()
            .h_full()
            .w(relative(PANEL))
            .flex_shrink_0()
            .border_l_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .bg(style::black(0.45, cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .justify_center()
                    .px_4()
                    .py_3()
                    .child(self.tab_strip(showing, true, window, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.tab_body(showing)),
            );

        div()
            .id("sheet")
            .size_full()
            .relative()
            .overflow_hidden()
            .opacity(entered)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_prepaint(move |bounds, _, cx| {
                owner
                    .update(cx, |this, cx| {
                        if this.root != Some(bounds) {
                            this.root = Some(bounds);
                            cx.notify();
                        }
                    })
                    .ok();
            })
            .children(self.backdrop(layout.backdrop, artwork.as_ref(), window, cx))
            .child(
                h_flex()
                    .size_full()
                    .items_stretch()
                    .map(|row| match layout.side {
                        Side::Left => row.child(panel).child(stage),
                        Side::Right => row.child(stage).child(panel),
                    }),
            )
            .children(self.flying_cover(window, cx))
            .into_any_element()
    }

    /// The left half: the cover's landing place, the title, the transport and the clocks.
    fn stage(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let track = self.track(cx);
        let owner = cx.entity().downgrade();
        let close = self.control_glyph(
            "sheet-close",
            style::white(style::TEXT_STRONG, cx),
            false,
            window,
            cx,
        );

        v_flex()
            .relative()
            .h_full()
            .flex_1()
            .min_w_0()
            .items_center()
            .justify_center()
            .gap_6()
            .p_10()
            .child(
                // Over a bright cover a bare glyph disappears, so the slot sits on a scrim of its own.
                div()
                    .absolute()
                    .top_6()
                    .left_6()
                    .rounded(CLOSE / 2.)
                    .bg(style::black(0.35, cx))
                    .child(
                        self.control_slot("sheet-close", CLOSE, window, cx).child(
                            Button::new("sheet-close")
                                .ghost()
                                .with_size(CLOSE_GLYPH * (4. / 3.))
                                .size(CLOSE)
                                .rounded(CLOSE / 2.)
                                .icon(icons::icon(icons::CHEVRON_DOWN))
                                .custom(style::bare_button(close, cx))
                                .tooltip(t("close"))
                                .on_click(|_, _, cx| {
                                    Chrome::global(cx)
                                        .update(cx, |chrome, cx| chrome.set_sheet(false, cx))
                                }),
                        ),
                    ),
            )
            .child(
                // The hero cover is drawn by the flying layer; this only says where it lands.
                div()
                    .id("sheet-hero-slot")
                    .flex_shrink_0()
                    .size(HERO_MAX)
                    .max_h(relative(0.5))
                    .on_prepaint(move |bounds, _, cx| {
                        owner
                            .update(cx, |this, cx| {
                                if this.hero != Some(bounds) {
                                    this.hero = Some(bounds);
                                    cx.notify();
                                }
                            })
                            .ok();
                    }),
            )
            .children(track.map(|track| self.title_block(&track, window, cx)))
            .child(
                v_flex()
                    .w_full()
                    .max_w(rems(COLUMN))
                    .items_center()
                    .gap_4()
                    .child(self.seek_row(window, cx))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_4()
                            .child(self.transport(Metrics::HERO, true, window, cx))
                            .child(self.transport_overflow(ACTION, ACTION_GLYPH, cx)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .max_w(rems(COLUMN / 2.))
                            .child(self.volume_control(true, window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// Title, artist, album and where the queue came from, with the track's own marks beside them.
    fn title_block(
        &self,
        track: &api::NowPlaying,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let capabilities = SourcesModel::global(cx).read(cx).capabilities();
        let favorite = FavoritesModel::global(cx).read(cx).is_favorite(&track.key);
        let key = track.key.clone();
        let artist = track.artist.clone();
        let album = SharedString::from(track.album.clone());
        let heart = motion::color(
            "sheet-heart",
            if favorite {
                cx.theme().danger
            } else {
                style::white(0.7, cx)
            },
            window,
            cx,
        );
        let origin = playing_from(cx);

        v_flex()
            .w_full()
            .max_w(rems(COLUMN))
            .items_center()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(rems(1.75))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_clamp(2)
                    .child(SharedString::from(track.title.clone())),
            )
            .child(
                h_flex()
                    .max_w_full()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .text_lg()
                    .text_color(style::white(0.7, cx))
                    .child(
                        div()
                            .id("sheet-artist")
                            .truncate()
                            .cursor_pointer()
                            .hover(|link| link.underline())
                            .child(SharedString::from(artist.clone()))
                            .on_click(move |_, _, cx| {
                                if artist.is_empty() {
                                    return;
                                }
                                Chrome::global(cx)
                                    .update(cx, |chrome, cx| chrome.set_sheet(false, cx));
                                Navigator::navigate(
                                    Route::Artist {
                                        name: artist.clone().into(),
                                        channel: None,
                                    },
                                    cx,
                                );
                            }),
                    )
                    .when(!album.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_color(style::white(style::TEXT_DIM, cx))
                                .child("•"),
                        )
                        .child(div().truncate().child(album.clone()))
                    }),
            )
            .children(origin.map(|(label, route)| {
                h_flex()
                    .id("sheet-playing-from")
                    .gap_1()
                    .max_w_full()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .hover(|line| line.underline())
                    .on_click(move |_, _, cx| {
                        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_sheet(false, cx));
                        Navigator::navigate(route.clone(), cx);
                    })
                    .child(div().flex_shrink_0().child(t("playing_from")))
                    .child(div().truncate().child(label))
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pt_2()
                    .child(
                        round("sheet-favorite", icons::HEART, heart, cx)
                            .tooltip(if favorite {
                                t("remove_from_favorites")
                            } else {
                                t("add_to_favorites")
                            })
                            .on_click({
                                let key = key.clone();
                                move |_, _, cx| {
                                    FavoritesModel::global(cx).update(cx, |favorites, cx| {
                                        favorites.toggle(key.clone(), cx)
                                    })
                                }
                            }),
                    )
                    .when(capabilities.dont_recommend, |row| {
                        row.child(
                            round(
                                "sheet-dont-recommend",
                                icons::THUMBS_DOWN,
                                style::white(0.5, cx),
                                cx,
                            )
                            .tooltip(t("dont_recommend"))
                            .disabled(favorite)
                            .on_click(cx.listener({
                                let key = key.clone();
                                move |this, _, _, cx| this.dont_recommend(key.clone(), cx)
                            })),
                        )
                    })
                    .child(self.share_button(ACTION, ACTION_GLYPH, window, cx))
                    .child(
                        round("sheet-actions", icons::MORE, style::white(0.7, cx), cx)
                            .dropdown_menu(menus::builder(track_target(track))),
                    ),
            )
    }

    /// The cover itself, drawn once, travelling between the bar's slot and the hero's.
    fn flying_cover(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let root = self.root?;
        let artwork = self.track(cx).and_then(|track| track.artwork);
        let bar = crate::cover_slot(cx);
        // Before the hero has been laid out there is nowhere to fly to, so it starts in the bar.
        let (origin, size) = match self.hero {
            Some(hero) => (hero.origin, hero.size.width.min(hero.size.height)),
            None => match bar {
                Some(bar) => (bar.origin, bar.size.width),
                None => return None,
            },
        };
        let local = point(origin.x - root.origin.x, origin.y - root.origin.y);
        let left = motion::length("sheet-cover-x", local.x, motion::SLOW, window, cx);
        let top = motion::length("sheet-cover-y", local.y, motion::SLOW, window, cx);
        let side = motion::length("sheet-cover-size", size, motion::SLOW, window, cx);
        let radius = style::to_px(style::radius_xl(), window)
            * (f32::from(side) / f32::from(cover_size(cx)).max(1.)).clamp(0.4, 1.);

        Some(
            div()
                .absolute()
                .left(left)
                .top(top)
                .size(side)
                .shadow_lg()
                .child(
                    Cover::new(artwork.as_ref())
                        .size(side)
                        .rounded(radius)
                        .hq(true),
                ),
        )
    }

    /// What is behind the sheet: the cover itself, the colour taken from it, or nothing at all.
    fn backdrop(
        &self,
        backdrop: Backdrop,
        artwork: Option<&api::ArtworkRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match backdrop {
            Backdrop::Solid => None,
            Backdrop::Palette => {
                let wash = theme::wash_for(theme::Surface::Sheet, window, cx)?;
                let tinted = motion::color_over(
                    "sheet-wash",
                    wash.opacity(0.35),
                    theme::ACCENT_TWEEN,
                    window,
                    cx,
                );
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(cx.theme().background)
                        .child(div().absolute().top_0().left_0().size_full().bg(tinted))
                        .into_any_element(),
                )
            }
            Backdrop::Artwork => {
                let config = ConfigModel::global(cx);
                let config = config.read(cx);
                let source: ImageSource = if !config.config().custom_background_path.is_empty() {
                    PathBuf::from(&config.config().custom_background_path).into()
                } else {
                    store::artwork_source(artwork?, true)
                };
                // The sheet is read over, so the art never gets to be as bright as a wallpaper.
                let darkening = config.config().cover_art_darkening.max(72);
                let blur = config.config().cover_art_blur;
                Some(paint(source, darkening, blur, cx))
            }
        }
    }
}

/// `w-11 h-11 rounded-full bg-white/10`: the shape every button beside the title takes.
fn round(id: &'static str, glyph: &'static str, tint: Hsla, cx: &gpui_kit::App) -> Button {
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(style::rest(style::white(style::HOVER, cx)))
                .foreground(tint)
                .hover(style::white(0.15, cx))
                .active(style::white(style::ACTIVE, cx)),
        )
        .rounded(px(999.))
        .with_size(ACTION)
        .icon(icons::icon(glyph))
}

/// The art behind the sheet, blurred and darkened.
fn paint(source: ImageSource, darkening: u8, blur: u8, cx: &gpui_kit::App) -> AnyElement {
    let scrim = f32::from(darkening.min(95)) / 100.;
    // Overscanned by the blur radius, so the softened edge never shows as a seam.
    let scale = 1. + f32::from(blur.min(100)) * 0.004;
    let inset = (1. - scale) / 2.;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .overflow_hidden()
        .bg(cx.theme().background)
        .child(
            img(store::blurred_backdrop(source, blur))
                .absolute()
                .top(relative(inset))
                .left(relative(inset))
                .w(relative(scale))
                .h(relative(scale))
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(cx.theme().background.opacity(scrim)),
        )
        .into_any_element()
}
