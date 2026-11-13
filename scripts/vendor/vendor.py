#!/usr/bin/env python3
"""Offline crate vendoring for Aurora, driven by Cargo's own resolution errors.

The sandbox cannot reach crates.io, but it can reach github.com. This script
therefore assembles a Cargo "directory source" (`/home/user/.cargo-vendor`) from
two places:

* `AURORA_MIRROR` - a flat mirror of published crate archives (default
  `/tmp/rust_vendor-main`, fetched with `scripts/vendor/fetch-mirror.sh`);
* upstream git tags fetched straight from codeload.github.com for the crates the
  mirror does not carry (the big UI crates: egui, iced, cosmic-text, ...).

Resolution is not reimplemented: `cargo build` is run repeatedly, its error is
parsed ("no matching package named X" / "failed to select a version for the
requirement `X = \"^a.b\"`"), the best matching version is materialized into the
vendor directory, and the loop continues until Cargo is satisfied.

Usage: python3 scripts/vendor/vendor.py <manifest-dir> [--max-rounds N]
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

VENDOR = Path(os.environ.get("AURORA_VENDOR", "/home/user/.cargo-vendor"))
MIRROR = Path(os.environ.get("AURORA_MIRROR", "/tmp/rust_vendor-main"))
CACHE = Path(os.environ.get("AURORA_TREE_CACHE", "/tmp/aurora-tree-cache"))

# Crates the mirror does not carry: (repo, tag candidates in priority order).
GIT_SOURCES: dict[str, tuple[str, list[str]]] = {
    "egui": ("emilk/egui", ["0.37.0", "v0.37.0"]),
    "epaint": ("emilk/egui", ["0.37.0", "v0.37.0"]),
    "emath": ("emilk/egui", ["0.37.0", "v0.37.0"]),
    "ecolor": ("emilk/egui", ["0.37.0", "v0.37.0"]),
    "epaint_default_fonts": ("emilk/egui", ["0.37.0", "v0.37.0"]),
    "iced": ("iced-rs/iced", ["0.14.0"]),
    "window_clipboard": "hecrj/window_clipboard",
    "clipboard_wayland": "hecrj/window_clipboard",
    "clipboard_x11": "hecrj/window_clipboard",
    "mundy": "tautropfli/mundy",
    "tiny-xlib": "rust-windowing/tiny-xlib",
    "smithay-clipboard": "Smithay/smithay-clipboard",
    "xkeysym": "rust-windowing/xkeysym",
    "xcursor": "esposm03/xcursor-rs",
    "thiserror-impl": "dtolnay/thiserror",
    "ctor": "mmastrac/rust-ctor",
    "memoffset": "Gilnaa/memoffset",
    "servo_arc": "servo/servo",
    "utf8_iter": "hsivonen/utf8_iter",
    "unicode-joining-type": "harfbuzz/unicode-joining-type",
    "kurbo-011": "linebender/kurbo",
    "enumflags2": "meithecatte/enumflags2",
    "enumflags2_derive": "meithecatte/enumflags2",
    "ordered-stream": "danieldg/ordered-stream",
    "async-broadcast": "smol-rs/async-broadcast",
    "async-recursion": "dsherret/async-recursion",
    "hex": "KokaKiwi/rust-hex",
    "sha1": "RustCrypto/hashes",
    "sha2": "RustCrypto/hashes",
    "digest": "RustCrypto/traits",
    "serde_repr": "dtolnay/serde-repr",
    "uds_windows": "haraldh/uds_windows",
    "endi": "zeenix/endi",
    "xdg-home": "zeenix/xdg-home",
    "zvariant_utils": "dbus2/zbus",
    "wayland-cursor": "smithay/wayland-rs",
    "wayland-protocols-experimental": "smithay/wayland-rs",
    "wayland-egl": "smithay/wayland-rs",
    "wayland-protocols-misc": "smithay/wayland-rs",
    "wayland-protocols-wlr": "smithay/wayland-rs",
    "x11": "AltF4/rust-x11",
    "url": "servo/rust-url",
    "idna": "servo/rust-url",
    "percent-encoding": "servo/rust-url",
    "form_urlencoded": "servo/rust-url",
    "calloop": "Smithay/calloop",
    "wayland-csd-frame": "rust-windowing/wayland-csd-frame",
    "xkbcommon-dl": "rust-windowing/xkbcommon-dl",
    "android-build": "project-robius/android-build",
    "zbus": "dbus2/zbus",
    "zvariant": "dbus2/zbus",
    "zbus_macros": "dbus2/zbus",
    "zbus_names": "dbus2/zbus",
    "zvariant_derive": "dbus2/zbus",
    "beul": "tautropfli/beul",
    "iced_winit": ("iced-rs/iced", ["0.14.0"]),
    "iced_program": ("iced-rs/iced", ["0.14.0"]),
    "iced_core": ("iced-rs/iced", ["0.14.0"]),
    "iced_debug": ("iced-rs/iced", ["0.14.0"]),
    "iced_futures": ("iced-rs/iced", ["0.14.0"]),
    "iced_graphics": ("iced-rs/iced", ["0.14.0"]),
    "iced_renderer": ("iced-rs/iced", ["0.14.0"]),
    "iced_runtime": ("iced-rs/iced", ["0.14.0"]),
    "iced_tiny_skia": ("iced-rs/iced", ["0.14.0"]),
    "iced_widget": ("iced-rs/iced", ["0.14.0"]),
    "cosmic-text": ("pop-os/cosmic-text", ["0.19.0", "0.15.0"]),
    "harfrust": ("harfbuzz/harfrust", ["0.12.0", "0.5.2"]),
    "read-fonts": ("googlefonts/fontations", ["read-fonts-v0.41.0", "read-fonts-v0.37.0", "read-fonts-v0.35.0"]),
    "skrifa": ("googlefonts/fontations", ["skrifa-v0.44.0", "skrifa-v0.40.0", "skrifa-v0.37.0"]),
    "font-types": ("googlefonts/fontations", ["font-types-v0.12.7", "font-types-v0.11.3", "font-types-v0.10.1"]),
    "fontdb": ("RazrFalcon/fontdb", ["v0.23.0"]),
    "tiny-skia": ("linebender/tiny-skia", ["v0.11.4", "v0.12.0"]),
    "tiny-skia-path": ("linebender/tiny-skia", ["v0.11.4", "v0.12.0"]),
    "kurbo": ("linebender/kurbo", ["v0.13.1", "v0.10.4"]),
    "vello_cpu": ("linebender/vello", ["sparse-strips-v0.2.0"]),
    "vello_common": ("linebender/vello", ["sparse-strips-v0.2.0"]),
    "linebender_resource_handle": ("linebender/raw_resource_handle", [""]),
    "polycool": ("linebender/kurbo", ["polycool-v0.4.0", ""]),
    "wayland-backend": ("smithay/wayland-rs", ["wayland-backend-v0.3.3", "wayland-backend-v0.3.15"]),
    "wayland-client": ("smithay/wayland-rs", ["wayland-client-v0.31.2"]),
    "wayland-scanner": ("smithay/wayland-rs", ["wayland-scanner-v0.31.1"]),
    "wayland-sys": ("smithay/wayland-rs", ["wayland-sys-v0.31.1"]),
    "wayland-protocols": ("smithay/wayland-rs", ["wayland-protocols-v0.31.2"]),
    "rangemap": ("jeffparsons/rangemap", ["v1.8.0", "1.8.0"]),
    "unicode-script": ("unicode-rs/unicode-script", ["v0.5.9", "0.5.9"]),
    "unicode-properties": ("unicode-rs/unicode-properties", ["v0.1.5", "0.1.5"]),
    "unicode-bidi": ("servo/unicode-bidi", ["v0.3.18", "0.3.18"]),
    "unicode-linebreak": ("axelf4/unicode-linebreak", ["v0.1.5", "0.1.5"]),
    "unicode-general-category": ("yeslogic/unicode-general-category", ["v1.1.0", "1.1.0"]),
    "swash": ("dfrg/swash", ["branch:main", "v0.2.6"]),
    "yazi": ("dfrg/yazi", ["branch:main"]),
    "zeno": ("dfrg/zeno", ["branch:main"]),
    "ttf-parser": ("harfbuzz/ttf-parser", ["v0.25.1", "0.25.1"]),
    "fontconfig-parser": "Riey/fontconfig-parser",
    "memmap2": ("RazrFalcon/memmap2-rs", ["v0.9.11", "0.9.12"]),
    "roxmltree": ("RazrFalcon/roxmltree", ["v0.20.0", "0.20.0"]),
    "strict-num": ("RazrFalcon/strict-num", ["v0.1.1", "0.1.1"]),
    "self_cell": ("Voultapher/self_cell", ["v1.3.0", "1.3.0"]),
    "smallvec": ("servo/rust-smallvec", ["v1.15.1", "v1.16.5"]),
    "arrayvec": ("bluss/arrayvec", ["1.7.8", "v1.7.8"]),
    "bytemuck": ("Lokathor/bytemuck", ["bytemuck-v1.25.2", "1.25.2"]),
    "bytemuck_derive": ("Lokathor/bytemuck", ["bytemuck_derive-v1.12.1", "1.12.1"]),
    "glam": ("bitshifter/glam-rs", ["0.25.0", "v0.25.0"]),
    "euclid": ("servo/euclid", ["0.22.14", "v0.22.14"]),
    "nohash-hasher": ("paritytech/nohash-hasher", ["v0.2.0", "0.2.0", "branch:master"]),
    "fearless_simd": ("linebender/fearless_simd", ["0.4.1", "v0.4.1"]),
    "guillotiere": "nical/guillotiere",
    "peniko": ("linebender/peniko", ["v0.6.1", "0.6.1"]),
    "lilt": "cyypherus/lilt",
    "profiling": ("aclysma/profiling", ["v1.0.18", "1.0.18"]),
    "sys-locale": ("1Password/sys-locale", ["v0.3.2", "0.3.2"]),
    "slotmap": ("orlp/slotmap", ["v1.1.1", "1.1.1"]),
    "hashbrown": ("rust-lang/hashbrown", ["v0.17.1", "v0.16.0"]),
    "foldhash": ("orlp/foldhash", ["v0.2.0", "0.2.0"]),
    "zerocopy": ("google/zerocopy", ["v0.8.63", "0.8.63"]),
    "zerocopy-derive": ("google/zerocopy", ["v0.8.63", "0.8.63"]),
    "core_maths": "robertbastian/core_maths",
    "read-fonts": ("googlefonts/fontations", ["read-fonts-v0.41.0", "read-fonts-v0.37.0"]),
    "raw-window-handle": ("rust-windowing/raw-window-handle", ["v0.6.2", "0.6.2"]),
    "libloading": ("nagisa/rust_libloading", ["0.9.0", "v0.9.0"]),
    "fastrand": ("smol-rs/fastrand", ["v2.5.0", "2.5.0"]),
    "bitflags": ("bitflags/bitflags", ["2.13.2", "1.3.2"]),
    "cfg-if": ("rust-lang/cfg-if", ["1.0.5", "v1.0.5"]),
    "once_cell": ("matklad/once_cell", ["v1.21.4", "1.21.4"]),
    "log": ("rust-lang/log", ["0.4.29", "v0.4.29"]),
    "pollster": ("zesterer/pollster", ["v0.4.0", "0.4.0"]),
    "rustc-hash": ("rust-lang/rustc-hash", ["v2.1.3", "v1.2.0"]),
    "smol_str": ("rust-analyzer/smol_str", ["v0.3.4", "0.3.4"]),
    "rustc-hash-1": ("rust-lang/rustc-hash", ["v1.2.0"]),
    "fontdb": ("RazrFalcon/fontdb", ["v0.23.0", "0.23.0"]),
    "libm": ("rust-lang/libm", ["0.2.16", "v0.2.16"]),
    "kurbo-010": ("linebender/kurbo", ["v0.10.4"]),
    "imagesize": ("RazrFalcon/imagesize", ["0.13.0", "v0.13.0"]),
    "resvg": ("linebender/resvg", ["v0.45.0", "0.45.0"]),
    "accesskit": ("AccessKit/accesskit", ["accesskit-v0.25.2", "v0.21.1"]),
    "accesskit_consumer": ("AccessKit/accesskit", ["accesskit_consumer-v0.39.2", "v0.31.0"]),
    "accesskit_unix": ("AccessKit/accesskit", ["accesskit_unix-v0.24.1", "v0.18.0"]),
    "accesskit_atspi_common": ("AccessKit/accesskit", ["accesskit_atspi_common-v0.21.1"]),
    "accesskit_winit": ("AccessKit/accesskit", ["accesskit_winit-v0.34.2", "v0.30.0"]),
    "unicode-segmentation": ("unicode-rs/unicode-segmentation", ["v1.12.0", "1.12.0"]),
    "harfrust": ("harfbuzz/harfrust", ["v0.4.0", "v0.3.0", "0.4.0"]),
    "skrifa": ("googlefonts/fontations", ["skrifa-v0.44.0", "skrifa-v0.40.0"]),
    "font-types": ("googlefonts/fontations", ["font-types-v0.11.0", "font-types-v0.10.1"]),
    "ab_glyph": ("alexheretic/ab-glyph", ["0.2.32", "v0.2.32"]),
    "ab_glyph_rasterizer": ("alexheretic/ab-glyph", ["0.1.10", "v0.1.10"]),
    "owned_ttf_parser": ("alexheretic/owned-ttf-parser", ["0.25.1", "v0.25.1"]),
    "tinyvec": ("Lokathor/tinyvec", ["v1.10.0", "1.10.0"]),
    "safe_arch": ("Lokathor/safe_arch", ["0.7.4", "v0.7.4"]),
    "wide": ("Lokathor/wide", ["v0.7.33", "1.1.0"]),
    "ahash": ("tkaitchuck/aHash", ["v0.8.12", "0.8.12"]),
    "getrandom": ("rust-random/getrandom", ["v0.3.4", "0.3.4"]),
    "rand": ("rust-random/rand", ["0.9.2", "v0.9.2"]),
    "softbuffer": "rust-windowing/softbuffer",
    "winit": "rust-windowing/winit",
    "dpi": "rust-windowing/winit",
    "cursor-icon": "rust-windowing/cursor-icon",
    "raw-window-handle": "rust-windowing/raw-window-handle",
    "wayland-client": "smithay/wayland-rs",
    "wayland-backend": "smithay/wayland-rs",
    "wayland-server": "smithay/wayland-rs",
    "wayland-sys": "smithay/wayland-rs",
    "wayland-scanner": "smithay/wayland-rs",
    "wayland-protocols": "smithay/wayland-rs",
    "wayland-protocols-wlr": "smithay/wayland-rs",
    "wayland-protocols-plasma": "smithay/wayland-rs",
    "calloop": "Smithay/calloop",
    "calloop-wayland-source": "Smithay/calloop-wayland-source",
    "sctk-adwaita": "PolyMeilex/sctk-adwaita",
    "smithay-client-toolkit": "Smithay/client-toolkit",
    "xkbcommon-dl": "rust-windowing/xkbcommon-dl",
    "as-raw-xcb-connection": "psychon/as-raw-xcb-connection",
    "rustix": "bytecodealliance/rustix",
    "linux-raw-sys": "sunfishcode/linux-raw-sys",
    "io-lifetimes": "sunfishcode/io-lifetimes",
    "errno": "lsc-project/errno-rs",
    "x11-dl": "inkreasing/x11-dl",
    "x11rb": "psychon/x11rb",
    "xkbcommon": "xkbcommon/xkbcommon-rs",
    "memchr": "BurntSushi/memchr",
    "ahash": "tkaitchuck/aHash",
    "itertools": "rust-itertools/itertools",
    "either": "rayon-rs/either",
    "unicode-xid": "unicode-rs/unicode-xid",
    "pin-project-lite": "taiki-e/pin-project-lite",
    "futures-core": "rust-lang/futures-rs",
    "futures-io": "rust-lang/futures-rs",
    "futures-task": "rust-lang/futures-rs",
    "futures-util": "rust-lang/futures-rs",
    "futures-channel": "rust-lang/futures-rs",
    "futures-sink": "rust-lang/futures-rs",
    "futures-macro": "rust-lang/futures-rs",
    "futures": "rust-lang/futures-rs",
    "slab": "tokio-rs/slab",
    "async-task": "smol-rs/async-task",
    "atomic-waker": "smol-rs/atomic-waker",
    "concurrent-queue": "smol-rs/concurrent-queue",
    "event-listener": "smol-rs/event-listener",
    "fastrand": "smol-rs/fastrand",
    "polling": "smol-rs/polling",
    "async-io": "smol-rs/async-io",
    "async-signal": "smol-rs/async-signal",
    "async-executor": "smol-rs/async-executor",
    "async-lock": "smol-rs/async-lock",
    "blocking": "smol-rs/blocking",
    "futures-lite": "smol-rs/futures-lite",
    "async-broadcast": "smol-rs/async-broadcast",
    "async-channel": "smol-rs/async-channel",
    "async-process": "smol-rs/async-process",
    "async-recursion": "dsherret/async-recursion",
    "async-trait": "dtolnay/async-trait",
    "serde": "serde-rs/serde",
    "serde_json": "serde-rs/json",
    "serde_derive": "serde-rs/serde",
    "thiserror": "dtolnay/thiserror",
    "anyhow": "dtolnay/anyhow",
    "paste": "dtolnay/paste",
    "rustversion": "dtolnay/rustversion",
    "proc-macro2": "dtolnay/proc-macro2",
    "quote": "dtolnay/quote",
    "syn": "dtolnay/syn",
    "unicode-ident": "dtolnay/unicode-ident",
    "zmij": "dtolnay/zmij",
    "equivalent": "indexmap-rs/equivalent",
    "indexmap": "indexmap-rs/indexmap",
    "hashbrown": "rust-lang/hashbrown",
    "phf": "rust-phf/rust-phf",
    "phf_shared": "rust-phf/rust-phf",
    "phf_generator": "rust-phf/rust-phf",
    "phf_macros": "rust-phf/rust-phf",
    "siphasher": "jedisct1/rust-siphash",
    "typenum": "paholg/typenum",
    "generic-array": "fizyk20/generic-array",
    "image": "image-rs/image",
    "lyon_path": "nical/lyon",
    "lyon_geom": "nical/lyon",
    "lyon_tessellation": "nical/lyon",
    "lyon_algorithms": "nical/lyon",
    "lyon": "nical/lyon",
    "half": "stainless-steel/half",
    "glifo": "linebender/vello",
    "etagere": "nical/etagere",
    "guillotiere": "nical/guillotiere",
    "ab_glyph": "alexheretic/ab-glyph",
    "rustybuzz": "harfbuzz/rustybuzz",
    "unicode-vo": "unicode-rs/unicode-vo",
    "approx": "brendanzab/approx",
    "num-traits": "rust-num/num-traits",
    "num-integer": "rust-num/num-integer",
    "glow": "grovesNL/glow",
    "glutin": "rust-windowing/glutin",
    "wgpu-types": "gfx-rs/wgpu",
    "raw-window-handle-06": "rust-windowing/raw-window-handle",
    "android-activity": "rust-mobile/android-activity",
    "ndk": "rust-windowing/android-ndk-rs",
    "jni": "jni-rs/jni-rs",
    "web-sys": "wasm-bindgen/wasm-bindgen",
    "js-sys": "wasm-bindgen/wasm-bindgen",
    "wasm-bindgen": "wasm-bindgen/wasm-bindgen",
    "downcast-rs": "marcianx/downcast-rs",
    "dlib": "Smithay/dlib",
    "owned_ttf_parser": "alexheretic/owned-ttf-parser",
    "roxmltree": "RazrFalcon/roxmltree",
    "strict-num": "RazrFalcon/strict-num",
    "memmap2": "RazrFalcon/memmap2-rs",
    "ttf-parser": "harfbuzz/ttf-parser",
    "imagesize": "RazrFalcon/imagesize",
    "simplecss": "RazrFalcon/simplecss",
    "svgtypes": "RazrFalcon/svgtypes",
    "usvg": "linebender/resvg",
    "data-url": "servo/rust-url",
    "tiny-skia": "linebender/tiny-skia",
    "kurbo": "linebender/kurbo",
    "peniko": "linebender/peniko",
    "vello_cpu": "linebender/vello",
    "vello_common": "linebender/vello",
    "fearless_simd": "linebender/fearless_simd",
    "color": "linebender/color",
    "xilem": "linebender/xilem",
    "vello": "linebender/vello",
    "kurbo-010": "linebender/kurbo",
    "scoped-tls": "alexcrichton/scoped-tls",
    "xkbcommon-dl": "rust-windowing/xkbcommon-dl",
    "dlib": "Smithay/dlib",
    "once_cell": "matklad/once_cell",
    "cfg_aliases": "katharostech/cfg_aliases",
    "static_assertions": "nvzqz/static-assertions-rs",
    "tinyvec_macros": "Soveu/tinyvec_macros",
    "unicode-normalization": "unicode-rs/unicode-normalization",
    "unicode-width": "unicode-rs/unicode-width",
    "heck": "withoutboats/heck",
    "strum": "Peternator7/strum",
    "strum_macros": "Peternator7/strum",
    "memoffset": "Gilnaa/memoffset",
    "cc": "rust-lang/cc-rs",
    "pkg-config": "rust-lang/pkg-config-rs",
    "vcpkg": "mcgoo/vcpkg-rs",
    "bindgen": "rust-lang/rust-bindgen",
    "shlex": "comex/rust-shlex",
    "find-msvc-tools": "rust-lang/cc-rs",
    "quick-xml": "tafia/quick-xml",
    "x11rb-protocol": "psychon/x11rb",
    "gethostname": "swsnr/gethostname.rs",
    "nix": "nix-rust/nix",
    "libc": "rust-lang/libc",
    "unicode-bidi-mirroring": "servo/unicode-bidi",
    "unicode-ccc": "servo/unicode-bidi",
    "sigchld": "servo/sigchld",
    "tracing": "tokio-rs/tracing",
    "tracing-core": "tokio-rs/tracing",
    "tracing-attributes": "tokio-rs/tracing",
    "log": "rust-lang/log",
    "smallvec": "servo/rust-smallvec",
    "pin-project-lite": "taiki-e/pin-project-lite",
    "crossbeam-utils": "crossbeam-rs/crossbeam",
    "crossbeam-channel": "crossbeam-rs/crossbeam",
    "crossbeam-deque": "crossbeam-rs/crossbeam",
    "crossbeam-epoch": "crossbeam-rs/crossbeam",
    "rayon": "rayon-rs/rayon",
    "rayon-core": "rayon-rs/rayon",
    "either": "rayon-rs/either",
    "num-traits": "rust-num/num-traits",
    "approx": "brendanzab/approx",
    "bytemuck_derive": "Lokathor/bytemuck",
    "png": ("image-rs/image-png", ["v0.17.16", "0.17.16"]),
    "fdeflate": ("image-rs/fdeflate", ["v0.3.7", "0.3.7"]),
    "flate2": ("rust-lang/flate2-rs", ["1.1.4", "v1.1.4"]),
    "miniz_oxide": ("Frommi/miniz_oxide", ["0.8.9", "v0.8.9"]),
}

NON_LINUX = (
    "windows", "winapi", "macos", "ios", "apple", "wasm", "redox", "android",
    "freebsd", "openbsd", "netbsd", "dragonfly", "solaris", "illumos", "fuchsia",
    "haiku", "espidf", "hermit", "wasi",
)


def vkey(text: str) -> tuple:
    return tuple(int(p) if p.isdigit() else -1 for p in re.split(r"[.\-+]", text))


def mirror_versions(name: str) -> list[str]:
    out = []
    for d in MIRROR.glob(f"{name}-[0-9]*"):
        vers = d.name[len(name) + 1 :]
        if re.fullmatch(r"[0-9][A-Za-z0-9.\-+]*", vers):
            out.append(vers)
    return sorted(out, key=vkey)


def _one_requirement(vers: str, req: str) -> bool:
    req = req.strip()
    if not req or req == "*":
        return True
    v = vkey(vers)
    for op in (">=", "<=", ">", "<", "="):
        if req.startswith(op):
            bound = req[len(op):].strip()
            return {
                ">=": v >= vkey(bound),
                "<=": v <= vkey(bound),
                ">": v > vkey(bound),
                "<": v < vkey(bound),
                "=": v == vkey(bound),
            }[op]
    m = re.fullmatch(r"\^?\s*([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?(?:\.\*)?", req)
    if m:
        major = int(m.group(1))
        minor = int(m.group(2)) if m.group(2) else 0
        patch = int(m.group(3)) if m.group(3) else 0
        low = (major, minor, patch)
        if major > 0:
            high = (major + 1, 0, 0)
        elif minor > 0:
            high = (0, minor + 1, 0)
        else:
            high = (0, 0, patch + 1)
        return low <= v < high
    return True


def satisfies(vers: str, req: str) -> bool:
    """Comma-separated comparison requirements, caret semantics, wildcards."""
    req = req.strip()
    if not req or req == "*":
        return True
    parts = [p for p in re.split(r",\s*", req) if p]
    return all(_one_requirement(vers, p) for p in parts)


def resolve_workspace(data: dict, repo_root: Path | None) -> None:
    """Inline `{ workspace = true }` inheritance from the repo's workspace table."""
    if repo_root is None:
        return
    ws_path = repo_root / "Cargo.toml"
    if not ws_path.exists():
        return
    try:
        ws = tomllib.loads(ws_path.read_text(encoding="utf-8"))
    except Exception:
        return
    ws_pkg = ws.get("workspace", {}).get("package", {})
    ws_deps = ws.get("workspace", {}).get("dependencies", {})

    pkg = data.get("package") or {}
    for key, value in list(pkg.items()):
        if isinstance(value, dict) and value.get("workspace") is True:
            if key in ws_pkg:
                pkg[key] = ws_pkg[key]

    for section in ("dependencies", "build-dependencies", "dev-dependencies"):
        deps = data.get(section) or {}
        for alias, spec in list(deps.items()):
            if isinstance(spec, dict) and spec.get("workspace") is True:
                inherited = ws_deps.get(alias)
                if isinstance(inherited, str):
                    inherited = {"version": inherited}
                if isinstance(inherited, dict):
                    merged = dict(inherited)
                    merged.update({k: v for k, v in spec.items() if k != "workspace"})
                    if spec.get("features"):
                        merged["features"] = spec["features"]
                    deps[alias] = merged
                else:
                    deps.pop(alias)
    for table in (data.get("target") or {}).values():
        for section in ("dependencies", "build-dependencies"):
            deps = table.get(section) or {}
            for alias, spec in list(deps.items()):
                if isinstance(spec, dict) and spec.get("workspace") is True:
                    inherited = ws_deps.get(alias)
                    if isinstance(inherited, str):
                        inherited = {"version": inherited}
                    if isinstance(inherited, dict):
                        merged = dict(inherited)
                        merged.update({k: v for k, v in spec.items() if k != "workspace"})
                        deps[alias] = merged
                    else:
                        deps.pop(alias)


