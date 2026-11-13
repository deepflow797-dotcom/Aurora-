//! Audio I/O boundary.
//!
//! Phase 0 only defines the contract; the concrete PipeWire (`cpal`) backend and
//! the `symphonia` decoder land in Phase 1. The point of having this crate now is
//! that the *shape* of the boundary is fixed before anybody writes DSP code:
//!
//! ```text
//!  decoder thread            clock source            render thread
//!  ──────────────            ────────────            ─────────────
//!  symphonia decode  ──►  AudioOutput::position()  ──►  PlaybackClock::publish()
//! ```
//!
//! The decoder never owns the position; the device does. That is what keeps the
//! lyrics in sync on a machine with a 512-frame PipeWire quantum.

use std::time::Duration;

use aurora_core::traits::AudioOutput;

/// Output backends Aurora plans to support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// `cpal` on top of PipeWire (Fedora default; falls back to ALSA/PulseAudio).
    #[default]
    PipeWire,
    /// `oboe`/AAudio (Android, Phase 9+).
    AAudio,
    /// A silent backend used by tests and headless benchmarks.
    Null,
}

/// A decoder configuration negotiated with the output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
        }
    }
}

/// Silent [`AudioOutput`], useful for tests, CI and the Phase 0 spike host.
///
/// It models a device that consumes exactly `frames_per_tick` frames each time
/// [`Self::tick`] is called, which is how the spike exercises the clock without
/// hardware.
#[derive(Debug, Clone)]
pub struct NullOutput {
    format: OutputFormat,
    frames: u64,
    playing: bool,
    volume: f32,
    frames_per_tick: u64,
}

impl NullOutput {
    #[must_use]
    pub fn new(format: OutputFormat, frames_per_tick: u64) -> Self {
        Self {
            format,
            frames: 0,
            playing: false,
            volume: 1.0,
            frames_per_tick,
        }
    }

    /// Advances the simulated device by one callback's worth of frames.
    pub fn tick(&mut self) {
        if self.playing {
            self.frames += self.frames_per_tick;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("device unavailable: {0}")]
    DeviceUnavailable(String),
    #[error("decoder error: {0}")]
    Decode(String),
}

impl AudioOutput for NullOutput {
    type Error = AudioError;

    fn open(sample_rate: u32, channels: u16) -> Result<Self, Self::Error> {
        Ok(Self::new(
            OutputFormat {
                sample_rate,
                channels,
            },
            480,
        ))
    }

    fn play(&mut self) -> Result<(), Self::Error> {
        self.playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), Self::Error> {
        self.playing = false;
        Ok(())
    }

    fn seek(&mut self, position: Duration) -> Result<(), Self::Error> {
        self.frames = (position.as_secs_f64() * f64::from(self.format.sample_rate)) as u64;
        Ok(())
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    fn frames_played(&self) -> u64 {
        self.frames
    }

    fn sample_rate(&self) -> u32 {
        self.format.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_map_to_position() {
        let mut out = NullOutput::new(
            OutputFormat {
                sample_rate: 48_000,
                channels: 2,
            },
            480,
        );
        out.play().unwrap();
        for _ in 0..100 {
            out.tick();
        }
        // 48_000 frames of simulated consumption = exactly one second.
        assert_eq!(out.position(), Duration::from_millis(1_000));
    }

    #[test]
    fn seek_resets_the_counter() {
        let mut out = NullOutput::new(OutputFormat::default(), 480);
        out.play().unwrap();
        out.tick();
        out.seek(Duration::from_secs(30)).unwrap();
        assert_eq!(out.position(), Duration::from_secs(30));
    }
}
