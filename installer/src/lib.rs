//! Install Ferrix: the live system's installer, one page at a time with
//! Back and Next, in the style of Ubuntu's, drawn with tole (Ferrix's
//! `docs/UI-FRAMEWORK.md`) in Settings' faces and Nocturne's colours.
//!
//! The parts:
//!
//! * [`pages`]: what the window shows and what a press does to it, with
//!   no window: the pages in order, each page's rows, the choices.
//! * [`protocol`]: the installer daemon's protocol, the client's side.
//! * [`fake`]: a stand-in daemon on a socket of its own, for the tests and
//!   `--screenshot`.
//! * [`keyboard`], [`zones`] and [`account`]: the layouts offered, the
//!   time zones read, and the account's names checked.
//! * [`ui`]: the markup under `ui/`, compiled by tole, handed the state.
//! * [`window`]: the Wayland window that joins them.
//! * [`cli`]: the command line, with `--autostart` and `--screenshot`.
//!
//! All but the window are host-tested.

pub mod account;
pub mod cli;
pub mod fake;
pub mod keyboard;
pub mod pages;
pub mod protocol;
pub mod ui;
pub mod window;
pub mod zones;

#[cfg(test)]
mod view_tests;