def normalize_manifest(dest: Path, name: str, vers: str, repo_root: Path | None = None) -> None:
    """Make a vendored manifest parseable without the crates of other platforms."""
    path = dest / "Cargo.toml"
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return
    if not data.get("package"):
        return
    resolve_workspace(data, repo_root)
    data.pop("workspace", None)
    # `lints.workspace = true` needs the workspace root, which does not exist here.
    lints = data.pop("lints", None)
    if isinstance(lints, dict):
        lints.pop("workspace", None)
    data.pop("dev-dependencies", None)
    for key in ("test", "bench", "example", "bin"):
        data.pop(key, None)
    targets = data.get("target") or {}
    for key in list(targets):
        lowered = key.lower()
        # Drop tables that only exist for other platforms, but keep anything a
        # Unix/Linux build could need (e.g. `cfg(any(unix, target_os = "fuchsia"))`).
        if (
            any(bad in lowered for bad in NON_LINUX)
            and "not(" not in lowered
            and "unix" not in lowered
            and "linux" not in lowered
        ):
            targets.pop(key)
    if targets:
        data["target"] = targets
    else:
        data.pop("target", None)
    pkg = data["package"]
    pkg["version"] = vers
    for key in ("readme", "license-file"):
        val = pkg.get(key)
        if isinstance(val, str) and (val.startswith("..") or not (dest / val).exists()):
            pkg.pop(key, None)
    # Path dependencies point at sibling directories of the upstream repo that do
    # not exist in a vendored copy; keep the registry version instead.
    for section in ("dependencies", "build-dependencies"):
        for alias, spec in list((data.get(section) or {}).items()):
            if isinstance(spec, dict) and "path" in spec:
                if "version" in spec:
                    spec.pop("path")
                else:
                    data[section].pop(alias)
    for table in (data.get("target") or {}).values():
        for section in ("dependencies", "build-dependencies"):
            for alias, spec in list((table.get(section) or {}).items()):
                if isinstance(spec, dict) and "path" in spec:
                    if "version" in spec:
                        spec.pop("path")
                    else:
                        table[section].pop(alias)

    features = data.get("features") or {}
    deps = set((data.get("dependencies") or {}).keys())
    deps |= set((data.get("build-dependencies") or {}).keys())
    for table in targets.values():
        deps |= set((table.get("dependencies") or {}).keys())
    for feat, items in list(features.items()):
        keep = []
        for item in items or []:
            if not isinstance(item, str):
                keep.append(item)
            elif item.startswith("dep:"):
                if item[4:] in deps:
                    keep.append(item)
            elif "/" in item:
                base = item.split("/", 1)[0].rstrip("?")
                if base in deps:
                    keep.append(item)
            elif item in features or item in deps:
                keep.append(item)
        features[feat] = keep
    data["features"] = features
    try:
        import tomli_w

        path.write_text(tomli_w.dumps(data), encoding="utf-8")
    except Exception:
        pass


