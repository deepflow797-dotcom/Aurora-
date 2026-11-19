//! Lyrics for the Android app, driven by the shared view model.
//!
//! The important part is what this module does **not** do: it does not decide
//! which line is lit, how far the column has scrolled, or which words are
//! highlighted. All of that is [`aurora_ui::LyricView`], the same toolkit-free
//! state machine the desktop renderer will consume, fed by the same
//! [`aurora_core::clock::PlaybackClock`]. Compose turns the plan into pixels; it
//! does not own the animation, so the phone and the desktop cannot drift apart.
//!
//! Discovery is [`aurora_lyrics::discover`]: a sidecar `.lrc` next to the audio
//! file, then `<artist> - <title>.lrc` in the app's lyrics folder. When the file
//! only has line timings, [`aurora_lyrics::estimate_word_timing`] distributes the
//! words inside the line (spec 63.3), so the UI always reads [`TimedWord`]s and
//! never learns which granularity it got.

use std::path::Path;
use std::time::Instant;

use aurora_core::clock::ClockSnapshot;
use aurora_core::lyrics::{LyricDocument, LyricSource, Millis};
use aurora_core::motion::word_envelope;
use aurora_core::traits::PathStorage;
use aurora_lyrics::{discover, estimate_word_timing, Lookup};
use aurora_ui::{FramePlan, Layout, LyricView};
use serde_json::{json, Value};

/// A lyric document plus the view state that animates it.
#[derive(Debug)]
pub struct LyricsSession {
    doc: LyricDocument,
    view: LyricView,
    source: LyricSource,
}

impl LyricsSession {
    /// Wraps a parsed document, estimating word timings when it has none.
    #[must_use]
    pub fn new(mut doc: LyricDocument) -> Self {
        estimate_word_timing(&mut doc);
        let source = doc.source;
        Self {
            doc,
            view: LyricView::new(Layout::default()),
            source,
        }
    }

    /// Number of lines in the document.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.doc.lines.len()
    }

    /// True when word-level timings exist (real or estimated).
    #[must_use]
    pub fn is_word_timed(&self) -> bool {
        self.doc.word_timed
    }

    /// Where the document came from, for the "source" label.
    #[must_use]
    pub fn source(&self) -> LyricSource {
        self.source
    }

    /// Advances the shared view model one frame and returns the plan.
    pub fn frame(&mut self, snapshot: ClockSnapshot, now: Instant) -> FramePlan {
        self.view.frame(&self.doc, snapshot, now)
    }

    /// Everything Compose needs to draw the lyric column, as JSON.
    ///
    /// Per line: text, start/end, and the highlight strength `0.0..=1.0`. Per word
    /// of the active line: its own strength, so a karaoke fill is a text colour
    /// change in Kotlin rather than a second animation system.
    pub fn to_json(&mut self, snapshot: ClockSnapshot, now: Instant) -> Value {
        let at = Millis(snapshot.position_at(now).as_millis() as u64);
        let plan = self.view.frame(&self.doc, snapshot, now);
        let fade = self.view.layout().highlight_fade;

        let lines: Vec<Value> = self
            .doc
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let is_active = plan.active_line == Some(index);
                let words: Vec<Value> = line
                    .words
                    .iter()
                    .enumerate()
                    .map(|(word_index, word)| {
                        let activation = plan
                            .words
                            .iter()
                            .find(|planned| planned.line == index && planned.word == word_index)
                            .map(|planned| planned.activation)
                            .unwrap_or(0.0);
                        json!({
                            "text": &word.text,
                            "activation": activation,
                            // Word-level blur (KaraokeLine / AuroraLyricPreviewBox)
                            // needs the singing window, not just the fade-in
                            // activation: blurProgress = (now - start) / (end - start).
                            // Sent since the word-blur rebuild; older Kotlin
                            // parsers ignore unknown fields, so this is additive.
                            "startMs": word.start.0,
                            "endMs": word.end.0,
                        })
                    })
                    .collect();
                let activation = if is_active {
                    words
                        .iter()
                        .filter_map(|word| word["activation"].as_f64())
                        .fold(0.0_f64, f64::max) as f32
                } else {
                    0.0
                };
                json!({
                    "text": line.text(),
                    "translation": line.translation.clone(),
                    "startMs": line.start.0,
                    "endMs": line.end.0,
                    "active": is_active,
                    "activation": activation,
                    "words": words,
                })
            })
            .collect();

        json!({
            "available": !self.doc.is_empty(),
            "source": self.source.label(),
            "wordTimed": self.doc.word_timed,
            "lineCount": self.doc.lines.len(),
            "activeLine": plan.active_line,
            "scrollPx": plan.scroll_y,
            "positionMs": at.0,
            "fadeMs": fade.0,
            "lines": lines,
        })
    }
}

