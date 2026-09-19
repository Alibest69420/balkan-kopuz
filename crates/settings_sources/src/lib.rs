//! The Library and Connectivity settings: local libraries, radio registries, media servers, integrations.

mod add_server;
mod integrations;
mod local;
mod login;
mod registries;
mod row;
mod servers;
mod ui;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};

use integrations::Integrations;
use local::LocalLibraries;
use registries::RadioRegistries;
use servers::MediaServers;

pub struct SourceSettings {
    local: Entity<LocalLibraries>,
    registries: Entity<RadioRegistries>,
    servers: Entity<MediaServers>,
    integrations: Entity<Integrations>,
    section: Option<SharedString>,
}

impl SourceSettings {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            local: cx.new(|cx| LocalLibraries::new(window, cx)),
            registries: cx.new(|cx| RadioRegistries::new(window, cx)),
            servers: cx.new(|cx| MediaServers::new(window, cx)),
            integrations: cx.new(|cx| Integrations::new(window, cx)),
            section: None,
        }
    }
}

impl SourceSettings {
    /// Which rail entry is showing; `None` stacks every section.
    pub fn show(&mut self, section: Option<SharedString>, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn shows(&self, section: &str) -> bool {
        self.section.as_deref().is_none_or(|shown| shown == section)
    }
}

impl Render for SourceSettings {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("source-settings")
            .size_full()
            .gap_4()
            .pb_4()
            .overflow_y_scroll()
            .when(self.shows("library"), |panel| {
                panel.child(self.local.clone())
            })
            .when(self.shows("library"), |panel| {
                panel.child(self.registries.clone())
            })
            .when(self.shows("library"), |panel| {
                panel.child(self.servers.clone())
            })
            .when(self.shows("connectivity"), |panel| {
                panel.child(self.integrations.clone())
            })
    }
}
