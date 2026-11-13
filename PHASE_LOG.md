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

---

## Phase 1 — Rust foundation (2026-11-13)

Goal: config, directories, error types, logging, settings persistence, and app
shell wiring, with the iced stack proven to build offline before any of it is
built on top.

### 1. Offline iced build: the blocker, and how it was closed

`iced 0.14` now **builds and links** in this sandbox from the vendored directory
source (`/tmp/iced-probe`, a throwaway crate with only iced as a dependency, and
the app workspace itself). Getting there took five real fixes in
`scripts/vendor/vendor.py`, all documented in `scripts/README.md`:

1. **Tag selection had to become a curated policy.** `wayland-rs`'s default
   branch is pre-release 0.31.14 and moved `WEnum` out of `wayland_client`'s root,
   breaking `smithay-client-toolkit` 0.19/0.20 (157–184 errors). `softbuffer`'s
   branch is a 0.5-era rewrite that iced cannot compile against. Those crates are
   now in `NO_BRANCH` and come from release tags; `lyon` publishes releases from
   `main` without tags, so it is in `PREFER_HEAD`.
2. **Path dependencies are stripped.** A flat vendor tree has no
   `wayland-sys/../wayland-backend` sibling; keeping the registry version of the
   dependency is correct and unblocked `wayland-backend`.
3. **Generated files and submodule contents are grafted.** `EXTRA_TREES` copies
   the Wayland protocol XML from `wayland-mirror/wayland-protocols`,
   `swaywm/wlr-protocols` and `KDE/plasma-wayland-protocols`; without it
   `wayland_scanner` panics 108 times per build.
4. **The mirror copies of `raw-window-handle`, `softbuffer` and `wayland-*` were
   torn** (files from two releases in one directory) and are now pinned to git
   tags.
5. **X11 is cut from the build on purpose.** `x11` → `tiny-xlib` → `ctor ^0.10`,
   and `mmastrac/rust-ctor` only tags 0.13+/1.0, so an X11-enabled iced cannot be
   vendored offline. Aurora therefore builds Wayland-only; see
   "Not verified / known gaps".

### 2. What was added

- **`aurora-platform`** (new crate, Linux side of the portability seam):
  `XdgDirectories` (`AppDirectories`: config/data/cache/lyrics/database),
  `Settings` (§27 surface, TOML, atomic save, clamped on load), `logging`
  (`AURORA_LOG`), `session` (Wayland / X11-only / headless + socket probe).
- **`aurora-app`** now has a real boot sequence: args → logging → directories →
  settings → session → window or headless. Modules: `cli`, `startup`, `shell`,
  `headless`, `demo`.
- **iced shell window**: sidebar (Now Playing / Library / Settings) and a live
  settings pane; every control writes `settings.toml` immediately and the status
  line reports success or the exact error.
- **Headless mode is a real mode**: loads settings, scans the configured folders
  into the catalog, prints and logs the summary.
- **`fixtures/`** added (placeholder `.flac` + sidecar `.lrc`) so `--demo` has a
  library to scan; it was missing, which made the Phase 0 demo fail with
  "No such file or directory" on a clean checkout.

### 3. Measured here

```text
$ cargo build --workspace
    Finished `dev` profile ... (iced 0.14, tiny-skia, no wgpu, no X11)

$ cargo clippy --workspace --all-targets -- -D warnings
    (no output: 0 warnings)

$ cargo test --workspace
    16 + 2 + 9 + 1 + 3 + 11 + 22 + 2 = 66 passed, 0 failed
```

```text
$ cargo run -q -p aurora-app -- --demo
Aurora Player 0.1.0 (demo)
library root: .../fixtures
first track: Unknown Artist - Rih il-Bahar [Demo]
lyrics: 4 lines (word-timed: true)
after 1s: active line Some(0), 1 word(s) highlighted, scroll 0.0px
```

