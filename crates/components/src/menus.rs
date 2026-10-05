//! The one menu builder, reached from below: `commands` registers it, every row and card calls it.

use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Context, Global, Window};
use store::Target;

type Builder = Rc<dyn Fn(Target, PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

#[derive(Default)]
struct Menus {
    build: Option<Builder>,
}

impl Global for Menus {}

/// Install the registry's builder; `commands::init` is the only caller.
pub fn register(
    builder: impl Fn(Target, PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    cx: &mut App,
) {
    cx.set_global(Menus {
        build: Some(Rc::new(builder)),
    });
}

/// Fill `menu` with what applies to `target`; an unregistered builder leaves it untouched.
pub fn build(
    target: Target,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let builder = cx
        .try_global::<Menus>()
        .and_then(|menus| menus.build.clone());
    match builder {
        Some(builder) => builder(target, menu, window, cx),
        None => {
            tracing::warn!("no menu builder is registered");
            menu
        }
    }
}

/// The closure `dropdown_menu` and `context_menu` both take.
pub fn builder(
    target: Target,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + Clone + 'static {
    move |menu, window, cx| build(target.clone(), menu, window, cx)
}
