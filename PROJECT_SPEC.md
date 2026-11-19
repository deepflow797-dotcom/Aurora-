# RUST MUSIC PLAYER — PROJECT SPECIFICATION

## 0. Project Identity

**Working name:** Aurora Player  
**Type:** Personal desktop music player  
**Primary goal:** Build a fast, beautiful, premium-feeling local music player with exceptional synchronized lyrics and motion design.

This project is intentionally designed so that **multiple independent AI coding agents can build it separately**. Each agent must receive this same specification and should produce a complete, runnable application. The final applications will be compared and the best implementation will be selected.

The application must be **Rust-first**. Core application logic, audio playback, library management, lyrics synchronization, state management, performance-sensitive systems, and data processing must be implemented in Rust.

The visual design may use a Rust-compatible UI technology, but the project must not become a thin wrapper around a non-Rust application.

---

# 1. Core Product Vision

Build a personal music player that feels significantly more polished than a conventional local music player.

The defining feature is the **lyrics experience**:

- Precisely synchronized lyrics.
- Line-level synchronization.
- Word-level synchronization when timing data exists.
- Smooth kinetic typography.
- Current-word emphasis.
- Intelligent scrolling.
- Beat-aware but restrained motion.
- Seeking by tapping a lyric.
- Beautiful transitions between lyric sections.
- Lyrics should feel like part of the music rather than subtitles placed on top of a player.

The second defining feature is the **Now Playing experience**:

- Large album artwork.
- Artwork-derived visual atmosphere.
- Beautiful typography.
- Smooth transitions.
- Elegant playback controls.
- Progress visualization.
- Optional ambient motion.
- Responsive layout.
- Minimal clutter.

The product must feel intentional, fast, fluid, and premium.

It must NOT be a pixel-for-pixel clone of Apple Music.

Use Apple Music only as inspiration for the quality of synchronized lyrics and motion, while creating an original visual identity.

---

# 2. Non-Negotiable Requirements

## 2.1 Technology

Primary language:

- Rust

Rust must handle:

- Audio engine.
- Playback state.
- Queue.
- Library indexing.
- Metadata.
- Lyrics parsing.
- Lyrics synchronization.
- Word timing.
- Search/indexing.
- Persistence.
- Settings.
- Performance-sensitive animation state where appropriate.

The UI must integrate naturally with the Rust application.

Possible Rust-compatible UI technologies may be evaluated by the agent, but the agent must justify the choice in `ARCHITECTURE.md`.

Do not choose a technology merely because it is trendy.

Evaluate:

- Performance.
- Animation capability.
- Linux support.
- macOS support.
- Windows support.
- GPU acceleration.
- Developer experience.
- Long-term maintainability.
- Rust integration.
- Accessibility.
- Packaging.

---

# 3. Platform Strategy

## Primary target

Desktop.

The first implementation should prioritize:

1. Linux
2. macOS
3. Windows

The architecture should avoid unnecessary platform-specific assumptions.

The project must be designed so that platform-specific audio/output functionality can be isolated behind interfaces.

---

# 4. Design Philosophy

The application should feel:

- Premium
- Calm
- Cinematic
- Modern
- Fast
- Minimal
- Responsive
- Original

Avoid:

- Generic dashboard design.
- Excessive glassmorphism.
- Excessive gradients.
- Huge shadows.
- Excessive animations.
- UI clutter.
- Copying Apple Music layouts exactly.
- Animations that consume CPU/GPU unnecessarily.

The interface should communicate hierarchy through:

- Typography
- Spacing
- Scale
- Motion
- Contrast
- Artwork
- Contextual color

Motion should support the music experience rather than distract from it.

---

# 5. Visual Identity

The agent may refine the final identity, but it must maintain these principles.

## Base palette

Dark-first.

Suggested base:

- Background: near-black / deep charcoal
- Primary text: off-white
- Secondary text: muted gray
- Accent: generated from artwork
- Interactive states: subtle brightness changes

The accent color can dynamically derive from album artwork.

However:

**Do not allow artwork colors to destroy readability.**

The application must calculate contrast and adjust generated colors.

---

# 6. Main Navigation

Suggested navigation:

```text
HOME
LIBRARY
PLAYLISTS
FAVORITES
RECENTLY PLAYED
SETTINGS
```

Navigation should remain simple.

The user should be able to reach:

- Current song
- Library
- Albums
- Artists
- Playlists
- Favorites
- Lyrics
- Settings

without unnecessary navigation depth.

---

# 7. Screens

## 7.1 Home

Home should show useful personal music content rather than fake recommendations.

