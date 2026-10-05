//! Settings → Interface → Home: which shelves the page draws, and in what order.

use components::format::t;
use components::settings::{SettingRow, Toggle, section};
use components::{icons, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window,
};
use store::LayoutModel;
use store::layout::{SHELVES, Section as Group};

/// Registered by the app at startup, beside the other Interface sections.
pub fn register(cx: &mut App) {
    settings_interface::register(
        "home",
        "home",
        3,
        Some(Group::Home),
        |window, cx| cx.new(|cx| Shelves::new(window, cx)).into(),
        cx,
    );
}

struct Shelves;

impl Shelves {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&LayoutModel::global(cx), |_, _, cx| cx.notify())
            .detach();
        Self
    }
}

/// Show or hide one shelf, and shuffle it past its neighbours.
fn shelf_row(name: &'static str, cx: &App) -> Option<SettingRow> {
    let home = &store::layout(cx).home;
    let shown = !home.hidden.iter().any(|hidden| hidden == name);
    let order = home.visible();
    let at = order.iter().position(|held| *held == name);
    let first = at == Some(0);
    let last = at.is_some_and(|at| at + 1 == order.len());
    SettingRow::new(t(crate::shelf::title(name)))
        .control(
            h_flex()
                .gap_1()
                .items_center()
                .child(step(name, -1, icons::CHEVRON_UP, shown && !first, cx))
                .child(step(name, 1, icons::CHEVRON_DOWN, shown && !last, cx))
                .child(
                    Toggle::new(SharedString::from(format!("home-shows-{name}")))
                        .enabled(shown)
                        .on_change(move |on, _, cx| show(name, *on, cx)),
                ),
        )
        .shown(cx)
}

fn step(
    name: &'static str,
    delta: isize,
    glyph: &'static str,
    enabled: bool,
    cx: &App,
) -> impl IntoElement {
    Button::new(SharedString::from(format!("home-move-{name}-{delta}")))
        .small()
        .custom(style::quiet_button(style::slate_400(cx), cx))
        .icon(icons::icon(glyph))
        .disabled(!enabled)
        .on_click(move |_, _, cx| move_by(name, delta, cx))
}

fn show(name: &str, shown: bool, cx: &mut App) {
    let name = name.to_string();
    LayoutModel::edit(cx, move |layout| match shown {
        true => layout.home.hidden.retain(|hidden| hidden != &name),
        false => {
            if !layout.home.hidden.contains(&name) {
                layout.home.hidden.push(name);
            }
        }
    });
}

/// Move a shelf one place down the page, in the full order rather than the visible one.
fn move_by(name: &str, delta: isize, cx: &mut App) {
    let name = name.to_string();
    LayoutModel::edit(cx, move |layout| order_step(&mut layout.home, &name, delta));
}

/// The walk itself, over a `Home` a test can hold.
fn order_step(home: &mut store::layout::Home, name: &str, delta: isize) {
    let order = &mut home.order;
    // A file written by hand may name only some of them; fill the rest in before moving one.
    for known in SHELVES {
        if !order.iter().any(|held| held == known) {
            order.push(known.to_string());
        }
    }
    let Some(from) = order.iter().position(|held| held == name) else {
        return;
    };
    let Some(to) = from.checked_add_signed(delta) else {
        return;
    };
    if to < order.len() {
        order.swap(from, to);
    }
}

impl Render for Shelves {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = SHELVES
            .iter()
            .filter_map(|name| shelf_row(name, cx))
            .collect::<Vec<_>>();
        v_flex()
            .w_full()
            .child(section(t("home")).children(rows))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::layout::Home;

    #[test]
    fn moving_a_shelf_changes_what_the_page_draws() {
        let mut home = Home::default();
        order_step(&mut home, "recently_added", -1);
        assert_eq!(&home.visible()[..2], &["recently_added", "jump_back_in"]);
    }

    #[test]
    fn the_first_shelf_cannot_walk_off_the_top() {
        let mut home = Home::default();
        order_step(&mut home, "jump_back_in", -1);
        assert_eq!(home.visible(), Home::default().visible());
    }

    #[test]
    fn a_partial_order_still_moves_the_right_one() {
        let mut home = Home {
            order: vec!["jump_back_in".into()],
            hidden: Vec::new(),
        };
        order_step(&mut home, "top_artists", -1);
        assert_eq!(
            &home.visible()[..4],
            &[
                "jump_back_in",
                "recently_added",
                "top_artists",
                "made_for_you"
            ]
        );
    }
}
