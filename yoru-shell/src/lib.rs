//! Yoru's shell (Ferrix's `docs/DESKTOP.md` §6): the customer's "Yoru WM"
//! design's dock -- the workspaces, the launchers, the tools, a pomodoro,
//! the clock -- as a layer-shell surface at the foot of the screen, drawn
//! with tole in the Nocturne tokens, kept up to date from hyprix's IPC.

pub mod cli;
pub mod hypr;
pub mod json;
pub mod look;
pub mod model;
pub mod ui;
pub mod window;