Possible sections:

- Recently Played
- Recently Added
- Favorites
- Albums
- Artists
- Playlists
- Continue Listening

If insufficient data exists, the application should gracefully adapt.

Never fill the UI with fake content.

---

# 7.2 Library

Library must support:

- Songs
- Albums
- Artists
- Genres
- Playlists
- Favorites
- Recently Added

Sorting:

- Title
- Artist
- Album
- Date Added
- Duration

Filtering/search should feel instant.

---

# 7.3 Album View

Show:

- Album artwork
- Album title
- Artist
- Year
- Track count
- Total duration
- Track list

Interactions:

- Play album
- Shuffle album
- Add to playlist
- Favorite
- Open artist
- Open song
- Open lyrics

---

# 7.4 Artist View

Show:

- Artist artwork if available
- Artist name
- Songs
- Albums
- Playlists containing the artist

---

# 7.5 Playlist View

Support:

- Create playlist
- Rename
- Delete
- Add/remove songs
- Reorder songs
- Play
- Shuffle

Persistence must survive application restarts.

---

# 8. NOW PLAYING — PRIMARY EXPERIENCE

This is one of the most important screens.

Layout:

```text
┌───────────────────────────────────────────┐
│                 Album Art                  │
│                                           │
│              Song Title                   │
│              Artist Name                  │
│                                           │
│       ─────── progress ───────            │
│                                           │
│        ◀        ▶ / ❚❚        ▶          │
│                                           │
│              Lyrics                       │
└───────────────────────────────────────────┘
```

But the final layout should be more sophisticated than this diagram.

Features:

- Artwork.
- Dynamic background.
- Song title.
- Artist.
- Album.
- Progress.
- Current time.
- Remaining time.
- Play/pause.
- Previous.
- Next.
- Shuffle.
- Repeat.
- Volume.
- Queue.
- Lyrics.
- Favorite.

---

# 9. ALBUM ART VISUAL SYSTEM

The album artwork should drive subtle visual atmosphere.

Pipeline:

```text
Album Art
    ↓
Image Analysis
    ↓
Color Extraction
    ↓
Dominant / Supporting Colors
    ↓
Contrast Adjustment
    ↓
Background Generation
    ↓
Smooth Transition
```

When the song changes:

- Do not instantly replace the entire background.
- Animate the transition.
- Prevent flashing.
- Avoid excessive GPU usage.

Optional effects:

- Blur.
- Gradient.
- Soft glow.
- Slow scale.
- Very subtle parallax.

Effects must degrade gracefully on weaker hardware.

---

# 10. LYRICS ENGINE

This is the defining technical feature.

The lyrics system must be an independent module.

## 10.1 Supported lyric types

At minimum:

### Plain lyrics

```text
Hello world
This is a song
```

### Line-timed LRC

```text
[00:12.50]Hello world
[00:15.20]This is a song
```

### Extended LRC

Support word-level timing where available.

Example conceptual structure:

```text
[
  {
    start: 12.50,
    end: 15.20,
    words: [
      { text: "Hello", start: 12.50, end: 13.10 },
      { text: "world", start: 13.11, end: 14.20 }
    ]
  }
]
```

The internal representation must be richer than raw LRC text.

---

# 11. LYRICS TIMELINE MODEL

Every synchronized lyric element should ultimately be represented on a timeline.

Example:

```text
Song timeline

0s ─────────────────────────────── 240s

        Line 1
        ├───────┤

                 Line 2
                 ├─────────┤

                           Line 3
                           ├──────┤
```

For word-level lyrics:

```text
Line:
I      don't      wanna      go

12.0   12.4       13.0       13.7
```

The engine must calculate the active lyric from the current playback position.

---

# 12. WORD-LEVEL ANIMATION

When word timing exists:

- Current word receives primary emphasis.
- Previous words remain visible but become secondary.
- Upcoming words remain readable.
- Transition must be smooth.
- Avoid abrupt jumps.
- Animation must remain synchronized even when seeking.

Possible effects:

- Opacity transition.
- Scale.
- Weight change.
- Position interpolation.
- Blur-to-sharp transition.
- Subtle glow.
- Gradient fill.

Do not use all effects simultaneously.

The animation system should support configurable animation profiles.

---

# 13. LYRIC ANIMATION PROFILES

Implement a reusable animation system.

Example profiles:

### Minimal

Very subtle.

### Classic

Smooth current-word emphasis.

### Kinetic

More movement and typography.

### Cinematic

Large transitions and slow visual movement.

### Performance

Minimal animation for low-power devices.

