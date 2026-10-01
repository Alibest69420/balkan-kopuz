//! First run: pick a source, configure it with the settings page's own forms, watch the first sync.

mod configure;
mod pick;
mod sync;
mod ui;

use api::{AlbumPage, JobKind, Page, ServiceInfo, SourceInfo, Table};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled,
    Subscription, Window, div, px,
};
use settings_sources::add_server::{AddServer, ServerAdded};
use store::{JobEvent, JobsModel, LayoutModel, Navigator, Query, Route, SourcesModel};
use widgets::format::t;
use widgets::{icons, style};

/// Which kind of source the first step chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Local,
    Server,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Pick,
    Configure(Kind),
    Sync,
}

impl Step {
    fn index(self) -> usize {
        match self {
            Step::Pick => 0,
            Step::Configure(_) => 1,
            Step::Sync => 2,
        }
    }
}

/// The guide the shell draws instead of a Home made of empty shelves.
pub struct FirstRun {
    step: Step,
    /// The source this run configured, so a stale active one never starts its job.
    target: Option<String>,
    job: Option<JobKind>,
    finished: bool,
    albums: Entity<Query<AlbumPage>>,
    services: Entity<Query<Vec<ServiceInfo>>>,
    host_access: Entity<Query<bool>>,
    add: Entity<AddServer>,
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
                    api.albums(Page {
                        offset: 0,
                        limit: 1,
                    })
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
        let add = cx.new(|cx| AddServer::new(window, cx));
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
            cx.subscribe_in(&add, window, |this, _, added: &ServerAdded, window, cx| {
                this.adopt_server(added.0.clone(), window, cx)
            }),
            cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                this.job_finished(event, cx)
            }),
        ];
        Self {
            step: Step::Pick,
            target: None,
            job: None,
            finished: false,
            albums,
            services,
            host_access,
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

    /// Real state moved: drop a dismissal the state no longer supports, and carry the flow forward.
    fn settle(&mut self, cx: &mut Context<Self>) {
        let stale = store::layout(cx).first_run_dismissed
            && self.ready(cx)
            && (self.has_content(cx) || self.is_configured(cx));
        if stale {
            LayoutModel::edit(cx, |layout| layout.first_run_dismissed = false);
        }
        if self.step == Step::Sync {
            self.maybe_start(cx);
        }
        cx.notify();
    }

    /// Start the first scan or sync, once the source this run configured is the live one.
    fn maybe_start(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let Some(source) = active_source(cx) else {
            return;
        };
        if self.target.as_deref() != Some(source.id.as_str()) || !ready_to_sync(&source) {
            return;
        }
        let kind = job_for(&source);
        self.job = Some(kind);
        JobsModel::global(cx).update(cx, |jobs, cx| jobs.start(kind, cx));
    }

    fn job_finished(&mut self, event: &JobEvent, cx: &mut Context<Self>) {
        if self.step != Step::Sync || self.job != Some(event.kind) {
            return;
        }
        if event.ok {
            self.finish(cx);
        } else {
            // The failure is already on screen as a toast; let the step offer another go.
            self.job = None;
            cx.notify();
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

    fn choose(&mut self, kind: Kind, cx: &mut Context<Self>) {
        self.step = Step::Configure(kind);
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Pick;
        cx.notify();
    }

    /// A server was saved: making it active is also what starts whichever sign-in it needs.
    fn adopt_server(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.step = Step::Sync;
        self.job = None;
        self.target = Some(id.clone());
        SourcesModel::global(cx).update(cx, |sources, cx| sources.switch(id, window, cx));
        cx.notify();
    }

    fn header(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_10()
                    .rounded(style::radius_xl())
                    .bg(style::accent(0.15, cx))
                    .child(
                        icons::icon(ui::SPARKLES)
                            .size(px(18.))
                            .text_color(cx.theme().primary),
                    ),
            )
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t("first_run_title")),
            )
            .child(ui::hint(t("first_run_subtitle"), cx))
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let quiet = style::bare_button(style::slate_400(cx), cx);
        h_flex()
            .w_full()
            .pt_2()
            .gap_2()
            .items_center()
            .justify_between()
            .child(div().when(self.step != Step::Pick, |slot| {
                slot.child(
                    Button::new("first-run-back")
                        .custom(quiet)
                        .small()
                        .icon(icons::icon(ui::ARROW_LEFT))
                        .label(t("first_run_back"))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.go_back(cx))),
                )
            }))
            .child(
                Button::new("first-run-skip")
                    .custom(quiet)
                    .small()
                    .label(t("first_run_skip"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.dismiss(cx))),
            )
    }
}

