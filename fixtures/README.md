# fixtures/

A deliberately tiny library used by `cargo run -p aurora -- --demo`, by the
scanner tests and by the decoder tests.

| File | What it is |
| --- | --- |
| `music/00 - Fixture Tone.wav` | 0.05 s of 440 Hz at −12 dBFS, 48 kHz mono, 16-bit PCM (4.8 kB). Written by `scripts/fixtures/make-wav.py`. Expected: 2400 frames, peak `0.25`, RMS `0.1768`. |
| `music/00 - Fixture Steps.flac` | 8192 frames at 48 kHz mono: 4096 samples at `0.25` full scale, then 4096 silent (64 bytes). Written byte by byte by `scripts/fixtures/make-flac.py`; expected peak `0.25`, RMS `0.1768`, duration 0.171 s. |
| `music/00 - Fixture Steps.lrc` | Sidecar lyrics for the FLAC file, word-timed by `estimate_word_timing` at load. Mixed Arabic/English on purpose: it exercises BiDi ordering and Arabic shaping, which is the UI acceptance test. |

The file names carry a `NN - ` prefix so the `FileNameProbe` heuristics (track
number, qualifier stripping) are covered too.

Both audio files are synthetic and tiny on purpose: they are checked into git, so
they must stay small, and their expected values must stay exact. Regenerate them
with:

```text
python3 scripts/fixtures/make-wav.py
python3 scripts/fixtures/make-flac.py "fixtures/music/00 - Fixture Steps.flac"
```
