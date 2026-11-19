#!/usr/bin/env python3
"""Generates `fixtures/music/00 - Fixture Tone.aiff`.

Byte-for-byte the same audio as `make-wav.py` - 0.05 s of a 440 Hz sine at
-12 dBFS, 48 kHz mono, 16-bit - but stored the AIFF way: big-endian samples in a
FORM/AIFF container, with the sample rate as an 80-bit IEEE 754 extended float.

That duplication is the point: `cargo test -p aurora-audio` decodes both fixtures
and asserts the two decoders agree sample by sample, so a sign-extension or byte
order regression in either one fails a test instead of reaching the speakers.

Regenerate with:
    python3 scripts/fixtures/make-aiff.py
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


def extended(value: float) -> bytes:
    """Encodes a positive number as an 80-bit IEEE 754 extended float."""
    if value <= 0:
        raise ValueError("sample rate must be positive")
    exponent = 16_383
    while value >= 2.0:
        value /= 2.0
        exponent += 1
    while value < 1.0:
        value *= 2.0
        exponent -= 1
    mantissa = int(value * 2**63)
    return bytes([(exponent >> 8) & 0x7F, exponent & 0xFF]) + mantissa.to_bytes(8, "big")


def chunk(ident: bytes, body: bytes) -> bytes:
    """An AIFF chunk: id, big-endian size, body, and a pad byte when odd."""
    out = ident + struct.pack(">I", len(body)) + body
    return out + b"\x00" if len(body) % 2 else out


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "fixtures/music/00 - Fixture Tone.aiff"
    frames = int(RATE * SECONDS)
    samples = b"".join(
        struct.pack(
            ">h",
            int(AMPLITUDE * 32_767.0 * math.sin(2.0 * math.pi * FREQUENCY * i / RATE)),
        )
        for i in range(frames)
    )

    comm = chunk(
        b"COMM",
        struct.pack(">hIh", CHANNELS, frames, BITS) + extended(RATE),
    )
    # SSND starts with offset and blockSize; both zero for a plain PCM file.
    ssnd = chunk(b"SSND", struct.pack(">II", 0, 0) + samples)
    body = b"AIFF" + comm + ssnd
    data = b"FORM" + struct.pack(">I", len(body)) + body

    with open(path, "wb") as handle:
        handle.write(data)
    print(f"wrote {path}: {len(data)} bytes, {frames} frames")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
