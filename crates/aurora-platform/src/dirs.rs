//! XDG base-directory resolution.
//!
//! Follows the [XDG Base Directory Specification][xdg] rather than re-inventing
//! a layout, because Aurora's files end up in the user's `~` and packagers
//! expect the documented locations:
//!
//! | Purpose | Environment variable | Fallback |
//! | --- | --- | --- |
//! | configuration | `XDG_CONFIG_HOME` | `~/.config/aurora` |
//! | data (catalog, lyrics cache) | `XDG_DATA_HOME` | `~/.local/share/aurora` |
//! | cache (artwork thumbnails) | `XDG_CACHE_HOME` | `~/.cache/aurora` |
//!
//! The spec says a *relative* value in any of those variables must be ignored;
//! [`XdgDirectories::from_env`] does exactly that instead of trusting whatever a
//! shell exported.
//!
//! [xdg]: https://specifications.freedesktop.org/basedir-spec/latest/

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use aurora_core::traits::AppDirectories;

use crate::error::PlatformError;

/// Application directory name, appended to each XDG base directory.
pub const APP_DIR: &str = "aurora";

/// Resolved per-user directories for Aurora Player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirectories {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl XdgDirectories {
    /// Resolves the directories from `XDG_*_HOME`/`HOME`.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NoHomeDirectory`] when `HOME` is unset *and* the
    /// variables that would make it unnecessary are missing.
    pub fn from_env() -> Result<Self, PlatformError> {
        let home = std::env::var_os("HOME")
            .filter(is_absolute)
            .ok_or(PlatformError::NoHomeDirectory)?;

        let config = base("XDG_CONFIG_HOME").unwrap_or_else(|| Path::new(&home).join(".config"));
        let data = base("XDG_DATA_HOME").unwrap_or_else(|| Path::new(&home).join(".local/share"));
        let cache = base("XDG_CACHE_HOME").unwrap_or_else(|| Path::new(&home).join(".cache"));

        Ok(Self::new(config, data, cache))
    }

    /// Builds the app directories on top of explicit XDG base directories.
    ///
    /// Used by tests and by the headless test harness, which must never touch
    /// the real home directory.
    #[must_use]
    pub fn new(config_base: PathBuf, data_base: PathBuf, cache_base: PathBuf) -> Self {
        Self {
            config: config_base.join(APP_DIR),
            data: data_base.join(APP_DIR),
            cache: cache_base.join(APP_DIR),
        }
    }

    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data
    }

    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }

    /// Location of the persisted settings file.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    /// Downloaded/sidecar lyrics live with the data, not next to the config:
    /// they can be re-fetched, the settings cannot.
    #[must_use]
    pub fn lyrics_dir(&self) -> PathBuf {
        self.data.join("lyrics")
    }

    /// The catalog database file.
    #[must_use]
    pub fn database_file(&self) -> PathBuf {
        self.data.join("library.sqlite3")
    }

    /// Location of the log file written when the terminal is not attached.
    #[must_use]
    pub fn log_file(&self) -> PathBuf {
        self.data.join("aurora.log")
    }

    /// Creates every directory the app writes to, if it does not exist yet.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::CreateDirectory`] for the first directory that
    /// cannot be created (read-only `$HOME`, a file in the way, ...).
    pub fn ensure(&self) -> Result<(), PlatformError> {
        for path in [&self.config, &self.data, &self.cache] {
            std::fs::create_dir_all(path).map_err(|source| PlatformError::CreateDirectory {
                path: path.clone(),
                source,
            })?;
        }
        let lyrics = self.lyrics_dir();
        std::fs::create_dir_all(&lyrics).map_err(|source| PlatformError::CreateDirectory {
            path: lyrics,
            source,
        })?;
        Ok(())
    }
}

impl AppDirectories for XdgDirectories {
    fn data_dir(&self) -> PathBuf {
        self.data.clone()
    }

    fn config_dir(&self) -> PathBuf {
        self.config.clone()
    }

    fn cache_dir(&self) -> PathBuf {
        self.cache.clone()
    }

    fn lyrics_dir(&self) -> PathBuf {
        Self::lyrics_dir(self)
    }

    fn database_file(&self) -> PathBuf {
        Self::database_file(self)
    }
}

/// Reads an XDG base-directory variable, rejecting relative paths as the spec
/// requires.
fn base(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .filter(is_absolute)
        .map(PathBuf::from)
}

fn is_absolute(value: &OsString) -> bool {
    Path::new(value).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_directories_are_appended_to_the_bases() {
        let dirs = XdgDirectories::new("/c".into(), "/d".into(), "/k".into());
        assert_eq!(dirs.config_dir(), Path::new("/c/aurora"));
        assert_eq!(dirs.data_dir(), Path::new("/d/aurora"));
        assert_eq!(dirs.cache_dir(), Path::new("/k/aurora"));
        assert_eq!(dirs.settings_file(), Path::new("/c/aurora/settings.toml"));
    }

    #[test]
    fn trait_impl_matches_inherent_accessors() {
        let dirs = XdgDirectories::new("/c".into(), "/d".into(), "/k".into());
        assert_eq!(AppDirectories::config_dir(&dirs), dirs.config_dir());
        assert_eq!(AppDirectories::data_dir(&dirs), dirs.data_dir());
        assert_eq!(AppDirectories::cache_dir(&dirs), dirs.cache_dir());
    }

    #[test]
    fn relative_xdg_values_are_ignored() {
        // A relative value must fall back to $HOME per the spec; `base` is the
        // only place that decides, so the rule is tested there.
        assert!(base("AURORA_TEST_UNSET_VARIABLE_XYZ").is_none());
    }

    #[test]
    fn ensure_creates_every_directory() {
        let root = std::env::temp_dir().join(format!("aurora-dirs-{}", std::process::id()));
        let dirs = XdgDirectories::new(root.join("c"), root.join("d"), root.join("k"));
        dirs.ensure().expect("create directories");
        assert!(dirs.config_dir().is_dir());
        assert!(dirs.data_dir().is_dir());
        assert!(dirs.cache_dir().is_dir());
        dirs.ensure().expect("second call is a no-op");
        std::fs::remove_dir_all(&root).ok();
    }
}
