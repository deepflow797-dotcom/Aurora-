//! Catalog storage.
//!
//! Decision for Phase 1 is SQLite (`rusqlite`, bundled) behind the [`Catalog`]
//! trait; see ARCHITECTURE.md for why. Phase 0 ships the trait plus an in-memory
//! implementation so the scanner and the UI can be written and tested today
//! without pulling a C dependency into the first commit.

use std::collections::BTreeMap;

use aurora_core::model::{Track, TrackId};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("track {0} not found")]
    NotFound(TrackId),
    #[error("storage error: {0}")]
    Storage(String),
}

/// The queries the UI actually needs. Deliberately not a general SQL wrapper:
/// every method is a screen or a scan step.
pub trait Catalog: Send + Sync {
    fn upsert_track(&self, track: Track) -> Result<TrackId, DbError>;
    fn track(&self, id: TrackId) -> Result<Option<Track>, DbError>;
    fn all_tracks(&self) -> Result<Vec<Track>, DbError>;
    fn remove_track(&self, id: TrackId) -> Result<(), DbError>;
    fn clear(&self) -> Result<(), DbError>;
    fn len(&self) -> Result<usize, DbError>;

    fn is_empty(&self) -> Result<bool, DbError> {
        Ok(self.len()? == 0)
    }
}

/// In-memory catalog used by tests, the spike and the very first run.
#[derive(Debug, Default)]
pub struct MemoryCatalog {
    tracks: std::sync::RwLock<BTreeMap<TrackId, Track>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl MemoryCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracks: std::sync::RwLock::new(BTreeMap::new()),
            next_id: Default::default(),
        }
    }
}

impl Catalog for MemoryCatalog {
    fn upsert_track(&self, mut track: Track) -> Result<TrackId, DbError> {
        let mut tracks = self
            .tracks
            .write()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        if track.id.0 == 0 {
            let next = self
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .saturating_add(1);
            track.id = TrackId(next);
        }
        let id = track.id;
        tracks.insert(id, track);
        Ok(id)
    }

    fn track(&self, id: TrackId) -> Result<Option<Track>, DbError> {
        let tracks = self
            .tracks
            .read()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        Ok(tracks.get(&id).cloned())
    }

    fn all_tracks(&self) -> Result<Vec<Track>, DbError> {
        let tracks = self
            .tracks
            .read()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        Ok(tracks.values().cloned().collect())
    }

    fn remove_track(&self, id: TrackId) -> Result<(), DbError> {
        let mut tracks = self
            .tracks
            .write()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        tracks.remove(&id).map(|_| ()).ok_or(DbError::NotFound(id))
    }

    fn clear(&self) -> Result<(), DbError> {
        let mut tracks = self
            .tracks
            .write()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        tracks.clear();
        Ok(())
    }

    fn len(&self) -> Result<usize, DbError> {
        let tracks = self
            .tracks
            .read()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        Ok(tracks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::model::{Album, Artist, TrackLocation};

    fn track(title: &str) -> Track {
        Track {
            id: TrackId(0),
            location: TrackLocation::Path("/music/a.flac".into()),
            title: title.to_owned(),
            artist: Artist::new("Aurora"),
            album: Album::default(),
            track_number: Some(1),
            duration_ms: 192_000,
            sample_rate_hz: Some(44_100),
            channels: Some(2),
            codec: Some("flac".into()),
        }
    }

    #[test]
    fn assigns_increasing_ids_and_round_trips() {
        let cat = MemoryCatalog::new();
        let a = cat.upsert_track(track("one")).unwrap();
        let b = cat.upsert_track(track("two")).unwrap();
        assert!(b.0 > a.0);
        assert_eq!(cat.track(a).unwrap().unwrap().title, "one");
        assert_eq!(cat.len().unwrap(), 2);
        cat.remove_track(a).unwrap();
        assert_eq!(cat.len().unwrap(), 1);
        assert!(matches!(cat.remove_track(a), Err(DbError::NotFound(_))));
    }
}
