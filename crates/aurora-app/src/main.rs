//! Aurora Player entry point.
//!
//! Phase 1 turns the Phase 0 demo into a real application shell:
//!
//! ```text
//!  args ──► logging ──► XDG dirs ──► settings (TOML) ──┬─► iced window (Wayland)
//!                                                     └─► headless run (no display)
//! ```
//!
//! `aurora --demo [PATH]` still runs the Phase 0 pipeline, which is the smoke
//! test to use on a new machine.

mod cli;
mod decode;
mod demo;
mod headless;
mod play;
mod shell;
#[cfg(test)]
mod shot;
mod startup;
mod theme;

use std::process::ExitCode;

use aurora_platform::logging;
use tracing::{error, info};

fn main() -> ExitCode {
    match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Parsed::Help) => {
            print!("{}", cli::help());
            ExitCode::SUCCESS
        }
        Ok(cli::Parsed::Version) => {
            println!("aurora {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(cli::Parsed::Run(args)) => run(args),
        Err(message) => {
            eprintln!("aurora: {message}\n\n{}", cli::help());
            ExitCode::from(2)
        }
    }
}

/// `m:ss.mmm`, the way a player shows a position.
fn format_duration(position: std::time::Duration) -> String {
    let total_ms = position.as_millis();
    let minutes = total_ms / 60_000;
    let seconds = (total_ms % 60_000) as f64 / 1_000.0;
    format!("{minutes}:{seconds:06.3}")
}

fn run(args: cli::Args) -> ExitCode {
    logging::init();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        level = %logging::level_from_env(),
        "starting"
    );

    if let Some(path) = args.play.clone() {
        return match play::simulate(&path, None) {
            Ok(report) => {
                println!("{}", path.display());
                println!(
                    "  played {} of {} in {} device callbacks (480 frames each)",
                    format_duration(report.delivered),
                    report
                        .duration
                        .map_or_else(|| "unknown length".to_string(), format_duration),
                    report.callbacks
                );
                println!(
                    "  clock position at end: {}",
                    format_duration(report.final_position)
                );
                ExitCode::SUCCESS
            }
            Err(err) => {
                error!(%err, "simulated playback failed");
                eprintln!("aurora --play: {err}");
                ExitCode::FAILURE
            }
        };
    }

    if let Some(path) = args.decode.clone() {
        return match decode::inspect(&path) {
            Ok(summary) => {
                println!("{}", path.display());
                println!(
                    "  {} Hz, {} channel(s), {}-bit {}",
                    summary.format.sample_rate,
                    summary.format.channels,
                    summary.format.bits_per_sample,
                    if summary.format.is_float {
                        "float"
                    } else {
                        "integer PCM"
                    }
                );
                println!(
                    "  {} frames, {:.3} s",
                    summary.frames,
                    summary.duration.as_secs_f64()
                );
                println!(
                    "  peak {:.4} ({:.1} dBFS), rms {:.4}{}",
                    summary.peak,
                    summary.peak_dbfs(),
                    summary.rms,
                    if summary.is_silent() {
                        " - file is silent"
                    } else {
                        ""
                    }
                );
                ExitCode::SUCCESS
            }
            Err(err) => {
                error!(%err, "decode failed");
                eprintln!("aurora --decode: {err}");
                ExitCode::FAILURE
            }
        };
    }

    if args.demo {
        return match demo::run(args.demo_root.clone()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                error!(%err, "demo failed");
                eprintln!("aurora --demo: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let boot = match startup::boot(&args) {
        Ok(boot) => boot,
        Err(err) => {
            error!(%err, "startup failed");
            eprintln!("aurora: {err}");
            return ExitCode::FAILURE;
        }
    };

    if !boot.session.can_open_window() {
        info!(
            session = boot.session.label(),
            "no window; running headless"
        );
        return match headless::run(&boot) {
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                error!(%err, "headless run failed");
                eprintln!("aurora: {err}");
                ExitCode::FAILURE
            }
        };
    }

    info!(session = boot.session.label(), "opening window");
    // Belt and braces: the socket was verified during boot, but a compositor can
    // die in between and iced reports that by panicking. Catching it here keeps
    // the promise that a missing display is a mode, not a crash.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        shell::run(boot.settings.clone(), boot.settings_path.clone())
    }))
    .unwrap_or_else(|_| {
        Err(iced::Error::WindowCreationFailed(Box::new(
            std::io::Error::other("the UI toolkit panicked while starting"),
        )))
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // A compositor that advertised itself but cannot serve a surface is
            // the common failure (stale `WAYLAND_DISPLAY`). Boot headless rather
            // than dying in front of the user.
            error!(%err, "cannot open a window with this build");
            eprintln!("aurora: unable to open a window ({err}); continuing headless");
            match headless::run(&boot) {
                Ok(_) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("aurora: {err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
