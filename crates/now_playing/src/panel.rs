//! The docked column beside the content: the same queue and lyrics the sheet draws, in a narrower slot.

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, DragMoveEvent, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use store::layout::SheetTab;
use store::{Chrome, SidePanel};
use widgets::format::t;
use widgets::{icons, motion, style};

use crate::NowPlaying;
use crate::channel;

// The Dioxus rightbar's drag bounds, and the width the shell's slot starts at.
const MIN_WIDTH: Pixels = px(280.);
const MAX_WIDTH: Pixels = px(600.);
const DEFAULT_WIDTH: Pixels = px(320.);
// `bg-black/40`: the column darkens whatever the window paints behind it.
const SCRIM: f32 = 0.4;
const TAB_HEIGHT: Pixels = px(28.);
const TAB_RADIUS: Pixels = px(6.);
const CLOSE_BUTTON: Pixels = px(36.);
const CLOSE_GLYPH: Pixels = px(14.);

/// Carried by the left-edge handle; nothing is dropped, so it paints nothing.
#[derive(Clone)]
pub(crate) struct ResizePanel;

impl Render for ResizePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

impl NowPlaying {
    /// What the docked column has been dragged to; the shell sizes the slot it puts it in.
    pub fn panel_width(&self) -> Pixels {
        self.panel_width.unwrap_or(DEFAULT_WIDTH)
    }

    pub(crate) fn render_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Devices is the bar's popover; this column only ever shows the queue or the lyrics.
        let showing = match Chrome::global(cx).read(cx).panel() {
            Some(SidePanel::Lyrics) => SheetTab::Lyrics,
            Some(SidePanel::Queue) => SheetTab::Queue,
            _ => return div().into_any_element(),
        };

        v_flex()
            .relative()
            .h_full()
            .w(self.panel_width())
            .flex_shrink_0()
            .bg(style::black(SCRIM, cx))
            .border_l_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .justify_between()
                    .items_center()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(style::white(style::BORDER, cx))
                    .child(self.tab_strip(showing, false, window, cx))
                    .child(
                        Button::new("panel-close")
                            .custom(style::quiet_button(style::white(0.4, cx), cx))
                            .with_size(CLOSE_GLYPH * (4. / 3.))
                            .size(CLOSE_BUTTON)
                            .rounded(CLOSE_BUTTON / 2.)
                            .icon(icons::icon(icons::CLOSE))
                            .tooltip(t("close"))
                            .on_click(|_, _, cx| {
                                Chrome::global(cx)
                                    .update(cx, |chrome, cx| chrome.set_panel(None, cx))
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.tab_body(showing)),
            )
            .child(self.resize_handle(cx))
            .into_any_element()
    }

    /// The queue / lyrics / related picker, shared by the column and the sheet.
    pub(crate) fn tab_strip(
        &self,
        showing: SheetTab,
        with_related: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        h_flex()
            .gap_1()
            .p_1()
            .rounded(style::radius_lg())
            .bg(style::white(style::HOVER, cx))
            .child(self.tab("tab-queue", SheetTab::Queue, "up_next", showing, window, cx))
            .child(self.tab(
                "tab-lyrics",
                SheetTab::Lyrics,
                "lyrics",
                showing,
                window,
                cx,
            ))
            .when(with_related, |strip| {
                strip.child(self.tab(
                    "tab-related",
                    SheetTab::Related,
                    "related",
                    showing,
                    window,
                    cx,
                ))
            })
    }

    fn tab(
        &self,
        id: &'static str,
        tab: SheetTab,
        key: &str,
        showing: SheetTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = showing == tab;
        let hovered = self.hovered == Some(id);
        let wash = motion::color(
            channel(id, "wash"),
            match (active, hovered) {
                (true, _) => style::white(style::ACTIVE, cx),
                (false, true) => style::white(style::RAISED, cx),
                (false, false) => cx.theme().transparent,
            },
            window,
            cx,
        );
        let label = motion::color(
            channel(id, "label"),
            match (active, hovered) {
                (true, _) => cx.theme().foreground,
                (false, true) => style::white(0.8, cx),
                (false, false) => style::white(0.5, cx),
            },
            window,
            cx,
        );
        Button::new(id)
            .xsmall()
            .label(t(key))
            .selected(active)
            .px_4()
            .h(TAB_HEIGHT)
            .rounded(TAB_RADIUS)
            .text_xs()
            .font_medium()
            // The tween goes to every state layer: a Button's own hover is refined over the base,
            // so an instance `.bg()` alone is overpainted for exactly as long as the pointer is on it.
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(style::rest(wash))
                    .hover(wash)
                    .active(wash)
                    .foreground(label),
            )
            .on_hover(cx.listener(move |this, over: &bool, _, cx| this.set_hovered(id, *over, cx)))
            .on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
    }

    /// Which tab is showing, remembered so the app opens on the one it was left on.
    pub(crate) fn set_tab(&mut self, tab: SheetTab, cx: &mut Context<Self>) {
        self.tab = tab;
        if self.presentation == crate::Presentation::Panel {
            let panel = match tab {
                SheetTab::Lyrics => SidePanel::Lyrics,
                _ => SidePanel::Queue,
            };
            Chrome::global(cx).update(cx, |chrome, cx| chrome.set_panel(Some(panel), cx));
        }
        store::LayoutModel::edit(cx, move |layout| layout.sheet.tab = tab);
        cx.notify();
    }

    pub(crate) fn tab_body(&self, showing: SheetTab) -> AnyElement {
        match showing {
            SheetTab::Lyrics => self
                .lyrics
                .clone()
                .map(IntoElement::into_any_element)
                .unwrap_or_else(|| div().into_any_element()),
            SheetTab::Related => widgets::EmptyState::new(t("related_soon"))
                .icon(icons::COMPASS)
                .into_any_element(),
            SheetTab::Queue => self
                .queue
                .clone()
                .map(IntoElement::into_any_element)
                .unwrap_or_else(|| div().into_any_element()),
        }
    }

    /// The left edge drags the column between 280 and 600 px; it is flush with the window.
    fn resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("panel-resize")
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
                    if this.panel_width != Some(width) {
                        this.panel_width = Some(width);
                        cx.notify();
                    }
                }),
            )
    }
}
