# Agent report

Short, honest status for the human. Updated at the end of every phase.

---

## Phase 0 — Architecture — DONE (2026-11-13)

### What works (verified by running it)

- The Cargo workspace builds; `cargo clippy --all-targets -- -D warnings` is
  clean; **28 unit tests pass**.
- The lyrics engine parses plain LRC, enhanced (word-timed) LRC, multi-timestamp
  lines, `[offset:±ms]`, tolerates malformed lines, and estimates word timing
  for plain LRC (all covered by tests).
- The playback clock is a seqlock over `(position, instant, playing)`; tests
  prove readers never see a torn pair, that a paused clock does not advance and
  that a seek re-anchors.
- The lyric view model produces `FramePlan`/`WordPlan` (active line, scroll
  offset, per-word activation/scale/opacity) with no UI toolkit involved.
- The UI spike runs both candidates for 60 s at ~59.4 FPS in a 1280×800
  software framebuffer and writes a report + screenshot:
  - iced: render p95 **2.18 ms**, CPU **9.7 %** of one core, **19.7 MiB** RSS;
  - egui: render p95 **5.38 ms**, CPU **24.5 %**, **29.0 MiB** RSS.
- Arabic rendering verified visually: iced shapes and orders a mixed
  Arabic/English line correctly (screenshot evidence committed); egui shapes the
  glyphs but lays the line out LTR, reversing Latin and disconnecting Arabic.

### What does not work / is not verified

- **No Wayland session, no GPU.** The spike ran headless with a software
  rasterizer, so "60 FPS on Fedora/Wayland" is *not* verified and GPU numbers do
  not exist (`gpu_percent: null`). Rerun on the real machine:
  `cd spikes/ui-spike && cargo run --release -- --candidate iced --seconds 60 --out out/iced`.
- **No Android.** No SDK/NDK/emulator/device here, so the 63.9 Android spike was
  not attempted. Strategy B (shared Rust core + Compose UI) is the recommendation
  and is labelled provisional.
- **No audio hardware.** `NullOutput` only; no real PipeWire playback yet.
- **No windowed application.** `aurora-app` is a console demo that prints a frame
  plan; the real window arrives in Phase 5.
- **Doctests were not run** (`rustdoc` is missing from this toolchain).

### How to run what exists

```text
cargo build
cargo test --lib --bins --tests
cargo run -p aurora-app                 # prints a lyric frame plan for the demo track

cd spikes/ui-spike
cargo run --release -- --candidate iced --seconds 60 --out out/iced
cargo run --release -- --candidate iced --probe --out out/probe-iced   # BiDi evidence
```

### Recommendation

Start **Phase 1 (Rust foundation)** next, and on the real Fedora machine first
re-run the two spike commands (plus `--probe`) so the Wayland/60 FPS claim moves
from "pending" to "verified" before the UI work in Phase 5 depends on it.

---

## Phase 1 — Rust foundation — DONE (2026-11-13)

### What works (verified by running it)

- iced 0.14 (tiny-skia, Wayland) **builds and links offline** from the vendored
  directory source. That was the phase's prerequisite: the whole toolkit graph
  (iced → winit → smithay-client-toolkit/softbuffer/zbus) now resolves.
- `cargo build --workspace` succeeds; clippy `--all-targets -- -D warnings` is
  clean; **85 unit tests pass** (was 28).
- The binary boots end to end: XDG directories are created, `settings.toml` is
  created-or-loaded, `tracing` is installed, and it picks a front end.
- The **§27 settings surface** is implemented as typed data (playback, lyrics,
  appearance, performance, library), persisted as TOML, saved atomically after
  every UI change, clamped and defaulted on load, and tolerant of unknown keys.
- **Headless mode** loads settings, scans the configured music folders into the
  catalog and reports counts. `--headless`, `AURORA_HEADLESS=1` and "no
  compositor" all reach it.
- A **stale `WAYLAND_DISPLAY` is handled**: the socket is probed before iced is
  allowed to run, and the window call is wrapped in `catch_unwind`, because iced
  panics (not errors) when the compositor connection fails.
- `--demo` runs the Phase 0 pipeline again, now against a real `fixtures/`
  library (it previously failed on a clean checkout: no such directory existed).

- The shell's layout is rendered headlessly through iced's own layout/draw passes
  (`src/shot.rs`, test-only) and committed to `docs/evidence/`: all three panes,
  with toggles showing their real on/off state. Tests assert the renders are
  non-blank, deterministic and respond to settings changes.

### What does not work / is not verified

- **No window has ever been displayed here.** No compositor, no DRM device: the
  window path (surface creation, HiDPI, input, frame pacing) is unproven. Run
  `cargo run -p aurora-app` on Fedora first.
- **Screenshot colours are swapped** (red↔blue) by an upstream `iced_tiny_skia`
  bug — the blue accent renders orange in `docs/evidence/`. Not compensated for
  in our code, because the wgpu backend is not affected; a characterisation test
  fails loudly if iced fixes it.
- **X11 is unsupported in this build**: the X11 backend needs `ctor ^0.10`, which
  has no vendorable source here. X11-only sessions get a warning and headless
  mode.
- **No audio, no lyric rendering, no SQLite** yet — Phase 2/3 work. The catalog
  is still in-memory, so a headless scan is discarded on exit.
- The settings pane's accent colour and artwork effects are stored but not
  painted yet.
- Doctests still cannot run (no `rustdoc`).

### How to run what exists

```text
cargo run -p aurora-app                  # shell window (Wayland), else headless
cargo run -p aurora-app -- --headless    # settings + scan, no window
cargo run -p aurora-app -- --demo        # Phase 0 pipeline over fixtures/
cargo run -p aurora-app -- --help
```

Settings live at `~/.config/aurora/settings.toml`; logs honour `AURORA_LOG`.

### Recommendation

On the Fedora machine: run the app, then the two spike commands, so both the
window and the 60 FPS claim become verified. Then Phase 2 — Symphonia decode +
CPAL output behind `AudioOutput`, with the clock driven by the device counter.

---

## Phase 2 (partial) — decoder seam + WAV — DONE (2026-11-13)

### What works (verified by running it)

- A dependency-free WAV decoder in `aurora-audio` behind a `Decoder` trait:
  PCM 8/16/24/32-bit, IEEE float 32/64, any channel count, `WAVE_FORMAT_EXTENSIBLE`,
  odd-sized chunk padding, `fmt ` after `data`, streamed `data` sizes.
- `aurora --decode <FILE>` prints format, frames, duration, peak (dBFS) and RMS;
  it decodes the whole file in blocks, so a corrupt stream fails immediately.
- `fixtures/music/00 - Fixture Tone.wav` (0.05 s, −12 dBFS) proves it end to end.

### What does not work / is not verified

- **No audio output at all**: no ALSA/PipeWire in this container and `cpal`
  cannot be vendored (needs `alsa-sys`), so `NullOutput` remains the only
  `AudioOutput` and the clock is still driven by tests, not a device.
- **No FLAC/ALAC/MP3/AAC/Opus**: `symphonia` is not in the offline mirror.
  The decoder trait is ready for it.
- No resampling, channel mapping, gapless handling or seek yet.
