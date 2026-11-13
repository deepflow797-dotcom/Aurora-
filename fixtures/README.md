# fixtures/

A deliberately tiny library used by `cargo run -p aurora -- --demo` and by the
scanner tests.

- `music/01 - Rih il-Bahar [Demo].flac` — a placeholder, **not** audio: the
  Phase 0 scanner only looks at extensions, and decoding arrives in Phase 2.
- `music/01 - Rih il-Bahar [Demo].lrc` — sidecar lyrics, word-timed by
  `estimate_word_timing` at load. Mixed Arabic/English on purpose: it exercises
  BiDi ordering and Arabic shaping, which is the Phase 0 acceptance test.

- `music/00 - Fixture Tone.wav` — the one real audio file here: 0.05 s of 440 Hz
  at −12 dBFS, 48 kHz mono, 16-bit PCM (4.8 kB). `aurora --decode` and the
  decoder tests use it; expected values are peak `0.25`, RMS `0.1768`, 2400
  frames.

The file names carry a `NN - ` prefix and `[Demo]` bracket so the
`FileNameProbe` heuristics (track number, qualifier stripping) are covered too.
