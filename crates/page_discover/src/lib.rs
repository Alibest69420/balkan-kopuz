//! The active source's browse catalog, and the catalog playlist and remote artist pages it opens.

mod shelves;

use api::{CatalogDetail, CatalogDetailRequest, CatalogItemKind};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Subscription, Task, Window, div,
    img, linear_color_stop, linear_gradient, px, rems,
};
use store::{Link, Route, SourcesModel, actions};
use widgets::format::t;
use widgets::{
    EmptyState, PageHeader, TrackActions, TrackList, TrackListEvent, TrackListOptions, icons,
    motion, style,
};

use crate::shelves::{Shelves, failure_text};

// The Dioxus sentinel's root margin: how close to the end a scroll asks for the next page.
const PAGE_MARGIN: Pixels = px(600.);
// `max-w-[1600px] mx-auto`: the catalog pages never run wider than this.
fn page_width() -> gpui_kit::Rems {
    rems(100.)
}

// The artist banner's `min-height: 360px`.
fn banner_height() -> gpui_kit::Rems {
    rems(22.5)
}

/// The active source's browse feed.
pub struct Discover {
    shelves: Entity<Shelves>,
    continuation: Option<String>,
    loading: bool,
    loading_more: bool,
    settled: bool,
    error: Option<SharedString>,
    scroll: ScrollHandle,
    _load: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Discover {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        let mut page = Self {
            shelves: cx.new(Shelves::new),
            continuation: None,
            loading: false,
            loading_more: false,
            settled: false,
            error: None,
            scroll: ScrollHandle::new(),
            _load: None,
            // Capabilities arrive with the source list, so the first load has to follow that answer.
            _subscriptions: vec![cx.observe(&sources, |this, _, cx| this.start(cx))],
        };
        page.start(cx);
        page
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.settled || !discover_supported(cx) {
            return;
        }
        self.load(None, cx);
    }

    fn load(&mut self, continuation: Option<String>, cx: &mut Context<Self>) {
        if continuation.is_some() {
            self.loading_more = true;
        } else {
            self.loading = true;
        }
        cx.notify();
        let call = Link::global(cx).call(move |api| async move { api.catalog(continuation).await });
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                this.loading_more = false;
                this.settled = true;
                match result {
                    Ok(page) => {
                        this.continuation = page.continuation;
                        this.error = None;
                        let shelves = page.shelves;
                        this.shelves.update(cx, |held, cx| held.extend(shelves, cx));
                    }
                    Err(error) => this.error = Some(failure_text(&error)),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Ask for the next page once the end of the feed is within reach.
    fn page_on_scroll(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.loading_more {
            return;
        }
        let Some(continuation) = self.continuation.clone() else {
            return;
        };
        let scrolled = -self.scroll.offset().y;
        if self.scroll.max_offset().y - scrolled > PAGE_MARGIN {
            return;
        }
        self.load(Some(continuation), cx);
    }
}

impl Render for Discover {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sources = SourcesModel::global(cx);
        let settled = sources.read(cx).is_loaded();
        if settled && !sources.read(cx).capabilities().discover {
            return EmptyState::new(t("discover"))
                .detail(t("discover_unsupported"))
                .icon(icons::COMPASS)
                .into_any_element();
        }
        let waiting = !settled || (self.loading && self.shelves.read(cx).is_empty());
        div()
            .id("discover")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.page_on_scroll(cx)))
            .p_10()
            .child(
                v_flex()
                    .w_full()
                    .max_w(page_width())
                    .mx_auto()
                    .child(
                        div()
                            .mb_2()
                            .text_size(rems(2.25))
                            .font_weight(gpui_kit::FontWeight::BLACK)
                            .child(t("discover")),
                    )
                    .child(
                        div()
                            .w_full()
                            .h_px()
                            .mb_8()
                            .bg(style::white(style::BORDER, cx)),
                    )
                    .when(waiting, |body| body.child(spinner_row()))
                    .when_some(self.error.clone(), |body, error| {
                        body.child(
                            div()
                                .py_12()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(error),
                        )
                    })
                    .child(self.shelves.clone())
                    .when(self.loading_more, |body| {
                        body.child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_center()
                                .gap_3()
                                .py_6()
                                .text_xs()
                                .text_color(style::white(0.5, cx))
                                .child(Spinner::new().small())
                                .child(t("discover_more_loading")),
                        )
                    }),
            )
            .into_any_element()
    }
}

