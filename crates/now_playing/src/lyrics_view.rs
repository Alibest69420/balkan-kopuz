//! The lyrics tab: plain words, or the synced view that follows the playhead line by line.

use std::ops::Range;
use std::time::Instant;

use api::{LyricLineView, LyricsView as LyricsSource};
use components::format::t;
use components::{KaraokeText, icons, style};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Task, Window, div, point, px, relative, rems,
};
use store::{ConfigModel, PlayerModel, Playhead};

// Where the active line parks in the panel, as a fraction of its height.
const COMFORT_OFFSET: f32 = 0.42;
// A line without an end time keeps the light until the next one, if it follows this soon.
const SEAMLESS_GAP: f64 = 3.0;
const CHUNK_FALLBACK: f64 = 0.35;
// A silence shorter than this is a breath between lines, not an interlude.
const INTERLUDE_MIN: f64 = 5.0;
// Only some providers timestamp a line's end; a sung line rarely runs longer than this.
const LINE_ASSUMED_MAX: f64 = 7.0;
// A chunk runs until the next one starts, which over a pause is far longer than the syllable.
const MAX_WIPE: f64 = 1.2;
const BACKGROUND_ALPHA: f32 = 0.7;
const SLIDE_MS: f64 = 720.0;
// Past this the list has moved under us — a remount or a resize — so re-park rather than wait.
const REPARK_DRIFT: Pixels = px(24.);
// Depth of field as opacity, per line of distance and clamped: GPUI paints no blur over text.
const DEPTH_STEP: f32 = 0.09;
const DEPTH_MAX: f32 = 0.55;
const INTERLUDE_BAR: Pixels = px(72.);
// `text-white/40` unsung, `text-white/60` under the pointer: a line the voice has left.
const DIM: f32 = 0.4;
const DIM_HOVER: f32 = 0.6;
// A background line is quieter still: `text-white/25`, lit at `text-white/70`.
const BACKGROUND_DIM: f32 = 0.25;
// `min(100%, 20rem)`, and `min(90%, 18rem)` once a duet pushes the lines to the edges.
const LINE_WIDTH: f32 = 20.;
const TURN_WIDTH: f32 = 18.;
// `leading-snug` on the small print of a background line, `leading-relaxed` on unsynced words.
const SNUG: f32 = 1.375;
const RELAXED: f32 = 1.625;

/// A word or syllable of a line, with the byte range it occupies in the line's text.
#[derive(Clone, Debug, PartialEq)]
struct Chunk {
    start: f64,
    end: f64,
    range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct Line {
    start: f64,
    end: Option<f64>,
    text: SharedString,
    chunks: Vec<Chunk>,
    background: bool,
    opposite: bool,
    interlude: bool,
}

impl Line {
    /// An instrumental stretch, synthesized for a gap the provider left empty.
    fn interlude(start: f64, end: f64) -> Self {
        Self {
            start,
            end: Some(end),
            text: SharedString::default(),
            chunks: Vec::new(),
            background: false,
            opposite: false,
            interlude: true,
        }
    }

