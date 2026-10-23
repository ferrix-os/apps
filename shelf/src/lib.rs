//! shelf, Ferrix's file manager, after the customer's "File Explorer v2"
//! design: tabs, one pane or two, list or grid, folder sizes, archives
//! read in place, tags, Quick access, filter chips, a search in three
//! modes, bulk rename, and a trash that follows the freedesktop.org
//! specification. Its window is drawn by tole from the markup under
//! `ui/`; this crate is its state ([`app`]), what it does to files
//! ([`fsops`], [`trash`], [`archive`]), what it keeps ([`store`]) and the
//! window ([`window`]).

pub mod app;
pub mod archive;
pub mod cli;
pub mod demo;
pub mod fsops;
pub mod sizes;
pub mod store;
pub mod trash;
pub mod view;
pub mod window;
