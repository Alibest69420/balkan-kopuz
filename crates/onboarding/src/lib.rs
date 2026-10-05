//! First run: add a source with the settings page's own form, then watch the daemon fill it.

mod pick;
mod sync;
mod ui;

use api::{AlbumPage, JobKind, Page, ServiceInfo, SourceInfo, Table};
use components::format::t;
use components::{StepFrame, Stepper};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use settings_sources::add_source::{AddSource, SourceAdded};
use store::{JobEvent, JobsModel, LayoutModel, Navigator, Query, Route, SourcesModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Pick,
    Sync,
}

impl Step {
    /// The rail's middle pip is the add form, which is a dialog over the first step.
    fn index(self) -> usize {
        match self {
            Step::Pick => 0,
            Step::Sync => 2,
        }
    }
}

/// The guide the shell draws instead of a Home made of empty shelves.
pub struct FirstRun {
    step: Step,
    /// The source this run added, so a finish is only ever about it.
    target: Option<String>,
    finished: bool,
    albums: Entity<Query<AlbumPage>>,
    services: Entity<Query<Vec<ServiceInfo>>>,
    add: Entity<AddSource>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl FirstRun {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sources = SourcesModel::global(cx);
        let jobs = JobsModel::global(cx);
        // One row is enough to answer "does this library hold anything".
        let albums = cx.new(|cx| {
            Query::new(
                &[Table::Albums, Table::Tracks],
                |api| async move {
                    api.albums(
                        Default::default(),
                        Page {
                            offset: 0,
                            limit: 1,
                        },
                    )
                    .await
                },
                cx,
            )
        });
        let services = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.services().await },
                cx,
            )
        });
        let host_access = cx.new(|cx| {
            Query::new(
                &[Table::Servers],
                |api| async move { api.can_open_browser().await },
                cx,
            )
        });
        let add = cx.new(|cx| AddSource::new(window, cx));
        let subscriptions = vec![
            cx.observe(&sources, |this, _, cx| this.settle(cx)),
            cx.observe(&albums, |this, _, cx| this.settle(cx)),
            cx.observe(&services, |_, _, cx| cx.notify()),
            cx.observe(&host_access, |this, query, cx| {
                let allowed = query.read(cx).value().copied().unwrap_or(true);
                this.add
                    .update(cx, |add, cx| add.set_host_access(allowed, cx));
                cx.notify();
            }),
            cx.subscribe_in(&add, window, |this, _, added: &SourceAdded, window, cx| {
                this.adopt(added.0.clone(), window, cx)
            }),
            cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                this.job_finished(event, cx)
            }),
        ];
        Self {
            step: Step::Pick,
            target: None,
            finished: false,
            albums,
            services,
            add,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Whether the shell should be drawing this at all.
    pub fn showing(&self, cx: &App) -> bool {
        wanted(
            self.in_flow(),
            self.ready(cx),
            self.has_content(cx),
            self.is_configured(cx),
            store::layout(cx).first_run_dismissed,
        )
    }

    /// Past the first step, so the answer stops depending on state the flow is busy changing.
    fn in_flow(&self) -> bool {
        !self.finished && self.step != Step::Pick
    }

    fn ready(&self, cx: &App) -> bool {
        SourcesModel::global(cx).read(cx).is_loaded() && self.albums.read(cx).is_settled()
    }

    fn has_content(&self, cx: &App) -> bool {
        self.albums
            .read(cx)
            .value()
            .is_some_and(|page| page.total > 0)
    }

    fn is_configured(&self, cx: &App) -> bool {
        configured(SourcesModel::global(cx).read(cx).sources())
    }

    /// Real state moved: drop a dismissal the state no longer supports.
    fn settle(&mut self, cx: &mut Context<Self>) {
        let stale = store::layout(cx).first_run_dismissed
            && self.ready(cx)
            && (self.has_content(cx) || self.is_configured(cx));
        if stale {
            LayoutModel::edit(cx, |layout| layout.first_run_dismissed = false);
        }
        cx.notify();
    }

    /// The daemon fills a newly active source on its own; its first library job ending is the flow's end.
    fn job_finished(&mut self, event: &JobEvent, cx: &mut Context<Self>) {
        let filling = matches!(event.kind, JobKind::Scan | JobKind::LibrarySync);
        let ours = active_source(cx).is_some_and(|source| self.target.as_ref() == Some(&source.id));
        if self.step == Step::Sync && event.ok && filling && ours {
            self.finish(cx);
        }
    }

    /// Land in the library, which is what the whole flow was for.
    fn finish(&mut self, cx: &mut Context<Self>) {
        self.finished = true;
        Navigator::navigate(Route::Library, cx);
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.finished = true;
        LayoutModel::edit(cx, |layout| layout.first_run_dismissed = true);
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Pick;
        cx.notify();
    }

    /// A source was saved: making it active is also what starts whichever sign-in it needs.
    fn adopt(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.step = Step::Sync;
        self.target = Some(id.clone());
        SourcesModel::global(cx).update(cx, |sources, cx| sources.switch(id, window, cx));
        cx.notify();
    }
}

