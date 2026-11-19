//! The playback engine: source → output → clock.
//!
//! The shape comes from spec §63.4 and is the reason the clock exists at all:
//!
//! ```text
//!   device callback ──► AudioSource::fill ──► samples
//!                            │
//!                            └─► PlaybackEngine::pull ──► clock.publish(frames/rate, now)
//! ```
//!
//! **The device drives, the engine counts, the clock publishes.** Nothing in the
//! UI thread ever advances a position, which is what keeps lyrics in sync when a
//! frame is late or the window is occluded.
//!
//! Why a separate [`AudioSource`] trait instead of calling [`Decoder`] directly:
//! a real backend (cpal, AAudio) hands the engine a *pull* callback with a fixed
//! frame budget, and the source is where volume, gapless hand-off and any future
//! channel mapping belong. Keeping that boundary explicit means the cpal backend
//! will be a thin wrapper, and the engine is testable with [`NullDevice`] today.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use aurora_core::clock::PlaybackClock;

use crate::wav::{DecodeError, Decoder};

/// Interleaved `f32` audio, pulled by an output device.
pub trait AudioSource: Send {
    /// Sample rate the source produces.
    fn sample_rate(&self) -> u32;
    /// Interleaved channel count.
    fn channels(&self) -> u16;
    /// Writes interleaved samples into `out`, returning how many were written.
    /// A short write means end of stream.
    ///
    /// # Errors
    ///
    /// Propagates decoder failures.
    fn fill(&mut self, out: &mut [f32]) -> Result<usize, DecodeError>;
    /// Requests a position and reports where the source actually landed.
    ///
    /// # Errors
    ///
    /// Propagates decoder seek failures.
    fn seek(&mut self, position: Duration) -> Result<Duration, DecodeError>;

    /// Total frames in the stream, when the format says.
    fn total_frames(&self) -> Option<u64> {
        None
    }
}

/// A [`Decoder`] plus gain, as an [`AudioSource`].
pub struct DecoderSource {
    decoder: Box<dyn Decoder>,
    gain: f32,
}

impl DecoderSource {
    /// Wraps any decoder. `gain` is linear (`1.0` = unchanged).
    pub fn new(decoder: Box<dyn Decoder>, gain: f32) -> Self {
        Self {
            decoder,
            gain: gain.clamp(0.0, 4.0),
        }
    }

    /// Sets the linear gain applied to every sample the source produces.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 4.0);
    }
}

impl AudioSource for DecoderSource {
    fn sample_rate(&self) -> u32 {
        self.decoder.format().sample_rate
    }

    fn channels(&self) -> u16 {
        self.decoder.format().channels
    }

    fn fill(&mut self, out: &mut [f32]) -> Result<usize, DecodeError> {
        let written = self.decoder.read(out)?;
        if self.gain != 1.0 {
            for sample in &mut out[..written] {
                *sample *= self.gain;
            }
        }
        Ok(written)
    }

    fn seek(&mut self, position: Duration) -> Result<Duration, DecodeError> {
        let frame = self.decoder.seek(position)?;
        Ok(Duration::from_micros(
            frame.saturating_mul(1_000_000) / u64::from(self.sample_rate().max(1)),
        ))
    }

    fn total_frames(&self) -> Option<u64> {
        self.decoder.total_frames()
    }
}

/// A device that pulls from an [`AudioSource`] and publishes the clock.
pub struct PlaybackEngine {
    source: Box<dyn AudioSource>,
    clock: Arc<PlaybackClock>,
    /// Frames handed to the device since the last seek.
    frames_delivered: u64,
    /// Frames the device has been told to expect at the position we seeked to.
    base_frames: u64,
    /// Live linear gain, applied to every sample handed to the device.
    ///
    /// It lives on the engine rather than on [`DecoderSource`] because a volume
    /// slider must not need to rebuild the source (and therefore re-open the
    /// file) while audio is running.
    gain: f32,
    playing: bool,
    finished: bool,
}

