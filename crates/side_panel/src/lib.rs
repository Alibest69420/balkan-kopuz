//! The right-hand panel: the queue and the lyrics for what is playing, and the handle that sizes it.

mod lyrics;
mod queue;

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, DragMoveEvent, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled,
    Subscription, Window, div, px,
};
use store::{Chrome, SidePanel};
use widgets::format::t;
use widgets::{icons, motion, style};

// The Dioxus rightbar's drag bounds, and the width the shell's slot starts at.
const MIN_WIDTH: Pixels = px(280.);
const MAX_WIDTH: Pixels = px(600.);
const DEFAULT_WIDTH: Pixels = px(320.);
// `bg-black/40`: the rightbar darkens whatever the window paints behind it.
const SCRIM: f32 = 0.4;
// `px-4 py-1.5 text-xs` on a tab, and the `rounded-md` it is cut with.
const TAB_HEIGHT: Pixels = px(28.);
const TAB_RADIUS: Pixels = px(6.);
// `w-9 h-9` with a `text-sm` glyph: the close button.
const CLOSE_BUTTON: Pixels = px(36.);
const CLOSE_GLYPH: Pixels = px(14.);

/// One transition channel of an element, so each tweened value keeps a key of its own.
pub(crate) fn channel(id: impl Into<ElementId>, name: &'static str) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.into()), name.into())
}

/// Carried by the left-edge handle; nothing is dropped, so it paints nothing.
#[derive(Clone)]
struct ResizePanel;

impl Render for ResizePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// The panel the player bar's queue and lyrics buttons open, rendering whichever tab `Chrome` says.
pub struct SidePanelView {
    width: Pixels,
    queue: Entity<queue::QueuePanel>,
    lyrics: Entity<lyrics::LyricsPanel>,
    _subscriptions: Vec<Subscription>,
}

impl SidePanelView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chrome = Chrome::global(cx);
        let subscriptions = vec![cx.observe(&chrome, |_, _, cx| cx.notify())];
        Self {
            width: DEFAULT_WIDTH,
            queue: cx.new(|cx| queue::QueuePanel::new(window, cx)),
            lyrics: cx.new(|cx| lyrics::LyricsPanel::new(window, cx)),
            _subscriptions: subscriptions,
        }
    }

    /// What the panel has been dragged to; the shell sizes the slot it puts the panel in.
    pub fn width(&self) -> Pixels {
        self.width
    }

    fn tab(
        &self,
        id: &'static str,
        panel: SidePanel,
        key: &str,
        showing: SidePanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = showing == panel;
        let wash = motion::color(
            channel(id, "wash"),
            if active {
                style::white(style::ACTIVE, cx)
            } else {
                cx.theme().transparent
            },
            window,
            cx,
        );
        let label = motion::color(
            channel(id, "label"),
            if active {
                cx.theme().foreground
            } else {
                style::white(0.5, cx)
            },
            window,
            cx,
        );
        Button::new(id)
            .ghost()
            .xsmall()
            .label(t(key))
            .selected(active)
            .px_4()
            .h(TAB_HEIGHT)
            .rounded(TAB_RADIUS)
            .text_xs()
            .font_medium()
            .bg(wash)
            .text_color(label)
            .on_click(cx.listener(move |_, _, _, cx| {
                Chrome::global(cx).update(cx, |chrome, cx| chrome.set_panel(Some(panel), cx));
            }))
    }

    fn header(
        &self,
        showing: SidePanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .flex_shrink_0()
            .justify_between()
            .items_center()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(style::white(style::BORDER, cx))
            .child(
                h_flex()
                    .gap_1()
                    .p_1()
                    .rounded(style::radius_lg())
                    .bg(style::white(style::HOVER, cx))
                    .child(self.tab(
                        "panel-tab-queue",
                        SidePanel::Queue,
                        "up_next",
                        showing,
                        window,
                        cx,
                    ))
                    .child(self.tab(
                        "panel-tab-lyrics",
                        SidePanel::Lyrics,
                        "lyrics",
                        showing,
                        window,
                        cx,
                    )),
            )
            .child(
                Button::new("panel-close")
                    .ghost()
                    .with_size(CLOSE_GLYPH * (4. / 3.))
                    .size(CLOSE_BUTTON)
                    .rounded(CLOSE_BUTTON / 2.)
                    .icon(icons::icon(icons::CLOSE))
                    .text_color(style::white(0.4, cx))
                    .tooltip(t("close"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        Chrome::global(cx).update(cx, |chrome, cx| chrome.set_panel(None, cx));
                    })),
            )
    }

    /// The left edge drags the panel between 280 and 600 px; the panel is flush with the window.
    fn resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("side-panel-resize")
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .w(px(6.))
            .occlude()
            .cursor_col_resize()
            .hover(|handle| handle.bg(style::white(style::ACTIVE, cx)))
            .on_drag(ResizePanel, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizePanel>, window, cx| {
                    let edge = window.viewport_size().width;
                    let width = (edge - event.event.position.x).clamp(MIN_WIDTH, MAX_WIDTH);
                    if width != this.width {
                        this.width = width;
                        cx.notify();
                    }
                }),
            )
    }
}

impl Render for SidePanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The devices panel is the player bar's; this one only ever shows the queue or the lyrics.
        let showing = match Chrome::global(cx).read(cx).panel() {
            Some(panel @ (SidePanel::Queue | SidePanel::Lyrics)) => panel,
            _ => return div().into_any_element(),
        };

        v_flex()
            .relative()
            .h_full()
            .w(self.width)
            .flex_shrink_0()
            .bg(style::black(SCRIM, cx))
            .border_l_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(self.header(showing, window, cx))
            .child(div().flex_1().min_h_0().w_full().map(|body| match showing {
                SidePanel::Lyrics => body.child(self.lyrics.clone()),
                _ => body.child(self.queue.clone()),
            }))
            .child(self.resize_handle(cx))
            .into_any_element()
    }
}
