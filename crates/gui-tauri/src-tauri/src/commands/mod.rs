//! Tauri command handlers.
//!
//! Each command is a thin bridge: validate input, call `geezipx-core`,
//! and return a serialisable result.  No compression logic lives here.

pub mod cancel;
pub mod compress;
pub mod extract;
pub mod formats;
pub mod list;
pub mod test;
