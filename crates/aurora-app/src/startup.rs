//! Everything that must happen before the first frame.
//!
//! Boot order is deliberate:
//!
//! 1. logging (so a failure in step 2 is visible),
//! 2. directories (create them, or fail loudly - never silently write elsewhere),
//! 3. settings (create on first run, clamp on every run),
//! 4. session detection (window vs. headless).
//!
//! The result, [`Boot`], is the only thing the two front ends (windowed shell
//! and headless run) receive; neither of them performs I/O of its own at start.

use std::path::PathBuf;

use aurora_platform::settings::{Settings, Source};
use aurora_platform::{
    PlatformError, Session, XdgDirectories, detect_session, wayland_socket_ready,
};
use tracing::{info, warn};

use crate::cli::Args;

/// Resolved startup state.
pub struct Boot {
    pub dirs: XdgDirectories,
    pub settings: Settings,
    pub settings_path: PathBuf,
    pub session: Session,
}

/// Errors that stop the app before it can draw anything.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

/// Runs the boot sequence.
///
/// # Errors
///
/// Fails when the directories cannot be created or the settings file exists but
/// cannot be read or parsed. In both cases there is nothing sensible to draw.
pub fn boot(args: &Args) -> Result<Boot, BootError> {
    let dirs = match &args.config_root {
        Some(root) => {
            // A single root is what tests and portable installs want: one
            // directory the user can delete.
            XdgDirectories::new(root.clone(), root.clone(), root.clone())
        }
        None => XdgDirectories::from_env()?,
    };
    dirs.ensure()?;
    info!(
        config = %dirs.config_dir().display(),
        data = %dirs.data_dir().display(),
        cache = %dirs.cache_dir().display(),
        "directories ready"
    );

    let settings_path = dirs.settings_file();
    let (settings, source) = Settings::load_or_create(&settings_path)?;
    match source {
        Source::File => info!(path = %settings_path.display(), "settings loaded"),
        Source::CreatedDefault => {
            info!(path = %settings_path.display(), "settings file created from defaults");
        }
    }

    let mut session = if args.headless {
        Session::Headless
    } else {
        detect_session()
    };
    if session == Session::Wayland && !wayland_socket_ready() {
        // iced *panics* when the compositor connection fails, so the socket is
        // verified here, before any toolkit code runs.
        warn!("WAYLAND_DISPLAY is set but no compositor answers; starting headless");
        session = Session::Headless;
    }
    if session == Session::X11Only {
        warn!(
            "an X11 display is present but this build only supports Wayland; \
             starting headless (see PHASE_LOG.md)"
        );
    }

    Ok(Boot {
        dirs,
        settings,
        settings_path,
        session,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("aurora-boot-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        root
    }

    #[test]
    fn boot_creates_directories_and_a_settings_file() {
        let root = temp_root("fresh");
        let args = Args {
            config_root: Some(root.clone()),
            headless: true,
            ..Args::default()
        };
        let state = boot(&args).expect("boot");
        assert!(state.settings_path.exists());
        assert!(state.dirs.lyrics_dir().is_dir());
        assert!(state.dirs.data_dir().is_dir());
        assert_eq!(state.session, Session::Headless);
        assert_eq!(state.settings, Settings::default());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn second_boot_reads_the_file_back() {
        let root = temp_root("reload");
        let args = Args {
            config_root: Some(root.clone()),
            headless: true,
            ..Args::default()
        };
        let mut state = boot(&args).expect("first boot");
        state.settings.playback.volume = 0.25;
        state.settings.save(&state.settings_path).expect("save");

        let again = boot(&args).expect("second boot");
        assert!((again.settings.playback.volume - 0.25).abs() < f32::EPSILON);
        std::fs::remove_dir_all(&root).ok();
    }
}
