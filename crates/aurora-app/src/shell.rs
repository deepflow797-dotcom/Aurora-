//! The windowed shell: iced 0.14 on the tiny-skia renderer.
//!
//! Phase 1 scope is deliberately narrow - a real window, a real settings
//! surface, real persistence:
//!
//! * three panes (Now Playing / Library / Settings) with a sidebar,
//! * the §27 settings rendered as live controls, saved to disk on change,
//! * a status line that says whether the last write succeeded, with the path.
//!
//! It is **not** the lyric view. `aurora-ui` owns that view model and the iced
//! adapter that draws it lands in Phase 3, so this file contains no lyric
//! rendering and no audio. Keeping the shell dumb is what lets the Phase 0
//! measurements stay valid: nothing here touches the hot path yet.

use std::path::PathBuf;

use iced::widget::{
    button, column, container, pick_list, row, rule, scrollable, slider, space, text, toggler,
};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use tracing::{error, info};

use aurora_platform::settings::{AnimationProfile, RepeatMode, Settings};

use crate::theme;

/// Panes reachable from the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    NowPlaying,
    Library,
    Settings,
}

impl Pane {
    const ALL: [Self; 3] = [Self::NowPlaying, Self::Library, Self::Settings];

    /// Human-readable pane name, used by the sidebar and the screenshot test.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::NowPlaying => "Now Playing",
            Self::Library => "Library",
            Self::Settings => "Settings",
        }
    }
}

/// Result of the last save attempt, shown under the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Saved,
    Failed(String),
}

/// Application state.
pub struct Shell {
    settings: Settings,
    path: PathBuf,
    pane: Pane,
    status: Status,
    /// Set when `--demo`-style diagnostics want the settings file location
    /// visible; always true in this build.
    show_path: bool,
}

impl Shell {
    #[must_use]
    pub fn new(settings: Settings, path: PathBuf) -> Self {
        Self {
            settings,
            path,
            pane: Pane::NowPlaying,
            status: Status::Unchanged,
            show_path: true,
        }
    }

    /// Window title.
    #[must_use]
    pub fn title(&self) -> String {
        format!("Aurora {}", env!("CARGO_PKG_VERSION"))
    }

    /// Maps the stored theme preference onto an iced theme.
    ///
    /// `System` yields `None`, which tells iced to follow the desktop's
    /// colour-scheme preference instead of guessing here.
    #[must_use]
    pub fn theme(&self) -> Option<Theme> {
        match self.settings.appearance.theme {
            aurora_platform::Theme::System => None,
            aurora_platform::Theme::Dark => Some(Theme::Dark),
            aurora_platform::Theme::Light => Some(Theme::Light),
        }
    }

    /// The settings currently in memory (read-only).
    #[must_use]
    #[cfg(test)]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Mutable settings, for tests that need to force a value.
    #[must_use]
    #[cfg(test)]
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Path of the settings file this shell writes.
    #[must_use]
    #[cfg(test)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The theme the window actually uses: the preference, or the dark theme
    /// when it follows the system (a window manager's preference is not visible
    /// to a headless render, and guessing light would be worse).
    #[must_use]
    #[cfg(test)]
    pub fn resolved_theme(&self) -> Theme {
        self.theme().unwrap_or(Theme::Dark)
    }

    fn touch(&mut self) {
        self.status = Status::Unchanged;
    }

    fn persist(&mut self) -> Task<Message> {
        match self.settings.save(&self.path) {
            Ok(()) => {
                info!(path = %self.path.display(), "settings saved");
                self.status = Status::Saved;
            }
            Err(err) => {
                error!(path = %self.path.display(), %err, "cannot save settings");
                self.status = Status::Failed(err.to_string());
            }
        }
        Task::none()
    }
}

/// Messages the shell reacts to.
#[derive(Debug, Clone)]
pub enum Message {
    Select(Pane),
    Volume(f32),
    Repeat(RepeatMode),
    Shuffle(bool),
    Resume(bool),
    LyricsEnabled(bool),
    WordHighlighting(bool),
    AutoScroll(bool),
    FontScale(f32),
    Animation(AnimationProfile),
    ReducedMotion(bool),
    GpuEffects(bool),
    Save,
    Revert,
}

