# Technology selection (Phase 0)

Every choice below is scored against the criteria the spec asks for
(performance, animation capability, Linux/macOS/Windows support, GPU
acceleration, developer experience, long-term maintainability, Rust
integration, accessibility, packaging), plus a risk line.

**Evidence discipline.** Only rows marked *(measured)* were executed in this
repository. Everything else is engineering judgement from the projects'
documented capabilities and is marked *(assessment)*. Nothing here was measured
on Windows, macOS or Android, and nothing here was measured with a GPU.

---

## 1. UI toolkit

Candidates prototyped in `spikes/ui-spike` (same scene, same fonts, headless
1280×800, 60 s at 60 FPS, software rasterizer):

| Criterion | iced 0.14 *(measured)* | egui 0.37 *(measured)* | Slint *(assessment)* | GTK4-rs *(assessment)* |
| --- | --- | --- | --- | --- |
| Mixed Arabic/English order | **correct** (cosmic-text) | **wrong** (LTR only) | correct via Pango only in the Qt-style backend; text stack split | correct (Pango) |
| Arabic shaping | correct, with harakat | correct per-run, no BiDi | depends on backend | correct |
| Render p95 (this workload) | **2.18 ms** | **5.38 ms** | n/a | n/a |
| CPU, % of one core | **9.7 %** | **24.5 %** | n/a | n/a |
| Peak RSS | **19.7 MiB** | **29.0 MiB** | n/a | n/a |
| Animation model | retained widgets + `Task`, animation in our core | immediate mode, per-frame repaint | declarative states + `animate` blocks | GTK tick callbacks |
| GPU | tiny-skia (CPU) now, wgpu backend available | wgpu/glow backends | femtovg/skia | GL renderer |
| Linux | Wayland/X11 via winit | wayland/x11 | wayland/x11 | Wayland/X11 native |
| macOS / Windows | yes (documented) | yes | yes | macOS via gtk-osx, Windows MSYS2 |
| Rust integration | native Rust, no C++ | native Rust | Rust but with `.slint` markup + separate language server | Rust bindings over a C toolkit |
| Accessibility | accesskit support | accesskit support | limited | strong (AT-SPI) |
| Packaging (Fedora) | one static binary | one static binary | one binary + build step for markup | needs GTK4 runtime |
| Android | no supported backend | experimental (eframe has android support, egui itself is portable) | Android backend exists (Slint targets mobile) | no |
| Verdict | **chosen** | rejected (hard reject: BiDi) | not evaluated in code | rejected (C toolkit, Android dead end) |

Why egui is rejected outright: 63.5 says a candidate that cannot render Arabic
correctly is rejected. `evidence/egui-probe.png` shows reversed Latin and
disconnected Arabic inside the same line — this is a property of epaint's
layout (shaping without BiDi reordering), not a bug we can configure away.

Risk register for iced:

1. **API churn between minors.** Mitigation: the app depends on `aurora-ui`
   (our view model), not on iced types; the toolkit is confined to one module
   in `aurora-app`; pinning by minor version and upgrading deliberately.
2. **Wayland specifics** (fractional scaling, IME, window decorations) not run
   here — no compositor available. First task on the Fedora machine.
3. **Text layout caching**: iced re-shapes when the text content changes. Our
   lyric lines are static per track, so we cache shaped lines per style and
   only animate transform/alpha (the spike does exactly this and holds 59.4 FPS
   with 40 lines).
4. **Blur.** iced has no blur primitive in the tiny-skia backend. Mitigation:
   layered alpha quads now; pre-baked glow sprites or a shadow pass later. The
   spec asks for "blur or the closest the framework supports", and the layered
   halo is measured at negligible cost.

## 2. Audio output and decoding

| Concern | Choice | Alternatives | Why | Risk |
| --- | --- | --- | --- | --- |
| Decoding | **Symphonia** (pure Rust: FLAC, MP3, AAC/ALAC, Vorbis, WAV, ADPCM) | GStreamer, ffmpeg-next, rodio | no C toolchain, no licensing trap, one decoder for both platforms, streaming API with seek | format gaps (e.g. APE, WavPack) — acceptable for a personal library; WavPack/APE can be added later behind the same trait |
| Output | **CPAL** (PipeWire through ALSA on Fedora; AAudio later via a second impl) | `pipewire-rs` directly, `rodio`, `alsa` crate | cross-platform trait-shaped API, small surface, keeps the device open/close and latency in our hands | ALSA emulation layer under PipeWire can report poor latency; escape hatch documented (`pipewire-rs` behind the same trait) |
| Clock source | `AudioOutput::frames_played()` + `sample_rate()` | system timer | 63.4 forbids free-running timers; device frame count is the only honest source | device callbacks jitter — handled by the seqlock publish + interpolation model |
| Resampling | `[planned: Phase 2]` rubato or device-native rate | none | avoid resampling unless the device rate differs | decide with measurements in Phase 2 |

