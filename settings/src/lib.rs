//! Ferrix's Settings, after the customer's design (`Ferrix Settings`):
//! a sidebar of four groups and thirteen pages, a search over them, and
//! the page shown, drawn with tole (Ferrix's `docs/UI-FRAMEWORK.md`).
//!
//! The parts:
//!
//! * [`pages`]: what the window shows and what a press does to it, with
//!   no window: the groups and pages, the search, each page's rows.
//! * [`look`]: the design's colours, dark and light, in seven accents.
//! * [`ui`]: the markup under `ui/`, compiled by tole, handed the state.
//! * [`apps`] and [`store`]: game mode's applications and the user's
//!   game-mode file (Ferrix's `docs/SETTINGS.md`).
//! * [`window`]: the Wayland window that joins them.
//! * [`cli`]: the command line, with `--render` to draw a page headless.
//!
//! All but the window are host-tested.

pub mod apps;
pub mod backend;
pub mod cli;
pub mod desktop;
pub mod look;
pub mod pages;
pub mod store;
pub mod ui;
pub mod wallpaper;
pub mod window;

#[cfg(test)]
mod testdir;
#[cfg(test)]
mod view_tests;
