//! `aurora-mobile-probe` - the Android app's command sequence, on a laptop.
//!
//! The Kotlin activity does four things: create the player through
//! [`aurora_mobile::create`], `scan` a folder, `open` + `play` a track, then poll
//! `state` every 200 ms while the listener watches a progress bar. This binary
//! does exactly that, through exactly the same functions, and prints what the UI
//! would render. It is the closest thing to running the app that a machine with no
//! JVM, no NDK and no Android target can offer - and it is the evidence that the
//! app core works, as opposed to the claim that it does.
//!
//! ```text
//! cargo run -p aurora-mobile --bin aurora-mobile-probe -- fixtures/music
//! ```

use std::time::{Duration, Instant};

use aurora_mobile::{call, create, destroy};
use serde_json::Value;

fn json(response: &str) -> Value {
    serde_json::from_str(response)
        .unwrap_or_else(|error| panic!("native call returned invalid JSON ({error}): {response}"))
}

fn main() -> std::process::ExitCode {
    let folder = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/music".to_owned());

    // Kotlin: `NativePlayer.nativeCreate("""{"configRoot": …}""")`
    let base = std::env::temp_dir().join(format!("aurora-mobile-probe-{}", std::process::id()));
    let roots = serde_json::json!({
        "configRoot": base.join("config").display().to_string(),
        "dataRoot": base.join("data").display().to_string(),
        "cacheRoot": base.join("cache").display().to_string(),
    })
    .to_string();
    let handle = create(&roots);
    if handle == 0 {
        eprintln!("could not create the native player");
        return std::process::ExitCode::FAILURE;
    }
    println!("native handle: {handle}");

    let scanned = json(&call(
        handle,
        "scan",
        &format!(r#"{{"folder":"{folder}"}}"#),
    ));
    println!(
        "scan: visited {} added {} skipped {}",
        scanned["visited"], scanned["added"], scanned["skipped"]
    );
    for track in scanned["library"].as_array().cloned().unwrap_or_default() {
        println!("  library: {} ({})", track["title"], track["codec"]);
    }

    let first = scanned["library"]
        .as_array()
        .and_then(|tracks| tracks.first())
        .and_then(|track| track["path"].as_str())
        .unwrap_or_default()
        .to_owned();
    if first.is_empty() {
        eprintln!("nothing to play in {folder}");
        destroy(handle);
        return std::process::ExitCode::FAILURE;
    }

    let opened = json(&call(
        handle,
        "open",
        &serde_json::json!({ "path": first }).to_string(),
    ));
    println!(
        "open: {} - {} ms, {} Hz, {} channel(s)",
        opened["title"], opened["durationMs"], opened["sampleRateHz"], opened["channels"]
    );
    let note = json(&call(handle, "outputNote", "{}"));
    println!("output: {}", note["output"].as_str().unwrap_or("?"));

    let started = Instant::now();
    let _ = call(handle, "play", "{}");
    // The UI polls; the clock does the interpolating.
    let mut last_state = String::new();
    let mut samples = 0_u32;
    while started.elapsed() < Duration::from_secs(2) {
        let state = json(&call(handle, "state", "{}"));
        let name = state["state"].as_str().unwrap_or("?").to_owned();
        if name != last_state {
            println!(
                "state -> {name} at {} ms of {} ms",
                state["positionMs"].as_u64().unwrap_or(0),
                state["durationMs"].as_u64().unwrap_or(0)
            );
            last_state = name.clone();
        }
        if samples % 5 == 0 {
            println!(
                "  poll: {:>4} ms  [{}]",
                state["positionMs"],
                progress_bar(
                    state["positionMs"].as_u64().unwrap_or(0),
                    state["durationMs"].as_u64()
                )
            );
        }
        samples += 1;
        if name == "finished" {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // Restart, run for a moment, pause: the clock must freeze, not drift.
    let _ = call(handle, "play", "{}");
    std::thread::sleep(Duration::from_millis(60));
    let _ = call(handle, "pause", "{}");
    let paused = json(&call(handle, "state", "{}"));
    std::thread::sleep(Duration::from_millis(250));
    let still = json(&call(handle, "state", "{}"));
    println!(
        "paused at {} ms, still {} ms after 250 ms of waiting (state {})",
        paused["positionMs"].as_u64().unwrap_or(0),
        still["positionMs"].as_u64().unwrap_or(0),
        still["state"].as_str().unwrap_or("?")
    );

    let seek = json(&call(handle, "seek", r#"{"positionMs":20}"#));
    println!("seek to 20 ms landed at {} ms", seek["landedMs"]);
    println!(
        "live players after destroy: {}",
        aurora_mobile::live_players()
    );
    destroy(handle);
    println!("live players now: {}", aurora_mobile::live_players());
    let _ = std::fs::remove_dir_all(&base);
    std::process::ExitCode::SUCCESS
}

/// A 24-cell bar, so the output shows progress the way the UI would.
fn progress_bar(position_ms: u64, duration_ms: Option<u64>) -> String {
    let duration = duration_ms.unwrap_or(1).max(1);
    let filled = ((position_ms.min(duration) * 24) / duration) as usize;
    format!("{}{}", "#".repeat(filled), ".".repeat(24 - filled))
}
