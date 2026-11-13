//! LRC parser (plain and enhanced).
//!
//! Reference behaviour used by Aurora:
//!
//! * `[mm:ss.xx]text` - one timestamp, one line. Several timestamps may prefix
//!   the same text (`[00:12.00][01:12.00]chorus`), all are emitted.
//! * `[mm:ss.xx]<mm:ss.xx>word<mm:ss.xx>word` - enhanced LRC, per-word timing.
//! * `[offset:+120]` - global shift in milliseconds (positive = lyrics earlier).
//! * `[ti:]`, `[ar:]`, `[al:]`, `[by:]` - metadata, kept but not rendered.
//!
//! Lines are sorted by start time, because hand-edited files are frequently out
//! of order and the animator assumes sorted input.

use aurora_core::lyrics::{LyricDocument, LyricLine, LyricSource, Millis, TimedWord};

/// The ID-tag block of an LRC file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LrcMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub author: Option<String>,
}

/// Parses an LRC document. Never fails: malformed lines are skipped, which is
/// what every real-world player does (a single typo must not hide a whole song).
#[must_use]
pub fn parse_lrc(text: &str, source: LyricSource) -> LyricDocument {
    let mut meta = LrcMetadata::default();
    let mut offset_ms: i64 = 0;
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_matches(['\u{feff}', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        if let Some(tag) = parse_id_tag(line) {
            match tag.0 {
                "offset" => offset_ms = tag.1.trim().parse().unwrap_or(0),
                "ti" => meta.title = Some(tag.1.to_owned()),
                "ar" => meta.artist = Some(tag.1.to_owned()),
                "al" => meta.album = Some(tag.1.to_owned()),
                "by" => meta.author = Some(tag.1.to_owned()),
                _ => {}
            }
            continue;
        }
        if let Some(mut parsed) = parse_timed_line(line) {
            lines.append(&mut parsed);
        }
    }

    apply_offset(&mut lines, offset_ms);
    lines.sort_by_key(|l| l.start);
    for i in 0..lines.len() {
        if lines[i].end == Millis(0) || lines[i].end <= lines[i].start {
            let next = lines
                .get(i + 1)
                .map_or(lines[i].start.0 + 4_000, |l| l.start.0);
            lines[i].end = Millis(next.max(lines[i].start.0 + 500));
        }
    }

    let word_timed = !lines.is_empty() && lines.iter().all(LyricLine::is_word_timed);
    LyricDocument {
        lines,
        offset: offset_ms as i32,
        source,
        word_timed,
    }
}

fn parse_id_tag(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('[')?;
    let (key, tail) = rest.split_once(':')?;
    let value = tail.strip_suffix(']')?;
    Some((key, value))
}

fn parse_timestamp(s: &str) -> Option<Millis> {
    // mm:ss(.xx)? or hh:mm:ss
    let mut parts = s.split(':');
    let first = parts.next()?.trim();
    let second = parts.next()?.trim();
    let (minutes, seconds, fraction_ms) = match parts.next() {
        Some(third) => {
            let hours: u64 = first.parse().ok()?;
            (hours * 60 + second.parse::<u64>().ok()?, third, true)
        }
        None => {
            let minutes: u64 = first.parse().ok()?;
            let (sec_str, frac) = match second.split_once(['.', ',']) {
                Some((s, f)) => (s, Some(f)),
                None => (second, None),
            };
            let ms = frac.map_or(0, |f| {
                let digits: String = f.chars().filter(char::is_ascii_digit).collect();
                match digits.len() {
                    0 => 0,
                    1 => digits.parse::<u64>().unwrap_or(0) * 100,
                    2 => digits.parse::<u64>().unwrap_or(0) * 10,
                    _ => digits[..3].parse::<u64>().unwrap_or(0),
                }
            });
            let seconds: u64 = sec_str.trim().parse().ok()?;
            return Some(Millis((minutes * 60 + seconds) * 1000 + ms));
        }
    };
    let (sec_str, frac) = match seconds.split_once(['.', ',']) {
        Some((s, f)) => (s, Some(f)),
        None => (seconds, None),
    };
    let _ = fraction_ms;
    let ms = frac.map_or(0, |f| {
        let digits: String = f.chars().filter(char::is_ascii_digit).collect();
        digits.parse::<u64>().unwrap_or(0) * 10
    });
    Some(Millis(
        (minutes * 60 + sec_str.trim().parse::<u64>().ok()?) * 1000 + ms,
    ))
}

/// Parses one line that may be prefixed by one or more `[mm:ss.xx]` stamps.
fn parse_timed_line(line: &str) -> Option<Vec<LyricLine>> {
    let mut starts = Vec::new();
    let mut rest = line;
    while let Some(stripped) = rest.strip_prefix('[') {
        let (stamp, tail) = stripped.split_once(']')?;
        let Some(at) = parse_timestamp(stamp) else {
            // `[ti:...]` style handled elsewhere; stop here.
            break;
        };
        starts.push(at);
        rest = tail;
    }
    if starts.is_empty() {
        return None;
    }

    let words = parse_words(rest);
    // Enhanced LRC word stamps are absolute; only the *repeats* of a multi
    // timestamp line (`[00:10][01:10]chorus`) need to be shifted.
    let base = words.first().map_or(0, |w| w.start.0);
    let lines = starts
        .into_iter()
        .map(|start| {
            let delta = start.0 as i64 - base as i64;
            let shift = |t: Millis| Millis((t.0 as i64 + delta).max(0) as u64);
            let shifted: Vec<TimedWord> = words
                .iter()
                .map(|w| TimedWord {
                    text: w.text.clone(),
                    start: shift(w.start),
                    end: shift(w.end),
                })
                .collect();
            let end = shifted.last().map_or(start, |w: &TimedWord| w.end);
            LyricLine {
                start,
                end,
                words: shifted,
                translation: None,
            }
        })
        .collect();
    Some(lines)
}

/// Splits a line into timed words. Enhanced LRC gives real per-word stamps;
/// plain LRC yields one word-timed unit per whitespace-separated token with a
/// zero-length placeholder that [`crate::estimate`] later fills in.
fn parse_words(text: &str) -> Vec<TimedWord> {
    if !text.contains('<') {
        let mut words = Vec::new();
        for token in text.split_inclusive(char::is_whitespace) {
            let trimmed = token.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            words.push(TimedWord {
                text: token.to_owned(),
                start: Millis(0),
                end: Millis(0),
            });
        }
        return words;
    }

    let mut words = Vec::new();
    let mut pending_start: Option<Millis> = None;
    let mut buffer = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '<' {
            let end_idx = text[i..].find('>').map(|o| i + o);
            if let Some(end_idx) = end_idx {
                let stamp = &text[i + 1..end_idx];
                if let Some(at) = parse_timestamp(stamp) {
                    if let Some(start) = pending_start.take()
                        && !buffer.is_empty()
                    {
                        words.push(TimedWord {
                            text: buffer.clone(),
                            start,
                            end: at,
                        });
                        buffer.clear();
                    }
                    pending_start = Some(at);
                    for _ in i..=end_idx {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        buffer.push(c);
    }
    if let Some(start) = pending_start
        && !buffer.is_empty()
    {
        let end = Millis(start.0 + 400);
        words.push(TimedWord {
            text: buffer,
            start,
            end,
        });
    }
    words
}

fn apply_offset(lines: &mut [LyricLine], offset_ms: i64) {
    if offset_ms == 0 {
        return;
    }
    let shift = |t: Millis| -> Millis {
        let shifted = t.0 as i64 + offset_ms;
        Millis(shifted.max(0) as u64)
    };
    for line in lines {
        line.start = shift(line.start);
        line.end = shift(line.end);
        for word in &mut line.words {
            word.start = shift(word.start);
            word.end = shift(word.end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[ti:Shik Shak Shok]\n[ar:Chikha Rimitti]\n[offset:-250]\n\
[00:12.34]سَلامٌ على الدُّنيا\n\
[00:15.00]Hello Aurora في الظَّلام\n";

    #[test]
    fn parses_plain_lrc_with_offset_and_metadata() {
        let doc = parse_lrc(SAMPLE, LyricSource::Sidecar);
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.offset, -250);
        assert_eq!(doc.lines[0].start, Millis(12_090));
        assert_eq!(doc.lines[1].start, Millis(14_750));
        assert!(!doc.word_timed);
        assert!(doc.lines[0].end > doc.lines[0].start);
    }

    #[test]
    fn keeps_arabic_text_intact() {
        let doc = parse_lrc(SAMPLE, LyricSource::Sidecar);
        assert!(doc.lines[0].text().contains("سَلامٌ"));
    }

    #[test]
    fn parses_enhanced_lrc_word_timing() {
        let src = "[00:10.00]<00:10.00>Nhel <00:10.40>lik <00:11.00>ya <00:11.60>hbibi";
        let doc = parse_lrc(src, LyricSource::Sidecar);
        assert!(doc.word_timed);
        let words = &doc.lines[0].words;
        assert_eq!(words.len(), 4);
        assert_eq!(words[1].start, Millis(10_400));
        assert_eq!(words[3].end, Millis(12_000));
    }

    #[test]
    fn multiple_timestamps_duplicate_the_line() {
        let doc = parse_lrc("[00:01.00][00:31.00]chorus", LyricSource::Sidecar);
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].start, Millis(1_000));
        assert_eq!(doc.lines[1].start, Millis(31_000));
    }

    #[test]
    fn sorts_out_of_order_lines() {
        let doc = parse_lrc("[00:20.00]second\n[00:10.00]first", LyricSource::Sidecar);
        assert_eq!(doc.lines[0].start, Millis(10_000));
        assert_eq!(doc.active_line(Millis(15_000)), Some(0));
    }

    #[test]
    fn garbage_is_skipped_not_fatal() {
        let doc = parse_lrc(
            "not a lyric\n[xx:yy]broken\n[00:05.00]ok",
            LyricSource::Sidecar,
        );
        assert_eq!(doc.lines.len(), 1);
    }
}
