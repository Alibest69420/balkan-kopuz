//! One now-playing entity, drawn at three sizes: the bar, the sheet that grows out of it, and the mini window.

mod bar;
mod buffer;
mod clock;
mod devices;
mod lyrics_view;
mod meta;
mod mini;
mod panel;
mod queue_view;
mod seek;
mod settings;
mod sheet;
mod transport;
mod volume;
mod window;

use gpui_kit::base::slider::{SliderEvent, SliderState};
use gpui_kit::{
    App, AppContext as _, Bounds, Context, ElementId, Entity, Global, IntoElement, Pixels, Render,
    Subscription, Task, Window,
};
use store::layout::SheetTab;
use store::{Chrome, ConfigModel, FavoritesModel, PlayerModel, Playhead, QueueModel, SourcesModel};

pub use bar::{bar_height, utility_metrics};
pub use lyrics_view::LyricsView;
pub use queue_view::QueueView;
pub use settings::register as register_settings;
pub use window::{MiniWindow, close_mini, is_open as mini_is_open, open_mini, toggle_mini};

/// A thousandth of the track is finer than one pixel of bar in any window this app opens in.
const SEEK_STEP: f32 = 0.001;
const VOLUME_STEP: f32 = 0.001;

/// Which of the sizes one entity is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// The bar across the bottom, always there.
    Bar,
    SheetBar,
    /// The sheet the bar grows into, over the page.
    Sheet,
    /// The same queue and lyrics, docked beside the content instead.
    Panel,
    /// The ~380×148 window that floats over everything else.
    Mini,
}

/// Where the bar's cover sits on screen, so the sheet's hero can travel out of it.
#[derive(Default)]
struct CoverSlot(Option<Bounds<Pixels>>);

impl Global for CoverSlot {}

fn remember_cover_slot(bounds: Bounds<Pixels>, cx: &mut App) {
    cx.default_global::<CoverSlot>().0 = Some(bounds);
}

fn cover_slot(cx: &App) -> Option<Bounds<Pixels>> {
    cx.try_global::<CoverSlot>().and_then(|slot| slot.0)
}

/// The player, in whichever size it was built for. Every part is written once and drawn by each.
pub struct NowPlaying {
    presentation: Presentation,
    sheet_focus: sheet::ModalFocus,
    seek: Entity<SliderState>,
    volume: Entity<SliderState>,
    /// The fraction under the pointer while a drag runs; the release commits it.
    scrubbing: Option<f32>,
    /// Whether the pointer is holding the level, which the daemon's own echo must not undo.
    volume_held: bool,
    /// How wide the bar's title column last measured, which is what a title has to fit.
    title_room: Pixels,
    /// What the pointer is hovering the seek bar at, and how far along the track that is.
    preview: Option<(f32, Pixels)>,
    /// The right-hand clock counts down rather than naming the length.
    remaining: bool,
    /// Wheel travel that has not yet added up to a volume step.
    wheel: f32,
    hovered: Option<&'static str>,
    tab: SheetTab,
    /// What the docked column has been dragged to, once it has been.
    panel_width: Option<Pixels>,
    /// Where the sheet and its hero cover landed last frame, so the cover can travel into it.
    root: Option<Bounds<Pixels>>,
    hero: Option<Bounds<Pixels>>,
    devices_open: bool,
    devices: Vec<api::ExternalDevice>,
    queue: Option<Entity<QueueView>>,
    lyrics: Option<Entity<LyricsView>>,
    sheet_bar: Option<Entity<NowPlaying>>,
    _devices_fetch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl NowPlaying {
    /// The bar across the bottom of the window.
    pub fn bar(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(Presentation::Bar, window, cx)
    }

    /// The sheet that grows out of the bar, over the page.
    pub fn sheet(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(Presentation::Sheet, window, cx)
    }

    /// The docked queue-and-lyrics column beside the content.
    pub fn panel(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(Presentation::Panel, window, cx)
    }

    /// The compact surface the mini window draws.
    pub fn mini(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(Presentation::Mini, window, cx)
    }

    fn new(presentation: Presentation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let level = PlayerModel::global(cx).read(cx).volume();
        let seek = cx.new(|_| fraction_state(SEEK_STEP, 0.));
        let volume = cx.new(|_| fraction_state(VOLUME_STEP, volume::to_ui(level)));
        let holds_tabs = matches!(presentation, Presentation::Sheet | Presentation::Panel);

        let subscriptions = vec![
            cx.subscribe_in(&seek, window, Self::on_seek),
            cx.subscribe_in(&volume, window, Self::on_volume),
            cx.observe_in(&Playhead::global(cx), window, |this, _, window, cx| {
                this.sync(window, cx);
            }),
            cx.observe_in(&PlayerModel::global(cx), window, |this, _, window, cx| {
                this.sync(window, cx);
            }),
            cx.observe_in(&Chrome::global(cx), window, |this, chrome, window, cx| {
                if this.presentation == Presentation::Sheet {
                    this.sheet_focus
                        .set_open(chrome.read(cx).is_sheet_open(), window, cx);
                }
                cx.notify();
            }),
            cx.observe(&QueueModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&FavoritesModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&ConfigModel::global(cx), |_, _, cx| cx.notify()),
            cx.observe(&store::LayoutModel::global(cx), |this, _, cx| {
                this.tab = store::layout(cx).sheet.tab;
                cx.notify();
            }),
            cx.observe(&SourcesModel::global(cx), |this, _, cx| {
                this.forget_devices(cx);
                cx.notify();
            }),
        ];

        Self {
            presentation,
            sheet_focus: sheet::ModalFocus::new(cx),
            seek,
            volume,
            scrubbing: None,
            volume_held: false,
            title_room: Pixels::ZERO,
            preview: None,
            remaining: false,
            wheel: 0.,
            hovered: None,
            tab: store::layout(cx).sheet.tab,
            panel_width: None,
            root: None,
            hero: None,
            devices_open: false,
            devices: Vec::new(),
            queue: holds_tabs.then(|| cx.new(|cx| QueueView::new(window, cx))),
            lyrics: holds_tabs.then(|| cx.new(|cx| LyricsView::new(window, cx))),
            sheet_bar: (presentation == Presentation::Sheet)
                .then(|| cx.new(|cx| Self::new(Presentation::SheetBar, window, cx))),
            _devices_fetch: None,
            _subscriptions: subscriptions,
        }
    }

    /// Carry the daemon's position and level into the sliders the pointer is not holding.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (played, level) = {
            let player = PlayerModel::global(cx);
            let player = player.read(cx);
            (played_fraction(player), player.volume())
        };
        if self.scrubbing.is_none() {
            self.seek
                .update(cx, |state, cx| state.set_value(played, window, cx));
        }
        if !self.volume_held {
            let shown = volume::to_ui(level);
            self.volume
                .update(cx, |state, cx| state.set_value(shown, window, cx));
        }
        cx.notify();
    }

    /// Prepaint runs every frame, so a width that has not moved must not dirty the window.
    fn set_title_room(&mut self, room: Pixels, cx: &mut Context<Self>) {
        if self.title_room != room {
            self.title_room = room;
            cx.notify();
        }
    }

    /// A leave only clears the control it belongs to; the enter of the next one has already landed.
    fn set_hovered(&mut self, id: &'static str, hovered: bool, cx: &mut Context<Self>) {
        let next = if hovered { Some(id) } else { None };
        if (hovered || self.hovered == Some(id)) && self.hovered != next {
            self.hovered = next;
            cx.notify();
        }
    }

    fn track(&self, cx: &App) -> Option<api::TrackInfo> {
        PlayerModel::global(cx).read(cx).track().cloned()
    }
}