The user can choose the profile.

---

# 14. LYRIC AUTO-SCROLL

The lyric view must automatically follow the currently active line.

Requirements:

- Current line remains in a comfortable visual position.
- Scrolling must be animated.
- Avoid jitter.
- Avoid fighting the user's manual scrolling.
- Manual scrolling should temporarily suspend auto-follow.
- Auto-follow should resume intelligently after inactivity.

---

# 15. SEEKING FROM LYRICS

Click/tap a synchronized lyric:

```text
User selects lyric
        ↓
Find timestamp
        ↓
Audio engine seeks
        ↓
Playback resumes
        ↓
Lyrics engine updates
        ↓
Animation catches up
```

Seeking must be accurate.

---

# 16. UNSYNCHRONIZED LYRICS

If only plain lyrics exist:

- Display them normally.
- Do not pretend they are synchronized.
- Provide a visually distinct but elegant static mode.

If lyrics do not exist:

```text
Lyrics unavailable
```

must be presented elegantly.

Never display broken parser output.

---

# 17. AUDIO ENGINE

The audio engine must provide:

- Play
- Pause
- Stop
- Seek
- Volume
- Next
- Previous
- Queue
- Repeat
- Shuffle
- Position
- Duration
- Playback state

Supported formats should be chosen based on reliable Rust audio libraries.

At minimum investigate:

- MP3
- FLAC
- WAV
- OGG/Vorbis
- AAC/M4A where platform support permits

The agent must document actual codec support.

---

# 18. GAPLESS / SMOOTH PLAYBACK

If practical with the chosen audio stack:

- Minimize gaps between tracks.
- Preload upcoming track metadata.
- Avoid unnecessary reinitialization.

Do not sacrifice stability for theoretical gapless playback.

---

# 19. AUDIO OUTPUT

Support:

- Default system output.
- Volume control.
- Mute.
- Device changes where supported.

Architecture should isolate output backend.

---

# 20. QUEUE SYSTEM

Queue operations:

- Add song
- Remove song
- Move song
- Clear queue
- Play next
- Play later
- Save queue as playlist

The queue must persist appropriately across restarts if the product design chooses that behavior.

---

# 21. SEARCH

Search must be fast.

Search across:

- Songs
- Artists
- Albums
- Playlists
- Genres

Support fuzzy matching where practical.

Search should not require network access.

---

# 22. MUSIC LIBRARY INDEXER

The user should be able to choose music folders.

The application should scan:

```text
Music folder
    ↓
Discover files
    ↓
Read metadata
    ↓
Extract artwork
    ↓
Detect lyrics
    ↓
Index
    ↓
Database
```

The indexer must not block the UI.

Large libraries must be handled incrementally.

---

# 23. FILE WATCHING

Where supported:

- Detect new songs.
- Detect deleted songs.
- Detect renamed files.
- Detect modified metadata.

Do not rescan the entire library unnecessarily.

---

# 24. METADATA

Read common metadata:

- Title
- Artist
- Album
- Album Artist
- Track Number
- Disc Number
- Genre
- Year
- Duration
- Artwork

Gracefully handle missing metadata.

---

# 25. ARTWORK CACHE

Artwork should be cached.

Requirements:

- Avoid decoding the same image repeatedly.
- Generate thumbnails.
- Cache appropriate sizes.
- Invalidate cache when artwork changes.

Memory usage must be controlled.

---

# 26. DATABASE / PERSISTENCE

Persistent data may include:

- Library index.
- Playlists.
- Favorites.
- Playback history.
- Settings.
- Queue.
- Lyrics cache.
- Artwork cache metadata.

Use a reliable embedded database or appropriate Rust persistence layer.

The database must not become a single point of failure for the music files themselves.

Deleting the database must never delete the user's actual music.

---

# 27. SETTINGS

Settings should include:

### Playback

- Volume
- Repeat
- Shuffle
- Resume playback

### Lyrics

- Enable/disable lyrics
- Animation profile
- Auto-scroll
- Word highlighting
- Lyrics font size

### Appearance

- Theme
- Accent behavior
- Artwork effects
- Animation intensity

### Performance

- Reduced motion
- GPU effects
- Cache size
- Background scanning

### Library

- Music folders
- Rescan
- Watch folders

---

# 28. PERFORMANCE MODE

The app must have a low-resource mode.

When enabled:

- Reduce blur.
- Reduce animation.
- Disable expensive background effects.
- Reduce artwork resolution.
- Reduce visual analysis frequency.

The application should remain beautiful without requiring high-end hardware.

---

# 29. ANIMATION ARCHITECTURE

