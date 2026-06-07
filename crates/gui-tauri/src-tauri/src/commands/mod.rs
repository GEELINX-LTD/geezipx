//! Tauri command handlers.
//!
//! Each command is a thin bridge: validate input, call `geezipx-core`,
//! and return a serialisable result.  No compression logic lives here.

pub mod app;
pub mod cancel;
pub mod compress;
pub mod drag;
pub mod extract;
pub mod extract_entries;
pub mod formats;
pub mod list;
pub mod preview_entry;
pub mod progress;
pub mod test;
