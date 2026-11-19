//! Command line for the `aurora` binary.
//!
//! Hand-rolled on purpose: four flags do not justify a dependency, and the
//! parsing rules are part of the contract (a mistyped flag must be an error, not
//! silently ignored - a GUI that starts with the wrong mode and no message is
//! the worst possible outcome).

use std::path::PathBuf;

/// What the binary was asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Args {
    /// Skip the window and run the headless boot path.
    pub headless: bool,
    /// Override the config/data/cache root (tests, portable installs).
    pub config_root: Option<PathBuf>,
    /// Print the Phase 0 demo pipeline instead of starting the shell.
    pub demo: bool,
    /// Folder the demo scans.
    pub demo_root: Option<PathBuf>,
    /// Decode one file, report what is in it, and exit.
    pub decode: Option<PathBuf>,
    /// Simulate playback of one file through a device-free engine, then exit.
    pub play: Option<PathBuf>,
}

/// Result of parsing: either arguments or something to print and exit on.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Args),
    Help,
    Version,
}

const HELP: &str = "\
Aurora Player

USAGE:
    aurora [OPTIONS]

OPTIONS:
    --headless             Start without a window (settings, scan and logs only)
    --config-dir <PATH>    Use <PATH> instead of $XDG_CONFIG_HOME/aurora
    --demo [<PATH>]        Run the Phase 0 pipeline over <PATH> and exit
    --decode <FILE>        Decode <FILE>, print its format and levels
    --play <FILE>          Simulate playback of <FILE> with no sound card
    -h, --help             Show this help
    -V, --version          Show the version

ENVIRONMENT:
    AURORA_LOG             error | warn | info | debug | trace (default: info)
    AURORA_HEADLESS=1      Same as --headless
    WAYLAND_DISPLAY        Selects the Wayland path; without it Aurora stays headless
";

/// Parses arguments (without the program name).
///
/// # Errors
///
/// Returns the offending flag when it is unknown or missing its value.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut parsed = Args::default();
    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--headless" => parsed.headless = true,
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "--demo" => {
                parsed.demo = true;
                // `--demo` takes an optional path; only consume the next token
                // when it is not itself a flag.
                if iter.peek().is_some_and(|next| !next.starts_with('-')) {
                    if let Some(next) = iter.next() {
                        parsed.demo_root = Some(PathBuf::from(next));
                    }
                }
            }
            "--decode" => {
                let value = iter.next().ok_or("--decode needs a file path")?;
                parsed.decode = Some(PathBuf::from(value));
            }
            "--play" => {
                let value = iter.next().ok_or("--play needs a file path")?;
                parsed.play = Some(PathBuf::from(value));
            }
            "--config-dir" => {
                let value = iter.next().ok_or("--config-dir needs a path")?;
                parsed.config_root = Some(PathBuf::from(value));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Parsed::Run(parsed))
}

/// The text `--help` prints.
#[must_use]
pub fn help() -> &'static str {
    HELP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn empty_command_line_runs_the_shell() {
        assert_eq!(parse(args(&[])), Ok(Parsed::Run(Args::default())));
    }

    #[test]
    fn headless_flag_is_recognised() {
        let Parsed::Run(parsed) = parse(args(&["--headless"])).unwrap() else {
            panic!("expected Run");
        };
        assert!(parsed.headless);
    }

    #[test]
    fn demo_takes_an_optional_path() {
        let Parsed::Run(parsed) = parse(args(&["--demo", "/music"])).unwrap() else {
            panic!("expected Run");
        };
        assert!(parsed.demo);
        assert_eq!(parsed.demo_root, Some(PathBuf::from("/music")));

        let Parsed::Run(parsed) = parse(args(&["--demo", "--headless"])).unwrap() else {
            panic!("expected Run");
        };
        assert!(parsed.demo);
        assert!(parsed.headless);
        assert_eq!(parsed.demo_root, None);
    }

    #[test]
    fn decode_takes_a_file() {
        let Parsed::Run(parsed) = parse(args(&["--decode", "a.wav"])).unwrap() else {
            panic!("expected Run");
        };
        assert_eq!(parsed.decode, Some(PathBuf::from("a.wav")));
        assert!(parse(args(&["--decode"])).is_err());
    }

    #[test]
    fn play_takes_a_file() {
        let Parsed::Run(parsed) = parse(args(&["--play", "a.flac"])).unwrap() else {
            panic!("expected Run");
        };
        assert_eq!(parsed.play, Some(PathBuf::from("a.flac")));
        assert!(parse(args(&["--play"])).is_err());
    }

    #[test]
    fn config_dir_requires_a_value() {
        assert!(parse(args(&["--config-dir"])).is_err());
        let Parsed::Run(parsed) = parse(args(&["--config-dir", "/tmp/x"])).unwrap() else {
            panic!("expected Run");
        };
        assert_eq!(parsed.config_root, Some(PathBuf::from("/tmp/x")));
    }

    #[test]
    fn unknown_flags_are_errors() {
        let err = parse(args(&["--window-size", "1"])).unwrap_err();
        assert!(err.contains("--window-size"), "{err}");
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert_eq!(parse(args(&["-h"])), Ok(Parsed::Help));
        assert_eq!(parse(args(&["--version"])), Ok(Parsed::Version));
        assert_eq!(parse(args(&["--headless", "--help"])), Ok(Parsed::Help));
    }
}
