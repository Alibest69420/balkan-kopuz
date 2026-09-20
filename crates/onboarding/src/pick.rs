//! Step one: what kind of source this is going to be.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled,
    Window,
};
use widgets::format::t;
use widgets::icons;

use crate::{FirstRun, Kind, ui};

impl FirstRun {
    pub(crate) fn pick_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let choices = [
            (
                Kind::Local,
                "first-run-pick-local",
                icons::FOLDER,
                cx.theme().primary,
                t("first_run_local"),
                t("first_run_local_detail"),
            ),
            (
                Kind::Server,
                "first-run-pick-server",
                ui::SERVER,
                cx.theme().info,
                t("first_run_server"),
                t("first_run_server_detail"),
            ),
            (
                Kind::Streaming,
                "first-run-pick-streaming",
                ui::CLOUD,
                cx.theme().success,
                t("first_run_streaming"),
                t("first_run_streaming_detail"),
            ),
        ];
        v_flex()
            .w_full()
            .gap_3()
            .children(
                choices
                    .into_iter()
                    .map(|(kind, id, glyph, accent, title, detail)| {
                        ui::choice_card(id, glyph, accent, title, Some(detail), window, cx)
                            .on_click(
                                cx.listener(move |this: &mut Self, _, _, cx| this.choose(kind, cx)),
                            )
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }
}
