//! Ferrix's Settings.
//!
//! A window with a list of pages down its left side and the chosen page
//! beside it. There is one page so far, **game mode**: every application a
//! launcher lists, each with a switch that says whether it is started as a
//! game (Ferrix's `docs/SETTINGS.md`). The switches are a file of the
//! user's, `~/.config/ferrix/game-mode.conf`, which the launcher reads when
//! it starts an application; the file's format, and the list of
//! applications, are the desktop's own `compositor-desktop`, which the
//! launcher reads them with too.
//!
//! The parts:
//!
//! * [`apps`]: the applications, as the page lists them.
//! * [`model`]: what the window shows and what a key or a click does to it,
//!   with no window: the pages, the filter, the selection, the scroll.
//! * [`store`]: turning one switch in the user's file.
//! * [`layout`]: where everything is, and what is under the pointer.
//! * [`paint`]: one frame, into pixels.
//! * [`window`]: the Wayland window that joins them.
//! * [`cli`]: the command line.
//!
//! All but the window are host-tested. It draws with the desktop's own
//! libraries -- `compositor-toolkit`, `compositor-text`, `compositor-image`
//! -- as the launcher, the bar and the lock screen do.

pub mod apps;
pub mod cli;
pub mod layout;
pub mod model;
pub mod paint;
pub mod store;
pub mod theme;
pub mod window;

#[cfg(test)]
mod testdir;
