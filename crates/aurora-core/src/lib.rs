//! Aurora Player's domain core.
//!
//! Everything in this crate is platform-agnostic on purpose: it must compile
//! unchanged for Linux (Wayland) *and* for Android through UniFFI, so there is
//! no `iced`, no `egui`, no `cpal`, no `rusqlite` and no filesystem access in
//! here. Anything that touches the outside world is expressed as a trait and
//! implemented by a platform crate (see [`traits`]).
//!
//! Module map:
//!
//! * [`model`] - library entities (`Track`, `Album`, ...).
//! * [`lyrics`] - lyric document model plus word-timing primitives.
//! * [`clock`] - the atomic `(position, monotonic instant)` playback clock.
//! * [`motion`] - the animation primitives the UI needs every frame.
//! * [`traits`] - `AudioOutput`, `StorageBackend`, `AppDirectories`.
//! * [`transport`] - playback state machine.

pub mod clock;
pub mod lyrics;
pub mod model;
pub mod motion;
pub mod traits;
pub mod transport;

pub use clock::{ClockSnapshot, PlaybackClock};
pub use model::{Album, Artist, Track, TrackId};
pub use motion::{Spring, ease_out_cubic, word_envelope};
pub use transport::{PlaybackState, Transport};
