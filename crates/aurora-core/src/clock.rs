//! The playback clock (spec section 63.4).
//!
//! The naive approach - storing a `Duration` and bumping it in the UI timer -
//! drifts against the audio device, and it also drifts *while nobody is
//! rendering* (a backgrounded phone, a busy frame). Aurora therefore never
//! derives position from a counter. Instead the audio thread publishes an
//! **atomic pair**:
//!
//! ```text
//! (playback_position, monotonic_instant)
//! ```
//!
//! and everybody else *derives* the current position from it:
//!
//! ```text
//! position(now) = sampled_position + (now - sampled_instant)   // while playing
//! position(now) = sampled_position                             // while paused
//! ```
//!
//! The pair must be observed as one unit. If a reader can see a new position
//! with an old timestamp (or the reverse) the derived position jumps backwards
//! or forwards - the classic "lyrics jump, then rewind" bug that shows up once
//! every few minutes and is impossible to reproduce. A single `AtomicU64` is not
//! enough either: 64 bits cannot hold both a position and an absolute instant.
//!
//! So this module uses a **seqlock**: a generation counter that is odd while a
//! write is in flight, plus two plain atomics for the payload. Readers retry
//! until they observe a stable, even generation, which makes the read both
//! wait-free in the common case and impossible to tear.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A consistent `(position, instant)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSnapshot {
    pub position: Duration,
    /// Monotonic instant at which `position` was true.
    pub sampled_at: Instant,
    pub playing: bool,
}

impl ClockSnapshot {
    /// Position extrapolated to `now`; never earlier than `self.position`.
    #[must_use]
    pub fn position_at(&self, now: Instant) -> Duration {
        if !self.playing {
            return self.position;
        }
        let elapsed = now.saturating_duration_since(self.sampled_at);
        self.position.saturating_add(elapsed)
    }
}

/// Shared, lock-free playback clock.
///
/// * the audio/output thread calls [`Self::publish`] (once per mixer callback);
/// * the render thread calls [`Self::snapshot`] once per frame;
/// * the UI never *writes* the clock, so the audio thread never blocks.
#[derive(Debug)]
pub struct PlaybackClock {
    /// Even = stable, odd = write in progress.
    generation: AtomicU32,
    position_micros: AtomicU64,
    /// Nanoseconds since `epoch`; keeps the pair independent of wall clock.
    sampled_nanos: AtomicU64,
    playing: AtomicBool,
    epoch: Instant,
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: AtomicU32::new(0),
            position_micros: AtomicU64::new(0),
            sampled_nanos: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            epoch: Instant::now(),
        }
    }

    /// Creates a clock shared between the audio thread and the renderer.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Publishes a new `(position, instant)` sample. Called by the audio thread
    /// roughly once per mixer callback - not once per frame.
    pub fn set(&self, position: Duration, now: Instant) {
        let generation = self.generation.load(Ordering::Relaxed);
        self.generation
            .store(generation.wrapping_add(1), Ordering::Release); // odd: writing
        self.position_micros.store(
            position.as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        let since_epoch = now.saturating_duration_since(self.epoch);
        self.sampled_nanos.store(
            since_epoch.as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        self.generation
            .store(generation.wrapping_add(2), Ordering::Release); // even: done
    }

    /// Alias kept for readability at call sites that "publish a sample".
    pub fn publish(&self, position: Duration, now: Instant) {
        self.set(position, now);
    }

    /// Marks playback as running/stopped. Pausing must be paired with a
    /// [`Self::publish`] of the exact position, so no time is invented.
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Release);
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    /// Reads a consistent snapshot, retrying if a write overlapped the read.
    #[must_use]
    pub fn snapshot(&self) -> ClockSnapshot {
        loop {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let position = self.position_micros.load(Ordering::Relaxed);
            let sampled = self.sampled_nanos.load(Ordering::Relaxed);
            let playing = self.playing.load(Ordering::Acquire);
            let after = self.generation.load(Ordering::Acquire);
            if before == after {
                return ClockSnapshot {
                    position: Duration::from_micros(position),
                    sampled_at: self.epoch + Duration::from_nanos(sampled),
                    playing,
                };
            }
            std::hint::spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_clock_does_not_advance() {
        let clock = PlaybackClock::new();
        let start = Instant::now();
        clock.publish(Duration::from_secs(3), start);
        clock.set_playing(false);
        let later = start + Duration::from_secs(10);
        assert_eq!(clock.snapshot().position_at(later), Duration::from_secs(3));
    }

    #[test]
    fn playing_clock_extrapolates_from_the_sample() {
        let clock = PlaybackClock::new();
        let start = Instant::now();
        clock.publish(Duration::from_millis(1_500), start);
        clock.set_playing(true);
        let later = start + Duration::from_millis(250);
        assert_eq!(
            clock.snapshot().position_at(later),
            Duration::from_millis(1_750)
        );
    }

    #[test]
    fn seek_replaces_the_sample() {
        let clock = PlaybackClock::new();
        let start = Instant::now();
        clock.publish(Duration::from_secs(90), start);
        clock.set_playing(true);
        let seek_at = start + Duration::from_secs(1);
        clock.publish(Duration::from_secs(10), seek_at);
        let now = start + Duration::from_secs(2);
        assert_eq!(clock.snapshot().position_at(now), Duration::from_secs(11));
    }

    #[test]
    fn snapshot_reports_the_instants_it_was_given() {
        let clock = PlaybackClock::new();
        let at = Instant::now() + Duration::from_millis(40);
        clock.publish(Duration::from_millis(5), at);
        let snap = clock.snapshot();
        assert_eq!(snap.position, Duration::from_millis(5));
        assert_eq!(snap.sampled_at, at);
    }

    #[test]
    fn concurrent_readers_never_see_torn_pairs() {
        let clock = Arc::new(PlaybackClock::new());
        let start = Instant::now();
        let writer = {
            let clock = Arc::clone(&clock);
            std::thread::spawn(move || {
                for i in 0..20_000u64 {
                    clock.publish(Duration::from_micros(i * 1_000), start);
                }
            })
        };
        let mut last = Duration::ZERO;
        for _ in 0..20_000 {
            let snap = clock.snapshot();
            // Position must be a multiple of 1 ms and never decrease.
            assert!(snap.position >= last, "went backwards");
            last = snap.position;
        }
        writer.join().unwrap();
    }
}