impl Render for FirstRun {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("first_run");
        components::scroll::smooth("first-run-scroll", &self.scroll, window, cx);
        let step = self.step;
        let body = match step {
            Step::Pick => self.pick_step(cx),
            Step::Sync => self.sync_step(window, cx),
        };
        let stepper = Stepper::new([
            t("first_run_step_source"),
            t("first_run_step_connect"),
            t("first_run_step_scan"),
        ])
        .current(step.index());
        v_flex()
            .id("first-run")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .bg(cx.theme().background)
            .child(
                StepFrame::new(t("first_run_title"), t("first_run_subtitle"))
                    .stepper(stepper)
                    .when(step != Step::Pick, |frame| {
                        frame.on_back(cx.listener(|this: &mut Self, _, _, cx| this.go_back(cx)))
                    })
                    .on_skip(cx.listener(|this: &mut Self, _, _, cx| this.dismiss(cx)))
                    .child(body),
            )
    }
}

/// Show the guide while a run is under way, and otherwise only when there is nothing to show.
fn wanted(
    in_flow: bool,
    ready: bool,
    has_content: bool,
    configured: bool,
    dismissed: bool,
) -> bool {
    in_flow || (ready && !dismissed && !has_content && !configured)
}

/// A source that could hold music: one whose settings list folders needs at least one of them.
fn configured(sources: &[SourceInfo]) -> bool {
    sources.iter().any(|source| {
        api::schema::spec_value(&source.settings, settings_sources::folders::KEY)
            .is_none_or(|folders| !api::schema::decode_directories(folders).is_empty())
    })
}

fn active_source(cx: &App) -> Option<SourceInfo> {
    SourcesModel::global(cx).read(cx).active().cloned()
}

/// Nothing can be read from a source still waiting on a sign-in.
fn ready_to_sync(source: &SourceInfo) -> bool {
    source.authenticated || source.anonymous
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::schema::{FieldKind, FieldSpec};

    fn with_folders(folders: &[&str]) -> SourceInfo {
        let folders: Vec<String> = folders.iter().map(|path| path.to_string()).collect();
        SourceInfo {
            authenticated: true,
            settings: vec![FieldSpec {
                key: settings_sources::folders::KEY.into(),
                kind: FieldKind::Directories,
                value: Some(api::schema::encode_directories(&folders)),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn signed_in(authenticated: bool) -> SourceInfo {
        SourceInfo {
            authenticated,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_daemon_is_what_the_guide_is_for() {
        assert!(wanted(false, true, false, false, false));
    }

    #[test]
    fn nothing_shows_before_the_first_answers_land() {
        assert!(!wanted(false, false, false, false, false));
    }

    #[test]
    fn a_library_with_anything_in_it_outranks_every_flag() {
        assert!(!wanted(false, true, true, false, false));
        assert!(!wanted(false, true, true, true, false));
    }

    #[test]
    fn a_run_under_way_survives_the_source_it_just_configured() {
        assert!(wanted(true, true, false, true, false));
    }

    #[test]
    fn a_dismissal_holds_only_while_there_is_still_nothing() {
        assert!(!wanted(false, true, false, false, true));
    }

    #[test]
    fn a_source_listing_folders_counts_once_it_has_one() {
        assert!(!configured(&[with_folders(&[])]));
        assert!(configured(&[with_folders(&["/music"])]));
    }

    #[test]
    fn a_source_without_folders_counts_before_it_is_signed_in() {
        assert!(configured(&[signed_in(false)]));
    }

    #[test]
    fn a_source_waiting_on_a_sign_in_is_not_ready() {
        assert!(!ready_to_sync(&signed_in(false)));
        assert!(ready_to_sync(&signed_in(true)));
    }
}
