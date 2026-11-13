//! Aurora Player entry point.
//!
//! Phase 0 wires the crates together and proves the seams work end to end:
//!
//! ```text
//!  scan folder ──► catalog ──► find lyrics ──► estimate word timing ──► frame plan
//! ```
//!
//! The real window (iced + wgpu/tiny-skia) arrives in Phase 6, once the UI stack
//! decision from the Phase 0 spike is locked in.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use aurora_core::clock::{ClockSnapshot, PlaybackClock};
use aurora_core::model::TrackLocation;
use aurora_core::traits::PathStorage;
use aurora_db::{Catalog, MemoryCatalog};
use aurora_library::{FileNameProbe, scan};
use aurora_lyrics::{discover, estimate_word_timing, parse_lrc};
use aurora_ui::{Layout, LyricView};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        PathBuf::from,
    );
    println!("Aurora Player {}", env!("CARGO_PKG_VERSION"));
    println!("library root: {}", root.display());

    let catalog = MemoryCatalog::new();
    let report = scan(
        &PathStorage,
        &FileNameProbe,
        &catalog,
        &TrackLocation::Path(root.clone()),
    )?;
    println!(
        "scan: {} visited, {} added, {} skipped",
        report.visited, report.added, report.skipped
    );

    let tracks = catalog.all_tracks()?;
    let Some(track) = tracks.first() else {
        println!("no audio files found; nothing else to do");
        return Ok(());
    };
    println!("first track: {}", track.display_name());

    let lyrics_dir = root.join("lyrics");
    let mut doc = match discover(&PathStorage, track, &lyrics_dir) {
        aurora_lyrics::Lookup::Found(doc) => doc,
        aurora_lyrics::Lookup::Missing { tried } => {
            println!("no lyrics found (tried: {tried:?}); using the bundled sample");
            parse_lrc(SAMPLE, aurora_core::lyrics::LyricSource::Embedded)
        }
    };
    estimate_word_timing(&mut doc);
    println!(
        "lyrics: {} lines (word-timed: {})",
        doc.lines.len(),
        doc.word_timed
    );

    // Exercise the clock + view for one second of simulated playback.
    let clock = PlaybackClock::new();
    let start = Instant::now();
    clock.publish(Duration::ZERO, start);
    clock.set_playing(true);

    let mut view = LyricView::new(Layout::default());
    let mut lit = 0;
    for frame in 0..60 {
        let now = start + Duration::from_millis(frame * 16);
        let snapshot: ClockSnapshot = clock.snapshot();
        let plan = view.frame(&doc, snapshot, now);
        lit = plan.words.len();
    }
    println!(
        "after 1s: active line {:?}, {} word(s) highlighted, scroll {:.1}px",
        doc.active_line(aurora_core::lyrics::Millis(1_000)),
        lit,
        view.scroll_position()
    );
    Ok(())
}

const SAMPLE: &str = "[00:00.50]سَلامٌ على الدُّنيا\n[00:03.00]Hello Aurora في الظَّلام\n";
