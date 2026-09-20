//! The knobs that belong to no single package: motion, accent and how big everything is drawn.

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window,
};
use store::LayoutModel;
use store::layout::{Accent, Motion, Section as Group};
use widgets::format::t;
use widgets::settings::{SettingRow, choice, section};

/// The scale steps the size setting offers, inside `layout::SCALE_RANGE`.
const STEPS: &[f32] = &[0.8, 0.9, 1.0, 1.1, 1.25, 1.4];

pub fn register(cx: &mut App) {
    crate::register(
        "general",
        "interface_general",
        0,
        Some(Group::Interface),
        |_, cx| cx.new(|_| General).into(),
        cx,
    );
}

struct General;

impl Render for General {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = store::layout(cx);
        let motion = layout.motion;
        let accent = layout.accent;
        let scale = layout.interface.scale;
        v_flex().w_full().child(
            section(t("interface_general"))
                .child(
                    SettingRow::new(t("interface_motion"))
                        .description(t("interface_motion_description"))
                        .control(choice(
                            "interface-motion",
                            &motion,
                            vec![
                                (Motion::Full, t("interface_motion_full")),
                                (Motion::Reduced, t("interface_motion_reduced")),
                                (Motion::Off, t("interface_motion_off")),
                            ],
                            |value, cx| LayoutModel::edit(cx, |layout| layout.motion = value),
                        )),
                )
                .child(
                    SettingRow::new(t("interface_accent"))
                        .description(t("interface_accent_description"))
                        .control(choice(
                            "interface-accent",
                            &accent,
                            vec![
                                (Accent::On, t("interface_accent_on")),
                                (Accent::SheetOnly, t("interface_accent_sheet")),
                                (Accent::Off, t("interface_accent_off")),
                            ],
                            |value, cx| LayoutModel::edit(cx, |layout| layout.accent = value),
                        )),
                )
                .child(
                    SettingRow::new(t("interface_scale"))
                        .description(t("interface_scale_description"))
                        .control(choice(
                            "interface-scale",
                            &nearest(scale),
                            STEPS.iter().map(|step| (*step, percent(*step))).collect(),
                            |value, cx| {
                                LayoutModel::edit(cx, |layout| layout.interface.scale = value)
                            },
                        )),
                ),
        )
    }
}

/// The step a saved scale reads as, so a hand-edited value still lights one option.
fn nearest(scale: f32) -> f32 {
    STEPS
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - scale)
                .abs()
                .partial_cmp(&(b - scale).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1.)
}

fn percent(scale: f32) -> SharedString {
    SharedString::from(format!("{}%", (scale * 100.).round() as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_scale_lights_the_step_it_is_closest_to() {
        assert_eq!(nearest(1.0), 1.0);
        assert_eq!(nearest(1.12), 1.1);
        assert_eq!(nearest(5.), 1.4);
    }

    #[test]
    fn a_step_reads_as_a_percentage() {
        assert_eq!(percent(1.25), "125%");
        assert_eq!(percent(0.8), "80%");
    }
}
