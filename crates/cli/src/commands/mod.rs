//! CLI subcommand implementations.
//!
//! Each subcommand lives in its own module and receives fully-resolved
//! arguments from the dispatch layer in [`main`](crate::main).

pub mod common;
pub mod compress;
pub mod decompress;
pub mod list;
