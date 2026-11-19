//! Lyric document model.
//!
//! Aurora supports two granularities in one model:
//!
//! * **line timed** (`.lrc` with `[mm:ss.xx]`, the overwhelming majority);
//! * **word timed** (enhanced LRC `<mm:ss.xx>` word tags, and future formats).
//!
//! When only line timing exists the animator falls back to *estimated* word
//! timings distributed inside the line (spec 63.3) - see
//! `aurora_lyrics::estimate`. The UI never learns which of the two it got, it
//! just reads [`TimedWord`]s.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Milliseconds from the start of the track.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Millis(pub u64);

impl Millis {
    #[must_use]
    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }

    #[must_use]
    pub fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.0;
        let (m, rest) = (total / 60_000, total % 60_000);
        write!(f, "{m}:{:02}.{:02}", rest / 1000, (rest % 1000) / 10)
    }
}

/// One word inside a lyric line, with its start/end inside the track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub start: Millis,
    pub end: Millis,
}

impl TimedWord {
    #[must_use]
    pub fn duration(&self) -> Millis {
        self.end.saturating_sub(self.start)
    }

    /// Highlight weight in `0.0..=1.0` at `at`, with a short fade-in.
    ///
    /// `fade` is the time the highlight takes to reach full strength; keeping it
    /// short (60-90 ms) is what makes karaoke text feel "sung" instead of
    /// stepped.
    #[must_use]
    pub fn activation(&self, at: Millis, fade: Millis) -> f32 {
        if at < self.start {
            return 0.0;
        }
        let since = at.saturating_sub(self.start).0 as f32;
        let fade = fade.0.max(1) as f32;
        (since / fade).clamp(0.0, 1.0)
    }
}

/// One displayed line of lyrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    /// Start of the line; equals the first word's start when word timing exists.
    pub start: Millis,
    /// End of the line; defaults to the next line's start when unknown.
    pub end: Millis,
    /// Words in *logical* order (left-to-right before BiDi reordering).
    pub words: Vec<TimedWord>,
    /// Optional translation shown under the original line.
    pub translation: Option<String>,
}

impl LyricLine {
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, w) in self.words.iter().enumerate() {
            if i > 0
                && !w
                    .text
                    .starts_with([' ', '\u{060C}', '.', ',', '!', '?', '\u{061F}'])
            {
                out.push(' ');
            }
            out.push_str(&w.text);
        }
        out
    }

    /// True when every word carries its own timing (enhanced LRC or better).
    #[must_use]
    pub fn is_word_timed(&self) -> bool {
        self.words.iter().all(|w| w.end > w.start)
    }
}

/// A whole lyric document for one track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricDocument {
    pub lines: Vec<LyricLine>,
    /// Offset applied to every timestamp (LRC `[offset:+/-ms]`).
    pub offset: i32,
    /// Where the document came from, for the "lyrics source" label.
    pub source: LyricSource,
    pub word_timed: bool,
}

impl LyricDocument {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Index of the line that should be highlighted at `at`.
    ///
    /// Returns `None` before the first line. Lines are sorted by `start`, which
    /// the parser guarantees.
    #[must_use]
    pub fn active_line(&self, at: Millis) -> Option<usize> {
        if self.lines.is_empty() || at < self.lines[0].start {
            return None;
        }
        let idx = self.lines.partition_point(|line| line.start <= at);
        idx.checked_sub(1)
    }

    /// Progress through the document in `0.0..=1.0`, used by the scroll
    /// auto-follow curve.
    #[must_use]
    pub fn progress(&self, at: Millis) -> f32 {
        let Some(last) = self.lines.last() else {
            return 0.0;
        };
        if last.end.0 == 0 {
            return 0.0;
        }
        (at.0 as f32 / last.end.0 as f32).clamp(0.0, 1.0)
    }
}

/// Where a lyric document was found (spec 63.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricSource {
    /// `<track>.lrc` next to the audio file.
    Sidecar,
    /// `<artist> - <title>.lrc` in the configured lyrics folder.
    LibraryFolder,
    /// Embedded in the audio file's tags (USLT/SYLT).
    Embedded,
    /// Nothing found.
    #[default]
    Missing,
}

impl LyricSource {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sidecar => "sidecar .lrc",
            Self::LibraryFolder => "lyrics folder",
            Self::Embedded => "embedded",
            Self::Missing => "no lyrics",
        }
    }
}