Not verified here: this container has no audio device (no PipeWire, no ALSA).
The Phase 0 `NullOutput` implements `AudioOutput` with a fake frame counter so
the clock and transport can be tested deterministically.

## 3. Database

| Criterion | SQLite via `rusqlite` (bundled) | redb | sled | JSON files |
| --- | --- | --- | --- | --- |
| Transactions | yes | yes | yes | no |
| SQL / FTS5 search | **yes** | no | no | no |
| Single file, portable | yes | yes | yes | yes (many files) |
| Android | bundles and works | works | works | works |
| Recovery tooling | excellent | limited | poor | none |
| Maturity | very high | high | unmaintained | n/a |

**Chosen: SQLite (`rusqlite`, bundled feature).** Bundling means no dependency
on the host's `libsqlite3` on Fedora or Android, and FTS5 covers library search
without a second index engine. Risk: it is a C library inside a Rust binary —
acceptable, it is the most widely deployed database in the world and the build
is well supported. Phase 0 ships `MemoryCatalog` behind a `Catalog` trait; the
SQLite implementation lands in Phase 3 with migration tests.

## 4. Metadata / tags

**Chosen (Phase 3): `lofty`.** Pure Rust, covers ID3v2 (incl. SYLT/USLT),
Vorbis/FLAC comments, MP4 atoms, plus ReplayGain and artwork — exactly the
formats 63.2 needs for embedded lyrics.

Alternatives and why not:
- `id3` — ID3 only; FLAC/MP4 would need a second library.
- `metaflac` / `claxon` — FLAC only.
- `taglib` bindings — C++ dependency, packaging and Android cross-compile pain.
- writing our own ID3/FLAC parsers — justified only for a narrow subset; lofty is
  maintained and pure Rust.

Risk: lofty's write path can still rewrite files; Aurora will only *read* tags by
default (no silent tag edits).

## 5. Lyrics parser

**Chosen: our own parsers in `aurora-lyrics`** (Phase 0 already implements plain
and enhanced LRC).

Why not a crate: the spec requires a specific internal model
(`LyricDocument`/`LyricLine`/`TimedWord` with `TimingSource`), `[offset:±ms]`
handling, multi-timestamp lines, graceful degradation on malformed input,
estimated word timing for plain LRC, and UAX #29 word splitting for Arabic and
CJK. Off-the-shelf LRC crates lose at least one of these and would have to be
wrapped anyway. The parser is ~300 lines with tests, and it is the component the
whole lyric experience depends on, so it belongs in this repo.

Alternatives: `lrc` crate (line-level only, no enhanced/word timing),
`symphonia`-adjacent community parsers (no offset/estimated timing). TTML
(`<p>`/`<span>` with word timing) is a `[planned: Phase 7]` stretch goal; if it
is skipped, the report will say so.

Risk: TTML is a large format; we will implement only the lyric-relevant subset
(paragraphs, spans, `begin`/`end`, `ttm:role`) and document what is unsupported.

## 6. Supporting choices

| Need | Choice | Note |
| --- | --- | --- |
| Errors | `thiserror` in libraries, `anyhow` only in the binary | typed errors at crate boundaries |
| Serialization | `serde`/`serde_json` | internal JSON lyric format, config |
| Logging | `tracing` `[planned]` | spans for scan/decode, off by default in release |
| FFI for Android | `uniffi` `[planned: Phase 12+]` | generates Kotlin bindings from the Rust core |
| Workspace | Cargo workspace, edition 2024, resolver 3 | one binary + libraries |

## 7. Rejected global alternatives

- **Electron/Tauri/web UI** — banned by section 2 and 63.1 (no webview shell
  moving logic out of Rust).
- **Flutter/Dart UI on Android with a Rust core** — possible, but Compose is the
  native path and needs no second language runtime in the app.
- **Python/C++ audio engines** — violates the "all core logic in Rust" rule.
- **GTK4-rs desktop + Compose Android** — two UI toolkits, two text stacks, and
  the Android side still needs FFI: strictly more work than Strategy B with no
  measured advantage.