impl Shell {
    /// Applies a message. Every settings change saves immediately: the file is
    /// a few hundred bytes and losing a preference is worse than an extra
    /// `write` + `rename`.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Select(pane) => {
                self.pane = pane;
                return Task::none();
            }
            Message::Save => return self.persist(),
            Message::Revert => {
                match Settings::load_or_create(&self.path) {
                    Ok((settings, _)) => {
                        self.settings = settings;
                        self.status = Status::Unchanged;
                    }
                    Err(err) => self.status = Status::Failed(err.to_string()),
                }
                return Task::none();
            }
            Message::Volume(value) => self.settings.playback.volume = value,
            Message::Repeat(mode) => self.settings.playback.repeat = mode,
            Message::Shuffle(on) => self.settings.playback.shuffle = on,
            Message::Resume(on) => self.settings.playback.resume = on,
            Message::LyricsEnabled(on) => self.settings.lyrics.enabled = on,
            Message::WordHighlighting(on) => self.settings.lyrics.word_highlighting = on,
            Message::AutoScroll(on) => self.settings.lyrics.auto_scroll = on,
            Message::FontScale(scale) => self.settings.lyrics.font_scale = scale,
            Message::Animation(profile) => self.settings.lyrics.animation = profile,
            Message::ReducedMotion(on) => self.settings.performance.reduced_motion = on,
            Message::GpuEffects(on) => self.settings.performance.gpu_effects = on,
        }
        self.touch();
        self.persist()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = column(Pane::ALL.map(|pane| {
            let selected = pane == self.pane;
            button(text(pane.label()).width(Length::Fill))
                .width(Length::Fill)
                .padding(Padding::from([8, 12]))
                .on_press(Message::Select(pane))
                .style(move |theme: &Theme, status| {
                    let mut style = button::Style::default();
                    if selected || status == button::Status::Hovered {
                        style.background =
                            Some(theme.extended_palette().background.weak.color.into());
                    }
                    style.text_color = theme.palette().text;
                    style
                })
                .into()
        }))
        .spacing(4)
        .padding(Padding::from([16, 12]))
        .width(Length::Fixed(200.0));

        let content = match self.pane {
            Pane::NowPlaying => self.now_playing(),
            Pane::Library => self.library(),
            Pane::Settings => self.settings_pane(),
        };

        row![
            container(sidebar).style(container::dark),
            container(scrollable(container(content).padding(24))).width(Length::Fill)
        ]
        .height(Length::Fill)
        .into()
    }

    fn now_playing(&self) -> Element<'_, Message> {
        let accent = self.accent_color();
        column![
            text("Now Playing").size(26),
            space().height(12),
            container(text("No track loaded").size(16))
                .padding(24)
                .width(Length::Fill)
                .style(container::rounded_box),
            space().height(20),
            text(
                "Playback arrives in Phase 2 (decoder + output) and the lyric view in Phase 3. \
                 The pane exists now so the window geometry is settled before either lands."
            ),
            space().height(16),
            row![
                container(space())
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(28.0))
                    .style(move |_| container::Style {
                        background: Some(accent.into()),
                        border: iced::border::rounded(6),
                        ..container::Style::default()
                    }),
                text(format!(
                    "accent #{:02x}{:02x}{:02x}",
                    self.settings.appearance.accent[0],
                    self.settings.appearance.accent[1],
                    self.settings.appearance.accent[2]
                ))
                .size(13)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .into()
    }

    fn library(&self) -> Element<'_, Message> {
        let folders = &self.settings.library.music_folders;
        let mut body = column![text("Library").size(26), space().height(12)].spacing(6);
        if folders.is_empty() {
            body = body.push(text(
                "No music folders configured. Add them to settings.toml for now; the folder \
                 picker lands with the library pane in Phase 4.",
            ));
        } else {
            for folder in folders {
                body = body.push(text(format!("• {}", folder.display())));
            }
        }
        body.into()
    }

    fn settings_pane(&self) -> Element<'_, Message> {
        let s = &self.settings;
        let accent = s.appearance.accent;
        let status = match &self.status {
            Status::Unchanged => text("").size(12),
            Status::Saved => text("Saved").size(12),
            Status::Failed(err) => text(format!("Not saved: {err}")).size(12),
        };
        let path_line = if self.show_path {
            text(format!("file: {}", self.path.display())).size(12)
        } else {
            text("").size(12)
        };

        let body = column![
            text("Settings").size(26),
            space().height(8),
            section("Playback"),
            labelled(
                "Volume",
                slider(0.0..=1.0, s.playback.volume, Message::Volume).step(0.01_f32),
            ),
            labelled(
                "Repeat",
                pick_list(&REPEAT_MODES[..], Some(s.playback.repeat), Message::Repeat),
            ),
            labelled(
                "Shuffle",
                toggler(s.playback.shuffle)
                    .on_toggle(Message::Shuffle)
                    .style(theme::accent_toggler(accent))
            ),
            labelled(
                "Resume last track",
                toggler(s.playback.resume)
                    .on_toggle(Message::Resume)
                    .style(theme::accent_toggler(accent))
            ),
            rule::horizontal(1),
            section("Lyrics"),
            labelled(
                "Show lyrics",
                toggler(s.lyrics.enabled)
                    .on_toggle(Message::LyricsEnabled)
                    .style(theme::accent_toggler(accent))
            ),
            labelled(
                "Word highlighting",
                toggler(s.lyrics.word_highlighting)
                    .on_toggle(Message::WordHighlighting)
                    .style(theme::accent_toggler(accent))
            ),
            labelled(
                "Auto-scroll",
                toggler(s.lyrics.auto_scroll)
                    .on_toggle(Message::AutoScroll)
                    .style(theme::accent_toggler(accent))
            ),
            labelled(
                "Font scale",
                slider(0.5..=2.0, s.lyrics.font_scale, Message::FontScale).step(0.05_f32)
            ),
            labelled(
                "Animation",
                pick_list(
                    &ANIMATION_PROFILES[..],
                    Some(s.lyrics.animation),
                    Message::Animation
                )
            ),
            rule::horizontal(1),
            section("Performance"),
            labelled(
                "Reduced motion",
                toggler(s.performance.reduced_motion)
                    .on_toggle(Message::ReducedMotion)
                    .style(theme::accent_toggler(accent))
            ),
            labelled(
                "GPU effects",
                toggler(s.performance.gpu_effects)
                    .on_toggle(Message::GpuEffects)
                    .style(theme::accent_toggler(accent))
            ),
            space().height(12),
            row![
                button("Save now")
                    .on_press(Message::Save)
                    .style(theme::primary_button(accent)),
                button("Revert to file").on_press(Message::Revert),
            ]
            .spacing(8),
            space().height(8),
            status,
            path_line,
        ]
        .spacing(10);

        body.into()
    }

    fn accent_color(&self) -> Color {
        let [r, g, b] = self.settings.appearance.accent;
        Color::from_rgb8(r, g, b)
    }
}