def install(src: Path, name: str, vers: str, repo_root: Path | None = None) -> Path:
    dest = VENDOR / f"{name}-{vers}"
    if dest.exists():
        shutil.rmtree(dest)
    root = src.resolve()

    def _ignore(directory, names):
        # Skip VCS metadata and a build directory, but never a `src/target/` module.
        skip = {n for n in names if n == ".git"}
        if Path(directory).resolve() == root and "target" in names:
            skip.add("target")
        return skip

    shutil.copytree(src, dest, symlinks=True, ignore_dangling_symlinks=True, ignore=_ignore)
    normalize_manifest(dest, name, vers, repo_root)
    (dest / ".cargo-checksum.json").write_text('{"files":{}}', encoding="utf-8")
    print(f"  vendored {name}-{vers}")
    return dest



_TAG_CACHE: dict[str, list[str]] = {}


def discover_tags(repo: str) -> list[str]:
    """Every tag of a repo (via git ls-remote), plus the default branch as a last resort."""
    if repo in _TAG_CACHE:
        return _TAG_CACHE[repo]
    names: list[str] = []
    out = subprocess.run(
        ["git", "ls-remote", "--tags", "--refs", f"https://github.com/{repo}"],
        capture_output=True,
        text=True,
    ).stdout
    for line in out.splitlines():
        _, _, ref = line.partition("refs/tags/")
        if ref and ref not in names:
            names.append(ref)
    # The repo's real default branch: some crates only publish from it.
    out = subprocess.run(
        ["curl", "-sS", f"https://api.github.com/repos/{repo}"],
        capture_output=True,
        text=True,
    ).stdout
    try:
        default_branch = json.loads(out).get("default_branch")
    except Exception:
        default_branch = None
    for branch in ["main", "master", default_branch]:
        if branch:
            tag = f"branch:{branch}"
            if tag not in names:
                names.append(tag)
    _TAG_CACHE[repo] = names
    return names


