//! Favorites: the source's liked tracks, filtered and sorted in the client.

mod sort;

use std::sync::Arc;
use std::time::Duration;

use api::{ApiError, FavoritesSyncMode, FavoritesView, JobKind, KopuzApi, Table, TrackInfo};
use gpui_kit::component::button::{
    Button, ButtonCustomVariant, ButtonRounded, ButtonVariants as _,
};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Task, Window, div, px, rems,
};
use store::{ConfigModel, JobsModel, Query, SourcesModel, actions};
use widgets::format::{t, t_with};
use widgets::{
    EmptyState, Loading, TrackActions, TrackList, TrackListEvent, TrackListOptions, columns, icons,
    motion, style,
};

// Filtering re-sorts the whole list, so a keystroke settles first; the Dioxus search bar used the same delay.
const FILTER_DELAY: Duration = Duration::from_millis(150);

// The shared icon set has no sign-in glyph, which is the one the anonymous empty state wants.
const SIGN_IN: &str = "icons/log-in.svg";

/// The favorites page: the liked list, its filter, its column sort, and the source's sync surface.
pub struct Favorites {
    filter: Entity<InputState>,
    favorites: Entity<Query<FavoritesView>>,
    tracks: Entity<Query<Vec<TrackInfo>>>,
    list: Entity<TrackList>,
    refs: Vec<String>,
    query: String,
    sort: sort::State,
    // The column header under the pointer, so its tint tweens rather than snaps.
    hovered_header: Option<sort::Field>,
    loaded: usize,
    // The header draws an album column only while the list does.
    show_album: bool,
    _filter_delay: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Favorites {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(t("search_placeholder")));
        let favorites = cx.new(|cx| {
            Query::new(
                &[Table::Favorites],
                |api| async move { api.favorites().await },
                cx,
            )
        });
        let tracks =
            cx.new(|cx| Query::new(&[Table::Tracks], |api| fetch_tracks(api, Vec::new()), cx));
        let options = list_options(cx);
        let list = cx.new(|cx| TrackList::new(cx).with_options(options));
        let jobs = JobsModel::global(cx);
        let sources = SourcesModel::global(cx);
        let config = ConfigModel::global(cx);
        let subscriptions = vec![
            cx.subscribe(&filter, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.delay_filter(value, cx);
                }
            }),
            cx.observe(&favorites, |this, _, cx| this.adopt_refs(cx)),
            cx.observe(&tracks, |this, _, cx| this.sync_rows(cx)),
            cx.subscribe(&list, |this, list, event: &TrackListEvent, cx| {
                this.on_list_event(list, event, cx)
            }),
            cx.observe(&jobs, |_, _, cx| cx.notify()),
            cx.observe(&sources, |this, _, cx| this.adopt_capabilities(cx)),
            cx.observe(&config, |this, _, cx| this.adopt_capabilities(cx)),
        ];
        let mut page = Self {
            filter,
            favorites,
            tracks,
            list,
            refs: Vec::new(),
            query: String::new(),
            sort: None,
            hovered_header: None,
            loaded: 0,
            show_album: options.show_album,
            _filter_delay: None,
            _subscriptions: subscriptions,
        };
        page.adopt_capabilities(cx);
        page
    }

    fn adopt_capabilities(&mut self, cx: &mut Context<Self>) {
        let delete = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        let options = list_options(cx);
        self.show_album = options.show_album;
        self.list.update(cx, |list, cx| {
            list.set_options(options, cx);
            list.set_actions(
                TrackActions {
                    delete,
                    ..TrackActions::default()
                },
                cx,
            );
        });
        cx.notify();
    }

    /// Point the track read at the keys the favorites read just answered with.
    fn adopt_refs(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.favorites.read(cx).value() else {
            return;
        };
        if view.refs == self.refs {
            return;
        }
        self.refs = view.refs.clone();
        let keys = self.refs.clone();
        self.tracks.update(cx, |query, cx| {
            query.set_fetch(move |api| fetch_tracks(api, keys.clone()), cx)
        });
    }

    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        let loaded = self.tracks.read(cx).value().cloned().unwrap_or_default();
        self.loaded = loaded.len();
        let mut rows: Vec<TrackInfo> = loaded
            .into_iter()
            .filter(|track| matches_filter(track, &self.query))
            .collect();
        sort::sort(&mut rows, self.sort);
        self.list.update(cx, |list, cx| list.set_rows(rows, cx));
        cx.notify();
    }

    fn delay_filter(&mut self, value: String, cx: &mut Context<Self>) {
        self._filter_delay = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FILTER_DELAY).await;
            this.update(cx, |this, cx| this.set_filter(value, cx)).ok();
        }));
    }

    fn set_filter(&mut self, value: String, cx: &mut Context<Self>) {
        let query = value.trim().to_lowercase();
        if query == self.query {
            return;
        }
        self.query = query;
        self.sync_rows(cx);
        self.list.read(cx).scroll_to(0);
    }

    fn toggle_sort(&mut self, field: sort::Field, cx: &mut Context<Self>) {
        self.sort = sort::next(self.sort, field);
        self.sync_rows(cx);
    }

    fn on_list_event(
        &mut self,
        list: Entity<TrackList>,
        event: &TrackListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TrackListEvent::Play(index) => {
                let keys = list.read(cx).keys();
                actions::play_tracks(keys, Some(*index as u32), cx);
            }
            TrackListEvent::Delete(keys) => self.delete(keys.clone(), cx),
            TrackListEvent::SelectionChanged => cx.notify(),
            TrackListEvent::RemoveFromPlaylist(_) | TrackListEvent::Reorder { .. } => {}
        }
    }

    fn delete(&mut self, keys: Vec<String>, cx: &mut Context<Self>) {
        let from_disk = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        actions::run(cx, move |api| async move {
            api.delete_tracks(keys, from_disk).await
        });
    }

    fn all_selected(&self, cx: &Context<Self>) -> bool {
        let list = self.list.read(cx);
        !list.rows().is_empty() && list.selected_count() == list.rows().len()
    }

    fn toggle_select_all(&mut self, cx: &mut Context<Self>) {
        let all = self.all_selected(cx);
        self.list.update(cx, |list, cx| {
            if all {
                list.clear_selection(cx);
            } else {
                list.select_all(cx);
            }
        });
    }

    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.clear_selection(cx));
    }

    fn selection_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let count = self.list.read(cx).selected_count();
        if count == 0 {
            return None;
        }
        let deletable = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .delete_from_disk;
        let pill = h_flex()
            .gap_4()
            .items_center()
            .px_5()
            .py_2()
            .rounded_full()
            // A raised surface, not the accent the Dioxus pill used: the ghost buttons on it set their own colour.
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    // TODO(i18n): the Dioxus selection bar hardcoded this line too.
                    .child(SharedString::from(format!("{count} selected"))),
            )
            .child(
                Button::new("selection-queue")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::QUEUE_ADD))
                    .label(t("add_to_queue"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let keys = this.list.read(cx).selected_keys();
                        actions::append(keys, cx);
                        this.clear_selection(cx);
                    })),
            )
            .child(
                Button::new("selection-playlist")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::PLUS))
                    .label(t("add_to_playlist"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let keys = this.list.read(cx).selected_keys();
                        widgets::playlist_picker::open(keys, window, cx);
                        this.clear_selection(cx);
                    })),
            )
            .when(deletable, |bar| {
                bar.child(
                    Button::new("selection-delete")
                        .ghost()
                        .small()
                        .icon(icons::icon(icons::TRASH))
                        .label(t("delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let keys = this.list.read(cx).selected_keys();
                            this.delete(keys, cx);
                            this.clear_selection(cx);
                        })),
                )
            })
            .child(
                Button::new("selection-cancel")
                    .ghost()
                    .small()
                    .icon(icons::icon(icons::CLOSE))
                    .tooltip(t("cancel"))
                    .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx))),
            );
        Some(
            h_flex()
                .absolute()
                .bottom_4()
                .left_0()
                .right_0()
                .justify_center()
                .child(pill)
                .into_any_element(),
        )
    }

    /// The spinner an instant-sync source shows, and the counter plus Refresh a paginated one does.
    fn sync_status(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mode = SourcesModel::global(cx)
            .read(cx)
            .capabilities()
            .favorites_sync;
        let progress = JobsModel::global(cx)
            .read(cx)
            .progress(JobKind::FavoritesSync)
            .cloned();
        let syncing = progress.is_some();
        let muted = cx.theme().muted_foreground;
        match mode {
            FavoritesSyncMode::Instant if !syncing => None,
            FavoritesSyncMode::Instant => Some(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .text_color(muted)
                    .child(Spinner::new().small())
                    .child(t("syncing_with_server"))
                    .into_any_element(),
            ),
            FavoritesSyncMode::Paginated => {
                let synced = progress.and_then(|job| job.current).unwrap_or(0);
                let total = self.list.read(cx).rows().len();
                let success = cx.theme().success;
                Some(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .text_xs()
                        .text_color(muted)
                        .child(h_flex().gap_2().items_center().map(|row| {
                            if syncing {
                                row.child(Spinner::new().small()).child(t_with(
                                    "yt_syncing_progress",
                                    &[("count", synced.to_string())],
                                ))
                            } else if total > 0 {
                                row.child(icons::icon(icons::CHECK).small().text_color(success))
                                    .child(t_with(
                                        "yt_synced_total",
                                        &[("count", total.to_string())],
                                    ))
                            } else {
                                row
                            }
                        }))
                        .child(
                            Button::new("favorites-refresh")
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .color(style::rest(style::white(style::HOVER, cx)))
                                        .foreground(style::white(0.8, cx))
                                        .hover(style::white(style::ACTIVE, cx))
                                        .active(style::white(style::ACTIVE, cx)),
                                )
                                .rounded(ButtonRounded::Large)
                                .small()
                                .icon(icons::icon(icons::REFRESH))
                                .label(t("refresh"))
                                .disabled(syncing)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    JobsModel::global(cx).update(cx, |jobs, cx| {
                                        jobs.start(JobKind::FavoritesSync, cx)
                                    });
                                })),
                        )
                        .into_any_element(),
                )
            }
        }
    }

    /// The select-all row and the sortable column header, built from the row's own column frame.
    fn columns(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let all_selected = self.all_selected(cx);
        let show_album = self.show_album;
        let title = self.sort_header(sort::Field::Title, window, cx);
        let artist = self.sort_header(sort::Field::Artist, window, cx);
        let album = self.sort_header(sort::Field::Album, window, cx);
        let duration = self.sort_header(sort::Field::Duration, window, cx);
        v_flex()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .mb_4()
                    .px_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(style::slate_500(cx))
                    .child(
                        Checkbox::new("favorites-select-all")
                            .checked(all_selected)
                            .accessibility_label(t("select_all_tracks"))
                            .on_click(
                                cx.listener(|this, _: &bool, _, cx| this.toggle_select_all(cx)),
                            ),
                    )
                    .child(t("select_all")),
            )
            .child(
                columns::frame()
                    .mb_2()
                    .border_b_1()
                    .border_color(style::white(style::BORDER, cx))
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(style::white(style::TEXT_MUTED, cx))
                    .child(columns::fixed(columns::INDEX))
                    .child(columns::flexible(columns::TITLE_MIN, 2.).child(title))
                    .child(columns::flexible(columns::TEXT_MIN, 1.).child(artist))
                    .when(show_album, |header| {
                        header.child(columns::flexible(columns::TEXT_MIN, 1.).child(album))
                    })
                    .child(
                        columns::fixed(columns::DURATION)
                            .justify_end()
                            .child(duration),
                    )
                    .child(columns::fixed(columns::MENU)),
            )
            .into_any_element()
    }

    fn sort_header(
        &self,
        field: sort::Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = matches!(self.sort, Some((sorted, _)) if sorted == field);
        let lit = active || self.hovered_header == Some(field);
        let color = motion::color(
            field.id(),
            if lit {
                cx.theme().foreground
            } else {
                style::slate_500(cx)
            },
            window,
            cx,
        );
        h_flex()
            .id(field.id())
            .gap_1()
            .items_center()
            .cursor_pointer()
            .text_color(color)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hovered_header = Some(field);
                } else if this.hovered_header == Some(field) {
                    this.hovered_header = None;
                }
                cx.notify();
            }))
            .map(|header| {
                // The duration column is too narrow for a word, so it reads as a clock.
                if field == sort::Field::Duration {
                    header.child(icons::icon(icons::CLOCK).xsmall())
                } else {
                    header.child(t(field.label_key()))
                }
            })
            .child(icons::icon(sort::indicator(self.sort, field)).size(style::text_10()))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sort(field, cx)))
            .into_any_element()
    }

    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.list.read(cx).rows().is_empty() {
            let columns = self.columns(window, cx);
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_2()
                .child(columns)
                .child(div().flex_1().min_h_0().child(self.list.clone()))
                .into_any_element();
        }
        // A running sync is already saying so above; an empty state on top of it would contradict it.
        if JobsModel::global(cx)
            .read(cx)
            .is_running(JobKind::FavoritesSync)
        {
            return div().flex_1().into_any_element();
        }
        if !self.favorites.read(cx).is_settled() || !self.tracks.read(cx).is_settled() {
            return Loading.into_any_element();
        }
        if self.loaded > 0 && !self.query.is_empty() {
            return EmptyState::new(t_with("no_results_found", &[("query", self.query.clone())]))
                .icon(icons::SEARCH)
                .into_any_element();
        }
        let anonymous = SourcesModel::global(cx)
            .read(cx)
            .active()
            .is_some_and(|source| source.anonymous);
        if anonymous {
            return EmptyState::new(t("source_anon_favorites"))
                .icon(SIGN_IN)
                .into_any_element();
        }
        EmptyState::new(t("no_favorites"))
            .icon(icons::HEART)
            .detail(t("heart_track_to_add"))
            .into_any_element()
    }
}

