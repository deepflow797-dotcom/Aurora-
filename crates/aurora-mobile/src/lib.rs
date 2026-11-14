//! Aurora Player for Android: the app core, minus the Java.
//!
//! This crate is the smallest thing that can be called "the Android app" without
//! a phone in the room:
//!
//! ```text
//!   Compose UI (Kotlin)  ──JNI──►  registry  ──►  Player facade  ──►  aurora-audio
//!        │                          │                 │
//!        └── 200 ms poll ───────────┴─ JSON state ─────┘
//! ```
//!
//! * [`facade`] owns playback, the library scan and the JSON command surface. Plain
//!   Rust, fully tested on the host.
//! * [`registry`] maps an `i64` handle (what Kotlin can hold) to a live player, and
//!   is tested too.
//! * [`jni_bridge`] (feature `jni-bridge`) is four exported functions that move
//!   strings. It is typechecked here, executed nowhere: this sandbox has no JVM,
//!   no NDK and no Android std.
//!
//! The honest summary of where Android stands: **the app logic works and is
//! tested; the APK is not built, because this machine cannot build one.** The
//! audio is paced in real time but silent - there is no AAudio/Oboe backend yet,
//! so [`facade::Player::output_note`] says so and the UI shows that string.

pub mod facade;
pub mod registry;

#[cfg(feature = "jni-bridge")]
pub mod jni_bridge;

pub use facade::{MobileError, Player, State, TrackInfo};
pub use registry::{call, create, destroy, live_players};

/// Version string the app displays in its about row.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_reports_a_version() {
        assert!(!version().is_empty());
        assert!(version().starts_with("0."));
    }
}