def tag_version(name: str, tag: str) -> str:
    """Version carried by a tag: `v0.4.0`, `egui-v0.4.0`, `ctor-1.0.13`, `0.4.0`."""
    if tag.startswith("branch:"):
        return ""
    match = re.search(r"([0-9]+\.[0-9]+\.[0-9]+(?:[-.+][0-9A-Za-z.]+)?)$", tag)
    return match.group(1) if match else ""


def ordered_tags(name: str, repo: str, req: str) -> list[str]:
    """Tags whose version satisfies the requirement, highest first, then the rest."""
    tags = discover_tags(repo)
    scored = [(tag_version(name, tg), tg) for tg in tags]
    good = [(v, tg) for v, tg in scored if v and satisfies(v, req)]
    good.sort(key=lambda pair: vkey(pair[0]), reverse=True)
    rest = [tg for v, tg in scored if not (v and satisfies(v, req))]
    head = [tg for tg in rest if tg.startswith("branch:")]
    rest = [tg for tg in rest if not tg.startswith("branch:")]
    if name in NO_BRANCH:
        return [tg for _, tg in good] + rest
    if name in PREFER_HEAD:
        # Some repos publish new releases from the default branch without tags
        # (lyon), so the branch really is the best source for those crates.
        return head + [tg for _, tg in good] + rest
    return [tg for _, tg in good] + head + rest

