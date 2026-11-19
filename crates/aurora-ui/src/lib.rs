//! Toolkit-independent view model.
//!
//! "Which words are lit, where is the viewport, what is the scroll offset" is
//! decided here, once, in plain Rust. The renderer (iced on desktop, Compose on
//! Android under Strategy B) only turns [`FramePlan`] into pixels. This is the
//! single most important structural decision for the Android port: the lyric
//! animation is not reimplemented in Kotlin, it is *driven* from shared code.

use std::time::Duration;

use aurora_core::clock::ClockSnapshot;
use aurora_core::lyrics::{LyricDocument, Millis};
use aurora_core::motion::{Spring, word_envelope};

/// Layout constants the spike and the real UI share.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub viewport_height: f32,
    pub line_height: f32,
    /// Fraction of the viewport height the active line is pinned at.
    pub anchor: f32,
    pub highlight_fade: Millis,
    pub highlight_scale: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            viewport_height: 720.0,
            line_height: 96.0,
            anchor: 0.42,
            highlight_fade: Millis(80),
            highlight_scale: 0.12,
        }
    }
}

/// Per-word state for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordPlan {
    pub line: usize,
    pub word: usize,
    /// `0.0..=1.0` highlight strength.
    pub activation: f32,
    pub scale: f32,
    pub opacity: f32,
}

/// Everything a renderer needs for one frame.
#[derive(Debug, Clone, Default)]
pub struct FramePlan {
    pub active_line: Option<usize>,
    /// Vertical scroll offset of the whole lyric column, in pixels.
    pub scroll_y: f32,
    pub words: Vec<WordPlan>,
}

/// The lyric screen state machine: spring-scrolled, clock-driven, no toolkit.
#[derive(Debug)]
pub struct LyricView {
    layout: Layout,
    scroll: Spring,
    last_now: Option<std::time::Instant>,
    last_dt: Duration,
}

impl LyricView {
    #[must_use]
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            scroll: Spring::scroll(0.0),
            last_now: None,
            last_dt: Duration::ZERO,
        }
    }

    #[must_use]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    #[must_use]
    pub fn scroll_position(&self) -> f32 {
        self.scroll.position
    }

    /// Advances the animation and produces the frame's plan.
    ///
    /// `snapshot` comes from [`aurora_core::clock::PlaybackClock`], `now` is the
    /// frame's monotonic timestamp - both required, so the view never guesses
    /// time from a frame counter.
    pub fn frame(
        &mut self,
        doc: &LyricDocument,
        snapshot: ClockSnapshot,
        now: std::time::Instant,
    ) -> FramePlan {
        let dt = match self.last_now {
            // First frame has no measurable delta; assume one 60 Hz period so the
            // spring does not stay at rest for a frame.
            None => Duration::from_millis(16),
            Some(prev) => now
                .saturating_duration_since(prev)
                .max(Duration::from_micros(1)),
        };
        self.last_now = Some(now);
        self.last_dt = dt;

        let at = Millis(snapshot.position_at(now).as_millis() as u64);
        let active = doc.active_line(at);

        let target = active.map_or(0.0, |idx| {
            let centered = idx as f32 * self.layout.line_height;
            centered - self.layout.viewport_height * self.layout.anchor
                + self.layout.line_height * 0.5
        });
        self.scroll.advance(target.max(0.0), dt);

        let mut words = Vec::new();
        if let Some(idx) = active
            && let Some(line) = doc.lines.get(idx)
        {
            for (wi, word) in line.words.iter().enumerate() {
                let activation = word.activation(at, self.layout.highlight_fade);
                if activation <= 0.0 {
                    continue;
                }
                let (scale, opacity) = word_envelope(activation, self.layout.highlight_scale);
                words.push(WordPlan {
                    line: idx,
                    word: wi,
                    activation,
                    scale,
                    opacity,
                });
            }
        }

        FramePlan {
            active_line: active,
            scroll_y: self.scroll.position,
            words,
        }
    }

    /// Time since the previous frame, for diagnostics.
    #[must_use]
    pub fn last_frame_dt(&self) -> Duration {
        self.last_dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::lyrics::{LyricLine, LyricSource, TimedWord};
    use std::time::Instant;

    fn doc() -> LyricDocument {
        let lines = (0..40)
            .map(|i| LyricLine {
                start: Millis(i * 2_000),
                end: Millis(i * 2_000 + 1_800),
                words: vec![
                    TimedWord {
                        text: "سَلامٌ".into(),
                        start: Millis(i * 2_000),
                        end: Millis(i * 2_000 + 800),
                    },
                    TimedWord {
                        text: "Aurora".into(),
                        start: Millis(i * 2_000 + 800),
                        end: Millis(i * 2_000 + 1_800),
                    },
                ],
                translation: None,
            })
            .collect();
        LyricDocument {
            lines,
            offset: 0,
            source: LyricSource::Sidecar,
            word_timed: true,
        }
    }

    #[test]
    fn highlights_the_line_under_the_playhead() {
        let mut view = LyricView::new(Layout::default());
        let doc = doc();
        let now = Instant::now();
        let snap = ClockSnapshot {
            position: Duration::from_millis(4_900),
            sampled_at: now,
            playing: false,
        };
        let plan = view.frame(&doc, snap, now);
        assert_eq!(plan.active_line, Some(2));
        assert!(!plan.words.is_empty());
    }

    #[test]
    fn scroll_follows_the_active_line() {
        let mut view = LyricView::new(Layout::default());
        let doc = doc();
        let start = Instant::now();
        let mut last = 0.0;
        for frame in 0..120 {
            let now = start + Duration::from_millis(frame * 16);
            let snap = ClockSnapshot {
                position: Duration::from_millis(30_000),
                sampled_at: start,
                playing: false,
            };
            let plan = view.frame(&doc, snap, now);
            assert!(plan.scroll_y >= last - 0.001, "scroll went backwards");
            last = plan.scroll_y;
        }
        assert!(last > 100.0, "scroll should have moved, got {last}");
    }
}
