//! The chrome over the listing: the facet switch, the filter bar and the selection pill.

use api::TrackFilter;
use commands::Command;
use components::format::{t, t_with};
use components::{ChipSize, ChipTone, PageHeader, SearchField, icons, menus, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px, rems,
};
use store::layout::{Density, TileSize, ViewMode};
use store::{Link, Target};

use crate::Library;
use crate::facet::Facet;
use crate::filters::{Chip, Filters};

/// How many verbs the pill spells out before the rest go under the dots.
const SPELLED_OUT: usize = 3;

impl Library {
    /// The heading, the facet switch, the filter bar and the facet's own controls.
    pub(crate) fn chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let count = self.total();
        let switch = self.facet_switch(cx);
        let controls = self.controls(cx);
        let filter_bar = self.filter_bar(cx);
        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_4()
            .pb_4()
            .child(
                PageHeader::new(t("your_library"))
                    .count(t_with("library_count", &[("count", count.to_string())]))
                    .flush()
                    .action(controls),
            )
            .child(switch)
            .child(filter_bar)
            .into_any_element()
            .tap(window)
    }

    /// Tracks, Albums, Artists, Playlists: the same listing, presented four ways.
    fn facet_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.filters().facet;
        h_flex()
            .gap_1()
            .p_1()
            .flex_shrink_0()
            .self_start()
            .rounded(style::radius_lg())
            .bg(style::white(style::RAISED, cx))
            .border_1()
            .border_color(style::white(style::BORDER_SOFT, cx))
            .children(Facet::ALL.iter().map(|facet| {
                let facet = *facet;
                let lit = facet == active;
                let ink = match lit {
                    true => cx.theme().foreground,
                    false => style::white(style::TEXT_SECONDARY, cx),
                };
                let fill = match lit {
                    true => style::white(style::HOVER, cx),
                    false => gpui_kit::transparent_black(),
                };
                Button::new(SharedString::from(format!(
                    "library-facet-{}",
                    facet.name()
                )))
                .custom(style::button(fill, style::white(style::HOVER, cx), ink, cx))
                .icon(icons::icon(facet.icon()))
                .label(t(facet.label_key()))
                .h_7()
                .px_3()
                .text_xs()
                .on_click(cx.listener(move |this, _, _, cx| {
                    let mut filters = this.filters().clone();
                    filters.facet = facet;
                    this.set_filters(filters, true, cx);
                }))
            }))
            .into_any_element()
    }

    /// Type to filter, then the chips that compose on top of what was typed.
    fn filter_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let filters = self.filters().clone();
        let chips: Vec<AnyElement> = Chip::ALL
            .iter()
            .copied()
            .filter(|chip| filters.applies(*chip))
            .map(|chip| self.chip(chip, &filters, cx))
            .collect();
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .flex_wrap()
            .child(
                // `max-w-2xl`: the filter never runs the width of the page.
                div()
                    .w_full()
                    .max_w(rems(26.))
                    .child(SearchField::new(&self.search).cleanable(true)),
            )
            .children(chips)
            .when(filters.is_narrowed(), |bar| {
                bar.child(
                    Button::new("library-clear-filters")
                        .ghost()
                        .small()
                        .icon(icons::icon(icons::CLOSE))
                        .label(t("clear"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            let cleared = this.filters().cleared();
                            this.search
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            this.set_filters(cleared, true, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// One chip: lit when it is narrowing, and carrying its own menu when it picks a value.
    fn chip(&self, chip: Chip, filters: &Filters, cx: &mut Context<Self>) -> AnyElement {
        let (lit, label) = match chip {
            Chip::Favorites => (filters.favorites, t(chip.label_key())),
            Chip::Downloaded => (filters.downloaded, t(chip.label_key())),
            Chip::Genre => match &filters.genre {
                Some(genre) => (true, SharedString::from(genre.clone())),
                None => (false, t(chip.label_key())),
            },
            Chip::Years => match filters.years {
                Some((from, to)) => (true, SharedString::from(format!("{from}–{to}"))),
                None => (false, t(chip.label_key())),
            },
            Chip::Folder => match &filters.folder {
                Some(folder) => (true, SharedString::from(leaf_of(folder))),
                None => (false, t(chip.label_key())),
            },
        };
        let button = components::Chip::new(chip.id())
            .tone(ChipTone::Filter)
            .size(ChipSize::Medium)
            .pill(true)
            .selected(lit)
            .icon(chip.icon())
            .label(label);
        if !chip.has_menu() {
            return button
                .on_click(cx.listener(move |this, _, _, cx| {
                    let mut filters = this.filters().clone();
                    match chip {
                        Chip::Favorites => filters.favorites = !filters.favorites,
                        Chip::Downloaded => filters.downloaded = !filters.downloaded,
                        _ => {}
                    }
                    this.set_filters(filters, true, cx);
                }))
                .into_any_element();
        }
        let entries = self.chip_entries(chip, cx);
        let page = cx.entity().downgrade();
        button
            .dropdown(move |mut menu, _, _| {
                let page = page.clone();
                for (value, label, checked) in entries.clone() {
                    let page = page.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                        move |_, _, cx| {
                            page.update(cx, |this, cx| {
                                let mut filters = this.filters().clone();
                                apply_chip(&mut filters, chip, value.clone());
                                this.set_filters(filters, true, cx);
                            })
                            .ok();
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    /// What a menu chip offers: the value it would set, what to call it, and whether it is on.
    fn chip_entries(&self, chip: Chip, cx: &App) -> Vec<(Option<String>, SharedString, bool)> {
        let filters = self.filters();
        let mut entries = vec![(None, t("library_chip_any"), chip_is_off(filters, chip))];
        match chip {
            Chip::Genre => {
                for genre in self.known_genres(cx) {
                    let checked = filters.genre.as_deref() == Some(genre.as_str());
                    entries.push((Some(genre.clone()), SharedString::from(genre), checked));
                }
            }
            Chip::Years => {
                for (from, to) in decades(self.year_span()) {
                    let value = format!("{from}-{to}");
                    let checked = filters.years == Some((from, to));
                    entries.push((
                        Some(value),
                        SharedString::from(format!("{from}–{to}")),
                        checked,
                    ));
                }
            }
            Chip::Folder => {
                for root in scan_roots(cx) {
                    let checked = filters.folder.as_deref() == Some(root.as_str());
                    entries.push((
                        Some(root.clone()),
                        SharedString::from(leaf_of(&root)),
                        checked,
                    ));
                }
            }
            _ => {}
        }
        entries
    }

    /// The facet's own knobs: how it is drawn, how it is ordered, and what it groups by.
    fn controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let facet = self.filters().facet;
        h_flex()
            .gap_2()
            .items_center()
            .flex_wrap()
            .when(facet == Facet::Tracks, |row| {
                row.child(self.group_control(cx))
                    .child(self.density_control(cx))
                    .child(self.columns_control(cx))
            })
            .when(facet != Facet::Tracks, |row| {
                row.child(self.view_control(cx))
                    .child(self.tile_control(cx))
                    .child(self.grid_sort_control(cx))
            })
            .into_any_element()
    }

    fn quiet(&self, id: &'static str, icon: &'static str, label: SharedString, cx: &App) -> Button {
        Button::new(id)
            .custom(style::button(
                style::white(style::RAISED, cx),
                style::white(style::HOVER, cx),
                style::white(0.7, cx),
                cx,
            ))
            .icon(icons::icon(icon))
            .label(label)
            .h_7()
            .px_3()
            .text_xs()
    }

    fn group_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let held = self.group_by().map(str::to_string);
        let label = match held.as_deref() {
            Some(field) => t(group_label_key(field)),
            None => t("library_group_none"),
        };
        let page = cx.entity().downgrade();
        self.quiet(
            "library-group",
            icons::ROWS,
            SharedString::from(format!("{}: {}", t("library_group_by"), label)),
            cx,
        )
        .dropdown_menu(move |mut menu, _, _| {
            for field in [None, Some("album"), Some("artist"), Some("year")] {
                let page = page.clone();
                let checked = held.as_deref() == field;
                let name = match field {
                    Some(field) => t(group_label_key(field)),
                    None => t("library_group_none"),
                };
                menu = menu.item(PopupMenuItem::new(name).checked(checked).on_click(
                    move |_, _, cx| {
                        page.update(cx, |this, cx| {
                            let group = field.map(str::to_string);
                            // Groups are only contiguous when the listing runs in their order.
                            let sort = group.clone().map(|field| store::layout::Sort {
                                field,
                                descending: false,
                            });
                            this.write_facet(cx, move |facet| {
                                facet.group_by = group;
                                if let Some(sort) = sort {
                                    facet.sort = sort;
                                }
                            });
                        })
                        .ok();
                    },
                ));
            }
            menu
        })
        .into_any_element()
    }

    fn density_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let held = self.density();
        let page = cx.entity().downgrade();
        self.quiet(
            "library-density",
            icons::GRID,
            t(density_label_key(held)),
            cx,
        )
        .dropdown_menu(move |mut menu, _, _| {
            for density in [Density::Comfortable, Density::Compact] {
                let page = page.clone();
                menu = menu.item(
                    PopupMenuItem::new(t(density_label_key(density)))
                        .checked(density == held)
                        .on_click(move |_, _, cx| {
                            page.update(cx, |this, cx| {
                                this.write_facet(cx, move |facet| facet.density = density)
                            })
                            .ok();
                        }),
                );
            }
            menu
        })
        .into_any_element()
    }

    /// Which columns the table draws; the same menu the header's right-click opens.
    fn columns_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = cx.entity().downgrade();
        let held = self.columns().to_vec();
        self.quiet("library-columns", icons::LIST, t("library_columns"), cx)
            .dropdown_menu(move |menu, window, cx| {
                crate::tracks::columns_menu(page.clone(), &held, menu, window, cx)
            })
            .into_any_element()
    }

    fn view_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let grid = self.mode == ViewMode::Grid;
        let (icon, label) = match grid {
            true => (icons::ROWS, t("view_list")),
            false => (icons::GRID, t("view_grid")),
        };
        self.quiet("library-view", icon, label, cx)
            .on_click(cx.listener(move |this, _, _, cx| {
                let next = match grid {
                    true => ViewMode::Table,
                    false => ViewMode::Grid,
                };
                this.write_facet(cx, move |facet| facet.mode = next);
            }))
            .into_any_element()
    }

    fn tile_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let held = self.tile;
        let page = cx.entity().downgrade();
        self.quiet("library-tile", icons::EXPAND, t(tile_label_key(held)), cx)
            .dropdown_menu(move |mut menu, _, _| {
                for size in [TileSize::Small, TileSize::Medium, TileSize::Large] {
                    let page = page.clone();
                    menu = menu.item(
                        PopupMenuItem::new(t(tile_label_key(size)))
                            .checked(size == held)
                            .on_click(move |_, _, cx| {
                                page.update(cx, |this, cx| {
                                    this.write_facet(cx, move |facet| facet.tile = size)
                                })
                                .ok();
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// A grid has no column headers to click, so its order lives in a menu.
    fn grid_sort_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let held = self.sort().clone();
        let fields: Vec<(&'static str, &'static str)> = match self.filters().facet {
            Facet::Artists => vec![
                ("title", "sort_field_name"),
                ("tracks", "sort_field_tracks"),
            ],
            Facet::Playlists => vec![
                ("title", "sort_field_name"),
                ("tracks", "sort_field_tracks"),
            ],
            _ => vec![
                ("title", "sort_field_title"),
                ("artist", "sort_field_artist"),
                ("year", "sort_field_year"),
            ],
        };
        let label = fields
            .iter()
            .find(|(id, _)| *id == held.field)
            .map(|(_, key)| t(key))
            .unwrap_or_else(|| t("sort_by"));
        let page = cx.entity().downgrade();
        let current = held.clone();
        self.quiet(
            "library-grid-sort",
            icons::SORT,
            SharedString::from(format!("{}: {label}", t("sort_by"))),
            cx,
        )
        .dropdown_menu(move |mut menu, _, _| {
            for (id, key) in fields.clone() {
                let page = page.clone();
                let current = current.clone();
                menu = menu.item(
                    PopupMenuItem::new(t(key))
                        .checked(current.field == id)
                        .on_click(move |_, _, cx| {
                            let next = current.toggled(id).to_layout();
                            page.update(cx, |this, cx| {
                                this.write_facet(cx, move |facet| facet.sort = next)
                            })
                            .ok();
                        }),
                );
            }
            menu
        })
        .into_any_element()
    }

    /// The pill over the listing while anything is picked. It never says nothing is.
    pub(crate) fn selection_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.selection.is_active() {
            return None;
        }
        let count = self.selection.count(self.total());
        if count == 0 {
            return None;
        }
        let target = self.pill_target(cx);
        let verbs: Vec<&'static Command> = commands::for_palette(&target, cx)
            .into_iter()
            .filter(|command| command.section == commands::Section::Act && command.icon.is_some())
            .take(SPELLED_OUT)
            .collect();
        let fade = components::motion::enter(
            "library-selection-bar",
            components::motion::MEDIUM,
            window,
            cx,
        );
        let ink = cx.theme().primary_foreground;
        let rule = style::black(0.2, cx);
        let bare = style::bare_button(ink, cx);
        Some(
            div()
                .absolute()
                .bottom_10()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .opacity(fade)
                .child(
                    h_flex()
                        .gap_4()
                        .items_center()
                        .px_6()
                        .py_2p5()
                        .rounded(cx.theme().radius_full())
                        .bg(cx.theme().primary)
                        .text_color(ink)
                        .shadow_2xl()
                        .child(
                            div()
                                .pl_2()
                                .font_weight(FontWeight::BOLD)
                                .child(t_with("library_selected", &[("count", count.to_string())])),
                        )
                        .child(div().w(px(1.)).h_5().flex_shrink_0().bg(rule))
                        .children(verbs.into_iter().map(|command| {
                            Button::new(SharedString::from(format!("library-act-{}", command.id)))
                                .custom(bare)
                                .icon(icons::icon(command.icon.unwrap_or(icons::PLAY)))
                                .label(command.label(&target, cx))
                                .font_weight(FontWeight::MEDIUM)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_on_selection(command, window, cx)
                                }))
                        }))
                        .child(
                            Button::new("library-selection-more")
                                .custom(bare)
                                .icon(icons::icon(icons::MORE))
                                .dropdown_menu(menus::builder(target.clone())),
                        )
                        .child(div().w(px(1.)).h_5().flex_shrink_0().bg(rule))
                        .child(
                            Button::new("library-selection-cancel")
                                .custom(bare)
                                .icon(icons::icon(icons::CLOSE))
                                .tooltip(t("cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.selection.clear();
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// What the pill's labels describe. Under select-all the keys are not known yet, so the
    /// target is the shape of the selection, and the keys are fetched when a verb is pressed.
    fn pill_target(&self, cx: &App) -> Target {
        if let Some(target) = self.picked_target(cx) {
            return target;
        }
        Target::tracks(Vec::new(), t("library_whole_selection"))
    }

    /// The target for what was actually clicked, when the keys are already in hand.
    fn picked_target(&self, cx: &App) -> Option<Target> {
        if self.selection.is_everything() {
            return None;
        }
        let picked = self.selection.picked();
        if picked.is_empty() {
            return None;
        }
        if self.filters().facet == Facet::Tracks {
            let keys: Vec<SharedString> = picked
                .iter()
                .map(|key| SharedString::from(key.clone()))
                .collect();
            return Some(Target::tracks(keys, self.selection_label(cx)));
        }
        if picked.len() == 1 {
            return self
                .tiles
                .iter()
                .find(|tile| tile.id.as_ref() == picked[0])
                .map(|tile| tile.target.clone());
        }
        None
    }

    /// "Title +3" rather than a count, which would need a plural form per locale.
    fn selection_label(&self, _: &App) -> SharedString {
        let picked = self.selection.picked();
        let first = picked
            .first()
            .and_then(|key| {
                self.rows
                    .iter()
                    .find(|row| &row.key == key)
                    .map(|row| row.title.clone())
            })
            .unwrap_or_default();
        match picked.len() {
            0 | 1 => SharedString::from(first),
            many => SharedString::from(format!("{first} +{}", many - 1)),
        }
    }

    /// Run a verb on the selection, fetching the keys first when the selection is a filter.
    pub(crate) fn run_on_selection(
        &mut self,
        command: &'static Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.picked_target(cx) {
            command.perform(target, window, cx);
            self.selection.clear();
            cx.notify();
            return;
        }
        let label = t("library_whole_selection");
        let resolve = self.resolve_selection(cx);
        self._resolve = Some(cx.spawn_in(window, async move |this, cx| {
            let keys = resolve.await;
            if keys.is_empty() {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                command.perform(Target::tracks(keys, label.clone()), window, cx);
                this.selection.clear();
                cx.notify();
            })
            .ok();
        }));
    }

    /// The keys a filter-scoped selection stands for, read once, at the moment they are needed.
    fn resolve_selection(
        &self,
        cx: &mut Context<Self>,
    ) -> impl std::future::Future<Output = Vec<SharedString>> + use<> {
        let filter = self.filters().track_filter(self.sort());
        let picked: Vec<String> = self.selection.picked().to_vec();
        let facet = self.filters().facet;
        let playlists: Vec<(String, Vec<String>)> = match facet {
            Facet::Playlists => store::PlaylistsModel::global(cx)
                .read(cx)
                .playlists()
                .iter()
                .map(|playlist| (playlist.id.clone(), playlist.track_keys.clone()))
                .collect(),
            _ => Vec::new(),
        };
        let call = Link::global(cx).call(move |api| {
            let filter = filter.clone();
            let picked = picked.clone();
            let playlists = playlists.clone();
            async move { gather(api, facet, filter, picked, playlists).await }
        });
        async move {
            match call.await {
                Ok(keys) => keys.into_iter().map(SharedString::from).collect(),
                Err(error) => {
                    tracing::debug!(%error, "could not resolve the selection");
                    Vec::new()
                }
            }
        }
    }
}

/// Every key the selection stands for, whichever facet it was made on.
async fn gather(
    api: std::sync::Arc<dyn api::KopuzApi>,
    facet: Facet,
    filter: TrackFilter,
    picked: Vec<String>,
    playlists: Vec<(String, Vec<String>)>,
) -> Result<Vec<String>, api::ApiError> {
    if picked.is_empty() {
        // Select-all: the filtered listing entire, read once rather than held as keys.
        let page = api.tracks(filter, store::all()).await?;
        return Ok(page.items.into_iter().map(|track| track.key).collect());
    }
    let mut keys = Vec::new();
    for id in picked {
        match facet {
            Facet::Tracks => keys.push(id),
            Facet::Albums => {
                let page = api.album_tracks(id, store::all()).await?;
                keys.extend(page.items.into_iter().map(|track| track.key));
            }
            Facet::Artists => {
                let page = api.artist_tracks(id, store::all()).await?;
                keys.extend(page.items.into_iter().map(|track| track.key));
            }
            Facet::Playlists => {
                if let Some((_, held)) = playlists.iter().find(|(held, _)| held == &id) {
                    keys.extend(held.iter().cloned());
                }
            }
        }
    }
    Ok(keys)
}

fn chip_is_off(filters: &Filters, chip: Chip) -> bool {
    match chip {
        Chip::Genre => filters.genre.is_none(),
        Chip::Years => filters.years.is_none(),
        Chip::Folder => filters.folder.is_none(),
        Chip::Favorites => !filters.favorites,
        Chip::Downloaded => !filters.downloaded,
    }
}

fn apply_chip(filters: &mut Filters, chip: Chip, value: Option<String>) {
    match chip {
        Chip::Genre => filters.genre = value,
        Chip::Folder => filters.folder = value,
        Chip::Years => {
            filters.years = value.and_then(|value| {
                let (from, to) = value.split_once('-')?;
                Some((from.parse().ok()?, to.parse().ok()?))
            })
        }
        _ => {}
    }
}

/// The decades a library actually spans, so the menu never offers an empty one.
fn decades(span: Option<(u16, u16)>) -> Vec<(u16, u16)> {
    let Some((first, last)) = span else {
        return Vec::new();
    };
    let start = first / 10 * 10;
    let end = last / 10 * 10;
    (0..=((end.saturating_sub(start)) / 10))
        .map(|step| {
            let from = start + step * 10;
            (from, from + 9)
        })
        .collect()
}

/// The last path segment, so a chip reads "Music" rather than a whole home directory.
fn leaf_of(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|leaf| !leaf.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// The folders the active source scans, which is what a folder chip can narrow to.
fn scan_roots(cx: &App) -> Vec<String> {
    store::SourcesModel::global(cx)
        .read(cx)
        .active()
        .and_then(|source| api::schema::spec_value(&source.settings, "directories"))
        .map(api::schema::decode_directories)
        .unwrap_or_default()
}

fn group_label_key(field: &str) -> &'static str {
    match field {
        "album" => "album",
        "artist" => "artist",
        "year" => "sort_field_year",
        _ => "library_group_none",
    }
}

fn density_label_key(density: Density) -> &'static str {
    match density {
        Density::Compact => "library_density_compact",
        Density::Comfortable => "library_density_comfortable",
    }
}

fn tile_label_key(size: TileSize) -> &'static str {
    match size {
        TileSize::Small => "library_tile_small",
        TileSize::Medium => "library_tile_medium",
        TileSize::Large => "library_tile_large",
    }
}

/// `AnyElement` has no builder tail; this keeps `chrome` reading as one expression.
trait Tap: Sized {
    fn tap(self, _: &mut Window) -> Self {
        self
    }
}

impl Tap for AnyElement {}

#[cfg(test)]
mod tests {
    use super::{decades, leaf_of};

    #[test]
    fn the_year_menu_offers_only_the_decades_the_library_spans() {
        assert_eq!(
            decades(Some((1983, 2004))),
            [(1980, 1989), (1990, 1999), (2000, 2009)]
        );
        assert!(decades(None).is_empty());
    }

    #[test]
    fn a_folder_chip_reads_as_its_last_segment() {
        assert_eq!(leaf_of("/home/someone/Music/"), "Music");
        assert_eq!(leaf_of("Music"), "Music");
    }
}