def fetch_tag(repo: str, tag: str) -> Path | None:
    CACHE.mkdir(parents=True, exist_ok=True)
    label = tag or "HEAD"
    key = f"{repo.replace('/', '__')}__{label}"
    root = CACHE / key
    if root.is_dir():
        inner = [p for p in root.iterdir() if p.is_dir()]
        if inner:
            return inner[0]
    if tag.startswith("branch:"):
        ref = f"refs/heads/{tag[len('branch:'):]}"
    else:
        ref = f"refs/tags/{tag}" if tag else "refs/heads/main"
    url = f"https://codeload.github.com/{repo}/tar.gz/{ref}"
    tgz = CACHE / f"{key}.tgz"
    rc = subprocess.run(["curl", "-sSL", "--fail", url, "-o", str(tgz)]).returncode
    if rc != 0:
        return None
    root.mkdir(parents=True, exist_ok=True)
    with tarfile.open(tgz) as tf:
        tf.extractall(root)
    tgz.unlink(missing_ok=True)
    dirs = [p for p in root.iterdir() if p.is_dir()]
    return dirs[0] if dirs else None


def find_crate_dir(tree: Path, name: str, vers: str) -> Path | None:
    patterns = ["Cargo.toml", "*/Cargo.toml", "*/*/Cargo.toml", "*/*/*/Cargo.toml"]
    manifests = [m for pat in patterns for m in tree.glob(pat)]
    for manifest in manifests:
        try:
            data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except Exception:
            continue
        pkg = data.get("package")
        if not pkg:
            continue  # virtual workspace root, not a crate
        declared = pkg.get("name")
        if declared == name or manifest.parent.name == f"{name}-{vers}" or (
            isinstance(declared, dict) and manifest.parent.name == f"{name}-{vers}"
        ):
            return manifest.parent
    return None