Animations must not be scattered randomly throughout UI code.

Create a reusable motion system.

Example:

```text
MotionSystem
├── Duration
├── Easing
├── Fade
├── Scale
├── Translate
├── Spring
├── Crossfade
├── Stagger
└── Timeline
```

Define reusable timing constants.

Example conceptual values:

```text
FAST
NORMAL
SLOW
SPRING
```

The exact values should be tuned by the agent.

---

# 30. MOTION PRINCIPLES

### Rule 1

Everything should feel intentional.

### Rule 2

No animation should delay interaction.

### Rule 3

User actions should receive immediate visual feedback.

### Rule 4

Song transitions should feel musical.

### Rule 5

Lyrics motion should follow timing rather than arbitrary UI timers.

### Rule 6

Animations must remain synchronized after seeking.

### Rule 7

Reduced-motion settings must disable or simplify non-essential motion.

---

# 31. RESPONSIVE DESIGN

Support:

- Small window.
- Medium window.
- Large window.
- Full screen.

The interface should reflow instead of merely shrinking.

Possible layout states:

```text
Compact
    ↓
Standard
    ↓
Wide
```

---

# 32. KEYBOARD SHORTCUTS

At minimum:

```text
Space       Play/Pause
Left        Previous / seek
Right       Next / seek
Up/Down     Volume
Ctrl/Cmd+K  Search
Ctrl/Cmd+L  Lyrics
Ctrl/Cmd+F  Focus search
```

The final shortcut map should be documented.

---

# 33. MEDIA KEYS

Where supported:

- Play/Pause
- Previous
- Next

The application should integrate with OS media controls where practical.

---

# 34. NOTIFICATIONS

Optional:

When song changes:

```text
Album artwork
Song title
Artist
```

Notifications should be unobtrusive.

---

# 35. ACCESSIBILITY

Support:

- Keyboard navigation.
- Visible focus.
- Reasonable contrast.
- Reduced motion.
- Screen-reader-friendly labels where supported.
- Large text option.

---

# 36. ERROR HANDLING

Never crash because:

- A song has broken metadata.
- A lyrics file is malformed.
- Artwork is corrupted.
- A file disappears.
- A codec fails.
- A folder becomes inaccessible.

Instead:

```text
Log error
    ↓
Skip/Recover
    ↓
Keep application running
```

User-facing errors must be understandable.

---

# 37. OFFLINE-FIRST

The application must work without internet for core functionality.

Internet must not be required for:

- Playback
- Library
- Local lyrics
- Playlists
- Search
- Favorites
- History
- Settings

If online lyrics are eventually added, they must be an optional layer.

---

# 38. SECURITY / PRIVACY

The application is personal and local-first.

Do not collect user music data.

Do not upload music files without explicit user action.

Do not send library contents to remote services by default.

If an online service is added later, make network activity explicit.

---

# 39. PROJECT STRUCTURE

The exact structure is up to the agent, but it should separate concerns.

Suggested conceptual structure:

```text
src/
├── app/
├── audio/
├── library/
├── metadata/
├── lyrics/
├── lyrics_sync/
├── playback/
├── queue/
├── playlists/
├── database/
├── artwork/
├── search/
├── settings/
├── shortcuts/
├── notifications/
├── performance/
├── ui/
├── motion/
└── main.rs
```

Do not force this exact structure if the chosen architecture has a better organization.

---

# 40. CORE DOMAIN MODELS

At minimum consider:

```text
Song
Album
Artist
Playlist
Lyrics
LyricLine
LyricWord
QueueItem
PlaybackState
LibraryItem
Artwork
Settings
```

Domain models should not depend heavily on UI implementation.

---

# 41. STATE MANAGEMENT

Separate:

### Application state

What the app currently knows.

### Playback state

What the audio engine is doing.

### UI state

What the interface is displaying.

### Persistent state

What must survive restart.

Do not create one giant mutable global state object.

---

# 42. AUDIO CLOCK VS UI CLOCK

This is critical.

Lyrics synchronization must use the **actual playback position**, not an independent UI timer.

Correct model:

```text
Audio Engine
      ↓
Playback Position
      ↓
Lyrics Timeline
      ↓
Current Word / Line
      ↓
UI Animation
```

Do NOT implement lyrics by simply incrementing a timer in the UI.

This prevents drift.

---

# 43. SEEK RECOVERY

After seeking:

1. Get actual audio position.
2. Find corresponding lyric line.
3. Find corresponding word.
4. Update lyric state.
5. Reposition scroll.
6. Continue animation from the new timeline position.

