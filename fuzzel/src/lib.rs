//! fuzzel, the application launcher, for Ferrix.
//!
//! A port of Daniel Eklöf's fuzzel (codeberg.org/dnkl/fuzzel, 1.12): it
//! reads `~/.config/fuzzel/fuzzel.ini` as fuzzel does, lists the `.desktop`
//! entries fuzzel would list, ranks what is typed against them by fuzzel's
//! rules, and starts the one chosen. Each module names the part of fuzzel's
//! source it follows.
//!
//! The parts here hold no socket and draw no pixel, so they are host-tested
//! against the behaviour fuzzel's source defines.
//!
//! Finding the `.desktop` entries and their icons is the desktop's own
//! `compositor-desktop`, [`desktop`] and [`icon`] here, since the Settings
//! app lists the same applications; fuzzel's `xdg.c` and `icon.c` are
//! followed there. So is what Ferrix adds to a start: an application the user
//! switched to game mode in Settings is started with the mark
//! ([`exec::Launch::mark`], Ferrix's `docs/SETTINGS.md`).

/// The applications: fuzzel's `xdg.c`, in the desktop's own crate.
pub use compositor_desktop::entry as desktop;
/// The icons: fuzzel's `icon.c`, in the desktop's own crate.
pub use compositor_desktop::icon;

pub mod cache;
pub mod cli;
pub mod config;
pub mod dmenu;
pub mod exec;
pub mod geometry;
pub mod keys;
pub mod keysym;
pub mod launcher;
pub mod matching;
pub mod paint;
pub mod prompt;
pub mod window;

#[cfg(test)]
mod testdir;
