# Aurora Player — Architecture (Phase 0)

Status: **Phase 0 deliverables complete**. This document is the architecture of
record for the desktop (Fedora/Wayland) target and the portability contract for
Android. It is updated at the end of every phase; sections marked
`[planned]` describe decisions taken now but implemented later.

Read together with `PROJECT_SPEC.md` (section 63, the Addendum, wins on any
conflict), `docs/technology-selection.md` (the evidence behind every choice),
`spikes/ui-spike/README.md` (the UI spike and its raw numbers), `PHASE_LOG.md`
and `AGENT_REPORT.md` (what was actually run, and what was not).

---

## 1. Shape of the system

```
                 ┌──────────────────────────────────────────────┐
                 │ aurora-app        (binary, composition root) │
                 │  wires engines + UI, owns the event loop     │
                 └───────────────┬──────────────────────────────┘
        ┌────────────────────────┼───────────────────────────┐
        │                        │                           │
┌───────▼────────┐      ┌────────▼────────┐        ┌─────────▼─────────┐
│ aurora-ui      │      │ aurora-audio    │        │ aurora-library    │
│ view model     │      │ decoder+output  │        │ scan, tags, index │
│ (no toolkit)   │      │ behind traits   │        │ + aurora-db       │
└───────┬────────┘      └────────┬────────┘        └─────────┬─────────┘
        │                        │                           │
        └────────────────────────┴───────────┬───────────────┘
                                             │
                                    ┌────────▼────────┐
                                    │ aurora-core     │
                                    │ domain + clock  │
                                    │ + traits        │
                                    └─────────────────┘
        ┌──────────────────────────────────────────────────────────┐
        │ aurora-lyrics: LRC/TTML/JSON parsing, discovery,         │
        │ estimated word timing (depends only on aurora-core)      │
        └──────────────────────────────────────────────────────────┘
```

Rules that are enforced, not aspirational:

- **Dependency direction is one-way.** `aurora-core` depends on std only.
  Everything else may depend on `aurora-core`; nothing below `aurora-app`
  depends on a UI toolkit, on Wayland/X11, or on `std::path` assumptions.
- **No UI toolkit in the engine crates.** `aurora-ui` is a pure view model
  (`Layout`, `LyricView`, `FramePlan`, `WordPlan`) and knows nothing about
  iced, egui or Compose. This is what makes the Android port a renderer swap
  rather than a rewrite (63.9).
- **The spike never ships.** `spikes/ui-spike` is a separate Cargo workspace,
  excluded from the root workspace (`exclude = ["spikes/ui-spike"]`), so a
  "temporary" iced/egui dependency cannot leak into the app graph.

### Workspace layout (as implemented)

| Crate | Role | Key public API |
| --- | --- | --- |
| `aurora-core` | Domain types, audio clock, motion curves, portability traits | `model::{TrackId, Track, TrackLocation, Album, Artist}`, `clock::{PlaybackClock, ClockSnapshot}`, `lyrics::{LyricDocument, LyricLine, TimedWord, TimingSource, Millis}`, `motion::{Spring, word_envelope}`, `transport::{Transport, PlaybackState}`, `traits::{AudioOutput, StorageBackend, AppDirectories, AudioFormat}` |
| `aurora-lyrics` | Discovery + parsers + estimated word timing | `parse_lrc`, `discover_lyrics`, `estimate_word_timing`, `LyricCandidate` |
| `aurora-audio` | Decoding and output, behind `AudioOutput` | `NullOutput` (Phase 0 placeholder), `SymphoniaDecoder` `[planned: Phase 2]` |
| `aurora-library` | Scanning and metadata probing | `Probe`, `FileNameProbe`, `scan`, `ScanError` |
| `aurora-db` | Persistence | `MemoryCatalog` (Phase 0 placeholder), `Catalog` trait `[planned: SQLite in Phase 3]` |
| `aurora-ui` | Toolkit-independent lyric view model | `Layout`, `LyricView::frame`, `FramePlan`, `WordPlan` |
| `aurora-app` | Composition root / binary | `cargo run -p aurora-app` prints the demo frame plan |

`cargo build`, `cargo fmt --check`, `cargo clippy -- -D warnings` and
`cargo test` all pass on this tree (see `PHASE_LOG.md` for the exact output).

---

## 2. UI stack decision (spike-gated, 63.5)

