//! GeeZipX core engine — compression and decompression primitives.
//!
//! This crate provides the platform-independent core for archive and
//! compression operations. It is designed to be consumed by both the
//! CLI (`geezipx`) and future Tauri GUI.
//!
//! ## Roadmap
//!
//! - `archive/` — zip, tar, tar.gz container handling
//! - `compress/` — gzip, zstd, xz compression algorithm wrappers
//! - `detect/` — format detection from magic bytes / file extension
//! - `pipeline/` — streaming read/write pipelines
//! - `progress/` — progress event traits for caller consumption
//! - `task/` — compression/decompression task models
//! - `fs/` — cross-platform filesystem helpers
//! - `error/` — unified error types

pub mod archive;
pub mod detect;
pub mod error;
pub mod io;

pub use error::{GeeZipError, GeeZipResult};
pub use io::{Phase, ProgressCallback, ProgressEvent, ProgressReader, ProgressWriter};

/// Return the crate version at compile time.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_version_is_set() {
        assert!(!version().is_empty());
    }
}
