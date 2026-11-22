# Word-level blur rebuild — status (2026-11-19)

Branch: `arena/45bdc33d-aurora` · Base: `13422ed` (initial commit) merged
forward to `4e2796f` (`arena/504adea6-aurora`, the old desktop-first
Rust/iced codebase). That merge is the *entire* source this rebuild had:
the modern Android/Compose codebase described in the handoff documents
(`KaraokeLine.kt`, `:designsystem`, glass shell, Phase D previews) was
**not** in this repository and **not** on GitHub (`Aurora-` `main` was an
empty initial commit; `store-template` is empty; no Actions, Releases or
artifacts exist there). The commits `d69ae24` / `0250dcb`, the file
`agon-agent-blur.zip`, and the APK download links in the Arabic prompt
do not exist in any repository the user owns on GitHub.

User decisions for this rebuild (2026-11-19):
- Priority **C**: both (1) word-blur on the mini lyric card and
  (2) CI fix + APK build, in that order.
- **Liquid glass cancelled**: no Phase D glass design system is built.
  The UI stays the existing plain dark Compose screen; the blur work is
  functional, not a re-skin.
- Rebuild from the old branch + handoff documents, since the modern
  source is unavailable.

## What was built

### Phase 1 — blur law on the full line (recreated)
- `android/app/src/main/java/app/aurora/player/spike/KaraokeLine.kt`
  - `WordBlurLaw.BLUR_MAX_PX = 14f`, `WORD_BLUR_MAX_PX = 8f`
  - `drawBlurredWord()`: 4 stroke passes, widths 5 → 3.5 → 2 → 0.8 px
    (density 1.0) with alphas 0.035, 0.06, 0.10, 0.17, then a sharp fill.
  - Law: `blurProgress = clamp((nowMs − ws)/(we − ws), 0, 1)`,
    `blurRadius = maxPx × (1 − progress)`, `alpha = progress`.
  - `KaraokeLine` composable: measures words with `TextMeasurer`, draws
    each word as a whole string on a Canvas (Arabic joining preserved),
    RTL layout when the paragraph direction is RTL.
- Rust: `crates/aurora-mobile/src/lyrics.rs` now also sends per-word
  `startMs`/`endMs` in the lyrics JSON (additive; old parsers ignore
  unknown fields). New test `word_timings_are_sent_to_the_ui_for_the_blur_effect`.
- Kotlin: `LyricWord.startMs/endMs` + `blurProgress(nowMs)`;
  `Lyrics.positionMs` parsed from the payload (audio-clock `nowMs`).

### Phase 2 — mini lyric preview card (new)
- `android/app/src/main/java/app/aurora/player/designsystem/Components.kt`
  - `AuroraLyricPreviewBox`: current line with the same law at
    `WORD_BLUR_MAX_PX = 8f`, next line dimmed, "No lyrics" state,
    optional tap handler. Lives in the app module (no `:designsystem`
    Gradle module exists in this codebase; the package is ready to
    extract later without API changes).
  - Wired into `MainActivity` under the transport controls, above the
    full lyric panel. The active line in the full panel now renders via
    `KaraokeLine` (14f); inactive lines stay dim text.
  - RTL is detected from the lyric text itself (`isRtlText`), not the
    device locale.

### Phase 3 — browser preview (new, replaces the lost JS file)
- `preview_blur_mini_card.html`: interactive preview with the same
  `blurAt()` law, Play/Pause, seek slider, English/Arabic (RTL) toggle,
  mini card (8px) + full line (14px). CSS `filter: blur()` is only a
  browser stand-in for the Android stroke halo; timing is simulated
  (no audio clock in a browser).

### Phase 4 — CI fix + APK (new workflow; no local build possible)
- `android/app/build.gradle.kts`: added `lint { warningsAsErrors = false;
  abortOnError = false; checkReleaseBuilds = false }` with an explanatory
  comment. This is the fix the Arabic prompt asked for — but note the
  old codebase never had `warningsAsErrors = true` in the first place;
  the setting is now explicit so a future lint warning can never again
  discard the APK.
- `.github/workflows/android.yml` (new — none existed):
  - Rust job: `cargo fmt --check`, `cargo clippy -D warnings`,
    `cargo test --workspace`.
  - Android job: JDK 17, Android SDK, NDK r26, Rust Android targets,
    cargo-ndk → `scripts/android/build.sh --lib-only` (real
    `libaurora_mobile.so`), then
    `gradle lint test assembleDebug assembleRelease -PskipRust`.
  - Uploads `aurora-apks` (debug + release APKs) as an Actions artifact
    on every run, plus lint/test reports; on `v*` tag pushes it also
    creates a GitHub Release with the APKs attached.
- JVM unit tests: `android/app/src/test/java/app/aurora/player/WordBlurLawTest.kt`
  (pure-math law tests + RTL detection), with `testImplementation junit`.
  Run by CI's `test` task.

## How to get the APK
1. This branch is pushed to GitHub → Actions → **Android CI** runs
   automatically.
2. When it finishes: Actions → that run → **Artifacts → `aurora-apks`**
   → download (contains `app-debug.apk` + `app-release.apk`).
