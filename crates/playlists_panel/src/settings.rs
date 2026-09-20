//! Settings → Interface → Playlists panel: whether the column is there, and how wide it opens.

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled, Window,
};
use store::LayoutModel;
use store::layout::{Panel, Section as Group};
use widgets::format::t;
use widgets::settings::{SettingRow, Toggle, choice, section};

/// The widths the panel offers, inside what the drag handle allows.
const WIDTHS: &[f32] = &[200., 260., 320., 380.];

pub fn register(cx: &mut App) {
    settings_interface::register(
        "playlists-panel",
        "interface_panel",
        2,
        Some(Group::Panel),
        |window, cx| cx.new(|cx| PanelSettings::new(window, cx)).into(),
        cx,
    );
}

struct PanelSettings;

impl PanelSettings {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&LayoutModel::global(cx), |_, _, cx| cx.notify())
            .detach();
        Self
    }
}

/// The width a saved value reads as, so a hand-edited or dragged one still lights an option.
fn nearest(width: f32) -> f32 {
    WIDTHS
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - width)
                .abs()
                .partial_cmp(&(b - width).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(|| Panel::default().width)
}

fn pixels(width: f32) -> gpui_kit::SharedString {
    gpui_kit::SharedString::from(format!("{} px", width.round() as i32))
}

impl Render for PanelSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = store::layout(cx).panel.clone();
        v_flex()
            .w_full()
            .child(
                section(t("interface_panel"))
                    .children(
                        SettingRow::new(t("interface_panel_shown"))
                            .description(t("interface_panel_shown_description"))
                            .control(
                                Toggle::new("interface-panel-shown")
                                    .enabled(panel.shown)
                                    .on_change(|on, _, cx| {
                                        let on = *on;
                                        LayoutModel::edit(cx, move |layout| {
                                            layout.panel.shown = on
                                        });
                                    }),
                            )
                            .shown(cx),
                    )
                    .children(
                        SettingRow::new(t("interface_panel_width"))
                            .description(t("interface_panel_width_description"))
                            .control(choice(
                                "interface-panel-width",
                                &nearest(panel.width),
                                WIDTHS.iter().map(|step| (*step, pixels(*step))).collect(),
                                |value, cx| {
                                    LayoutModel::edit(cx, |layout| layout.panel.width = value)
                                },
                            ))
                            .shown(cx),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_width_lights_the_step_it_is_closest_to() {
        assert_eq!(nearest(263.), 260.);
        assert_eq!(nearest(1000.), 380.);
        assert_eq!(nearest(0.), 200.);
    }

    #[test]
    fn a_width_reads_as_pixels() {
        assert_eq!(pixels(260.), "260 px");
    }
}
