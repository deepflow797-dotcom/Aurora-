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
mod demo;
mod headless;
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

fn run(args: cli::Args) -> ExitCode {
    logging::init();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        level = %logging::level_from_env(),
        "starting"
    );

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