    /// Providers only timestamp a line's start, so the tail has to be guessed.
    fn end_estimate(&self) -> f64 {
        self.end
            .or_else(|| self.chunks.last().map(|chunk| chunk.end))
            .unwrap_or(self.start + LINE_ASSUMED_MAX)
    }
}

fn seconds(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// A chunk runs until the next one starts, the last one until the line ends; the wipe needs a span.
fn chunk_end(chunks: &[api::LyricChunkView], index: usize, line_end: Option<u64>) -> f64 {
    let start = seconds(chunks[index].start_ms);
    chunks
        .get(index + 1)
        .map(|next| seconds(next.start_ms))
        .or(line_end.map(seconds))
        .filter(|&end| end > start)
        .unwrap_or(start + CHUNK_FALLBACK)
}

fn convert(view: LyricLineView) -> Line {
    let mut text = String::new();
    let mut chunks = Vec::with_capacity(view.chunks.len());
    for index in 0..view.chunks.len() {
        let start = text.len();
        text.push_str(&view.chunks[index].text);
        chunks.push(Chunk {
            start: seconds(view.chunks[index].start_ms),
            end: chunk_end(&view.chunks, index, view.end_ms),
            range: start..text.len(),
        });
    }
    if chunks.is_empty() {
        text = view.text;
    }
    Line {
        start: seconds(view.start_ms),
        end: view.end_ms.map(seconds),
        text: text.into(),
        chunks,
        background: view.background,
        opposite: view.opposite_turn,
        interlude: false,
    }
}

/// Foreground lines carry the song; a file of nothing but background lines falls back to all of them.
fn main_indices(lines: &[Line]) -> Vec<usize> {
    let foreground = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (!line.background).then_some(index))
        .collect::<Vec<_>>();
    if foreground.is_empty() {
        (0..lines.len()).collect()
    } else {
        foreground
    }
}

/// Providers emit nothing for an instrumental stretch, so synthesize a line for every long gap.
fn with_interludes(lines: Vec<Line>) -> Vec<Line> {
    let main = main_indices(&lines);
    let mut gaps: Vec<(usize, f64, f64)> = Vec::new();

    if let Some(&first) = main.first()
        && lines[first].start >= INTERLUDE_MIN
    {
        gaps.push((first, 0.0, lines[first].start));
    }
    for pair in main.windows(2) {
        let (current, next) = (pair[0], pair[1]);
        let next_start = lines[next].start;
        // A background line sits after its parent and can outlast it, so the gap starts after the run.
        let gap_start = lines[current..next]
            .iter()
            .map(Line::end_estimate)
            .fold(f64::NEG_INFINITY, f64::max)
            .clamp(lines[current].start, next_start);
        if next_start - gap_start >= INTERLUDE_MIN {
            gaps.push((next, gap_start, next_start));
        }
    }
    if gaps.is_empty() {
        return lines;
    }

    let mut display = Vec::with_capacity(lines.len() + gaps.len());
    let mut gaps = gaps.into_iter().peekable();
    for (index, line) in lines.into_iter().enumerate() {
        while let Some(&(at, start, end)) = gaps.peek() {
            if at != index {
                break;
            }
            gaps.next();
            display.push(Line::interlude(start, end));
        }
        display.push(line);
    }
    display
}

fn next_main_start(lines: &[Line], main: &[usize], index: usize) -> Option<f64> {
    main.iter()
        .position(|&candidate| candidate == index)
        .and_then(|position| main.get(position + 1))
        .map(|&next| lines[next].start)
}

fn line_active_at(line: &Line, time: f64, next_start: Option<f64>) -> bool {
    if time < line.start {
        return false;
    }
    let Some(end) = line.end else {
        return next_start.is_none_or(|next| time < next);
    };
    if time <= end {
        return true;
    }
    next_start
        .filter(|&next| next > end && next - end <= SEAMLESS_GAP)
        .is_some_and(|next| time < next)
}

fn active_main_index(lines: &[Line], main: &[usize], time: f64) -> Option<usize> {
    main.iter()
        .copied()
        .take_while(|&index| lines[index].start <= time)
        .filter(|&index| line_active_at(&lines[index], time, next_main_start(lines, main, index)))
        .last()
}

/// A background line carries its own timing; without an end it runs until the next main line.
fn background_bound(lines: &[Line], main: &[usize], line: &Line) -> Option<f64> {
    if line.end.is_some() {
        return None;
    }
    main.iter()
        .map(|&index| lines[index].start)
        .find(|&start| start > line.start)
}

/// Lines lit alongside the active one: backing vocals and anything still overlapping it.
fn active_secondary(
    lines: &[Line],
    main: &[usize],
    time: f64,
    active: Option<usize>,
) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            let next = if line.background {
                background_bound(lines, main, line)
            } else {
                next_main_start(lines, main, *index)
            };
            Some(*index) != active
                && line_active_at(line, time, next)
                // A foreground line only stays lit alongside another one, never on its own.
                && (line.background || active.is_some())
        })
        .map(|(index, _)| index)
        .collect()
}

