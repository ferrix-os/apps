//! lode-viewer: lode's log in a window.
//!
//! The parts, all but the window host-tested:
//!
//! * [`json`]: a JSONL export (`lode -o json`, `cargo xtask box-log`) read
//!   back into lode's records.
//! * [`store`]: lode's store read as `lode` reads it, and followed.
//! * [`data`]: the records as the viewer holds them.
//! * [`query`]: lode's filter language, the chips and the facets.
//! * [`lines`]: the Logs table's lines, held and folded.
//! * [`stats`]: what the Overview counts.

pub mod app;
pub mod cli;
pub mod crashes;
pub mod data;
pub mod form;
pub mod json;
pub mod lines;
pub mod paged;
pub mod query;
pub mod remote;
pub mod scrollback;
pub mod selection;
pub mod stats;
pub mod store;
pub mod theme;
pub mod tls;
pub mod ui;
pub mod window;

#[cfg(test)]
mod sample_tests;
