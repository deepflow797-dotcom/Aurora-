# fixtures/

A deliberately tiny library used by `cargo run -p aurora -- --demo` and by the
scanner tests.

- `music/01 - Rih il-Bahar [Demo].flac` — a placeholder, **not** audio: the
  Phase 0 scanner only looks at extensions, and decoding arrives in Phase 2.
- `music/01 - Rih il-Bahar [Demo].lrc` — sidecar lyrics, word-timed by
  `estimate_word_timing` at load. Mixed Arabic/English on purpose: it exercises
  BiDi ordering and Arabic shaping, which is the Phase 0 acceptance test.

The file name carries a `NN - ` prefix and `[Demo]` bracket so the
`FileNameProbe` heuristics (track number, qualifier stripping) are covered too.