impl Render for Favorites {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sync_status = self.sync_status(cx);
        let body = self.body(window, cx);
        let selection_bar = self.selection_bar(cx);
        v_flex()
            .size_full()
            .relative()
            .px_8()
            .pt_8()
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .mb_8()
                    .child(
                        icons::icon(icons::HEART)
                            .size(px(24.))
                            .text_color(cx.theme().danger),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child(t("favorites")),
                    ),
            )
            .child(
                // `max-w-2xl`: the filter never runs the width of the page.
                div().w_full().max_w(rems(42.)).mb_8().child(
                    Input::new(&self.filter)
                        .cleanable(true)
                        .prefix(icons::icon(icons::SEARCH).small()),
                ),
            )
            .children(sync_status)
            .child(body)
            .children(selection_bar)
    }
}

fn list_options(cx: &App) -> TrackListOptions {
    TrackListOptions {
        show_cover: ConfigModel::global(cx).read(cx).config().show_row_images,
        ..TrackListOptions::default()
    }
}

/// No favorites means no keys, and no call worth making.
async fn fetch_tracks(
    api: Arc<dyn KopuzApi>,
    keys: Vec<String>,
) -> Result<Vec<TrackInfo>, ApiError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    api.tracks_by_keys(keys).await
}

fn matches_filter(track: &TrackInfo, query: &str) -> bool {
    query.is_empty()
        || track.title.to_lowercase().contains(query)
        || track.artist.to_lowercase().contains(query)
        || track.album.to_lowercase().contains(query)
        || track
            .artists
            .iter()
            .any(|artist| artist.to_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::matches_filter;

    fn track() -> api::TrackInfo {
        api::TrackInfo {
            key: "test.flac".to_string(),
            uid: "test.flac".to_string(),
            album_id: "album-id".to_string(),
            title: "Midnight City".to_string(),
            artist: "M83".to_string(),
            album: "Hurry Up, We're Dreaming".to_string(),
            duration_ms: Some(244_000),
            artists: vec!["Anthony Gonzalez".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn the_filter_matches_every_credited_field_case_insensitively() {
        let track = track();
        for query in ["", "midnight", "m83", "dreaming", "gonzalez"] {
            assert!(matches_filter(&track, query));
        }
        assert!(!matches_filter(&track, "unrelated"));
    }
}
