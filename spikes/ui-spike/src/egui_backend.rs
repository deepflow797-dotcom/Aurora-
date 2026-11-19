//! Candidate B: `egui` 0.37, tessellated by epaint and rasterized here.
//!
//! egui is immediate mode and does not rasterize anything itself - it produces
//! `Mesh`es with a font atlas. To keep the comparison honest (same "draw N
//! styled words" workload, same software rasterizer family) this backend
//! tessellates with epaint and then rasterizes the clipped meshes with a small
//! barycentric triangle rasterizer. No GPU, no window.

use std::sync::Arc;

use egui::epaint::{ClippedPrimitive, Primitive, Vertex};
use egui::{Color32, FontFamily, FontId, Rect};

use crate::scene::{FrameScene, HEIGHT, WIDTH};

pub struct EguiBackend {
    ctx: egui::Context,
    pixmap: tiny_skia::Pixmap,
    atlas: Vec<[u8; 4]>,
    atlas_size: [usize; 2],
}

impl EguiBackend {
    pub fn new(font_bytes: Vec<u8>) -> Self {
        let ctx = egui::Context::default();

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "plex-arabic".to_owned(),
            Arc::new(egui::FontData::from_owned(font_bytes)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "plex-arabic".to_owned());
        ctx.set_fonts(fonts);

        let pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT).expect("pixmap");
        Self {
            ctx,
            pixmap,
            atlas: Vec::new(),
            atlas_size: [0, 0],
        }
    }

    pub fn render(&mut self, scene: &FrameScene) {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };

        let output = self.ctx.run_ui(input, |ui| {
            let painter = ui.painter();
            for glow in &scene.glows {
                let r = Rect::from_min_size(egui::pos2(glow.x, glow.y), egui::vec2(glow.w, glow.h));
                painter.rect_filled(
                    r,
                    10.0,
                    Color32::from_rgba_unmultiplied(255, 190, 70, (glow.alpha * 255.0) as u8),
                );
            }
            for word in &scene.words {
                let color = if word.active {
                    Color32::from_rgba_unmultiplied(255, 220, 115, (word.alpha * 255.0) as u8)
                } else {
                    Color32::from_rgba_unmultiplied(160, 168, 185, (word.alpha * 255.0) as u8)
                };
                let anchor = if word.rtl {
                    egui::pos2(word.x + word.width, word.y)
                } else {
                    egui::pos2(word.x, word.y)
                };
                painter.text(
                    anchor,
                    if word.rtl {
                        egui::Align2::RIGHT_TOP
                    } else {
                        egui::Align2::LEFT_TOP
                    },
                    word.text.clone(),
                    FontId::new(word.size, FontFamily::Proportional),
                    color,
                );
            }
        });

        // Fresh atlas each frame (egui resizes it as glyphs are added).
        let (atlas, size) = self.ctx.fonts(|f| {
            let image = f.image();
            let size = image.size;
            let mut pixels = Vec::with_capacity(size[0] * size[1]);
            for p in image.pixels.iter() {
                pixels.push(p.to_array());
            }
            (pixels, size)
        });
        self.atlas = atlas;
        self.atlas_size = size;

        self.pixmap.fill(tiny_skia::Color::BLACK);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        for ClippedPrimitive {
            clip_rect,
            primitive,
        } in primitives
        {
            if let Primitive::Mesh(mesh) = primitive {
                self.rasterize_mesh(&mesh.vertices, &mesh.indices, clip_rect);
            }
        }
    }

    /// Draws whole lines as single text runs, with no pre-computed word boxes.
    /// This isolates each engine's own BiDi handling from our layout code.
    pub fn render_probe(&mut self, lines: &[&str]) {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(input, |ui| {
            let painter = ui.painter();
            for (row, line) in lines.iter().enumerate() {
                painter.text(
                    egui::pos2(80.0, 120.0 + row as f32 * 130.0),
                    egui::Align2::LEFT_TOP,
                    (*line).to_owned(),
                    FontId::new(34.0, FontFamily::Proportional),
                    Color32::from_rgb(255, 220, 115),
                );
            }
        });
        let (atlas, size) = self.ctx.fonts(|f| {
            let image = f.image();
            let size = image.size;
            let mut pixels = Vec::with_capacity(size[0] * size[1]);
            for p in image.pixels.iter() {
                pixels.push(p.to_array());
            }
            (pixels, size)
        });
        self.atlas = atlas;
        self.atlas_size = size;
        self.pixmap.fill(tiny_skia::Color::BLACK);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        for ClippedPrimitive {
            clip_rect,
            primitive,
        } in primitives
        {
            if let Primitive::Mesh(mesh) = primitive {
                self.rasterize_mesh(&mesh.vertices, &mesh.indices, clip_rect);
            }
        }
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Barycentric triangle rasterizer with texture + vertex-colour modulation.
    fn rasterize_mesh(&mut self, vertices: &[Vertex], indices: &[u32], clip: Rect) {
        let width = WIDTH as usize;
        let height = HEIGHT as usize;
        let clip_x0 = clip.min.x.max(0.0) as usize;
        let clip_x1 = (clip.max.x as usize).min(width);
        let clip_y0 = clip.min.y.max(0.0) as usize;
        let clip_y1 = (clip.max.y as usize).min(height);

        for tri in indices.as_chunks::<3>().0 {
            let (a, b, c) = (
                &vertices[tri[0] as usize],
                &vertices[tri[1] as usize],
                &vertices[tri[2] as usize],
            );
            let min_x = a
                .pos
                .x
                .min(b.pos.x)
                .min(c.pos.x)
                .floor()
                .max(clip_x0 as f32);
            let max_x = a.pos.x.max(b.pos.x).max(c.pos.x).ceil().min(clip_x1 as f32);
            let min_y = a
                .pos
                .y
                .min(b.pos.y)
                .min(c.pos.y)
                .floor()
                .max(clip_y0 as f32);
            let max_y = a.pos.y.max(b.pos.y).max(c.pos.y).ceil().min(clip_y1 as f32);
            if max_x <= min_x || max_y <= min_y {
                continue;
            }

            let area = edge(a.pos, b.pos, c.pos);
            if area.abs() < 1e-6 {
                continue;
            }

            for y in min_y as usize..max_y as usize {
                for x in min_x as usize..max_x as usize {
                    let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = edge(b.pos, c.pos, p) / area;
                    let w1 = edge(c.pos, a.pos, p) / area;
                    let w2 = edge(a.pos, b.pos, p) / area;
                    if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                        continue;
                    }

                    let color = [
                        blend_channel(a.color, b.color, c.color, w0, w1, w2, 0),
                        blend_channel(a.color, b.color, c.color, w0, w1, w2, 1),
                        blend_channel(a.color, b.color, c.color, w0, w1, w2, 2),
                        blend_channel(a.color, b.color, c.color, w0, w1, w2, 3),
                    ];
                    let uv = (
                        a.uv.x * w0 + b.uv.x * w1 + c.uv.x * w2,
                        a.uv.y * w0 + b.uv.y * w1 + c.uv.y * w2,
                    );
                    let texel = self.sample(uv);
                    let src = [
                        (color[0] as f32 * texel[0] as f32 / 255.0) as u32,
                        (color[1] as f32 * texel[1] as f32 / 255.0) as u32,
                        (color[2] as f32 * texel[2] as f32 / 255.0) as u32,
                        (color[3] as f32 * texel[3] as f32 / 255.0) as u32,
                    ];
                    self.blend(x, y, src);
                }
            }
        }
    }

    fn sample(&self, uv: (f32, f32)) -> [u8; 4] {
        if self.atlas_size[0] == 0 {
            return [255, 255, 255, 255];
        }
        let x = (uv.0 * self.atlas_size[0] as f32) as isize;
        let y = (uv.1 * self.atlas_size[1] as f32) as isize;
        let x = x.clamp(0, self.atlas_size[0] as isize - 1) as usize;
        let y = y.clamp(0, self.atlas_size[1] as isize - 1) as usize;
        self.atlas[y * self.atlas_size[0] + x]
    }

    /// Source-over blend of premultiplied sRGB bytes.
    fn blend(&mut self, x: usize, y: usize, src: [u32; 4]) {
        let idx = (y * WIDTH as usize + x) * 4;
        let data = self.pixmap.data_mut();
        let inv = 255 - src[3];
        for c in 0..4 {
            let dst = data[idx + c] as u32;
            data[idx + c] = (src[c] + dst * inv / 255).min(255) as u8;
        }
    }
}

fn edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn blend_channel(a: Color32, b: Color32, c: Color32, w0: f32, w1: f32, w2: f32, i: usize) -> u8 {
    let value =
        a.to_array()[i] as f32 * w0 + b.to_array()[i] as f32 * w1 + c.to_array()[i] as f32 * w2;
    value.clamp(0.0, 255.0) as u8
}

#[allow(dead_code)]
fn _unused(_: &[u8]) {}