/// A slider over the 0..1 fraction of something, since a `SliderState`'s range is fixed at build time.
fn fraction_state(step: f32, start: f32) -> SliderState {
    components::settings::slider_state(0., 1., step, start)
}

fn played_fraction(player: &PlayerModel) -> f32 {
    match track_duration_ms(player) {
        Some(total) => (player.position_ms() as f64 / total as f64).clamp(0., 1.) as f32,
        None => 0.,
    }
}

/// A playable length, so radio (which reports none) and a stopped player both answer `None`.
fn track_duration_ms(player: &PlayerModel) -> Option<u64> {
    player
        .duration_ms()
        .filter(|total| *total > 0 && *total != u64::MAX)
}

/// One transition channel of a control, so each tweened value keeps a key of its own.
fn channel(id: impl Into<ElementId>, name: &'static str) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.into()), name.into())
}

impl Render for NowPlaying {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("now_playing");
        match self.presentation {
            Presentation::Bar | Presentation::SheetBar => self.render_bar(window, cx),
            Presentation::Sheet => self.render_sheet(window, cx),
            Presentation::Panel => self.render_panel(window, cx),
            Presentation::Mini => self.render_mini(window, cx),
        }
    }
}

impl NowPlaying {
    fn on_seek(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SliderEvent::Change(value) => self.scrubbing = Some(value.end()),
            SliderEvent::Release(value) => {
                let fraction = value.end();
                self.scrubbing = None;
                if let Some(total) = seekable_duration_ms(cx) {
                    let position = (f64::from(fraction) * total as f64).round() as u64;
                    PlayerModel::global(cx)
                        .update(cx, |player, cx| player.seek(position.min(total), cx));
                }
            }
        }
        cx.notify();
    }

    fn on_volume(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (shown, save) = match event {
            SliderEvent::Change(value) => {
                self.volume_held = true;
                (value.end(), false)
            }
            SliderEvent::Release(value) => {
                self.volume_held = false;
                (value.end(), true)
            }
        };
        let level = volume::to_wire(shown);
        PlayerModel::global(cx).update(cx, |player, cx| {
            if save {
                player.set_volume_saved(level, cx)
            } else {
                player.set_volume(level, cx)
            }
        });
    }
}

fn seekable_duration_ms(cx: &App) -> Option<u64> {
    let player = PlayerModel::global(cx);
    let player = player.read(cx);
    if player.is_loading() {
        return None;
    }
    track_duration_ms(player)
}
