//! Library scanner.
//!
//! Phase 0 scope: walk a storage root, filter by extension, derive a minimal
//! track record and upsert it into the catalog. Tag reading (`lofty`) and
//! duration probing land in Phase 1; the [`Probe`] trait is the seam they plug
//! into, so the scanner's control flow does not change.

use aurora_core::model::{Album, Artist, Track, TrackId, TrackLocation};
use aurora_core::traits::{AudioFormat, PathStorage, StorageBackend};
use aurora_db::{Catalog, DbError};

/// Turns a raw file location into a [`Track`]. Phase 1 swaps in `lofty`.
pub trait Probe {
    fn probe(&self, location: &TrackLocation) -> Option<ProbeResult>;
}

/// Everything a probe can tell us about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u8>,
    pub codec: Option<String>,
}

/// Filename-based fallback probe: `NN - Artist - Title.ext`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileNameProbe;

impl Probe for FileNameProbe {
    fn probe(&self, location: &TrackLocation) -> Option<ProbeResult> {
        let raw = location.as_str();
        let path = std::path::Path::new(&raw);
        let stem = path.file_stem()?.to_str()?;
        let ext = path.extension().and_then(|e| e.to_str())?;
        let format = AudioFormat::from_extension(ext)?;

        let (number, rest) = match stem.split_once(" - ") {
            Some((head, tail)) if head.chars().all(|c| c.is_ascii_digit()) => {
                (head.parse::<u16>().ok(), tail)
            }
            _ => (None, stem),
        };
        let (artist, title) = rest.split_once(" - ").unwrap_or(("Unknown Artist", rest));

        Some(ProbeResult {
            title: title.trim().to_owned(),
            artist: artist.trim().to_owned(),
            album: String::new(),
            track_number: number,
            duration_ms: 0,
            sample_rate_hz: None,
            channels: None,
            codec: Some(format.label().to_owned()),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("storage: {0}")]
    Storage(String),
    #[error("catalog: {0}")]
    Catalog(#[from] DbError),
}

/// Summary of one scan run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub visited: usize,
    pub added: usize,
    pub skipped: usize,
}

/// Scans `root` and upserts everything found into `catalog`.
pub fn scan<P: Probe, C: Catalog>(
    storage: &PathStorage,
    probe: &P,
    catalog: &C,
    root: &TrackLocation,
) -> Result<ScanReport, ScanError> {
    let locations = storage
        .walk_audio(root)
        .map_err(|e| ScanError::Storage(e.to_string()))?;
    let mut report = ScanReport::default();
    for location in locations {
        report.visited += 1;
        match probe.probe(&location) {
            Some(result) => {
                catalog.upsert_track(Track {
                    id: TrackId(0),
                    location,
                    title: result.title,
                    artist: Artist::new(result.artist),
                    album: Album {
                        title: result.album,
                        ..Album::default()
                    },
                    track_number: result.track_number,
                    duration_ms: result.duration_ms,
                    sample_rate_hz: result.sample_rate_hz,
                    channels: result.channels,
                    codec: result.codec,
                })?;
                report.added += 1;
            }
            None => report.skipped += 1,
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_db::MemoryCatalog;

    #[test]
    fn filename_probe_parses_number_artist_title() {
        let loc = TrackLocation::Path("/music/07 - Chikha Rimitti - Charrak.flac".into());
        let p = FileNameProbe.probe(&loc).unwrap();
        assert_eq!(p.track_number, Some(7));
        assert_eq!(p.artist, "Chikha Rimitti");
        assert_eq!(p.title, "Charrak");
        assert_eq!(p.codec.as_deref(), Some("fLaC"));
    }

    #[test]
    fn filename_probe_ignores_non_audio() {
        let loc = TrackLocation::Path("/music/cover.jpg".into());
        assert!(FileNameProbe.probe(&loc).is_none());
    }

    #[test]
    fn scan_walks_a_real_directory_tree() {
        let dir = std::env::temp_dir().join(format!("aurora-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        for name in ["01 - A - One.flac", "nested/02 - B - Two.mp3", "notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let catalog = MemoryCatalog::new();
        let report = scan(
            &PathStorage,
            &FileNameProbe,
            &catalog,
            &TrackLocation::Path(dir.clone()),
        )
        .unwrap();
        assert_eq!(report.visited, 2);
        assert_eq!(report.added, 2);
        assert_eq!(catalog.len().unwrap(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
