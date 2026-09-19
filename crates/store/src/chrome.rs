use gpui_kit::{App, Context, Entity};

/// What the right-hand panel shows, when it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePanel {
    Queue,
    Lyrics,
    Devices,
}

/// Window chrome state shared by the shell and the pieces that toggle it.
pub struct Chrome {
    panel: Option<SidePanel>,
    fullscreen: bool,
    compact: bool,
    quick_search: bool,
}

impl Chrome {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(_: &mut Context<Self>) -> Self {
        Self {
            panel: None,
            fullscreen: false,
            compact: false,
            quick_search: false,
        }
    }

    pub fn panel(&self) -> Option<SidePanel> {
        self.panel
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn is_compact(&self) -> bool {
        self.compact
    }

    pub fn is_quick_search_open(&self) -> bool {
        self.quick_search
    }

    /// Open `panel`, or close it when it is already the one showing.
    pub fn toggle_panel(&mut self, panel: SidePanel, cx: &mut Context<Self>) {
        self.panel = if self.panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        cx.notify();
    }

    pub fn set_panel(&mut self, panel: Option<SidePanel>, cx: &mut Context<Self>) {
        self.panel = panel;
        cx.notify();
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, cx: &mut Context<Self>) {
        self.fullscreen = fullscreen;
        cx.notify();
    }

    pub fn set_compact(&mut self, compact: bool, cx: &mut Context<Self>) {
        self.compact = compact;
        cx.notify();
    }

    pub fn set_quick_search(&mut self, open: bool, cx: &mut Context<Self>) {
        self.quick_search = open;
        cx.notify();
    }
}
