//! The benchmark scene: a lyric viewport with Arabic + English text.
//!
//! Both candidates render exactly this scene, and the "what to draw" decision is
//! made once, by `aurora-ui` (the shipping view model) plus `cosmic-text` for
//! layout metrics. The backends only turn [`FrameScene`] into pixels, so a
//! frame-time difference really is a renderer difference.

use std::collections::HashMap;
use std::time::Instant;

use aurora_core::clock::ClockSnapshot;
use aurora_core::lyrics::{LyricDocument, LyricSource, Millis};
use aurora_lyrics::{estimate_word_timing, parse_lrc};
use aurora_ui::{LyricView, WordPlan};

/// Hero line: mixed Arabic + English, the exact string from the spec.
pub const HERO_LINE: &str = "سَلامٌ على الدُّنيا Hello Aurora في الظَّلام";

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 800;
pub const FONT_SIZE: f32 = 34.0;
pub const LINE_HEIGHT: f32 = 96.0;

/// Horizontal breathing room so an RTL run never touches the buffer edge.
pub const MARGIN: f32 = 80.0;

/// The family both the layout engine and the renderers use.
pub const FAMILY: &str = "IBM Plex Sans Arabic";

/// One word, placed and styled for one frame.
#[derive(Debug, Clone)]
pub struct PlacedWord {
    pub text: String,
    pub x: f32,
    /// Top of the word's line box (baseline minus ascent).
    pub y: f32,
    pub size: f32,
    /// Advance width of the word, in pixels (RTL runs need their own box).
    pub width: f32,
    /// True when the word's first strong character is right-to-left.
    pub rtl: bool,
    pub alpha: f32,
    pub active: bool,
}

/// A soft glow rectangle behind the active word (blur stand-in, see README).
#[derive(Debug, Clone, Copy)]
pub struct Glow {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub alpha: f32,
}

/// Everything a backend draws for one frame.
#[derive(Debug, Clone, Default)]
pub struct FrameScene {
    pub words: Vec<PlacedWord>,
    pub glows: Vec<Glow>,
}

/// Word box measured once per line by cosmic-text (BiDi + shaping aware).
#[derive(Debug, Clone)]
struct WordBox {
    text: String,
    x0: f32,
    x1: f32,
    baseline: f32,
}

/// The animated scene: lyric document + view model + cached layout.
pub struct Scene {
    doc: LyricDocument,
    view: LyricView,
    boxes: HashMap<usize, Vec<WordBox>>,
    line_width: HashMap<usize, f32>,
    font_system: cosmic_text::FontSystem,
}