impl Render for FirstRun {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _timing = profile::render("first_run");
        widgets::scroll::smooth("first-run-scroll", &self.scroll, window, cx);
        let step = self.step;
        let body = match step {
            Step::Pick => self.pick_step(window, cx),
            Step::Configure(kind) => self.configure_step(kind, window, cx),
            Step::Sync => self.sync_step(window, cx),
        };
        v_flex()
            .id("first-run")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .w_full()
                    .max_w(ui::column_width())
                    .mx_auto()
                    .px_8()
                    .py_12()
                    .gap_6()
                    .child(self.header(cx))
                    .child(ui::rail(
                        [
                            t("first_run_step_source"),
                            t("first_run_step_connect"),
                            t("first_run_step_scan"),
                        ],
                        step.index(),
                        cx,
                    ))
                    .child(body)
                    .child(self.footer(cx)),
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

/// A source that could hold music: one that scans folders needs at least one to scan.
fn configured(sources: &[SourceInfo]) -> bool {
    sources
        .iter()
        .any(|source| !source.capabilities.scan_folders || !source.directories.is_empty())
}

/// The local library the first run writes folders into: the live one, or whichever the daemon lists.
fn local_source(cx: &App) -> Option<SourceInfo> {
    let model = SourcesModel::global(cx);
    let sources = model.read(cx).sources();
    sources
        .iter()
        .find(|source| source.capabilities.scan_folders && source.active)
        .or_else(|| {
            sources
                .iter()
                .find(|source| source.capabilities.scan_folders)
        })
        .cloned()
}

fn active_source(cx: &App) -> Option<SourceInfo> {
    SourcesModel::global(cx).read(cx).active().cloned()
}

/// Nothing can be read from a source that is still waiting on a sign-in or has nowhere to look.
fn ready_to_sync(source: &SourceInfo) -> bool {
    if source.capabilities.scan_folders {
        return !source.directories.is_empty();
    }
    source.authenticated || source.anonymous
}

fn job_for(source: &SourceInfo) -> JobKind {
    if source.capabilities.scan_folders {
        JobKind::Scan
    } else {
        JobKind::LibrarySync
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(directories: &[&str]) -> SourceInfo {
        SourceInfo {
            capabilities: api::SourceCapabilities {
                scan_folders: true,
                ..Default::default()
            },
            authenticated: true,
            directories: directories.iter().map(|path| path.to_string()).collect(),
            ..Default::default()
        }
    }

    fn server(authenticated: bool) -> SourceInfo {
        SourceInfo {
            service: Some(Default::default()),
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
    fn a_local_source_counts_once_it_has_somewhere_to_scan() {
        assert!(!configured(&[local(&[])]));
        assert!(configured(&[local(&["/music"])]));
    }

    #[test]
    fn a_server_counts_before_it_is_signed_in() {
        assert!(configured(&[server(false)]));
    }

    #[test]
    fn a_source_waiting_on_a_sign_in_starts_no_job() {
        assert!(!ready_to_sync(&server(false)));
        assert!(ready_to_sync(&server(true)));
        assert!(!ready_to_sync(&local(&[])));
        assert!(ready_to_sync(&local(&["/music"])));
    }

    #[test]
    fn a_server_syncs_where_a_local_library_scans() {
        assert_eq!(job_for(&server(true)), JobKind::LibrarySync);
        assert_eq!(job_for(&local(&["/music"])), JobKind::Scan);
    }
}
