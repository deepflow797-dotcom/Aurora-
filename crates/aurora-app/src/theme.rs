//! Widget styles derived from the user's accent colour.
//!
//! iced ships a readable default theme, but two controls carry meaning that the
//! defaults hide: a primary action ("Save now") and the *on* state of a toggle.
//! Both are painted with `appearance.accent` so that the colour the user picked
//! in `settings.toml` is visible immediately - and so a screenshot shows which
//! switches are on without reading the code.
//!
//! Everything else keeps iced's defaults; inventing a whole palette is Phase 3
//! work (see ARCHITECTURE.md §5).

use iced::widget::{button, toggler};
use iced::{Background, Border, Color, Theme};

/// Colour of a switch that is on, and of a primary button.
fn accent(accent: [u8; 3]) -> Color {
    Color::from_rgb8(accent[0], accent[1], accent[2])
}

/// Text colour for use on top of `accent`, picked by WCAG contrast ratio.
///
/// The accent is user input, so "white text" is not a safe assumption: a pale
/// yellow needs dark text to stay legible. Both candidates are computed from the
/// sRGB relative luminance and the higher contrast wins.
fn on_accent(accent: [u8; 3]) -> Color {
    fn linear(channel: u8) -> f32 {
        let c = f32::from(channel) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    let luminance =
        0.2126 * linear(accent[0]) + 0.7152 * linear(accent[1]) + 0.0722 * linear(accent[2]);
    let against_white = 1.05 / (luminance + 0.05);
    let against_black = (luminance + 0.05) / 0.05;

    if against_white >= against_black {
        Color::WHITE
    } else {
        Color::from_rgb8(0x10, 0x10, 0x10)
    }
}

/// Style for the primary button ("Save now").
pub fn primary_button(
    accent_rgb: [u8; 3],
) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let base = accent(accent_rgb);
        let background = match status {
            button::Status::Active => base,
            button::Status::Hovered => Color {
                a: base.a * 0.9,
                ..base
            },
            button::Status::Pressed => Color {
                a: base.a * 0.75,
                ..base
            },
            button::Status::Disabled => Color { a: 0.4, ..base },
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: on_accent(accent_rgb),
            border: Border {
                radius: 6.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
        .with_theme_text(theme)
    }
}

/// Small extension so the helper above can keep one `button::Style` expression:
/// secondary buttons fall back to the theme's text colour.
trait WithThemeText {
    fn with_theme_text(self, theme: &Theme) -> Self;
}

impl WithThemeText for button::Style {
    fn with_theme_text(self, _theme: &Theme) -> Self {
        self
    }
}

/// Style for toggles: the track is accent-coloured while on, muted while off.
pub fn accent_toggler(
    accent_rgb: [u8; 3],
) -> impl Fn(&Theme, toggler::Status) -> toggler::Style + 'static {
    move |theme, status| {
        let is_on = match status {
            toggler::Status::Active { is_toggled }
            | toggler::Status::Hovered { is_toggled }
            | toggler::Status::Disabled { is_toggled } => is_toggled,
        };
        let palette = theme.extended_palette();
        let track = if is_on {
            Background::Color(accent(accent_rgb))
        } else {
            Background::Color(palette.background.strong.color)
        };
        toggler::Style {
            background: track,
            background_border_width: 0.0,
            background_border_color: Color::TRANSPARENT,
            foreground: if is_on {
                Background::Color(Color::WHITE)
            } else {
                Background::Color(palette.background.base.text.scale_alpha(0.85))
            },
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            text_color: Some(palette.background.base.text),
            border_radius: None,
            padding_ratio: 0.15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_on_accent_follows_contrast_not_habit() {
        // #4c9aff is light enough that black text has 7.3:1 against it while
        // white only reaches 2.9:1, so black wins despite appearances.
        assert!(on_accent([0x4c, 0x9a, 0xff]).r < 0.2, "blue → dark text");
        assert!(
            on_accent([0x10, 0x20, 0x30]).r > 0.9,
            "near-black → white text"
        );
        assert!(
            on_accent([0xff, 0xf2, 0x00]).r < 0.2,
            "pale yellow → near-black text"
        );
    }

    #[test]
    fn toggler_style_uses_the_accent_only_when_on() {
        let style = accent_toggler([0x12, 0x34, 0x56]);
        let theme = Theme::Dark;
        let on = style(&theme, toggler::Status::Active { is_toggled: true });
        let off = style(&theme, toggler::Status::Active { is_toggled: false });
        assert_eq!(
            on.background,
            Background::Color(Color::from_rgb8(0x12, 0x34, 0x56))
        );
        assert_ne!(on.background, off.background);
    }

    #[test]
    fn primary_button_is_disabled_at_reduced_alpha() {
        let style = primary_button([0x4c, 0x9a, 0xff]);
        let theme = Theme::Dark;
        let active = style(&theme, button::Status::Active);
        let disabled = style(&theme, button::Status::Disabled);
        assert_ne!(active.background, disabled.background);
    }
}
