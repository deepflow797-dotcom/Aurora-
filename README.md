# Aurora Player

A personal, local-first music player written in Rust. Desktop first (Fedora
Linux, Wayland, PipeWire), Android second. No streaming, no accounts, no
webview: playback, library, lyrics and animation are all Rust.

Built from `PROJECT_SPEC.md` (section 63 — the Addendum — overrides earlier
sections where they conflict). Working rules for the agent live in `AGENTS.md`.

## Status

**Phase 1 — Rust foundation: complete.** The app now boots: it resolves XDG
directories, loads (or creates) `~/.config/aurora/settings.toml`, installs
logging, then opens an iced window on Wayland — or runs headless when there is
no compositor. No audio yet (Phase 2) and no lyric rendering (Phase 3); Phase 0
delivered the architecture, the technology selection and the UI spike that chose
iced.

The shell's layout is captured headlessly in `docs/evidence/shell-*.png`
(rendered through iced's real layout/draw passes). It has **not been displayed on
a compositor** — this container has none — so `cargo run -p aurora-app` on the
Fedora machine is the first true check. Note that those screenshots have red and
blue swapped by an upstream `iced_tiny_skia` bug; see `PHASE_LOG.md`.

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
crates/aurora-platform  XDG directories, settings (TOML), logging, session probe
crates/aurora-app       binary / composition root (the only crate with iced)
fixtures                tiny demo library (placeholder .flac + sidecar .lrc)
spikes/ui-spike         separate workspace: UI candidates (never shipped)
assets/fonts            IBM Plex Sans Arabic + IBM Plex Sans (OFL)
```

## Build and test

```text
cargo build
cargo test --lib --bins --tests
cargo clippy --all-targets -- -D warnings
cargo run -p aurora-app                 # the shell window (headless without Wayland)
cargo run -p aurora-app -- --headless   # settings + library scan, no window
cargo run -p aurora-app -- --demo       # Phase 0 pipeline over fixtures/
cargo run -p aurora-app -- --decode "fixtures/music/00 - Fixture Steps.flac"
cargo run -p aurora-app -- --play "fixtures/music/00 - Fixture Steps.flac"
```

## Settings

Stored as TOML at `$XDG_CONFIG_HOME/aurora/settings.toml` (usually
`~/.config/aurora/settings.toml`), written atomically on every change. It covers
the §27 surface: volume, repeat, shuffle, resume; lyrics on/off, animation
profile, auto-scroll, word highlighting, font scale; theme, accent,
animation intensity, artwork effects; reduced motion, GPU effects, cache size,
background scanning; music folders, rescan and folder watching. Unknown keys are
ignored and out-of-range values are clamped, so hand-editing is safe.

Logging goes to stderr at `AURORA_LOG` level (`error|warn|info|debug|trace`);
`AURORA_HEADLESS=1` forces the headless path.

## Chosen stack (details and evidence in the docs)

- **UI:** iced 0.14 on the tiny-skia renderer — the only spike candidate that
  orders mixed Arabic/English text correctly (cosmic-text shaping + BiDi);
  measured faster than egui on the same scene. Built Wayland-only offline: the
  X11 backend needs a `ctor` release that cannot be vendored here (see
  `scripts/README.md`).
- **Audio:** Symphonia (FLAC/MP3/AAC/ALAC/CAF) plus in-crate WAV and AIFF
  decoders, behind one `Decoder` trait, feeding a `PlaybackEngine` that
  publishes the audio clock. Every decoder can seek to, or before, a requested
  position. CPAL for output is still missing (no ALSA in this sandbox), so
  playback runs against a simulated device: `aurora --decode <file>` and
  `aurora --play <file>`. See the format table below for what is verified.
- **Database:** SQLite via `rusqlite` (bundled), FTS5 for search.
- **Tags:** `lofty` (pure Rust).
- **Lyrics:** in-repo parsers — plain LRC, enhanced LRC, internal JSON; TTML
  planned; estimated word timing for line-only files.
- **Android:** shared Rust core + Jetpack Compose (Strategy B). `crates/aurora-mobile`
  is the app core (JSON façade + handle registry, tested on the host) and
  `android/` is the Compose app. FFI is hand-written JSON rather than UniFFI
  because UniFFI cannot be vendored offline. **No APK has been built and nothing
  has run on a device**: this machine has no JDK, NDK or Android std — see
  `android/README.md`, which also lists exactly what to run first on a machine
  that has them.

## Format support

`Verified` means a decoder exists *and* a test decodes committed audio with it.
MP3/AAC/ALAC/CAF build but no fixture exercises them: this sandbox cannot fetch
an encoder for any of them, and a hand-written MP3 is not a serious proposition.

| Format | Container / codec | Decoder | State |
| --- | --- | --- | --- |
| WAV | RIFF/WAVE, PCM 8/16/24/32 + float 32/64, `EXTENSIBLE` | `aurora-audio::wav` | Verified (`00 - Fixture Tone.wav`) |
| AIFF | FORM/AIFF, AIFF-C `NONE`/`twos`/`sowt`/`fl32`/`fl64` | `aurora-audio::aiff` | Verified (`00 - Fixture Tone.aiff`) |
| FLAC | native FLAC | Symphonia | Verified (`00 - Fixture Steps.flac`) |
| ALAC | ALAC in CAF/M4A | Symphonia | Compiles, not exercised |
| MP3 | MPEG-1/2/2.5 Layer III | Symphonia | Compiles, not exercised |
| AAC | AAC-LC in ADTS/M4A | Symphonia | Compiles, not exercised |
| CAF | Core Audio Format | Symphonia | Compiles, not exercised |
| Ogg/Vorbis | — | — | Not supported: needs `symphonia-format-ogg`, not vendored |
| Opus | — | — | Not supported, same reason |

Rejected by name rather than guessed at: µ-law, A-law, ADPCM, IMA4, RF64.

## Not verified yet

The development sandbox has no GPU, no Wayland compositor, no audio device and
no Android tooling. The spike's 60 FPS numbers are software-rasterizer numbers;
GPU %, Wayland behaviour and Android are explicitly pending (`PHASE_LOG.md`).