const REPEAT_MODES: [RepeatMode; 3] = [RepeatMode::Off, RepeatMode::All, RepeatMode::One];
const ANIMATION_PROFILES: [AnimationProfile; 4] = [
    AnimationProfile::None,
    AnimationProfile::Subtle,
    AnimationProfile::Standard,
    AnimationProfile::Expressive,
];

fn section(title: &'static str) -> Element<'static, Message> {
    text(title).size(16).into()
}

/// One settings row: label on the left, control on the right.
fn labelled<'a>(label: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![text(label).width(Length::Fixed(190.0)), control.into()]
        .align_y(Alignment::Center)
        .spacing(12)
        .into()
}

/// Runs the window. Returns `Err` when iced cannot open one, which the caller
/// turns into the headless path.
///
/// # Errors
///
/// Propagates iced's window/graphics initialisation error.
pub fn run(settings: Settings, path: PathBuf) -> Result<(), iced::Error> {
    // Size is fixed at the spike's 1280x800 minus chrome so measurements stay
    // comparable; `min_size` keeps the sidebar from collapsing on tiling WMs.
    iced::application(
        move || Shell::new(settings.clone(), path.clone()),
        Shell::update,
        Shell::view,
    )
    .title(Shell::title)
    .theme(Shell::theme)
    .window_size((1180.0, 760.0))
    .window(iced::window::Settings {
        min_size: Some(iced::Size::new(720.0, 480.0)),
        ..iced::window::Settings::default()
    })
    .antialiasing(true)
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One directory per test: tests run in parallel and delete what they make.
    fn shell(name: &str) -> Shell {
        let dir = std::env::temp_dir().join(format!("aurora-shell-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        Shell::new(Settings::default(), dir.join("settings.toml"))
    }

    #[test]
    fn title_and_theme_follow_settings() {
        let mut shell = shell("theme");
        assert!(shell.title().starts_with("Aurora"));
        assert_eq!(shell.theme(), None, "System follows the desktop");

        shell.settings.appearance.theme = aurora_platform::Theme::Dark;
        assert_eq!(shell.theme(), Some(Theme::Dark));
        shell.settings.appearance.theme = aurora_platform::Theme::Light;
        assert_eq!(shell.theme(), Some(Theme::Light));
    }

    #[test]
    fn changing_a_setting_persists_it() {
        let mut shell = shell("persist");
        let path = shell.path.clone();
        let _ = shell.update(Message::Shuffle(true));
        assert!(shell.settings.playback.shuffle);
        assert_eq!(shell.status, Status::Saved);
        let text = std::fs::read_to_string(&path).expect("settings written");
        assert!(text.contains("shuffle = true"), "{text}");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn revert_reloads_from_disk() {
        let mut shell = shell("revert");
        let _ = shell.update(Message::Volume(0.2));
        assert!((shell.settings.playback.volume - 0.2).abs() < f32::EPSILON);

        // Change the file behind the app's back, then revert.
        let mut other = Settings::default();
        other.playback.volume = 0.9;
        other.save(&shell.path).expect("save");
        let _ = shell.update(Message::Revert);
        assert!((shell.settings.playback.volume - 0.9).abs() < f32::EPSILON);
        std::fs::remove_dir_all(shell.path.parent().unwrap()).ok();
    }

    #[test]
    fn a_bad_path_reports_failure_instead_of_panicking() {
        let mut shell = shell("bad-path");
        shell.path = PathBuf::from("/proc/definitely/not/writable/settings.toml");
        let _ = shell.update(Message::Shuffle(true));
        assert!(
            matches!(shell.status, Status::Failed(_)),
            "{:?}",
            shell.status
        );
        std::fs::remove_dir_all(shell.path.parent().unwrap()).ok();
    }

    #[test]
    fn accent_colour_round_trips() {
        let mut shell = shell("accent");
        shell.settings.appearance.accent = [0x10, 0x20, 0x30];
        assert_eq!(shell.accent_color(), Color::from_rgb8(0x10, 0x20, 0x30));
    }
}
