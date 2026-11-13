//! The no-window boot path.
//!
//! Used by `--headless`, by `AURORA_HEADLESS=1`, and automatically when no
//! Wayland compositor is advertised. It is not a stub: settings are loaded and
//! clamped, the configured music folders are scanned into the catalog, and the
//! summary is logged, so the same code paths the window would take run *and*
//! report. CI and this container exercise it on every build.

use std::path::PathBuf;

use aurora_core::model::TrackLocation;
use aurora_core::traits::PathStorage;
use aurora_db::{Catalog, MemoryCatalog};
use aurora_library::{FileNameProbe, scan};
use tracing::{info, warn};

use crate::startup::Boot;

/// What a headless run did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeadlessReport {
    pub folders_scanned: usize,
    pub tracks_found: usize,
}

/// Runs the headless boot: scan configured folders, log the summary.
///
/// # Errors
///
/// Fails only on a catalog error; an unreadable folder is reported as a warning
/// and skipped, because one bad path must not stop the app from starting.
pub fn run(boot: &Boot) -> Result<HeadlessReport, aurora_db::DbError> {
    let catalog = MemoryCatalog::new();
    let mut report = HeadlessReport::default();

    println!(
        "Aurora Player {} ({} mode)",
        env!("CARGO_PKG_VERSION"),
        boot.session.label()
    );
    println!("settings: {}", boot.settings_path.display());
    println!(
        "lyrics: enabled={} animation={:?} word-highlighting={} | motion disabled={}",
        boot.settings.lyrics.enabled,
        boot.settings.lyrics.animation,
        boot.settings.lyrics.word_highlighting,
        boot.settings.motion_disabled()
    );
    println!(
        "library: {} folder(s), watch={} rescan-on-start={}",
        boot.settings.library.music_folders.len(),
        boot.settings.library.watch_folders,
        boot.settings.library.rescan_on_start
    );

    for folder in &boot.settings.library.music_folders {
        let location = TrackLocation::Path(folder.clone());
        match scan(&PathStorage, &FileNameProbe, &catalog, &location) {
            Ok(summary) => {
                report.folders_scanned += 1;
                report.tracks_found += summary.added;
                println!(
                    "  {}: {} visited, {} added, {} skipped",
                    folder.display(),
                    summary.visited,
                    summary.added,
                    summary.skipped
                );
            }
            Err(err) => warn!(folder = %folder.display(), %err, "skipping unreadable folder"),
        }
    }

    if boot.settings.library.music_folders.is_empty() {
        println!("no music folders configured yet; add one from Settings");
        let suggestions = suggested_folders();
        if !suggestions.is_empty() {
            println!(
                "  found on this machine: {}",
                suggestions
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    info!(
        folders = report.folders_scanned,
        tracks = report.tracks_found,
        catalog = catalog.len()?,
        data = %boot.dirs.data_dir().display(),
        "headless boot complete"
    );
    Ok(report)
}

/// Folders a fresh install should suggest, in order of preference.
///
/// Only used by the UI's "Add music folder" affordance later; kept here so the
/// list is testable without a display.
#[must_use]
pub fn suggested_folders() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    ["Music", "music", "Downloads/Music"]
        .into_iter()
        .map(|name| home.join(name))
        .filter(|path| path.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_platform::settings::Settings;
    use aurora_platform::{Session, XdgDirectories};

    fn boot_with(root: &std::path::Path, folders: Vec<PathBuf>) -> Boot {
        let dirs = XdgDirectories::new(root.into(), root.into(), root.into());
        dirs.ensure().expect("dirs");
        let mut settings = Settings::default();
        settings.library.music_folders = folders;
        let settings_path = dirs.settings_file();
        settings.save(&settings_path).expect("save");
        Boot {
            dirs,
            settings,
            settings_path,
            session: Session::Headless,
        }
    }

    #[test]
    fn scanning_a_folder_counts_new_tracks() {
        let root = std::env::temp_dir().join(format!("aurora-headless-{}", std::process::id()));
        let music = root.join("music");
        std::fs::create_dir_all(&music).expect("music dir");
        std::fs::write(music.join("song.flac"), b"not really audio").expect("write");
        std::fs::write(music.join("notes.txt"), b"ignored").expect("write");

        let boot = boot_with(&root, vec![music.clone()]);
        let report = run(&boot).expect("headless run");
        assert_eq!(
            report,
            HeadlessReport {
                folders_scanned: 1,
                tracks_found: 1
            }
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_missing_folder_is_skipped_not_fatal() {
        let root =
            std::env::temp_dir().join(format!("aurora-headless-missing-{}", std::process::id()));
        let boot = boot_with(&root, vec![root.join("does-not-exist")]);
        let report = run(&boot).expect("headless run");
        assert_eq!(report, HeadlessReport::default());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn suggestions_are_directories_that_exist() {
        for folder in suggested_folders() {
            assert!(folder.is_dir(), "{}", folder.display());
        }
    }
}
