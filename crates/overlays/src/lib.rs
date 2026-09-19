//! The banners over the routed page and the download progress overlay (docs/PARITY.md §3.1, §3.15).

use api::{DownloadItemState, JobKind, JobProgress};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div,
    px,
};
use store::{JobEvent, JobsModel, Link, PlayerModel};
use widgets::format::t;
use widgets::{icons, style};

// TODO(parity): the offline banner needs a connectivity probe this client does not have (§3.1).
// TODO(parity): the update banner checks GitHub, which is outside the daemon seam (§3.1).

const CARD_WIDTH: Pixels = px(288.);
const ROWS_MAX_HEIGHT: Pixels = px(192.);
const SCAN_BAR: Pixels = px(2.);
const SCAN_ROW_PY: Pixels = px(3.);
const TINY_ICON: Pixels = px(9.);
const SMALL_ICON: Pixels = px(12.);
const ROW_GLYPH: Pixels = px(14.);

// `bg-black/30` behind the scan line, and the two off-scale text alphas it uses.
const SCAN_SCRIM: f32 = 0.30;
const SCAN_GLYPH: f32 = 0.30;
const SCAN_TEXT: f32 = 0.35;
// `text-white/40` on the collapse chevron, and `bg-rose-500/15` behind the error banner.
const CHEVRON_TEXT: f32 = 0.40;
const BANNER_FILL: f32 = 0.15;
const BANNER_EDGE: f32 = 0.20;
// `text-red-400/70`: a failed row and the Stop chip sit below full danger.
const DANGER_SOFT: f32 = 0.70;

/// One requested download, named the way a row shows it.
struct DownloadRow {
    title: SharedString,
    artist: SharedString,
    state: DownloadItemState,
}

