//! Linux platform services for Aurora Player.
//!
//! `aurora-core` states what the app needs from its host ([`AppDirectories`]);
//! this crate is the Fedora/Wayland implementation of that contract, plus the
//! three things every desktop binary needs before it can draw a pixel:
//!
//! * [`dirs::XdgDirectories`] - XDG base-directory layout (`~/.config/aurora`,
//!   `~/.local/share/aurora`, `~/.cache/aurora`), resolved from the environment.
//! * [`settings`] - the §27 settings surface, persisted as TOML, written
//!   atomically so a crash mid-write cannot corrupt a user's preferences.
//! * [`logging`] - one `tracing` subscriber, level taken from `AURORA_LOG`.
//! * [`session`] - is there a Wayland compositor? The app must still boot
//!   (headless) when there is not, which is what CI and this sandbox look like.
//!
//! The Android build does **not** use this crate: it supplies its own
//! `AppDirectories` over `Context.getFilesDir()` and does no XDG lookup. Keeping
//! that split is why nothing here leaks into `aurora-core`.
//!
//! [`AppDirectories`]: aurora_core::traits::AppDirectories

pub mod dirs;
pub mod error;
pub mod logging;
pub mod session;
pub mod settings;

pub use dirs::XdgDirectories;
pub use error::PlatformError;
pub use session::{Session, detect_session, wayland_socket_ready};
pub use settings::{Appearance, Settings, Theme};