impl Scene {
    /// 40 lines: Arabic, English and mixed, so scrolling and BiDi are both real.
    ///
    /// `fonts` are raw TTF bytes (IBM Plex Sans Arabic + IBM Plex Sans) so the
    /// layout uses exactly the fonts the renderers draw with, not whatever the
    /// host happens to have installed.
    pub fn new(fonts: Vec<Vec<u8>>) -> Self {
        let mut lines = Vec::new();
        for i in 0..40u64 {
            let text = match i % 4 {
                0 => format!("سَلامٌ على الدُّنيا رقم {i}"),
                1 => format!("Hello Aurora, line number {i}"),
                2 => HERO_LINE.to_owned(),
                _ => format!("في الظَّلام نُغَنّي {i} times"),
            };
            lines.push(format!("[00:{:02}.00]{}", (i * 3) % 60, text));
        }
        let mut doc = parse_lrc(&lines.join("\n"), LyricSource::Sidecar);
        // Plain LRC: distribute word timing so the highlight has something to do.
        estimate_word_timing(&mut doc);

        Self {
            doc,
            view: LyricView::new(aurora_ui::Layout {
                viewport_height: HEIGHT as f32,
                line_height: LINE_HEIGHT,
                anchor: 0.42,
                highlight_fade: Millis(80),
                highlight_scale: 0.12,
            }),
            boxes: HashMap::new(),
            line_width: HashMap::new(),
            font_system: cosmic_text::FontSystem::new_with_fonts(
                fonts
                    .into_iter()
                    .map(|f| cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(f))),
            ),
        }
    }

    pub fn document(&self) -> &LyricDocument {
        &self.doc
    }

    /// Builds the frame for `at` seconds into the track.
    pub fn frame(&mut self, position_ms: u64, now: Instant) -> FrameScene {
        let snapshot = ClockSnapshot {
            position: std::time::Duration::from_millis(position_ms),
            sampled_at: now,
            playing: true,
        };
        let plan = self.view.frame(&self.doc, snapshot, now);
        let scroll = plan.scroll_y;

        let mut words = Vec::new();
        let mut glows = Vec::new();
        let anchor = HEIGHT as f32 * 0.42 - LINE_HEIGHT * 0.5;

        for idx in 0..self.doc.lines.len() {
            let y_top = idx as f32 * LINE_HEIGHT - scroll + anchor;
            if y_top < -LINE_HEIGHT || y_top > HEIGHT as f32 {
                continue;
            }
            let is_active_line = plan.active_line == Some(idx);
            let boxes = self.word_boxes(idx);
            let line_w = self.line_width.get(&idx).copied().unwrap_or(0.0);
            let left = MARGIN + (WIDTH as f32 - MARGIN * 2.0 - line_w) / 2.0;

            for (wi, wb) in boxes.iter().enumerate() {
                let plan_word: Option<&WordPlan> =
                    plan.words.iter().find(|w| w.line == idx && w.word == wi);
                let (scale, alpha, active) = match plan_word {
                    Some(p) => (p.scale, p.opacity, p.activation > 0.0),
                    None if is_active_line => (1.0, 0.55, false),
                    None => (1.0, 0.32, false),
                };
                let box_w = wb.x1 - wb.x0;
                let center = left + (wb.x0 + wb.x1) / 2.0;
                let x = center - box_w * scale / 2.0;
                let size = FONT_SIZE * scale;
                let y = y_top + wb.baseline - size * 0.80;

                if active {
                    let pad = 10.0;
                    glows.push(Glow {
                        x: x - pad,
                        y: y + size * 0.10 - pad * 0.5,
                        w: box_w * scale + pad * 2.0,
                        h: size * 1.05 + pad,
                        alpha: 0.22 * alpha,
                    });
                }

                words.push(PlacedWord {
                    text: wb.text.clone(),
                    x,
                    y,
                    size,
                    width: box_w * scale,
                    rtl: is_rtl(&wb.text),
                    alpha: if active { 1.0 } else { alpha },
                    active,
                });
            }
        }

        FrameScene { words, glows }
    }

    fn word_boxes(&mut self, idx: usize) -> Vec<WordBox> {
        if let Some(cached) = self.boxes.get(&idx) {
            return cached.clone();
        }
        let line = self.doc.lines[idx].clone();

        // Build the drawn string ourselves so every word's byte range is exact.
        // `LyricLine::text()` inserts its own separators, and searching for
        // substrings afterwards silently overlapped boxes on repeated words.
        let mut text = String::new();
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(line.words.len());
        for (i, word) in line.words.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            let start = text.len();
            text.push_str(word.text.trim_end());
            ranges.push((start, text.len()));
        }

        let mut buffer = cosmic_text::Buffer::new(
            &mut self.font_system,
            cosmic_text::Metrics::new(FONT_SIZE, LINE_HEIGHT),
        );
        buffer.set_size(Some(WIDTH as f32 - MARGIN * 2.0), Some(LINE_HEIGHT * 2.0));
        let attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Name(FAMILY));
        buffer.set_text(&text, &attrs, cosmic_text::Shaping::Advanced, None);
        // Shaping has to happen before glyph boxes exist; this is also the step
        // that applies Arabic joining and BiDi reordering.
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut boxes: Vec<WordBox> = line
            .words
            .iter()
            .map(|w| WordBox {
                text: w.text.clone(),
                x0: f32::MAX,
                x1: f32::MIN,
                baseline: 0.0,
            })
            .collect();

        let mut width: f32 = 0.0;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                width = width.max(glyph.x + glyph.w);
                if let Some(wi) = ranges
                    .iter()
                    .position(|(s, e)| glyph.start < *e && glyph.end > *s)
                {
                    let b = &mut boxes[wi];
                    b.x0 = b.x0.min(glyph.x);
                    b.x1 = b.x1.max(glyph.x + glyph.w);
                    b.baseline = run.line_y;
                }
            }
        }
        boxes.retain(|b| b.x0.is_finite() && b.x1 > b.x0);
        for b in &mut boxes {
            if b.baseline == 0.0 {
                b.baseline = LINE_HEIGHT * 0.7;
            }
        }
        if std::env::var("SPIKE_DEBUG").is_ok() {
            eprintln!("line {idx} text={text:?} width={width}");
            for b in &boxes {
                eprintln!(
                    "  {:?} x0={:.1} x1={:.1} base={:.1}",
                    b.text, b.x0, b.x1, b.baseline
                );
            }
        }
        self.line_width.insert(idx, width);
        self.boxes.insert(idx, boxes.clone());
        boxes
    }
}

/// First strong character rule (what UAX #9 uses to pick a paragraph level).
fn is_rtl(text: &str) -> bool {
    for ch in text.chars() {
        if matches!(ch as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF) {
            return true;
        }
        if ch.is_alphabetic() {
            return false;
        }
    }
    false
}
