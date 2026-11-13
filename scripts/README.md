# scripts/

Build-support tooling. Nothing here is part of the app.

## `vendor/` — offline dependency vendoring

This repository's development sandbox has no direct access to crates.io. To make
`cargo build` work offline, the crates the workspace needs are mirrored into a
local directory source:

- `~/.cargo/config.toml`:

  ```toml
  [source.crates-io]
  replace-with = "vendored-sources"

  [source.vendored-sources]
  directory = "/home/user/.cargo-vendor"

  [net]
  offline = true
  ```

- `vendor/fetch-mirror.sh` downloads a large public crate mirror
  (`ClickHouse/rust_vendor`, main branch) and extracts it flat into
  `/tmp/rust_vendor-main` — the "mirror" the resolver prefers.
- `vendor/vendor.py <manifest-dir>` is the resolver driver. It runs
  `cargo build`, parses the failure, and keeps satisfying it:

  | Cargo says | The driver does |
  | --- | --- |
  | `no matching package named X` / `failed to select a version for X = "req"` | copies the newest version from the mirror that satisfies `req`; if the mirror has none, fetches the crate's upstream git tree and copies that crate's directory |
  | `locked to X` (stale `Cargo.lock`) | drops the lockfile and retries |
  | `couldn't find file .../tables.rs` | grafts generated files from `EXTRA_FILES` |

  For git sources it discovers tags with `git ls-remote` (no GitHub API rate
  limit), picks the highest tag whose version satisfies the requirement, falls
  back to the default branch, and only then relabels a stale tag to the floor
  version the requirement asks for (logged as `relabelling`).

  Every copied crate is normalised (`vendor.py`): workspace inheritance inlined,
  `[workspace]`/`[lints]`/`dev-dependencies`/cross-platform targets stripped,
  feature lists filtered to dependencies that still exist, and a
  `{"files":{}}` checksum written — which is what Cargo expects for a directory
  source.

  `FORCE_GIT` lists crates that must come from git because the mirror copy mixes
  files from two releases (`cc`, `libc`).

  Two curated lists steer tag choice, because picking the wrong tree inside a
  compatible semver range produces code that does not compile:

  | List | Meaning |
  | --- | --- |
  | `PREFER_HEAD` | the default branch *is* the best source (lyon publishes releases from `main` without tags) |
  | `NO_BRANCH` | the default branch is newer than any release and already breaks API compatibility inside a patch series (`wayland-rs` main tracks unreleased 0.3x, `softbuffer` main is a 0.5-era rewrite) — always take a release tag |

  `path = "../crate"` entries in a dependency table are stripped (they point at
  sibling directories of the upstream repo that do not exist in a flat vendor
  tree); the registry version of the same dependency is kept. Without this,
  `wayland-backend`'s path dependency on `wayland-sys` can never resolve.

  `SOURCE_PATCHES` documents the source fixes currently needed
  (`cosmic-text` `Blob::new` bound, see the spike README).

  `EXTRA_FILES` grafts generated files that are not checked in upstream
  (`unicode-linebreak`'s `tables.rs`). `EXTRA_TREES` grafts submodule contents
  that codeload tarballs do not contain — the Wayland protocol XML in
  `wayland-protocols`, `wayland-protocols-wlr` and `wayland-protocols-plasma`.
  Without those the `wayland_scanner` proc macro panics 108 times at build time.

### Decoder crates (symphonia)

`symphonia` publishes 22 crates in one workspace, so `GIT_SOURCES` maps each crate
name to `pdeljanov/Symphonia@v0.5.5` and `find_crate_dir` picks the matching
sub-directory. Two constraints are baked into the pinned feature list:

- `wav` and `aiff` are **off**: both pull `symphonia-format-riff`, which depends
  on the `extended` crate. No source for `extended` is reachable from here (crate
  repo 404s), so WAV and AIFF are decoded by `aurora-audio`'s own modules
  (`wav.rs`, `aiff.rs`) instead. That is a supported path, not a workaround:
  both formats are linear PCM, so the decoders are small and exactly testable.
- `flacenc` (a pure-Rust FLAC *encoder*, wanted only to generate a test fixture)
  is listed in `GIT_SOURCES` but cannot be resolved: it needs `heapless` →
  `built` → more. The fixture is therefore hand-written by
  `scripts/fixtures/make-flac.py` instead.

### iced 0.14 offline recipe

```toml
iced = { version = "0.14", default-features = false, features = [
  "tiny-skia", "wayland", "advanced-shaping", "crisp",
  "linux-theme-detection", "thread-pool",
] }
```

Verified building and linking in this sandbox (2026-11-13). Notes:

- `tiny-skia` is the software rasteriser the spike measured; leave `wgpu` off
  (no GPU in the sandbox, and it drags in a second renderer).
- `x11` is **deliberately omitted**. It pulls `tiny-xlib` → `ctor ^0.10`, and
  `mmastrac/rust-ctor` only tags 0.13+/1.0, so an X11 build cannot be vendored
  offline today. The app therefore boots through Wayland only; see
  `PHASE_LOG.md` for the follow-up. Add `"x11"` back once a 0.10-compatible
  `ctor` source exists.
- `wayland` is required by `iced_tiny_skia`'s softbuffer surface.

Usage:

```text
scripts/vendor/fetch-mirror.sh                 # once per machine / sandbox reset
python3 scripts/vendor/vendor.py .             # the app workspace
python3 scripts/vendor/vendor.py spikes/ui-spike
python3 scripts/vendor/vendor.py /tmp/iced-probe   # iced 0.14 smoke probe
```

Both commands are idempotent: already-vendored crates are skipped and Cargo is
re-invoked until it either succeeds (`OK after N round(s)`) or hits something the
driver cannot resolve, in which case it prints the unresolved requirement and
the raw Cargo error.

**Caveat, stated plainly:** this scheme exists for a network-isolated sandbox.
It mirrors third-party source into a machine-local directory; it does not alter
crate behaviour beyond the two documented patches, and the patches are listed in
one place so they cannot be forgotten.
