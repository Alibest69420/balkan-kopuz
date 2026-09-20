//! The Library and Connectivity settings: local libraries, radio registries, media servers, integrations.

mod add_server;
mod integrations;
mod local;
mod login;
mod registries;
mod servers;
mod ui;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled, Window,
};

use integrations::Integrations;
use local::LocalLibraries;
use registries::RadioRegistries;
use servers::MediaServers;

/// Let a source switched from anywhere — the sidebar included — open the password form.
///
/// Which sign-in a source takes is the daemon's answer and rides on the switch; the form it may
/// need lives in this crate, so `store` is handed a way to reach it.
pub fn register(cx: &mut gpui_kit::App) {
    store::register_password_form(login::open, cx);
}

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
        // The settings page owns the scroll; this panel is content inside it.
        v_flex()
            .id("source-settings")
            .w_full()
            .gap_4()
            .pb_4()
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