/// One catalog playlist opened from the feed, read-only: a browsed list never becomes a saved one.
pub struct DiscoverPlaylist {
    id: SharedString,
    title: SharedString,
    list: Entity<TrackList>,
    loading: bool,
    error: Option<SharedString>,
    // Bumped per load; a slow answer for one id must not land after the user moved to another.
    generation: u64,
    _load: Option<Task<()>>,
    _subscription: Subscription,
}

impl DiscoverPlaylist {
    pub fn new(
        id: SharedString,
        title: SharedString,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = cx.new(|cx| {
            TrackList::new(cx)
                .with_options(TrackListOptions {
                    selectable: false,
                    ..TrackListOptions::default()
                })
                .with_actions(TrackActions::default())
        });
        let subscription = cx.subscribe(&list, |_, list, event: &TrackListEvent, cx| {
            if let TrackListEvent::Play(index) = event {
                let keys = list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
        });
        let mut page = Self {
            id,
            title,
            list,
            loading: false,
            error: None,
            generation: 0,
            _load: None,
            _subscription: subscription,
        };
        page.load(cx);
        page
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let id = self.id.to_string();
        self.loading = true;
        self.error = None;
        cx.notify();
        let call = Link::global(cx).call(move |api| async move {
            let request = CatalogDetailRequest {
                kind: CatalogItemKind::Playlist,
                id: id.clone(),
                continuation: None,
            };
            match api.catalog_detail(request).await {
                Ok(detail) => Ok(detail),
                // The route carries no kind, so an album opened here is a second try, not an id sniff.
                Err(_) => {
                    api.catalog_detail(CatalogDetailRequest {
                        kind: CatalogItemKind::Album,
                        id,
                        continuation: None,
                    })
                    .await
                }
            }
        });
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(detail) => {
                        this.list
                            .update(cx, |list, cx| list.set_rows(detail.tracks, cx));
                    }
                    Err(error) => this.error = Some(failure_text(&error)),
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Render for DiscoverPlaylist {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = !self.loading && self.error.is_none() && self.list.read(cx).rows().is_empty();
        v_flex()
            .size_full()
            .p_8()
            .child(PageHeader::new(self.title.clone()).back(Route::Discover))
            .map(|page| {
                if self.loading {
                    page.child(spinner_row())
                } else if let Some(error) = self.error.clone() {
                    page.child(div().text_sm().text_color(cx.theme().danger).child(error))
                } else if empty {
                    page.child(EmptyState::new(t("playlist_not_found")).icon(icons::LIST))
                } else {
                    page.child(div().flex_1().min_h_0().child(self.list.clone()))
                }
            })
    }
}

/// A remote artist profile: the source's own page for an artist the library does not hold.
pub struct DiscoverArtist {
    id: SharedString,
    name: SharedString,
    detail: Option<CatalogDetail>,
    shelves: Entity<Shelves>,
    loading: bool,
    error: Option<SharedString>,
    generation: u64,
    shuffle_hovered: bool,
    scroll: ScrollHandle,
    _load: Option<Task<()>>,
}

impl DiscoverArtist {
    pub fn new(
        id: SharedString,
        name: SharedString,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut page = Self {
            id,
            name,
            detail: None,
            shelves: cx.new(Shelves::new),
            loading: false,
            error: None,
            generation: 0,
            shuffle_hovered: false,
            scroll: ScrollHandle::new(),
            _load: None,
        };
        page.load(cx);
        page
    }

    /// The channel id wins; a caller that only has a name passes that and the daemon resolves it.
    fn reference(&self) -> Option<String> {
        [self.id.trim(), self.name.trim()]
            .into_iter()
            .find(|candidate| !candidate.is_empty())
            .map(str::to_string)
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(reference) = self.reference() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        cx.notify();
        let call = Link::global(cx).call(move |api| async move {
            api.catalog_detail(CatalogDetailRequest {
                kind: CatalogItemKind::Artist,
                id: reference,
                continuation: None,
            })
            .await
        });
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(detail) => {
                        let shelves = detail.shelves.clone();
                        this.shelves.update(cx, |held, cx| held.set(shelves, cx));
                        this.detail = Some(detail);
                    }
                    Err(error) => this.error = Some(failure_text(&error)),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn banner(
        &self,
        detail: &CatalogDetail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // `hover:scale-105` on the inverted pill: GPUI has no transform, so the wash carries it.
        let shuffle_target = if self.shuffle_hovered {
            style::white(0.9, cx)
        } else {
            cx.theme().foreground
        };
        let shuffle_color = motion::color("artist-shuffle-bg", shuffle_target, window, cx);
        div()
            .relative()
            .w_full()
            .h(banner_height())
            .overflow_hidden()
            .bg(cx.theme().secondary)
            .when_some(detail.artwork.as_ref(), |frame, artwork| {
                frame.child(
                    img(store::artwork_source(artwork, true))
                        .absolute()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
            })
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(style::black(0.2, cx), 0.),
                linear_color_stop(style::black(0.95, cx), 1.),
            )))
            .child(
                v_flex()
                    .absolute()
                    .size_full()
                    .justify_end()
                    .gap_4()
                    .px_10()
                    .pt_16()
                    .pb_10()
                    .child(
                        div()
                            .text_size(rems(3.75))
                            .font_weight(gpui_kit::FontWeight::BLACK)
                            .child(detail.title.clone()),
                    )
                    .when_some(detail.subtitle.clone(), |banner, subtitle| {
                        banner.child(
                            div()
                                .text_sm()
                                .text_color(style::white(0.7, cx))
                                .child(subtitle),
                        )
                    })
                    .when_some(detail.description.clone(), |banner, description| {
                        banner.child(
                            div()
                                .max_w(rems(48.))
                                .text_sm()
                                .line_clamp(3)
                                .text_color(style::white(style::TEXT_SECONDARY, cx))
                                .child(description),
                        )
                    })
                    .when_some(detail.playback_id.clone(), |banner, id| {
                        banner.child(
                            h_flex().mt_2().child(
                                // `bg-white text-black`: the banner's one inverted control.
                                Button::new("artist-shuffle")
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(shuffle_color)
                                            .foreground(cx.theme().background)
                                            .hover(shuffle_color)
                                            .active(style::white(0.8, cx)),
                                    )
                                    .rounded(px(999.))
                                    .icon(icons::icon(icons::SHUFFLE).xsmall())
                                    .label(t("shuffle"))
                                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                        this.shuffle_hovered = *hovered;
                                        cx.notify();
                                    }))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let id = id.clone();
                                        this.shelves.update(cx, |shelves, cx| {
                                            shelves.play(CatalogItemKind::Playlist, id, cx)
                                        });
                                    })),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl Render for DiscoverArtist {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.reference().is_none() {
            return EmptyState::new(t("artist_none_selected"))
                .icon(icons::MIC)
                .into_any_element();
        }
        let banner = self
            .detail
            .clone()
            .map(|detail| self.banner(&detail, window, cx));
        div()
            .id("discover-artist")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                v_flex()
                    .w_full()
                    .max_w(page_width())
                    .mx_auto()
                    .px_10()
                    .pt_6()
                    .pb_10()
                    .gap_8()
                    .child(PageHeader::new(self.name.clone()).back(Route::Discover))
                    .when(self.loading, |page| page.child(spinner_row()))
                    .when_some(self.error.clone(), |page, error| {
                        page.child(div().text_sm().text_color(cx.theme().danger).child(error))
                    })
                    .children(banner)
                    .child(self.shelves.clone()),
            )
            .into_any_element()
    }
}

fn discover_supported(cx: &mut Context<impl Render>) -> bool {
    SourcesModel::global(cx).read(cx).capabilities().discover
}

fn spinner_row() -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_center()
        .py_12()
        .child(Spinner::new().large())
}