No visible desynchronization.

---

# 44. FRAME PERFORMANCE

The UI should target smooth rendering.

The agent should profile:

- CPU usage.
- GPU usage.
- Memory.
- Startup time.
- Library scan time.
- Artwork decoding.
- Lyrics rendering.
- Animation frame time.

Avoid premature micro-optimization, but performance regressions must be measurable.

---

# 45. STARTUP

Startup should feel fast.

Avoid:

- Blocking library scan.
- Blocking artwork processing.
- Blocking database migrations unnecessarily.
- Loading the entire music library into memory.

Recommended:

```text
Start
 ↓
Show UI
 ↓
Load essential state
 ↓
Background indexing
 ↓
Update UI progressively
```

---

# 46. CACHING

Use caching for:

- Artwork
- Metadata
- Search indexes where appropriate
- Parsed lyrics
- Thumbnails

Cache invalidation must be deterministic.

---

# 47. TESTING

The project must contain tests.

## Unit tests

Test:

- LRC parsing.
- Timestamp parsing.
- Word timing.
- Lyrics lookup.
- Seek behavior.
- Queue.
- Shuffle.
- Repeat.
- Metadata parsing.
- Search.

## Integration tests

Test:

- Library indexing.
- Database persistence.
- Playback state.
- Playlist persistence.
- Lyrics loading.

## UI tests

Where practical:

- Navigation.
- Playback controls.
- Lyrics interaction.
- Keyboard shortcuts.

---

# 48. LYRICS TEST DATA

Create deterministic test fixtures.

Example:

```text
[00:01.00]Hello
[00:02.00]world
[00:03.00]this is a test
```

Word-level fixture:

```text
Line start = 3.0

this   3.0
is     3.4
a      3.7
test   3.9
```

Test:

- Before line.
- Start of line.
- Middle of word.
- End of word.
- Between lines.
- Seek.
- Reverse seek.
- Rapid seek.

---

# 49. EDGE CASES

Must handle:

- Missing lyrics.
- Empty lyrics.
- Malformed LRC.
- Overlapping timestamps.
- Duplicate timestamps.
- Very short lyrics.
- Extremely long lyrics.
- Lyrics faster than audio.
- Lyrics slower than audio.
- Songs with no artwork.
- Huge artwork.
- Very large libraries.
- Missing files.
- Unicode filenames.
- Arabic lyrics.
- Japanese lyrics.
- Korean lyrics.
- RTL text.
- Mixed RTL/LTR lyrics.

---

# 50. INTERNATIONALIZATION

Architecture should not hard-code English.

At minimum, text rendering must correctly support Unicode.

The lyric renderer must correctly handle:

- Arabic.
- Hebrew.
- Japanese.
- Chinese.
- Korean.
- Latin languages.

RTL lyric layout must be considered from the beginning.

---

# 51. VISUAL QUALITY BAR

Before declaring the project complete, the agent must perform a visual quality pass.

Check:

- Alignment.
- Spacing.
- Typography.
- Contrast.
- Animation timing.
- Artwork transitions.
- Empty states.
- Loading states.
- Error states.
- Hover states.
- Press states.
- Focus states.
- Small-window layout.
- Large-window layout.

Do not accept default framework styling as final design.

---

# 52. NO FAKE FEATURES

If a feature is not implemented:

- Do not create a fake button that does nothing.
- Do not display fake statistics.
- Do not display fake recommendations.
- Do not claim support that does not exist.

Either implement it or clearly mark it as planned.

---

# 53. DEVELOPMENT PHASES

The project is intentionally divided into independent phases.

The user can choose how many phases to execute.

## Phase 0 — Architecture

Deliver:

- Architecture decision.
- Technology selection.
- Repository structure.
- Domain model.
- State model.
- Audio strategy.
- Lyrics strategy.
- UI strategy.
- Database strategy.
- Testing strategy.

No large feature implementation required.

---

## Phase 1 — Rust Foundation

Deliver:

- Rust project.
- Application boot.
- Basic UI shell.
- Configuration.
- Logging.
- Error handling.
- Build system.
- Basic persistence.

Acceptance:

Application launches reliably.

---

## Phase 2 — Audio Engine

Deliver:

- File playback.
- Play/pause.
- Seek.
- Volume.
- Duration.
- Previous/next.
- Queue.

Acceptance:

A local song can be played reliably from start to finish.

---

## Phase 3 — Library

Deliver:

- Folder selection.
- File scanning.
- Metadata.
- Albums.
- Artists.
- Songs.
- Search.
- Database persistence.

Acceptance:

