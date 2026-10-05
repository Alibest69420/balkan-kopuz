//! The one selection pill: how much is picked, what to do with it, and the way out.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, IntoElement, ParentElement as _, Rems, RenderOnce,
    SharedString, Styled as _, Window, div, px, rems,
};
use store::Target;

use crate::format::{t, t_with};
use crate::{dialogs, icons, menus, motion, style};

const FADE: u64 = 0;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// One verb the pill spells out: a glyph, what it is called, and what it does to the selection.
pub struct Verb {
    id: SharedString,
    icon: &'static str,
    label: SharedString,
    run: Run,
}

impl Verb {
    pub fn new(
        id: impl Into<SharedString>,
        icon: &'static str,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            run: Rc::new(run),
        }
    }
}

/// The accent pill floating over a listing while anything in it is picked.
#[derive(IntoElement)]
pub struct SelectionBar {
    id: SharedString,
    count: usize,
    verbs: Vec<Verb>,
    target: Option<Target>,
    delete: Option<Run>,
    cancel: Option<Run>,
    lift: Rems,
}

impl SelectionBar {
    pub fn new(id: impl Into<SharedString>, count: usize) -> Self {
        Self {
            id: id.into(),
            count,
            verbs: Vec::new(),
            target: None,
            delete: None,
            cancel: None,
            lift: rems(2.5),
        }
    }

    pub fn verbs(mut self, verbs: impl IntoIterator<Item = Verb>) -> Self {
        self.verbs.extend(verbs);
        self
    }

    /// What the dots menu acts on, and what a delete's confirmation names.
    pub fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// A delete at the end of the verbs, run only once the confirmation is accepted.
    pub fn on_delete(mut self, delete: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.delete = Some(Rc::new(delete));
        self
    }

    pub fn on_cancel(mut self, cancel: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.cancel = Some(Rc::new(cancel));
        self
    }

    /// How far above the listing's bottom edge it floats, clearing whatever docks there.
    pub fn lift(mut self, lift: Rems) -> Self {
        self.lift = lift;
        self
    }
}

impl RenderOnce for SelectionBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let fade = motion::enter(motion::channel(&id, FADE), motion::MEDIUM, window, cx);
        let ink = cx.theme().primary_foreground;
        let rule = style::black(0.2, cx);
        let divider = move || -> Div { div().w(px(1.)).h_5().flex_shrink_0().bg(rule) };
        // Without this variant a Button's hover layer repaints the glyph grey over the accent slab.
        let bare = style::bare_button(ink, cx);
        let verb = |verb_id: SharedString, icon: &'static str, label: SharedString, run: Run| {
            Button::new(SharedString::from(format!("{id}-{verb_id}")))
                .custom(bare)
                .icon(icons::icon(icon))
                .label(label)
                .font_weight(FontWeight::MEDIUM)
                .on_click(move |_, window, cx| run(window, cx))
        };
        let verbs: Vec<AnyElement> = self
            .verbs
            .into_iter()
            .map(|entry| verb(entry.id, entry.icon, entry.label, entry.run).into_any_element())
            .collect();
        let named = self.target.as_ref().map(Target::label).unwrap_or_default();
        let delete = self.delete.map(|delete| {
            let confirm: Run = Rc::new(move |window, cx| {
                let delete = delete.clone();
                dialogs::confirm(
                    dialogs::ConfirmKind::Delete,
                    t("delete"),
                    named.clone(),
                    move |window, cx| delete(window, cx),
                    window,
                    cx,
                )
            });
            verb("delete".into(), icons::TRASH, t("delete"), confirm)
        });
        let more = self.target.map(|target| {
            Button::new(SharedString::from(format!("{id}-more")))
                .custom(bare)
                .icon(icons::icon(icons::MORE))
                .dropdown_menu(menus::builder(target))
        });
        let cancel = self.cancel;
        div()
            .absolute()
            .bottom(self.lift)
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
                    .child(div().pl_2().font_weight(FontWeight::BOLD).child(t_with(
                        "library_selected",
                        &[("count", self.count.to_string())],
                    )))
                    .child(divider())
                    .children(verbs)
                    .children(delete)
                    .children(more)
                    .child(divider())
                    .child(
                        Button::new(SharedString::from(format!("{id}-cancel")))
                            .custom(bare)
                            .icon(icons::icon(icons::CLOSE))
                            .tooltip(t("cancel"))
                            .when_some(cancel, |button, cancel| {
                                button.on_click(move |_, window, cx| cancel(window, cx))
                            }),
                    ),
            )
    }
}