impl std::fmt::Debug for PlaybackEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackEngine")
            .field("sample_rate", &self.source.sample_rate())
            .field("channels", &self.source.channels())
            .field("frames_delivered", &self.frames_delivered)
            .field("playing", &self.playing)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl PlaybackEngine {
    /// Builds an engine over `source`, publishing into `clock`.
    #[must_use]
    pub fn new(source: Box<dyn AudioSource>, clock: Arc<PlaybackClock>) -> Self {
        Self {
            source,
            clock,
            frames_delivered: 0,
            base_frames: 0,
            gain: 1.0,
            playing: false,
            finished: false,
        }
    }

    /// Starts producing audio.
    pub fn play(&mut self) {
        self.playing = true;
        self.clock.set_playing(true);
        self.clock.publish(self.position(), Instant::now());
    }

    /// Stops producing audio but keeps the position.
    pub fn pause(&mut self) {
        self.playing = false;
        self.clock.publish(self.position(), Instant::now());
        self.clock.set_playing(false);
    }

    /// Sets the live linear gain (`1.0` = unchanged, clamped to `0.0 ..= 4.0`).
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 4.0);
    }

    /// The live linear gain.
    #[must_use]
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// True while the device would receive samples.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// True once the source returned a short write (end of stream).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Current position, derived from frames delivered rather than from a timer.
    #[must_use]
    pub fn position(&self) -> Duration {
        let frames = self.base_frames + self.frames_delivered;
        Duration::from_micros(frames.saturating_mul(1_000_000) / u64::from(self.sample_rate()))
    }

    /// Total playing time, when the source knows it.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.source.total_frames().map(|frames| {
            Duration::from_micros(
                frames.saturating_mul(1_000_000) / u64::from(self.sample_rate().max(1)),
            )
        })
    }

    /// Seeks to `position`, publishing the landed position.
    ///
    /// # Errors
    ///
    /// Propagates source seek errors; the engine keeps its old position.
    pub fn seek(&mut self, position: Duration) -> Result<Duration, DecodeError> {
        let landed = self.source.seek(position)?;
        let rate = u64::from(self.sample_rate().max(1));
        self.base_frames = landed.as_micros().saturating_mul(u128::from(rate)) as u64 / 1_000_000;
        self.frames_delivered = 0;
        self.finished = false;
        self.clock.publish(landed, Instant::now());
        Ok(landed)
    }

    /// Called by the device callback: fill `out`, advance, publish the clock.
    ///
    /// Returns the number of samples written. Zero means end of stream (or
    /// paused, in which case `out` is silence and the position does not move).
    ///
    /// # Errors
    ///
    /// Propagates source errors; the engine stops playing rather than repeating
    /// a broken read every callback.
    pub fn pull(&mut self, out: &mut [f32], now: Instant) -> Result<usize, DecodeError> {
        if !self.playing || self.finished {
            out.fill(0.0);
            return Ok(0);
        }

        let written = match self.source.fill(out) {
            Ok(written) => written,
            Err(err) => {
                self.playing = false;
                self.clock.set_playing(false);
                return Err(err);
            }
        };

        if self.gain != 1.0 {
            for sample in &mut out[..written] {
                *sample *= self.gain;
            }
        }
        if written < out.len() {
            self.finished = written == 0 || written < out.len();
        }
        self.frames_delivered += (written / usize::from(self.source.channels().max(1))) as u64;
        self.clock.publish(self.position(), now);

        if self.finished {
            self.playing = false;
            self.clock.set_playing(false);
        }
        Ok(written)
    }

    /// Sample rate of the source.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }

    /// Channel count of the source.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.source.channels()
    }
}

/// An engine shared between the UI thread and a device thread.
///
/// A real backend owns the engine on its own thread and the UI reaches it through
/// a command queue; this mutex is the smallest thing with the same shape, so the
/// calls a device thread makes (`pull`) and the calls a UI makes (`play`,
/// `pause`, `seek`) already go through the seam they will use later.
pub type SharedEngine = Arc<Mutex<PlaybackEngine>>;

