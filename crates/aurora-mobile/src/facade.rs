//! The app core Android talks to: one `Player` behind a JSON dispatch surface.
//!
//! Why a JSON façade instead of a generated binding:
//!
//! * the spec's choice for Android is **UniFFI**, and UniFFI cannot be vendored
//!   in this sandbox (it needs `uniffi` + `uniffi_bindgen` + `askama` + a
//!   proc-macro chain, none of it reachable offline). Rather than block the
//!   Android work on that, the boundary is a `(method, args) -> json` call - the
//!   same shape UniFFI would produce, minus the code generator;
//! * it is **testable without a JVM**: [`Player::dispatch`] is plain Rust, so
//!   every command the Kotlin app can send is covered by a host test. The JNI
//!   shim on top is then a twenty-line adapter instead of a hundred lines of
//!   marshalling;
//! * the payloads are owned, `Copy`-free, plain data (`serde_json::Value`), which
//!   is exactly what a future UniFFI switch needs, so the migration is mechanical.
//!
//! What the façade deliberately does *not* hide: there is no audio output
//! backend on Android yet. [`Player::output_note`] says so, and the app shows it
//! on screen. The timeline is real (a [`PacedDevice`] paces it); the sound is not.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aurora_audio::{DecoderSource, PacedDevice, PlaybackEngine, SharedEngine};
use aurora_core::clock::PlaybackClock;
use aurora_db::{Catalog, MemoryCatalog};
use aurora_library::{FileNameProbe, ScanReport, scan};

use crate::lyrics::LyricsSession;
use aurora_platform::dirs::XdgDirectories;
use serde_json::{Value, json};

/// Frames per device callback the app pretends to have (≈10 ms at 48 kHz).
const FRAMES_PER_TICK: usize = 480;
/// Wall-clock period between simulated callbacks.
const TICK_PERIOD: Duration = Duration::from_millis(10);

#[derive(Debug, thiserror::Error)]
pub enum MobileError {
    #[error("no track is loaded")]
    NoTrack,
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
    #[error("{0}")]
    Decode(#[from] aurora_audio::DecodeError),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("library: {0}")]
    Library(String),
    #[error("directories: {0}")]
    Directories(String),
}

impl MobileError {
    /// Machine-readable code, so Kotlin can branch without string matching.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoTrack => "no_track",
            Self::UnknownMethod(_) => "unknown_method",
            Self::Decode(_) => "decode",
            Self::BadRequest(_) => "bad_request",
            Self::Library(_) => "library",
            Self::Directories(_) => "directories",
        }
    }
}

/// What the UI shows about the loaded file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub is_float: bool,
    pub duration_ms: u64,
}

impl TrackInfo {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "title": self.title,
            "artist": self.artist,
            "sampleRateHz": self.sample_rate_hz,
            "channels": self.channels,
            "bitsPerSample": self.bits_per_sample,
            "isFloat": self.is_float,
            "durationMs": self.duration_ms,
        })
    }
}

/// The transport states the UI may render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Playing,
    Paused,
    Finished,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Finished => "finished",
        }
    }
}

/// Everything the Android activity needs, and nothing it does not.
pub struct Player {
    dirs: XdgDirectories,
    catalog: MemoryCatalog,
    clock: Arc<PlaybackClock>,
    engine: Option<SharedEngine>,
    device: Option<PacedDevice>,
    track: Option<TrackInfo>,
    /// Lyrics for the loaded track, if a sidecar or the lyrics folder had any.
    lyrics: Option<LyricsSession>,
    /// Paths lyric discovery looked at, so "no lyrics" can say where it looked.
    lyrics_tried: Vec<String>,
    volume: f32,
    /// Set once the engine reported the end of the stream.
    finished: bool,
    last_error: Option<String>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("state", &self.state())
            .field("track", &self.track.as_ref().map(|t| &t.path))
            .field("volume", &self.volume)
            .finish_non_exhaustive()
    }
}