/// How far through its chunk the voice is, capped so a long tail does not creep through the silence.
fn chunk_fill(chunk: &Chunk, time: f64) -> f32 {
    let span = (chunk.end - chunk.start).min(MAX_WIPE);
    let fill = if span > 0.0 {
        (time - chunk.start) / span
    } else if time >= chunk.start {
        1.0
    } else {
        0.0
    };
    fill.clamp(0.0, 1.0) as f32
}

enum Lyrics {
    /// A station, or nothing to ask about yet.
    Idle,
    Loading,
    Plain(SharedString),
    Synced {
        lines: Vec<Line>,
        main: Vec<usize>,
    },
}

/// An eased glide of the scroll offset, so the active line slides into place instead of jumping.
struct Slide {
    from: Pixels,
    to: Pixels,
    started: Instant,
}

/// The lyrics tab: the words for what is playing, and the sync that follows them.
pub struct LyricsView {
    lyrics: Lyrics,
    key: Option<String>,
    generation: u64,
    scroll: ScrollHandle,
    auto_sync: bool,
    active: Option<usize>,
    slide: Option<Slide>,
    _fetch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl LyricsView {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let player = PlayerModel::global(cx);
        let playhead = Playhead::global(cx);
        let config = ConfigModel::global(cx);
        let subscriptions = vec![
            cx.observe(&player, |this, player, cx| this.track_changed(&player, cx)),
            cx.observe(&playhead, |_, _, cx| cx.notify()),
            cx.observe(&config, |_, _, cx| cx.notify()),
        ];
        Self {
            lyrics: Lyrics::Idle,
            key: None,
            generation: 0,
            scroll: ScrollHandle::new(),
            auto_sync: true,
            active: None,
            slide: None,
            _fetch: None,
            _subscriptions: subscriptions,
        }
    }

    fn track_changed(&mut self, player: &Entity<PlayerModel>, cx: &mut Context<Self>) {
        let player = player.read(cx);
        let radio = player.is_radio();
        let key = player.track().map(|track| track.key.clone());
        if key == self.key {
            cx.notify();
            return;
        }
        self.key = key.clone();
        self.auto_sync = true;
        self.active = None;
        self.slide = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.generation += 1;

        // A station's name only ever matches junk, so it is never asked about.
        match key.filter(|key| !key.is_empty() && !radio) {
            Some(key) => self.fetch(key, cx),
            None => {
                self.lyrics = Lyrics::Idle;
                self._fetch = None;
            }
        }
        cx.notify();
    }

    fn fetch(&mut self, key: String, cx: &mut Context<Self>) {
        self.lyrics = Lyrics::Loading;
        let generation = self.generation;
        // The checklist's bug: the words are looked up by the library key, never by the track uid.
        let call = link::Link::global(cx).call(move |api| async move { api.lyrics(key).await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let answer = call.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.lyrics = match answer {
                    Ok(view) => adopt(view),
                    Err(error) => {
                        tracing::debug!(%error, "no lyrics for this track");
                        Lyrics::Plain(t("lyrics_not_found"))
                    }
                };
                cx.notify();
            })
            .ok();
        }));
    }

    fn rearm(&mut self, cx: &mut Context<Self>) {
        self.auto_sync = true;
        self.start_slide();
        cx.notify();
    }

    /// Where the scroll offset has to sit for line `index` to park at the comfort offset.
    fn comfort_offset(&self, index: usize) -> Option<Pixels> {
        let bounds = self.scroll.bounds();
        if bounds.size.height <= px(0.) {
            return None;
        }
        // Children are the leading spacer, then one element per line.
        let line = self.scroll.bounds_for_item(index + 1)?;
        let target = bounds.size.height * COMFORT_OFFSET + bounds.origin.y - line.origin.y;
        let furthest = self.scroll.max_offset().y;
        Some(target.clamp(-furthest, px(0.)))
    }

    fn start_slide(&mut self) {
        let Some(target) = self.active.and_then(|index| self.comfort_offset(index)) else {
            return;
        };
        self.slide = Some(Slide {
            from: self.scroll.offset().y,
            to: target,
            started: Instant::now(),
        });
    }

    /// Whether the line being sung carries per-word timing, and so has a fill that wants frames.
    fn has_karaoke(&self, active: Option<usize>) -> bool {
        let (Lyrics::Synced { lines, .. }, Some(active)) = (&self.lyrics, active) else {
            return false;
        };
        lines.get(active).is_some_and(|line| line.chunks.len() > 1)
    }

    /// Advance the glide, answering whether another frame is owed.
    fn follow(&mut self, active: Option<usize>) -> bool {
        let moved = active != self.active;
        self.active = active;
        if !self.auto_sync {
            self.slide = None;
            return false;
        }
        if moved {
            self.start_slide();
        } else if self.slide.is_none()
            && let Some(target) = self.active.and_then(|index| self.comfort_offset(index))
            && (target - self.scroll.offset().y).abs() > REPARK_DRIFT
        {
            self.slide = Some(Slide {
                from: self.scroll.offset().y,
                to: target,
                started: Instant::now(),
            });
        }

        let Some(slide) = &self.slide else {
            // Before the first paint there are no bounds to park against, so ask for one more frame.
            return self
                .active
                .is_some_and(|index| self.comfort_offset(index).is_none());
        };
        let progress = (slide.started.elapsed().as_secs_f64() * 1000.0 / SLIDE_MS).min(1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        let y = slide.from + (slide.to - slide.from) * eased as f32;
        self.scroll.set_offset(point(self.scroll.offset().x, y));
        if progress >= 1.0 {
            self.slide = None;
        }
        true
    }

    fn scroller(&self, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id("lyrics-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .px_2()
            .py_2()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                if this.auto_sync {
                    this.auto_sync = false;
                    this.slide = None;
                    cx.notify();
                }
            }))
    }
}

