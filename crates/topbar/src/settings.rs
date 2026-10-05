//! Settings → Interface → Navigation: where the bar sits, what it holds, and where the app opens.

use components::format::t;
use components::settings::{SettingRow, Toggle, choice, section};
use components::{icons, style};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window,
};
use store::LayoutModel;
use store::layout::{DESTINATIONS, NavPosition, Section as Group};

pub fn register(cx: &mut App) {
    settings_interface::register(
        "navigation",
        "interface_nav",
        1,
        Some(Group::Nav),
        |window, cx| cx.new(|cx| Navigation::new(window, cx)).into(),
        cx,
    );
}

struct Navigation;

impl Navigation {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&LayoutModel::global(cx), |_, _, cx| cx.notify())
            .detach();
        Self
    }
}

/// Show or hide one destination, and shuffle it past its neighbours.
fn destination_row(name: &'static str, cx: &App) -> Option<SettingRow> {
    let nav = &store::layout(cx).nav;
    let shown = !nav.hidden.iter().any(|hidden| hidden == name);
    let order = nav.visible();
    let at = order.iter().position(|held| *held == name);
    let first = at == Some(0);
    let last = at.is_some_and(|at| at + 1 == order.len());
    SettingRow::new(t(crate::tabs::label(name)))
        .control(
            h_flex()
                .gap_1()
                .items_center()
                .child(step(name, -1, icons::CHEVRON_UP, shown && !first, cx))
                .child(step(name, 1, icons::CHEVRON_DOWN, shown && !last, cx))
                .child(
                    Toggle::new(SharedString::from(format!("nav-shows-{name}")))
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
    Button::new(SharedString::from(format!("nav-move-{name}-{delta}")))
        .small()
        .custom(style::quiet_button(style::slate_400(cx), cx))
        .icon(icons::icon(glyph))
        .disabled(!enabled)
        .on_click(move |_, _, cx| move_by(name, delta, cx))
}

fn show(name: &str, shown: bool, cx: &mut App) {
    let name = name.to_string();
    LayoutModel::edit(cx, move |layout| match shown {
        true => layout.nav.hidden.retain(|hidden| hidden != &name),
        false => {
            if !layout.nav.hidden.contains(&name) {
                layout.nav.hidden.push(name);
            }
        }
    });
}

/// Move a destination one place along the bar, in the full order rather than the visible one.
pub(crate) fn move_by(name: &str, delta: isize, cx: &mut App) {
    let name = name.to_string();
    LayoutModel::edit(cx, move |layout| {
        let order = &mut layout.nav.order;
        // A file written by hand may name only some of them; fill the rest in before moving one.
        for known in DESTINATIONS {
            if !order.iter().any(|held| held == known) {
                order.push(known.to_string());
            }
        }
        let Some(from) = order.iter().position(|held| held == &name) else {
            return;
        };
        let Some(to) = from.checked_add_signed(delta) else {
            return;
        };
        if to < order.len() {
            order.swap(from, to);
        }
    });
}

impl Render for Navigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = &store::layout(cx).nav;
        let position = nav.position;
        let start = nav.start.clone();
        let rows = DESTINATIONS
            .iter()
            .filter_map(|name| destination_row(name, cx))
            .collect::<Vec<_>>();
        v_flex()
            .w_full()
            .child(
                section(t("interface_nav"))
                    .children(
                        SettingRow::new(t("interface_nav_position"))
                            .description(t("interface_nav_position_description"))
                            .control(choice(
                                "interface-nav-position",
                                &position,
                                vec![
                                    (NavPosition::Top, t("interface_nav_position_top")),
                                    (NavPosition::Left, t("interface_nav_position_left")),
                                ],
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.nav.position = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("interface_nav_start"))
                            .description(t("interface_nav_start_description"))
                            .control(choice(
                                "interface-nav-start",
                                &start,
                                DESTINATIONS
                                    .iter()
                                    .map(|name| (name.to_string(), t(crate::tabs::label(name))))
                                    .collect(),
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.nav.start = value)
                                },
                            ))
                            .shown(cx),
                    )
                    .children(rows),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use store::layout::Nav;

    /// The same walk `move_by` does, over a `Nav` a test can hold.
    fn step(nav: &mut Nav, name: &str, delta: isize) {
        let order = &mut nav.order;
        for known in store::layout::DESTINATIONS {
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

    #[test]
    fn moving_a_destination_changes_what_the_bar_draws() {
        let mut nav = Nav::default();
        step(&mut nav, "library", -1);
        assert_eq!(nav.visible(), &["library", "home", "discover", "radio"]);
    }

    #[test]
    fn the_first_destination_cannot_walk_off_the_front() {
        let mut nav = Nav::default();
        step(&mut nav, "home", -1);
        assert_eq!(nav.visible(), Nav::default().visible());
    }

    #[test]
    fn showing_a_hidden_destination_draws_it() {
        let mut nav = Nav::default();
        nav.hidden.retain(|name| name != "favorites");
        assert!(nav.visible().contains(&"favorites"));
    }

    #[test]
    fn a_partial_order_still_moves_the_right_one() {
        let mut nav = Nav {
            order: vec!["home".into()],
            hidden: Vec::new(),
            ..Nav::default()
        };
        step(&mut nav, "radio", -1);
        assert_eq!(
            &nav.visible()[..4],
            &["home", "library", "radio", "discover"]
        );
    }
}