/// Loads lyrics for `audio_path`, if there are any.
///
/// Returns the session, or the list of paths that were tried - the app shows that
/// list under "no lyrics", because "no lyrics" is not the same as "I looked in the
/// wrong folder".
#[must_use]
pub fn load(audio_path: &Path, lyrics_dir: &Path) -> Result<LyricsSession, Vec<String>> {
    let track = track_for(audio_path);
    match discover(&PathStorage, &track, lyrics_dir) {
        Lookup::Found(doc) => Ok(LyricsSession::new(doc)),
        Lookup::Missing { tried } => Err(tried),
    }
}

/// The minimal [`aurora_core::model::Track`] lyric discovery needs.
fn track_for(audio_path: &Path) -> aurora_core::model::Track {
    use aurora_core::model::{Album, Artist, Track, TrackId, TrackLocation};
    Track {
        id: TrackId(0),
        location: TrackLocation::Path(audio_path.to_path_buf()),
        title: crate::facade::title_from_path(audio_path),
        artist: Artist::new("Unknown Artist"),
        album: Album::default(),
        track_number: None,
        duration_ms: 0,
        sample_rate_hz: None,
        channels: None,
        codec: None,
    }
}

/// `0.0..=1.0` opacity for a line, from the shared envelope rather than a
/// Kotlin-side guess. `activation` 0 = dim, 1 = fully lit.
///
/// [`word_envelope`]'s opacity never drops below `0.55` (it is meant to modulate a
/// word that is already visible), so it is remapped onto `dim..=1.0` here - a line
/// that has not been reached yet has to actually look dim.
#[must_use]
pub fn line_opacity(activation: f32, dim: f32) -> f32 {
    let (_, opacity) = word_envelope(activation, 0.12);
    let lit = ((opacity - 0.55) / 0.45).clamp(0.0, 1.0);
    dim + (1.0 - dim) * lit
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::clock::PlaybackClock;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music")
            .join(name)
    }

    fn lyrics_dir() -> PathBuf {
        std::env::temp_dir().join(format!("aurora-lyrics-{}", std::process::id()))
    }

    #[test]
    fn the_flac_fixture_finds_its_sidecar() {
        let session = load(&fixture("00 - Fixture Steps.flac"), &lyrics_dir()).expect("lyrics");
        assert_eq!(session.source(), LyricSource::Sidecar);
        assert_eq!(session.line_count(), 4);
        assert!(session.is_word_timed(), "line-only files get estimated words");
    }

    #[test]
    fn the_first_line_keeps_its_arabic_in_logical_order() {
        let mut session = load(&fixture("00 - Fixture Steps.flac"), &lyrics_dir()).expect("lyrics");
        let clock = PlaybackClock::new();
        // The first line starts at 0.5 s; sit in the middle of it.
        clock.publish(Duration::from_millis(700), Instant::now());
        let json = session.to_json(clock.snapshot(), Instant::now());
        assert_eq!(json["activeLine"], 0);
        assert_eq!(json["lines"][0]["text"], "سَلامٌ على الدُّنيا");
        assert!(json["lines"][0]["activation"].as_f64().unwrap() > 0.0);
        // The other lines are not lit.
        assert_eq!(json["lines"][1]["activation"].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn the_active_line_follows_the_clock() {
        let mut session = load(&fixture("00 - Fixture Steps.flac"), &lyrics_dir()).expect("lyrics");
        let clock = PlaybackClock::new();
        for (at_ms, expected) in [(0_u64, Value::Null), (600, json!(0)), (3_100, json!(1))] {
            clock.publish(Duration::from_millis(at_ms), Instant::now());
            let json = session.to_json(clock.snapshot(), Instant::now());
            assert_eq!(json["activeLine"], expected, "at {at_ms} ms");
        }
    }

    #[test]
    fn word_activation_fills_inside_the_line() {
        let mut session = load(&fixture("00 - Fixture Steps.flac"), &lyrics_dir()).expect("lyrics");
        let clock = PlaybackClock::new();
        clock.publish(Duration::from_millis(520), Instant::now());
        let early = session.to_json(clock.snapshot(), Instant::now());
        clock.publish(Duration::from_millis(2_900), Instant::now());
        let late = session.to_json(clock.snapshot(), Instant::now());

        let lit = |value: &Value| -> usize {
            value["lines"][0]["words"]
                .as_array()
                .map(|words| {
                    words
                        .iter()
                        .filter(|word| word["activation"].as_f64().unwrap_or(0.0) > 0.0)
                        .count()
                })
                .unwrap_or(0)
        };
        assert!(
            lit(&late) >= lit(&early),
            "the line fills as it is sung: {} early, {} late",
            lit(&early),
            lit(&late)
        );
        assert!(lit(&late) > 0, "some words are lit by the end of the line");
    }

    #[test]
    fn a_track_without_lyrics_reports_what_was_tried() {
        let tried = load(&fixture("00 - Fixture Tone.wav"), &lyrics_dir()).expect_err("no lyrics");
        assert!(!tried.is_empty(), "{tried:?}");
        assert!(tried.iter().any(|path| path.ends_with(".lrc")));
    }

    #[test]
    fn word_timings_are_sent_to_the_ui_for_the_blur_effect() {
        // KaraokeLine / AuroraLyricPreviewBox compute
        // blurProgress = (nowMs - startMs) / (endMs - startMs) in Kotlin.
        // That needs real per-word windows in the JSON, not only the
        // 80 ms fade-in `activation`.
        let mut session = load(&fixture("00 - Fixture Steps.flac"), &lyrics_dir()).expect("lyrics");
        let clock = PlaybackClock::new();
        clock.publish(Duration::from_millis(700), Instant::now());
        let json = session.to_json(clock.snapshot(), Instant::now());

        // Top-level position is what Kotlin uses as `nowMs` for the blur law.
        assert_eq!(json["positionMs"], 700);

        let line = &json["lines"][0];
        let line_start = line["startMs"].as_u64().unwrap();
        let line_end = line["endMs"].as_u64().unwrap();
        let words = line["words"].as_array().expect("words array");
        assert!(!words.is_empty(), "first line should have words");
        let mut previous_end = line_start;
        for word in words {
            let start = word["startMs"].as_u64().expect("word startMs");
            let end = word["endMs"].as_u64().expect("word endMs");
            assert!(
                start >= previous_end,
                "words are in order: start {start} after previous end {previous_end}"
            );
            assert!(end > start, "word window is non-empty: {start}..{end}");
            assert!(
                end <= line_end,
                "word end {end} stays inside its line (ends {line_end})"
            );
            previous_end = end;
        }
    }

    #[test]
    fn opacity_is_dim_until_the_line_starts() {
        assert!((line_opacity(0.0, 0.5) - 0.5).abs() < 1e-6);
        assert!((line_opacity(1.0, 0.5) - 1.0).abs() < 1e-6);
        // Monotonic in between: a fade, not a step.
        let middle = line_opacity(0.5, 0.5);
        assert!(middle > 0.5 && middle < 1.0, "{middle}");
    }
}
