//! The §27 settings surface, persisted as TOML.
//!
//! Every user-visible preference named in the spec lives here exactly once; the
//! UI reads and writes this struct and nothing else, so "which settings exist"
//! is answerable by reading one file instead of grepping the view code.
//!
//! Two rules make the format safe to live with:
//!
//! * **Forward compatible.** Unknown keys are ignored and every field has a
//!   default, so a downgrade or a hand-edited file never bricks startup.
//! * **Atomic.** [`Settings::save`] writes `settings.toml.tmp` and renames it,
//!   so a crash mid-write leaves the previous file intact. Values are clamped on
//!   load ([`Settings::normalize`]) because a TOML file is user input.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::PlatformError;

/// Schema version written into the file. Bumped when a field's meaning changes;
/// reading a *newer* file only warns, it never fails.
pub const SCHEMA_VERSION: u32 = 1;

/// Repeat mode for the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl std::fmt::Display for RepeatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Off => "Off",
            Self::All => "All",
            Self::One => "One",
        };
        f.write_str(label)
    }
}

/// How much animation the lyric view is allowed to use (§27 lyric animation
/// profile, §41 motion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnimationProfile {
    /// No animation at all; words simply turn on.
    None,
    /// Fades only, no per-word travel.
    Subtle,
    /// Word envelopes plus line transitions.
    #[default]
    Standard,
    /// Adds spring overshoot and artwork motion.
    Expressive,
}

impl std::fmt::Display for AnimationProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::None => "None",
            Self::Subtle => "Subtle",
            Self::Standard => "Standard",
            Self::Expressive => "Expressive",
        };
        f.write_str(label)
    }
}

/// Colour theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        };
        f.write_str(label)
    }
}

/// Playback preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Playback {
    /// Linear gain, `0.0..=1.0`.
    pub volume: f32,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    /// Restore the last track and position on launch.
    pub resume: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            volume: 0.8,
            repeat: RepeatMode::Off,
            shuffle: false,
            resume: true,
        }
    }
}

/// Lyric presentation preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Lyrics {
    pub enabled: bool,
    pub animation: AnimationProfile,
    /// Keep the active line vertically centred.
    pub auto_scroll: bool,
    /// Highlight the sung word, not just the line.
    pub word_highlighting: bool,
    /// Multiplier on the base lyric font size, `0.5..=2.0`.
    pub font_scale: f32,
}

impl Default for Lyrics {
    fn default() -> Self {
        Self {
            enabled: true,
            animation: AnimationProfile::Standard,
            auto_scroll: true,
            word_highlighting: true,
            font_scale: 1.0,
        }
    }
}

/// Look and feel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Appearance {
    pub theme: Theme,
    /// Accent colour as `[r, g, b]`.
    pub accent: [u8; 3],
    /// Blur/gradient treatment behind artwork.
    pub artwork_effects: bool,
    /// Global motion multiplier, `0.0..=1.0`.
    pub animation_intensity: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: [0x4c, 0x9a, 0xff],
            artwork_effects: true,
            animation_intensity: 1.0,
        }
    }
}

/// Performance and battery preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Performance {
    /// Spec §41 accessibility switch: disables lyric animation entirely.
    pub reduced_motion: bool,
    /// Use shader effects when a GPU is available.
    pub gpu_effects: bool,
    /// Artwork thumbnail cache budget in MiB, `0..=65536`.
    pub cache_size_mb: u32,
    /// Scan the library while the app is idle.
    pub background_scanning: bool,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            gpu_effects: true,
            cache_size_mb: 512,
            background_scanning: true,
        }
    }
}

/// Library preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Library {
    /// Folders the scanner walks, in the order the user added them.
    pub music_folders: Vec<PathBuf>,
    /// Rescan on every launch instead of only on demand.
    pub rescan_on_start: bool,
    /// Watch folders for changes (inotify) instead of polling on demand.
    pub watch_folders: bool,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            music_folders: Vec::new(),
            rescan_on_start: false,
            watch_folders: true,
        }
    }
}

/// The whole settings file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub schema_version: u32,
    pub playback: Playback,
    pub lyrics: Lyrics,
    pub appearance: Appearance,
    pub performance: Performance,
    pub library: Library,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            playback: Playback::default(),
            lyrics: Lyrics::default(),
            appearance: Appearance::default(),
            performance: Performance::default(),
            library: Library::default(),
        }
    }
}

/// Where the settings in memory came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Read from an existing `settings.toml`.
    File,
    /// No file existed; defaults were written so the next run has one to edit.
    CreatedDefault,
}

