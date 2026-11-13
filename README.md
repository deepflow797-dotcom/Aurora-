# Aurora Player

A personal, local-first music player written in Rust. Desktop first (Fedora
Linux, Wayland, PipeWire), Android second. No streaming, no accounts, no
webview: playback, library, lyrics and animation are all Rust.

Built from `PROJECT_SPEC.md` (section 63 — the Addendum — overrides earlier
sections where they conflict). Working rules for the agent live in `AGENTS.md`.

## Status

**Phase 0 — Architecture: complete.** No playable app yet: this phase delivers
the architecture, the technology selection, a compiling workspace skeleton and
the mandatory UI spike that picks the UI stack.

| Document | What it is |
| --- | --- |
| `ARCHITECTURE.md` | System shape, crate layout, audio clock, lyrics strategy, UI spike results, Android strategy |
| `docs/technology-selection.md` | Every technology choice with alternatives, risks and the measured evidence |
| `spikes/ui-spike/` | Throwaway iced vs egui benchmark (screenshots + JSON reports in `evidence/`) |
| `PHASE_LOG.md` | What each phase actually did, with the exact gate results |
| `AGENT_REPORT.md` | Short honest status: what works, what was not verified |
| `scripts/README.md` | Offline vendoring tooling used in this sandbox |

## Layout

```
crates/aurora-core      domain types, seqlock audio clock, motion, portability traits
crates/aurora-lyrics    LRC/JSON parsing, discovery, estimated word timing
crates/aurora-audio     decoding + output behind AudioOutput (NullOutput today)
crates/aurora-library   scanning and metadata probing
crates/aurora-db        persistence behind a Catalog trait (in-memory today)
crates/aurora-ui        toolkit-independent lyric view model
crates/aurora-app       binary / composition root
spikes/ui-spike         separate workspace: UI candidates (never shipped)
assets/fonts            IBM Plex Sans Arabic + IBM Plex Sans (OFL)
```

## Build and test

```text
cargo build
cargo test --lib --bins --tests
cargo clippy --all-targets -- -D warnings
cargo run -p aurora-app          # prints a lyric frame plan for the demo track
```

## Chosen stack (details and evidence in the docs)

- **UI:** iced 0.14 — the only spike candidate that orders mixed Arabic/English
  text correctly (cosmic-text shaping + BiDi); measured faster than egui on the
  same scene.
- **Audio:** Symphonia (decode) + CPAL (output), both behind `AudioOutput`.
- **Database:** SQLite via `rusqlite` (bundled), FTS5 for search.
- **Tags:** `lofty` (pure Rust).
- **Lyrics:** in-repo parsers — plain LRC, enhanced LRC, internal JSON; TTML
  planned; estimated word timing for line-only files.
- **Android:** shared Rust core + UniFFI + Jetpack Compose (Strategy B),
  provisional until measured on a device.

## Not verified yet

The development sandbox has no GPU, no Wayland compositor, no audio device and
no Android tooling. The spike's 60 FPS numbers are software-rasterizer numbers;
GPU %, Wayland behaviour and Android are explicitly pending (`PHASE_LOG.md`).