fn adopt(view: LyricsSource) -> Lyrics {
    if let Some(plain) = view.plain.filter(|plain| !plain.trim().is_empty()) {
        return Lyrics::Plain(plain.into());
    }
    if view.synced.is_empty() {
        return Lyrics::Plain(t("lyrics_not_found"));
    }
    let lines = with_interludes(view.synced.into_iter().map(convert).collect());
    let main = main_indices(&lines);
    Lyrics::Synced { lines, main }
}

/// One rendered line: its own colouring, its karaoke fill, and the seek its click asks for.
#[allow(clippy::too_many_arguments)]
fn line_element(
    index: usize,
    line: &Line,
    lit: bool,
    opacity: f32,
    time: f64,
    has_opposite: bool,
    sung: Hsla,
    cx: &mut Context<LyricsView>,
) -> AnyElement {
    let start_ms = (line.start.max(0.0) * 1000.0) as u64;
    let tint = if line.background {
        BACKGROUND_ALPHA
    } else {
        1.0
    };
    let base = if lit {
        sung.alpha(tint)
    } else if line.background {
        sung.alpha(BACKGROUND_DIM)
    } else {
        sung.alpha(DIM)
    };
    let turned = line.opposite || has_opposite;

    let body = div()
        .id(("lyric", index))
        .w(if turned { relative(0.9) } else { relative(1.) })
        .max_w(rems(if turned { TURN_WIDTH } else { LINE_WIDTH }))
        .py_0p5()
        .opacity(opacity)
        .text_color(base)
        .when(line.background, |row| {
            row.pl_4()
                .text_sm()
                .font_medium()
                .line_height(relative(SNUG))
        })
        .when(!line.background, |row| row.font_semibold())
        // The `scale(1.12)` the lit Dioxus line grows by is a transform GPUI has not; a size step stands in.
        .when(!line.background && lit, |row| row.text_xl())
        .when(!line.background && !lit, |row| row.text_lg())
        .when(line.opposite, |row| row.text_right().italic().ml_auto())
        .when(!line.opposite && has_opposite, |row| row.text_left())
        .when(!line.opposite && !has_opposite, |row| {
            row.text_center().mx_auto()
        })
        .when(!lit, |row| {
            row.hover(move |row| row.text_color(sung.alpha(DIM_HOVER * tint)))
        })
        .on_click(move |_, _, cx| {
            PlayerModel::global(cx).update(cx, |player, cx| player.seek(start_ms, cx));
        });

    if line.interlude {
        let span = line.end.unwrap_or(line.start) - line.start;
        let progress = if span > 0.0 {
            ((time - line.start) / span).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        return body
            .flex()
            .py_2()
            .items_center()
            .gap_2()
            .when(!has_opposite, |row| row.justify_center())
            .child(icons::icon(icons::MUSIC).size(px(16.)))
            .child(
                div()
                    .w(INTERLUDE_BAR)
                    .h(px(3.))
                    .rounded_full()
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .h(px(3.))
                            .w(INTERLUDE_BAR * progress)
                            .rounded_full()
                            .bg(sung.alpha(tint)),
                    ),
            )
            .into_any_element();
    }

    if line.chunks.is_empty() || !lit {
        return body.child(line.text.clone()).into_any_element();
    }

    let fills = line
        .chunks
        .iter()
        .map(|chunk| (chunk.range.clone(), chunk_fill(chunk, time)))
        .collect();
    body.child(KaraokeText::new(
        line.text.clone(),
        sung.alpha(tint),
        sung.alpha(DIM * tint),
        fills,
    ))
    .into_any_element()
}

