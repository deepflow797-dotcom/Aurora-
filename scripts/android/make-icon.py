#!/usr/bin/env python3
"""Draws the Android launcher icon.

No image library is available here (no Pillow), so this writes PNGs directly:
zlib plus a filter byte per row. The mark is drawn from signed-distance functions
and supersampled 3x, which keeps the edges clean at every density.

The shape is the app's own palette: a dark rounded tile with three bars of the
accent gradient the Compose theme uses (#7FB2FF -> #B49BFF) - a player, not a
generic blob, and readable at 48 px.

Regenerate with:
    python3 scripts/android/make-icon.py
"""

import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RES = ROOT / "android/app/src/main/res"
DENSITIES = {
    "mdpi": 48,
    "hdpi": 72,
    "xhdpi": 96,
    "xxhdpi": 144,
    "xxxhdpi": 192,
}
SUPERSAMPLE = 3

BACKDROP_TOP = (0x12, 0x14, 0x1A)
BACKDROP_BOTTOM = (0x1C, 0x20, 0x2B)
BAR_START = (0x7F, 0xB2, 0xFF)
BAR_END = (0xB4, 0x9B, 0xFF)
BARS = (
    # (centre x, half width, half height) in unit coordinates.
    (0.315, 0.052, 0.150),
    (0.500, 0.052, 0.255),
    (0.685, 0.052, 0.115),
)


def rounded_box_distance(px, py, cx, cy, hw, hh, radius):
    """Signed distance to a rounded box centred at (cx, cy)."""
    qx = abs(px - cx) - (hw - radius)
    qy = abs(py - cy) - (hh - radius)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    inside = min(max(qx, qy), 0.0)
    return outside + inside - radius


def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def shade(x, y):
    """Colour of one supersampled sample, in unit coordinates."""
    # Backdrop: rounded tile, vertical gradient.
    if rounded_box_distance(x, y, 0.5, 0.5, 0.5, 0.5, 0.22) > 0.0:
        return None
    colour = lerp(BACKDROP_TOP, BACKDROP_BOTTOM, y)

    for index, (cx, hw, hh) in enumerate(BARS):
        if rounded_box_distance(x, y, cx, 0.5, hw, hh, hw) <= 0.0:
            tint = lerp(BAR_START, BAR_END, index / (len(BARS) - 1))
            colour = tint
    return colour


def render(size):
    scale = size * SUPERSAMPLE
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            r = g = b = 0.0
            covered = 0
            for sy in range(SUPERSAMPLE):
                for sx in range(SUPERSAMPLE):
                    x = (px * SUPERSAMPLE + sx + 0.5) / scale
                    y = (py * SUPERSAMPLE + sy + 0.5) / scale
                    sample = shade(x, y)
                    if sample is None:
                        continue
                    r += sample[0]
                    g += sample[1]
                    b += sample[2]
                    covered += 1
            if covered == 0:
                row += b"\x00\x00\x00\x00"
            else:
                total = SUPERSAMPLE * SUPERSAMPLE
                row += bytes(
                    (
                        round(r / total),
                        round(g / total),
                        round(b / total),
                        round(255 * covered / total),
                    )
                )
        rows.append(bytes(row))
    return rows


def write_png(path, rows, size):
    raw = b"".join(b"\x00" + row for row in rows)

    def chunk(kind, payload):
        body = kind + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return len(data)


def main() -> int:
    written = []
    for density, size in DENSITIES.items():
        rows = render(size)
        path = RES / f"mipmap-{density}" / "ic_launcher.png"
        written.append((path, write_png(path, rows, size)))
    for path, size in written:
        print(f"wrote {path.relative_to(ROOT)} ({size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
