//! The JNI shim: three functions, no logic.
//!
//! Every decision lives in [`crate::registry`] and [`crate::facade`], which are
//! plain Rust and tested on the host. This module only moves strings across the
//! FFI boundary in both directions, so if the app misbehaves the bug is almost
//! certainly not here - and if it *is* here, it is one of four lines.
//!
//! The Kotlin side declares:
//!
//! ```kotlin
//! package app.aurora.player
//! object NativePlayer {
//!     external fun nativeCreate(rootsJson: String): Long
//!     external fun nativeCall(handle: Long, method: String, argsJson: String): String
//!     external fun nativeDestroy(handle: Long): Boolean
//! }
//! ```
//!
//! Symbol names come from the JNI spec's mangling of `app.aurora.player.NativePlayer`
//! (the package separator becomes `_`; no character here needs an `_1`/`_2` escape,
//! which is why the package name is worth keeping boring).
//!
//! **Verification status:** this module is typechecked on Linux by
//! `cargo check -p aurora-mobile --features jni-bridge`, and the functions it
//! calls are covered by tests. It has never been *executed*: no JVM, no NDK and no
//! Android target exist in this sandbox. Anything it does wrong will only show up
//! on a machine that can build the APK, which is exactly what
//! `scripts/android/README.md` asks the reader to run first.

use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jlong, jstring};

use crate::registry;

/// Creates a player and returns its handle (`0` when the roots are unusable).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aurora_player_NativePlayer_nativeCreate<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    roots_json: JString<'caller>,
) -> jlong {
    let outcome = unowned.with_env(|_env| -> Result<jlong, jni::errors::Error> {
        let roots: String = roots_json.to_string();
        Ok(registry::create(&roots))
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Runs one command. Always returns JSON, including for errors.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aurora_player_NativePlayer_nativeCall<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    handle: jlong,
    method: JString<'caller>,
    args_json: JString<'caller>,
) -> jstring {
    let outcome = unowned.with_env(|env| -> Result<jstring, jni::errors::Error> {
        let method: String = method.to_string();
        let args: String = args_json.to_string();
        let response = registry::call(handle, &method, &args);
        Ok(JString::from_str(env, response)?.into_raw())
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Drops the player behind `handle`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aurora_player_NativePlayer_nativeDestroy<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    handle: jlong,
) -> jboolean {
    let outcome = unowned.with_env(|_env| -> Result<jboolean, jni::errors::Error> {
        // `jboolean` is Rust's `bool` in jni-sys 0.4.
        Ok(registry::destroy(handle))
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Number of live players, for the app's leak check on background/foreground.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aurora_player_NativePlayer_nativeLivePlayers<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> jlong {
    let outcome = unowned.with_env(|_env| -> Result<jlong, jni::errors::Error> {
        Ok(registry::live_players() as jlong)
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// `JNI_OnLoad` is not needed: nothing here registers methods dynamically and
/// `jni` initialises itself lazily on first use. The function is deliberately
/// absent rather than empty, because an empty one would suggest otherwise.
#[cfg(test)]
mod tests {
    /// The mangled names the Kotlin `external fun`s resolve to, written out so a
    /// rename of either side fails here before it fails on a phone.
    #[test]
    fn the_kotlin_class_and_the_jni_symbols_agree() {
        let package = "app.aurora.player";
        let class = "NativePlayer";
        let prefix = format!(
            "Java_{}_{}",
            package.replace('.', "_"),
            class.replace('_', "_1")
        );
        assert_eq!(prefix, "Java_app_aurora_player_NativePlayer");
        for method in [
            "nativeCreate",
            "nativeCall",
            "nativeDestroy",
            "nativeLivePlayers",
        ] {
            assert!(!method.contains('_'), "{method} would need a JNI escape");
        }
    }
}
