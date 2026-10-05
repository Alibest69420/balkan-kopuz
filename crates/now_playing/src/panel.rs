//! The docked column beside the content: the same queue and lyrics the sheet draws, in a narrower slot.

use components::format::t;
use components::{DockPanel, Edge, Segment, Segmented, icons};
use gpui_kit::{AnyElement, Context, IntoElement, Pixels, Window, div, px};
use store::layout::SheetTab;
use store::{Chrome, SidePanel};

use crate::NowPlaying;

// The Dioxus rightbar's drag bounds, and the width the shell's slot starts at.
const MIN_WIDTH: Pixels = px(280.);
const MAX_WIDTH: Pixels = px(600.);
const DEFAULT_WIDTH: Pixels = px(320.);

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
        let panel = cx.entity().downgrade();

        DockPanel::new("panel", Edge::Left)
            .width(self.panel_width())
            .header(self.tab_strip(showing, false, cx))
            .body(self.tab_body(showing))
            .on_close(|_, cx| {
                Chrome::global(cx).update(cx, |chrome, cx| chrome.set_panel(None, cx))
            })
            // The left edge drags the column between 280 and 600 px.
            .resizable(
                self.panel_width(),
                MIN_WIDTH,
                MAX_WIDTH,
                move |width, _, cx| {
                    panel
                        .update(cx, |this, cx| {
                            if this.panel_width != Some(width) {
                                this.panel_width = Some(width);
                                cx.notify();
                            }
                        })
                        .ok();
                },
            )
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
}