impl Player {
    /// Builds a player whose files live under the three roots Android hands in
    /// (`Context.filesDir` / `cacheDir` subdirectories).
    ///
    /// # Errors
    ///
    /// Returns [`MobileError::Directories`] if the roots cannot be used.
    pub fn new(
        config_root: &Path,
        data_root: &Path,
        cache_root: &Path,
    ) -> Result<Self, MobileError> {
        let dirs = XdgDirectories::new(
            config_root.to_path_buf(),
            data_root.to_path_buf(),
            cache_root.to_path_buf(),
        );
        dirs.ensure()
            .map_err(|e| MobileError::Directories(e.to_string()))?;
        Ok(Self {
            dirs,
            catalog: MemoryCatalog::new(),
            clock: PlaybackClock::shared(),
            engine: None,
            device: None,
            track: None,
            lyrics: None,
            lyrics_tried: Vec::new(),
            volume: 1.0,
            finished: false,
            last_error: None,
        })
    }

    /// Directories the app writes into (settings, catalog cache, artwork).
    #[must_use]
    pub fn directories(&self) -> &XdgDirectories {
        &self.dirs
    }

    /// The audio clock the UI reads positions from.
    #[must_use]
    pub fn clock(&self) -> &Arc<PlaybackClock> {
        &self.clock
    }

    /// Loads `path`, replacing whatever was loaded before.
    ///
    /// # Errors
    ///
    /// Propagates decoder errors (unsupported format, missing file, …).
    pub fn open(&mut self, path: impl AsRef<Path>) -> Result<TrackInfo, MobileError> {
        let path = path.as_ref();
        self.stop_device();
        self.finished = false;
        self.last_error = None;

        let decoder = aurora_audio::open(path)?;
        let format = decoder.format();
        let total = decoder.total_frames();
        let source = DecoderSource::new(decoder, 1.0);

        let clock = PlaybackClock::shared();
        self.clock = Arc::clone(&clock);
        let mut engine = PlaybackEngine::new(Box::new(source), Arc::clone(&clock));
        engine.set_gain(self.volume);
        let info = TrackInfo {
            path: path.display().to_string(),
            title: title_from_path(path),
            artist: "Unknown Artist".to_owned(),
            sample_rate_hz: format.sample_rate,
            channels: format.channels,
            bits_per_sample: format.bits_per_sample,
            is_float: format.is_float,
            duration_ms: total
                .map(|frames| {
                    Duration::from_micros(
                        frames.saturating_mul(1_000_000) / u64::from(format.sample_rate.max(1)),
                    )
                    .as_millis() as u64
                })
                .unwrap_or(0),
        };
        self.engine = Some(Arc::new(Mutex::new(engine)));
        self.track = Some(info.clone());

        // Lyrics are loaded with the track, not on the first frame: a 200 ms poll
        // must never be the thing that touches the disk.
        match crate::lyrics::load(path, &self.dirs.lyrics_dir()) {
            Ok(session) => {
                tracing::debug!(lines = session.line_count(), "lyrics loaded");
                self.lyrics = Some(session);
                self.lyrics_tried.clear();
            }
            Err(tried) => {
                self.lyrics = None;
                self.lyrics_tried = tried;
            }
        }
        Ok(info)
    }

    /// Starts (or resumes) playback.
    ///
    /// # Errors
    ///
    /// [`MobileError::NoTrack`] when nothing is loaded.
    pub fn play(&mut self) -> Result<(), MobileError> {
        let engine = self.engine.as_ref().ok_or(MobileError::NoTrack)?.clone();
        {
            let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
            if engine.is_finished() {
                // Pressing play at the end restarts from the top, like every
                // other player.
                engine.seek(Duration::ZERO).map_err(MobileError::Decode)?;
            }
            engine.play();
        }
        self.finished = false;
        self.stop_device();
        self.device = Some(PacedDevice::spawn(engine, FRAMES_PER_TICK, TICK_PERIOD));
        Ok(())
    }

    /// Pauses, keeping the position.
    pub fn pause(&mut self) {
        self.stop_device();
        if let Some(engine) = self.engine.as_ref() {
            engine.lock().unwrap_or_else(|e| e.into_inner()).pause();
        }
    }

