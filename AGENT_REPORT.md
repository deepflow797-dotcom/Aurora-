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