/// Drawn over the routed page, so it paints nothing until it has something to say.
pub struct Overlays {
    rows: Vec<DownloadRow>,
    collapsed: bool,
    // The error the user closed, so the banner stays gone until the daemon reports a different one.
    dismissed_error: Option<String>,
    // The job step the rows describe, so the titles are fetched once per reported step.
    fetched_step: Option<(String, u64)>,
    _fetch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Overlays {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = window;
        let jobs = JobsModel::global(cx);
        let player = PlayerModel::global(cx);
        let subscriptions = vec![
            cx.observe(&jobs, |this, _, cx| {
                this.sync_rows(cx);
                cx.notify();
            }),
            cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                if event.kind == JobKind::Download {
                    this.rows.clear();
                    this.fetched_step = None;
                    this.collapsed = false;
                    cx.notify();
                }
            }),
            cx.observe(&player, |_, _, cx| cx.notify()),
        ];
        Self {
            rows: Vec::new(),
            collapsed: false,
            dismissed_error: None,
            fetched_step: None,
            _fetch: None,
            _subscriptions: subscriptions,
        }
    }

    /// The batch reports keys; the titles come from the library, so a row says what is downloading.
    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        let jobs = JobsModel::global(cx);
        let Some(progress) = jobs.read(cx).progress(JobKind::Download).cloned() else {
            if self.fetched_step.take().is_some() {
                self.rows.clear();
            }
            return;
        };
        let step = (progress.id, progress.current.unwrap_or(0));
        if self.fetched_step.as_ref() == Some(&step) || self._fetch.is_some() {
            return;
        }
        let call = Link::global(cx).call(|api| async move {
            let items = api.download_statuses().await?;
            let keys: Vec<String> = items.iter().map(|item| item.key.clone()).collect();
            let tracks = api.tracks_by_keys(keys).await.unwrap_or_default();
            Ok((items, tracks))
        });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let answer = call.await;
            this.update(cx, |this, cx| {
                this._fetch = None;
                let Ok((items, tracks)) = answer else { return };
                this.fetched_step = Some(step);
                this.rows = items
                    .into_iter()
                    .map(|item| {
                        let track = tracks.iter().find(|track| track.key == item.key);
                        DownloadRow {
                            title: track
                                .map(|track| track.title.clone())
                                .filter(|title| !title.is_empty())
                                .unwrap_or(item.key)
                                .into(),
                            artist: track
                                .map(|track| track.artist.clone())
                                .unwrap_or_default()
                                .into(),
                            state: item.state,
                        }
                    })
                    .collect();
                cx.notify();
            })
            .ok();
        }));
    }

    /// Switching source clears the error it belonged to, so what is here is about what plays now.
    fn playback_error(&self, cx: &App) -> Option<SharedString> {
        let player = PlayerModel::global(cx);
        let message = player.read(cx).error()?.message.clone();
        if self.dismissed_error.as_deref() == Some(message.as_str()) {
            return None;
        }
        Some(message.into())
    }

    fn scan_banner(&self, progress: &JobProgress, cx: &mut Context<Self>) -> AnyElement {
        let file = progress
            .message
            .clone()
            .filter(|file| !file.is_empty())
            .map(SharedString::from);
        // TODO(i18n): the shell hard-coded this label, and a job's phases are raw daemon English.
        let phase = if progress.phase.is_empty() {
            SharedString::from("Scanning library…")
        } else {
            SharedString::from(progress.phase.clone())
        };
        v_flex()
            .w_full()
            .flex_shrink_0()
            .occlude()
            .child(
                Progress::new("scan-progress")
                    .xsmall()
                    .loading(true)
                    .color(cx.theme().primary)
                    .h(SCAN_BAR),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py(SCAN_ROW_PY)
                    .bg(style::black(SCAN_SCRIM, cx))
                    .border_b_1()
                    .border_color(style::white(style::BORDER_SOFT, cx))
                    .text_size(style::text_10())
                    .text_color(style::white(SCAN_TEXT, cx))
                    .child(
                        icons::icon(icons::DISC)
                            .size(TINY_ICON)
                            .flex_shrink_0()
                            .text_color(style::white(SCAN_GLYPH, cx)),
                    )
                    .child(div().flex_shrink_0().child(phase))
                    .when_some(file, |row, file| {
                        row.child(div().flex_1().min_w_0().truncate().child(file))
                    }),
            )
            .into_any_element()
    }

    fn error_banner(&self, message: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let danger = cx.theme().danger;
        h_flex()
            .w_full()
            .flex_shrink_0()
            .occlude()
            .items_center()
            .justify_between()
            .gap_3()
            .px_4()
            .py_2()
            .text_sm()
            .bg(danger.opacity(BANNER_FILL))
            .border_b_1()
            .border_color(danger.opacity(BANNER_EDGE))
            .text_color(danger)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(icons::icon(icons::WARNING).size(SMALL_ICON).flex_shrink_0())
                    .child(div().flex_1().min_w_0().child(message.clone())),
            )
            .child(
                div()
                    .id("dismiss-playback-error")
                    .flex()
                    .flex_shrink_0()
                    .p_1()
                    .cursor_pointer()
                    .opacity(0.5)
                    .hover(|button| button.opacity(1.))
                    .tooltip(|window, cx| Tooltip::new(t("close")).build(window, cx))
                    .child(icons::icon(icons::CLOSE).size(SMALL_ICON))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dismissed_error = Some(message.to_string());
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn download_card(&self, progress: &JobProgress, cx: &mut Context<Self>) -> AnyElement {
        let done = progress.current.unwrap_or(0);
        let total = progress.total.unwrap_or(0).max(done);
        let percent = if total == 0 {
            0.
        } else {
            done as f32 / total as f32 * 100.
        };
        let failed = self
            .rows
            .iter()
            .filter(|row| row.state == DownloadItemState::Failed)
            .count();
        let job_id = progress.id.clone();
        let collapsed = self.collapsed;

        let danger = cx.theme().danger;
        v_flex()
            .absolute()
            .bottom_4()
            .right_4()
            .w(CARD_WIDTH)
            .occlude()
            .rounded(style::radius_xl())
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(style::white(style::BORDER, cx))
            .shadow_lg()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(style::white(style::BORDER_SOFT, cx))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .items_center()
                            .child(
                                icons::icon(icons::DOWNLOAD)
                                    .size(ROW_GLYPH)
                                    .flex_shrink_0()
                                    .text_color(cx.theme().primary_hover),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    // TODO(i18n): the Dioxus overlay hard-coded this title.
                                    .child(format!("Downloading {done} / {total}")),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .id("stop-downloads")
                                    .px_2()
                                    .py_0p5()
                                    .rounded(style::radius(cx))
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(danger.opacity(style::HOVER))
                                    .text_color(danger.opacity(DANGER_SOFT))
                                    .hover(|chip| {
                                        chip.bg(danger.opacity(style::ACTIVE)).text_color(danger)
                                    })
                                    // TODO(i18n): no key exists for the overlay's Stop button.
                                    .child("Stop")
                                    .on_click(move |_, _, cx| {
                                        let id = job_id.clone();
                                        JobsModel::global(cx)
                                            .update(cx, |jobs, cx| jobs.cancel(id, cx));
                                    }),
                            )
                            .child(
                                div()
                                    .id("collapse-downloads")
                                    .flex()
                                    .cursor_pointer()
                                    .text_color(style::white(CHEVRON_TEXT, cx))
                                    .hover(|chevron| chevron.text_color(cx.theme().foreground))
                                    .child(
                                        icons::icon(if collapsed {
                                            icons::CHEVRON_DOWN
                                        } else {
                                            icons::CHEVRON_UP
                                        })
                                        .size(SMALL_ICON),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.collapsed = !this.collapsed;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .when(!collapsed, |card| {
                card.child(
                    v_flex()
                        .w_full()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .child(
                            Progress::new("download-progress")
                                .small()
                                .value(percent)
                                .color(cx.theme().primary),
                        )
                        .when(failed > 0, |body| {
                            body.child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .text_xs()
                                    .text_color(danger)
                                    .child(
                                        icons::icon(icons::WARNING)
                                            .size(SMALL_ICON)
                                            .flex_shrink_0(),
                                    )
                                    // TODO(i18n): no key exists for the overlay's failed count.
                                    .child(format!("{failed} failed")),
                            )
                        })
                        .when(!self.rows.is_empty(), |body| {
                            body.child(
                                v_flex()
                                    .id("download-rows")
                                    .w_full()
                                    .gap_1()
                                    .pt_2()
                                    .border_t_1()
                                    .border_color(style::white(style::BORDER_SOFT, cx))
                                    .max_h(ROWS_MAX_HEIGHT)
                                    .overflow_y_scroll()
                                    .children(self.rows.iter().map(|row| item_row(row, cx))),
                            )
                        }),
                )
            })
            .into_any_element()
    }
}

fn item_row(row: &DownloadRow, cx: &App) -> AnyElement {
    let (glyph, glyph_color) = match row.state {
        DownloadItemState::Downloading => (icons::DOWNLOAD, cx.theme().primary_hover),
        DownloadItemState::Failed => (icons::CLOSE, cx.theme().danger),
        DownloadItemState::Queued => (icons::CLOCK, style::slate_500(cx)),
    };
    let title_color = match row.state {
        DownloadItemState::Downloading => cx.theme().foreground,
        DownloadItemState::Failed => cx.theme().danger.opacity(DANGER_SOFT),
        DownloadItemState::Queued => style::slate_500(cx),
    };
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .text_xs()
        .child(
            icons::icon(glyph)
                .size(SMALL_ICON)
                .flex_shrink_0()
                .text_color(glyph_color),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_color(title_color)
                        .child(row.title.clone()),
                )
                .when(!row.artist.is_empty(), |cell| {
                    cell.child(
                        div()
                            .truncate()
                            .text_color(style::slate_500(cx))
                            .child(row.artist.clone()),
                    )
                }),
        )
        .into_any_element()
}

impl Render for Overlays {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (scan, download) = {
            let jobs = JobsModel::global(cx);
            let jobs = jobs.read(cx);
            (
                jobs.progress(JobKind::Scan).cloned(),
                jobs.progress(JobKind::Download).cloned(),
            )
        };
        let error = self.playback_error(cx);
        if scan.is_none() && download.is_none() && error.is_none() {
            return div().into_any_element();
        }

        let scan = scan.map(|progress| self.scan_banner(&progress, cx));
        let error = error.map(|message| self.error_banner(message, cx));
        let card = download.map(|progress| self.download_card(&progress, cx));
        v_flex()
            .absolute()
            .inset_0()
            .child(v_flex().w_full().children(scan).children(error))
            .children(card)
            .into_any_element()
    }
}