```text
$ cargo run -q -p aurora-app -- --headless --config-dir /tmp/aurora-run
 INFO aurora::startup: settings file created from defaults path=/tmp/aurora-run/aurora/settings.toml
 INFO aurora: no window; running headless session="headless"
Aurora Player 0.1.0 (headless mode)
settings: /tmp/aurora-run/aurora/settings.toml
lyrics: enabled=true animation=Standard word-highlighting=true | motion disabled=false
library: 1 folder(s), watch=true rescan-on-start=false
  /home/user/Aurora-/fixtures/music: 1 visited, 1 added, 0 skipped
 INFO aurora::headless: headless boot complete folders=1 tracks=1 catalog=1
```

A stale `WAYLAND_DISPLAY` is handled, not crashed on. iced *panics* when the
compositor connection fails, so boot probes the socket first and falls back:

```text
$ WAYLAND_DISPLAY=wayland-9 cargo run -q -p aurora-app -- --config-dir /tmp/aurora-run
 WARN aurora::startup: WAYLAND_DISPLAY is set but no compositor answers; starting headless
Aurora Player 0.1.0 (headless mode)
```

The settings file written on first run (`/tmp/aurora-run/aurora/settings.toml`)
is the §27 surface in the documented order (playback, lyrics, appearance,
performance, library), kebab-case keys, every field defaulted.

### 4. Headless render evidence

`crates/aurora-app/src/shot.rs` (test-only) drives the shell through iced's real
layout and draw passes into a `tiny_skia` pixmap — the same two passes the window
runs — and writes PNGs to `target/shots/`. Panes committed to
`docs/evidence/`:

| File | What it shows |
| --- | --- |
| `docs/evidence/shell-nowplaying.png` | sidebar + Now Playing pane, accent swatch |
| `docs/evidence/shell-library.png` | Library pane with a configured folder |
| `docs/evidence/shell-settings.png` | every §27 control, toggles showing on/off |

Three tests back this up: each pane renders more than 50 distinct colours (a
blank render fails), rendering is deterministic, and toggling four settings
changes the pixels. A fourth test *characterises an upstream iced bug*:

> `iced_tiny_skia`'s `into_color` calls
> `tiny_skia::Color::from_rgba(color.b, color.g, color.r, alpha)`, so the software
> renderer swaps red and blue. It is present on iced 0.14.0 and on iced master.
> **The screenshots therefore show the blue accent (`#4c9aff`) as orange.**
> The app does not compensate: the wgpu backend does not have the bug, so a
> channel swap in our code would make the real GPU build wrong to make the
> offline one look right. If the characterisation test ever fails, iced fixed it
> and the caveat (and the screenshots) should be refreshed.

This is also why the settings pane's palette looks salmon-ish: iced's default
primary colour is blue.

### Not verified / known gaps

- **No window was ever displayed.** The layout is proven by the headless
  renders above (same layout/draw code as the window), but there is no compositor
  and no DRM device here, so the window itself — winit surface creation, HiDPI
  scaling, input, frame pacing — is unverified. The first real check is
  `cargo run -p aurora-app` on the Fedora machine.
- **Screenshot colours are wrong on purpose**: red and blue are swapped by an
  upstream `iced_tiny_skia` bug (see above). Do not use these images as a colour
  reference for the design work.
- **X11 is not supported by this build** (see above). On an X11-only session the
  app logs a warning and starts headless instead of dying.
- **The accent colour and artwork effects are stored, not rendered.** Phase 1
  wires persistence; theming lands with the lyric view.
- **No SQLite yet.** The catalog is still `MemoryCatalog`; the headless scan is
  discarded on exit. Persistence of the *catalog* is Phase 3 work, settings
  persistence is done.
- Doctests still cannot run (no `rustdoc` in this toolchain).

### Recommended next step

Run `cargo run -p aurora-app` on the Fedora/Wayland machine (window sanity) and
`spikes/ui-spike`'s two candidate commands there (the 60 FPS claim), then Phase 2:
decoding (`symphonia`) and output (`cpal`) behind `AudioOutput`, with the clock
driven by the device counter.