3. Install on Android 8.0+ (arm64-v8a): allow "install unknown apps",
   open the APK. Open a track with a sidecar `.lrc`, press Play, and
   watch the mini card under the transport controls: each word starts
   blurred and sharpens as it is sung. Try the Arabic fixture
   (`fixtures/music/00 - Fixture Steps.flac` + its `.lrc`).
4. No APK can be built in this sandbox: there is no JDK, Gradle,
   Android SDK/NDK, or Rust toolchain here (`cargo`/`java` not found).
   The workflow above is the only builder — same constraint as before.

## Honest limitations (not verified)
- **Nothing here has been compiled or run.** No JDK/Gradle/Rust in this
  environment, so Kotlin compilation, Rust tests, and the APK are all
  *unverified* until the first CI run. If CI fails, the logs (Actions →
  run → job) name the failing step; fix and re-run.
- The blur updates at the existing **200 ms poll rate**
  (`PlayerViewModel`), so on Android it advances in steps, not per
  frame. Smooth per-frame interpolation from the audio clock (spec
  63.4) is future work — the browser preview is smooth because it uses
  `requestAnimationFrame`, which Android does not yet.
- Audio output is still the simulated paced device (no AAudio backend
  in this codebase), so "playback" advances the timeline silently.
  The blur is driven by that same timeline, which is what makes it
  testable without sound hardware.
- The scanner walks a filesystem path (no SAF/MediaStore yet), as
  documented in `android/README.md`.
- Liquid glass / Phase D is cancelled by user decision; the design
  documents in the handoff files describe a UI that this rebuild
  intentionally does not implement.

## What is still missing from the original prompt
- The lost modern codebase (glass shell, `:designsystem` module,
  Paparazzi snapshots, Test Lab results) — needs the user to upload
  the source archive or push it to GitHub; it cannot be reconstructed
  from the two Markdown handoffs alone.
- Original design photos (`Aurora-All-Designs.jpg`, board, mini-player
  crop, lyrics-header photo) — not attached; only the two `.md` files
  were attached.
- `agon-agent-blur.zip` / `lyrics.js` / `lyrics-patch.html` — not found
  on GitHub; superseded by `preview_blur_mini_card.html` above.

## Phase 5 — blur confined to the mini card (2026-11-21)

Branch `arena/e40be466-aurora`, based on `main` @ `8140669`.

- `MainActivity.kt` → `LyricsPanel` no longer renders the active line with
  `KaraokeLine`; the whole full-lyrics panel is plain `Text` (active line
  bolder/brighter, karaoke word tint kept). `KaraokeLine` / `WordBlurLaw`
  imports dropped from the activity. `KaraokeLine.kt` itself stays as the
  single implementation the mini card calls.
- Result: the word-level blur now has exactly one home —
  `AuroraLyricPreviewBox` (`android/.../designsystem/Components.kt`,
  `WORD_BLUR_MAX_PX = 8f`) — matching the design constraint in the handoff.
  Size, position, typography, colours and the `LYRICS` label of the card
  are unchanged; RTL detection is unchanged.
- `preview_blur_mini_card.html`: the `FULL LINE (14px)` panel became
  `FULL LYRICS PANEL · PLAIN TEXT (no blur)` so the preview matches the
  shipped screen.

### CI status — correction to the handoff note

The handoff described a 12-attempt fight with a Gradle
`CachingState.fold` / `CacheBasedImmutableWorkspaceProvider` crash on
`ubuntu-24.04`, and a `.github/workflows/ci.yml` running Gradle 8.11.1.
None of that exists in this repository:

- There is no `ci.yml`; the only workflow is `.github/workflows/android.yml`
  (Gradle 8.9 via `gradle/actions/setup-gradle@v4`, `ubuntu-latest`).
- Nothing in the repo references `CachingState`, `ubuntu-22.04`,
  `--no-build-cache`, `configure-on-demand`, or Gradle 8.11/8.14.
- Actions history shows the workflow **green** on `main`
  (run `44602358190`) and on `arena/fe6943ef-aurora` (run `44762180370`,
  2026-11-20), the latter with a 30 MB `aurora-apks` artifact containing
  `app-debug.apk` + `app-release.apk`.

So no CI repair was needed or attempted; the caching-crash note belongs to
a different workspace.

Compile evidence for the change above: the final `MainActivity.kt` is
byte-identical to `f5307d5` (`arena/fe6943ef-aurora`) apart from one
comment, and that commit's Android job is green in run `44762180370`.

### CI blocked again (2026-11-21, billing — not Gradle)

After this branch was pushed, every job failed within two seconds with
zero steps executed and no runner assigned. The check-run annotation
reads: *"The job was not started because recent account payments have
failed or your spending limit needs to be increased. Please check the
'Billing & plans' section in your settings."* A deliberately trivial
three-line probe workflow (`.github/workflows/probe.yml`, since removed)
failed identically, which rules out `android.yml`, Gradle, and the code.
Fix: **Settings → Billing & plans** (Actions minutes / payment method),
then re-run `Android CI`; the workflow itself needs no change. The green run of 2026-11-20 already covered the
"full line off / mini card only" change, which had been left on an
unmerged branch (`arena/fe6943ef-aurora`); Phase 5 above re-lands it on the
current branch so `main` gets it too.
