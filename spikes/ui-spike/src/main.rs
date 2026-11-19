//! Phase 0 UI spike: `iced` 0.14.1 (tiny-skia) vs `egui` 0.37 (epaint).
//!
//! Both candidates render the same scene (a 40-line Arabic/English lyric sheet
//! with per-word highlighting and spring auto-follow) into the same-sized
//! software framebuffer, head-lessly, at 60 FPS for a configurable duration.
//!
//! Usage:
//! ```text
//! cargo run --release -- --candidate iced --seconds 60 --out out/iced
//! cargo run --release -- --candidate egui --seconds 60 --out out/egui
//! ```
//!
//! Outputs `<out>/frame.png` (hero frame with the mixed line active) and
//! `<out>/report.json` (frame-time percentiles, CPU, RSS).

mod egui_backend;
mod iced_backend;
mod metrics;
mod scene;

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use metrics::FrameTimes;

const ARABIC_FONT: &[u8] = include_bytes!("../../../assets/fonts/IBMPlexSansArabic-Regular.ttf");
const LATIN_FONT: &[u8] = include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf");

struct Args {
    candidate: String,
    seconds: f64,
    out: PathBuf,
    probe: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        candidate: "iced".to_owned(),
        seconds: 5.0,
        out: PathBuf::from("out"),
        probe: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--candidate" => {
                args.candidate = it.next().ok_or("--candidate needs a value")?;
            }
            "--seconds" => {
                args.seconds = it
                    .next()
                    .ok_or("--seconds needs a value")?
                    .parse()
                    .map_err(|e| format!("bad --seconds: {e}"))?;
            }
            "--probe" => {
                args.probe = true;
            }
            "--out" => {
                args.out = PathBuf::from(it.next().ok_or("--out needs a value")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    std::fs::create_dir_all(&args.out)?;

    let mut scene = scene::Scene::new(vec![ARABIC_FONT.to_vec(), LATIN_FONT.to_vec()]);
    println!(
        "candidate: {} | {}x{} | {:.0}s @ 60 FPS | lines: {}",
        args.candidate,
        scene::WIDTH,
        scene::HEIGHT,
        args.seconds,
        scene.document().lines.len()
    );

    // Renderers are boxed behind a tiny enum-free trait so the benchmark loop is
    // identical for both candidates.
    let mut iced = None;
    let mut egui = None;
    match args.candidate.as_str() {
        "iced" => iced = Some(iced_backend::IcedBackend::new(ARABIC_FONT)),
        "egui" => egui = Some(egui_backend::EguiBackend::new(ARABIC_FONT.to_vec())),
        other => return Err(format!("unknown candidate {other}").into()),
    }

    // `--probe`: render the same three lines as *single* text runs and stop.
    // This is the BiDi verdict image: no word boxes, no per-word drawing.
    if args.probe {
        let lines = [
            scene::HERO_LINE,
            "سَلامٌ على الدُّنيا Hello Aurora في الظَّلام",
            "Hello Aurora في الظَّلام is the hard case",
        ];
        let pixels: Vec<u8> = match (&mut iced, &mut egui) {
            (Some(b), _) => {
                b.render_probe(&lines);
                b.pixels().to_vec()
            }
            (_, Some(b)) => {
                b.render_probe(&lines);
                b.pixels().to_vec()
            }
            _ => unreachable!(),
        };
        let path = args.out.join("probe.png");
        write_png(&path, &pixels, scene::WIDTH, scene::HEIGHT)?;
        println!("wrote {} (whole-line text layout probe)", path.display());
        return Ok(());
    }

    let mut times = FrameTimes::default();
    let mut render_times = FrameTimes::default();

    // Hero frame: settle the auto-follow spring on the mixed Arabic/English line
    // before capturing, so the screenshot shows the steady state (active line at
    // the anchor, word glow visible) rather than frame zero.
    // A synthetic 16 ms clock, so the spring actually settles instead of
    // advancing by the microseconds this loop really takes.
    let mut hero_scene = scene::FrameScene::default();
    let hero_base = Instant::now();
    for step in 0..180u64 {
        let at = HERO_MS.saturating_sub((180 - step) * 16);
        hero_scene = scene.frame(at, hero_base + Duration::from_millis(step * 16));
    }
    let hero_pixels: Vec<u8> = match (&mut iced, &mut egui) {
        (Some(b), _) => {
            b.render(&hero_scene);
            b.pixels().to_vec()
        }
        (_, Some(b)) => {
            b.render(&hero_scene);
            b.pixels().to_vec()
        }
        _ => unreachable!(),
    };
    write_png(
        &args.out.join("frame.png"),
        &hero_pixels,
        scene::WIDTH,
        scene::HEIGHT,
    )?;
    let ink = ink_ratio(&hero_pixels);
    println!(
        "hero frame: {} words, ink coverage {:.2}%",
        hero_scene.words.len(),
        ink * 100.0
    );

    // Warm-up: first frames pay font-atlas and glyph-cache costs.
    for i in 0..30u64 {
        let s = scene.frame(i * 16, Instant::now());
        match (&mut iced, &mut egui) {
            (Some(b), _) => b.render(&s),
            (_, Some(b)) => b.render(&s),
            _ => unreachable!(),
        }
    }

    let cpu_before = metrics::cpu_seconds();
    let wall_start = Instant::now();
    let target = Duration::from_secs_f64(1.0 / 60.0);
    let mut rss_peak = 0.0f64;
    let mut frames = 0u64;

    while wall_start.elapsed().as_secs_f64() < args.seconds {
        let frame_start = Instant::now();
        let position_ms = (frames as f64 * 1000.0 / 60.0) as u64;
        let s = scene.frame(position_ms, frame_start);
        match (&mut iced, &mut egui) {
            (Some(b), _) => b.render(&s),
            (_, Some(b)) => b.render(&s),
            _ => unreachable!(),
        }
        let rendered = frame_start.elapsed();
        render_times.record(rendered);

        // Pace to 60 FPS; the sleep is outside the measured render time.
        if rendered < target {
            std::thread::sleep(target - rendered);
        }
        times.record(frame_start.elapsed());
        rss_peak = rss_peak.max(metrics::rss_mib());
        frames += 1;
    }
    let wall = wall_start.elapsed().as_secs_f64();
    let cpu = metrics::cpu_seconds() - cpu_before;

    let report = serde_json_like(&args, &times, &render_times, frames, wall, cpu, rss_peak);
    let mut file = std::fs::File::create(args.out.join("report.json"))?;
    file.write_all(report.as_bytes())?;

    println!(
        "frames: {frames} in {wall:.2}s ({:.1} FPS)",
        frames as f64 / wall
    );
    println!(
        "render ms  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}  mean {:.2}",
        render_times.p50(),
        render_times.p95(),
        render_times.p99(),
        render_times.max(),
        render_times.mean()
    );
    println!(
        "frame ms   p50 {:.2}  p95 {:.2}  p99 {:.2}",
        times.p50(),
        times.p95(),
        times.p99()
    );
    println!(
        "cpu {:.1}% of one core | cpu/{:.0} fps = {:.2} ms/frame | rss peak {:.1} MiB",
        cpu / wall * 100.0,
        frames as f64 / wall,
        cpu * 1000.0 / frames as f64,
        rss_peak
    );
    println!(
        "wrote {}/frame.png and {}/report.json",
        args.out.display(),
        args.out.display()
    );
    Ok(())
}

/// The hero frame: line index 2 (a mixed line) starts at 00:06 in the scene.
const HERO_MS: u64 = 6_700;

fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

/// Fraction of pixels that are not background-dark; a cheap "did it draw" check.
fn ink_ratio(rgba: &[u8]) -> f32 {
    let mut ink = 0usize;
    let mut total = 0usize;
    for px in rgba.as_chunks::<4>().0 {
        total += 1;
        let luma =
            0.2126 * f32::from(px[0]) + 0.7152 * f32::from(px[1]) + 0.0722 * f32::from(px[2]);
        if luma > 60.0 {
            ink += 1;
        }
    }
    ink as f32 / total.max(1) as f32
}

fn serde_json_like(
    args: &Args,
    times: &FrameTimes,
    render: &FrameTimes,
    frames: u64,
    wall: f64,
    cpu: f64,
    rss_peak: f64,
) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"candidate\": \"{}\",\n",
            "  \"width\": {},\n  \"height\": {},\n",
            "  \"target_fps\": 60,\n  \"seconds\": {:.1},\n",
            "  \"frames\": {},\n  \"wall_seconds\": {:.3},\n  \"measured_fps\": {:.2},\n",
            "  \"render_ms\": {{ \"mean\": {:.3}, \"p50\": {:.3}, \"p95\": {:.3}, \"p99\": {:.3}, \"max\": {:.3} }},\n",
            "  \"frame_ms\": {{ \"mean\": {:.3}, \"p50\": {:.3}, \"p95\": {:.3}, \"p99\": {:.3}, \"max\": {:.3} }},\n",
            "  \"cpu_seconds\": {:.3},\n  \"cpu_percent_one_core\": {:.1},\n",
            "  \"cpu_ms_per_frame\": {:.3},\n  \"rss_peak_mib\": {:.1},\n",
            "  \"gpu_percent\": null,\n",
            "  \"gpu_note\": \"no GPU device / no DRM node in this container; software rasterizer only\"\n",
            "}}\n"
        ),
        args.candidate,
        scene::WIDTH,
        scene::HEIGHT,
        args.seconds,
        frames,
        wall,
        frames as f64 / wall,
        render.mean(),
        render.p50(),
        render.p95(),
        render.p99(),
        render.max(),
        times.mean(),
        times.p50(),
        times.p95(),
        times.p99(),
        times.max(),
        cpu,
        cpu / wall * 100.0,
        cpu * 1000.0 / frames as f64,
        rss_peak
    )
}