Candidates were **iced 0.14** (cosmic-text shaping, tiny-skia renderer) and
**egui 0.37** (epaint/harfrust, no BiDi reordering). Both were driven headlessly
into a 1280×800 software framebuffer, 60 FPS, 60 s, same scene, same fonts.

### 2.1 Measurements (this container, no GPU, software rasterizer)

| Candidate | render p50 | render p95 | render p99 | CPU (% of one core) | CPU ms/frame | peak RSS | 60 s FPS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| iced 0.14 (iced_tiny_skia) | 1.52 ms | 2.18 ms | 2.65 ms | 9.7 % | 1.63 ms | 19.7 MiB | 59.44 |
| egui 0.37 (epaint + spike rasterizer) | 4.01 ms | 5.38 ms | 6.05 ms | 24.5 % | 4.11 ms | 29.0 MiB | 59.48 |

GPU %: **not available** — this container exposes no DRM device, so
`report.json` records `"gpu_percent": null` with that note. No number here is a
GPU claim.

Caveat that must be repeated whenever this table is quoted: the egui column
includes *our* barycentric software rasterizer over epaint meshes; a real egui
app on a GPU backend would rasterize differently. The timing comparison is
therefore fair for the *CPU-side* work (layout + tessellation + fill), not for
GPU performance.

### 2.2 The decisive evidence: BiDi correctness

`spikes/ui-spike/evidence/*-probe.png` render three lines as single text runs,
bypassing our word boxes, so only the engine's own layout is visible:

- **iced/cosmic-text** — correct. The RTL paragraph is right-aligned; the Arabic
  run keeps logical order with correct joining and harakat; the embedded Latin
  `Hello Aurora` stays left-to-right inside the Arabic sentence.
- **egui/epaint** — wrong. The same string is laid out strictly left-to-right:
  `Hello Aurora` is reversed (`aroruA olleH`) and the Arabic tail comes out as
  disconnected, mis-ordered letters (`ي ف ا ل ظ ّ ل ا م`).

`egui-frame.png` / `iced-frame.png` show the shipping rendering path instead
(per-word boxes from cosmic-text, per-word scale/opacity/glow). Both look
correct there because the spike computes word boxes itself in both cases — which
is exactly why the whole-line probe is the test that matters, and why the spike
deliberately includes both views.

### 2.3 Decision

**iced 0.14** is the desktop UI stack (tiny-skia renderer; wgpu optional later).
Rationale: it is the only candidate whose own text engine produces correct
mixed Arabic/English output (a hard reject criterion in 63.5), it is faster on
the same workload, it keeps text layout in Rust so the shared view model and the
renderer agree, and its `iced::Application`/`Task` model fits a local player.
Rejected: egui (BiDi failure; would require an external BiDi pass and per-run
manual placement — i.e. reimplementing what cosmic-text already does), Slint
(separate markup language, weakest Rust-native text/lyric control), GTK4-rs
(excellent text stack via Pango but ties the UI to a C toolkit and complicates
the Android story), Xilem (immature, moving API).

The concrete risk: iced's public API churns between minor versions. Mitigation:
one `aurora-ui` view-model boundary, `iced` pinned by minor version, and the
renderer confined to `aurora-app`'s UI module.

`[planned: Phase 5]` The windowed app will use `iced::Application` with the
tiny-skia renderer first (no GPU requirement, predictable on Fedora/Wayland),
and can switch to the wgpu backend behind a feature flag once measured.

---

## 3. Audio strategy

- Decoding: **Symphonia** (pure Rust) for FLAC/MP3/AAC/OGG-Vorbis/WAV, with the
  same decoded stream used for both playback and (later) waveform data.
- Output: **CPAL**, with a PipeWire-backed path on Fedora (CPAL's ALSA host is
  the fallback when PipeWire's ALSA plugin is present; `pipewire-rs` is the
  documented escape hatch if ALSA emulation misbehaves).
- Everything sits behind `aurora_core::traits::AudioOutput`
  (`open/play/pause/seek/set_volume/frames_played/sample_rate/position`).
  Phase 0 ships `NullOutput`; Phase 2 replaces it.

Why traits instead of a concrete type: Android uses AAudio/Oboe, tests use a
deterministic fake, and the UI must never block on the audio device.

`[planned: Phase 2]` Gapless/crossfade, ReplayGain, and device-follow all belong
in `aurora-audio`, not in the UI.