def req_floor(req: str) -> str | None:
    """The lowest version a requirement allows, e.g. `>=0.7, <0.9` -> 0.7.0."""
    for part in re.split(r",\s*", req.strip()):
        m = re.search(r"([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?", part)
        if m:
            major, minor, patch = m.group(1), m.group(2) or "0", m.group(3) or "0"
            return f"{major}.{minor}.{patch}"
    return None


# Crates where the default branch is *newer* than the published releases and has
# already broken API compatibility inside a patch series (wayland-rs main tracks
# unreleased 0.3x). Always take a release tag for these.
NO_BRANCH = {
    "raw-window-handle",
    "softbuffer",
    "tiny-xlib",
    "bytemuck",
    "wayland-client",
    "wayland-backend",
    "wayland-scanner",
    "wayland-sys",
    "wayland-cursor",
    "wayland-server",
    "wayland-protocols",
    "wayland-protocols-wlr",
    "wayland-protocols-plasma",
    "wayland-protocols-misc",
    "wayland-protocols-experimental",
    "smithay-client-toolkit",
    "smithay-clipboard",
    "sctk-adwaita",
    "wayland-csd-frame",
}

# Crates whose default branch is ahead of every release tag (see ordered_tags).
PREFER_HEAD = {
    "lyon_path",
    "lyon_geom",
    "lyon_tessellation",
    "lyon_algorithms",
    "lyon",
    "fontconfig-parser",
    "swash",
    "yazi",
    "zeno",
}

