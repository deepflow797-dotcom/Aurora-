//! Errors from the platform layer.
//!
//! One enum for the whole crate: the binary only has to print these, and any
//! failure here is fatal at startup (no config directory, settings file that is
//! valid TOML but wrong types, ...). Each variant names the path it touched so
//! the message is actionable without a backtrace.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("cannot determine the user's home directory (HOME is unset)")]
    NoHomeDirectory,

    #[error("cannot create {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not valid TOML: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cannot serialise settings: {0}")]
    Serialise(#[from] toml::ser::Error),
}
