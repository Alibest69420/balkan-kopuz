mod router;
mod shell;

use gpui_kit::component::Root;
use gpui_kit::{
    App, AppContext as _, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, px,
    size,
};
use shell::{Back, Escape, QuickSearch, Shell, TogglePlayback};

fn keymap(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("space", TogglePlayback, Some("Shell")),
        KeyBinding::new("cmd-k", QuickSearch, Some("Shell")),
        KeyBinding::new("ctrl-k", QuickSearch, Some("Shell")),
        KeyBinding::new("escape", Escape, Some("Shell")),
        KeyBinding::new("alt-left", Back, Some("Shell")),
        // Bindings beat key listeners, so without this the Shell's space would
        // toggle playback instead of typing one; NoAction suppresses it.
        KeyBinding::new("space", gpui_kit::NoAction, Some("Input")),
    ]);
}

/// Where the daemon is: `KOPUZ_DAEMON_ADDR` for a TCP daemon, `KOPUZ_SOCKET` for a socket elsewhere.
fn daemon_address() -> anyhow::Result<link::Address> {
    if let Ok(host) = std::env::var("KOPUZ_DAEMON_ADDR") {
        return Ok(link::Address::Tcp(host));
    }
    let socket = std::env::var_os("KOPUZ_SOCKET")
        .map(Into::into)
        .or_else(link::default_socket_path)
        .ok_or_else(|| anyhow::anyhow!("no runtime directory to find the daemon socket in"))?;
    Ok(link::Address::Socket(socket))
}

/// jemalloc, so the profiler can ask the allocator what is held instead of reading RSS.
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Hand freed pages back to the OS on a timer, so releasing a cover shows up in RSS rather than
/// leaving the process looking as large as it ever was.
#[unsafe(export_name = "malloc_conf")]
pub static MALLOC_CONF: &[u8] = b"background_thread:true,dirty_decay_ms:5000,muzzy_decay_ms:5000\0";

fn main() -> anyhow::Result<()> {
    // Held until main returns: dropping it is what flushes the chrome trace.
    let _profile = profile::init();

    let address = daemon_address()?;

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            i18n::init("en");
            if let Err(error) = link::init(address, cx) {
                tracing::error!(%error, "could not set up the daemon link");
                cx.quit();
                return;
            }
            store::init(cx);
            metadata_dialog::register(cx);
            theme::apply_named("default", None, cx);
            keymap(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1350.), px(800.)),
                    cx,
                ))),
                window_background: gpui_kit::WindowBackgroundAppearance::Opaque,
                app_id: Some("kopuz-gpui".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kopuz".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                let shell = cx.new(|cx| Shell::new(window, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            })
            .expect("failed to open the main window");
        });
    Ok(())
}
