//! Continue: the queue the last session was interrupted in, and the one button that picks it up.

use components::ResumeCard;
use gpui_kit::{AnyElement, Context, IntoElement as _};
use store::{PlayerModel, QueueModel};

use crate::Home;

/// The banner the page opens with, or nothing at all when no queue was left behind.
pub(crate) fn banner(cx: &mut Context<Home>) -> Option<AnyElement> {
    let queue = QueueModel::global(cx).read(cx);
    let track = queue.current()?;
    let left = queue
        .len()
        .saturating_sub(queue.position().unwrap_or(0) as usize + 1);
    let playing = PlayerModel::global(cx).read(cx).is_playing();
    Some(
        ResumeCard::new("home-continue", track)
            .left(left)
            .playing(playing)
            .on_toggle(|_, cx| PlayerModel::global(cx).update(cx, |player, cx| player.toggle(cx)))
            .into_any_element(),
    )
}
