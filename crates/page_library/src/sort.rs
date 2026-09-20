//! The stacked sort control: one submenu per criterion, writing `cfg.library_sort`.

use config::{SortCriterion, SortDirection, TrackSortField};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use store::ConfigModel;
use widgets::format::t;
use widgets::{icons, motion, style};

use crate::{Library, clear_variant};

/// The sort button tracks its own hover, since a Button cannot carry a tween.
pub const CONTROL: &str = "library-sort-frame";

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
pub fn control(
    criteria: Criteria,
    hovered: bool,
    window: &mut Window,
    cx: &mut Context<Library>,
) -> impl IntoElement {
    let first = criteria.first().copied();
    let wash = style::white(if hovered { style::HOVER } else { style::RAISED }, cx);
    let target = if hovered {
        cx.theme().foreground
    } else {
        style::white(0.7, cx)
    };
    let background = motion::color(motion::id(CONTROL, "bg"), wash, window, cx);
    let ink = motion::color(motion::id(CONTROL, "ink"), target, window, cx);
    let button = Button::new("library-sort")
        // A Button paints its own hover, so the frame carries the wash and the variant the ink.
        .custom(clear_variant(ink, cx))
        .icon(icons::icon(first.map_or(icons::SORT, |criterion| {
            direction_icon(criterion.direction)
        })))
        .label(match first {
            Some(criterion) => {
                SharedString::from(format!("{}: {}", t("sort_by"), label(criterion.field)))
            }
            None => t("sort_by"),
        })
        .h_7()
        .px_3()
        .text_xs();
    h_flex()
        .id(CONTROL)
        .flex_shrink_0()
        .rounded(style::radius_lg())
        .bg(background)
        .border_1()
        .border_color(style::white(style::BORDER_SOFT, cx))
        .on_hover(
            cx.listener(|this, hovered: &bool, _, cx| this.track_control(CONTROL, *hovered, cx)),
        )
        .child(button.dropdown_menu(move |popup, window, cx| menu(&criteria, popup, window, cx)))
}