/// Every line but the lit ones recedes with distance from the anchor.
fn depth_opacity(index: usize, anchor: Option<usize>, lit: bool, strength: f32) -> f32 {
    let Some(anchor) = anchor.filter(|_| !lit && strength > 0.0) else {
        return 1.0;
    };
    let distance = index.abs_diff(anchor) as f32;
    1.0 - (distance * DEPTH_STEP * strength).min(DEPTH_MAX * strength)
}

impl Render for LyricsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = PlayerModel::global(cx).read(cx);
        let playing = player.is_playing();
        let position = player.position_ms();
        let latency = player.output_latency_ms().unwrap_or_default();

        let config = ConfigModel::global(cx).read(cx);
        let config = config.config();
        // The clock runs ahead of the speakers; hold the lyrics back by whatever the output costs.
        let offset = if config.lyrics_offset_auto {
            seconds(latency)
        } else {
            f64::from(config.lyrics_offset_ms) / 1000.0
        };
        let strength = if config.lyrics_depth_blur {
            f32::from(config.lyrics_depth_blur_strength) / 100.0
        } else {
            0.0
        };
        let time = position as f64 / 1000.0 - offset;

        let (mut active, mut lit) = (None, Vec::new());
        if let Lyrics::Synced { lines, main } = &self.lyrics {
            active = active_main_index(lines, main, time);
            lit = active_secondary(lines, main, time, active);
        }
        let settling = self.follow(active);
        // Nothing lit anchors the depth on the last line still ringing out.
        let anchor = active.or_else(|| lit.iter().copied().max());
        // The sung words wear the cover's colour, clamped until it is legible on the background.
        let base = cx.theme().foreground;
        let background = cx.theme().background;
        let sung = theme::readable_on(
            components::motion::accent(theme::Surface::Sheet, base, window, cx),
            background,
        );

        let body = match &self.lyrics {
            Lyrics::Idle => div().into_any_element(),
            Lyrics::Loading => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(style::white(BACKGROUND_ALPHA, cx))
                .child(t("loading_lyrics"))
                .into_any_element(),
            Lyrics::Plain(text) => self
                .scroller(cx)
                .py_4()
                .px_4()
                .text_sm()
                .text_center()
                .font_medium()
                .line_height(relative(RELAXED))
                .text_color(style::white(BACKGROUND_ALPHA, cx))
                .child(text.clone())
                .into_any_element(),
            Lyrics::Synced { lines, .. } => {
                let has_opposite = lines.iter().any(|line| line.opposite);
                let height = self.scroll.bounds().size.height;
                let children = lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| {
                        let is_lit = Some(index) == active || lit.contains(&index);
                        let opacity = depth_opacity(index, anchor, is_lit, strength);
                        line_element(index, line, is_lit, opacity, time, has_opposite, sung, cx)
                    })
                    .collect::<Vec<_>>();
                self.scroller(cx)
                    .child(div().h(height * COMFORT_OFFSET))
                    .children(children)
                    .child(div().h(height * (1.0 - COMFORT_OFFSET)))
                    .into_any_element()
            }
        };

        let synced = matches!(self.lyrics, Lyrics::Synced { .. });
        // A frame request re-renders the whole window: ask only while the glide or a fill moves.
        let filling = playing && self.has_karaoke(active);
        if synced && (filling || settling) {
            window.request_animation_frame();
        }

        v_flex()
            .relative()
            .size_full()
            .child(body)
            .when(synced && !self.auto_sync, |panel| {
                panel.child(
                    components::FollowButton::new("lyrics-sync")
                        .on_click(cx.listener(|this, _, _, cx| this.rearm(cx))),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start: f64, end: Option<f64>) -> Line {
        Line {
            start,
            end,
            text: "la".into(),
            chunks: Vec::new(),
            background: false,
            opposite: false,
            interlude: false,
        }
    }

    fn background(start: f64, end: Option<f64>) -> Line {
        Line {
            background: true,
            ..line(start, end)
        }
    }

    #[test]
    fn marks_the_intro_and_the_instrumental_gaps() {
        let lines = with_interludes(vec![
            line(12.0, Some(15.0)),
            line(40.0, Some(43.0)),
            line(45.0, Some(48.0)),
        ]);

        let marks: Vec<bool> = lines.iter().map(|line| line.interlude).collect();
        assert_eq!(marks, vec![true, false, true, false, false]);
        assert_eq!(lines[0].start, 0.0);
        assert_eq!(lines[0].end, Some(12.0));
        assert_eq!(lines[2].start, 15.0);
        assert_eq!(lines[2].end, Some(40.0));
    }

    #[test]
    fn the_gap_starts_after_a_background_line_outlasts_its_parent() {
        let lines = with_interludes(vec![
            line(1.0, Some(4.0)),
            background(3.0, Some(9.0)),
            line(30.0, Some(33.0)),
        ]);

        let marks: Vec<bool> = lines.iter().map(|line| line.interlude).collect();
        assert_eq!(marks, vec![false, false, true, false]);
        assert_eq!(lines[2].start, 9.0);
    }

    #[test]
    fn a_background_line_stays_lit_when_the_next_main_line_starts() {
        let lines = vec![
            line(63.167, Some(67.299)),
            background(65.48, Some(67.299)),
            line(66.236, Some(70.567)),
        ];
        let main = main_indices(&lines);

        assert_eq!(active_main_index(&lines, &main, 66.5), Some(2));
        assert_eq!(active_secondary(&lines, &main, 66.5, Some(2)), vec![0, 1]);
        assert!(active_secondary(&lines, &main, 67.5, Some(2)).is_empty());
    }

    #[test]
    fn an_untimed_background_line_runs_until_the_next_main_line() {
        let lines = vec![
            line(1.0, Some(2.0)),
            background(1.5, None),
            line(4.0, Some(6.0)),
        ];
        let main = main_indices(&lines);

        assert_eq!(active_secondary(&lines, &main, 3.0, None), vec![1]);
        assert!(active_secondary(&lines, &main, 4.5, Some(2)).is_empty());
    }

    #[test]
    fn lyrics_without_a_long_gap_are_left_alone() {
        let lines = vec![line(1.0, Some(4.0)), line(5.0, Some(8.0))];

        assert_eq!(with_interludes(lines.clone()), lines);
    }

    #[test]
    fn a_chunk_lights_over_its_own_span_and_then_holds() {
        let chunk = Chunk {
            start: 1.0,
            end: 1.5,
            range: 0..2,
        };

        assert_eq!(chunk_fill(&chunk, 0.5), 0.0);
        assert_eq!(chunk_fill(&chunk, 1.25), 0.5);
        assert_eq!(chunk_fill(&chunk, 9.0), 1.0);
    }

    #[test]
    fn depth_leaves_the_lit_lines_alone_and_clamps_the_far_ones() {
        assert_eq!(depth_opacity(4, Some(4), true, 1.0), 1.0);
        assert_eq!(depth_opacity(9, Some(4), false, 0.0), 1.0);
        assert!(depth_opacity(5, Some(4), false, 1.0) > depth_opacity(9, Some(4), false, 1.0));
        assert_eq!(depth_opacity(99, Some(4), false, 1.0), 1.0 - DEPTH_MAX);
    }
}
