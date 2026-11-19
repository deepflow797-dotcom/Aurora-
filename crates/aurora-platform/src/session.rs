//! Display-session detection.
//!
//! Aurora must *boot* on a machine with no compositor: CI, a TTY, this
//! container. The binary asks [`detect_session`] once and either opens the iced
//! window or runs the headless path, which still loads settings, scans the
//! library and logs - a missing display is a mode, not a crash.
//!
//! Detection is deliberately environment-based. Probing for a socket by name is
//! guesswork (a stale `WAYLAND_DISPLAY` is common after a crashed compositor),
//! and the iced/winit error path is the authoritative check: if opening the
//! window fails, the app falls back to headless and says so.

/// What the app can draw into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// A Wayland compositor is advertised.
    Wayland,
    /// Only an X11 display is advertised. Not supported by this build (the X11
    /// backend needs a `ctor` source that cannot be vendored offline yet).
    X11Only,
    /// No display at all.
    Headless,
}

impl Session {
    /// True when a window can be opened.
    #[must_use]
    pub fn can_open_window(self) -> bool {
        matches!(self, Self::Wayland)
    }

    /// Short human-readable label used in startup logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11Only => "x11",
            Self::Headless => "headless",
        }
    }
}

/// Turns `WAYLAND_DISPLAY` into an absolute socket path.
///
/// Wayland names are either relative to `XDG_RUNTIME_DIR` (`wayland-0`, the
/// normal case) or already absolute; both forms appear in the wild.
fn wayland_socket_path(display: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(display);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| std::path::Path::new(&runtime).join(display))
}

/// True when the advertised Wayland socket actually accepts a connection.
///
/// An advertised `WAYLAND_DISPLAY` is not proof of a compositor: crashed
/// sessions leave stale variables behind, and iced *panics* rather than
/// returning an error when the connection fails. One `connect(2)` here is
/// cheaper than a panic and is the difference between a clean headless boot and
/// a crash on someone's terminal.
#[must_use]
pub fn wayland_socket_ready() -> bool {
    let Some(display) = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|d| !d.is_empty())
    else {
        return false;
    };
    wayland_socket_ready_at(&display)
}

/// Testable core of [`wayland_socket_ready`]: accepts the same relative or
/// absolute display names.
#[must_use]
pub fn wayland_socket_ready_at(display: &str) -> bool {
    let Some(path) = wayland_socket_path(display) else {
        return false;
    };
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

/// Classifies the current environment.
///
/// `AURORA_HEADLESS=1` forces [`Session::Headless`], which is how the test
/// harness and the `--headless` flag pin the mode regardless of `$DISPLAY`.
#[must_use]
pub fn detect_session() -> Session {
    detect_from(std::env::var("AURORA_HEADLESS").ok().as_deref(), |key| {
        std::env::var(key).ok()
    })
}

/// Testable core of [`detect_session`].
///
/// `lookup` is injected so the rules can be tested without mutating the process
/// environment (which is racy under `cargo test`).
#[must_use]
pub fn detect_from<F>(headless_override: Option<&str>, lookup: F) -> Session
where
    F: Fn(&str) -> Option<String>,
{
    if matches!(headless_override, Some("1" | "true" | "yes")) {
        return Session::Headless;
    }
    let present = |key: &str| lookup(key).is_some_and(|value| !value.is_empty());
    if present("WAYLAND_DISPLAY") {
        Session::Wayland
    } else if present("DISPLAY") {
        Session::X11Only
    } else {
        Session::Headless
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listening_socket_counts_as_ready() {
        let dir = std::env::temp_dir().join(format!("aurora-wayland-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let socket = dir.join("wayland-test");
        std::fs::remove_file(&socket).ok();
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind");
        // Absolute form.
        assert!(wayland_socket_ready_at(&socket.to_string_lossy()));
        std::fs::remove_file(&socket).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_socket_is_not_ready() {
        assert!(!wayland_socket_ready_at("/nonexistent/wayland-0"));
    }

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn wayland_is_preferred_over_x11() {
        let session = detect_from(
            None,
            env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]),
        );
        assert_eq!(session, Session::Wayland);
        assert!(session.can_open_window());
    }

    #[test]
    fn x11_alone_is_reported_but_unsupported() {
        let session = detect_from(None, env(&[("DISPLAY", ":0")]));
        assert_eq!(session, Session::X11Only);
        assert!(!session.can_open_window());
    }

    #[test]
    fn no_variables_means_headless() {
        assert_eq!(detect_from(None, env(&[])), Session::Headless);
    }

    #[test]
    fn empty_values_do_not_count() {
        let session = detect_from(None, env(&[("WAYLAND_DISPLAY", "")]));
        assert_eq!(session, Session::Headless);
    }

    #[test]
    fn override_forces_headless() {
        let session = detect_from(Some("1"), env(&[("WAYLAND_DISPLAY", "wayland-0")]));
        assert_eq!(session, Session::Headless);
    }
}
