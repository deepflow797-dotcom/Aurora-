//! Handle registry: the only state the JNI layer has.
//!
//! Android has no Rust objects. Kotlin holds a `Long` and passes it back on every
//! call, so the native side needs a process-wide table of live [`Player`]s. This
//! module is deliberately **not** feature-gated behind `jni-bridge`: it is plain
//! Rust, so the "create → call → destroy" lifecycle the app performs can be
//! tested on any machine, JVM or not.
//!
//! Two rules that matter on a phone:
//!
//! * handles are never reused, so a call that arrives after `destroy` (an
//!   in-flight UI callback) fails loudly instead of touching another player;
//! * everything is behind one mutex, so a JNI call from the UI thread and one
//!   from a worker thread cannot interleave inside a `Player`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use crate::facade::{MobileError, Player};

struct Registry {
    players: Mutex<HashMap<i64, Player>>,
    next: AtomicI64,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        players: Mutex::new(HashMap::new()),
        next: AtomicI64::new(1),
    })
}

/// Creates a player from `{"configRoot": "...", "dataRoot": "...", "cacheRoot": "..."}`.
///
/// Returns the handle, or `0` when the roots are unusable - a Kotlin-side `0` is
/// an unambiguous "nothing was created" without an exception crossing JNI.
#[must_use]
pub fn create(roots_json: &str) -> i64 {
    let roots: Value = serde_json::from_str(roots_json).unwrap_or_else(|_| json!({}));
    let root = |key: &str| -> Option<std::path::PathBuf> {
        roots
            .get(key)
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from)
    };
    let (Some(config), Some(data), Some(cache)) =
        (root("configRoot"), root("dataRoot"), root("cacheRoot"))
    else {
        return 0;
    };
    match Player::new(&config, &data, &cache) {
        Ok(player) => {
            let handle = registry().next.fetch_add(1, Ordering::Relaxed);
            let mut players = registry().players.lock().unwrap_or_else(|e| e.into_inner());
            players.insert(handle, player);
            handle
        }
        Err(error) => {
            tracing::warn!(error = %error, "could not create the mobile player");
            0
        }
    }
}

/// Runs one command against `handle`, returning JSON (errors included).
///
/// An unknown handle returns an error object rather than panicking: a Kotlin
/// callback that arrives after `destroy` must not take the process down.
#[must_use]
pub fn call(handle: i64, method: &str, args: &str) -> String {
    let mut players = registry().players.lock().unwrap_or_else(|e| e.into_inner());
    match players.get_mut(&handle) {
        Some(player) => player.dispatch(method, args),
        None => json!({
            "error": "no_such_player",
            "message": format!("handle {handle} is not registered (destroyed?)"),
        })
        .to_string(),
    }
}

/// Drops the player behind `handle`. Returns whether anything was there.
pub fn destroy(handle: i64) -> bool {
    let mut players = registry().players.lock().unwrap_or_else(|e| e.into_inner());
    players.remove(&handle).is_some()
}

/// How many players are alive (tests and leak checks).
#[must_use]
pub fn live_players() -> usize {
    registry()
        .players
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .len()
}

/// Convenience for Kotlin: the error code a failed call carries, if any.
#[must_use]
pub fn error_code(response: &str) -> Option<String> {
    serde_json::from_str::<Value>(response)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_owned))
}

impl From<MobileError> for Value {
    fn from(error: MobileError) -> Self {
        json!({ "error": error.code(), "message": error.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::temp_roots;

    fn roots_json(tag: &str) -> String {
        let (config, data, cache) = temp_roots(tag);
        json!({
            "configRoot": config.display().to_string(),
            "dataRoot": data.display().to_string(),
            "cacheRoot": cache.display().to_string(),
        })
        .to_string()
    }

    #[test]
    fn create_call_destroy_is_the_whole_lifecycle() {
        let before = live_players();
        let handle = create(&roots_json("lifecycle"));
        assert!(handle > 0);
        assert_eq!(live_players(), before + 1);

        let state: Value = serde_json::from_str(&call(handle, "state", "{}")).expect("json");
        assert_eq!(state["state"], "idle");

        assert!(destroy(handle));
        assert!(
            !destroy(handle),
            "destroying twice is not an error, just false"
        );
        assert_eq!(live_players(), before);
    }

    #[test]
    fn a_stale_handle_reports_instead_of_panicking() {
        let handle = create(&roots_json("stale"));
        assert!(destroy(handle));
        let response = call(handle, "state", "{}");
        assert_eq!(error_code(&response).as_deref(), Some("no_such_player"));
    }

    #[test]
    fn handles_are_never_reused() {
        let first = create(&roots_json("reuse-1"));
        destroy(first);
        let second = create(&roots_json("reuse-2"));
        assert_ne!(first, second);
        assert!(second > first);
        destroy(second);
    }

    #[test]
    fn bad_roots_produce_the_null_handle() {
        assert_eq!(create("{not json"), 0);
        assert_eq!(create(r#"{"configRoot":"/tmp/x"}"#), 0);
    }

    #[test]
    fn a_command_with_bad_arguments_comes_back_as_json() {
        let handle = create(&roots_json("bad-args"));
        let response = call(handle, "seek", r#"{"positionMs":"soon"}"#);
        assert_eq!(error_code(&response).as_deref(), Some("bad_request"));
        destroy(handle);
    }
}
