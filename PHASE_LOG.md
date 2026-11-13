# Phase log

Newest phase last. Every entry states what was actually executed, on what
hardware/environment, and what was *not* verified (PROJECT_SPEC.md 63.8).

Environment for all Phase 0 work: Fedora-class container, x86_64, **no GPU/DRM
node, no Wayland compositor, no PipeWire/ALSA device, no Android SDK/NDK, no
network access to crates.io**, Rust 1.99.0 (a standalone toolchain; `rustdoc` is
not part of it, so doctests cannot run). Anything that depends on those missing
pieces is marked as not verified rather than assumed.

---

## Phase 0 — Architecture (2026-11-13)

### Delivered

1. **Mandatory UI spike (63.5)** — `spikes/ui-spike`, separate Cargo workspace,
   candidates iced 0.14 (`iced_tiny_skia`) and egui 0.37 (epaint). Same scene:
   40 lyric lines (Arabic / English / mixed hero line), per-word
   scale+opacity+layered-glow highlight on a 60 FPS timeline, animated
   auto-follow spring, IBM Plex Sans Arabic from `assets/fonts/`, rendered
   headlessly into a 1280×800 framebuffer.
   - 60 s runs, both candidates: see the table in `ARCHITECTURE.md` §2 and
     `spikes/ui-spike/README.md`. Raw data in `spikes/ui-spike/evidence/`.
   - BiDi verdict images (`evidence/iced-probe.png`, `evidence/egui-probe.png`):
     iced orders the mixed line correctly, egui does not (reversed Latin,
     disconnected Arabic).
   - GPU % is `null`: no GPU device in this container.
2. **Technology selection** — `docs/technology-selection.md` (UI, audio decode
   and output, database, tags, lyrics parser, supporting crates), with
   alternatives, rejection reasons and risks. Measured rows are marked; the
   rest are explicitly assessments.
3. **Architecture** — `ARCHITECTURE.md`: workspace/crate layout, domain models,
   state separation, the atomic `(position, instant)` audio clock, lyrics
   discovery + estimated word timing, rendering/motion split, database and
   caching plan, testing strategy, portability traits and the Android
   Strategy A/B decision.
4. **Compiling workspace skeleton** — `aurora-core`, `aurora-lyrics`,
   `aurora-audio`, `aurora-library`, `aurora-db`, `aurora-ui`, `aurora-app`.
   Real code, no placeholder features: LRC parser with offsets/multi-timestamps,
   estimated word timing, seqlock playback clock, spring motion, lyric view
   model, path storage, file-name probe, null audio output, in-memory catalog.

### Gates (exact commands and results)

| Gate | Command | Result |
| --- | --- | --- |
| format | `rustfmt --edition 2024 --check $(find crates -name '*.rs')` | clean |
| format (spike) | `rustfmt --edition 2024 --check $(find spikes/ui-spike/src -name '*.rs')` | clean |
| lints | `cargo clippy --all-targets -- -D warnings` | 0 warnings |
| lints (spike) | `cargo clippy --release` in `spikes/ui-spike` | 0 warnings |
| tests | `cargo test --lib --bins --tests` | 28 passed, 0 failed |
| build | `cargo build` | ok |
| doctests | `cargo test --doc` | **could not run**: `rustdoc` is not installed in this toolchain |

`cargo fmt` itself is unavailable (the `cargo-fmt` subcommand is not shipped with
this standalone toolchain), so `rustfmt` was invoked directly on the same file
sets — same formatter, same edition, same config.

### Deliberately not done

- No Wayland/GPU run of the spike: no compositor, no DRM device. This is the
  single outstanding 63.5 item; the exact commands are in the spike README.
- No Android build/run of the spike: no SDK, NDK, emulator or device. Strategy A
  vs B is argued from the architecture and iced's lack of an Android backend,
  and is explicitly labelled provisional.
- No windowed app yet (that is Phase 5). `aurora-app` prints a demo frame plan.
- No audio device, so playback timing was exercised through `NullOutput` and the
  clock tests only.

### Notes / gotchas worth keeping

- The dev sandbox has no crates.io access; dependencies come from a vendored
  directory source built by `scripts/vendor/vendor.py` (documented in
  `scripts/README.md`, including the two source patches it applies).
- Two real bugs were found and fixed while making the spike honest: word boxes
  were overlapping because `LyricLine::text()` inserts its own separators (the
  spike now builds the drawn string itself), and RTL words disappeared because
  they were drawn into a full-width bound (each word now gets its own box).
- iced's `into_color` swaps R/B and tiny-skia premultiplies; the spike documents
  both in `rgba()` so colours are not a mystery next phase.

### Recommended next step

Phase 1 (Rust foundation: config, directories, error types, logging,
settings persistence, app shell wiring) — after, if possible, re-running the two
spike commands on the actual Fedora/Wayland machine so the "60 FPS on Wayland"
claim becomes verified instead of pending.
