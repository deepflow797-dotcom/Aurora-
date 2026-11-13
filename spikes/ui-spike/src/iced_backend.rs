//! Candidate A: `iced_tiny_skia` 0.14.1 rendered headlessly.
//!
//! This drives iced's *real* renderer: `Renderer::draw` takes a
//! `tiny_skia::PixmapMut`, so a window is not required. Text goes through
//! iced's own pipeline (cosmic-text 0.15 inside iced), which is the whole point
//! of the spike: Arabic shaping and BiDi are the engine's job, not ours.

use std::borrow::Cow;

use iced_tiny_skia::core::text::Renderer as _;
use iced_tiny_skia::core::{
    Background, Color, Font, Pixels, Point, Rectangle, Renderer as _, Size,
    text::{LineHeight, Shaping, Text, Wrapping},
};
use iced_tiny_skia::graphics::Viewport;

use crate::scene::{FrameScene, HEIGHT, WIDTH};

pub struct IcedBackend {
    renderer: iced_tiny_skia::Renderer,
    pixmap: tiny_skia::Pixmap,
    mask: tiny_skia::Mask,
    viewport: Viewport,
}

const BG: Color = Color::from_rgb(0.07, 0.08, 0.11);
const IDLE_RGB: (f32, f32, f32) = (0.62, 0.65, 0.72);
const ACTIVE_RGB: (f32, f32, f32) = (1.0, 0.86, 0.45);

/// `iced_tiny_skia::into_color` swaps the red and blue channels
/// (`from_rgba(color.b, color.g, color.r, color.a)`), so the channels are handed
/// over reversed to get the colour asked for. tiny-skia premultiplies by alpha
/// itself, so the RGB values stay unmultiplied here.
fn rgba(rgb: (f32, f32, f32), alpha: f32) -> Color {
    Color::from_rgba(rgb.2, rgb.1, rgb.0, alpha.clamp(0.0, 1.0))
}

impl IcedBackend {
    pub fn new(font_bytes: &'static [u8]) -> Self {
        // Load the Arabic font into iced's global cosmic-text font system; the
        // renderer picks it up by family name.
        let mut system = iced_tiny_skia::graphics::text::font_system()
            .write()
            .expect("font system");
        system.load_font(Cow::Borrowed(font_bytes));
        drop(system);

        let font = Font::with_name("IBM Plex Sans Arabic");
        let renderer = iced_tiny_skia::Renderer::new(font, Pixels(34.0));
        let pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT).expect("pixmap");
        let mask = tiny_skia::Mask::new(WIDTH, HEIGHT).expect("mask");
        let viewport = Viewport::with_physical_size(Size::new(WIDTH, HEIGHT), 1.0);
        Self {
            renderer,
            pixmap,
            mask,
            viewport,
        }
    }

    pub fn render(&mut self, scene: &FrameScene) {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(WIDTH as f32, HEIGHT as f32));
        self.renderer.reset(bounds);

        // The glow is a stand-in for a real blur: three nested quads with
        // falling alpha, cheap enough to stay off the GPU budget.
        for glow in &scene.glows {
            for (layer, spread) in [(0.0f32, 18.0f32), (1.0, 10.0), (2.0, 3.0)] {
                let pad = spread;
                let alpha = glow.alpha * (0.30 - layer * 0.08);
                self.renderer.fill_quad(
                    quad(
                        glow.x - pad,
                        glow.y - pad,
                        glow.w + pad * 2.0,
                        glow.h + pad * 2.0,
                        14.0 + pad,
                    ),
                    Background::Color(rgba((1.0, 0.72, 0.25), alpha)),
                );
            }
        }

        for word in &scene.words {
            let rgb = if word.active { ACTIVE_RGB } else { IDLE_RGB };
            let text = Text {
                content: word.text.trim_end().to_owned(),
                bounds: Size::new(word.width.max(1.0) + 2.0, word.size * 2.0),
                size: Pixels(word.size),
                line_height: LineHeight::default(),
                font: Font::with_name("IBM Plex Sans Arabic"),
                align_x: iced_tiny_skia::core::text::Alignment::Left,
                align_y: iced_tiny_skia::core::alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::None,
            };
            self.renderer.fill_text(
                text,
                Point::new(word.x.round(), word.y.round()),
                rgba(rgb, word.alpha),
                bounds,
            );
        }

        let mut pixmap = self.pixmap.as_mut();
        self.renderer
            .draw(&mut pixmap, &mut self.mask, &self.viewport, &[bounds], BG);
    }

    /// Draws whole lines as single text runs, with no pre-computed word boxes.
    /// This isolates each engine's own BiDi handling from our layout code.
    pub fn render_probe(&mut self, lines: &[&str]) {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(WIDTH as f32, HEIGHT as f32));
        self.renderer.reset(bounds);
        for (row, line) in lines.iter().enumerate() {
            let text = Text {
                content: (*line).to_owned(),
                bounds: Size::new(WIDTH as f32 - 160.0, 120.0),
                size: Pixels(34.0),
                line_height: LineHeight::default(),
                font: Font::with_name("IBM Plex Sans Arabic"),
                align_x: iced_tiny_skia::core::text::Alignment::Left,
                align_y: iced_tiny_skia::core::alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::None,
            };
            self.renderer.fill_text(
                text,
                Point::new(80.0, 120.0 + row as f32 * 130.0),
                rgba(ACTIVE_RGB, 1.0),
                bounds,
            );
        }
        let mut pixmap = self.pixmap.as_mut();
        self.renderer
            .draw(&mut pixmap, &mut self.mask, &self.viewport, &[bounds], BG);
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }
}

fn quad(x: f32, y: f32, w: f32, h: f32, radius: f32) -> iced_tiny_skia::core::renderer::Quad {
    iced_tiny_skia::core::renderer::Quad {
        bounds: Rectangle::new(Point::new(x, y), Size::new(w, h)),
        border: iced_tiny_skia::core::Border {
            radius: radius.into(),
            ..Default::default()
        },
        shadow: iced_tiny_skia::core::Shadow::default(),
        snap: false,
    }
}