## 4. The audio clock (63.4)

The single most important invariant in the app: **lyrics never follow a
free-running UI timer.**

`aurora_core::clock::PlaybackClock` stores one `(position, sampled_at, playing)`
triple behind a seqlock (an even/odd generation counter, no mutex on the read
path — a reader can never observe a torn `(position, instant)` pair; there is a
test for exactly that). Readers call `snapshot()` and then

```text
position(t) = anchor.position + (t - anchor.sampled_at) * rate - output_latency
```

`ClockSnapshot::position_at(now)` is monotonic while playing and only moves
backwards after an explicit seek (which re-anchors). `LyricView::frame` takes
both the snapshot and the frame's monotonic `Instant` — the view cannot invent
time from a frame counter, and a paused clock returns a constant position.

Tests cover: paused clock does not advance, playing clock extrapolates from the
sample, seek replaces the sample, snapshots report the instants they were given,
and concurrent readers never see torn pairs.

`[planned: Phase 2]` The audio thread publishes on every callback (or every N
callbacks) with the device's own `frames_played`; `output_latency` comes from
the device buffer size. `[planned: Phase 11]` A debug overlay (shortcut toggle)
shows audio position, interpolated position, drift in ms and frame time.

## 5. Lyrics strategy

Discovery order (63.2), implemented in `aurora_lyrics::discover_lyrics`:

1. sidecar next to the audio file, same base name: `.ttml`, `.lrc`, `.txt`,
   then the internal JSON format;
2. embedded tags (ID3 USLT/SYLT, Vorbis/FLAC `LYRICS`/`SYNCEDLYRICS`, MP4
   `©lyr`) — `[planned: Phase 6]`, needs the tag reader;
3. previously imported lyrics in the database;
4. nothing → the "Lyrics unavailable" state.

Parsers (`parse_lrc` today; TTML `[planned]`) all produce one internal model:
`LyricDocument { lines: [{ start, end, words: [{ text, start, end, timing }] }] }`
with `TimingSource::Exact | Estimated` on every line and word. The parser never
fails: malformed lines are skipped (a single typo must not hide a song), lines
are sorted by start time, `[offset:±ms]` is applied, and `end` is derived from
the next line when the file does not give one.

Estimated word timing (63.3), implemented in `aurora_lyrics::estimate_word_timing`:
the sung window is `line.start .. min(line.end, next.start - 120 ms)`; the window
is split proportionally to grapheme count (+1 per word minimum); every produced
word is marked `Estimated`. Words that already carry `Exact` timing are never
overwritten. Estimation is a setting (`ON` by default) and the UI must never
present estimated timing as exact.

## 6. Rendering and motion

- `aurora_ui::LyricView` owns the scroll spring (`Spring`, critically-damped-ish
  scroll with a target of `active_line_index * line_height - viewport*anchor`),
  and emits, per frame, `FramePlan { active_line, scroll_y, words }` where each
  `WordPlan` carries `activation`, `scale` and `opacity`.
- Word activation comes from `TimedWord::activation(at, fade)`; the envelope
  (`motion::word_envelope`) maps activation to scale/opacity. Blur is a renderer
  concern: the spike proves the stand-in (nested rounded quads with falling
  alpha, 3 layers, ≈0.3/0.22/0.14 alpha) is cheap; `[planned: Phase 8]` a real
  blur/shadow pass or a pre-baked glow sprite replaces it if it looks better.
- The renderer only draws what the plan says. No animation state in the UI
  toolkit.

## 7. Database and caching

`[planned: Phase 3, decision already taken]` **SQLite via `rusqlite`
(bundled)** — single file, transactional, well-understood, ships without system
dependencies on Fedora and Android, and gives FTS5 for search.

Phase 0 ships `aurora_db::MemoryCatalog` behind a `Catalog` trait so the engine
and tests do not depend on SQLite yet.

Cache layout comes from `AppDirectories` (`data_dir`, `config_dir`,
`cache_dir`, `lyrics_dir`, `database_file`) — on desktop XDG, on Android the
platform's scoped directories. Cached artefacts: imported lyrics (with their
`timing_source`), artwork thumbnails, waveform peaks `[planned]`, search index.
Nothing user-authored is ever written next to the audio files without an
explicit action; sidecars are read-only unless the user exports lyrics.

## 8. Testing strategy

