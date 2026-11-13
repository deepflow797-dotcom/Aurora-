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

  `SOURCE_PATCHES` documents the two source fixes currently needed
  (`cosmic-text` `Blob::new` bound, see the spike README).

Usage:

```text
scripts/vendor/fetch-mirror.sh                 # once per machine / sandbox reset
python3 scripts/vendor/vendor.py .             # the app workspace
python3 scripts/vendor/vendor.py spikes/ui-spike
```

Both commands are idempotent: already-vendored crates are skipped and Cargo is
re-invoked until it either succeeds (`OK after N round(s)`) or hits something the
driver cannot resolve, in which case it prints the unresolved requirement and
the raw Cargo error.

**Caveat, stated plainly:** this scheme exists for a network-isolated sandbox.
It mirrors third-party source into a machine-local directory; it does not alter
crate behaviour beyond the two documented patches, and the patches are listed in
one place so they cannot be forgotten.
