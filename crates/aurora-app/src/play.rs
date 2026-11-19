//! The `aurora --play <FILE>` command: simulated playback, no sound card.
//!
//! Playback cannot be *heard* here (no ALSA, no PipeWire), but everything except
//! the device driver can be exercised: the decoder, the engine's frame
//! arithmetic, the clock publishing, seeks and end-of-stream. This command runs
//! the same [`PlaybackEngine`] a cpal callback would drive, against
//! [`NullDevice`], and prints what the UI would have shown.
//!
//! That is worth having: the engine bugs that matter (position drift, end of
//! stream never arriving, a seek that leaves the clock behind) are arithmetic
//! bugs and they show up here exactly as they would on the Fedora machine.

use std::path::Path;

use std::sync::Arc;
use std::time::{Duration, Instant};

use aurora_audio::engine::{DecoderSource, NullDevice, PlaybackEngine};
use aurora_audio::wav::DecodeError;
use aurora_core::clock::PlaybackClock;
use tracing::info;

/// What a simulated playback did.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackReport {
    /// Audio actually rendered, from the engine's frame counter.
    pub delivered: Duration,
    /// Duration the file claims, when the container says.
    pub duration: Option<Duration>,
    /// Device callbacks used.
    pub callbacks: usize,
    /// Position the clock reported at the end.
    pub final_position: Duration,
}

/// Plays `path` through a device-free engine.
///
/// `seconds` limits how much audio is rendered (`None` = play to the end).
///
/// # Errors
///
/// Propagates open/decode errors.
pub fn simulate(path: &Path, seconds: Option<Duration>) -> Result<PlaybackReport, DecodeError> {
    let decoder = aurora_audio::open(path)?;
    let clock = Arc::new(PlaybackClock::new());
    let source = DecoderSource::new(decoder, 1.0);
    let engine = PlaybackEngine::new(Box::new(source), clock.clone());

    // 480 frames ≈ the 10 ms quantum a PipeWire graph uses by default.
    let mut device = NullDevice::new(engine, 480);
    let duration = device.engine().duration();
    device.engine_mut().play();

    let start = Instant::now();
    let limit = seconds.unwrap_or(Duration::MAX);
    let mut callbacks = 0;
    while device.engine().is_playing() && !device.engine().is_finished() {
        // A synthetic clock: each callback is one quantum of device time.
        let now = start + Duration::from_micros(callbacks as u64 * 10_000);
        device.tick(now)?;
        callbacks += 1;
        if device.engine().position() >= limit {
            device.engine_mut().pause();
        }
        if callbacks > 10_000_000 {
            break;
        }
    }

    let report = PlaybackReport {
        delivered: device.engine().position(),
        duration,
        callbacks,
        final_position: clock.snapshot().position,
    };
    info!(
        path = %path.display(),
        delivered_ms = report.delivered.as_millis(),
        callbacks = report.callbacks,
        "simulated playback finished"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music")
            .join(name)
    }

    #[test]
    fn simulating_a_flac_file_reports_its_full_length() {
        let report = simulate(&fixture("00 - Fixture Steps.flac"), None).expect("simulate");
        assert_eq!(report.duration, Some(Duration::from_micros(170_666)));
        assert_eq!(report.delivered, Duration::from_micros(170_666));
        assert_eq!(report.final_position, report.delivered);
        // 8192 frames / 480 per callback = 17.07 → 18 callbacks.
        assert_eq!(report.callbacks, 18);
    }

    #[test]
    fn a_time_limit_stops_early() {
        let report = simulate(
            &fixture("00 - Fixture Steps.flac"),
            Some(Duration::from_millis(50)),
        )
        .expect("simulate");
        assert!(report.delivered >= Duration::from_millis(50));
        assert!(report.delivered < Duration::from_millis(80));
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(simulate(Path::new("/no/such.wav"), None).is_err());
    }
}
