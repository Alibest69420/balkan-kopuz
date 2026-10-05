//! The docked column beside the content: the same queue and lyrics the sheet draws, in a narrower slot.

use components::format::t;
use components::{Edge, EdgeResizer, IconButton, Segment, Segmented, icons, style};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Pixels, Styled, Window, div, px,
};
use store::layout::SheetTab;
use store::{Chrome, SidePanel};

use crate::NowPlaying;

// The Dioxus rightbar's drag bounds, and the width the shell's slot starts at.
const MIN_WIDTH: Pixels = px(280.);
const MAX_WIDTH: Pixels = px(600.);
const DEFAULT_WIDTH: Pixels = px(320.);
// `bg-black/40`: the column darkens whatever the window paints behind it.
const SCRIM: f32 = 0.4;
const CLOSE_GLYPH: Pixels = px(14.);

impl NowPlaying {
    /// What the docked column has been dragged to; the shell sizes the slot it puts it in.
    pub fn panel_width(&self) -> Pixels {
        self.panel_width.unwrap_or(DEFAULT_WIDTH)
    }

    pub(crate) fn render_panel(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(self.tab_strip(showing, false, cx))
                    .child(
                        IconButton::new("panel-close", icons::CLOSE)
                            .size(style::chrome().round_button)
                            .glyph(CLOSE_GLYPH)
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut tabs = vec![(SheetTab::Queue, "up_next"), (SheetTab::Lyrics, "lyrics")];
        if with_related {
            tabs.push((SheetTab::Related, "related"));
        }
        let page = cx.entity().downgrade();
        Segmented::new("now-playing-tabs")
            .segments(tabs.iter().map(|(_, key)| Segment::new(*key).label(t(key))))
            .selected(tabs.iter().position(|(tab, _)| *tab == showing))
            .on_select(move |index, _, cx| {
                let Some((tab, _)) = tabs.get(*index).copied() else {
                    return;
                };
                page.update(cx, |this, cx| this.set_tab(tab, cx)).ok();
            })
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
            SheetTab::Related => components::EmptyState::new(t("related_soon"))
                .icon(icons::COMPASS)
                .into_any_element(),
            SheetTab::Queue => self
                .queue
                .clone()
                .map(IntoElement::into_any_element)
                .unwrap_or_else(|| div().into_any_element()),
        }
    }

    /// The left edge drags the column between 280 and 600 px.
    fn resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let panel = cx.entity().downgrade();
        EdgeResizer::new("panel-resize", Edge::Left, self.panel_width())
            .range(MIN_WIDTH, MAX_WIDTH)
            .on_resize(move |width, _, cx| {
                panel
                    .update(cx, |this, cx| {
                        if this.panel_width != Some(width) {
                            this.panel_width = Some(width);
                            cx.notify();
                        }
                    })
                    .ok();
            })
    }
}