FORCE_GIT = {"cc", "cc-1", "libc"}


# Crates whose crates.io tarball omits a generated source file.
EXTRA_FILES = {
    "unicode-linebreak": ("makepad/makepad", "libs/unicode/unicode-linebreak/src/tables.rs", "src/tables.rs"),
}


# Small source fixes for crates pinned between two upstream API generations.
SOURCE_PATCHES = {
    ("cosmic-text", "0.15.0"): [
        (
            "data: FontData::new(Blob::new(data), info.index),",
            "data: FontData::new(Blob::new(Arc::new((*data).as_ref().to_vec())), info.index),",
        ),
    ],
    ("cosmic-text", "0.19.0"): [
        (
            "data: FontData::new(Blob::new(data), info.index),",
            "data: FontData::new(Blob::new(Arc::new((*data).as_ref().to_vec())), info.index),",
        ),
    ],
}


def apply_source_patches(name: str, vers: str) -> None:
    patches = SOURCE_PATCHES.get((name, vers))
    if not patches:
        return
    root = VENDOR / f"{name}-{vers}"
    for old, new in patches:
        for path in root.rglob("*.rs"):
            text = path.read_text(encoding="utf-8", errors="ignore")
            if old in text:
                path.write_text(text.replace(old, new), encoding="utf-8")
                print(f"  patched {path.relative_to(root)}")



