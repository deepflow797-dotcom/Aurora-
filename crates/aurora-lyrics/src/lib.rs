//! Lyrics engine: discovery, parsing and word-timing estimation.
//!
//! Scope is deliberately local-first (spec 63.2): Aurora never calls a lyric
//! service. A track's lyrics come from, in order:
//!
//! 1. a sidecar `<track>.lrc` next to the audio file,
//! 2. `<artist> - <title>.lrc` inside the user's lyrics folder,
//! 3. tags embedded in the file (Phase 2),
//! 4. nothing - the UI then shows "no lyrics" instead of an ad.
//!
//! Parsing covers plain LRC (`[mm:ss.xx]`) and enhanced LRC with per-word
//! `<mm:ss.xx>` stamps. Everything else (`.srt`, `.ass`, Apple's TTML) is out of
//! scope for Phase 0 and noted in ARCHITECTURE.md.

pub mod estimate;
pub mod lrc;

pub use estimate::estimate_word_timing;
pub use lrc::{LrcMetadata, parse_lrc};

use aurora_core::lyrics::{LyricDocument, LyricSource};
use aurora_core::model::{Artist, Track};
use aurora_core::traits::StorageBackend;

/// Result of a lyric lookup, including why it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found(LyricDocument),
    Missing { tried: Vec<String> },
}

impl Lookup {
    #[must_use]
    pub fn document(self) -> Option<LyricDocument> {
        match self {
            Self::Found(doc) => Some(doc),
            Self::Missing { .. } => None,
        }
    }
}

/// Looks for lyrics for `track` using `storage` for all I/O.
///
/// `lyrics_dir` is the configured lyrics folder (from `AppDirectories`). The
/// function is intentionally synchronous and small: the caller runs it on a
/// worker thread and hands the document to the UI.
pub fn discover<S: StorageBackend>(
    storage: &S,
    track: &Track,
    lyrics_dir: &std::path::Path,
) -> Lookup {
    let mut tried = Vec::new();

    if let Some(sibling) = storage.sibling_lrc(&track.location) {
        tried.push(sibling.as_str());
        if storage.exists(&sibling)
            && let Ok(bytes) = storage.read(&sibling)
            && let Ok(text) = String::from_utf8(bytes)
        {
            let doc = parse_lrc(&text, LyricSource::Sidecar);
            if !doc.is_empty() {
                return Lookup::Found(doc);
            }
        }
    }

    let file_name = format!(
        "{} - {}.lrc",
        sanitize_component(&track.artist.name),
        sanitize_component(&track.title)
    );
    let candidate = lyrics_dir.join(file_name);
    tried.push(candidate.display().to_string());
    if let Ok(text) = std::fs::read_to_string(&candidate) {
        let doc = parse_lrc(&text, LyricSource::LibraryFolder);
        if !doc.is_empty() {
            return Lookup::Found(doc);
        }
    }

    Lookup::Missing { tried }
}

/// Normalizes `naming` rules shared by the scanner and the lyrics folder.
#[must_use]
pub fn track_file_stem(artist: &Artist, title: &str) -> String {
    format!(
        "{} - {}",
        sanitize_component(&artist.name),
        sanitize_component(title)
    )
}

fn sanitize_component(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stem_sanitizes_separators() {
        let artist = Artist::new("AC/DC");
        assert_eq!(
            track_file_stem(&artist, "Back: in Black"),
            "AC_DC - Back_ in Black"
        );
    }
}