    /// Play if paused, pause if playing.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::play`]'s errors.
    pub fn toggle(&mut self) -> Result<(), MobileError> {
        if self.state() == State::Playing {
            self.pause();
            Ok(())
        } else {
            self.play()
        }
    }

    /// Stops and rewinds to the beginning.
    pub fn stop(&mut self) {
        self.pause();
        if let Some(engine) = self.engine.as_ref() {
            let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
            let _ = engine.seek(Duration::ZERO);
        }
        self.finished = false;
        self.clock.set(Duration::ZERO, std::time::Instant::now());
    }

    /// Seeks to `position_ms`, returning where the decoder actually landed.
    ///
    /// # Errors
    ///
    /// [`MobileError::NoTrack`] or a decoder seek error.
    pub fn seek_ms(&mut self, position_ms: u64) -> Result<u64, MobileError> {
        let engine = self.engine.as_ref().ok_or(MobileError::NoTrack)?;
        let landed = {
            let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
            engine
                .seek(Duration::from_millis(position_ms))
                .map_err(MobileError::Decode)?
        };
        Ok(landed.as_millis() as u64)
    }

    /// Position the UI should draw: the clock's view, extrapolated from the last
    /// device callback so a 200 ms poll still looks smooth.
    #[must_use]
    pub fn position_ms(&self) -> u64 {
        let snapshot = self.clock.snapshot();
        let position = snapshot.position_at(std::time::Instant::now()).as_millis() as u64;
        // Never draw past the end: the clock extrapolates, the file does not.
        position.min(self.duration_ms().unwrap_or(position))
    }

    /// Length of the loaded track, when the container states it.
    #[must_use]
    pub fn duration_ms(&self) -> Option<u64> {
        self.track.as_ref().map(|track| track.duration_ms)
    }

    /// Transport state, derived from the engine rather than kept in a field.
    #[must_use]
    pub fn state(&self) -> State {
        let Some(engine) = self.engine.as_ref() else {
            return State::Idle;
        };
        let engine = engine.lock().unwrap_or_else(|e| e.into_inner());
        if engine.is_playing() {
            State::Playing
        } else if engine.is_finished() || self.finished {
            State::Finished
        } else {
            State::Paused
        }
    }

