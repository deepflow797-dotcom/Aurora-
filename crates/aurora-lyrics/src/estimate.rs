//! Estimated word timing for line-timed lyrics (spec 63.3).
//!
//! Most `.lrc` files only say *when a line starts*. Aurora still wants the
//! per-word highlight, so it distributes the line's duration across its words
//! using a weight that approximates sung duration:
//!
//! * each non-diacritic character is one unit (Arabic harakat are skipped, they
//!   are vowel marks that do not add a syllable);
//! * a word with a long vowel (ا و ي) or a shadda is slightly longer;
//! * a trailing space adds a small gap, capped, so long gaps do not swallow the
//!   line;
//! * every word gets at least [`MIN_WORD_MS`] so the highlight is visible.
//!
//! This is a *display* heuristic. When real word timing exists (enhanced LRC),
//! none of this runs.

use aurora_core::lyrics::{LyricDocument, LyricLine, Millis, TimedWord};

/// Shortest visible highlight for a word.
pub const MIN_WORD_MS: u64 = 90;

/// Weight of one Arabic diacritic: they contribute almost nothing.
const DIACRITIC_WEIGHT: f32 = 0.05;
/// Weight of a base character.
const BASE_WEIGHT: f32 = 1.0;
/// Extra weight for a long vowel letter.
const LONG_VOWEL_BONUS: f32 = 0.35;
/// Weight of a trailing space (sung silence), capped by [`MAX_GAP_WEIGHT`].
const SPACE_WEIGHT: f32 = 0.5;
const MAX_GAP_WEIGHT: f32 = 1.5;

/// Is `c` an Arabic diacritic (harakat / tanwin / shadda / sukun)?
#[must_use]
pub fn is_arabic_diacritic(c: char) -> bool {
    matches!(c as u32, 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED)
}

/// Estimates per-word timings for a line whose start/end are known.
///
/// Returns the words unchanged when the line already has usable word timing.
#[must_use]
pub fn distribute_line(line: &LyricLine) -> Vec<TimedWord> {
    if line.is_word_timed() {
        return line.words.clone();
    }
    let total = line.end.saturating_sub(line.start).0;
    if line.words.is_empty() {
        return Vec::new();
    }
    if line.words.len() == 1 && total <= MIN_WORD_MS {
        return vec![TimedWord {
            text: line.words[0].text.clone(),
            start: line.start,
            end: Millis(line.start.0 + total.max(MIN_WORD_MS)),
        }];
    }

    let weights: Vec<f32> = line.words.iter().map(|w| word_weight(&w.text)).collect();
    let sum: f32 = weights.iter().sum::<f32>().max(0.001);

    let mut out = Vec::with_capacity(line.words.len());
    let mut cursor = line.start.0 as f64;
    let span = total.max(MIN_WORD_MS * line.words.len() as u64) as f64;
    for (word, weight) in line.words.iter().zip(&weights) {
        let dur = (span * f64::from(*weight / sum)).max(MIN_WORD_MS as f64);
        let start = cursor as u64;
        let end = (cursor + dur) as u64;
        out.push(TimedWord {
            text: word.text.clone(),
            start: Millis(start),
            end: Millis(end),
        });
        cursor += dur;
    }
    // Anchor the last word to the line end so highlighting does not lag behind.
    if let Some(last) = out.last_mut() {
        last.end = Millis(last.end.0.min(line.end.0.max(last.start.0 + MIN_WORD_MS)));
    }
    out
}

fn word_weight(text: &str) -> f32 {
    let mut weight = 0.0f32;
    let mut gap = 0.0f32;
    for c in text.chars() {
        if c.is_whitespace() {
            gap = (gap + SPACE_WEIGHT).min(MAX_GAP_WEIGHT);
        } else if is_arabic_diacritic(c) {
            weight += DIACRITIC_WEIGHT;
        } else {
            weight += BASE_WEIGHT;
            if matches!(c, 'ا' | 'و' | 'ي' | 'ى' | 'آ') {
                weight += LONG_VOWEL_BONUS;
            }
        }
    }
    (weight + gap).max(0.1)
}

/// Fills in estimated word timing for every line of `doc`, in place.
///
/// Documents that are already word-timed are left untouched.
pub fn estimate_word_timing(doc: &mut LyricDocument) {
    if doc.word_timed {
        return;
    }
    for line in &mut doc.lines {
        line.words = distribute_line(line);
    }
    doc.word_timed = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> LyricLine {
        let words = text
            .split_whitespace()
            .map(|w| TimedWord {
                text: w.to_owned(),
                start: Millis(0),
                end: Millis(0),
            })
            .collect();
        LyricLine {
            start: Millis(1_000),
            end: Millis(5_000),
            words,
            translation: None,
        }
    }

    #[test]
    fn words_are_contiguous_and_inside_the_line() {
        let l = line("Hello Aurora في الظَّلام");
        let words = distribute_line(&l);
        assert_eq!(words.len(), 4);
        assert!(words[0].start >= l.start);
        for pair in words.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        assert!(words.last().unwrap().end <= l.end);
    }

    #[test]
    fn diacritics_add_much_less_than_a_base_letter() {
        let plain = word_weight("سلام");
        let voweled = word_weight("سَلامٌ");
        let added = voweled - plain;
        assert!(added > 0.0, "harakat still take a little time");
        assert!(
            added < word_weight("ا"),
            "two harakat must weigh less than one letter, got {added}"
        );
    }

    #[test]
    fn already_timed_lines_are_untouched() {
        let mut l = line("a b");
        l.words[0].end = Millis(1_200);
        l.words[1].end = Millis(1_800);
        let out = distribute_line(&l);
        assert_eq!(out[0].end, Millis(1_200));
    }

    #[test]
    fn estimate_marks_document_word_timed() {
        let mut doc = LyricDocument {
            lines: vec![line("one two three")],
            offset: 0,
            source: aurora_core::lyrics::LyricSource::Sidecar,
            word_timed: false,
        };
        estimate_word_timing(&mut doc);
        assert!(doc.word_timed);
        assert!(doc.lines[0].is_word_timed());
    }
}
