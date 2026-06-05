//! Tauri command handlers.
//!
//! Each command is a thin bridge: validate input, call `geezipx-core`,
//! and return a serialisable result.  No compression logic lives here.

pub mod formats;
