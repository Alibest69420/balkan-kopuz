use std::time::{Duration, Instant};

use api::{
    ApiEvent, BufferedRange, ExternalPlayback, Intent, LoopMode, NowPlaying, Phase, PlayerCommand,
    PlayerState, PositionAnchor, TrackKind,
};
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

// Fast enough for a seek bar's second labels; lyrics request their own frames.
const PLAYHEAD_TICK: Duration = Duration::from_millis(250);

/// The daemon's player, mirrored from `PlayerState` events with the position interpolated locally.
pub struct PlayerModel {
    state: PlayerState,
    // Local instant at which the daemon clock read `state.now_ms`.
    observed_at: Instant,
    optimistic_playing: Option<bool>,
    // The level to come back to when unmuting, so it is never a jump to full volume.
    muted_from: f32,
    _subscription: Subscription,
}

/// Anything at or below this is silence; f32 comparison needs a floor.
const SILENT: f32 = f32::EPSILON;
/// Where unmuting lands when nothing was remembered, rather than 100%.
const DEFAULT_UNMUTE: f32 = 0.5;

impl PlayerModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| {
            this.apply_event(event, cx)
        });
        Self {
            state: PlayerState::default(),
            observed_at: Instant::now(),
            optimistic_playing: None,
            muted_from: DEFAULT_UNMUTE,
            _subscription: subscription,
        }
    }

    fn apply_event(&mut self, event: &ApiEvent, cx: &mut Context<Self>) {
        match event {
            ApiEvent::PlayerState(state) => self.apply_state((**state).clone(), cx),
            ApiEvent::PlayerPosition {
                token,
                position_ms,
                at_ms,
                playing,
            } => {
                self.observed_at = Instant::now();
                self.state.now_ms = *at_ms;
                if Some(*token) == self.token() {
                    self.state.position = Some(PositionAnchor {
                        ms: *position_ms,
                        at_ms: *at_ms,
                        playing: *playing,
                    });
                } else if let Some(fading) = self
                    .state
                    .fading
                    .as_mut()
                    .filter(|fading| fading.from_token == *token)
                {
                    fading.position_ms = *position_ms;
                }
                crate::notify(cx);
            }
            ApiEvent::PlayerBuffered { token, ranges } if Some(*token) == self.token() => {
                self.state.buffered = ranges.clone();
                crate::notify(cx);
            }
            ApiEvent::Resync => self.refetch(cx),
            _ => {}
        }
    }

    fn refetch(&mut self, cx: &mut Context<Self>) {
        let call = Link::global(cx).call(|api| async move { api.player_state().await });
        cx.spawn(async move |this, cx| {
            if let Ok(state) = call.await {
                this.update(cx, |this, cx| this.apply_state(state, cx)).ok();
            }
        })
        .detach();
    }

    fn apply_state(&mut self, state: PlayerState, cx: &mut Context<Self>) {
        self.state = state;
        self.observed_at = Instant::now();
        self.optimistic_playing = None;
        crate::notify(cx);
    }

    fn token(&self) -> Option<u64> {
        match self.state.intent {
            Intent::Stopped => None,
            Intent::Loading { token, .. } | Intent::Committed { token } => Some(token),
        }
    }

    fn daemon_now_ms(&self) -> u64 {
        self.state.now_ms + self.observed_at.elapsed().as_millis() as u64
    }

    pub fn state(&self) -> &PlayerState {
        &self.state
    }

    /// What to show as playing: the outgoing track during a crossfade, else the current one.
    pub fn track(&self) -> Option<&NowPlaying> {
        match &self.state.fading {
            Some(fading) => Some(&fading.track),
            None => self.state.track.as_ref(),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.optimistic_playing.unwrap_or(match self.state.intent {
            Intent::Loading { .. } => true,
            Intent::Committed { .. } => self.state.phase == Phase::Playing,
            Intent::Stopped => false,
        })
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state.intent, Intent::Loading { .. })
    }

    pub fn is_radio(&self) -> bool {
        self.track()
            .is_some_and(|track| track.kind == TrackKind::Radio)
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.track().and_then(|track| track.duration_ms)
    }

    pub fn position_ms(&self) -> u64 {
        if let Some(fading) = &self.state.fading {
            return fading.position_ms;
        }
        let Some(anchor) = self.state.position else {
            return 0;
        };
        let position = if anchor.playing {
            anchor.ms + self.daemon_now_ms().saturating_sub(anchor.at_ms)
        } else {
            anchor.ms
        };
        self.duration_ms()
            .map_or(position, |duration| position.min(duration))
    }

    pub fn buffered(&self) -> &[BufferedRange] {
        &self.state.buffered
    }

    pub fn volume(&self) -> f32 {
        self.state.volume
    }

    pub fn shuffle(&self) -> bool {
        self.state.queue.shuffle
    }

    pub fn loop_mode(&self) -> LoopMode {
        self.state.queue.loop_mode
    }

    pub fn error(&self) -> Option<&api::ErrorBody> {
        self.state.error.as_ref()
    }

    pub fn external(&self) -> Option<&ExternalPlayback> {
        self.state.external.as_ref()
    }

    pub fn output_latency_ms(&self) -> Option<u64> {
        self.state.output_latency_ms
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.optimistic_playing = Some(!self.is_playing());
        crate::notify(cx);
        send(PlayerCommand::Toggle, cx);
    }

    pub fn play(&mut self, cx: &mut Context<Self>) {
        self.optimistic_playing = Some(true);
        crate::notify(cx);
        send(PlayerCommand::Play, cx);
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.optimistic_playing = Some(false);
        crate::notify(cx);
        send(PlayerCommand::Pause, cx);
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        send(PlayerCommand::Next, cx);
    }

    pub fn previous(&mut self, cx: &mut Context<Self>) {
        send(PlayerCommand::Previous, cx);
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        send(PlayerCommand::Stop, cx);
    }

    pub fn seek(&mut self, position_ms: u64, cx: &mut Context<Self>) {
        self.state.position = Some(PositionAnchor {
            ms: position_ms,
            at_ms: self.daemon_now_ms(),
            playing: self.is_playing(),
        });
        crate::notify(cx);
        send(PlayerCommand::Seek { position_ms }, cx);
    }

    /// `volume` is 0.0..=1.0.
    pub fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.state.volume = volume.clamp(0.0, 1.0);
        crate::notify(cx);
        send(
            PlayerCommand::SetVolume {
                volume: self.state.volume,
            },
            cx,
        );
    }

    /// The same, and remembered: this is the level the app opens at next time.
    pub fn set_volume_saved(&mut self, volume: f32, cx: &mut Context<Self>) {
        let volume = volume.clamp(0.0, 1.0);
        self.set_volume(volume, cx);
        crate::ConfigModel::global(cx).update(cx, |config, cx| {
            if config.is_loaded() && config.config().volume != volume {
                config.update(cx, |config| config.volume = volume);
            }
        });
    }

    /// Mute, or come back to the level muting left. Unmuting must never be a jump to full
    /// volume: someone wearing headphones pays for that.
    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let level = self.volume();
        if level > SILENT {
            self.muted_from = level;
            self.set_volume_saved(0., cx);
        } else if self.muted_from > SILENT {
            self.set_volume_saved(self.muted_from, cx);
        } else {
            self.set_volume_saved(DEFAULT_UNMUTE, cx);
        }
    }

    /// Nudge by `step`, for a wheel over a volume control.
    pub fn nudge_volume(&mut self, step: f32, cx: &mut Context<Self>) {
        self.set_volume_saved(self.volume() + step, cx);
    }

    pub fn set_shuffle(&mut self, shuffle: bool, cx: &mut Context<Self>) {
        self.state.queue.shuffle = shuffle;
        crate::notify(cx);
        send(
            PlayerCommand::SetMode {
                shuffle: Some(shuffle),
                loop_mode: None,
            },
            cx,
        );
    }

    pub fn cycle_loop_mode(&mut self, cx: &mut Context<Self>) {
        let loop_mode = self.state.queue.loop_mode.next();
        self.state.queue.loop_mode = loop_mode;
        crate::notify(cx);
        send(
            PlayerCommand::SetMode {
                shuffle: None,
                loop_mode: Some(loop_mode),
            },
            cx,
        );
    }
}

