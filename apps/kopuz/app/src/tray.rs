//! The status-notifier icon that keeps Balkan Kopuz reachable after its window closes.

use std::sync::LazyLock;

use futures::channel::mpsc::UnboundedSender;
use i18n::t;
use image::GenericImageView as _;
use ksni::Tray;
use ksni::blocking::TrayMethods as _;
use ksni::menu::StandardItem;

/// What a tray click asks the GPUI side to do; the tray only owns a channel.
#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    ShowHide,
    Toggle,
    Previous,
    Next,
    Quit,
}

pub struct BalkanTray {
    actions: UnboundedSender<TrayAction>,
}

impl BalkanTray {
    fn send(&self, action: TrayAction) {
        let _ = self.actions.unbounded_send(action);
    }
}

/// The window icon, decoded once and kept in the ARGB order SNI wants.
fn icon() -> ksni::Icon {
    static ICON: LazyLock<ksni::Icon> = LazyLock::new(|| {
        let image = image::load_from_memory_with_format(
            include_bytes!("../../../legacy/kopuz/assets/logo-128.png"),
            image::ImageFormat::Png,
        )
        .expect("the bundled tray icon is a PNG");
        let (width, height) = image.dimensions();
        let mut data = image.into_rgba8().into_vec();
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1); // RGBA to ARGB
        }
        ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        }
    });
    ICON.clone()
}

impl Tray for BalkanTray {
    fn id(&self) -> String {
        "balkan-kopuz".into()
    }

    fn title(&self) -> String {
        "Balkan Kopuz".into()
    }

    fn icon_name(&self) -> String {
        "balkan-kopuz".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::ShowHide);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::MenuItem;
        let toggle = format!("{} / {}", t("play"), t("pause"));
        vec![
            StandardItem {
                label: t("tray_show_hide").to_string(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::ShowHide)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: toggle,
                icon_name: "media-playback-start".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Toggle)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t("previous_track").to_string(),
                icon_name: "media-skip-backward".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Previous)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t("next_track").to_string(),
                icon_name: "media-skip-forward".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Next)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: t("tray_quit").to_string(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Start the tray service, returning whether the desktop accepted it.
///
/// The handle must outlive this call: the service stops when it drops, so it is
/// leaked for the lifetime of the process.
pub fn start(actions: UnboundedSender<TrayAction>) -> bool {
    match (BalkanTray { actions }).spawn() {
        Ok(handle) => {
            std::mem::forget(handle);
            true
        }
        Err(error) => {
            tracing::warn!(%error, "system tray unavailable; closing the window will quit");
            false
        }
    }
}
