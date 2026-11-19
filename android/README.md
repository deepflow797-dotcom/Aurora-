# Aurora Player — Android

The Android app is the same Rust core as the desktop player, called over JNI from
a Jetpack Compose screen.

```text
  ┌──────────────────────────────┐
  │ Compose UI (Kotlin)          │  MainActivity.kt
  │  poll state every 200 ms     │  PlayerViewModel.kt
  └──────────────┬───────────────┘
                 │  NativePlayer.nativeCall(handle, "seek", "{\"positionMs\":…}")
  ┌──────────────▼───────────────┐
  │ JNI shim (4 functions)       │  crates/aurora-mobile/src/jni_bridge.rs
  ├──────────────────────────────┤
  │ handle registry              │  crates/aurora-mobile/src/registry.rs
  ├──────────────────────────────┤
  │ Player facade (JSON)         │  crates/aurora-mobile/src/facade.rs
  ├──────────────────────────────┤
  │ aurora-audio / -library /    │  the same crates the desktop binary uses:
  │ -core / -db / -platform      │  decoders, engine, clock, scanner, settings
  └──────────────────────────────┘
```

## Status, stated plainly

| Part | State |
| --- | --- |
| App core (`aurora-mobile`: facade + registry) | **Works and is tested on the host** — 17 tests, including real-time playback, pause, seek, scan, JSON dispatch |
| JNI shim | **Typechecked, never executed** — `cargo check -p aurora-mobile --features jni-bridge` passes against the vendored `jni` 0.22; no JVM exists here to run it |
| Kotlin / Compose UI | **Written, never compiled** — no JDK and no Gradle in this sandbox |
| Lyrics (Phase 2.8) | **Written, never compiled** — the Rust side lost its toolchain mid-phase; see the warning in `PHASE_LOG.md` |
| Rust cross-compilation | **Not possible here** — the installed toolchain has only `x86_64-unknown-linux-gnu` std, no `rustup`, and `static.rust-lang.org` is unreachable |
| APK | **Not built.** `scripts/android/build.sh --check` names the four missing tools |
| Audio output | **Silent.** The timeline is real (`PacedDevice`, 480 frames every 10 ms); there is no AAudio/Oboe backend, and the screen says so |

So: the part that could be built and verified was built and verified, and the part
that needs a phone-shaped toolchain is written but unproven. Nothing in this
directory has been run on a device.

## What to run first on a machine that has the tools

```bash
cargo test -p aurora-mobile                     # app core, no Android needed
cargo check -p aurora-mobile --features jni-bridge   # the JNI layer compiles
bash scripts/android/build.sh --check           # what is missing, if anything
bash scripts/android/build.sh                   # .so files + assembleDebug
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

Then, in the app, check these in order — each one isolates a layer:

1. the screen shows a track name field and no red error line → `nativeCreate`
   returned a real handle and `state` parsed;
2. `Scan` on `/sdcard/Music` fills the list → JNI strings, `walk`, the scanner and
   JSON arrays all work;
3. `Play` on a track makes the progress bar move over ~the track's length → the
   clock, the engine and the paced device agree with wall time;
4. pause freezes the bar, seek jumps it → transport wiring;
5. rotate the device and background/foreground it → the view model, not the
   activity, owns the handle (no crash, no second player).

If step 1 fails with `UnsatisfiedLinkError`, the `.so` is missing for that ABI or
`libaurora_mobile.so` was built without `--features jni-bridge`.

## Why a JSON façade instead of UniFFI

The spec's plan for Android is **UniFFI**. UniFFI cannot be vendored here: it
pulls `uniffi`/`uniffi_bindgen` plus a proc-macro and template stack, and this
sandbox reaches only GitHub, npm and PyPI. Rather than stop the Android work, the
boundary is a single `(method, argsJson) -> jsonString` call — the same shape
UniFFI generates, minus the generator:

* every command is a plain Rust method, so **all of it is unit-tested on the host**
  (the Kotlin app cannot contain a bug the tests cannot see);
* the JNI layer stays at four exported functions that only move strings;
* the payloads are owned plain data, which is what a future UniFFI switch needs, so
  replacing the façade with generated bindings is mechanical.

This is a deliberate, documented deviation, not an oversight.

## Files

| Path | What it is |
| --- | --- |
| `app/src/main/java/app/aurora/player/NativePlayer.kt` | the four `external fun`s, `Snapshot`/`LibraryTrack` parsing |
| `app/src/main/java/app/aurora/player/PlayerViewModel.kt` | all JNI calls on `Dispatchers.IO`, the 200 ms poll, `onCleared` closes the handle |
| `app/src/main/java/app/aurora/player/MainActivity.kt` | Compose screen: folder, library, transport, volume, the "simulated output" notice |
| `app/src/main/res/values-ar/strings.xml` | Arabic strings; the app is BiDi-correct from the first commit |
| `app/src/main/res/mipmap-*/ic_launcher.png` | the icon, drawn by `scripts/android/make-icon.py` (pure-Python PNG writer: no Pillow here) |
| `../scripts/android/build.sh` | the reproducible build recipe, with precondition checks |

## Known gaps

- **No audio.** AAudio/Oboe cannot be vendored here either; the next step is a
  `cpal`/`oboe` backend behind the same `AudioSource` trait the paced device
  implements.
- **No media-store access.** The scanner walks a path, so it needs a folder the app
  can read (`/sdcard/Music` after the permission prompt, or the app's own
  directory). Android's `MediaStore` and `content://` URIs are the Phase 9 work
  the spec describes as Strategy B; `PathStorage` is the seam they plug into.
- **No foreground service.** Playback stops when the app is killed, which is
  correct for a simulated device and wrong for a real one.
- **No persisted library.** The catalog is in memory; SQLite is Phase 3.
