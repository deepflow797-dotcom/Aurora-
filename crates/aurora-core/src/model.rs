//! Library entities.
//!
//! These are deliberately small and `serde`-friendly: the same structs cross the
//! UniFFI boundary on Android (Strategy B) and are stored in the local catalog.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable identifier for a track inside the local catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TrackId(pub u64);

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "track:{}", self.0)
    }
}

/// An artist as credited in tags.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    /// Sort key with leading articles removed ("Beatles, The").
    pub sort_name: Option<String>,
}

impl Artist {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sort_name: None,
        }
    }

    /// Display name, falling back to the raw name when no sort key exists.
    #[must_use]
    pub fn sort_key(&self) -> &str {
        self.sort_name.as_deref().unwrap_or(&self.name)
    }
}

/// An album grouping tracks that share a release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub title: String,
    pub album_artist: Option<Artist>,
    pub year: Option<u16>,
    pub disc_count: Option<u8>,
}

/// One playable audio file, as discovered by the library scanner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    /// Location relative to a storage root; see [`crate::traits::StorageBackend`].
    pub location: TrackLocation,
    pub title: String,
    pub artist: Artist,
    pub album: Album,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u8>,
    /// Container/codec label as probed from the file ("flac", "mp3", ...).
    pub codec: Option<String>,
}

impl Track {
    /// Human readable "Artist - Title" label used by notifications and tests.
    #[must_use]
    pub fn display_name(&self) -> String {
        format!("{} - {}", self.artist.name, self.title)
    }

    #[must_use]
    pub fn duration_secs_f64(&self) -> f64 {
        self.duration_ms as f64 / 1000.0
    }
}

/// Where a track's bytes live.
///
/// Android hands out `content://` URIs instead of paths, so the catalog stores
/// the *kind* of location rather than a `PathBuf`. Only the storage backend of
/// the current platform interprets the payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackLocation {
    /// A filesystem path, relative to the library root when possible.
    Path(PathBuf),
    /// An Android `content://` URI (Strategy B, SAF-imported library).
    ContentUri(String),
}

impl TrackLocation {
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::Path(path) => path.display().to_string(),
            Self::ContentUri(uri) => uri.clone(),
        }
    }
}