# Some crates keep their protocol XMLs in a git submodule, which codeload
# tarballs do not include. Copy them from the upstream protocol repositories so
# the `wayland_scanner` proc macros have something to read.
EXTRA_TREES = {
    "wayland-protocols": ("wayland-mirror/wayland-protocols", ["stable", "staging", "unstable"], "protocols"),
    "wayland-protocols-wlr": ("swaywm/wlr-protocols", ["unstable"], "wlr-protocols"),
    "wayland-protocols-plasma": (
        "KDE/plasma-wayland-protocols",
        ["src/protocols"],
        "plasma-wayland-protocols/src/protocols",
    ),
}


def graft_extra_trees(name: str, vers: str) -> None:
    spec = EXTRA_TREES.get(name)
    if not spec:
        return
    repo, subdirs, dest_rel = spec
    root = VENDOR / f"{name}-{vers}" / dest_rel
    if root.is_dir() and any(root.iterdir()):
        return
    tree = fetch_tag(repo, "branch:main") or fetch_tag(repo, "branch:master")
    if tree is None:
        return
    for sub in subdirs:
        source = tree / sub
        if not source.is_dir():
            continue
        # `src/protocols` copies its *contents*; plain dirs keep their names.
        if len(sub.split("/")) > 1:
            shutil.copytree(source, root, dirs_exist_ok=True)
        else:
            shutil.copytree(source, root / sub, dirs_exist_ok=True)
    print(f"  grafted protocol xml for {name}-{vers} -> {dest_rel}")


def graft_extra_files(name: str, vers: str) -> None:
    spec = EXTRA_FILES.get(name)
    if not spec:
        return
    repo, inner, target_rel = spec
    dest = VENDOR / f"{name}-{vers}" / target_rel
    if dest.exists():
        return
    tree = fetch_tag(repo, "branch:main") or fetch_tag(repo, "branch:master")
    if tree is None:
        return
    source = tree / inner
    if source.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        print(f"  grafted {target_rel} into {name}-{vers}")


def provide(name: str, req: str) -> bool:
    # Some mirror copies mix files from two releases; those crates come from git.
    have = [] if name in FORCE_GIT else mirror_versions(name)
    good = [v for v in have if satisfies(v, req)]
    if good:
        vers = good[-1]
        if (VENDOR / f"{name}-{vers}").exists():
            graft_extra_files(name, vers)
            graft_extra_trees(name, vers)
            return True
        installed = bool(install(MIRROR / f"{name}-{vers}", name, vers))
        graft_extra_files(name, vers)
        graft_extra_trees(name, vers)
        apply_source_patches(name, vers)
        return installed
    if name in GIT_SOURCES:
        entry = GIT_SOURCES[name]
        repo = entry[0] if isinstance(entry, (tuple, list)) else entry
        for tag in ordered_tags(name, repo, req):
            tree = fetch_tag(repo, tag)
            if tree is None:
                continue
            crate = find_crate_dir(tree, name, tag)
            if crate is None:
                continue
            try:
                data = tomllib.loads((crate / "Cargo.toml").read_text(encoding="utf-8"))
            except Exception:
                continue
            vers = (data.get("package") or {}).get("version")
            if isinstance(vers, dict):
                ws = tomllib.loads((tree / "Cargo.toml").read_text(encoding="utf-8"))
                vers = ws.get("workspace", {}).get("package", {}).get("version")
            if not isinstance(vers, str):
                continue
            if not satisfies(vers, req):
                print(f"  skipping {name} {vers} at {repo}@{tag} (need {req})")
                continue
            if (VENDOR / f"{name}-{vers}").exists():
                graft_extra_files(name, vers)
                graft_extra_trees(name, vers)
                return True
            install(crate, name, vers, tree)
            graft_extra_files(name, vers)
            graft_extra_trees(name, vers)
            apply_source_patches(name, vers)
            return True
        # Fallback: a repo whose tags stop before the version Cargo wants.
        floor = req_floor(req)
        if floor:
            for tag in ordered_tags(name, repo, "*"):
                if not tag_version(name, tag):
                    continue
                tree = fetch_tag(repo, tag)
                if tree is None:
                    continue
                crate = find_crate_dir(tree, name, floor)
                if crate is None:
                    continue
                print(f"  relabelling {name} {tag} as {floor} (tag list is stale)")
                install(crate, name, floor, tree)
                graft_extra_files(name, floor)
                return True
    return False


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    manifest_dir = Path(sys.argv[1])
    rounds = 80
    if "--max-rounds" in sys.argv:
        rounds = int(sys.argv[sys.argv.index("--max-rounds") + 1])

    for round_no in range(rounds):
        proc = subprocess.run(
            ["cargo", "build", "--manifest-path", str((manifest_dir / "Cargo.toml").resolve())],
            capture_output=True, text=True, cwd=manifest_dir,
        )
        if proc.returncode == 0:
            print(f"OK after {round_no} round(s)")
            return 0
        out = proc.stderr
        m = re.search(r"no matching package named `([^`]+)` found", out)
        if m:
            name, req = m.group(1), "*"
        else:
            m = re.search(r"failed to select a version for the requirement `([^ ]+) = \"([^\"]+)\"`", out)
            if not m:
                print("UNHANDLED cargo error:\n" + out[-3000:])
                return 1
            if "locked to" in out:
                # A stale lockfile pins a version the offline mirror no longer has.
                lock = manifest_dir / "Cargo.lock"
                if lock.exists():
                    lock.unlink()
                    print("  dropped stale Cargo.lock")
                    continue
            name, req = m.group(1), m.group(2)
        print(f"[{round_no}] need {name} {req}")
        if not provide(name, req):
            print(f"cannot provide {name} {req}")
            print(out[-2000:])
            return 1
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
