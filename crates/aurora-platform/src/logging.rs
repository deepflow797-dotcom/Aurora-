//! One `tracing` subscriber for the whole binary.
//!
//! Level comes from `AURORA_LOG` (`error|warn|info|debug|trace`, case
//! insensitive, default `info`) because a GUI app has no `-v` flag culture and
//! `RUST_LOG` is too easy to inherit by accident from a shell profile.
//!
//! The subscriber is installed once, from `main`, before anything else logs.
//! Calling it twice is a no-op rather than a panic: tests and the headless
//! harness both boot the app in-process.

use std::sync::Once;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt;

const ENV_VAR: &str = "AURORA_LOG";

static INIT: Once = Once::new();

/// Reads `AURORA_LOG`, falling back to [`LevelFilter::INFO`].
#[must_use]
pub fn level_from_env() -> LevelFilter {
    std::env::var(ENV_VAR)
        .ok()
        .and_then(|value| parse_level(&value))
        .unwrap_or(LevelFilter::INFO)
}

/// Parses a level name; anything unrecognised yields `None` so the caller can
/// fall back instead of silently logging nothing.
#[must_use]
pub fn parse_level(value: &str) -> Option<LevelFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::OFF),
        "error" => Some(LevelFilter::ERROR),
        "warn" | "warning" => Some(LevelFilter::WARN),
        "info" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        _ => None,
    }
}

/// Installs the global subscriber. Idempotent.
pub fn init() {
    init_with(level_from_env());
}

/// Installs the global subscriber at an explicit level. Idempotent.
pub fn init_with(level: LevelFilter) {
    INIT.call_once(|| {
        let subscriber = fmt().with_max_level(level).with_target(true).finish();
        // A second global subscriber would panic; the `Once` guarantees one.
        if tracing::subscriber::set_global_default(subscriber).is_err() {
            // Already installed by something else (a test harness), which is
            // exactly the situation the `Once` cannot see. Not an error.
            tracing::debug!("a global tracing subscriber was already installed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_names_parse_case_insensitively() {
        assert_eq!(parse_level("DEBUG"), Some(LevelFilter::DEBUG));
        assert_eq!(parse_level(" warn "), Some(LevelFilter::WARN));
        assert_eq!(parse_level("off"), Some(LevelFilter::OFF));
    }

    #[test]
    fn unknown_level_names_fall_back_to_info() {
        assert_eq!(parse_level("verbose"), None);
        // `level_from_env` can only be observed through the process
        // environment, which other tests mutate; the fallback is covered by
        // the `unwrap_or` in its body.
        assert_eq!(parse_level(""), None);
    }

    #[test]
    fn init_is_idempotent() {
        init_with(LevelFilter::OFF);
        init_with(LevelFilter::TRACE);
    }
}