/// A device stand-in that runs **in real time** on its own thread.
///
/// [`NullDevice`] pulls as fast as it can, which is exactly right for tests and
/// useless for a UI: a progress bar driven by it would jump to the end instantly.
/// `PacedDevice` sleeps between callbacks, so the position advances at the rate a
/// sound card would produce it. Android uses this until an AAudio/Oboe backend
/// exists, which means: **the timeline is real, the sound is not.**
pub struct PacedDevice {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for PacedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PacedDevice")
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl PacedDevice {
    /// Starts a thread that pulls `frames_per_tick` frames every `period`.
    ///
    /// The thread returns on its own once the engine stops playing or reaches the
    /// end of the stream, so a paused player does not keep a thread warm.
    #[must_use]
    pub fn spawn(engine: SharedEngine, frames_per_tick: usize, period: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("aurora-paced-device".to_owned())
            .spawn(move || {
                let channels = {
                    let engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                    usize::from(engine.channels().max(1))
                };
                let mut buffer = vec![0.0_f32; frames_per_tick.max(1) * channels];
                let mut deadline = Instant::now();
                while !thread_stop.load(Ordering::Relaxed) {
                    let keep_going = {
                        let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                        match engine.pull(&mut buffer, Instant::now()) {
                            Ok(_) => engine.is_playing() && !engine.is_finished(),
                            // A broken decoder must not spin the thread: the
                            // engine has already stopped itself.
                            Err(_) => false,
                        }
                    };
                    if !keep_going {
                        break;
                    }
                    deadline += period;
                    let now = Instant::now();
                    if deadline > now {
                        std::thread::sleep(deadline - now);
                    } else {
                        // We fell behind (a suspended process, a busy phone). Do
                        // not accumulate a backlog of catch-up callbacks.
                        deadline = now;
                    }
                }
            })
            .expect("spawn the paced device thread");
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// True while the device thread is alive.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    /// Asks the thread to stop and waits for it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PacedDevice {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A device-free stand-in for an audio backend.
///
/// It models what cpal does: call [`PlaybackEngine::pull`] with a fixed frame
/// budget, `tick` after `tick`, until the source ends. Tests and the
/// `aurora --play` command drive playback with it, so the engine's arithmetic is
/// exercised on a machine with no sound card.
pub struct NullDevice {
    engine: PlaybackEngine,
    frames_per_tick: usize,
    buffer: Vec<f32>,
}

impl NullDevice {
    /// Creates a device with `frames_per_tick` frames per callback.
    #[must_use]
    pub fn new(engine: PlaybackEngine, frames_per_tick: usize) -> Self {
        let channels = usize::from(engine.channels().max(1));
        Self {
            buffer: vec![0.0; frames_per_tick * channels],
            engine,
            frames_per_tick,
        }
    }

    /// Runs one device callback's worth of frames.
    ///
    /// # Errors
    ///
    /// Propagates engine errors.
    pub fn tick(&mut self, now: Instant) -> Result<usize, DecodeError> {
        let written = self.engine.pull(&mut self.buffer, now)?;
        Ok(written)
    }

    /// The engine, for position and transport queries.
    #[must_use]
    pub fn engine(&self) -> &PlaybackEngine {
        &self.engine
    }

    /// Mutable engine, for `play`/`pause`/`seek`.
    pub fn engine_mut(&mut self) -> &mut PlaybackEngine {
        &mut self.engine
    }

    /// Frames per callback.
    #[must_use]
    pub fn frames_per_tick(&self) -> usize {
        self.frames_per_tick
    }

    /// Last buffer the device received (for assertions about what was played).
    #[must_use]
    pub fn last_buffer(&self) -> &[f32] {
        &self.buffer
    }

    /// Runs until end of stream, returning how many callbacks were needed.
    ///
    /// `now` is supplied by the caller so tests can use a synthetic clock.
    ///
    /// # Errors
    ///
    /// Propagates engine errors.
    pub fn run_to_end(&mut self, mut now: impl FnMut() -> Instant) -> Result<usize, DecodeError> {
        let mut ticks = 0;
        while self.engine.is_playing() && !self.engine.is_finished() {
            self.tick(now())?;
            ticks += 1;
            if ticks > 10_000_000 {
                break;
            }
        }
        Ok(ticks)
    }
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

    fn engine_for(name: &str) -> (PlaybackEngine, Arc<PlaybackClock>) {
        let decoder = crate::open(&fixture(name)).expect("open fixture");
        let clock = Arc::new(PlaybackClock::new());
        (
            PlaybackEngine::new(Box::new(DecoderSource::new(decoder, 1.0)), clock.clone()),
            clock,
        )
    }

    #[test]
    fn pulling_advances_the_position_by_frames_not_time() {
        let (mut engine, clock) = engine_for("00 - Fixture Steps.flac");
        engine.play();
        let start = Instant::now();
        let mut buffer = vec![0.0_f32; 480];

        // 4 callbacks of 480 frames = 1920 frames = 40 ms at 48 kHz.
        for tick in 0..4 {
            let written = engine
                .pull(&mut buffer, start + Duration::from_millis(tick * 10))
                .expect("pull");
            assert_eq!(written, 480);
        }
        assert_eq!(engine.position(), Duration::from_millis(40));

        // The clock agrees, and extrapolates from its own instant.
        let snapshot = clock.snapshot();
        assert!(snapshot.playing);
        assert_eq!(snapshot.position, Duration::from_millis(40));
        // The pair was published at start+30 ms holding 40 ms of audio, so five
        // milliseconds later the derived position is 45 ms.
        assert_eq!(
            snapshot.position_at(start + Duration::from_millis(35)),
            Duration::from_millis(45)
        );
    }

    #[test]
    fn a_paced_device_advances_in_real_time() {
        let decoder = crate::open(&fixture("00 - Fixture Tone.wav")).expect("open");
        let clock = Arc::new(PlaybackClock::new());
        let engine: SharedEngine = Arc::new(Mutex::new(PlaybackEngine::new(
            Box::new(DecoderSource::new(decoder, 1.0)),
            clock,
        )));
        engine.lock().unwrap_or_else(|e| e.into_inner()).play();

        let started = Instant::now();
        let mut device = PacedDevice::spawn(engine.clone(), 48, Duration::from_millis(5));
        // The fixture is 50 ms long; wait for the thread to run itself out.
        while device.is_running() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let elapsed = started.elapsed();
        let position = engine.lock().unwrap_or_else(|e| e.into_inner()).position();

        assert_eq!(position, Duration::from_millis(50));
        // Ten callbacks of 5 ms: the lower bound is the assertion that matters -
        // a device that ignores wall time finishes in microseconds.
        assert!(
            elapsed >= Duration::from_millis(45),
            "the paced device returned in {elapsed:?}, which is not real time"
        );
        assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
        device.stop();
    }

    #[test]
    fn stopping_a_paced_device_ends_its_thread() {
        let (mut engine, _clock) = engine_for("00 - Fixture Steps.flac");
        engine.play();
        let engine: SharedEngine = Arc::new(Mutex::new(engine));
        let mut device = PacedDevice::spawn(engine, 480, Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(10));
        device.stop();
        assert!(!device.is_running());
        // Dropping must be safe too, even after an explicit stop.
        drop(device);
    }

    #[test]
    fn volume_scales_the_samples() {
        let decoder = crate::open(&fixture("00 - Fixture Steps.flac")).expect("open");
        let clock = Arc::new(PlaybackClock::new());
        let mut engine = PlaybackEngine::new(Box::new(DecoderSource::new(decoder, 0.5)), clock);
        engine.play();
        let mut buffer = vec![0.0_f32; 64];
        engine.pull(&mut buffer, Instant::now()).expect("pull");
        assert!((buffer[0] - 0.125).abs() < 1e-6, "{}", buffer[0]);
    }

    #[test]
    fn the_engine_gain_changes_mid_stream() {
        let (mut engine, _clock) = engine_for("00 - Fixture Steps.flac");
        engine.set_gain(0.5);
        engine.play();
        let mut buffer = vec![0.0_f32; 64];
        engine.pull(&mut buffer, Instant::now()).expect("pull");
        assert!((buffer[0] - 0.125).abs() < 1e-6, "{}", buffer[0]);
        engine.set_gain(2.0);
        engine.pull(&mut buffer, Instant::now()).expect("pull");
        assert!((buffer[0] - 0.5).abs() < 1e-6, "{}", buffer[0]);
        assert_eq!(engine.gain(), 2.0);
        engine.set_gain(99.0);
        assert_eq!(engine.gain(), 4.0);
    }

    #[test]
    fn pausing_outputs_silence_and_freezes_the_position() {
        let (mut engine, clock) = engine_for("00 - Fixture Steps.flac");
        engine.play();
        let mut buffer = vec![0.0_f32; 480];
        engine.pull(&mut buffer, Instant::now()).expect("pull");
        let position = engine.position();
        engine.pause();

        let written = engine
            .pull(&mut buffer, Instant::now())
            .expect("paused pull");
        assert_eq!(written, 0);
        assert!(buffer.iter().all(|sample| *sample == 0.0));
        assert_eq!(engine.position(), position);
        assert!(!clock.snapshot().playing);
    }

    #[test]
    fn end_of_stream_stops_playback_exactly_at_the_end() {
        let (mut engine, _clock) = engine_for("00 - Fixture Steps.flac");
        engine.play();
        let mut device = NullDevice::new(engine, 1024);
        let start = Instant::now();
        let ticks = device.run_to_end(|| start).expect("run to end");
        // 8192 frames in 1024-frame callbacks = 8 full callbacks + one short.
        assert_eq!(ticks, 9);
        assert!(device.engine().is_finished());
        assert!(!device.engine().is_playing());
        assert_eq!(device.engine().position(), Duration::from_micros(170_666));
    }

    #[test]
    fn seeking_repositions_the_clock_and_the_source() {
        let (mut engine, clock) = engine_for("00 - Fixture Steps.flac");
        engine.play();
        let mut buffer = vec![0.0_f32; 64];
        engine.pull(&mut buffer, Instant::now()).expect("pull");
        assert!((buffer[0] - 0.25).abs() < 1e-6);

        let landed = engine.seek(Duration::from_micros(85_333)).expect("seek");
        assert!(landed <= Duration::from_micros(85_333));
        assert_eq!(clock.snapshot().position, landed);

        // Block 1 is silence; the first sample after the seek must not be the
        // pre-seek constant.
        engine
            .pull(&mut buffer, Instant::now())
            .expect("pull after seek");
        assert!(
            buffer[1..].iter().all(|sample| sample.abs() < 1e-6),
            "expected silence after the block boundary"
        );
    }

    #[test]
    fn a_broken_source_stops_playback_instead_of_looping_forever() {
        struct Broken;
        impl AudioSource for Broken {
            fn sample_rate(&self) -> u32 {
                48_000
            }
            fn channels(&self) -> u16 {
                2
            }
            fn fill(&mut self, _out: &mut [f32]) -> Result<usize, DecodeError> {
                Err(DecodeError::Invalid("simulated failure"))
            }
            fn seek(&mut self, _position: Duration) -> Result<Duration, DecodeError> {
                Ok(Duration::ZERO)
            }
        }

        let clock = Arc::new(PlaybackClock::new());
        let mut engine = PlaybackEngine::new(Box::new(Broken), clock.clone());
        engine.play();
        let mut buffer = vec![0.0_f32; 16];
        assert!(engine.pull(&mut buffer, Instant::now()).is_err());
        assert!(!engine.is_playing());
        assert!(!clock.snapshot().playing);
    }
}
