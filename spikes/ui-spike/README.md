# Phase 0 UI spike — iced 0.14 vs egui 0.37

Throwaway prototype required by PROJECT_SPEC.md 63.5. It is **not** part of the
app: it lives in its own Cargo workspace (`spikes/ui-spike/Cargo.toml` declares
an empty `[workspace]`), and the root workspace excludes it.

## What it renders

One scene, drawn by both candidates:

- 40 lyric lines (Arabic-only, English-only, and the mixed hero line
  `سَلامٌ على الدُّنيا Hello Aurora في الظَّلام`), generated as LRC, parsed by
  `aurora-lyrics`, and animated by `aurora-ui`'s spring/`FramePlan`.
- Per-word highlight: scale, opacity and a layered-alpha glow (the "closest to
  blur" that both toolkits support without a post-processing pass).
- Animated auto-follow: `LyricView` scrolls the column so the active line rests
  at the configured anchor.
- 1280×800, 60 FPS target, 60 s measurement window.
- Fonts are shipped in the repo (`assets/fonts/`) and loaded from bytes, so
  rendering does not depend on what the host has installed.

Two rendering paths are measured deliberately:

1. **Shipping path** — word boxes come from `cosmic-text` (same layout engine
   the app uses), and each backend draws the words. This is what
   `frame.png` shows.
2. **Whole-line probe** (`--probe`) — each backend draws the three lines as
   single text runs with no help from us. This isolates the toolkit's own BiDi
   handling and is the image the UI decision is based on.

## Run it

```text
cd spikes/ui-spike
cargo run --release -- --candidate iced --seconds 60 --out out/iced
cargo run --release -- --candidate egui --seconds 60 --out out/egui
cargo run --release -- --candidate iced --probe --out out/probe-iced
cargo run --release -- --candidate egui --probe --out out/probe-egui
```

Outputs per run: `frame.png` (or `probe.png` with `--probe`) and `report.json`
(`render_ms` and `frame_ms` percentiles, CPU seconds, CPU % of one core, peak
RSS, FPS, plus an explicit `gpu_percent: null` when no GPU is available).
`SPIKE_DEBUG=1` dumps the measured word boxes per line.

## Results (this container)

Both candidates, 60 s, 1280×800, software rasterizer, no GPU:

| Metric | iced 0.14 | egui 0.37 |
| --- | --- | --- |
| frames / measured FPS | 3567 / 59.44 | 3569 / 59.48 |
| render p50 | 1.52 ms | 4.01 ms |
| render p95 | 2.18 ms | 5.38 ms |
| render p99 | 2.65 ms | 6.05 ms |
| frame p95 | 16.85 ms | 16.83 ms |
| CPU (% of one core) | 9.7 % | 24.5 % |
| CPU ms per frame | 1.63 ms | 4.11 ms |
| peak RSS | 19.7 MiB | 29.0 MiB |
| GPU % | n/a (`null`) | n/a (`null`) |

Raw files: `evidence/iced-report.json`, `evidence/egui-report.json`.

## Verdict

- **Arabic shaping**: both shape correctly (iced via cosmic-text, egui via
  harfrust).
- **RTL ordering of a mixed line** (`evidence/*-probe.png`):
  - iced — correct: RTL sentence right-aligned, Arabic logical order preserved,
    embedded `Hello Aurora` kept LTR.
  - egui — **wrong**: the whole line is laid out LTR, `Hello Aurora` appears as
    `aroruA olleH`, and the Arabic tail comes out as disconnected, reversed
    letters.
- A hard reject criterion of 63.5 is "if no candidate renders Arabic correctly,
  that candidate is rejected" → **egui is rejected, iced is chosen**
  (`ARCHITECTURE.md` §2).

The GPU column is empty on purpose: this machine has no DRM device. The egui
timing also includes this spike's own software rasterizer for epaint meshes, so
its numbers describe CPU-side layout + tessellation + fill, not a GPU egui app.

## Environment honesty (63.8)

This spike ran **headless in a container**:

- no Wayland compositor → the "run it under a Wayland session on Fedora"
  requirement is **not verified**; rerun the two commands above on the target
  machine, and check window creation, fractional scaling and VSync behaviour;
- no GPU/DRM node → no GPU utilisation numbers, everything is software
  rasterized;
- no audio device → the timeline is driven by a synthetic 60 Hz clock and the
  `aurora-core` clock model, not by real playback;
- no Android SDK/NDK, emulator or device → no Android spike was attempted
  (documented in `ARCHITECTURE.md` §9), and no Android claim is made.

## Build notes (offline vendoring)

The machine that produced these numbers had no crates.io access. Dependencies
were resolved through a local vendor directory driven by
`scripts/vendor/vendor.py` (see `scripts/README.md`). Two crates needed small,
documented source fixes to build (recorded in that script):

- `cosmic-text` 0.15.0 and 0.19.0: `Blob::new(data)` no longer matches
  `linebender_resource_handle` 0.1.1's `BlobStorage` bound; data is copied into
  an `Arc<Vec<u8>>` instead.
- `unicode-linebreak` 0.1.5: the crates.io tarball omits the generated
  `src/tables.rs`; it is grafted from the makepad mirror.

Neither patch touches shaping, BiDi or line breaking results.

## Known limitations of the spike itself

- Lines are drawn word-by-word (that is what per-word highlight needs); Arabic
  words are natural shaping units, so this is correct, but ligatures or marks
  that cross a space are not modelled (none exist in Arabic).
- The glow is three layered rounded quads, not a blur.
- Scroll easing is `aurora-ui`'s spring, measured on a synthetic 16 ms frame
  clock; jitter from a real compositor is not reproduced.
