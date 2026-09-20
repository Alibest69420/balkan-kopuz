//! Fetching a URL to a file: the tool, the formats and every option are the daemon's.

use api::schema::{ChoiceOption, FieldValue};
use api::{ApiEvent, DownloadHistoryEntry, DownloadState, JobKind, JobProgress};
use gpui_kit::component::button::{
    Button, ButtonCustomVariant, ButtonRounded, ButtonVariants as _,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window, div, rems,
};
use store::{JobEvent, JobsModel, Link, Notices, Query};
use widgets::format::{t, t_with, text};
use widgets::{EmptyState, FieldChanged, PageHeader, SchemaForm, icons, motion, style};

/// The downloader page: a URL, a format, the daemon's own options, and what has been fetched.
pub struct Downloader {
    url: Entity<InputState>,
    formats: Entity<Query<Vec<ChoiceOption>>>,
    format: Option<String>,
    options: Entity<SchemaForm>,
    show_options: bool,
    history: Entity<Query<Vec<DownloadHistoryEntry>>>,
    failure: Option<SharedString>,
    // Named before the daemon reports a title, so the active row has something to say.
    active_url: String,
    _subscriptions: Vec<Subscription>,
}

impl Downloader {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("downloader_url_placeholder")));
        let formats =
            cx.new(|cx| Query::new(&[], |api| async move { api.download_formats().await }, cx));
        let history =
            cx.new(|cx| Query::new(&[], |api| async move { api.downloader_history().await }, cx));
        let options = cx.new(|cx| SchemaForm::new(Vec::new(), window, cx));
        let jobs = JobsModel::global(cx);
        let connection = Link::connection(cx);

        let subscriptions = vec![
            cx.subscribe_in(
                &url,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.start(window, cx),
                    InputEvent::Change => this.clear_failure(cx),
                    _ => {}
                },
            ),
            cx.observe(&formats, |_, _, cx| cx.notify()),
            cx.observe(&history, |_, _, cx| cx.notify()),
            cx.observe(&jobs, |_, _, cx| cx.notify()),
            cx.subscribe(&jobs, |this, _, event: &JobEvent, cx| {
                // The history gains a row the moment the job stops running.
                if event.kind == JobKind::UrlDownload {
                    this.history.update(cx, |history, cx| history.refresh(cx));
                }
            }),
            cx.subscribe_in(
                &options,
                window,
                |this, _, changed: &FieldChanged, window, cx| {
                    this.save_setting(changed.0.clone(), window, cx)
                },
            ),
            cx.subscribe_in(
                &connection,
                window,
                |this, _, event: &ApiEvent, window, cx| {
                    if matches!(event, ApiEvent::Resync) {
                        this.load_settings(window, cx);
                    }
                },
            ),
        ];

        let mut page = Self {
            url,
            formats,
            format: None,
            options,
            show_options: false,
            history,
            failure: None,
            active_url: String::new(),
            _subscriptions: subscriptions,
        };
        page.load_settings(window, cx);
        page
    }

    /// The picked format, or the first the daemon offers: the page never names one itself.
    fn format(&self, cx: &Context<Self>) -> Option<String> {
        let offered = self.formats.read(cx).value();
        self.format
            .clone()
            .filter(|picked| {
                offered.is_some_and(|options| options.iter().any(|option| &option.value == picked))
            })
            .or_else(|| {
                offered
                    .and_then(|options| options.first())
                    .map(|option| option.value.clone())
            })
    }

    fn clear_failure(&mut self, cx: &mut Context<Self>) {
        if self.failure.take().is_some() {
            cx.notify();
        }
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value().trim().to_string();
        if url.is_empty() {
            return;
        }
        let format = self.format(cx).unwrap_or_default();
        self.failure = None;
        self.active_url = url.clone();
        self.url
            .update(cx, |state, cx| state.set_value("", window, cx));
        let call =
            Link::global(cx).call(move |api| async move { api.download_url(url, format).await });
        cx.spawn(async move |this, cx| {
            // Whether the tool is there and the folder writable is checked in the daemon, so a refusal arrives as an error.
            if let Err(error) = call.await {
                tracing::warn!(%error, "the download was refused");
                this.update(cx, |this, cx| {
                    this.failure = Some(error.to_string().into());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    fn load_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.downloader_settings().await });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(specs) = call.await else { return };
            this.update_in(cx, |this, window, cx| {
                this.options
                    .update(cx, |form, cx| form.set_specs(specs, window, cx));
            })
            .ok();
        })
        .detach();
    }

    fn save_setting(&mut self, value: FieldValue, window: &mut Window, cx: &mut Context<Self>) {
        let call = Link::global(cx)
            .call(move |api| async move { api.set_downloader_settings(vec![value]).await });
        cx.spawn_in(window, async move |this, cx| match call.await {
            Ok(specs) => {
                this.update_in(cx, |this, window, cx| {
                    this.options
                        .update(cx, |form, cx| form.set_specs(specs, window, cx));
                })
                .ok();
            }
            Err(error) => {
                tracing::warn!(%error, "saving a downloader setting failed");
                this.update(cx, |_, cx| Notices::error(store::human(&error), cx))
                    .ok();
            }
        })
        .detach();
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.clear_downloader_history().await });
        cx.spawn(async move |this, cx| match call.await {
            Ok(()) => {
                this.update(cx, |this, cx| {
                    this.history.update(cx, |history, cx| history.refresh(cx));
                })
                .ok();
            }
            Err(error) => {
                tracing::warn!(%error, "clearing the download history failed");
                this.update(cx, |_, cx| Notices::error(store::human(&error), cx))
                    .ok();
            }
        })
        .detach();
    }

    fn url_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .child(div().flex_1().child(Input::new(&self.url)))
            .child(
                // `bg-white/10`, not the accent: the Dioxus button is a quiet chip.
                Button::new("downloader-start")
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(style::rest(style::white(style::HOVER, cx)))
                            .foreground(cx.theme().foreground)
                            .hover(style::white(style::ACTIVE, cx))
                            .active(style::white(style::ACTIVE, cx)),
                    )
                    .rounded(ButtonRounded::Large)
                    .icon(icons::icon(icons::DOWNLOAD))
                    .label(t("downloader_download"))
                    .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
            )
    }

    fn format_chips(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = self.format(cx);
        let formats = self.formats.read(cx).value().cloned().unwrap_or_default();
        h_flex().gap_2().flex_wrap().children(
            formats
                .into_iter()
                .map(|option| {
                    let selected = picked.as_deref() == Some(option.value.as_str());
                    let value = option.value.clone();
                    // `transition-colors` as the pick moves from one chip to the next.
                    let wash = motion::color(
                        SharedString::from(format!("format-wash-{}", option.value)),
                        if selected {
                            style::white(style::ACTIVE, cx)
                        } else {
                            style::white(style::RAISED, cx)
                        },
                        window,
                        cx,
                    );
                    let label = motion::color(
                        SharedString::from(format!("format-label-{}", option.value)),
                        if selected {
                            cx.theme().foreground
                        } else {
                            style::slate_400(cx)
                        },
                        window,
                        cx,
                    );
                    let chip = ButtonCustomVariant::new(cx)
                        .color(style::rest(wash))
                        .foreground(label)
                        .hover(if selected {
                            wash
                        } else {
                            style::white(style::HOVER, cx)
                        })
                        .active(style::white(style::HOVER, cx));
                    Button::new(SharedString::from(format!("format-{}", option.value)))
                        .custom(chip)
                        .rounded(ButtonRounded::Large)
                        .xsmall()
                        .label(text(&option.label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.format = Some(value.clone());
                            cx.notify();
                        }))
                })
                .collect::<Vec<_>>(),
        )
    }

    fn failure_box(&self, failure: SharedString, cx: &Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        h_flex()
            .gap_2()
            .items_start()
            .px_4()
            .py_3()
            .rounded(style::radius_xl())
            .border_1()
            .border_color(danger.opacity(0.2))
            .bg(danger.opacity(0.1))
            .text_sm()
            .text_color(danger)
            .child(Icon::default().path(icons::WARNING).small())
            .child(div().flex_1().child(failure))
    }

    fn active_row(
        &self,
        progress: &JobProgress,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let processing = progress.phase == "processing";
        let percent = match (progress.current, progress.total) {
            (Some(current), Some(total)) if total > 0 => current as f32 * 100.0 / total as f32,
            _ => 0.0,
        };
        // `transition-all duration-300` on the bar, so a jumped percentage still slides.
        let filled = motion::amount("downloader-progress", percent, motion::SLOW, window, cx);
        let bar = motion::color(
            "downloader-progress-color",
            if processing {
                cx.theme().warning.opacity(0.6)
            } else {
                style::white(0.5, cx)
            },
            window,
            cx,
        );
        let status = if processing {
            t("downloader_status_processing")
        } else if progress.phase == "downloading" {
            t_with(
                "downloader_status_downloading",
                &[("percent", format!("{percent:.0}"))],
            )
        } else {
            t("downloader_status_waiting")
        };
        let title: SharedString = progress
            .message
            .clone()
            .map(SharedString::from)
            .unwrap_or_else(|| shorten(&self.active_url));
        let mark: AnyElement = if processing {
            Icon::default()
                .path(icons::REFRESH)
                .small()
                .text_color(cx.theme().warning)
                .into_any_element()
        } else {
            Spinner::new().small().into_any_element()
        };

        row(cx)
            .child(mark)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().truncate().child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                    .when(percent > 0.0, |column| {
                        column.child(
                            Progress::new("downloader-progress")
                                .value(filled)
                                .color(bar),
                        )
                    }),
            )
            .into_any_element()
    }

    fn history_row(
        &self,
        entry: &DownloadHistoryEntry,
        formats: &[ChoiceOption],
        cx: &Context<Self>,
    ) -> AnyElement {
        let failed = entry.state == DownloadState::Failed;
        let status: SharedString = match (failed, entry.error.clone()) {
            (true, Some(error)) => error.into(),
            (true, None) => t("downloader_status_failed"),
            (false, _) => t("downloader_status_completed"),
        };
        let format = formats
            .iter()
            .find(|option| option.value == entry.format)
            .map(|option| text(&option.label))
            .unwrap_or_default();
        let title: SharedString = if entry.title == entry.url {
            shorten(&entry.url)
        } else {
            entry.title.clone().into()
        };
        let (mark, mark_color, status_color) = if failed {
            (icons::CLOSE, cx.theme().danger, cx.theme().danger)
        } else {
            (
                icons::CHECK,
                cx.theme().success,
                cx.theme().muted_foreground,
            )
        };

        row(cx)
            .child(Icon::default().path(mark).small().text_color(mark_color))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_start()
                            .child(div().flex_1().min_w_0().text_sm().truncate().child(title))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(status_color)
                            .child(status),
                    ),
            )
            .into_any_element()
    }
}

