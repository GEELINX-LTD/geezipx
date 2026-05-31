//! GeeZipX CLI — high-performance compression/decompression tool.
//!
//! ## Roadmap
//!
//! - `commands/` — compress, extract, list, test subcommands
//! - `output/` — human and machine-readable output formatting
//! - `progress.rs` — progress bar rendering via indicatif
//!
//! This binary is a thin shell over `geezipx-core`. All archive and
//! compression logic lives in the core crate.

fn main() {
    println!("GeeZipX {}", geezipx_core::version());
    println!("CLI commands coming in Phase 1 — see docs/PHASE1_CLI_TASKS.md");
}
