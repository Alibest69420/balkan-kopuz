//! Running whichever sign-in a source names, from wherever the source was switched.
//!
//! Which flow a service takes is the daemon's answer, not a service name this side knows: it
//! reports `SignInKind` with the source. A browser flow it runs itself, and only has to be asked;
//! a password is collected here. The password form lives in a settings crate, so that crate
//! registers an opener and this calls it — the same shape the track metadata dialog uses.

use std::rc::Rc;

use api::{SignInKind, SourceInfo};
use gpui_kit::{App, Global, SharedString, Window};

use crate::Notices;
use crate::sources::SourcesModel;

type Opener = Rc<dyn Fn(String, SharedString, &mut Window, &mut App)>;

#[derive(Default)]
struct PasswordForm(Option<Opener>);

impl Global for PasswordForm {}

/// Register the username/password dialog; the app does this at startup.
pub fn register_password_form(
    opener: impl Fn(String, SharedString, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    cx.set_global(PasswordForm(Some(Rc::new(opener))));
}

/// Start whichever sign-in `source` still needs. Nothing happens when it needs none.
pub fn start_sign_in(source: &SourceInfo, window: &mut Window, cx: &mut App) {
    match source.sign_in {
        SignInKind::None => {}
        SignInKind::Browser => browser(source.id.clone(), cx),
        SignInKind::Password => password(source, window, cx),
    }
}

/// Ask the daemon to run the browser flow. It owns the window that opens and the token it ends
/// holding; this only says which source it is for.
pub fn browser(id: String, cx: &mut App) {
    let call =
        crate::Link::global(cx).call(move |api| async move { api.authenticate_source(id).await });
    cx.spawn(async move |cx| match call.await {
        Ok(_) => {
            cx.update(|cx| {
                SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
            });
        }
        Err(error) => {
            cx.update(|cx| Notices::error(crate::human(&error), cx));
        }
    })
    .detach();
}

fn password(source: &SourceInfo, window: &mut Window, cx: &mut App) {
    let Some(opener) = cx
        .try_global::<PasswordForm>()
        .and_then(|form| form.0.clone())
    else {
        tracing::warn!(id = %source.id, "no password form is registered");
        return;
    };
    // Daemon-supplied text: a key is translated, a literal is shown as it came.
    let service = source
        .service
        .as_ref()
        .map(|service| match &service.name {
            api::Text::Key(key) => SharedString::from(i18n::t(key)),
            api::Text::Literal(literal) => SharedString::from(literal.clone()),
        })
        .unwrap_or_else(|| SharedString::from(source.name.clone()));
    opener(source.id.clone(), service, window, cx);
}