/// A URL is what a row shows until the downloader names the file it is writing.
fn shorten(url: &str) -> SharedString {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .take(60)
        .collect::<String>()
        .into()
}

fn row(cx: &Context<Downloader>) -> gpui_kit::Div {
    h_flex()
        .w_full()
        .gap_3()
        .items_start()
        .px_4()
        .py_3()
        .rounded(style::radius_xl())
        .border_1()
        .border_color(style::white(style::BORDER, cx))
        .bg(style::white(style::RAISED, cx))
}

impl Render for Downloader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let formats = self.formats.read(cx).value().cloned().unwrap_or_default();
        let history = self.history.read(cx).value().cloned().unwrap_or_default();
        let progress = JobsModel::global(cx)
            .read(cx)
            .progress(JobKind::UrlDownload)
            .cloned();
        let listed = !history.is_empty();

        v_flex()
            .size_full()
            .p_6()
            .gap_3()
            .child(
                PageHeader::new(t("downloader_title"))
                    .subtitle(t("downloader_subtitle"))
                    .action(
                        Button::new("downloader-options")
                            .ghost()
                            .icon(icons::icon(icons::SETTINGS))
                            .tooltip(t("downloader_options"))
                            .selected(self.show_options)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_options = !this.show_options;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.url_row(cx))
            .child(self.format_chips(window, cx))
            .when_some(self.failure.clone(), |page, failure| {
                page.child(self.failure_box(failure, cx))
            })
            .when(self.show_options, |page| {
                page.child(
                    div()
                        .id("downloader-options-panel")
                        .max_h(rems(20.))
                        .overflow_y_scroll()
                        .px_4()
                        .py_2()
                        .rounded(style::radius_xl())
                        .border_1()
                        .border_color(style::white(style::BORDER, cx))
                        .bg(style::white(style::RAISED, cx))
                        .child(self.options.clone()),
                )
            })
            .map(|page| {
                if progress.is_none() && !listed {
                    return page.child(div().flex_1().min_h_0().child(
                        EmptyState::new(t("downloader_empty_state")).icon(icons::DOWNLOAD),
                    ));
                }
                page.child(
                    v_flex()
                        .id("downloader-list")
                        .flex_1()
                        .min_h_0()
                        .gap_2()
                        .overflow_y_scroll()
                        .when(listed, |list| {
                            list.child(
                                h_flex().w_full().justify_end().child(
                                    Button::new("downloader-clear")
                                        .text()
                                        .xsmall()
                                        .label(t("downloader_clear_history"))
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.clear_history(cx)),
                                        ),
                                ),
                            )
                        })
                        .children(progress.map(|progress| self.active_row(&progress, window, cx)))
                        .children(
                            history
                                .iter()
                                .map(|entry| self.history_row(entry, &formats, cx)),
                        ),
                )
            })
    }
}
