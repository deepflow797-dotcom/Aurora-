//! Headless rendering of the shell, so its layout can be checked without a
//! compositor.
//!
//! This is the Phase 0 spike's trick, applied to the real shell: iced's layout
//! and draw passes are pure enough to run against a `tiny_skia` pixmap. The
//! window path is `winit` + the same two passes, so a bug found here is a bug
//! the user would have seen.
//!
//! Compiled only for tests (`#[cfg(test)]` in `main.rs`). Output goes to
//! `target/shots/`, which is git-ignored; copy a picture into the docs when it is
//! worth keeping.

use std::borrow::Cow;
use std::path::PathBuf;

use iced::advanced::graphics::Viewport;
use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::Style;
use iced::mouse;
use iced::{Font, Pixels, Rectangle, Size};
use iced_runtime::UserInterface;
use iced_runtime::user_interface::Cache;
use tiny_skia::{Mask, Pixmap};

use crate::shell::Shell;

/// Bundled UI fonts, so text renders even on a machine with no fontconfig.
const FONTS: [&[u8]; 3] = [
    include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../../../assets/fonts/IBMPlexSansArabic-Regular.ttf"),
    include_bytes!("../../../assets/fonts/IBMPlexSansArabic-Bold.ttf"),
];

fn load_fonts() {
    let mut system = font_system().write().expect("font system");
    for bytes in FONTS {
        system.load_font(Cow::Borrowed(bytes));
    }
}

/// Renders `shell` at `width`x`height` into a pixmap, at `scale` pixels per
/// logical pixel.
///
/// The base fill is the theme's background colour, which is what the real
/// compositor clears to; passing a fixed colour here would hide the theme
/// entirely (and did, in the first version of this file).
#[must_use]
pub fn render_at(shell: &Shell, width: u32, height: u32, scale: f32) -> Pixmap {
    load_fonts();

    let mut renderer = iced::Renderer::new(Font::with_name("IBM Plex Sans"), Pixels(16.0));
    let bounds = Size::new(width as f32, height as f32);
    let mut ui = UserInterface::build(shell.view(), bounds, Cache::default(), &mut renderer);

    let theme = shell.resolved_theme();
    ui.draw(
        &mut renderer,
        &theme,
        &Style::default(),
        mouse::Cursor::Unavailable,
    );

    let physical = Size::new(
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    );
    let mut pixmap = Pixmap::new(physical.width, physical.height).expect("pixmap");
    let mut mask = Mask::new(physical.width, physical.height).expect("mask");
    let viewport = Viewport::with_physical_size(physical, scale);
    renderer.draw(
        &mut pixmap.as_mut(),
        &mut mask,
        &viewport,
        &[Rectangle::with_size(bounds)],
        theme.palette().background,
    );
    pixmap
}

/// Renders at 1:1.
#[must_use]
pub fn render(shell: &Shell, width: u32, height: u32) -> Pixmap {
    render_at(shell, width, height, 1.0)
}

/// Where a shot for `name` is written.
#[must_use]
pub fn output_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/shots");
    std::fs::create_dir_all(&dir).expect("shots directory");
    dir.join(format!("{name}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{Message, Pane};
    use aurora_platform::settings::Settings;

    fn shell() -> Shell {
        let dir = std::env::temp_dir().join(format!("aurora-shot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut settings = Settings::default();
        settings.library.music_folders = vec![PathBuf::from("/home/user/Music")];
        Shell::new(settings, dir.join("settings.toml"))
    }

    /// Distinct colours in the pixmap: a blank or single-colour render is a bug,
    /// not a small image.
    fn distinct_colours(pixmap: &Pixmap) -> usize {
        let mut seen = std::collections::HashSet::new();
        for pixel in pixmap.pixels() {
            seen.insert((pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()));
        }
        seen.len()
    }

    #[test]
    fn every_pane_renders_with_real_content() {
        let mut shell = shell();
        for pane in [Pane::NowPlaying, Pane::Library, Pane::Settings] {
            let _ = shell.update(Message::Select(pane));
            let pixmap = render(&shell, 1180, 760);
            let colours = distinct_colours(&pixmap);
            assert!(
                colours > 50,
                "{} pane rendered only {colours} distinct colours",
                pane.label()
            );
            let path = output_path(&format!("shell-{:?}", pane).to_lowercase());
            pixmap.save_png(&path).expect("save png");
            assert!(path.exists());

            // A 2x zoom of the settings pane is what to look at when a control
            // "looks wrong": at 1:1 a toggler is 36x14 pixels and the knob's
            // side is impossible to judge.
            if pane == Pane::Settings {
                render_at(&shell, 1180, 760, 2.0)
                    .save_png(output_path("shell-settings@2x"))
                    .expect("save zoomed png");
            }
        }
        std::fs::remove_dir_all(shell.path().parent().unwrap()).ok();
    }

    #[test]
    fn toggled_and_untoggled_settings_render_differently() {
        let mut on = shell();
        let _ = on.update(Message::Select(Pane::Settings));
        let mut off_settings = Settings::default();
        off_settings.lyrics.enabled = false;
        off_settings.lyrics.word_highlighting = false;
        off_settings.lyrics.auto_scroll = false;
        off_settings.performance.gpu_effects = false;
        off_settings.library.music_folders = on.settings().library.music_folders.clone();
        let mut off = Shell::new(
            off_settings,
            on.path().parent().unwrap().join("off-settings.toml"),
        );
        let _ = off.update(Message::Select(Pane::Settings));

        let a = render(&on, 1180, 760);
        let b = render(&off, 1180, 760);
        assert_ne!(
            a.data(),
            b.data(),
            "toggler state is not reaching the renderer"
        );
        std::fs::remove_dir_all(on.path().parent().unwrap()).ok();
    }

    /// Characterisation test for an **upstream** iced 0.14 bug, not ours:
    /// `iced_tiny_skia`'s `into_color` calls
    /// `tiny_skia::Color::from_rgba(color.b, color.g, color.r, alpha)`, so every
    /// software-rendered pixel has red and blue swapped (see
    /// `tiny_skia/src/engine.rs`, still present on iced master). The wgpu backend
    /// does not do this, which is why the app does **not** compensate: doing so
    /// would make the software build look right and the real GPU build wrong.
    ///
    /// If this test ever fails, iced fixed it: delete the caveat in PHASE_LOG.md
    /// and re-take the screenshots in `docs/evidence/`.
    #[test]
    fn red_and_blue_are_swapped_by_the_software_renderer() {
        let mut on = shell();
        let _ = on.update(Message::Select(Pane::Settings));
        on.settings_mut().appearance.accent = [255, 0, 0];

        let pixmap = render(&on, 1180, 760);
        // "Show lyrics" is on by default; its track is painted with the accent.
        let (mut red, mut blue) = (0, 0);
        for y in 294..312 {
            for x in 422..464 {
                let pixel = pixmap.pixel(x, y).expect("pixel in bounds");
                if pixel.red() > 200 && pixel.blue() < 60 {
                    red += 1;
                }
                if pixel.blue() > 200 && pixel.red() < 60 {
                    blue += 1;
                }
            }
        }
        assert!(
            blue > red,
            "expected the software renderer's R/B swap (red={red}, blue={blue})"
        );
        std::fs::remove_dir_all(on.path().parent().unwrap()).ok();
    }

    #[test]
    fn rendering_is_deterministic() {
        let shell = shell();
        let first = render(&shell, 640, 400);
        let second = render(&shell, 640, 400);
        assert_eq!(first.data(), second.data());
        std::fs::remove_dir_all(shell.path().parent().unwrap()).ok();
    }
}