- Unit tests live next to the code (`aurora-core`, `aurora-lyrics`, `aurora-ui`,
  `aurora-library`, `aurora-db`), with fixtures mirroring spec section 48:
  plain LRC, enhanced LRC, multi-timestamp, `[offset:]`, malformed lines,
  Arabic/Hebrew RTL, mixed RTL/LTR, CJK.
- The transport/clock seam is tested with a fake `AudioOutput` and a manual
  clock, so playback-state tests are deterministic (no sleeps).
- Integration tests `[planned: Phase 3]` for the SQLite catalog (open, scan,
  insert, query, migration) using a tempdir.
- The UI is tested at the view-model level (`FramePlan` assertions), not by
  pixel-diffing the toolkit.
- The spike is the only place allowed to measure pixels; it stays a separate
  workspace and is never part of `cargo test` for the app.

Current state: 13 unit tests, all green; `cargo clippy -- -D warnings` clean.

## 9. Portability and Android (63.9)

Android-visible abstractions, all in `aurora-core`:

| Concern | Trait | Desktop | Android |
| --- | --- | --- | --- |
| Audio out | `AudioOutput` | CPAL/PipeWire | AAudio/Oboe impl `[planned]` |
| Files | `StorageBackend` (`read`, `walk_audio`, `sibling_lrc`, `exists`) | `PathStorage` | content-URI storage `[planned]` |
| Dirs | `AppDirectories` | XDG paths | app-private dirs `[planned]` |
| Library | `Probe` | `FileNameProbe`, then `LoftyProbe` | same (pure Rust) |

Core state is input-agnostic: every interaction has a touch equivalent (tap a
lyric to seek, swipe between songs, mini player) and the transports are plain
enums, not mouse/keyboard events.

### Strategy A vs Strategy B

- **Strategy A — one Rust UI for both platforms.** Attractive (one renderer, one
  layout) but *not verified*: iced has no supported Android backend today; the
  spike could only be built for the host. Adopting A would mean maintaining a
  custom Android windowing/GL surface layer ourselves.
- **Strategy B — shared Rust core + FFI (UniFFI) + Kotlin/Jetpack Compose UI.**
  All engines stay in Rust; only the Android UI and platform glue (foreground
  service, media session, audio focus, permissions) are Kotlin. The lyric view
  model (`aurora-ui`) is already toolkit-free, so the Compose screen consumes
  the same `FramePlan`/`WordPlan` that iced consumes on desktop — the animation
  is not reimplemented, it is driven from Rust.

**Recommendation: Strategy B**, contingent on one Phase 2–3 experiment (expose
`PlaybackClock` + `FramePlan` over UniFFI and render a lyric screen in Compose
on a real device). Strategy A remains open only if iced publishes a maintained
Android backend.

### What was *not* done here (honest reporting)

- No Android SDK/NDK, no emulator and no physical device exist in this
  environment: no Android build or run was attempted, and none is claimed.
- No Wayland compositor, no GPU/DRM node, no PipeWire/ALSA device: the spike ran
  headless with a software rasterizer. The "run under a Wayland session on
  Fedora" requirement of 63.5 is therefore **not** satisfied yet; it is the
  first thing to re-run on the real target machine (`spikes/ui-spike/README.md`
  documents the exact command).

## 10. Decision log (rejected alternatives)

| Decision | Chosen | Rejected and why |
| --- | --- | --- |
| UI | iced 0.14 | egui (BiDi failure, measured), Slint (markup/UI language split, weaker lyric control), GTK4-rs (C toolkit, Android dead end), Xilem (immature) |
| Text layout | cosmic-text (inside iced) | egui/epaint + manual BiDi pass (reimplementing shaping/BiDi), HarfBuzz FFI (C dependency) |
| Audio decode | Symphonia | GStreamer (heavy C dependency, packaging pain), ffmpeg bindings (licensing/packaging) |
| Audio out | CPAL | raw PipeWire (ALSA fallback lost, no portability), rodio (thinner control over the clock) |
| Database | SQLite/rusqlite | redb (no SQL/FTS), sled (unmaintained), JSON files (no transactions) |
| Tags | lofty (later) | id3-only (no FLAC/MP4), taglib C bindings (packaging) |
| Lyrics | own parsers | third-party LRC crates (no word-level/offset fidelity, no estimated timing) |
| Motion | hand-written spring in core | toolkit animation APIs (unavailable on Android Compose side, and would split the animation between platforms) |
