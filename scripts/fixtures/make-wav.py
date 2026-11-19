#!/usr/bin/env python3
"""Generates `fixtures/music/00 - Fixture Tone.wav`.

0.05 s of a 440 Hz sine at -12 dBFS, 48 kHz mono, 16-bit PCM. Small enough to
commit (4.8 kB), exact enough to assert on: 2400 frames, peak 0.25, RMS 0.1768.

Regenerate with:
    python3 scripts/fixtures/make-wav.py
"""

import math
import struct
import sys

RATE = 48_000
SECONDS = 0.05
AMPLITUDE = 0.25
FREQUENCY = 440.0
CHANNELS = 1
BITS = 16


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "fixtures/music/00 - Fixture Tone.wav"
    frames = int(RATE * SECONDS)
    samples = b"".join(
        struct.pack(
            "<h",
            int(AMPLITUDE * 32_767.0 * math.sin(2.0 * math.pi * FREQUENCY * i / RATE)),
        )
        for i in range(frames)
    )

    block_align = CHANNELS * BITS // 8
    fmt = struct.pack(
        "<HHIIHH", 1, CHANNELS, RATE, RATE * block_align, block_align, BITS
    )
    body = (
        b"WAVE"
        + b"fmt "
        + struct.pack("<I", len(fmt))
        + fmt
        + b"data"
        + struct.pack("<I", len(samples))
        + samples
    )
    data = b"RIFF" + struct.pack("<I", len(body)) + body

    with open(path, "wb") as handle:
        handle.write(data)
    print(f"wrote {path}: {len(data)} bytes, {frames} frames")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