A real music folder can be imported and browsed.

---

## Phase 4 — Playlists / Favorites / History

Deliver:

- Playlists.
- Favorites.
- Recently played.
- Queue persistence where appropriate.

---

## Phase 5 — Now Playing

Deliver:

- Album artwork.
- Playback controls.
- Progress.
- Artwork-derived background.
- Responsive layout.
- Initial motion system.

This phase establishes the visual identity.

---

## Phase 6 — Lyrics Foundation

Deliver:

- Lyrics loading.
- LRC parser.
- Plain lyrics.
- Line synchronization.
- Lyrics screen.

Acceptance:

Lyrics remain synchronized during normal playback.

---

## Phase 7 — Word-Level Lyrics

Deliver:

- Word timestamps.
- Current-word detection.
- Word highlighting.
- Accurate seeking.
- Timeline engine.

This is a major milestone.

---

## Phase 8 — Kinetic Lyrics

Deliver:

- Smooth word transitions.
- Auto-scroll.
- Motion profiles.
- Typography animation.
- Line transitions.
- Reduced-motion mode.

Acceptance:

Lyrics feel like a premium synchronized experience rather than subtitles.

---

## Phase 9 — Advanced Visual System

Deliver:

- Artwork color extraction.
- Dynamic backgrounds.
- Album transitions.
- Ambient effects.
- Performance-aware rendering.

---

## Phase 10 — UX Polish

Deliver:

- Keyboard shortcuts.
- Media keys.
- Notifications.
- Context menus.
- Hover/press states.
- Loading states.
- Empty states.
- Error states.
- Accessibility.

---

## Phase 11 — Performance

Profile and optimize:

- Startup.
- Library scanning.
- Search.
- Playback.
- Lyrics rendering.
- Artwork.
- Animation.
- Memory.

Create before/after measurements.

---

## Phase 12 — Reliability

Test:

- Large libraries.
- Corrupted files.
- Missing files.
- Malformed lyrics.
- Rapid song switching.
- Rapid seeking.
- Suspend/resume.
- App restart.
- Database recovery.

---

## Phase 13 — Packaging

Produce installable builds for target platforms.

Document:

- Build.
- Run.
- Package.
- Update.
- Uninstall.

---

## Phase 14 — Final Release

Deliver:

- Stable application.
- Documentation.
- Architecture documentation.
- Known limitations.
- Test report.
- Performance report.
- Screenshots.
- Release build.

---

# 54. AGENT EXECUTION RULES

Every AI agent working on this project must follow these rules.

## Rule A — Read before coding

Before implementation:

- Inspect repository.
- Read specification.
- Identify existing work.
- Create architecture notes.

## Rule B — Do not rewrite blindly

Never replace working systems merely because another implementation looks cleaner.

## Rule C — Keep scope controlled

Implement the current phase before jumping into unrelated features.

## Rule D — Test after meaningful changes

Run relevant tests after modifying:

- Audio.
- Lyrics.
- Database.
- Playback.
- State.
- UI interactions.

## Rule E — Self-review

After each phase:

```text
Implementation
    ↓
Tests
    ↓
Manual review
    ↓
Performance check
    ↓
Bug fixing
    ↓
Phase completion
```

## Rule F — Document decisions

Important architecture decisions must be recorded.

---

# 55. AGENT HANDOFF / COMPETITION MODE

Because multiple agents will independently build this project, each agent must produce:

```text
AGENT_REPORT.md
```

Containing:

- What was implemented.
- What was not implemented.
- Architecture decisions.
- Libraries used.
- Why those libraries were selected.
- Known bugs.
- Known limitations.
- Test results.
- Performance results.
- Build instructions.
- Recommended next phase.

The agent must never hide unfinished work.

---

# 56. COMPLETION CHECKLIST

Before calling a phase complete:

```text
[ ] Feature implemented
[ ] No fake functionality
[ ] Tests added
[ ] Tests passing
[ ] Errors handled
[ ] UI reviewed
[ ] Performance checked
[ ] Documentation updated
[ ] Known limitations documented
```

---

# 57. FINAL QUALITY GATE

The finished application should answer "yes" to these questions:

### Performance

- Does it start quickly?
- Does playback remain smooth?
- Does the UI remain responsive?
- Does lyrics synchronization remain accurate?

### Lyrics

- Are lyrics synchronized?
- Is word-level timing supported?
- Does seeking work?
- Does auto-scroll work?
- Do animations remain synchronized?

### Design

- Does the app look intentional?
- Does it have a coherent visual identity?
- Are animations tasteful?
- Does artwork integrate into the UI?
- Does it avoid looking like a generic framework demo?