impl Settings {
    /// Loads `path`, creating it with defaults when it does not exist.
    ///
    /// # Errors
    ///
    /// Fails on unreadable files, invalid TOML, and when the fresh default file
    /// cannot be written. A *stale* schema version is not an error (it warns).
    pub fn load_or_create(path: &Path) -> Result<(Self, Source), PlatformError> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let settings: Self =
                    toml::from_str(&text).map_err(|source| PlatformError::Parse {
                        path: path.to_path_buf(),
                        source,
                    })?;
                let mut settings = settings;
                if settings.schema_version > SCHEMA_VERSION {
                    warn!(
                        file = %path.display(),
                        found = settings.schema_version,
                        supported = SCHEMA_VERSION,
                        "settings file is newer than this build; unknown keys are ignored"
                    );
                }
                settings.normalize();
                Ok((settings, Source::File))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let settings = Self::default();
                settings.save(path)?;
                Ok((settings, Source::CreatedDefault))
            }
            Err(source) => Err(PlatformError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes the settings atomically: temp file in the same directory, then
    /// `rename(2)`, which is atomic within a filesystem.
    ///
    /// # Errors
    ///
    /// Fails when the parent directory is missing or not writable.
    pub fn save(&self, path: &Path) -> Result<(), PlatformError> {
        let text = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).map_err(|source| PlatformError::Write {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, path).map_err(|source| {
            // Do not leave a stray temp file behind if the rename fails.
            fs::remove_file(&tmp).ok();
            PlatformError::Write {
                path: path.to_path_buf(),
                source,
            }
        })
    }

    /// Clamps every value that came from a file. Called on load so the rest of
    /// the program can treat settings as in-range without re-checking.
    pub fn normalize(&mut self) {
        self.playback.volume = self.playback.volume.clamp(0.0, 1.0);
        self.lyrics.font_scale = self.lyrics.font_scale.clamp(0.5, 2.0);
        self.appearance.animation_intensity = self.appearance.animation_intensity.clamp(0.0, 1.0);
        self.performance.cache_size_mb = self.performance.cache_size_mb.min(65_536);
        // Drop empty / duplicate folders so the scanner never walks `""`.
        let mut seen = Vec::new();
        self.library.music_folders.retain(|folder| {
            if folder.as_os_str().is_empty() || seen.contains(folder) {
                return false;
            }
            seen.push(folder.clone());
            true
        });
        self.schema_version = self.schema_version.min(SCHEMA_VERSION);
    }

    /// True when motion must be suppressed regardless of the animation profile.
    ///
    /// Both the accessibility switch and a zero intensity end up here so the UI
    /// asks one question.
    #[must_use]
    pub fn motion_disabled(&self) -> bool {
        self.performance.reduced_motion
            || self.appearance.animation_intensity <= 0.0
            || self.lyrics.animation == AnimationProfile::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per test so a parallel `cargo test` cannot collide.
    fn temp_path(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "aurora-settings-{}-{seq}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir.join("settings.toml")
    }

    #[test]
    fn defaults_are_written_when_the_file_is_missing() {
        let path = temp_path("create");
        let (settings, source) = Settings::load_or_create(&path).expect("create defaults");
        assert_eq!(source, Source::CreatedDefault);
        assert_eq!(settings, Settings::default());
        assert!(path.exists(), "the file must exist after the first run");
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let path = temp_path("roundtrip");
        let mut settings = Settings::default();
        settings.playback.volume = 0.35;
        settings.playback.repeat = RepeatMode::One;
        settings.playback.shuffle = true;
        settings.lyrics.animation = AnimationProfile::Subtle;
        settings.lyrics.font_scale = 1.25;
        settings.appearance.theme = Theme::Light;
        settings.appearance.accent = [1, 2, 3];
        settings.performance.cache_size_mb = 64;
        settings.library.music_folders = vec![PathBuf::from("/music")];
        settings.save(&path).expect("save");

        let (loaded, source) = Settings::load_or_create(&path).expect("load");
        assert_eq!(source, Source::File);
        assert_eq!(loaded, settings);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn unknown_keys_and_missing_sections_fall_back_to_defaults() {
        let path = temp_path("forward");
        fs::write(
            &path,
            "[playback]\nvolume = 0.5\nfuture-key = true\n\n[not-a-section]\nx = 1\n",
        )
        .expect("write");
        let (settings, _) = Settings::load_or_create(&path).expect("load");
        assert!((settings.playback.volume - 0.5).abs() < f32::EPSILON);
        assert_eq!(settings.lyrics, Lyrics::default());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let path = temp_path("clamp");
        fs::write(
            &path,
            "[playback]\nvolume = 4.0\n\n[lyrics]\nfont-scale = 9.0\n\n[performance]\ncache-size-mb = 999999999\n",
        )
        .expect("write");
        let (settings, _) = Settings::load_or_create(&path).expect("load");
        assert!((settings.playback.volume - 1.0).abs() < f32::EPSILON);
        assert!((settings.lyrics.font_scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(settings.performance.cache_size_mb, 65_536);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn invalid_toml_reports_the_path() {
        let path = temp_path("broken");
        fs::write(&path, "this is not toml = =").expect("write");
        let err = Settings::load_or_create(&path).expect_err("must fail");
        assert!(matches!(err, PlatformError::Parse { .. }), "{err}");
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn save_leaves_no_temporary_file() {
        let path = temp_path("atomic");
        Settings::default().save(&path).expect("save");
        assert!(path.exists());
        assert!(!path.with_extension("toml.tmp").exists());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn empty_and_duplicate_folders_are_dropped() {
        let mut settings = Settings::default();
        settings.library.music_folders = vec![
            PathBuf::from("/music"),
            PathBuf::new(),
            PathBuf::from("/music"),
        ];
        settings.normalize();
        assert_eq!(
            settings.library.music_folders,
            vec![PathBuf::from("/music")]
        );
    }

    #[test]
    fn motion_is_disabled_by_any_switch() {
        let mut settings = Settings::default();
        assert!(!settings.motion_disabled());
        settings.performance.reduced_motion = true;
        assert!(settings.motion_disabled());

        let mut settings = Settings::default();
        settings.appearance.animation_intensity = 0.0;
        assert!(settings.motion_disabled());

        let mut settings = Settings::default();
        settings.lyrics.animation = AnimationProfile::None;
        assert!(settings.motion_disabled());
    }
}