    /// Linear volume, `0.0 ..= 1.0`.
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Sets the live linear volume, `0.0 ..= 1.0`.
    ///
    /// The gain is applied by the engine on every callback, so this takes effect
    /// mid-stream without re-opening the file.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(engine) = self.engine.as_ref() {
            engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_gain(self.volume);
        }
    }

    /// Scans `folder` into the in-memory catalog.
    ///
    /// # Errors
    ///
    /// Propagates walk and catalog errors.
    pub fn scan(&mut self, folder: impl AsRef<Path>) -> Result<ScanReport, MobileError> {
        let root = aurora_core::model::TrackLocation::Path(folder.as_ref().to_path_buf());
        scan(
            &aurora_core::traits::PathStorage,
            &FileNameProbe,
            &self.catalog,
            &root,
        )
        .map_err(|e| MobileError::Library(e.to_string()))
    }

    /// Everything the last scan found, in catalog order.
    ///
    /// # Errors
    ///
    /// Propagates catalog read errors.
    pub fn library(&self) -> Result<Vec<Value>, MobileError> {
        let tracks = self
            .catalog
            .all_tracks()
            .map_err(|e| MobileError::Library(e.to_string()))?;
        Ok(tracks
            .into_iter()
            .map(|track| {
                json!({
                    "path": track.location.as_str(),
                    "title": track.title,
                    "artist": track.artist.name,
                    "durationMs": track.duration_ms,
                    "trackNumber": track.track_number,
                    "codec": track.codec,
                })
            })
            .collect())
    }

    /// The lyric column for the current position, as the shared view model plans it.
    ///
    /// Not `#[must_use]`-only: it advances the spring scroll by one frame, which is
    /// why the app calls it at its refresh rate rather than from a getter.
    pub fn lyrics_json(&mut self) -> Value {
        let snapshot = self.clock.snapshot();
        let now = std::time::Instant::now();
        match self.lyrics.as_mut() {
            Some(session) => session.to_json(snapshot, now),
            None => json!({
                "available": false,
                "lines": [],
                "activeLine": null,
                "scrollPx": 0.0,
                "tried": self.lyrics_tried,
            }),
        }
    }

    /// True when the loaded track has lyrics (real or estimated words).
    #[must_use]
    pub fn has_lyrics(&self) -> bool {
        self.lyrics.is_some()
    }

    /// One line the app shows so nobody believes there is sound yet.
    #[must_use]
    pub fn output_note(&self) -> &'static str {
        "simulated output: this build has no audio device backend"
    }

    fn stop_device(&mut self) {
        if let Some(mut device) = self.device.take() {
            device.stop();
        }
    }

    /// Snapshot the app renders, as JSON.
    #[must_use]
    pub fn state_json(&self) -> Value {
        json!({
            "state": self.state().as_str(),
            "positionMs": self.position_ms(),
            "durationMs": self.duration_ms(),
            "volume": self.volume,
            "output": self.output_note(),
            "hasLyrics": self.lyrics.is_some(),
            "track": self.track.as_ref().map(TrackInfo::to_json),
            "error": self.last_error,
        })
    }

    /// The one entry point the JNI shim uses.
    ///
    /// `method` is a camelCase command, `args` is a JSON object (may be empty).
    /// The return value is always a JSON object: on failure it carries
    /// `{"error": code, "message": "..."}` so Kotlin never has to interpret a
    /// null pointer.
    pub fn dispatch(&mut self, method: &str, args: &str) -> String {
        match self.handle(method, args) {
            Ok(value) => value.to_string(),
            Err(error) => {
                self.last_error = Some(error.to_string());
                json!({
                    "error": error.code(),
                    "message": error.to_string(),
                    "state": self.state_json(),
                })
                .to_string()
            }
        }
    }

    fn handle(&mut self, method: &str, args: &str) -> Result<Value, MobileError> {
        let args: Value = if args.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(args).map_err(|e| MobileError::BadRequest(e.to_string()))?
        };
        let text = |key: &str| -> Result<String, MobileError> {
            args.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| MobileError::BadRequest(format!("missing string '{key}'")))
        };

        match method {
            "open" => Ok(self.open(text("path")?)?.to_json()),
            "play" => {
                self.play()?;
                Ok(self.state_json())
            }
            "pause" => {
                self.pause();
                Ok(self.state_json())
            }
            "toggle" => {
                self.toggle()?;
                Ok(self.state_json())
            }
            "stop" => {
                self.stop();
                Ok(self.state_json())
            }
            "seek" => {
                let target = args
                    .get("positionMs")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| MobileError::BadRequest("missing 'positionMs'".to_owned()))?;
                let landed = self.seek_ms(target)?;
                Ok(json!({ "landedMs": landed, "state": self.state_json() }))
            }
            "setVolume" => {
                let volume = args
                    .get("volume")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| MobileError::BadRequest("missing 'volume'".to_owned()))?;
                self.set_volume(volume as f32);
                Ok(self.state_json())
            }
            "state" => Ok(self.state_json()),
            "scan" => {
                let report = self.scan(text("folder")?)?;
                Ok(json!({
                    "visited": report.visited,
                    "added": report.added,
                    "skipped": report.skipped,
                    "library": self.library()?,
                }))
            }
            "library" => Ok(json!({ "library": self.library()? })),
            "lyrics" => Ok(self.lyrics_json()),
            "outputNote" => Ok(json!({ "output": self.output_note() })),
            other => Err(MobileError::UnknownMethod(other.to_owned())),
        }
    }
}

/// `NN - Artist - Title` when the name says so, else the file stem.
pub(crate) fn title_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Untitled");
    let rest = match stem.split_once(" - ") {
        Some((head, tail)) if head.chars().all(|c| c.is_ascii_digit()) => tail,
        _ => stem,
    };
    match rest.split_once(" - ") {
        Some((_artist, title)) => title.trim().to_owned(),
        None => rest.trim().to_owned(),
    }
}