### Reliability

- Can it survive malformed media?
- Can it handle missing files?
- Can it recover from errors?
- Does data persist correctly?

### UX

- Can the user quickly find music?
- Can the user control playback without thinking?
- Can the user quickly reach lyrics?
- Does the interface feel fast?

---

# 58. OPTIONAL FUTURE FEATURES

Do not implement these before the core system is stable.

Potential future phases:

- Online lyrics providers.
- Lyrics editing.
- Manual timestamp correction.
- Automatic lyrics synchronization.
- Audio visualizer.
- Equalizer.
- ReplayGain.
- Crossfade.
- Gapless playback improvements.
- Smart playlists.
- Statistics.
- Listening history visualization.
- Multiple themes.
- Custom themes.
- Plugin system.
- Remote control.
- Mobile companion.
- Local network control.

---

# 59. IMPORTANT PRODUCT PRINCIPLE

The application is not successful because it has many features.

It is successful if:

> Playing music feels effortless, the interface feels beautiful, and the lyrics feel physically connected to the song.

Prioritize:

```text
Playback reliability
        ↓
Lyrics accuracy
        ↓
Performance
        ↓
Motion quality
        ↓
Visual polish
        ↓
Extra features
```

Do not sacrifice the first four for feature count.

---

# 60. DEFINITION OF DONE

The project is considered production-ready when:

1. A user can import a real music library.
2. Music plays reliably.
3. Library metadata is correctly indexed.
4. Search is fast.
5. Playlists and favorites persist.
6. Now Playing is polished.
7. Lyrics load correctly.
8. Line-level synchronization works.
9. Word-level synchronization works when timing data exists.
10. Lyrics seeking works.
11. Kinetic lyric animations remain synchronized.
12. Artwork transitions are smooth.
13. The application handles errors without crashing.
14. Performance has been profiled.
15. Tests pass.
16. Documentation exists.
17. Installable builds exist for the selected target platforms.

---

# 61. AGENT'S FIRST TASK

Before writing significant application code, the agent must create:

```text
ARCHITECTURE.md
```

This document must explain:

- Selected Rust UI framework.
- Audio library/backend.
- Database.
- Metadata parser.
- Lyrics parser.
- Rendering strategy.
- Animation strategy.
- State management.
- Caching.
- Platform strategy.
- Testing strategy.
- Why each major dependency was chosen.
- Alternatives considered.

Then begin with the smallest working vertical slice.

---

# 62. VERTICAL SLICE PRINCIPLE

Do not build the entire application layer-by-layer without ever testing the complete flow.

A preferred early vertical slice is:

```text
Select music file
      ↓
Read metadata
      ↓
Load audio
      ↓
Play audio
      ↓
Load lyrics
      ↓
Read timestamp
      ↓
Display current lyric
      ↓
Synchronize with playback
```

Once this works, expand the system.

---

# END OF SPECIFICATION

---

# 63. ADDENDUM (takes priority over earlier sections where they conflict)

## 63.1 Locked decisions
- Product: personal, local-first desktop music player. Working name: Aurora Player.
- Daily-use targets: (1) Fedora Linux desktop (Wayland session, PipeWire audio), then (2) Android phone. Windows/macOS must not be deliberately broken, but are not verified targets.
- All core logic in Rust (see section 2). No webview-based shell that moves the logic out of Rust.

## 63.2 Where lyrics come from (the original spec does not define this)
The app must NOT depend on the internet. Lyrics are discovered locally, in this priority order:
1. Sidecar file next to the audio file with the same base name: `.ttml`, `.lrc`, `.txt` (also accept the internal JSON format below).
2. Embedded tags: ID3 USLT/SYLT, Vorbis/FLAC `LYRICS` / `SYNCEDLYRICS`, MP4 lyrics atom.
3. Lyrics cache in the database (previously imported).
4. Nothing found -> elegant "Lyrics unavailable" state.

Supported formats, in order of precision:
- Plain text (static display only).
- Line LRC: `[mm:ss.xx]text`, including multiple timestamps per line and `[offset:+/-ms]`.
- Enhanced LRC (word level): `[mm:ss.xx]<mm:ss.xx>word <mm:ss.xx>word`.
- TTML with word/syllable spans (stretch goal, document if skipped).
- Internal JSON: `{lines:[{start,end,words:[{text,start,end}]}]}`; every parser must output this one internal model.

Every lyric line and word carries `timing_source`: `Exact` (from file) or `Estimated`.