fn send(command: PlayerCommand, cx: &App) {
    Link::global(cx).fire(cx, "player command", move |api| async move {
        api.player_command(command).await
    });
}

/// Notifies a few times a second while playback runs; observe it (not [`PlayerModel`]) for anything that tracks the position.
pub struct Playhead {
    playing: bool,
    visible: bool,
    _ticker: Option<Task<()>>,
    _visibility: Option<Subscription>,
    _subscription: Subscription,
}

impl Playhead {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let player = PlayerModel::global(cx);
        let subscription = cx.observe(&player, |this, player, cx| {
            this.playing = player.read(cx).is_playing();
            this.retick(cx);
            crate::notify(cx);
        });
        Self {
            playing: false,
            visible: true,
            _ticker: None,
            _visibility: None,
            _subscription: subscription,
        }
    }

    /// The tick runs only while something is moving and someone can see it.
    fn retick(&mut self, cx: &mut Context<Self>) {
        match (self.playing && self.visible, self._ticker.is_some()) {
            (true, false) => self._ticker = Some(Self::tick(cx)),
            (false, true) => self._ticker = None,
            _ => {}
        }
    }

    fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.retick(cx);
        // Back on screen the position is up to a tick behind: catch it up rather than wait.
        if visible {
            crate::notify(cx);
        }
    }

    fn tick(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PLAYHEAD_TICK).await;
                let unwatched = this.update(cx, |this, cx| {
                    if this.visible {
                        crate::notify(cx);
                    }
                    this._visibility.is_none()
                });
                match unwatched {
                    Ok(true) => Self::watch_window(&this, cx),
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
        })
    }

    /// Installed on the first tick that finds a window, since the models are built before there is one.
    fn watch_window(this: &gpui_kit::WeakEntity<Self>, cx: &mut gpui_kit::AsyncApp) {
        let Some(playhead) = this.upgrade() else {
            return;
        };
        cx.update(|cx| {
            let Some(handle) = cx.windows().first().copied() else {
                return;
            };
            handle
                .update(cx, |_, window, cx| {
                    playhead.update(cx, |this, cx| {
                        this.visible = window.is_visible();
                        tracing::debug!(visible = this.visible, "playhead watching the window");
                        this._visibility = Some(cx.observe_window_visibility(
                            window,
                            |this, visibility, _, cx| {
                                this.set_visible(visibility.is_visible(), cx);
                            },
                        ));
                    });
                })
                .ok();
        });
    }
}