/// Paths the façade needs, so tests and the probe do not touch a real home dir.
#[must_use]
pub fn temp_roots(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("aurora-mobile-{tag}-{}", std::process::id()));
    (base.join("config"), base.join("data"), base.join("cache"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music")
            .join(name)
    }

    fn player(tag: &str) -> Player {
        let (config, data, cache) = temp_roots(tag);
        Player::new(&config, &data, &cache).expect("player")
    }

    #[test]
    fn opening_a_tone_reports_its_format_and_length() {
        let mut player = player("open");
        let info = player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        assert_eq!(info.sample_rate_hz, 48_000);
        assert_eq!(info.channels, 1);
        assert_eq!(info.bits_per_sample, 16);
        assert!(!info.is_float);
        assert_eq!(info.duration_ms, 50);
        assert_eq!(info.title, "Fixture Tone");
        assert_eq!(player.state(), State::Paused);
    }

    #[test]
    fn the_track_title_prefers_the_tag_style_file_name() {
        assert_eq!(
            title_from_path(Path::new("/music/07 - Chikha Rimitti - Charrak.flac")),
            "Charrak"
        );
        assert_eq!(title_from_path(Path::new("/music/bare.wav")), "bare");
    }

    #[test]
    fn playing_advances_the_clock_in_real_time_and_finishes() {
        let mut player = player("play");
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        player.play().expect("play");
        assert_eq!(player.state(), State::Playing);

        std::thread::sleep(Duration::from_millis(20));
        let middle = player.position_ms();
        assert!(middle > 0 && middle <= 50, "position {middle} ms");

        // The 50 ms fixture runs out; wait for the state to settle.
        for _ in 0..100 {
            if player.state() == State::Finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(player.state(), State::Finished);
        assert_eq!(player.position_ms(), 50);
    }

    #[test]
    fn pause_freezes_the_position_and_play_resumes_it() {
        let mut player = player("pause");
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        player.play().expect("play");
        std::thread::sleep(Duration::from_millis(20));
        player.pause();
        let paused = player.position_ms();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            player.position_ms(),
            paused,
            "a paused player does not move"
        );
        assert_eq!(player.state(), State::Paused);

        player.play().expect("resume");
        assert_eq!(player.state(), State::Playing);
        std::thread::sleep(Duration::from_millis(10));
        assert!(player.position_ms() >= paused);
    }

    #[test]
    fn seek_moves_to_the_requested_millisecond() {
        let mut player = player("seek");
        player
            .open(fixture("00 - Fixture Steps.flac"))
            .expect("open");
        assert_eq!(player.seek_ms(85).expect("seek"), 85);
        assert!(player.position_ms() >= 85);

        // Out of range is a *format* decision: the in-crate WAV decoder refuses,
        // while symphonia clamps to the last packet and reports success. The
        // façade forwards whatever the decoder says instead of inventing a rule.
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        assert!(matches!(
            player.seek_ms(10_000),
            Err(MobileError::Decode(
                aurora_audio::DecodeError::SeekOutOfRange { .. }
            ))
        ));
    }

    #[test]
    fn opening_a_track_with_a_sidecar_exposes_its_lyrics() {
        let mut player = player("lyrics");
        player.open(fixture("00 - Fixture Steps.flac")).expect("open");
        assert!(player.has_lyrics());

        // Put the clock inside the second line (starts at 3.0 s) and ask for the
        // column the way Compose does.
        player
            .clock()
            .publish(Duration::from_millis(3_200), std::time::Instant::now());
        let lyrics = player.lyrics_json();
        assert_eq!(lyrics["available"], true);
        assert_eq!(lyrics["activeLine"], 1);
        assert_eq!(lyrics["source"], "sidecar .lrc");
        assert_eq!(lyrics["wordTimed"], true);
        assert_eq!(lyrics["lines"].as_array().unwrap().len(), 4);
        assert!(
            lyrics["lines"][1]["text"]
                .as_str()
                .unwrap()
                .contains("Hello Aurora")
        );

        // Through the dispatch surface too: this is what Kotlin actually calls.
        let via_dispatch: Value =
            serde_json::from_str(&player.dispatch("lyrics", "{}")).expect("json");
        assert_eq!(via_dispatch["activeLine"], 1);
    }

    #[test]
    fn a_track_without_lyrics_says_where_it_looked() {
        let mut player = player("no-lyrics");
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        assert!(!player.has_lyrics());
        let lyrics = player.lyrics_json();
        assert_eq!(lyrics["available"], false);
        assert!(
            !lyrics["tried"].as_array().unwrap().is_empty(),
            "the app must be able to show which paths were checked"
        );
        let state: Value = serde_json::from_str(&player.dispatch("state", "{}")).expect("json");
        assert_eq!(state["hasLyrics"], false);
    }

    #[test]
    fn scanning_a_folder_fills_the_library() {
        let mut player = player("scan");
        let report = player
            .scan(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/music"))
            .expect("scan");
        assert_eq!(report.visited, 3, "wav, aiff and flac");
        assert_eq!(report.added, 3);
        let library = player.library().expect("library");
        assert_eq!(library.len(), 3);
        assert!(library.iter().any(|track| track["codec"] == "fLaC"));
    }

    #[test]
    fn dispatch_speaks_json_both_ways() {
        let mut player = player("dispatch");
        let path = fixture("00 - Fixture Tone.aiff").display().to_string();
        let opened = player.dispatch("open", &json!({ "path": path }).to_string());
        let opened: Value = serde_json::from_str(&opened).expect("json");
        assert_eq!(opened["durationMs"], 50);
        assert_eq!(opened["sampleRateHz"], 48_000);

        let state: Value = serde_json::from_str(&player.dispatch("state", "{}")).expect("json");
        assert_eq!(state["state"], "paused");
        assert!(state["output"].as_str().unwrap().contains("simulated"));

        let error: Value = serde_json::from_str(&player.dispatch("nonsense", "{}")).expect("json");
        assert_eq!(error["error"], "unknown_method");

        let error: Value = serde_json::from_str(&player.dispatch("open", "{}")).expect("json");
        assert_eq!(error["error"], "bad_request");
        assert!(error["message"].as_str().unwrap().contains("path"));
    }

    #[test]
    fn dispatching_without_a_track_says_so() {
        let mut player = player("empty");
        let error: Value = serde_json::from_str(&player.dispatch("play", "{}")).expect("json");
        assert_eq!(error["error"], "no_track");
        assert_eq!(player.state(), State::Idle);
    }

    #[test]
    fn volume_is_clamped_and_reported() {
        let mut player = player("volume");
        player.set_volume(2.0);
        assert_eq!(player.volume(), 1.0);
        player.set_volume(-1.0);
        assert_eq!(player.volume(), 0.0);
        let state: Value =
            serde_json::from_str(&player.dispatch("setVolume", r#"{"volume":0.5}"#)).expect("json");
        assert_eq!(state["volume"], 0.5);
    }

    #[test]
    fn pressing_play_after_the_end_restarts_from_the_top() {
        let mut player = player("restart");
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        player.play().expect("play");
        for _ in 0..100 {
            if player.state() == State::Finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(player.state(), State::Finished);

        player.play().expect("replay");
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(player.state(), State::Playing);
        assert!(player.position_ms() < 50, "{}", player.position_ms());
    }

    #[test]
    fn volume_reaches_the_engine_while_it_is_playing() {
        let mut player = player("live-volume");
        player.open(fixture("00 - Fixture Tone.wav")).expect("open");
        player.play().expect("play");
        player.set_volume(0.25);
        let engine = player.engine.as_ref().expect("engine");
        let gain = engine.lock().unwrap_or_else(|e| e.into_inner()).gain();
        assert!((gain - 0.25).abs() < 1e-6, "{gain}");
    }
}