## 63.3 Estimated word timing (so word-by-word animation works with plain line LRC)
When a line has timestamps but no word timestamps:
- Sung window = line.start to min(line.end, next_line.start - 120ms). If no end is known, cap at a sane maximum.
- Split the window across words proportionally to grapheme count (+1 per word as a minimum weight).
- Mark every such word `Estimated`.
- Settings: "Estimated word animation" ON / OFF (default ON). When OFF, only line-level highlight is used for Estimated lines.
- Never present Estimated timing as exact in the UI or in reports.
- For Arabic/RTL and CJK text, split by Unicode word boundaries (UAX #29), not by spaces only; document the limitation for CJK.

## 63.4 Audio clock (clarifies sections 15, 42, 43)
Lyrics follow the audio clock, but polling the device position directly gives stair-stepped values. Required model:
- The audio thread publishes `(playback_position, monotonic_instant)` atomically (lock-free or very short critical section).
- The UI/lyrics side computes `position = anchor_position + (now - anchor_instant) * playback_rate - output_latency`, and re-anchors on every new publish.
- Position is monotonic during normal playback and only jumps backwards/forwards after an explicit seek.
- A free-running UI timer that is never re-anchored to the audio clock is forbidden.
- Add a debug overlay (toggle with a shortcut) showing audio position, interpolated position, drift in ms, and frame time.

## 63.5 Phase 0 is gated by a mandatory UI spike
Before choosing the UI stack, build a throwaway prototype for EACH candidate (at least 2) that renders:
- One line mixing Arabic and English, with per-word highlight advancing on a 60 FPS timeline.
- Scale + opacity + blur (or the closest the framework supports) on the active word.
- A scrolling list of 30+ lines with animated auto-follow.
- Proper Arabic shaping and RTL ordering (verify visually, with a screenshot).
Measure CPU %, GPU % if available, and frame-time p95 over 60 seconds. Put the results table in ARCHITECTURE.md and choose based on the data. If no candidate renders Arabic correctly, that candidate is rejected. Also run the spike under a Wayland session on Fedora, and apply the Android extension in 63.9.

## 63.6 Execution order
Phase numbers stay as defined in section 53, but the recommended run order is:
0, 1, 2, 6, 7, 3, 5, 8, 4, 9, 10, 11, 12, 13, 14
Reason: prove audio + word-synced lyrics + the chosen UI stack end to end before investing in library, playlists and polish.
Phases 6 and 7 may use a single hardcoded/selected file instead of a library until Phase 3 lands.

## 63.7 Per-phase protocol
At the end of every phase the agent must:
1. Run `cargo fmt`, `cargo clippy` (no new warnings) and `cargo test`.
2. Append to `PHASE_LOG.md`: what was done, what was skipped and why, test results, measurements, known bugs.
3. Update `AGENT_REPORT.md`.
4. Commit with message `phase-N: <summary>`.
5. STOP and wait. Do not start the next phase.

## 63.8 Honest reporting
Never claim a feature works unless it was run. Mark anything unverified as "unverified". Include real measurements, not estimates.

## 63.9 Android (second target, built after the desktop app is stable)
Android affects the architecture from day one, even though it is built later.

Portability rules (apply from Phase 0):
- Core crates (audio, lyrics, library, queue, database, settings) must not depend on any UI crate or on desktop-only APIs.
- Audio output sits behind a trait. File access sits behind a storage abstraction (Android uses scoped storage / content URIs, not plain paths). Config/data/cache directories come from an abstraction.
- Core state must not assume mouse or keyboard. Every interaction has a touch equivalent (tap a lyric to seek, swipe between songs, mini player).
- Lyrics, estimated timing, audio clock (63.4) and the motion system live in core crates so both platforms share them.

Phase 0 spike extension:
- For each UI candidate, also try to build and run the lyrics spike on an Android device or emulator, or document precisely why it is not possible.
- Evaluate two strategies and record the decision in ARCHITECTURE.md with measurements:
  - Strategy A: one Rust UI codebase for desktop and Android.
  - Strategy B: shared Rust core exposed through FFI (for example UniFFI) with a native Kotlin / Jetpack Compose UI on Android.
- If Strategy B wins, all engines stay in Rust and only the Android UI and platform glue are Kotlin.

Rules for Android:
- A WebView-based UI is not acceptable (same rule as the desktop thin-wrapper ban in section 2).
- Thin Kotlin/Java glue is allowed and expected for what Rust cannot do alone: foreground service, media notification and session, audio focus, permissions.
- No fake claims: Android features are only "done" when run on a real device, and the report must say which device and Android version.
