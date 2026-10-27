//! Running whichever sign-in a source names, from wherever the source was switched.
//!
//! Which flow a service takes is the daemon's answer, not a service name this side knows: it
//! reports `SignInKind` with the source. A browser flow it runs itself, and only has to be asked;
//! a password is collected here. The password form lives in a settings crate, so that crate
//! registers an opener and this calls it — the same shape the track metadata dialog uses.

use std::rc::Rc;

use api::{BrowserSession, SignInKind, SourceInfo};
use gpui_kit::{App, Global, SharedString, Window};

use crate::Notices;
use crate::sources::SourcesModel;

type Opener = Rc<dyn Fn(String, SharedString, &mut Window, &mut App)>;
type Chooser = Rc<dyn Fn(SourceInfo, Vec<BrowserSession>, &mut Window, &mut App)>;

#[derive(Default)]
struct PasswordForm(Option<Opener>);

impl Global for PasswordForm {}

/// The dialog offering the browser profiles already signed in; a settings crate registers it.
#[derive(Default)]
struct SessionChooser(Option<Chooser>);

impl Global for SessionChooser {}

/// The sources whose browser flow the daemon is running right now.
#[derive(Default)]
struct Waiting(Vec<String>);

impl Global for Waiting {}

/// Whether `id` is waiting on a browser window the daemon opened.
pub fn is_waiting_for_browser(id: &str, cx: &App) -> bool {
    cx.try_global::<Waiting>()
        .is_some_and(|waiting| waiting.0.iter().any(|held| held == id))
}

fn set_waiting(id: &str, waiting: bool, cx: &mut App) {
    if cx.try_global::<Waiting>().is_none() {
        cx.set_global(Waiting::default());
    }
    let held = cx.global_mut::<Waiting>();
    held.0.retain(|other| other != id);
    if waiting {
        held.0.push(id.to_string());
    }
    // A global has no observers of its own, and the row showing the spinner is somebody else's view.
    cx.refresh_windows();
}

/// Register the username/password dialog; the app does this at startup.
pub fn register_password_form(
    opener: impl Fn(String, SharedString, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    cx.set_global(PasswordForm(Some(Rc::new(opener))));
}

/// Register the dialog that offers signed-in browser profiles; the app does this at startup.
pub fn register_session_chooser(
    chooser: impl Fn(SourceInfo, Vec<BrowserSession>, &mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    cx.set_global(SessionChooser(Some(Rc::new(chooser))));
}

/// Sign `source` in because someone asked to: a browser profile already signed in is offered before a new window.
pub fn sign_in_again(source: &SourceInfo, window: &mut Window, cx: &mut App) {
    // `sign_in` reads None once a source holds credentials, stale or not; `reauth` says what renewing them takes.
    let kind = match source.sign_in {
        SignInKind::None => source.reauth,
        kind => kind,
    };
    match kind {
        SignInKind::None => {}
        SignInKind::Browser => offer_sessions(source.clone(), window, cx),
        SignInKind::Password => password(source, window, cx),
    }
}

/// The daemon checks each profile with the service, so the source shows as waiting meanwhile.
fn offer_sessions(source: SourceInfo, window: &mut Window, cx: &mut App) {
    let Some(chooser) = cx
        .try_global::<SessionChooser>()
        .and_then(|chooser| chooser.0.clone())
    else {
        return browser(source.id, cx);
    };
    set_waiting(&source.id, true, cx);
    let call = {
        let service = source.service.id.clone();
        crate::Link::global(cx).call(move |api| async move { api.browser_sessions(service).await })
    };
    window
        .spawn(cx, async move |cx| {
            let found = call.await.unwrap_or_else(|error| {
                tracing::debug!(%error, "could not list the browser sessions");
                Vec::new()
            });
            cx.update(|window, cx| {
                set_waiting(&source.id, false, cx);
                match found.is_empty() {
                    true => browser(source.id.clone(), cx),
                    false => chooser(source, found, window, cx),
                }
            })
            .ok();
        })
        .detach();
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
    set_waiting(&id, true, cx);
    let call = {
        let id = id.clone();
        crate::Link::global(cx).call(move |api| async move { api.authenticate_source(id).await })
    };
    cx.spawn(async move |cx| {
        let result = call.await;
        cx.update(|cx| {
            set_waiting(&id, false, cx);
            match result {
                Ok(_) => {
                    SourcesModel::global(cx).update(cx, |sources, cx| sources.refetch(cx));
                    Notices::info(i18n::t("signed_in"), cx);
                }
                Err(error) => Notices::failed(&error, cx),
            }
        });
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
    let service = match &source.service.name {
        api::Text::Key(key) => SharedString::from(i18n::t(key)),
        api::Text::Literal(literal) => SharedString::from(literal.clone()),
    };
    opener(source.id.clone(), service, window, cx);
}
