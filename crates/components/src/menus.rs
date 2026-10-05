//! The one menu builder, reached from below: `commands` registers it, every row and card calls it.

use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenu};
use gpui_kit::{AnyElement, App, Context, Global, IntoElement, Window};
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

/// One builder shared by a dots button and a right-click, so the two menus cannot drift apart.
pub type Menu = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// The menu for `target`, ready to hand to a [`crate::MenuButton`] or [`WithContextMenu`].
pub fn menu_for(target: Target) -> Menu {
    Rc::new(builder(target))
}

/// Right-click opens `menu`, when there is one.
pub trait WithContextMenu: ContextMenuExt + IntoElement + Sized + 'static {
    fn with_context_menu(self, menu: Option<Menu>) -> AnyElement {
        match menu {
            Some(menu) => self
                .context_menu(move |popup, window, cx| menu(popup, window, cx))
                .into_any_element(),
            None => self.into_any_element(),
        }
    }
}

impl<T: ContextMenuExt + IntoElement + Sized + 'static> WithContextMenu for T {}
