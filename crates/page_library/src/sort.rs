//! The stacked sort control: one submenu per criterion, writing `cfg.library_sort`.

use config::{SortCriterion, SortDirection, TrackSortField};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Selectable as _, Sizable as _};
use gpui_kit::{App, Context, IntoElement, SharedString, Window};
use store::ConfigModel;
use widgets::format::t;
use widgets::icons;

type Criteria = Vec<SortCriterion<TrackSortField>>;

const FIELDS: &[TrackSortField] = &[
    TrackSortField::Title,
    TrackSortField::Artist,
    TrackSortField::Album,
    TrackSortField::Duration,
    TrackSortField::DateAdded,
];

fn label(field: TrackSortField) -> SharedString {
    t(match field {
        TrackSortField::Title => "sort_field_title",
        TrackSortField::Artist => "sort_field_artist",
        TrackSortField::Album => "sort_field_album",
        TrackSortField::Duration => "sort_field_duration",
        TrackSortField::DateAdded => "sort_field_date_added",
    })
}

fn direction_icon(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => icons::CHEVRON_UP,
        SortDirection::Desc => icons::CHEVRON_DOWN,
    }
}

fn write(cx: &mut App, edit: impl FnOnce(&mut Criteria)) {
    ConfigModel::global(cx).update(cx, |model, cx| {
        model.update(cx, |config| edit(&mut config.library_sort))
    });
}

fn set_field(index: usize, field: TrackSortField, cx: &mut App) {
    write(cx, |criteria| {
        if let Some(criterion) = criteria.get_mut(index) {
            criterion.field = field;
        }
    });
}

fn set_direction(index: usize, direction: SortDirection, cx: &mut App) {
    write(cx, |criteria| {
        if let Some(criterion) = criteria.get_mut(index) {
            criterion.direction = direction;
        }
    });
}

fn remove(index: usize, cx: &mut App) {
    write(cx, |criteria| {
        if index < criteria.len() {
            criteria.remove(index);
        }
    });
}

fn add(cx: &mut App) {
    write(cx, |criteria| {
        let used: Criteria = criteria.clone();
        if let Some(field) = FIELDS
            .iter()
            .copied()
            .find(|field| !used.iter().any(|criterion| criterion.field == *field))
        {
            criteria.push(SortCriterion::new(field, SortDirection::Asc));
        }
    });
}

fn criterion_menu(
    index: usize,
    criterion: SortCriterion<TrackSortField>,
    mut menu: PopupMenu,
) -> PopupMenu {
    for field in FIELDS.iter().copied() {
        menu = menu.item(
            PopupMenuItem::new(label(field))
                .checked(field == criterion.field)
                .on_click(move |_, _, cx| set_field(index, field, cx)),
        );
    }
    menu.separator()
        .item(
            PopupMenuItem::new(t("sort_ascending"))
                .icon(icons::icon(icons::CHEVRON_UP))
                .checked(criterion.direction == SortDirection::Asc)
                .on_click(move |_, _, cx| set_direction(index, SortDirection::Asc, cx)),
        )
        .item(
            PopupMenuItem::new(t("sort_descending"))
                .icon(icons::icon(icons::CHEVRON_DOWN))
                .checked(criterion.direction == SortDirection::Desc)
                .on_click(move |_, _, cx| set_direction(index, SortDirection::Desc, cx)),
        )
        .separator()
        .item(
            PopupMenuItem::new(t("sort_remove"))
                .icon(icons::icon(icons::CLOSE))
                .on_click(move |_, _, cx| remove(index, cx)),
        )
}

fn menu(
    criteria: &Criteria,
    mut menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if criteria.is_empty() {
        menu = menu.item(PopupMenuItem::label(t("sort_none")));
    }
    for (index, criterion) in criteria.iter().copied().enumerate() {
        let heading = if index == 0 {
            t("sort_by")
        } else {
            t("sort_then")
        };
        let title = format!("{heading}: {}", label(criterion.field));
        menu = menu.submenu(title, window, cx, move |submenu, _, _| {
            criterion_menu(index, criterion, submenu)
        });
    }
    if criteria.len() < FIELDS.len() {
        menu = menu.separator().item(
            PopupMenuItem::new(t("sort_add_criterion"))
                .icon(icons::icon(icons::PLUS))
                .on_click(move |_, _, cx| add(cx)),
        );
    }
    menu
}

/// The button and its stacked criteria menu; `criteria` is `cfg.library_sort`.
pub fn control(criteria: Criteria) -> impl IntoElement {
    let first = criteria.first().copied();
    let button = Button::new("library-sort")
        .ghost()
        .small()
        .selected(!criteria.is_empty())
        .icon(icons::icon(first.map_or(icons::SORT, |criterion| {
            direction_icon(criterion.direction)
        })))
        .label(match first {
            Some(criterion) => {
                SharedString::from(format!("{}: {}", t("sort_by"), label(criterion.field)))
            }
            None => t("sort_by"),
        });
    button.dropdown_menu(move |popup, window, cx| menu(&criteria, popup, window, cx))
}
