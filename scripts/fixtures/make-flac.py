#!/usr/bin/env python3
"""Generates the FLAC fixture used by the decoder tests.

Why a generator instead of a committed binary from elsewhere:

* the sandbox has no encoder (`flac`, `ffmpeg`, `sox` are all absent) and
  `flacenc` cannot be vendored offline (it needs `heapless` -> `built`);
* a FLAC file written by hand is auditable: every byte below is specified by the
  format, and Symphonia has to agree with it, which is the point of the test;
* the signal is deliberately simple - two constant-value blocks - so the expected
  output is exact (`fixtures/README.md` records the numbers).

Format used (FLAC, fixed blocking, 48 kHz, mono, 16 bit):
    "fLaC" + STREAMINFO + N frames, each a CONSTANT subframe.

Usage: python3 scripts/fixtures/make-flac.py fixtures/music/00\\ -\\ Fixture\\ Steps.flac
"""

import struct
import sys

RATE = 48_000
BITS = 16
CHANNELS = 1
BLOCK = 4096
# (frame index, sample value) - 4096 samples per frame.
BLOCKS = [(0, 8192), (1, 0)]


def crc8(data: bytes) -> int:
    """FLAC CRC-8: polynomial x^8 + x^2 + x + 1 (0x07), init 0."""
    crc = 0
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = ((crc << 1) ^ 0x07) & 0xFF if crc & 0x80 else (crc << 1) & 0xFF
    return crc


def crc16(data: bytes) -> int:
    """FLAC CRC-16: polynomial x^16 + x^15 + x^2 + 1 (0x8005), init 0."""
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def utf8_number(value: int) -> bytes:
    """FLAC frame-number coding: UTF-8 style, as used for frame numbers."""
    if value < 0x80:
        return bytes([value])
    if value < 0x800:
        return bytes([0xC0 | (value >> 6), 0x80 | (value & 0x3F)])
    raise ValueError("frame number too large for this fixture")


def streaminfo(total_samples: int) -> bytes:
    bits = 0
    for field, width in (
        (BLOCK, 16),
        (BLOCK, 16),
        (0, 24),  # min frame size: unknown
        (0, 24),  # max frame size: unknown
    ):
        bits = (bits << width) | field
    # 20 bits rate, 3 bits channels-1, 5 bits bits-1, 36 bits total samples.
    body = bits << 20 | RATE
    body = body << 3 | (CHANNELS - 1)
    body = body << 5 | (BITS - 1)
    body = body << 36 | total_samples
    # 80 bits of frame sizes + 64 bits of rate/channels/depth/length = 18 bytes.
    packed = body.to_bytes(18, "big")
    # 128-bit MD5 of the unencoded samples: zero means "not computed".
    return packed + b"\x00" * 16


def metadata_block(block_type: int, body: bytes, last: bool) -> bytes:
    header = ((0x80 if last else 0x00) | block_type) << 24 | len(body)
    return header.to_bytes(4, "big") + body


def frame(index: int, value: int, rate_code: int, block_code: int, bits_code: int) -> bytes:
    # Sync 0b11111111111110, reserved 0, blocking strategy 0 (fixed).
    header = bytearray()
    header += bytes([0xFF, 0xF8])
    header.append((block_code << 4) | rate_code)
    header.append((0 << 4) | (bits_code << 1))  # mono, 16-bit, reserved bit 0
    header += utf8_number(index)
    header.append(crc8(bytes(header)))

    # CONSTANT subframe: zero bit, 6-bit type 000000, wasted-bits flag 0.
    subframe = bytearray()
    subframe.append(0x00)  # 0 | 000000 | 0
    subframe += struct.pack(">h", value)

    # 16-bit samples and byte-sized header fields leave nothing to pad; FLAC
    # would require zero padding here otherwise.
    body = bytes(header) + bytes(subframe)
    return body + struct.pack(">H", crc16(body))


def build() -> bytes:
    total = sum(BLOCK for _ in BLOCKS)
    out = bytearray(b"fLaC")
    out += metadata_block(0, streaminfo(total), last=True)
    rate_code = 0b0000  # from STREAMINFO
    block_code = 0b1100  # 4096 samples
    bits_code = 0b100  # 16 bits per sample
    for index, value in BLOCKS:
        out += frame(index, value, rate_code, block_code, bits_code)
    return bytes(out)


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "fixtures/music/00 - Fixture Steps.flac"
    data = build()
    with open(path, "wb") as handle:
        handle.write(data)
    print(f"wrote {path}: {len(data)} bytes, {len(BLOCKS)} frames of {BLOCK} samples")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
