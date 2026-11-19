//! Portability seams (spec 63.9).
//!
//! Every platform-specific capability sits behind one of these traits so the
//! catalog, the lyric engine and the UI state machine stay identical on Fedora
//! Linux and on Android:
//!
//! * Linux: `cpal`/PipeWire backend, `std::fs` storage, XDG directories.
//! * Android: `oboe`/AAudio backend, SAF `content://` storage, app-private dirs.
//!
//! No trait in this file mentions a concrete audio API, path type or URI scheme
//! beyond the two [`TrackLocation`](crate::model::TrackLocation) variants.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::TrackLocation;

/// Audio formats Aurora can decode (decoder support lands in Phase 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    Flac,
    Alac,
    Wav,
    Aiff,
    Mp3,
    Aac,
    Opus,
    Vorbis,
}

impl AudioFormat {
    /// Guesses a format from a file name, which is all the scanner needs before
    /// the real probe runs. `None` means "not a file we care about".
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "flac" => Some(Self::Flac),
            "m4a" | "alac" => Some(Self::Alac),
            "wav" | "wave" => Some(Self::Wav),
            "aiff" | "aif" | "aifc" => Some(Self::Aiff),
            "mp3" => Some(Self::Mp3),
            "aac" => Some(Self::Aac),
            "opus" => Some(Self::Opus),
            "ogg" | "oga" => Some(Self::Vorbis),
            _ => None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Flac => "fLaC",
            Self::Alac => "ALAC",
            Self::Wav => "WAVE",
            Self::Aiff => "AIFF",
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::Opus => "Opus",
            Self::Vorbis => "Vorbis",
        }
    }
}

/// A device that turns PCM into sound.
///
/// The contract that matters for the clock: [`Self::position`] must report the
/// position of the *last sample handed to the device*, not what the UI thinks,
/// and the caller republishes it into [`crate::clock::PlaybackClock`] from the
/// callback thread.
pub trait AudioOutput: Send {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Opens the default device with `sample_rate`/`channels` negotiated.
    fn open(sample_rate: u32, channels: u16) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn play(&mut self) -> Result<(), Self::Error>;
    fn pause(&mut self) -> Result<(), Self::Error>;
    fn seek(&mut self, position: Duration) -> Result<(), Self::Error>;
    fn set_volume(&mut self, volume: f32);
    /// Frames consumed by the device since playback started.
    fn frames_played(&self) -> u64;
    fn sample_rate(&self) -> u32;

    /// Position derived from the device counter; this is what feeds the clock.
    fn position(&self) -> Duration {
        let rate = u64::from(self.sample_rate().max(1));
        Duration::from_micros(self.frames_played().saturating_mul(1_000_000) / rate)
    }
}

/// Read-side view of the music library, abstracting paths vs. content URIs.
pub trait StorageBackend: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the bytes of a track.
    fn read(&self, location: &TrackLocation) -> Result<Vec<u8>, Self::Error>;

    /// Enumerates audio files under a root. On Android this walks a SAF tree
    /// instead of a directory, which is why it is a method and not `fs::read_dir`.
    fn walk_audio(&self, root: &TrackLocation) -> Result<Vec<TrackLocation>, Self::Error>;

    /// Sidecar lyrics next to a track (`<file>.lrc`), if the backend can express
    /// "sibling of" for this location kind.
    fn sibling_lrc(&self, location: &TrackLocation) -> Option<TrackLocation>;

    /// True when `location` still exists (used to prune the catalog).
    fn exists(&self, location: &TrackLocation) -> bool;
}

/// Per-platform well-known directories.
///
/// Linux implementations map to XDG (`~/.local/share/aurora`, ...); Android
/// implementations map to `Context.getFilesDir()` children. Nothing above this
/// trait may call `dirs::data_dir()` directly.
pub trait AppDirectories: Send + Sync {
    fn data_dir(&self) -> PathBuf;
    fn config_dir(&self) -> PathBuf;
    fn cache_dir(&self) -> PathBuf;
    fn lyrics_dir(&self) -> PathBuf;
    fn database_file(&self) -> PathBuf;

    /// Creates the directories if missing. Called once at startup.
    fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            self.data_dir(),
            self.config_dir(),
            self.cache_dir(),
            self.lyrics_dir(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }
}

/// Convenience for backends that are literally "a directory on this machine".
#[derive(Debug, Clone)]
pub struct PathStorage;

impl StorageBackend for PathStorage {
    type Error = std::io::Error;

    fn read(&self, location: &TrackLocation) -> Result<Vec<u8>, Self::Error> {
        std::fs::read(Self::path_of(location)?)
    }

    fn walk_audio(&self, root: &TrackLocation) -> Result<Vec<TrackLocation>, Self::Error> {
        let root = Self::path_of(root)?;
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && AudioFormat::from_extension(ext).is_some()
                {
                    out.push(TrackLocation::Path(path));
                }
            }
        }
        out.sort_by_key(TrackLocation::as_str);
        Ok(out)
    }

    fn sibling_lrc(&self, location: &TrackLocation) -> Option<TrackLocation> {
        let path = Self::path_of(location).ok()?;
        Some(TrackLocation::Path(path.with_extension("lrc")))
    }

    fn exists(&self, location: &TrackLocation) -> bool {
        Self::path_of(location).map(|p| p.exists()).unwrap_or(false)
    }
}

impl PathStorage {
    fn path_of(location: &TrackLocation) -> std::io::Result<&Path> {
        match location {
            TrackLocation::Path(path) => Ok(path),
            TrackLocation::ContentUri(uri) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("content URI {uri} needs the Android storage backend"),
            )),
        }
    }
}
